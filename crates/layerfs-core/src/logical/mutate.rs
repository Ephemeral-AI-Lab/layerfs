use super::resolver::{namespace, resolve, resolve_parent, LogicalCounters};
use crate::content::rope::{build, replace, FileStateRoot};
use crate::inode::{
    inode_table_lookup, inode_table_remove, inode_table_upsert, InodeId, InodeKind, InodeRecordV1,
    InodeTableRoot,
};
use crate::namespace::SymlinkStateV1;
use crate::namespace::{
    directory_insert, directory_lookup, directory_page_after, directory_remove, directory_rename,
    empty_directory, DirectoryStateRoot, NamespaceRootV1,
};
use crate::namespace_codec::{
    decode_inode_record, encode_inode_record, encode_namespace_root, encode_symlink,
};
use crate::object::access::ObjectStore;
use crate::{CanonicalPath, CoreError, CoreResult, ObjectId};
use std::io::Read;

include!("mutate/model.rs");
include!("mutate/file.rs");
include!("mutate/rename.rs");
include!("mutate/batch.rs");
include!("mutate/create.rs");
include!("mutate/accounting.rs");
