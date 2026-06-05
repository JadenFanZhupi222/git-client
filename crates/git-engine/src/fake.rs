use std::path::Path;
use git_core::{GitBackend, GitError};
use git_core::model::{Commit, Signature, WorkingTreeStatus};

/// 测试/演示用的假后端:返回写死的数据,不碰真实仓库。
/// 它存在的意义:证明上层只依赖 trait,可以无成本替换实现 + 测试飞快。
#[derive(Default)]
pub struct FakeBackend;

impl GitBackend for FakeBackend {
    fn open(&self, _path: &Path) -> Result<(), GitError> {
        Ok(())
    }

    fn head_commit(&self, _path: &Path) -> Result<Commit, GitError> {
        Ok(Commit {
            id: "0123456789abcdef0123456789abcdef01234567".into(),
            short_id: "0123456".into(),
            summary: "这是来自 FakeBackend 的假提交".into(),
            body: String::new(),
            author: Signature { name: "测试者".into(), email: "test@example.com".into() },
            timestamp: 1_700_000_000,
            parents: vec![],
        })
    }

    fn status(&self, _path: &Path) -> Result<WorkingTreeStatus, GitError> {
        Ok(WorkingTreeStatus { entries: vec![] })
    }
}
