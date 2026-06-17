use serde::{Deserialize, Serialize};

/// 一个远程的展示信息:名字 + fetch URL。供「管理远程」面板展示。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteInfo {
    pub name: String,
    /// fetch URL;未配置 URL(罕见)时为空串。
    pub url: String,
}

/// 一次 fetch 的结果(MVP:不解析结构化更新明细)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FetchOutcome {
    /// 实际 fetch 的远程名;用默认远程时为空串。
    pub remote: String,
    /// git 的人类输出(stderr 那几行);为空表示「已是最新」。
    pub summary: String,
}

/// 一次 pull(fetch + merge)的成功结果。冲突/无上游等以 GitError 返回。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PullOutcome {
    /// git pull 的人类输出(Already up to date. / Fast-forward / Merge made…)。
    pub summary: String,
}

/// 一次「把某分支合并进当前分支」的成功结果。冲突以 GitError::MergeConflict 返回。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeOutcome {
    /// git merge 的人类输出(Already up to date. / Fast-forward / Merge made…)。
    pub summary: String,
    /// 本次是否为快进(fast-forward,未产生合并提交)。
    pub fast_forward: bool,
}

/// 一次 push 的成功结果。被拒(non-fast-forward)等以 GitError 返回。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PushOutcome {
    /// git push 的人类输出(Everything up-to-date / 各 ref 更新行)。
    pub summary: String,
    /// 本次是否顺带建立了上游(首次 push 自动 -u 时为 true)。
    pub set_upstream: bool,
}
