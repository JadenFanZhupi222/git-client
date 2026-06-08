//! 多级 Undo/Redo + 操作日志的核心:一条「操作时间线 + 光标」。
//!
//! 把 HEAD 最近经历的位置想成一条线,每个点是一次写操作后 HEAD 的落点。`cursor`
//! 指向「当前 HEAD 所在的点」:它左边是可撤销的历史,右边是可重做的分支。
//! - **撤销** = 光标后退一格(reset --soft 到左邻点)。
//! - **重做** = 光标前进一格(reset --soft 到右邻点)。
//! - **跳转(操作日志点击)** = 光标移到任意点(reset --soft 过去)——撤销/重做的泛化。
//! - **新操作** = 砍掉光标右侧的重做分支,再追加新点(编辑器标准行为)。
//!
//! 关键:撤销/重做/跳转**只移动光标、不追加新点**,所以不会出现「撤销的撤销」这种乒乓,
//! 标签也永远诚实。整条时间线 + 时间戳就是「操作日志面板」的数据。
//!
//! 这是**纯逻辑**:只认 oid 字符串、中文操作名、还原语义、时间戳(由 RepoContext 注入,
//! 本层不读钟),不碰任何 git —— 所有刁钻的截断/对齐/重建分支在这里被穷尽单测。

use git_core::UndoKind;

/// 时间线上的一个点:某次操作后 HEAD 的落点。
#[derive(Debug, Clone, PartialEq, Eq)]
struct NavPoint {
    /// 此点 HEAD 指向的完整 oid。
    oid: String,
    /// **导致**到达此点的操作中文名(撤销此点时显示);基点用 "起点"。
    label: String,
    /// **导致**到达此点的操作的还原语义:撤销该操作时 RepoContext 据此决定 soft / hard。
    /// 基点(起点)的 kind 无意义(它不是被撤销的对象),约定填 `Restore`。
    kind: UndoKind,
    /// 记录此点时的 Unix 时间戳(秒),供操作日志显示「几分钟前」。
    timestamp: i64,
}

/// 一步撤销/重做/跳转:目标 oid + 文案 + 还原语义。`RepoContext` 据此选 reset 模式并生成 DTO。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavStep {
    pub label: String,
    pub target_oid: String,
    /// 被撤销/重做的那个操作的还原语义。`Uncommit`→soft,`Restore`→hard。
    pub kind: UndoKind,
}

