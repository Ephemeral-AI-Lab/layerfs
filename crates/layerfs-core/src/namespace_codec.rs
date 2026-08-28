use crate::inode::{InodeId, InodeKind, InodeRecordV1};
use crate::metadata::{MetadataEntryV1, MetadataKey};
use crate::namespace::{DirectoryStateV1, NamespaceRootV1, SymlinkStateV1};
use crate::{
    decode_bytes_object, encode_bytes_object, CanonicalName, CoreError, CoreResult, ObjectId,
};

include!("namespace_codec/model.rs");
include!("namespace_codec/directory.rs");
include!("namespace_codec/inode.rs");
include!("namespace_codec/metadata.rs");
include!("namespace_codec/state.rs");
include!("namespace_codec/framing.rs");
