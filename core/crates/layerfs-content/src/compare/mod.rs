//! Comparison of immutable roots and bounded texts.
//!
//! Entry module: declarations and re-exports only.

pub mod bytes;
pub mod changes;
pub mod memo;
pub mod named;
pub mod sorted;
pub mod text;
pub mod tree;

pub use bytes::{same_content, same_file_bytes, same_file_bytes_within};
pub use changes::{BindingHints, ChangeWalk, Pair, Plan, MAXIMUM_CHANGED};
pub use memo::{Memo, MEMO_PAGES};
pub use named::{name_from_hints, NamingWork};
pub use sorted::DifferenceWork;
pub use text::{
    as_text, diff_exact, line_diff, lines, match_lines, DiffLine, LineOp, MAXIMUM_DIFF_CELLS,
    MAXIMUM_TEXT_BYTES,
};
pub use tree::{
    binding_changes, inode_changes, inode_differences, list_directory, locate, new_bindings,
    root_changes, same_value, subtree, ChangeKind, EntryFacts, PathChange, RootChanges, TreeTable,
};