/// 操作日志的一项(给面板渲染):操作名 + 落点 oid + 时间戳。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpLogItem {
    pub label: String,
    pub oid: String,
    pub timestamp: i64,
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
    /// `boot`:当 reflog 顶项可安全撤销时,给 `(上一步 oid, 该操作中文名, 还原语义)`;否则 `None`。
    /// `now`:重建出来的点的时间戳(冷启动时操作真实时间未知,用当前时间近似)。
    pub fn sync(&mut self, head: &str, boot: Option<(String, String, UndoKind)>, now: i64) {
        if self.is_aligned(head) {
            return;
        }
        match boot {
            Some((prev, label, kind)) => {
                self.timeline = vec![
                    NavPoint {
                        oid: prev,
                        label: "起点".into(),
                        kind: UndoKind::Restore,
                        timestamp: now,
                    },
                    NavPoint {
                        oid: head.into(),
                        label,
                        kind,
                        timestamp: now,
                    },
                ];
                self.cursor = 1;
            }
            None => {
                self.timeline = vec![NavPoint {
                    oid: head.into(),
                    label: "起点".into(),
                    kind: UndoKind::Restore,
                    timestamp: now,
                }];
                self.cursor = 0;
            }
        }
    }

    /// 记录一次「我们刚做的」HEAD 移动操作。
    /// `before` = 操作前 HEAD(`RepoContext` 在调后端前捕获,精确;空仓库为 None);
    /// `after` = 操作后 HEAD;`label` = 操作中文名;`kind` = 还原语义;`now` = 记录时间戳。
    pub fn record(
        &mut self,
        before: Option<&str>,
        after: &str,
        label: &str,
        kind: UndoKind,
        now: i64,
    ) {
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
                kind,
                timestamp: now,
            });
        } else {
            // 对不上(初次 / 外部改动)→ 以 before 为基重建,丢弃旧分支。
            self.timeline = match before {
                Some(b) => vec![
                    NavPoint {
                        oid: b.into(),
                        label: "起点".into(),
                        kind: UndoKind::Restore,
                        timestamp: now,
                    },
                    NavPoint {
                        oid: after.into(),
                        label: label.into(),
                        kind,
                        timestamp: now,
                    },
                ],
                None => vec![NavPoint {
                    oid: after.into(),
                    label: label.into(),
                    kind,
                    timestamp: now,
                }],
            };
        }
        self.cursor = self.timeline.len() - 1;
    }

    /// 下一步撤销(光标左邻),无则 None。
    pub fn can_undo(&self) -> Option<NavStep> {
        self.point_at(self.cursor.checked_sub(1)?)
    }

    /// 下一步重做(光标右邻),无则 None。
    pub fn can_redo(&self) -> Option<NavStep> {
        self.point_at(self.cursor + 1)
    }

    /// 跳到任意时间线点的目标(供操作日志点击)。越界或就是当前点 → None。
    pub fn point_at(&self, index: usize) -> Option<NavStep> {
        if index == self.cursor || index >= self.timeline.len() {
            return None;
        }
        // 文案与还原语义同源:撤销方向(左移)取「被撤销点」(当前 cursor)的操作;
        // 重做方向(右移)取「目标点」(index)的操作。kind 跟 label 同一个点 —— 这样
        // 「撤销 reset」用 reset 的 hard 语义、「撤销提交」用提交的 soft 语义。
        let op_idx = if index < self.cursor {
            self.cursor
        } else {
            index
        };
        Some(NavStep {
            label: self.timeline[op_idx].label.clone(),
            target_oid: self.timeline[index].oid.clone(),
            kind: self.timeline[op_idx].kind,
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

    /// reset 成功后提交光标跳到任意点(越界忽略)。
    pub fn commit_goto(&mut self, index: usize) {
        if index < self.timeline.len() {
            self.cursor = index;
        }
    }

    /// 当前光标位置(操作日志高亮用)。
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// 整条时间线快照(操作日志面板渲染用),oldest→newest。
    pub fn items(&self) -> Vec<OpLogItem> {
        self.timeline
            .iter()
            .map(|p| OpLogItem {
                label: p.label.clone(),
                oid: p.oid.clone(),
                timestamp: p.timestamp,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use UndoKind::{Restore, Uncommit};

    fn step(label: &str, oid: &str, kind: UndoKind) -> NavStep {
        NavStep {
            label: label.into(),
            target_oid: oid.into(),
            kind,
        }
    }

    #[test]
    fn record_builds_linear_history_and_enables_undo() {
        let mut nav = UndoNav::default();
        nav.record(None, "A", "提交", Uncommit, 0); // 首提交(空仓库,无 before)
        assert_eq!(nav.can_undo(), None, "首提交无父,撤不了");
        nav.record(Some("A"), "B", "提交", Uncommit, 0);
        assert_eq!(nav.can_undo(), Some(step("提交", "A", Uncommit)));
        assert_eq!(nav.can_redo(), None);
    }

    #[test]
    fn multi_level_undo_then_redo_walks_the_line() {
        let mut nav = UndoNav::default();
        nav.record(Some("base"), "A", "提交", Uncommit, 0);
        nav.record(Some("A"), "B", "cherry-pick", Restore, 0);
        nav.record(Some("B"), "C", "重置(reset)", Restore, 0);
        // 连撤三步:C→B→A→base。每步 kind 跟着被撤的操作走(reset/cherry-pick=Restore,提交=Uncommit)。
        assert_eq!(nav.can_undo(), Some(step("重置(reset)", "B", Restore)));
        nav.commit_undo();
        assert_eq!(nav.can_undo(), Some(step("cherry-pick", "A", Restore)));
        nav.commit_undo();
        assert_eq!(nav.can_undo(), Some(step("提交", "base", Uncommit)));
        nav.commit_undo();
        assert_eq!(nav.can_undo(), None, "已到底");
        // 连重做:base→A→B→C,文案与 kind 都与撤销镜像
        assert_eq!(nav.can_redo(), Some(step("提交", "A", Uncommit)));
        nav.commit_redo();
        assert_eq!(nav.can_redo(), Some(step("cherry-pick", "B", Restore)));
        nav.commit_redo();
        assert_eq!(nav.can_redo(), Some(step("重置(reset)", "C", Restore)));
        nav.commit_redo();
        assert_eq!(nav.can_redo(), None, "已到最前");
    }

    #[test]
    fn goto_any_point_jumps_with_correct_label() {
        let mut nav = UndoNav::default();
        nav.record(Some("base"), "A", "提交", Uncommit, 0);
        nav.record(Some("A"), "B", "cherry-pick", Restore, 0);
        nav.record(Some("B"), "C", "重置(reset)", Restore, 0); // cursor=3 (timeline: base,A,B,C)
        // 往回跳两格到 A(index 1):撤销方向 → 显示被撤销点(当前 C)的操作名 + kind。
        assert_eq!(nav.point_at(1), Some(step("重置(reset)", "A", Restore)));
        nav.commit_goto(1);
        // 现在在 A(index1)。往前跳到 C(index3):重做方向 → 显示目标点 C 的操作名 + kind。
        assert_eq!(nav.point_at(3), Some(step("重置(reset)", "C", Restore)));
        // 跳到当前点 / 越界 → None。
        assert_eq!(nav.point_at(1), None);
        assert_eq!(nav.point_at(9), None);
    }

    #[test]
    fn items_snapshot_carries_labels_and_cursor() {
        let mut nav = UndoNav::default();
        nav.record(Some("base"), "A", "提交", Uncommit, 100);
        nav.record(Some("A"), "B", "cherry-pick", Restore, 200);
        let items = nav.items();
        assert_eq!(items.len(), 3); // 起点 + A + B
        assert_eq!(items[0].label, "起点");
        assert_eq!(items[2].label, "cherry-pick");
        assert_eq!(items[2].timestamp, 200);
        assert_eq!(nav.cursor(), 2);
    }

    #[test]
    fn new_op_after_undo_truncates_redo_branch() {
        let mut nav = UndoNav::default();
        nav.record(Some("base"), "A", "提交", Uncommit, 0);
        nav.record(Some("A"), "B", "提交", Uncommit, 0);
        nav.commit_undo(); // 回到 A,B 在重做分支上
        assert_eq!(nav.can_redo(), Some(step("提交", "B", Uncommit)));
        // 在 A 上做了新提交 C → B 的重做分支被砍掉
        nav.record(Some("A"), "C", "提交", Uncommit, 0);
        assert_eq!(nav.can_redo(), None, "新操作清空重做分支");
        assert_eq!(nav.can_undo(), Some(step("提交", "A", Uncommit)));
    }

    #[test]
    fn record_skips_when_head_did_not_move() {
        let mut nav = UndoNav::default();
        nav.record(Some("A"), "B", "提交", Uncommit, 0);
        let before = (nav.cursor, nav.timeline.len());
        nav.record(Some("B"), "B", "拉取(pull)", Restore, 0); // up-to-date pull:HEAD 没动
        assert_eq!((nav.cursor, nav.timeline.len()), before, "无移动不该记一笔");
    }

    #[test]
    fn record_rebuilds_when_external_change_breaks_continuity() {
        let mut nav = UndoNav::default();
        nav.record(Some("base"), "A", "提交", Uncommit, 0);
        // 用户在终端切到别的分支(我们没记),现在从 X 提交到 Y:before=X 与光标点 A 对不上
        nav.record(Some("X"), "Y", "提交", Uncommit, 0);
        // 旧时间线作废,以 X 为基重建,只保留这次的可撤销
        assert_eq!(nav.can_undo(), Some(step("提交", "X", Uncommit)));
        assert_eq!(nav.can_redo(), None);
    }

    #[test]
    fn sync_bootstraps_single_undo_from_reflog_hint() {
        let mut nav = UndoNav::default();
        // 冷启动:时间线空,真实 HEAD=H,reflog 顶项是可撤销的「提交」,上一步=P
        nav.sync("H", Some(("P".into(), "提交".into(), Uncommit)), 0);
        assert_eq!(nav.can_undo(), Some(step("提交", "P", Uncommit)));
        assert_eq!(nav.can_redo(), None, "reflog 无法得知前进历史");
        assert!(nav.is_aligned("H"));
    }

    #[test]
    fn sync_bootstrap_preserves_restore_kind_for_reset() {
        let mut nav = UndoNav::default();
        // 冷启动顶项是 reset → 撤销它必须 hard。kind 要原样带进时间线。
        nav.sync("H", Some(("P".into(), "重置(reset)".into(), Restore)), 0);
        assert_eq!(nav.can_undo(), Some(step("重置(reset)", "P", Restore)));
    }

    #[test]
    fn sync_without_hint_has_no_undo() {
        let mut nav = UndoNav::default();
        nav.sync("H", None, 0); // 顶项不可撤销(如 checkout)或无上一步
        assert_eq!(nav.can_undo(), None);
        assert!(nav.is_aligned("H"));
    }

    #[test]
    fn sync_noop_when_already_aligned() {
        let mut nav = UndoNav::default();
        nav.record(Some("A"), "B", "提交", Uncommit, 0);
        nav.commit_undo(); // 在 A,保留 B 的重做分支
        nav.sync("A", Some(("zzz".into(), "重置(reset)".into(), Restore)), 0); // 已对齐 → 应忽略 boot
        assert_eq!(
            nav.can_redo(),
            Some(step("提交", "B", Uncommit)),
            "对齐时不该被 boot 重建"
        );
    }
}
