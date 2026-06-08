//! 多级 Undo/Redo 的核心:一条「操作时间线 + 光标」。
//!
//! 把 HEAD 最近经历的位置想成一条线,每个点是一次写操作后 HEAD 的落点。`cursor`
//! 指向「当前 HEAD 所在的点」:它左边是可撤销的历史,右边是可重做的分支。
//! - **撤销** = 光标后退一格(reset --soft 到左邻点)。
//! - **重做** = 光标前进一格(reset --soft 到右邻点)。
//! - **新操作** = 砍掉光标右侧的重做分支,再追加新点(编辑器标准行为)。
//!
//! 关键:撤销/重做**只移动光标、不追加新点**,所以不会出现「撤销的撤销」这种乒乓,
//! 标签也永远诚实(撤销显示被撤的操作名,重做显示被重做的操作名)。
//!
//! 这是**纯逻辑**:只认 oid 字符串与中文操作名,不碰任何 git 实现 —— 所有刁钻的
//! 截断/对齐/重建分支在这里被穷尽单测,`RepoContext` 那层只是把它接到后端 reset 上。

/// 时间线上的一个点:某次操作后 HEAD 的落点。
#[derive(Debug, Clone, PartialEq, Eq)]
struct NavPoint {
    /// 此点 HEAD 指向的完整 oid。
    oid: String,
    /// **导致**到达此点的操作中文名(撤销此点时显示);基点用 "起点"。
    label: String,
}

/// 一步撤销/重做:目标 oid + 文案。`RepoContext` 据此 reset 并生成 DTO。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavStep {
    pub label: String,
    pub target_oid: String,
}

/// 操作时间线 + 光标。挂在 `RepoContext` 内(`Mutex` 保护),随上下文长驻。
#[derive(Debug, Default)]
pub struct UndoNav {
    timeline: Vec<NavPoint>,
    cursor: usize,
}

impl UndoNav {
    /// 当前光标点是否正好对齐真实 HEAD。不对齐 = 初次使用或被外部(终端 git / 切分支)
    /// 改动过,需要 [`UndoNav::sync`] 重建。
    pub fn is_aligned(&self, head: &str) -> bool {
        self.timeline.get(self.cursor).map(|p| p.oid.as_str()) == Some(head)
    }

    /// 让时间线对齐真实 HEAD;对不上就用 reflog 提示重建一个**最小**时间线。
    ///
    /// `boot`:当 reflog 顶项可安全撤销时,给 `(上一步 oid, 该操作中文名)`;否则 `None`。
    /// - 有 boot → 建 `[起点(上一步), 当前(可撤销操作)]`,光标在末,提供一次撤销。
    /// - 无 boot → 建 `[当前]`,光标在 0,无撤销可用。
    pub fn sync(&mut self, head: &str, boot: Option<(String, String)>) {
        if self.is_aligned(head) {
            return;
        }
        match boot {
            Some((prev, label)) => {
                self.timeline = vec![
                    NavPoint {
                        oid: prev,
                        label: "起点".into(),
                    },
                    NavPoint {
                        oid: head.into(),
                        label,
                    },
                ];
                self.cursor = 1;
            }
            None => {
                self.timeline = vec![NavPoint {
                    oid: head.into(),
                    label: "起点".into(),
                }];
                self.cursor = 0;
            }
        }
    }

    /// 记录一次「我们刚做的」HEAD 移动操作。
    /// `before` = 操作前 HEAD(`RepoContext` 在调后端前捕获,精确;空仓库为 None);
    /// `after` = 操作后 HEAD;`label` = 操作中文名。
    pub fn record(&mut self, before: Option<&str>, after: &str, label: &str) {
        // HEAD 没动 → 不是一次历史移动(如 up-to-date 的 pull),不记。
        if before == Some(after) {
            return;
        }
        // 与时间线光标点一致 → 正常推进:砍掉重做分支,追加新点。
        let consistent = matches!(
            (before, self.timeline.get(self.cursor)),
            (Some(b), Some(cur)) if cur.oid == b
        );
        if consistent {
            self.timeline.truncate(self.cursor + 1);
            self.timeline.push(NavPoint {
                oid: after.into(),
                label: label.into(),
            });
        } else {
            // 对不上(初次 / 外部改动)→ 以 before 为基重建,丢弃旧分支。
            self.timeline = match before {
                Some(b) => vec![
                    NavPoint {
                        oid: b.into(),
                        label: "起点".into(),
                    },
                    NavPoint {
                        oid: after.into(),
                        label: label.into(),
                    },
                ],
                None => vec![NavPoint {
                    oid: after.into(),
                    label: label.into(),
                }],
            };
        }
        self.cursor = self.timeline.len() - 1;
    }

    /// 下一步撤销(光标左邻),无则 None。
    pub fn can_undo(&self) -> Option<NavStep> {
        if self.cursor == 0 {
            return None;
        }
        Some(NavStep {
            label: self.timeline[self.cursor].label.clone(),
            target_oid: self.timeline[self.cursor - 1].oid.clone(),
        })
    }

