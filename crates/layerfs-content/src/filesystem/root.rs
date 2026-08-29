use super::change::metadata;
use crate::object::access::ObjectStore;
use crate::tree::directory::codec::{encode_namespace_root, profile_id};
use crate::tree::directory::empty_directory;
use crate::tree::inode::codec::encode_inode_record;
use crate::tree::inode::{inode_table_from_root, InodeId, InodeKind, InodeRecordV1};
use crate::tree::NamespaceRootV1;
use crate::{CoreResult, ObjectId};

pub fn empty_root<S: ObjectStore>(store: &mut S, seed: [u8; 32]) -> CoreResult<ObjectId> {
    let root_inode = InodeId::allocate(seed, 0);
    let directory = empty_directory(store)?;
    let metadata = metadata(store, InodeKind::Directory, 0o755)?;
    let record = store.put(&encode_inode_record(InodeRecordV1 {
        kind: InodeKind::Directory,
        namespace_ref_count: 0,
        content_root: directory.0,
        metadata_root: metadata,
    })?)?;
    let table = inode_table_from_root(store, root_inode, record)?;
    store.put(&encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: root_inode,
        inode_table_root: table.0,
    })?)
}
