//! Filesystem trees: checked logical inputs, sorted COW and bounded reads.
//!
//! Entry module: declarations and re-exports only.

pub mod attributes;
pub mod directory;
pub mod identity;
pub mod inode;
pub mod input;
pub mod limits;
pub mod objects;
pub mod path;
pub mod read;
pub mod references;
pub mod root;
pub mod sorted;
pub mod symlink;
pub mod update;
pub mod validate;

pub use identity::{InodeIdentity, InodeScope};
pub use input::{DirectoryUpdate, FilesystemInput, FilesystemResources, InodeUpdate};
pub use objects::{FilesystemObjects, FilesystemPhases, ObjectWork};
pub use path::{LogicalPath, PathName};
pub use read::{DirectoryListing, FilesystemRead, FilesystemReadWork, Resolved, Stat};
pub use root::{profile_id, scope_for_seed, FilesystemRoot, FilesystemRootId};
pub use sorted::{DirectoryRoot, SortedWork, MAXIMUM_SCRATCH_BYTES};
pub use symlink::SymlinkTarget;
pub use update::{
    build_filesystem, build_filesystem_timed, update_filesystem, update_filesystem_timed,
    FilesystemResult, FilesystemUpdateCounters,
};
pub use validate::{check, CheckedInput, FilesystemTopology};
