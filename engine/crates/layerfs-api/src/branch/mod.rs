//! Branch methods: history, differences, forks, merges and publication.
mod changed_paths;
mod diff_branches;
mod diff_commits;
mod fork;
mod graph;
mod list_commits;
mod merge;
mod publish;
pub use changed_paths::changed_paths;
pub use diff_branches::diff_branches;
pub use diff_commits::diff_commits;
pub use fork::fork;
pub use graph::graph;
pub use list_commits::list_commits;
pub use merge::merge;
pub use publish::publish;
