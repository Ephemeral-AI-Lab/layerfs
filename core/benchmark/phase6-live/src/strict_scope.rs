//! Producer/caller bindings, including C1 inode-value reference facts.
use crate::{strict_catalog::LogicalUse, strict_writer::Writer};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole,
};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

pub struct MetadataConsumer {
    pub owner: Rc<RefCell<Writer>>,
}
pub struct FileConsumer {
    pub owner: Rc<RefCell<Writer>>,
}
pub struct MetadataReader {
    pub owner: Rc<RefCell<Writer>>,
}
pub struct FileReader {
    pub owner: Rc<RefCell<Writer>>,
}
fn failure(error: String) -> ContentError {
    eprintln!("strict scoped boundary: {error}");
    ContentError::Io
}

impl FinalizedConsumer for MetadataConsumer {
    fn accept(&mut self, mut object: FinalizedObject) -> ContentResult<()> {
        if object.role() == ObjectRole::InodeLeaf {
            // Canonical bytes/IDs stay untouched. C1's compact leaf sidecar omits
            // these encoded value roots, so the filesystem caller binds them here.
            let leaf = layerfs_content::inode_leaf::InodeLeaf::decode(object.canonical())?;
            let mut references: BTreeSet<_> = object.references().iter().copied().collect();
            for row in leaf.rows {
                let value = layerfs_content::inode_leaf::decode_inode_value(&row.value)?;
                references.insert(value.content_root);
                references.insert(value.metadata_root);
            }
            object = object.with_references(references.into_iter().collect());
        }
        self.owner
            .borrow_mut()
            .offer(LogicalUse::MetadataGraph, object)
            .map_err(failure)
    }
}
impl FinalizedConsumer for FileConsumer {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        if !matches!(
            object.role(),
            ObjectRole::WholeFile
                | ObjectRole::Chunk
                | ObjectRole::ExtentLeaf
                | ObjectRole::ExtentBranch
                | ObjectRole::FileState
        ) {
            return Err(ContentError::InvalidRecord("regular-file producer grammar"));
        }
        self.owner
            .borrow_mut()
            .offer(LogicalUse::RegularFileGraph, object)
            .map_err(failure)
    }
}
impl AuthenticatedObjects for MetadataReader {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.owner
            .borrow_mut()
            .read(LogicalUse::MetadataGraph, ids)
            .map_err(failure)
    }
}
impl AuthenticatedObjects for FileReader {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.owner
            .borrow_mut()
            .read(LogicalUse::RegularFileGraph, ids)
            .map_err(failure)
    }
}
