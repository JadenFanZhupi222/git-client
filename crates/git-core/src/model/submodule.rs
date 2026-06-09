use serde::{Deserialize, Serialize};

/// 子模块相对超级项目(superproject)的状态,对应 `git submodule status` 行首字符。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubmoduleStatus {
    /// 未初始化(`-`):尚未 clone / checkout,工作区里是空目录。
    #[default]
    Uninitialized,
    /// 已同步(空格):当前检出的提交 = 超级项目记录的提交。
    UpToDate,
    /// 未同步(`+`):当前检出的提交 ≠ 超级项目记录的提交(子模块里有提交或被改动)。
    Modified,
    /// 合并冲突(`U`):子模块引用处于冲突状态。
    Conflict,
}

/// 一个子模块的信息:路径 + 远程地址 + 当前提交 + 状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubmoduleInfo {
    /// 仓库根相对路径(git 风格,正斜杠),如 "vendor/libfoo"。
    pub path: String,
    /// `.gitmodules` 里登记的远程 URL(读不到时为空)。
    pub url: String,
    /// `git submodule status` 报告的提交 SHA(已检出的提交;未初始化时为记录的提交)。
    pub head_sha: String,
    /// 状态(未初始化 / 已同步 / 未同步 / 冲突)。
    pub status: SubmoduleStatus,
    /// `git submodule status` 末尾括号里的描述(如 `heads/main` / `v1.0-2-gabc`),无则空。
    pub describe: String,
}
