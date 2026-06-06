//! git-engine:适配器层。GitBackend trait 的具体实现。

mod fake;
pub use fake::FakeBackend;

mod cli_backend;
pub use cli_backend::CliBackend;

#[cfg(feature = "git2-backend")]
mod git2_backend;
#[cfg(feature = "git2-backend")]
pub use git2_backend::Git2Backend;

// CompositeBackend 组合 git2(本地)+ cli(网络),依赖 git2 后端。
#[cfg(feature = "git2-backend")]
mod composite;
#[cfg(feature = "git2-backend")]
pub use composite::CompositeBackend;
