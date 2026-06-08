use crate::model::Signature;
use serde::{Deserialize, Serialize};

/// 一条 reflog 记录(HEAD 的移动历史)。
/// reflog 是 git 的"后悔药":即使提交不在任何分支上(被 reset/rebase 丢弃),
/// 只要其 SHA 还在 reflog 里就能找回。`new_oid` 即该步操作后 HEAD 指向的提交,
/// 也就是"重置回这一步"时 reset 的目标。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflogEntry {
    /// 在 reflog 中的位置,即 `HEAD@{index}`(0 = 最近)。
    pub index: usize,
    /// 选择器文本,如 `HEAD@{0}`。
    pub selector: String,
    /// 该步操作后 HEAD 指向的完整 SHA(reset 回此步的目标)。
    pub new_oid: String,
    /// `new_oid` 的短 SHA。
    pub new_short: String,
    /// 操作描述,如 "commit: ..."、"reset: moving to ..."、"checkout: moving from ...”。
    pub message: String,
    /// 记录该步的署名(谁、何时)。
    pub committer: Signature,
    /// Unix 时间戳(秒)。
    pub timestamp: i64,
}
