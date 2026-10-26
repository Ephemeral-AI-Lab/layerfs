//! Workspace-local open state, under the same exclusive turn as SQLite.
mod counts;
mod directories;
pub use counts::OpenStateCounts;
mod journal;
pub use directories::OpenDisposals;
pub(crate) use directories::{CookiePage, DirectoryState};
pub(crate) mod files;
mod kernel;
pub(crate) use kernel::{KernelReference, OpenState};
