//! Filesystem trees: checked logical inputs, sorted COW and bounded reads.
//!
//! Entry module: declarations and re-exports only.

pub mod construction;
pub mod limits;
pub mod name;
pub mod objects;
pub mod read;
pub mod root;
pub mod topology;
pub mod tree;

pub use construction::input::resident::{
    DirectoryUpdate, FilesystemInput, FilesystemResources, InodeUpdate, StreamedFilesystemInput,
};
pub use construction::input::{
    check_input, DirectoryRowSource, InodeRowSource, PreparedRows, SerialRowSource,
};
pub use construction::input::{
    check_streamed_input, DirectoryChangeLookup, DirectoryChangeSource, DirectoryHeader,
    DirectoryHeaderSource, PreparedDirectoryStreams, StreamedRowSource,
};
pub use construction::operation::{
    build_filesystem, build_filesystem_streamed, build_filesystem_streamed_timed,
    build_filesystem_timed, update_filesystem, update_filesystem_streamed,
    update_filesystem_streamed_timed, update_filesystem_timed, FilesystemResult,
    FilesystemUpdateCounters,
};
pub use construction::operation::{
    build_filesystem_streamed_backed, build_filesystem_streamed_backed_timed,
    update_filesystem_streamed_backed, update_filesystem_streamed_backed_timed,
};
pub use construction::validate::checked::{check, CheckedInput, FilesystemTopology};
pub use name::identity::{InodeIdentity, InodeScope};
pub use name::path::{LogicalPath, PathName};
pub use name::symlink::SymlinkTarget;
pub use objects::{FilesystemObjects, FilesystemPhases, ObjectWork};
pub use read::{DirectoryListing, FilesystemRead, FilesystemReadWork, Resolved, Stat};
pub use root::{profile_id, scope_for_seed, FilesystemRoot, FilesystemRootId};
pub use topology::{qualify_root, QualificationWork, QualifiedRoot, RootContext};
pub use tree::pages::{DirectoryRoot, SortedWork, MAXIMUM_SCRATCH_BYTES};

// #828 compat: old public module paths, removed in wave 3.
#[doc(hidden)]
pub use crate::filesystem::construction::captured_bindings as binding_changes; // #828 compat
#[doc(hidden)]
pub use crate::filesystem::construction::input as rows; // #828 compat
#[doc(hidden)]
pub use crate::filesystem::construction::input::resident as input; // #828 compat
#[doc(hidden)]
pub use crate::filesystem::construction::operation as update; // #828 compat
#[doc(hidden)]
pub use crate::filesystem::construction::reference as references; // #828 compat
#[doc(hidden)]
pub use crate::filesystem::construction::statistics; // #828 compat
#[doc(hidden)]
pub use crate::filesystem::construction::validate::checked as validate;
#[doc(hidden)]
pub use crate::filesystem::name::identity; // #828 compat
#[doc(hidden)]
pub use crate::filesystem::name::path; // #828 compat
#[doc(hidden)]
pub use crate::filesystem::name::symlink; // #828 compat
#[doc(hidden)]
pub use crate::filesystem::topology as qualify; // #828 compat
#[doc(hidden)]
pub use crate::filesystem::tree::attribute as attributes; // #828 compat
#[doc(hidden)]
pub use crate::filesystem::tree::directory; // #828 compat
#[doc(hidden)]
pub use crate::filesystem::tree::inode; // #828 compat
#[doc(hidden)]
pub use crate::filesystem::tree::pages as sorted; // #828 compat
#[doc(hidden)]
pub use crate::filesystem::tree::pages::decoded; // #828 compat // #828 compat
