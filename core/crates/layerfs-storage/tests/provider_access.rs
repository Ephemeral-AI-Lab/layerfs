//! A real SQLite body source behind operation-specific eligibility.
mod support;

use std::{cell::Cell, collections::BTreeMap};

use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::{
    access::{ObjectLocation, ValueGroupRow},
    encoding::{
        delta::{
            read::{BodyCaches, ChainBases, ChainCounters, Resolver},
            select::DepthCache,
        },
        pool::PoolReader,
        DecompressionWorkspace, GroupCache,
    },
    PackAccess, StorageError, StorageResult,
};
use support::{assembled_small_object, create_store, save_one, TempDir};

struct ScopedAccess {
    db: rusqlite::Connection,
    eligible: Cell<bool>,
}

impl PackAccess for ScopedAccess {
    fn location(&self, id: ObjectId, ceiling: i64) -> StorageResult<Option<ObjectLocation>> {
        if self.eligible.get() {
            self.db.location(id, ceiling)
        } else {
            Ok(None)
        }
    }
    fn pack_bytes(&self, id: i64) -> StorageResult<Vec<u8>> {
        self.db.pack_bytes(id)
    }
    fn metadata_window_start(&self) -> StorageResult<u32> {
        self.db.metadata_window_start()
    }
    fn group_page(&self, from: u32, limit: usize) -> StorageResult<Vec<ValueGroupRow>> {
        self.db.group_page(from, limit)
    }
    fn group_for(&self, ordinal: u32) -> StorageResult<Option<ValueGroupRow>> {
        self.db.group_for(ordinal)
    }
}

fn fixture() -> (
    TempDir,
    ScopedAccess,
    ObjectLocation,
    Vec<u8>,
    layerfs_storage::StorageCapacities,
) {
    let dir = TempDir::new("neutral-access");
    let path = dir.store_path("source");
    let store = create_store(&path);
    let canonical = assembled_small_object(b"the real persisted provider bytes");
    let object = FinalizedObject::new(ObjectRole::WholeFile, canonical.clone()).unwrap();
    let id = object.id();
    save_one(&store, object).unwrap();
    let db = layerfs_storage::sqlite::connection::open(&path, false).unwrap();
    let root = db.location(id, i64::MAX).unwrap().unwrap();
    (
        dir,
        ScopedAccess {
            db,
            eligible: Cell::new(true),
        },
        root,
        canonical,
        store.capacities(),
    )
}

#[test]
fn cached_body_does_not_authorize_a_missing_placement() {
    let (_dir, access, root, expected, capacities) = fixture();
    let mut packs = BTreeMap::new();
    let mut pool = PoolReader::new();
    let mut groups = GroupCache::new();
    let mut workspace = DecompressionWorkspace::new().unwrap();
    let mut counters = ChainCounters::default();
    {
        let mut resolver = Resolver::new(
            &access,
            i64::MAX,
            &capacities,
            BodyCaches {
                packs: &mut packs,
                pool: &mut pool,
            },
            &mut groups,
            &mut workspace,
            &mut counters,
        );
        assert_eq!(resolver.resolve_at(root).unwrap().0, expected);
    }
    assert!(!packs.is_empty());
    access.eligible.set(false);
    let mut resolver = Resolver::new(
        &access,
        i64::MAX,
        &capacities,
        BodyCaches {
            packs: &mut packs,
            pool: &mut pool,
        },
        &mut groups,
        &mut workspace,
        &mut counters,
    );
    assert!(matches!(
        resolver.resolve_at(root),
        Err(StorageError::Integrity("selected locator eligibility"))
    ));
}

#[test]
fn cached_depth_requires_current_placement_eligibility() {
    let (_dir, access, root, _, _) = fixture();
    let mut depths = DepthCache::new();
    let mut packs = BTreeMap::new();
    let mut bases = ChainBases::new(&mut packs);
    let mut workspace = DecompressionWorkspace::new().unwrap();
    assert_eq!(
        depths
            .depth_of(
                &access,
                &mut workspace,
                root.object_id,
                |source, decode, location| bases.base_of(source, decode, location)
            )
            .unwrap(),
        Some(0)
    );
    assert_eq!(depths.len(), 1);
    access.eligible.set(false);
    assert_eq!(
        depths
            .depth_of(
                &access,
                &mut workspace,
                root.object_id,
                |source, decode, location| bases.base_of(source, decode, location)
            )
            .unwrap(),
        None
    );
}

#[test]
fn pooled_selection_rejects_a_mismatched_canonical_descriptor() {
    let (_dir, access, _, _, capacities) = fixture();
    let mut depths = DepthCache::new();
    let mut pool = PoolReader::new();
    let mut decode = DecompressionWorkspace::new().unwrap();
    let mut counters = layerfs_storage::cas::PoolCounters::default();
    let mut profile = layerfs_storage::cas::SaveProfile::default();
    let mut input = layerfs_storage::encoding::pool::PooledSelectInput {
        access: &access,
        ceiling: i64::MAX,
        capacities: &capacities,
        depths: &mut depths,
        reader: &mut pool,
        decode: &mut decode,
        counters: &mut counters,
        profile: &mut profile,
    };
    assert!(layerfs_storage::encoding::pool::select_pooled(&mut input, 1, &[], &[]).is_err());
    assert_eq!(counters.leaves, 0);
}
