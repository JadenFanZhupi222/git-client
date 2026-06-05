use serde::{Deserialize, Serialize};

/// 我们自己的 Commit 模型。底层是 gix 还是 git2,上层和前端都不需要知道。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub body: String,
    pub author: Signature,
    pub timestamp: i64,
    pub parents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub name: String,
    pub email: String,
}
