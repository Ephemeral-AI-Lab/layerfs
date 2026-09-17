//! Inode table pages: typed values, branches and shared reads.

pub mod codec;
pub mod read;
pub mod update;

pub use codec::{decode_inode_page, encode_inode_page, InodePage};
pub use read::{lookup, lookup_many, InodeReadWork, InodeTable};
pub use update::{apply_changes, apply_inode_values, build_table, InodeChange};
