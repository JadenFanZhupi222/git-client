//! 提交图谱布局(lane assignment)。见 ARCHITECTURE 第 3.3 / 阶段 3。
//!
//! 纯逻辑、可单测。把"提交 + 父子关系"排成多分支泳道图:
//! 给每个提交分配一个列(lane),并算出每行单元格里要画的连线段。
//!
//! 渲染契约:每行单元格节点居中(中点)。
//!
//! - `top`:顶边 → 中点(其它分支汇入本节点,或直穿的上半段)
//! - `bottom`:中点 → 底边(本节点分叉出父提交,或直穿的下半段)
//!
//! 前端按 列号 × 列宽 画线即可。

use git_core::model::Commit;
use ipc_types::{CommitDto, GraphRowDto, GraphSegDto};
use std::collections::HashSet;

/// 调色板大小:颜色循环复用。
const NCOLORS: u32 = 8;

/// 一条 lane:正在等待到达 `sha` 的提交,以及它出生时分到的颜色。
///
/// 关键:颜色随 lane 走、不随列号变。一条分支从分叉到合并始终同色,
/// 这样视觉上能跟踪「这是同一条分支」,而不是「这是第 1 列」。
#[derive(Clone)]
struct Lane {
    sha: String,
    color: u32,
}

/// 最左空闲列;没有则返回末尾(= 新增一列)。
fn first_free(lanes: &[Option<Lane>]) -> usize {
    lanes
        .iter()
        .position(|l| l.is_none())
        .unwrap_or(lanes.len())
}

fn ensure_len(lanes: &mut Vec<Option<Lane>>, n: usize) {
    if lanes.len() < n {
        lanes.resize(n, None);
    }
}

fn was_active(before: &[Option<Lane>], k: usize) -> bool {
    before.get(k).map(|l| l.is_some()).unwrap_or(false)
}

/// 给新 lane 挑一个颜色:当前所有活跃 lane 都没用到的最小色号。
/// 这样并存的相邻分支保证不同色;并发分支 ≤ NCOLORS 时永不撞色。
fn pick_color(lanes: &[Option<Lane>]) -> u32 {
    let used: HashSet<u32> = lanes.iter().flatten().map(|l| l.color).collect();
    (0..NCOLORS).find(|c| !used.contains(c)).unwrap_or(0)
}

