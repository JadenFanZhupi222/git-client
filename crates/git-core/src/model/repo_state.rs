use serde::{Deserialize, Serialize};

/// 仓库当前是否处于某个「进行中」的操作(决定是否显示冲突横幅与 继续/中止)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepoState {
    Clean,
    Merging,
    Rebasing,
    CherryPicking,
    Reverting,
    /// 其它进行中状态(bisect / am 等),暂不细分。
    Other,
}
