use serde::{Deserialize, Serialize};

/// 文件中的一行及「最后修改它的提交」信息(git blame)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlameLine {
    pub line_no: u32,
    /// 最后修改本行的提交完整 SHA;未提交的本地改动为空串。
    pub commit_id: String,
    pub short_id: String,
    pub author_name: String,
    pub timestamp: i64,
    pub content: String,
}
