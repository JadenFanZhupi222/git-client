use serde::{Deserialize, Serialize};

/// 一条贮藏(git stash)。index 即 stash@{index},message 为其描述。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StashEntry {
    pub index: usize,
    pub message: String,
}