    /// 下一步重做(光标右邻),无则 None。
    pub fn can_redo(&self) -> Option<NavStep> {
        if self.cursor + 1 >= self.timeline.len() {
            return None;
        }
        Some(NavStep {
            label: self.timeline[self.cursor + 1].label.clone(),
            target_oid: self.timeline[self.cursor + 1].oid.clone(),
        })
    }

    /// reset 成功后提交光标后退。
    pub fn commit_undo(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// reset 成功后提交光标前进。
    pub fn commit_redo(&mut self) {
        if self.cursor + 1 < self.timeline.len() {
            self.cursor += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(label: &str, oid: &str) -> NavStep {
        NavStep {
            label: label.into(),
            target_oid: oid.into(),
        }
    }

    #[test]
    fn record_builds_linear_history_and_enables_undo() {
        let mut nav = UndoNav::default();
        nav.record(None, "A", "提交"); // 首提交(空仓库,无 before)
        assert_eq!(nav.can_undo(), None, "首提交无父,撤不了");
        nav.record(Some("A"), "B", "提交");
        assert_eq!(nav.can_undo(), Some(step("提交", "A")));
        assert_eq!(nav.can_redo(), None);
    }

    #[test]
    fn multi_level_undo_then_redo_walks_the_line() {
        let mut nav = UndoNav::default();
        nav.record(Some("base"), "A", "提交");
        nav.record(Some("A"), "B", "cherry-pick");
        nav.record(Some("B"), "C", "重置(reset)");
        // 连撤三步:C→B→A→base
        assert_eq!(nav.can_undo(), Some(step("重置(reset)", "B")));
        nav.commit_undo();
        assert_eq!(nav.can_undo(), Some(step("cherry-pick", "A")));
        nav.commit_undo();
        assert_eq!(nav.can_undo(), Some(step("提交", "base")));
        nav.commit_undo();
        assert_eq!(nav.can_undo(), None, "已到底");
        // 连重做:base→A→B→C,文案与撤销镜像
        assert_eq!(nav.can_redo(), Some(step("提交", "A")));
        nav.commit_redo();
        assert_eq!(nav.can_redo(), Some(step("cherry-pick", "B")));
        nav.commit_redo();
        assert_eq!(nav.can_redo(), Some(step("重置(reset)", "C")));
        nav.commit_redo();
        assert_eq!(nav.can_redo(), None, "已到最前");
    }

    #[test]
    fn new_op_after_undo_truncates_redo_branch() {
        let mut nav = UndoNav::default();
        nav.record(Some("base"), "A", "提交");
        nav.record(Some("A"), "B", "提交");
        nav.commit_undo(); // 回到 A,B 在重做分支上
        assert_eq!(nav.can_redo(), Some(step("提交", "B")));
        // 在 A 上做了新提交 C → B 的重做分支被砍掉
        nav.record(Some("A"), "C", "提交");
        assert_eq!(nav.can_redo(), None, "新操作清空重做分支");
        assert_eq!(nav.can_undo(), Some(step("提交", "A")));
    }

    #[test]
    fn record_skips_when_head_did_not_move() {
        let mut nav = UndoNav::default();
        nav.record(Some("A"), "B", "提交");
        let before = (nav.cursor, nav.timeline.len());
        nav.record(Some("B"), "B", "拉取(pull)"); // up-to-date pull:HEAD 没动
        assert_eq!((nav.cursor, nav.timeline.len()), before, "无移动不该记一笔");
    }

    #[test]
    fn record_rebuilds_when_external_change_breaks_continuity() {
        let mut nav = UndoNav::default();
        nav.record(Some("base"), "A", "提交");
        // 用户在终端切到别的分支(我们没记),现在从 X 提交到 Y:before=X 与光标点 A 对不上
        nav.record(Some("X"), "Y", "提交");
        // 旧时间线作废,以 X 为基重建,只保留这次的可撤销
        assert_eq!(nav.can_undo(), Some(step("提交", "X")));
        assert_eq!(nav.can_redo(), None);
    }

    #[test]
    fn sync_bootstraps_single_undo_from_reflog_hint() {
        let mut nav = UndoNav::default();
        // 冷启动:时间线空,真实 HEAD=H,reflog 顶项是可撤销的「提交」,上一步=P
        nav.sync("H", Some(("P".into(), "提交".into())));
        assert_eq!(nav.can_undo(), Some(step("提交", "P")));
        assert_eq!(nav.can_redo(), None, "reflog 无法得知前进历史");
        assert!(nav.is_aligned("H"));
    }

    #[test]
    fn sync_without_hint_has_no_undo() {
        let mut nav = UndoNav::default();
        nav.sync("H", None); // 顶项不可撤销(如 checkout)或无上一步
        assert_eq!(nav.can_undo(), None);
        assert!(nav.is_aligned("H"));
    }

    #[test]
    fn sync_noop_when_already_aligned() {
        let mut nav = UndoNav::default();
        nav.record(Some("A"), "B", "提交");
        nav.commit_undo(); // 在 A,保留 B 的重做分支
        nav.sync("A", Some(("zzz".into(), "重置(reset)".into()))); // 已对齐 → 应忽略 boot
        assert_eq!(
            nav.can_redo(),
            Some(step("提交", "B")),
            "对齐时不该被 boot 重建"
        );
    }
}
