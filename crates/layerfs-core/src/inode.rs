use crate::content::rope::{ObjectRead, ObjectStore};
use crate::metadata::merge_metadata_roots;
use crate::namespace::{merge_directory_roots, DirectoryStateRoot, NamespaceCounters};
use crate::namespace_codec::{
    decode_inode_record, decode_inode_table_node, encode_inode_record, encode_inode_table_node,
    InodeTableNodeV1,
};
use crate::CoreError;
use crate::{CoreResult, ObjectId};
use std::collections::{BTreeMap, BTreeSet};

include!("inode/model.rs");
include!("inode/read.rs");
include!("inode/merge.rs");
include!("inode/diff.rs");
include!("inode/mutation.rs");
include!("inode/rebalance.rs");
include!("inode/diff_cursor.rs");
