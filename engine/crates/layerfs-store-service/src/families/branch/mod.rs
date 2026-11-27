//! Branch methods: history, differences, forks, merges and publication.
mod fork;
pub(crate) mod list;
mod merge;
mod publish;
mod remove;
pub use super::commit::diff::diff;
pub use super::commit::diff_branches::diff_branches;
pub use super::commit::diff_commits::diff_commits;
pub use super::commit::get::get_commit;
pub use super::commit::list::list_commits;
pub use fork::fork;
pub use merge::merge;
pub use publish::publish;
pub use remove::remove;
