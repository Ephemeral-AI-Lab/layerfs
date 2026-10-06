//! Public canonical fixtures and observed provider demands for context tests.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use layerfs_content::file::mapping::{
    encode_chunk_object, encode_file_state, encode_node, profile_id, ExtentNode, ExtentSlice,
    FileState,
};
use layerfs_content::filesystem::attributes::codec::{row_bytes, LEAF_ROW_OVERHEAD};
use layerfs_content::filesystem::attributes::{
    encode_attribute_page, AttributeEntry, AttributeKey, AttributePage,
};
use layerfs_content::filesystem::{scope_for_seed, InodeScope};
use layerfs_content::{
    AuthenticatedObjects, ConstructionPolicy, ContentError, ContentResult, FinalizedObject,
    ObjectId, ObjectRole,
};
use layerfs_telemetry::timer::{Timing, TimingScope};

pub fn scope() -> InodeScope {
    scope_for_seed([0x71; 32])
}

pub fn policy() -> ConstructionPolicy {
    ConstructionPolicy::new(1_048_576, 8, 4)
}

pub fn object(role: ObjectRole, canonical: Vec<u8>) -> FinalizedObject {
    Timing::disabled("admit", |timing| {
        FinalizedObject::admit(
            ObjectId::for_bytes(&canonical),
            role,
            canonical,
            policy(),
            scope(),
            timing.child("object"),
        )
    })
    .0
    .expect("local admission")
}

pub fn validate(object: &FinalizedObject, reader: &dyn AuthenticatedObjects) -> ContentResult<()> {
    Timing::disabled("context", |timing| {
        object.validate_context(reader, policy(), scope(), 1, timing.child("object"))
    })
    .0
}

#[derive(Default)]
pub struct Store {
    values: BTreeMap<ObjectId, Vec<u8>>,
    objects: Vec<FinalizedObject>,
    pub demands: RefCell<Vec<Vec<ObjectId>>>,
    pub scoped: Cell<usize>,
    pub failure: Option<ContentError>,
    pub returned: Option<usize>,
}

impl Store {
    pub fn put(&mut self, role: ObjectRole, canonical: Vec<u8>) -> FinalizedObject {
        let object = object(role, canonical);
        self.values.insert(object.id(), object.canonical().to_vec());
        self.objects.push(object.clone());
        object
    }

    pub fn objects(&self) -> &[FinalizedObject] {
        &self.objects
    }

    pub fn clear_demands(&self) {
        self.demands.borrow_mut().clear();
        self.scoped.set(0);
    }
}

impl AuthenticatedObjects for Store {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.demands.borrow_mut().push(ids.to_vec());
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let values = ids
            .iter()
            .map(|id| {
                self.values
                    .get(id)
                    .cloned()
                    .ok_or(ContentError::MissingObject)
            })
            .collect::<ContentResult<Vec<_>>>()?;
        if let Some(returned) = self.returned {
            return Ok((0..returned).map(|_| values[0].clone()).collect());
        }
        Ok(values)
    }

    fn read_canonical_batch_scoped(
        &self,
        ids: &[ObjectId],
        timing: TimingScope<'_>,
    ) -> ContentResult<Vec<Vec<u8>>> {
        self.scoped.set(self.scoped.get() + 1);
        timing.run(|_| self.read_canonical_batch(ids))
    }
}

pub fn extent_value(store: &mut Store, bytes: &[u8]) -> FinalizedObject {
    let chunk = store.put(ObjectRole::Chunk, encode_chunk_object(bytes).unwrap());
    let leaf = store.put(
        ObjectRole::ExtentLeaf,
        encode_node(
            &ExtentNode::Leaf {
                subtree_logical_bytes: bytes.len() as u64,
                extents: vec![ExtentSlice::new(chunk.id(), 0, bytes.len() as u32).unwrap()],
            },
            true,
        )
        .unwrap(),
    );
    store.put(
        ObjectRole::FileState,
        encode_file_state(FileState {
            logical_len: bytes.len() as u64,
            extent_count: 1,
            tree_level: 0,
            profile_id: profile_id(),
            mapping_root: leaf.id(),
        })
        .unwrap(),
    )
}

pub fn metadata(store: &mut Store, mode: u32) -> FinalizedObject {
    let mode = extent_value(store, &mode.to_be_bytes());
    let mtime = extent_value(store, &[0; 12]);
    let entries = vec![
        AttributeEntry {
            key: AttributeKey::new("portable".into(), b"mode".to_vec()).unwrap(),
            value_root: mode.id(),
        },
        AttributeEntry {
            key: AttributeKey::new("portable".into(), b"mtime".to_vec()).unwrap(),
            value_root: mtime.id(),
        },
    ];
    store.put(
        ObjectRole::AttributeLeaf,
        encode_attribute_page(&AttributePage::Leaf {
            subtree_bytes: entries
                .iter()
                .map(|entry| row_bytes(&entry.key, LEAF_ROW_OVERHEAD) as u64)
                .sum(),
            entries,
        })
        .unwrap(),
    )
}
