//! The same C1/codec vectors through acknowledged, reference-closed port writes.
#[path = "support/memory_engines.rs"]
mod engines;
mod support;
use engines::{MemoryMetadata, MemoryObjects};
use layerfs_content::{AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::{location::PackDomain, port::*, Storage, StorageError};
use std::sync::{Arc, Mutex};

fn open(metadata: &Arc<MemoryMetadata>, objects: &Arc<MemoryObjects>) -> Storage {
    Storage::new(metadata.clone(), objects.clone()).unwrap()
}
fn save(storage: &Storage, objects: Vec<FinalizedObject>) -> layerfs_storage::WriteOutcome {
    let operation = storage.begin_save().unwrap();
    for object in objects {
        operation.accept(object).unwrap();
    }
    operation.finish().unwrap()
}
fn whole(raw: &[u8]) -> FinalizedObject {
    FinalizedObject::new(ObjectRole::WholeFile, support::assembled_small_object(raw)).unwrap()
}
fn all_packs(metadata: &MemoryMetadata, objects: &MemoryObjects) -> Vec<Vec<u8>> {
    let state = metadata.state.lock().unwrap();
    let payloads = objects.bodies.lock().unwrap();
    let mut packs: Vec<_> = state
        .packs
        .values()
        .map(|p| {
            p.body
                .clone()
                .unwrap_or_else(|| payloads[&p.info.key].clone())
        })
        .collect();
    packs.sort();
    packs
}
fn legacy_packs(path: &std::path::Path) -> Vec<Vec<u8>> {
    let (snapshot, payloads) = engines::snapshot(path);
    let mut packs: Vec<_> = snapshot
        .packs
        .into_iter()
        .filter_map(|p| p.body)
        .chain(payloads.into_iter().map(|(_, body)| body))
        .collect();
    packs.sort();
    packs
}
#[test]
fn file_vectors_preserve_canonical_identities_and_exact_pack_bytes() {
    for raw in [
        vec![],
        vec![1],
        support::noise(96_000),
        vec![0x77; 96_000],
        support::noise(400_000),
        support::noise(2_000_000),
    ] {
        let dir = support::TempDir::new("port-file-parity");
        let path = dir.store_path("old");
        let old = support::create_store(&path);
        let (collected, root, _) = support::construct_file(&raw);
        support::save_all(&old, &collected).unwrap();
        let metadata = Arc::new(MemoryMetadata::default());
        let objects = Arc::new(MemoryObjects::default());
        let storage = open(&metadata, &objects);
        let first = save(&storage, collected.finalized());
        assert_eq!(first.inserted, collected.objects().len() as u64);
        let mut logical = Vec::new();
        support::disabled(|scope| {
            layerfs_content::read_all(
                &storage.reader().unwrap(),
                root,
                &mut logical,
                scope.child("read"),
            )
        })
        .unwrap();
        assert_eq!(logical, raw);
        assert_eq!(all_packs(&metadata, &objects), legacy_packs(&path));
        let before = storage.diagnostics();
        let reused = save(&storage, collected.finalized());
        assert_eq!(reused.inserted, 0);
        assert_eq!(reused.reused, collected.objects().len() as u64);
        assert_eq!(storage.diagnostics().puts, before.puts);
        assert_eq!(storage.diagnostics().register, before.register);
        let reopened = open(&metadata, &objects);
        assert_eq!(
            reopened.reader().unwrap().read_objects(&[root]).unwrap(),
            storage.reader().unwrap().read_objects(&[root]).unwrap()
        );
        println!(
            "diagnostics file {:?} outcome {:?}",
            storage.diagnostics(),
            first
        );
    }
}
#[test]
fn same_save_whole_file_prefix_and_reopened_chain_match_old_bytes() {
    let metadata = Arc::new(MemoryMetadata::default());
    let objects = Arc::new(MemoryObjects::default());
    let storage = open(&metadata, &objects);
    let dir = support::TempDir::new("port-chain");
    let path = dir.store_path("old");
    let old = support::create_store(&path);
    let mut raw = support::noise(90_000);
    let mut previous = None;
    for save_number in 0..3 {
        let operation = storage.begin_save().unwrap();
        let mut legacy = Vec::new();
        for version in 0..4 {
            raw[save_number * 4 + version] ^= 0x55;
            let mut object = whole(&raw);
            if let Some(id) = previous {
                object = object.with_predecessors(AdvisoryPredecessors::explicit(id).unwrap());
            }
            previous = Some(object.id());
            operation.accept(object.clone()).unwrap();
            legacy.push(object);
        }
        let outcome = operation.finish().unwrap();
        support::disabled(|scope| {
            let mut operation = old.begin_save(scope.child("begin"))?;
            for object in legacy {
                operation.accept(object)?;
            }
            operation.finish(scope.child("finish"))
        })
        .unwrap();
        assert!(outcome.prefix_records > 0);
        let expected = whole(&raw).canonical().to_vec();
        assert_eq!(
            open(&metadata, &objects)
                .reader()
                .unwrap()
                .read_objects(&[previous.unwrap()])
                .unwrap(),
            vec![expected]
        );
        assert_eq!(all_packs(&metadata, &objects), legacy_packs(&path));
    }
}
#[test]
fn grouped_records_cross_waves_with_bounded_pending_ownership() {
    let metadata = Arc::new(MemoryMetadata::default());
    let objects = Arc::new(MemoryObjects::default());
    let storage = open(&metadata, &objects);
    let dir = support::TempDir::new("port-groups");
    let path = dir.store_path("old");
    let old = support::create_store(&path);
    support::disabled(|scope| {
        let mut legacy = old.begin_save(scope.child("begin"))?;
        let operation = storage.begin_save().unwrap();
        for i in 0..1600_u32 {
            let mut raw = vec![3; 600];
            raw[..4].copy_from_slice(&i.to_be_bytes());
            let object = whole(&raw);
            legacy.accept(object.clone())?;
            operation.accept(object)?;
            assert!(
                operation.pending_canonical_bytes()
                    <= layerfs_storage::policy::WAVE_CANONICAL_BYTES_LIMIT
            );
        }
        assert_eq!(operation.finish().unwrap().inserted, 1600);
        legacy.finish(scope.child("finish"))
    })
    .unwrap();
    assert_eq!(all_packs(&metadata, &objects), legacy_packs(&path));
}
#[test]
fn same_save_reads_and_duplicates_use_the_bounded_pending_object() {
    let metadata = Arc::new(MemoryMetadata::default());
    let objects = Arc::new(MemoryObjects::default());
    let storage = open(&metadata, &objects);
    let operation = storage.begin_save().unwrap();
    let object = whole(&support::noise(200));
    operation.accept(object.clone()).unwrap();
    assert_eq!(
        operation.read_objects(&[object.id()]).unwrap(),
        vec![object.canonical().to_vec()]
    );
    operation.accept(object.clone()).unwrap();
    let result = operation.finish().unwrap();
    assert_eq!(result.inserted, 1);
    assert_eq!(result.reused, 1);
}
#[test]
fn missing_reference_never_reaches_registration() {
    let metadata = Arc::new(MemoryMetadata::default());
    let objects = Arc::new(MemoryObjects::default());
    let storage = open(&metadata, &objects);
    let operation = storage.begin_save().unwrap();
    let object = whole(b"missing").with_references(vec![ObjectId::for_bytes(b"absent")]);
    operation.accept(object).unwrap();
    assert!(matches!(
        operation.finish(),
        Err(StorageError::MissingDependency { .. })
    ));
    assert!(!metadata
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|(call, _)| *call == "register"));
    assert!(objects.calls.lock().unwrap().is_empty());
}
struct FailedObjects {
    inner: MemoryObjects,
}
impl ObjectStore for FailedObjects {
    fn put_if_absent(&self, _: ObjectKey, _: &[u8]) -> Result<Put, ObjectError> {
        Err(ObjectError::Uncertain)
    }
    fn read(
        &self,
        k: ObjectKey,
        r: Option<ByteRange>,
        out: &mut Vec<u8>,
    ) -> Result<(), ObjectError> {
        self.inner.read(k, r, out)
    }
    fn head(&self, k: ObjectKey) -> Result<Option<u64>, ObjectError> {
        self.inner.head(k)
    }
}
#[test]
fn failed_payload_cannot_publish_an_ordinary_parent() {
    let metadata = Arc::new(MemoryMetadata::default());
    let storage = Storage::new(
        metadata.clone(),
        Arc::new(FailedObjects {
            inner: MemoryObjects::default(),
        }),
    )
    .unwrap();
    let (collected, _, _) = support::construct_file(&support::noise(400_000));
    let operation = storage.begin_save().unwrap();
    for object in collected.finalized() {
        operation.accept(object).unwrap();
    }
    assert!(matches!(
        operation.finish(),
        Err(StorageError::UnknownOutcome { .. })
    ));
    assert!(metadata.state.lock().unwrap().objects.is_empty());
    assert!(!metadata
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|(call, _)| *call == "register"));
}
#[test]
fn lost_registration_reply_is_not_retried_or_cleaned_up() {
    let metadata = Arc::new(MemoryMetadata::default());
    let objects = Arc::new(MemoryObjects::default());
    metadata.state.lock().unwrap().fail_register = Some(MetadataError::Uncertain);
    let storage = open(&metadata, &objects);
    let operation = storage.begin_save().unwrap();
    operation.accept(whole(b"uncertain")).unwrap();
    assert!(matches!(
        operation.finish(),
        Err(StorageError::UnknownOutcome { .. })
    ));
    assert_eq!(
        metadata
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(call, _)| *call == "register")
            .count(),
        1
    );
    assert_eq!(
        objects
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| **call == "put")
            .count(),
        1
    );
    assert!(!objects.bodies.lock().unwrap().is_empty());
}
struct RaceMetadata {
    inner: MemoryMetadata,
    race: Mutex<Option<ObjectId>>,
    winner: Option<Registration>,
}
impl MetadataStore for RaceMetadata {
    fn policy(&self) -> Result<layerfs_storage::StoragePolicy, MetadataError> {
        self.inner.policy()
    }
    fn locate(
        &self,
        ids: &[ObjectId],
        out: &mut Vec<layerfs_storage::location::LocatedObject>,
    ) -> Result<(), MetadataError> {
        self.inner.locate(ids, out)
    }
    fn read_packs(&self, ids: &[i64], out: &mut Vec<MetadataPack>) -> Result<(), MetadataError> {
        self.inner.read_packs(ids, out)
    }
    fn value_groups(&self, q: ValueGroupQuery<'_>) -> Result<ValueGroups, MetadataError> {
        self.inner.value_groups(q)
    }
    fn signatures(
        &self,
        out: &mut Vec<layerfs_storage::location::SignatureRow>,
    ) -> Result<(), MetadataError> {
        self.inner.signatures(out)
    }
    fn reserve(&self, q: Reserve) -> Result<Reserved, MetadataError> {
        self.inner.reserve(q)
    }
    fn register(&self, batch: &Registration) -> Result<Registered, MetadataError> {
        if let Some(id) = self.race.lock().unwrap().take() {
            if let Some(winner) = &self.winner {
                self.inner.register(winner)?;
            } else {
                let mut row = *batch
                    .objects
                    .iter()
                    .find(|row| row.object_id == id)
                    .unwrap();
                let mut pack = batch
                    .packs
                    .iter()
                    .find(|pack| pack.info.pack_id == row.pack_id)
                    .unwrap()
                    .clone();
                row.pack_id = 10_000;
                pack.info.pack_id = 10_000;
                self.inner.register(&Registration {
                    packs: vec![pack],
                    objects: vec![row],
                    ..Default::default()
                })?;
            }
        }
        self.inner.register(batch)
    }
}
#[test]
fn higher_pack_first_wins_base_keeps_its_lower_pack_dependent_readable() {
    let mut raw = support::noise(90_000);
    let base = whole(&raw);
    raw[700] ^= 3;
    let dependent =
        whole(&raw).with_predecessors(AdvisoryPredecessors::explicit(base.id()).unwrap());
    let metadata = Arc::new(RaceMetadata {
        inner: MemoryMetadata::default(),
        race: Mutex::new(Some(base.id())),
        winner: None,
    });
    let storage = Storage::new(metadata.clone(), Arc::new(MemoryObjects::default())).unwrap();
    raw[900] ^= 7;
    let later =
        whole(&raw).with_predecessors(AdvisoryPredecessors::explicit(dependent.id()).unwrap());
    let result = save(&storage, vec![base.clone(), dependent.clone(), later]);
    assert_eq!(result.inserted, 2);
    assert_eq!(result.reused, 1);
    assert_eq!(result.prefix_records, 2);
    let state = metadata.inner.state.lock().unwrap();
    assert!(state.objects[&base.id()].pack_id > state.objects[&dependent.id()].pack_id);
    drop(state);
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[dependent.id()])
            .unwrap(),
        vec![dependent.canonical().to_vec()]
    );
}
#[test]
fn port_reader_rejects_a_cycle_without_relying_on_pack_order() {
    let dir = support::TempDir::new("port-cycle");
    let path = dir.store_path("old");
    let old = support::create_store(&path);
    let mut raw = support::noise(90_000);
    let first = whole(&raw);
    support::save_one(&old, first.clone()).unwrap();
    raw[0] ^= 1;
    let second = whole(&raw).with_predecessors(AdvisoryPredecessors::explicit(first.id()).unwrap());
    support::save_one(&old, second.clone()).unwrap();
    raw[1] ^= 1;
    let third = whole(&raw).with_predecessors(AdvisoryPredecessors::explicit(second.id()).unwrap());
    support::save_one(&old, third.clone()).unwrap();
    support::forge_stored_base(&path, second.id(), third.id());
    let (batch, payloads) = engines::snapshot(&path);
    let metadata = Arc::new(MemoryMetadata::default());
    let objects = Arc::new(MemoryObjects::default());
    for (key, body) in payloads {
        objects.put_if_absent(key, &body).unwrap();
    }
    metadata.register(&batch).unwrap();
    assert!(matches!(
        open(&metadata, &objects)
            .reader()
            .unwrap()
            .read_objects(&[second.id()]),
        Err(StorageError::Integrity("dependency cycle"))
    ));
}
#[test]
fn c1_edit_uses_the_save_provider_and_sink_without_a_legacy_store() {
    use support::edits::{Edits, Parts};
    let metadata = Arc::new(MemoryMetadata::default());
    let objects = Arc::new(MemoryObjects::default());
    let storage = open(&metadata, &objects);
    let raw = support::noise(280_000);
    let (collected, root, _) = support::construct_file(&raw);
    save(&storage, collected.finalized());
    let operation = storage.begin_save().unwrap();
    let mut source = Parts::new();
    source.push(vec![0x79; 300]);
    let stream = Edits::new(
        raw.len() as u64,
        vec![layerfs_content::Edit::overwrite(100_000, 100_300)],
    )
    .unwrap();
    let policy = layerfs_content::ConstructionPolicy::frozen_default();
    let edited = support::disabled(|scope| {
        layerfs_content::apply_edits(
            policy,
            &policy.capacities(),
            &operation,
            layerfs_content::EditRequest {
                root,
                edits: &stream,
                source: &source,
            },
            &mut operation.sink(),
            scope.child("edit"),
        )
    })
    .unwrap();
    operation.finish().unwrap();
    let mut actual = Vec::new();
    support::disabled(|scope| {
        layerfs_content::read_all(
            &storage.reader().unwrap(),
            edited.root,
            &mut actual,
            scope.child("read"),
        )
    })
    .unwrap();
    let mut expected = raw;
    expected[100_000..100_300].fill(0x79);
    assert_eq!(actual, expected);
    let state = metadata.state.lock().unwrap();
    assert!(state
        .packs
        .values()
        .any(|p| p.info.domain == PackDomain::Payload));
    assert!(state
        .packs
        .values()
        .any(|p| p.info.domain == PackDomain::Metadata));
}
#[test]
fn closure_forces_the_unfinished_payload_before_registering_its_parents() {
    let metadata = Arc::new(MemoryMetadata::default());
    let objects = Arc::new(MemoryObjects::default());
    let storage = open(&metadata, &objects);
    let raw = support::noise(400_000);
    let (collected, _, _) = support::construct_file(&raw);
    let child = collected
        .finalized()
        .into_iter()
        .find(|o| o.role() == ObjectRole::Chunk)
        .unwrap();
    let operation = storage.begin_save().unwrap();
    operation.accept(child.clone()).unwrap();
    for i in 0..512_u32 {
        let mut value = support::noise(1500);
        value[..4].copy_from_slice(&i.to_be_bytes());
        let canonical = layerfs_content::object::codec::encode_bytes_object(&value).unwrap();
        let parent = FinalizedObject::new(ObjectRole::DirectoryLeaf, canonical)
            .unwrap()
            .with_references(vec![child.id()]);
        operation.accept(parent).unwrap();
    }
    assert!(storage.diagnostics().forced_seals > 0);
    let snapshot = metadata.state.lock().unwrap();
    assert!(snapshot.objects.contains_key(&child.id()));
    assert!(snapshot
        .objects
        .values()
        .any(|row| row.role == ObjectRole::DirectoryLeaf));
    drop(snapshot);
    operation.finish().unwrap();
    println!("diagnostics closure {:?}", storage.diagnostics());
}
#[test]
fn singleton_keeps_the_existing_large_object_exception_and_exact_bytes() {
    let policy = layerfs_storage::StoragePolicy::new(
        layerfs_storage::policy::FORMAT_PROFILE,
        1_048_576,
        8,
        8,
    )
    .validated()
    .unwrap();
    let metadata = Arc::new(MemoryMetadata::default());
    metadata.state.lock().unwrap().policy = policy;
    let objects = Arc::new(MemoryObjects::default());
    let storage = open(&metadata, &objects);
    let object = whole(&support::noise(1_048_553));
    let dir = support::TempDir::new("port-singleton");
    let path = dir.store_path("old");
    let old = support::disabled(|scope| {
        layerfs_storage::Store::create(&path, policy, scope.child("create"))
    })
    .unwrap();
    support::save_one(&old, object.clone()).unwrap();
    save(&storage, vec![object.clone()]);
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[object.id()])
            .unwrap(),
        vec![object.canonical().to_vec()]
    );
    assert_eq!(all_packs(&metadata, &objects), legacy_packs(&path));
    let packs = all_packs(&metadata, &objects);
    assert_eq!(
        layerfs_storage::pack::parse_header(&packs[0]).unwrap().lane,
        layerfs_storage::pack::PackLane::Singleton
    );
}
#[test]
fn a_deeper_first_wins_base_is_acknowledged_before_the_next_selection() {
    let dir = support::TempDir::new("port-race-depth");
    let path = dir.store_path("winner");
    let old = support::create_store(&path);
    let mut raw = support::noise(20_000);
    let mut base = whole(&raw);
    support::save_one(&old, base.clone()).unwrap();
    for i in 0..8 {
        raw[i] ^= 7;
        let next =
            whole(&raw).with_predecessors(AdvisoryPredecessors::explicit(base.id()).unwrap());
        assert_eq!(
            support::save_one(&old, next.clone())
                .unwrap()
                .prefix_records,
            1
        );
        base = next;
    }
    let (mut winner, payloads) = engines::snapshot(&path);
    for pack in &mut winner.packs {
        pack.info.pack_id += 10_000;
    }
    for row in &mut winner.objects {
        row.pack_id += 10_000;
    }
    let metadata = Arc::new(RaceMetadata {
        inner: MemoryMetadata::default(),
        race: Mutex::new(Some(base.id())),
        winner: Some(winner),
    });
    let objects = Arc::new(MemoryObjects::default());
    for (key, body) in payloads {
        objects.put_if_absent(key, &body).unwrap();
    }
    let storage = Storage::new(metadata, objects).unwrap();
    raw[900] ^= 9;
    let dependent =
        whole(&raw).with_predecessors(AdvisoryPredecessors::explicit(base.id()).unwrap());
    let result = save(
        &storage,
        vec![
            base.clone().with_predecessors(AdvisoryPredecessors::new()),
            dependent.clone(),
        ],
    );
    assert_eq!(result.prefix_records, 0);
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[dependent.id()])
            .unwrap(),
        vec![dependent.canonical().to_vec()]
    );
    println!("diagnostics depth-race {:?}", storage.diagnostics());
}
