mod diff;
mod merge;
mod mutate;
mod read;
mod resolver;

pub use diff::{diff_roots, RootDiff};
pub use merge::{merge_inode_change, merge_roots, MergeConflict};
pub use mutate::{
    apply_directory_changes, apply_inode_mutations, create_directory, create_symlink, hard_link,
    remove_path, rename, replace_file, replace_range, replace_range_with_metadata, symlink_content,
    CandidateRoot, InodeMutation,
};
pub use read::{list, read_range, readlink, stat, stream, ListPage, Stat};
pub use resolver::{namespace, resolve, resolve_parent, LogicalCounters, Resolved};
