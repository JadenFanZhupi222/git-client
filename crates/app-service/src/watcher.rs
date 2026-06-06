//! 文件系统监听:工作区一变,自动通知上层刷新。
//! 见 ARCHITECTURE 第 7 部分。设计要点:
//! - debounce:批量事件攒一个窗口合并成一次通知,避免刷屏。
//! - 区分 `.git` 内部信号(HEAD/index/refs) vs 工作区 vs 噪音。
//! - 复用 `.gitignore`:target/node_modules 等海量变化必须忽略。
//!
//! 纯逻辑(classify/coalesce)可单测;RepoWatcher 是 notify 整合层。

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::{RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

/// 一次变化需要触发的刷新范围。优先级:GitRef > Index > WorkingTree。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// 工作区文件变化 → 刷 status。
    WorkingTree,
    /// .git/index 变化 → 暂存状态变化 → 刷 status。
    Index,
    /// HEAD / refs 变化 → 切分支或新提交 → 刷 log + branch + status。
    GitRef,
}

impl ChangeKind {
    fn priority(self) -> u8 {
        match self {
            ChangeKind::WorkingTree => 1,
            ChangeKind::Index => 2,
            ChangeKind::GitRef => 3,
        }
    }
}

/// 把一个变化路径分类。返回 None 表示噪音,应忽略。
/// `repo_root` 为仓库根绝对路径,`changed` 为变化文件的绝对路径。
pub fn classify(repo_root: &Path, changed: &Path) -> Option<ChangeKind> {
    let rel = changed.strip_prefix(repo_root).ok()?;
    let mut comps = rel.components();
    let first = comps.next()?.as_os_str();
    if first == ".git" {
        // .git 内部:只认 HEAD / index / refs/*,其余(objects、各种 .lock 等)忽略。
        match comps
            .next()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
        {
            Some(ref s) if s == "index" => Some(ChangeKind::Index),
            Some(ref s) if s == "HEAD" => Some(ChangeKind::GitRef),
            Some(ref s) if s == "refs" => Some(ChangeKind::GitRef),
            _ => None,
        }
    } else {
        Some(ChangeKind::WorkingTree)
    }
}

/// 把一批变化类型合并成单个"最广刷新范围"。空批返回 None。
pub fn coalesce(kinds: &[ChangeKind]) -> Option<ChangeKind> {
    kinds.iter().copied().max_by_key(|k| k.priority())
}

/// 监听一个仓库目录。Drop 时自动停止监听并结束后台线程。
pub struct RepoWatcher {
    _watcher: notify::RecommendedWatcher,
}

impl RepoWatcher {
    /// 开始监听 `repo_root`。每攒满一个 debounce 窗口、且有有效变化时,
    /// 在后台线程调用 `on_change(合并后的 ChangeKind)`。
    pub fn new(
        repo_root: PathBuf,
        debounce: Duration,
        on_change: impl Fn(ChangeKind) + Send + 'static,
    ) -> Result<Self, notify::Error> {
        let (tx, rx) = mpsc::channel::<Vec<PathBuf>>();

        let mut watcher =
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    let _ = tx.send(event.paths);
                }
            })?;
        watcher.watch(&repo_root, RecursiveMode::Recursive)?;

        // 复用仓库忽略规则;失败则退化为空匹配器(不过滤)。
        let gi = build_gitignore(&repo_root);

        std::thread::spawn(move || {
            // 阻塞等第一个事件;watcher 被 drop → channel 关闭 → 退出线程。
            while let Ok(first) = rx.recv() {
                let mut batch = first;
                let deadline = Instant::now() + debounce;
                // debounce:窗口内继续吸收后续事件,合并成一批。
                loop {
                    let now = Instant::now();
                    if now >= deadline {
                        break;
                    }
                    match rx.recv_timeout(deadline - now) {
                        Ok(mut more) => batch.append(&mut more),
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }

                let kinds: Vec<ChangeKind> = batch
                    .iter()
                    .filter_map(|p| {
                        let k = classify(&repo_root, p)?;
                        // 工作区变化再过一道 .gitignore:被忽略的(target/node_modules)丢弃。
                        if k == ChangeKind::WorkingTree
                            && gi.matched_path_or_any_parents(p, false).is_ignore()
                        {
                            return None;
                        }
                        Some(k)
                    })
                    .collect();

                if let Some(kind) = coalesce(&kinds) {
                    on_change(kind);
                }
            }
        });

        Ok(Self { _watcher: watcher })
    }
}

/// 从仓库根的 .gitignore 构建匹配器。任何失败都退化成空匹配器(不过滤)。
fn build_gitignore(repo_root: &Path) -> Gitignore {
    let mut b = GitignoreBuilder::new(repo_root);
    let _ = b.add(repo_root.join(".gitignore"));
    b.build().unwrap_or_else(|_| Gitignore::empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("/repo")
    }

    #[test]
    fn classify_working_tree_file() {
        assert_eq!(
            classify(&root(), Path::new("/repo/src/main.rs")),
            Some(ChangeKind::WorkingTree)
        );
    }

    #[test]
    fn classify_git_index_is_index() {
        assert_eq!(
            classify(&root(), Path::new("/repo/.git/index")),
            Some(ChangeKind::Index)
        );
    }

    #[test]
    fn classify_git_head_is_ref() {
        assert_eq!(
            classify(&root(), Path::new("/repo/.git/HEAD")),
            Some(ChangeKind::GitRef)
        );
    }

    #[test]
    fn classify_git_refs_is_ref() {
        assert_eq!(
            classify(&root(), Path::new("/repo/.git/refs/heads/main")),
            Some(ChangeKind::GitRef)
        );
    }

    #[test]
    fn classify_git_internal_noise_ignored() {
        // objects、index.lock 等都是噪音
        assert_eq!(
            classify(&root(), Path::new("/repo/.git/objects/ab/cd")),
            None
        );
        assert_eq!(classify(&root(), Path::new("/repo/.git/index.lock")), None);
    }

    #[test]
    fn classify_outside_repo_ignored() {
        assert_eq!(classify(&root(), Path::new("/elsewhere/x")), None);
    }

    #[test]
    fn coalesce_picks_broadest() {
        // 同批里既有工作区又有 HEAD 变化 → 取最广的 GitRef
        let kinds = [
            ChangeKind::WorkingTree,
            ChangeKind::GitRef,
            ChangeKind::Index,
        ];
        assert_eq!(coalesce(&kinds), Some(ChangeKind::GitRef));
    }

    #[test]
    fn coalesce_index_over_worktree() {
        let kinds = [ChangeKind::WorkingTree, ChangeKind::Index];
        assert_eq!(coalesce(&kinds), Some(ChangeKind::Index));
    }

    #[test]
    fn coalesce_empty_is_none() {
        assert_eq!(coalesce(&[]), None);
    }
}
