//! Branch methods: history, differences, forks, merges and publication.
mod diff;
mod diff_branches;
mod diff_commits;
mod fork;
mod get_commit;
mod list_commits;
mod merge;
mod publish;
mod remove;
pub use diff::diff;
pub use diff_branches::diff_branches;
pub use diff_commits::diff_commits;
pub use fork::fork;
pub use get_commit::get_commit;
pub use list_commits::list_commits;
pub use merge::merge;
pub use publish::publish;
pub use remove::remove;
