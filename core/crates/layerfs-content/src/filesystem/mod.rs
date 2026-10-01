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
pub mod rows;
pub mod sorted;
pub mod state;
pub mod symlink;
pub mod update;
pub mod validate;

pub use identity::{InodeIdentity, InodeScope};
pub use input::{DirectoryUpdate, FilesystemInput, FilesystemResources, InodeUpdate};
pub use objects::{FilesystemObjects, FilesystemPhases, ObjectWork};
pub use path::{LogicalPath, PathName};
pub use read::{DirectoryListing, FilesystemRead, FilesystemReadWork, Resolved, Stat};
pub use root::{profile_id, scope_for_seed, FilesystemRoot, FilesystemRootId};
pub use rows::{check_input, DirectoryRowSource, InodeRowSource, PreparedRows, SerialRowSource};
pub use sorted::{DirectoryRoot, SortedWork, MAXIMUM_SCRATCH_BYTES};
pub use symlink::SymlinkTarget;
pub use update::{
    build_filesystem, build_filesystem_binding_rows_with_alias_graph_state,
    build_filesystem_binding_rows_with_canonical_state,
    build_filesystem_binding_rows_with_construction_state,
    build_filesystem_binding_rows_with_graph_state,
    build_filesystem_binding_rows_with_namespace_state,
    build_filesystem_binding_rows_with_site_state, build_filesystem_binding_rows_with_state,
    build_filesystem_timed, build_filesystem_with_state, build_filesystem_with_state_timed,
    update_filesystem, update_filesystem_binding_rows_with_alias_graph_state,
    update_filesystem_binding_rows_with_canonical_state,
    update_filesystem_binding_rows_with_construction_state,
    update_filesystem_binding_rows_with_graph_state,
    update_filesystem_binding_rows_with_namespace_state,
    update_filesystem_binding_rows_with_site_state, update_filesystem_binding_rows_with_state,
    update_filesystem_timed, update_filesystem_with_state, update_filesystem_with_state_timed,
    FilesystemResult, FilesystemUpdateCounters,
};
pub use validate::{
    check, check_bindings, check_with_claims, check_with_graph, check_with_sites,
    CheckedBindingInput, CheckedInput, CheckedTopologyInput, FilesystemTopology,
};
