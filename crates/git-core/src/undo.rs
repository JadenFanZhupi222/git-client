//! 「撤销上一步」的纯领域逻辑。
//!
//! 撤销 = 把当前分支指针挪回上一步操作之前的位置(reflog 里的 `HEAD@{1}`)。
//! 我们用 `reset --soft` 实现:它只移动分支指针,**绝不碰工作区文件内容**,
//! 所以撤销永不丢数据——被撤销的提交内容会原样回到暂存区。
//!
//! 但不是所有 reflog 顶项都能这样安全撤销:`checkout`(切分支)只是把 HEAD
//! 指到别的分支,用 `reset` 去撤销会**错误地移动分支指针**。所以这里按
//! reflog 顶项的 message 分类,只放行"在当前分支上造成提交 / 移动 HEAD"的安全集。
//!
//! 这一层是纯函数(只吃一个字符串),不碰任何 git 实现,可毫秒级单测。

/// 撤销一个操作时,该用哪种「还原语义」——决定上层用 `reset --soft` 还是 `--hard`。
///
/// 这是本模块的核心区分。两类操作的撤销方式根本不同:
/// - [`UndoKind::Uncommit`](提交 / amend):撤销 = 「反提交」,内容退回暂存区供你继续改。
///   只动 HEAD 指针,**绝不碰工作区**,永远安全 → `reset --soft`。
/// - [`UndoKind::Restore`](reset / cherry-pick / revert / merge / rebase / pull):
///   这些操作改写了历史**并改动了工作区/暂存区**。撤销必须**忠实还原**到操作前的完整
///   状态(HEAD+暂存区+工作区),否则会留下残渣(比如撤销 `reset --hard` 后用 soft,
///   暂存区会凭空多出文件)→ `reset --hard`(由上层在工作区干净时才执行)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoKind {
    /// 提交类:撤销退回暂存区(soft),保留你的活。
    Uncommit,
    /// 改写/移动类:撤销需忠实还原完整工作区(hard)。
    Restore,
}

/// 判断一条 reflog 顶项(`HEAD@{0}`)的操作能否用 reset 安全撤销,并给出还原语义。
///
/// 返回 `Some((中文操作名, 还原类别))` 表示可撤销;`None` 表示不可撤销
/// (checkout 切分支 / clone / branch / 未识别 —— 用 reset 撤销会错移分支指针)。
///
/// git 的 reflog message 形如:`"commit: 修复 X"`、`"commit (amend): ..."`、
/// `"reset: moving to HEAD~1"`、`"merge feature: Merge made by..."`、
/// `"cherry-pick: ..."`、`"revert: ..."`、`"rebase (finish): ..."`、
/// `"pull: Fast-forward"`、`"checkout: moving from a to b"`。
pub fn classify_undoable(reflog_message: &str) -> Option<(&'static str, UndoKind)> {
    use UndoKind::*;
    let msg = reflog_message.trim_start();
    // 取第一个 token(到空格或冒号为止),即操作类型关键字。
    let head = msg.split([' ', ':']).next().unwrap_or("");
    match head {
        // amend 也是一次 commit,但语义上要分开提示。提交类 → 撤销退回暂存区。
        "commit" if msg.starts_with("commit (amend)") => Some(("修改提交(amend)", Uncommit)),
        "commit" => Some(("提交", Uncommit)),
        // 以下都改动了工作区/暂存区,撤销需忠实还原(hard)。
        "cherry-pick" => Some(("cherry-pick", Restore)),
        "revert" => Some(("回退提交(revert)", Restore)),
        "merge" => Some(("合并", Restore)),
        "rebase" => Some(("变基(rebase)", Restore)),
        "reset" => Some(("重置(reset)", Restore)),
        "pull" => Some(("拉取(pull)", Restore)),
        // checkout / clone / branch / 未识别 → 不冒险用 reset 撤销。
        _ => None,
    }
}

/// [`classify_undoable`] 的便捷封装:只取中文操作名(用于只关心文案、不关心还原语义的地方)。
pub fn undoable_op_label(reflog_message: &str) -> Option<&'static str> {
    classify_undoable(reflog_message).map(|(label, _)| label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_undoable_ops() {
        assert_eq!(undoable_op_label("commit: 修复登录"), Some("提交"));
        assert_eq!(
            undoable_op_label("commit (amend): 改了信息"),
            Some("修改提交(amend)")
        );
        assert_eq!(
            undoable_op_label("commit (initial): 第一次提交"),
            Some("提交")
        );
        assert_eq!(
            undoable_op_label("reset: moving to HEAD~1"),
            Some("重置(reset)")
        );
        assert_eq!(undoable_op_label("cherry-pick: 摘取"), Some("cherry-pick"));
        assert_eq!(undoable_op_label("revert: 回退"), Some("回退提交(revert)"));
        assert_eq!(
            undoable_op_label("merge feature: Merge made by the 'ort' strategy."),
            Some("合并")
        );
        assert_eq!(
            undoable_op_label("rebase (finish): returning to refs/heads/main"),
            Some("变基(rebase)")
        );
        assert_eq!(undoable_op_label("pull: Fast-forward"), Some("拉取(pull)"));
    }

    #[test]
    fn refuses_branch_switching_and_unknown() {
        // checkout 切分支:用 reset 撤销会错移分支指针,必须拒绝。
        assert_eq!(undoable_op_label("checkout: moving from main to dev"), None);
        assert_eq!(undoable_op_label("clone: from https://x"), None);
        assert_eq!(undoable_op_label("branch: Created from HEAD"), None);
        assert_eq!(undoable_op_label(""), None);
        assert_eq!(undoable_op_label("某种未来才有的操作"), None);
    }

    #[test]
    fn classifies_restore_semantics() {
        use UndoKind::*;
        // 提交类 → Uncommit(撤销退回暂存区)。
        assert_eq!(classify_undoable("commit: x").map(|c| c.1), Some(Uncommit));
        assert_eq!(
            classify_undoable("commit (amend): x").map(|c| c.1),
            Some(Uncommit)
        );
        // 改写/移动类 → Restore(撤销需忠实还原工作区)。这正是 reset --hard 撤销
        // 必须走 hard、而非 soft 的依据(soft 留残渣)。
        for msg in [
            "reset: moving to HEAD~1",
            "cherry-pick: x",
            "revert: x",
            "merge feature: ...",
            "rebase (finish): ...",
            "pull: Fast-forward",
        ] {
            assert_eq!(
                classify_undoable(msg).map(|c| c.1),
                Some(Restore),
                "{msg} 应归为 Restore"
            );
        }
        assert_eq!(classify_undoable("checkout: moving"), None);
    }
}
