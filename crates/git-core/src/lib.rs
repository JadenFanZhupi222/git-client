//! git-core:领域层。定义"在我们的世界里 git 长什么样"。
//! 这一层不依赖任何具体 git 实现(不 import gix/git2),保持纯净可测试。

pub mod model;
pub mod error;
pub mod backend;

pub use error::GitError;
pub use backend::GitBackend;
