//! git-engine:适配器层。GitBackend trait 的具体实现。

mod fake;
pub use fake::FakeBackend;

#[cfg(feature = "git2-backend")]
mod git2_backend;
#[cfg(feature = "git2-backend")]
pub use git2_backend::Git2Backend;
