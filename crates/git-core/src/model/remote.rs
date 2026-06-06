use serde::{Deserialize, Serialize};

/// 一次 fetch 的结果(MVP:不解析结构化更新明细)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FetchOutcome {
    /// 实际 fetch 的远程名;用默认远程时为空串。
    pub remote: String,
    /// git 的人类输出(stderr 那几行);为空表示「已是最新」。
    pub summary: String,
}
