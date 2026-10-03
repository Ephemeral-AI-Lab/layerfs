//! Real-engine parity with the same finalized inputs as the retained old path.
#[path = "support/old_snapshot.rs"]
mod old_snapshot;
mod support;
#[path = "../../layerfs-storage/tests/support/mod.rs"]
mod vectors;
use layerfs_content::inode_leaf::{
    encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, LEAF_ROW_BYTES,
};
use layerfs_content::{AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole};
use layerfs_metadata::{PgConfig, PgMetadata};
use layerfs_s3::{S3Config, S3Objects};
use layerfs_storage::{
    port::{ObjectKey, ObjectStore},
    Storage, StoragePolicy, WriteOutcome,
};
use std::sync::Arc;
struct Services {
    _fixture: support::Fixture,
    config: PgConfig,
    metadata: Arc<PgMetadata>,
    objects: Arc<S3Objects>,
}
impl Services {
    fn new(policy: StoragePolicy) -> Self {
        let fixture = support::Fixture::new(1);
        let config = fixture.service_config();
        let metadata = Arc::new(PgMetadata::create(config.clone(), policy).unwrap());
        let mut s3 = S3Config::from_env().unwrap();
        s3.prefix = format!(
            "{}/parity-{}",
            s3.prefix,
            fixture.path.file_name().unwrap().to_string_lossy()
        );
        let objects = Arc::new(S3Objects::connect(s3).unwrap());
        Self {
            _fixture: fixture,
            config,
            metadata,
            objects,
        }
    }
    fn open(&self) -> Storage {
        Storage::new(self.metadata.clone(), self.objects.clone()).unwrap()
    }
    fn control(&self) -> postgres::Client {
        let mut c = postgres::Config::new()
            .host(&self.config.host)
            .port(self.config.port)
            .user(&self.config.user)
            .password(&self.config.password)
            .dbname(&self.config.database)
            .connect(postgres::NoTls)
            .unwrap();
        c.batch_execute(&format!("SET search_path TO \"{}\"", self.config.schema))
            .unwrap();
        c
    }
    fn packs(&self) -> Vec<Vec<u8>> {
        let mut control = self.control();
        let mut packs = Vec::new();
        for row in control
            .query("SELECT digest,body FROM pack ORDER BY pack_id", &[])
            .unwrap()
        {
            let digest: Vec<u8> = row.get(0);
            let body: Option<Vec<u8>> = row.get(1);
            let body = body.unwrap_or_else(|| {
                let mut out = Vec::new();
                self.objects
                    .read(
                        ObjectKey::from_bytes(digest.try_into().unwrap()),
                        None,
                        &mut out,
                    )
                    .unwrap();
                out
            });
            packs.push(body);
        }
        packs.sort();
        packs
    }
    fn diagnostic(&self, label: &str, storage: &Storage, outcome: WriteOutcome) {
        println!(
            "DIAGNOSTIC parity-{label} outcome={outcome:?} storage={:?} pg={:?} s3={:?}",
            storage.diagnostics(),
            self.metadata.diagnostics().unwrap(),
            self.objects.diagnostics().unwrap()
        );
    }
}
fn save(storage: &Storage, objects: &[FinalizedObject]) -> WriteOutcome {
    let save = storage.begin_save().unwrap();
    for object in objects {
        save.accept(object.clone()).unwrap();
    }
    save.finish().unwrap()
}
fn old_packs(path: &std::path::Path) -> Vec<Vec<u8>> {
    let (batch, payloads) = old_snapshot::snapshot(path);
    let mut packs = batch
        .packs
        .into_iter()
        .filter_map(|p| p.body)
        .chain(payloads.into_iter().map(|(_, b)| b))
        .collect::<Vec<_>>();
    packs.sort();
    packs
}
fn whole(raw: &[u8]) -> FinalizedObject {
    FinalizedObject::new(ObjectRole::WholeFile, vectors::assembled_small_object(raw)).unwrap()
}
fn value(seed: u64) -> [u8; 73] {
    encode_inode_value(InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: ObjectId::for_bytes(&seed.to_be_bytes()),
        metadata_root: ObjectId::for_bytes(&seed.to_le_bytes()),
    })
}
fn leaf(values: &[[u8; 73]]) -> FinalizedObject {
    let rows = values
        .iter()
        .enumerate()
        .map(|(i, v)| InodeLeafRow {
            serial: 1 + i as u64,
            value: *v,
        })
        .collect::<Vec<_>>();
    FinalizedObject::new(
        ObjectRole::InodeLeaf,
        InodeLeaf {
            subtree_bytes: rows.len() as u64 * LEAF_ROW_BYTES as u64,
            rows,
        }
        .encode()
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn identical_reuse_prefix_and_cross_pack_equal_the_old_bytes() {
    let policy = StoragePolicy::new(layerfs_storage::policy::FORMAT_PROFILE, 131072, 2, 2)
        .validated()
        .unwrap();
    let services = Services::new(policy);
    let storage = services.open();
    let temp = vectors::TempDir::new("services-whole-parity");
    let path = temp.store_path("old");
    let old = vectors::disabled(|scope| {
        layerfs_storage::Store::create(&path, policy, scope.child("create"))
    })
    .unwrap();
    let mut raw = vectors::noise(90_000);
    let mut previous = None;
    let mut full = 0;
    let mut prefix = 0;
    for version in 0..8 {
        raw[version] ^= 0x55;
        let mut object = whole(&raw);
        if let Some(id) = previous {
            object = object.with_predecessors(AdvisoryPredecessors::explicit(id).unwrap());
        }
        let baseline = vectors::save_one(&old, object.clone()).unwrap();
        let candidate = save(&storage, std::slice::from_ref(&object));
        assert_eq!(candidate.prefix_records, baseline.prefix_records);
        assert_eq!(candidate.full_records, baseline.full_records);
        full += candidate.full_records;
        prefix += candidate.prefix_records;
        let reopened = services.open();
        assert_eq!(
            reopened
                .reader()
                .unwrap()
                .read_objects(&[object.id()])
                .unwrap(),
            vec![object.canonical().to_vec()]
        );
        services.diagnostic(&format!("whole-version-{version}"), &storage, candidate);
        assert_eq!(services.packs(), old_packs(&path));
        let puts = services.objects.diagnostics().unwrap().puts;
        let reuse = save(&storage, std::slice::from_ref(&object));
        assert_eq!(reuse.reused, 1);
        assert_eq!(reuse.inserted, 0);
        assert_eq!(services.objects.diagnostics().unwrap().puts, puts);
        services.diagnostic(&format!("reuse-version-{version}"), &storage, reuse);
        previous = Some(object.id());
    }
    assert!(prefix > 0);
    assert!(full >= 1);
    let mut control = services.control();
    let distinct: i64 = control
        .query_one("SELECT count(DISTINCT pack_id) FROM object", &[])
        .unwrap()
        .get(0);
    assert!(distinct > 1);
}
#[test]
fn pooled_values_prefix_and_depth_bound_full_equal_the_old_bytes() {
    let policy = StoragePolicy::frozen_default().with_metadata_depth(2);
    let services = Services::new(policy);
    let storage = services.open();
    let temp = vectors::TempDir::new("services-pooled-parity");
    let path = temp.store_path("old");
    let old = vectors::disabled(|scope| {
        layerfs_storage::Store::create(&path, policy, scope.child("create"))
    })
    .unwrap();
    let mut values = (0..100).map(value).collect::<Vec<_>>();
    let mut previous = None;
    let mut prefix = 0;
    let mut full = 0;
    for version in 0..8 {
        values[0] = value(1000 + version);
        let mut object = leaf(&values);
        if let Some(id) = previous {
            object = object.with_predecessors(AdvisoryPredecessors::explicit(id).unwrap());
        }
        let baseline = vectors::save_one(&old, object.clone()).unwrap();
        let candidate = save(&storage, std::slice::from_ref(&object));
        assert_eq!(candidate.prefix_records, baseline.prefix_records);
        assert_eq!(candidate.full_records, baseline.full_records);
        assert_eq!(candidate.pool, baseline.pool);
        prefix += candidate.prefix_records;
        full += candidate.full_records;
        assert_eq!(
            services
                .open()
                .reader()
                .unwrap()
                .read_objects(&[object.id()])
                .unwrap(),
            vec![object.canonical().to_vec()]
        );
        services.diagnostic(&format!("pooled-version-{version}"), &storage, candidate);
        assert_eq!(services.packs(), old_packs(&path));
        previous = Some(object.id());
    }
    assert!(prefix > 0);
    assert!(full > 1);
    assert_eq!(services.objects.diagnostics().unwrap().puts, 0);
}
#[test]
fn missing_prefix_base_and_corrupt_or_missing_pooled_bodies_are_refused_once() {
    for damage in ["prefix-base", "pooled-group", "pooled-pack", "pooled-base"] {
        let services = Services::new(StoragePolicy::frozen_default());
        let storage = services.open();
        let mut control = services.control();
        let id = if damage == "prefix-base" {
            let mut raw = vectors::noise(90_000);
            let base = whole(&raw);
            save(&storage, std::slice::from_ref(&base));
            raw[0] ^= 0x55;
            let next =
                whole(&raw).with_predecessors(AdvisoryPredecessors::explicit(base.id()).unwrap());
            assert_eq!(
                save(&storage, std::slice::from_ref(&next)).prefix_records,
                1
            );
            control
                .execute(
                    "DELETE FROM content_signature WHERE object_id=$1",
                    &[&base.id().as_bytes().as_slice()],
                )
                .unwrap();
            control
                .execute(
                    "DELETE FROM object WHERE object_id=$1",
                    &[&base.id().as_bytes().as_slice()],
                )
                .unwrap();
            next.id()
        } else {
            let object = leaf(&[value(4)]);
            save(&storage, std::slice::from_ref(&object));
            if damage == "pooled-group" {
                control
                    .execute("DELETE FROM metadata_value_group", &[])
                    .unwrap();
            } else if damage == "pooled-base" {
                control.execute("UPDATE pack SET body=set_byte(body,octet_length(body)-1,get_byte(body,octet_length(body)-1)#1) WHERE pack_id IN (SELECT pack_id FROM metadata_value_group)",&[]).unwrap();
            } else {
                control.execute("UPDATE pack SET body=set_byte(body,octet_length(body)-1,get_byte(body,octet_length(body)-1)#1) WHERE domain=0",&[]).unwrap();
            }
            object.id()
        };
        let reopened = services.open();
        let before = services.metadata.diagnostics().unwrap();
        let error = reopened.reader().unwrap().read_objects(&[id]).unwrap_err();
        assert!(matches!(
            error,
            layerfs_storage::StorageError::ObjectMissing(_)
                | layerfs_storage::StorageError::Integrity(_)
        ));
        let after = services.metadata.diagnostics().unwrap();
        println!("DIAGNOSTIC parity-damage-{damage} error={error:?} metadata_operation_delta={} storage={:?} s3={:?}",after.operations-before.operations,reopened.diagnostics(),services.objects.diagnostics().unwrap());
        assert!(reopened.diagnostics().value_groups <= 1);
        // Body acquisition is diagnostic; catalogue misses are not retried.
        assert_eq!(
            reopened.diagnostics().value_groups,
            u64::from(damage != "prefix-base" && damage != "pooled-pack")
        );
    }
}

#[test]
fn explicit_chunk_predecessors_reach_depth_bound_full_with_exact_old_bytes() {
    let policy = StoragePolicy::new(layerfs_storage::policy::FORMAT_PROFILE, 131072, 2, 2)
        .validated()
        .unwrap();
    let services = Services::new(policy);
    let storage = services.open();
    let temp = vectors::TempDir::new("services-chunk-depth");
    let path = temp.store_path("old");
    let old = vectors::disabled(|scope| {
        layerfs_storage::Store::create(&path, policy, scope.child("create"))
    })
    .unwrap();
    let mut raw = vectors::noise(layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES);
    let mut previous = None;
    let mut full = 0;
    let mut prefix = 0;
    for version in 0..8 {
        raw[version] ^= 0x55;
        let mut object = FinalizedObject::new(
            ObjectRole::Chunk,
            layerfs_content::file::mapping::encode_chunk_object(&raw).unwrap(),
        )
        .unwrap();
        if let Some(id) = previous {
            object = object.with_predecessors(AdvisoryPredecessors::explicit(id).unwrap());
        }
        let baseline = vectors::save_one(&old, object.clone()).unwrap();
        let candidate = save(&storage, std::slice::from_ref(&object));
        assert_eq!(candidate.prefix_records, baseline.prefix_records);
        assert_eq!(candidate.full_records, baseline.full_records);
        full += candidate.full_records;
        prefix += candidate.prefix_records;
        assert_eq!(
            services
                .open()
                .reader()
                .unwrap()
                .read_objects(&[object.id()])
                .unwrap(),
            vec![object.canonical().to_vec()]
        );
        services.diagnostic(&format!("chunk-version-{version}"), &storage, candidate);
        assert_eq!(services.packs(), old_packs(&path));
        previous = Some(object.id());
    }
    assert!(prefix > 0);
    assert!(full > 1);
}
