use crate::driver::*;
use crate::{topology_edge_key, NativeRoute, OperationCounters, VfsError, VfsResult};
use layerfs_core::content::rope::{build, read_all, FileStateRoot, ObjectRead, ObjectStore};
use layerfs_core::inode::{
    generated_inode_table_from_root, generated_inode_table_upsert, inode_table_lookup,
    GeneratedInodeTable, InodeId, InodeKind, InodeRecordV1, InodeTableCounters, InodeTableRoot,
};
use layerfs_core::metadata::{
    decode_apple_acl, encode_bsd_flags, MetadataEntryV1, MetadataKey, MetadataTreeBuilder,
    PortableMetadataV1,
};
use layerfs_core::namespace::{
    directory_insert, empty_directory, visit_directory_entries, DirectoryStateRoot,
    NamespaceCounters, NamespaceRootV1,
};
use layerfs_core::namespace_codec::{
    decode_inode_record, decode_namespace_root, encode_inode_record, encode_namespace_root,
    encode_symlink, profile_id,
};
use layerfs_workspace::{DiskNamespace, DiskTable, WorkingCandidateWrite, WorkingStore};
use std::collections::HashMap;
use std::io::{Cursor, Seek, SeekFrom};
use std::sync::Mutex;

mod directory;
mod entry;
mod existing;
mod metadata;
mod regular;
mod state;
mod workspace;

pub(crate) use existing::live_hard_link_authority_working;
pub(crate) use metadata::{put_metadata_observed, spooled_metadata_len};
pub(crate) use state::{CaptureStore, SemanticDigestCache};
pub(crate) use workspace::capture_workspace_candidate;

use directory::capture_directory;
use entry::{decode_entry, encode_entry};
use existing::{
    child_path, existing_inode, existing_record, put_record, seed_existing_hard_links,
    seed_existing_paths,
};
use regular::capture_regular;
use state::HardLink;
use workspace::working_error;
