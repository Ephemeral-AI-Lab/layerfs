mod apply;
mod change;
mod diff;
mod merge;
mod read;
mod resolve;
mod root;

pub use apply::{
    apply_directory_changes, apply_directory_changes_observed, apply_inode_mutations,
    create_directory, create_symlink, hard_link, remove_path, rename, replace_file, replace_range,
    replace_range_with_metadata, symlink_content, CandidateRoot, InodeMutation,
    StructuralBatchCounters,
};
pub use change::{
    apply_changes, set_mode, set_mtime, write_file, AppliedRoot, ApplyCounters, ContentChange,
    ContentConflict,
};
pub use diff::{diff_roots, RootDiff};
pub use merge::{merge_inode_change, merge_roots, three_way, MergeConflict, ThreeWayOutcome};
pub use read::{list, read_range, readlink, stat, stream, ListPage, Stat};
pub use resolve::{namespace, resolve, resolve_parent, LogicalCounters, Resolved};
pub use root::empty_root;
