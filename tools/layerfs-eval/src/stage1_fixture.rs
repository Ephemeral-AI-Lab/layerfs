//! Frozen Stage One fixture preparation, custody, and evidence owners.

mod apfs;
mod attempt;
mod cdc;
mod contract;
mod error;
mod location;
mod master;
mod oracle;
mod preparation;
mod preparation_receipt;
mod selector;
mod tree;

pub use apfs::assert_apfs;
pub(crate) use apfs::clone_directory;
pub use contract::{
    Attempt, BaseManifest, CloneReceipt, EvalResult, Master, BUFFER_BYTES, FILE_BYTES, FILE_PATH,
    RANDOM_RANGE_BYTES,
};
pub use location::{fixture_root, input_path, workspace_root};
pub use master::{read_master, verify_master};
pub use oracle::{edit_bytes, expected_bytes, fill_retained_buffer, hash_file, stream_expected};
pub use preparation::{prepare_single_file, regular_file_ceiling_preflight};
pub(crate) use tree::{make_writable, seal_tree, sync_directory, verify_sealed};
pub use tree::{tree_digest, verify_user_file_ceiling};
