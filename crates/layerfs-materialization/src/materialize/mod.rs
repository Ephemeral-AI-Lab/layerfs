use crate::capture::{
    capture_workspace_candidate, live_hard_link_authority_working, SemanticDigestCache,
};
use crate::driver::*;
use crate::{topology_edge_key, NativeRoute, OperationCounters, VfsError, VfsResult};
use layerfs_core::content::rope::{read_all, read_all_bounded, FileStateRoot, ObjectRead};
use layerfs_core::inode::{
    inode_table_lookup, InodeId, InodeKind, InodeTableCounters, InodeTableRoot,
};
use layerfs_core::metadata::{decode_apple_acl, visit_metadata_entries, SUPPORTED_BSD_FLAGS};
use layerfs_core::namespace::{visit_directory_entries, DirectoryStateRoot, NamespaceCounters};
use layerfs_core::namespace_codec::{decode_inode_record, decode_namespace_root, decode_symlink};
use layerfs_core::ObjectId;
use layerfs_workspace::{DiskNamespace, DiskTable, WorkingStore};
use std::io::{BufWriter, Write};

mod arithmetic;
mod directory;
mod entry;
mod hard_link;
mod metadata;
mod record;
mod regular;
mod traversal;
mod workspace;

pub(crate) use metadata::metadata;
pub(crate) use workspace::materialize_workspace_working;

use arithmetic::checked_add;
use directory::materialize_directory;
use entry::materialize_entry;
use hard_link::{
    child_path, create_hard_link_from_path, decode_link_state, encode_link_state,
    finish_hard_link_from_path,
};
use record::record;
use regular::project_regular_file;
use traversal::{visit_materialization_source, MaterializeTarget};
