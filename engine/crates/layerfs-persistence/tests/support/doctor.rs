//! Persisted Disposable Doctor fixtures bypassing trusted Save only for corruption setup.
#![allow(dead_code)]
use super::Temp;
use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    diagnostic::{Observation, Options, Outcome},
    PersistenceConfig, SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_storage::{
    encoding::{codec::CompressionWorkspace, encode_full},
    location::{InodeValueRow, ObjectLocation, PackDomain},
    pack::{assemble, build_group, layout::PackLane},
    port::{PackPersistence, Publication, PublishedPack, Reserve},
    save::SaveProfile,
    StorageCapacities, StoragePolicy,
};
use std::sync::Arc;
pub struct Fixture {
    pub temp: Temp,
    pub handles: SqliteHandles,
    pub policy: StoragePolicy,
}
impl Fixture {
    pub fn new() -> Self {
        Self::with_policy(StoragePolicy::frozen_default())
    }
    pub fn with_policy(policy: StoragePolicy) -> Self {
        let temp = Temp::new("doctor-adversarial");
        let handles = SqliteHandles::create(
            PersistenceConfig::sqlite(temp.join("store.sqlite"))
                .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
            policy,
            &HistoryCatalogConfig {
                binding_key: b"doctor-adversarial".to_vec(),
                cursor_key: [71; 32],
                incarnation: 7,
            },
        )
        .unwrap();
        Self {
            temp,
            handles,
            policy,
        }
    }
    pub fn raw(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self.temp.join("store.sqlite")).unwrap()
    }
    pub fn audit(&self, quick: bool) -> Vec<Observation> {
        let mut out = Vec::new();
        self.handles
            .diagnose(
                Options {
                    quick,
                    ..Options::default()
                },
                &mut |o| out.push(o),
            )
            .unwrap();
        out
    }
    pub fn put(&self, role: ObjectRole, canonical: Vec<u8>) -> FinalizedObject {
        let object = FinalizedObject::new(role, canonical).unwrap();
        let record = encode_full(
            object.canonical(),
            role,
            &StorageCapacities::from_policy(self.policy).unwrap(),
            &mut CompressionWorkspace::new().unwrap(),
            &mut SaveProfile::default(),
        )
        .unwrap();
        self.record(&object, record.lane, record.record, vec![]);
        object
    }
    pub fn record(
        &self,
        object: &FinalizedObject,
        lane: PackLane,
        record: Vec<u8>,
        values: Vec<InodeValueRow>,
    ) {
        let body = assemble(lane, &[build_group(lane, &[record], None).unwrap()]).unwrap();
        self.body(object, lane, body, values);
    }
    pub fn body(
        &self,
        object: &FinalizedObject,
        lane: PackLane,
        body: Vec<u8>,
        values: Vec<InodeValueRow>,
    ) {
        let reserved = self
            .handles
            .storage
            .reserve(Reserve {
                packs: 1,
                ordinals: 0,
            })
            .unwrap();
        self.handles
            .storage
            .publish(&Publication {
                packs: vec![PublishedPack::seal(
                    reserved.first_pack_id,
                    PackDomain::for_lane(lane),
                    Arc::new(body),
                )],
                objects: vec![ObjectLocation {
                    object_id: object.id(),
                    role: object.role(),
                    canonical_length: object.canonical_len(),
                    pack_id: reserved.first_pack_id,
                    group_number: 0,
                    record_number: 0,
                }],
                inode_values: values,
                ..Publication::default()
            })
            .unwrap();
    }
    pub fn inode(&self, canonical: Vec<u8>, values: Vec<[u8; 73]>) -> ObjectId {
        let object = FinalizedObject::new(ObjectRole::InodeLeaf, canonical).unwrap();
        let reserved = self
            .handles
            .storage
            .reserve(Reserve {
                packs: 0,
                ordinals: values.len(),
            })
            .unwrap();
        let ordinals: Vec<_> =
            (reserved.first_ordinal..reserved.first_ordinal + values.len() as u32).collect();
        let pooled =
            layerfs_content::inode_leaf::pooled_body(object.canonical(), &ordinals).unwrap();
        let record = layerfs_storage::encoding::pool::leaf::encode_full(&pooled).unwrap();
        self.record(
            &object,
            PackLane::Ordinary,
            record,
            ordinals
                .into_iter()
                .zip(values)
                .map(|(ordinal, value)| InodeValueRow { ordinal, value })
                .collect(),
        );
        object.id()
    }
}
pub fn clean(out: &[Observation]) {
    assert!(
        !out.iter()
            .any(|o| o.outcome == Outcome::Fail || o.outcome == Outcome::Unknown),
        "{out:?}"
    );
}
pub fn failure(out: &[Observation], id: &str) {
    assert!(
        out.iter().any(|o| o.id == id && o.outcome == Outcome::Fail),
        "missing{id}: {out:?}"
    );
}
