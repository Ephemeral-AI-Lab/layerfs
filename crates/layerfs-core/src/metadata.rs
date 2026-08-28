use crate::content::rope::{ObjectRead, ObjectStore};
use crate::inode::InodeKind;
use crate::namespace_codec::{decode_metadata_node, encode_metadata_node, MetadataNodeV1};
use crate::{CoreError, CoreResult, ObjectId};

include!("metadata/model.rs");
include!("metadata/build.rs");
include!("metadata/read.rs");
include!("metadata/merge.rs");
