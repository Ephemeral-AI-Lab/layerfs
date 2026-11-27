//! Workspace-local open state, under the same exclusive turn as SQLite.
mod counts;
mod directories;
pub use counts::OpenStateCounts;
mod journal;
pub use directories::OpenDisposals;
pub(crate) use directories::{CookiePage, DirectoryState};
pub(crate) mod files;
pub(crate) mod state;
pub(crate) use state::{KernelParent, KernelReference, OpenState};
