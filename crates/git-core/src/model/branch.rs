use serde::{Deserialize, Serialize};

/// 本地分支信息(阶段 2a 最小集:名字 + 是否当前)。
/// 后续切片会扩展 upstream / ahead-behind 等字段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchInfo {
    /// 短名,如 "main" / "feat/graph"。
    pub name: String,
    /// 是否为当前 HEAD 所在分支。
    pub is_head: bool,
}
