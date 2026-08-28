use layerfs_core::content::extent::{ChildDescriptorV3, ExtentNodeV3, ExtentSliceV3, FileStateV3};
use layerfs_core::content::extent_codec::{
    decode_file_state, decode_node_with_context, encode_file_state, encode_node, profile_id,
};
use layerfs_core::content::rope::FileStateRoot;
use layerfs_core::content::rope::{
    build, diff_ranges, read_all, read_range, replace, validate_file, visit_extents, ObjectRead,
    ObjectStore,
};
use layerfs_core::{decode_bytes_object, encode_bytes_object};
use layerfs_core::{CoreError, CoreResult, ObjectId};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct MemoryStore(BTreeMap<ObjectId, Vec<u8>>);

impl ObjectStore for MemoryStore {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.0.get(&id).cloned().ok_or(CoreError::MissingObject)
    }

    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(canonical);
        if let Some(incumbent) = self.0.get(&id) {
            if incumbent != canonical {
                return Err(CoreError::IdentityMismatch);
            }
        } else {
            self.0.insert(id, canonical.to_vec());
        }
        Ok(id)
    }
}
