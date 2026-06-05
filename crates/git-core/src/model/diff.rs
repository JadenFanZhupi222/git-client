use serde::{Deserialize, Serialize};
use crate::model::FileState;

/// 一个提交里改动的单个文件(文件级,不含行)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub status: FileState,
}
