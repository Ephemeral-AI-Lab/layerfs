//! Branch methods: listing, forks, merges, publication and removal.
mod fork;
mod list;
mod merge;
mod publish;
mod remove;
pub use fork::fork;
pub use list::list_branches;
pub use merge::merge;
pub use publish::publish;
pub use remove::remove;
