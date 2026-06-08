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

/// 判断一条 reflog 顶项(`HEAD@{0}`)的操作能否用「reset --soft 到上一步」安全撤销。
///
/// 返回 `Some(中文操作名)` 表示可撤销(用于按钮/toast 文案),`None` 表示不可撤销。
///
/// git 的 reflog message 形如:`"commit: 修复 X"`、`"commit (amend): ..."`、
/// `"reset: moving to HEAD~1"`、`"merge feature: Merge made by..."`、
/// `"cherry-pick: ..."`、`"revert: ..."`、`"rebase (finish): ..."`、
/// `"pull: Fast-forward"`、`"checkout: moving from a to b"`。
pub fn undoable_op_label(reflog_message: &str) -> Option<&'static str> {
    let msg = reflog_message.trim_start();
    // 取第一个 token(到空格或冒号为止),即操作类型关键字。
    let head = msg.split([' ', ':']).next().unwrap_or("");
    match head {
        // amend 也是一次 commit,但语义上要分开提示。
        "commit" if msg.starts_with("commit (amend)") => Some("修改提交(amend)"),
        "commit" => Some("提交"),
        "cherry-pick" => Some("cherry-pick"),
        "revert" => Some("回退提交(revert)"),
        "merge" => Some("合并"),
        "rebase" => Some("变基(rebase)"),
        "reset" => Some("重置(reset)"),
        "pull" => Some("拉取(pull)"),
        // checkout / clone / branch / 未识别 → 不冒险用 reset 撤销。
        _ => None,
    }
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
}
