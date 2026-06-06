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

/// 调色板大小:颜色按列号取模循环。
const NCOLORS: u32 = 8;

fn color_of(col: usize) -> u32 {
    (col as u32) % NCOLORS
}

/// 最左空闲列;没有则返回末尾(= 新增一列)。
fn first_free(lanes: &[Option<String>]) -> usize {
    lanes
        .iter()
        .position(|l| l.is_none())
        .unwrap_or(lanes.len())
}

fn ensure_len(lanes: &mut Vec<Option<String>>, n: usize) {
    if lanes.len() < n {
        lanes.resize(n, None);
    }
}

fn was_active(before: &[Option<String>], k: usize) -> bool {
    before.get(k).map(|l| l.is_some()).unwrap_or(false)
}

/// 把按时间倒序(新→旧)的提交列表排成图谱行。
pub fn layout(commits: &[Commit]) -> Vec<GraphRowDto> {
    // lanes[k] = Some(sha):第 k 列正等待到达该 sha 的提交;None = 空闲。
    let mut lanes: Vec<Option<String>> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());

    for c in commits {
        let before = lanes.clone();

        // 1) 节点列:等待本提交的最左列;没有则取最左空闲列。
        let node_col = lanes
            .iter()
            .position(|l| l.as_deref() == Some(c.id.as_str()))
            .unwrap_or_else(|| first_free(&lanes));
        ensure_len(&mut lanes, node_col + 1);

        // 2) 所有等待本提交的列(全部汇入节点;非 node_col 的随后释放)。
        let merging: Vec<usize> = before
            .iter()
            .enumerate()
            .filter(|(_, l)| l.as_deref() == Some(c.id.as_str()))
            .map(|(k, _)| k)
            .collect();

        // 3) 计算 lanes_after。
        for &k in &merging {
            lanes[k] = None; // 先清空所有汇入列
        }
        // node_col 接第一个父(没有父则空)
        lanes[node_col] = c.parents.first().cloned();
        // 其余父各占一条新列
        for p in c.parents.iter().skip(1) {
            let k = first_free(&lanes);
            ensure_len(&mut lanes, k + 1);
            lanes[k] = Some(p.clone());
        }
        let after = lanes.clone();

        // 4) 上半段:基于 before。汇入本提交的列连到 node_col,其余直穿。
        let mut top = Vec::new();
        for (k, l) in before.iter().enumerate() {
            if l.is_none() {
                continue;
            }
            let to = if merging.contains(&k) { node_col } else { k };
            top.push(GraphSegDto {
                from: k as u32,
                to: to as u32,
                color: color_of(k),
            });
        }

        // 5) 下半段:基于 after。本提交新生的列(node 延续 / 新父)从 node_col 出发,其余直穿。
        let mut bottom = Vec::new();
        for (k, l) in after.iter().enumerate() {
            if l.is_none() {
                continue;
            }
            let spawned = k == node_col || !was_active(&before, k);
            let from = if spawned { node_col } else { k };
            bottom.push(GraphSegDto {
                from: from as u32,
                to: k as u32,
                color: color_of(k),
            });
        }

        rows.push(GraphRowDto {
            commit: CommitDto::from(c.clone()),
            column: node_col as u32,
            color: color_of(node_col),
            top,
            bottom,
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

        // base 收敛:上半段有一条来自列 1 汇入列 0
        let base = &rows[3];
        assert!(
            base.top.iter().any(|s| s.from == 1 && s.to == 0),
            "base 应有从第 1 列汇入第 0 列的合并线"
        );
        assert!(base.bottom.is_empty(), "根提交无父,无下半段");
    }

    #[test]
    fn second_parent_lane_is_colored_by_column() {
        let rows = layout(&[
            commit("m", &["a", "b"]),
            commit("a", &["x"]),
            commit("b", &["x"]),
        ]);
        // b 在第 1 列,颜色应 = 列号 1
        assert_eq!(rows[2].column, 1);
        assert_eq!(rows[2].color, 1);
    }
}
