//! git-core:领域层。定义"在我们的世界里 git 长什么样"。
//! 这一层不依赖任何具体 git 实现(不 import gix/git2),保持纯净可测试。

pub mod backend;
pub mod error;
pub mod model;

pub use backend::GitBackend;
pub use error::GitError;