/// 把按时间倒序(新→旧)的提交列表排成图谱行。
pub fn layout(commits: &[Commit]) -> Vec<GraphRowDto> {
    // lanes[k] = Some(Lane):第 k 列正等待到达该 sha 的提交;None = 空闲。
    let mut lanes: Vec<Option<Lane>> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());

    for c in commits {
        let before = lanes.clone();

        // 1) 节点列:等待本提交的最左列;没有则取最左空闲列。
        let node_col = lanes
            .iter()
            .position(|l| l.as_ref().map(|x| x.sha.as_str()) == Some(c.id.as_str()))
            .unwrap_or_else(|| first_free(&lanes));
        ensure_len(&mut lanes, node_col + 1);

        // 节点颜色:延续 node_col 上已有 lane 的颜色;若该列是新生的(分支 tip /
        // 孤立 HEAD),则现挑一个不撞色的新颜色。整条分支由此保持同色。
        let node_color = match &lanes[node_col] {
            Some(l) => l.color,
            None => pick_color(&lanes),
        };

        // 2) 所有等待本提交的列(全部汇入节点;非 node_col 的随后释放)。
        let merging: Vec<usize> = before
            .iter()
            .enumerate()
            .filter(|(_, l)| l.as_ref().map(|x| x.sha.as_str()) == Some(c.id.as_str()))
            .map(|(k, _)| k)
            .collect();

        // 3) 计算 lanes_after,并记录每个父的去向(用于下半段连线)。
        for &k in &merging {
            lanes[k] = None; // 先清空所有汇入列
        }

        // 每个父决定一条「从本节点出发」的下半段线 (to_col, color):
        //   - 父已有 lane 在等  → 直接并入该列(干净的合并弧,不再开冗余 lane);
        //   - 否则第一个父       → 延续 node_col,主干同色直下;
        //   - 否则               → 各开一条新列,挑不撞色的新颜色。
        let mut spawns: Vec<(usize, u32)> = Vec::new();
        for (i, p) in c.parents.iter().enumerate() {
            if let Some(k) = lanes
                .iter()
                .position(|l| l.as_ref().map(|x| x.sha.as_str()) == Some(p.as_str()))
            {
                // 该父已在 col k 等候(通常是更早的主干)→ 本节点并入,不新开列。
                spawns.push((k, node_color));
            } else if i == 0 {
                lanes[node_col] = Some(Lane {
                    sha: p.clone(),
                    color: node_color,
                });
                spawns.push((node_col, node_color));
            } else {
                let k = first_free(&lanes);
                ensure_len(&mut lanes, k + 1);
                let color = pick_color(&lanes);
                lanes[k] = Some(Lane {
                    sha: p.clone(),
                    color,
                });
                spawns.push((k, color));
            }
        }
        let after = lanes.clone();

        // 4) 上半段:基于 before。汇入本提交的列连到 node_col,其余直穿。
        //    线的颜色 = 该 lane 自己的颜色(随分支,不随列)。
        let mut top = Vec::new();
        for (k, l) in before.iter().enumerate() {
            let Some(lane) = l else { continue };
            let to = if merging.contains(&k) { node_col } else { k };
            top.push(GraphSegDto {
                from: k as u32,
                to: to as u32,
                color: lane.color,
            });
        }

        // 5) 下半段:本节点出发的线(延续 / 新父 / 并入已有列)+ 与本提交无关的直穿 lane。
        let mut bottom = Vec::new();
        for &(to, color) in &spawns {
            bottom.push(GraphSegDto {
                from: node_col as u32,
                to: to as u32,
                color,
            });
        }
        for (k, l) in after.iter().enumerate() {
            // 直穿:本就活跃、且本行没动过它(非 node_col、非汇入列)的 lane。
            if k == node_col || merging.contains(&k) || !was_active(&before, k) {
                continue;
            }
            if let Some(lane) = l {
                bottom.push(GraphSegDto {
                    from: k as u32,
                    to: k as u32,
                    color: lane.color,
                });
            }
        }

        rows.push(GraphRowDto {
            commit: CommitDto::from(c.clone()),
            column: node_col as u32,
            color: node_color,
            top,
            bottom,
            refs: Vec::new(),    // 由 service 在布局后按 SHA 挂上引用。
            sync: String::new(), // 由 service 在布局后按未 push/未 pull 标记。
        });
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use git_core::model::Signature;

    fn commit(id: &str, parents: &[&str]) -> Commit {
        Commit {
            id: id.into(),
            short_id: id.into(),
            summary: format!("commit {id}"),
            body: String::new(),
            author: Signature {
                name: "t".into(),
                email: "e".into(),
            },
            timestamp: 0,
            parents: parents.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn linear_history_single_column() {
        // a → b → c,全在第 0 列
        let rows = layout(&[commit("a", &["b"]), commit("b", &["c"]), commit("c", &[])]);
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|r| r.column == 0));
        assert!(rows.iter().all(|r| r.color == 0));
        // 第一个提交(HEAD)向下延续一条线
        assert_eq!(rows[0].bottom.len(), 1);
        assert_eq!((rows[0].bottom[0].from, rows[0].bottom[0].to), (0, 0));
        // 最后一个提交(根)无父 → 无下半段
        assert!(rows[2].bottom.is_empty());
    }

    #[test]
    fn diamond_merge_assigns_two_lanes() {
        // 菱形:m 合并 a、b;a、b 同源 base
        //   m(parents a,b) / a(base) / b(base) / base()
        let rows = layout(&[
            commit("m", &["a", "b"]),
            commit("a", &["base"]),
            commit("b", &["base"]),
            commit("base", &[]),
        ]);
        let col: Vec<u32> = rows.iter().map(|r| r.column).collect();
        // m 在 0;a 承接 m 的第一个父留在 0;b 是 m 的第二个父 → 第 1 列;base 收敛回 0
        assert_eq!(col, vec![0, 0, 1, 0]);

        // m 向下分叉出两条线:到列 0 和列 1
        let m = &rows[0];
        assert!(m.bottom.iter().any(|s| s.to == 0));
        assert!(m.bottom.iter().any(|s| s.to == 1));

        // 去重后:b 的父 base 已在第 0 列等候,合并在 b 这一行就完成 ——
        // b 的下半段直接画一条「第 1 列 → 第 0 列」的合并弧,不再多占一行。
        let b = &rows[2];
        assert!(
            b.bottom.iter().any(|s| s.from == 1 && s.to == 0),
            "b 应直接并入第 0 列(去重:不开冗余 lane)"
        );
        // 因此 base 不再需要承接一条来自第 1 列的汇入线。
        let base = &rows[3];
        assert!(
            !base.top.iter().any(|s| s.from == 1),
            "合并已在 b 行完成,base 上半段不应再有第 1 列汇入"
        );
        assert!(base.bottom.is_empty(), "根提交无父,无下半段");
    }

    #[test]
    fn concurrent_lanes_get_distinct_colors() {
        // m 分叉出两条并存分支,二者颜色必须不同(相邻不撞色)。
        let rows = layout(&[
            commit("m", &["a", "b"]),
            commit("a", &["x"]),
            commit("b", &["x"]),
        ]);
        // 主干(m→a)沿用 m 的颜色;第二父 b 现挑一个不同色。
        assert_eq!(rows[0].color, rows[1].color, "主干延续应同色");
        assert_ne!(rows[1].color, rows[2].color, "并存的两条分支必须异色");
    }

    #[test]
    fn lane_color_is_stable_along_branch() {
        // 一条分支从分叉到合并应始终同色,不随它所在列变化。
        //   m(a,b) / a(c) / c(e) / b(e) / e()
        // 第二父分支:b 这一条 lane 走到 e 之前颜色不变。
        let rows = layout(&[
            commit("m", &["a", "b"]),
            commit("a", &["c"]),
            commit("c", &["e"]),
            commit("b", &["e"]),
            commit("e", &[]),
        ]);
        // b 分支的颜色 = m 下半段里通向 b 那条线的颜色,且 b 节点本身同色。
        let b_branch_color = rows[0]
            .bottom
            .iter()
            .find(|s| s.to != rows[0].column)
            .expect("m 应分叉出第二父分支")
            .color;
        let b_row = rows.iter().find(|r| r.commit.id == "b").unwrap();
        assert_eq!(b_row.color, b_branch_color, "b 分支从诞生到此处应保持同色");
        // b 并入主干时(b 行下半段)那条合并弧仍是 b 分支的颜色 —— 同色贯穿始终。
        assert!(
            b_row
                .bottom
                .iter()
                .any(|s| s.from != s.to && s.color == b_branch_color),
            "b 并入主干的合并弧应保持同色"
        );
    }
}
