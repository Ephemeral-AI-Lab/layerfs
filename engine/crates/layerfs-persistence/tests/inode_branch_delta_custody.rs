//! Custody of inode-table branch deltas in a real SQLite Store (#360 item 3).
//!
//! * Reference closure: a branch stored as a delta names its base by id; the
//!   base stays readable as long as the branch is. Published packs and pack
//!   units cannot be deleted or changed (the schema aborts the statement),
//!   space reclamation only frees unused pages, and no product statement
//!   deletes an `object_location` row. The test shows the aborts, runs
//!   reclamation and reads every table back identically.
//! * Chronology: a base is a branch already published before the Save that
//!   names it. Two Saves begun together on two handles, each choosing the
//!   same published predecessor as base, both store delta branches and both
//!   read back, with different changes and with an identical change.
mod support;
use layerfs_content::filesystem::attributes::portable::PortableMetadata;
use layerfs_content::filesystem::inode::{
    apply_inode_values, lookup_many, InodeReadWork, InodeTable,
};
use layerfs_content::filesystem::{FilesystemObjects, MAXIMUM_SCRATCH_BYTES};
use layerfs_content::object::inode_leaf::{InodeKind, InodeMetadata, InodeValue};
use layerfs_content::{ContentResult, FinalizedConsumer, FinalizedObject, ObjectId};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePersistenceProfile,
    RECLAMATION_PAGE_LIMIT,
};
use layerfs_storage::{Save, Storage, StoragePolicy, WriteOutcome};

const INODES: u64 = 20_000;

#[derive(Default)]
struct Emitted(Vec<FinalizedObject>);
impl FinalizedConsumer for Emitted {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.push(object);
        Ok(())
    }
}

fn value(serial: u64, version: u64) -> InodeValue {
    InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: ObjectId::for_bytes(&serial.to_le_bytes()),
        metadata_root: InodeMetadata::Inline(PortableMetadata {
            mode: 0o644,
            mtime_seconds: 1_700_000_000 + version as i64,
            mtime_nanoseconds: 0,
        }),
    }
}

/// The objects the inode writer emits for `changes` on `base`. Content roots
/// are synthetic, so objects are re-made without logical references, keeping
/// the advisory predecessors the writer declared.
fn emit(
    storage: &Storage,
    base: Option<ObjectId>,
    changes: &[(u64, u64)],
) -> (ObjectId, Vec<FinalizedObject>) {
    let reader = storage.reader().unwrap();
    let mut emitted = Emitted::default();
    let (root, _) = {
        let mut objects = FilesystemObjects::new(&reader, &mut emitted);
        apply_inode_values(
            &mut objects,
            base,
            changes
                .iter()
                .map(|&(serial, version)| Ok((serial, Some(value(serial, version))))),
            MAXIMUM_SCRATCH_BYTES,
        )
        .unwrap()
    };
    let objects = emitted
        .0
        .into_iter()
        .map(|object| {
            let predecessors = object.predecessors().clone();
            let parts = object.into_parts();
            FinalizedObject::new(parts.role, parts.canonical)
                .unwrap()
                .with_predecessors(predecessors)
        })
        .collect();
    (root, objects)
}

fn save(save: Save<'_>, objects: Vec<FinalizedObject>) -> WriteOutcome {
    for object in objects {
        save.accept(object).unwrap();
    }
    save.finish().unwrap()
}

fn read_all(storage: &Storage, root: ObjectId) -> Vec<Option<InodeValue>> {
    let reader = storage.reader().unwrap();
    let serials: Vec<u64> = (1..=INODES).collect();
    let mut work = InodeReadWork::default();
    lookup_many(
        &reader,
        InodeTable {
            root,
            root_serial: 1,
        },
        &serials,
        &mut work,
    )
    .unwrap()
}

fn expected(versions: &[u64]) -> Vec<Option<InodeValue>> {
    (1..=INODES)
        .map(|serial| Some(value(serial, versions[serial as usize])))
        .collect()
}

fn create(temp: &support::Temp) -> Handles {
    Handles::create(
        PersistenceConfig::sqlite(temp.join("store.sqlite"))
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"inode-branch-delta-custody".to_vec(),
            cursor_key: [36; 32],
            incarnation: 1,
        },
    )
    .unwrap()
}

/// The initial table, saved whole.
fn initial(storage: &Storage) -> (ObjectId, Vec<u64>) {
    let rows: Vec<(u64, u64)> = (1..=INODES).map(|serial| (serial, 0)).collect();
    let (root, objects) = emit(storage, None, &rows);
    let outcome = save(storage.begin_save().unwrap(), objects);
    assert_eq!(outcome.delta.branch.delta_branches, 0);
    (root, vec![0; INODES as usize + 1])
}

#[test]
fn a_branch_base_cannot_be_dropped_and_survives_reclamation() {
    let temp = support::Temp::new("branch-delta-closure");
    let path = temp.join("store.sqlite");
    let handles = create(&temp);
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let (mut root, mut versions) = initial(&storage);
    let base = root;
    let mut roots = vec![(root, versions.clone())];
    let mut deltas = 0;
    for round in 0..6_u64 {
        let serial = 1 + (round * 7_919) % INODES;
        versions[serial as usize] += 1;
        let (next, objects) = emit(&storage, Some(root), &[(serial, versions[serial as usize])]);
        deltas += save(storage.begin_save().unwrap(), objects)
            .delta
            .branch
            .delta_branches;
        root = next;
        roots.push((root, versions.clone()));
    }
    assert!(
        deltas >= 6,
        "every Save stores its root branch as a delta: {deltas}"
    );
    // The first base's own pack, group and record, as published.
    let raw = rusqlite::Connection::open(&path).unwrap();
    let pack: i64 = raw
        .query_row(
            "SELECT pack_id FROM object_location WHERE object_id=?1",
            [base.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    let units: bool = raw
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name='pack_unit'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
        == 1;
    let refused = |sql: &str| {
        let error = raw.execute(sql, [pack]).unwrap_err().to_string();
        assert!(error.contains("immutable"), "{sql}: {error}");
    };
    refused("DELETE FROM pack WHERE pack_id=?1");
    refused("UPDATE pack SET length=length WHERE pack_id=?1");
    if units {
        refused("DELETE FROM pack_unit WHERE pack_id=?1");
        refused("UPDATE pack_unit SET body=body WHERE pack_id=?1");
    }
    drop(raw);
    // Reclamation frees unused pages only; it runs to completion and every
    // table, including every base a delta names, still reads back.
    loop {
        let job = handles.reclaim_space(RECLAMATION_PAGE_LIMIT).unwrap();
        if job.reclaimed_pages == 0 {
            break;
        }
    }
    let fresh = Storage::new(handles.storage.clone()).unwrap();
    for (root, versions) in &roots {
        assert_eq!(read_all(&fresh, *root), expected(versions));
    }
}

#[test]
fn two_saves_choosing_the_same_published_base_both_store_deltas_and_read_back() {
    let temp = support::Temp::new("branch-delta-chronology");
    let handles = create(&temp);
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let (base, versions) = initial(&storage);
    let one = Storage::new(handles.storage.clone()).unwrap();
    let two = Storage::new(handles.storage.clone()).unwrap();
    // Different changes on the same base: both Saves begin before either
    // publishes; each names the same published root as its branch base.
    let (first, second) = (1, INODES);
    let (root_one, objects_one) = emit(&one, Some(base), &[(first, 1)]);
    let (root_two, objects_two) = emit(&two, Some(base), &[(second, 1)]);
    let save_one = one.begin_save().unwrap();
    let save_two = two.begin_save().unwrap();
    let outcome_two = save(save_two, objects_two);
    let outcome_one = save(save_one, objects_one);
    for outcome in [&outcome_one, &outcome_two] {
        assert!(
            outcome.delta.branch.delta_branches >= 1,
            "{:?}",
            outcome.delta.branch
        );
        assert_eq!(outcome.delta.branch.work_exceeded, 0);
    }
    // An identical change from both: the same objects; the later Save finds
    // them already stored or stores them again, never against itself.
    let (same_one, same_objects_one) = emit(&one, Some(base), &[(7, 2)]);
    let (same_two, same_objects_two) = emit(&two, Some(base), &[(7, 2)]);
    assert_eq!(same_one, same_two);
    let save_one = one.begin_save().unwrap();
    let save_two = two.begin_save().unwrap();
    save(save_one, same_objects_one);
    save(save_two, same_objects_two);
    let fresh = Storage::new(handles.storage.clone()).unwrap();
    let mut changed = versions.clone();
    assert_eq!(read_all(&fresh, base), expected(&versions));
    changed[first as usize] = 1;
    assert_eq!(read_all(&fresh, root_one), expected(&changed));
    let mut changed = versions.clone();
    changed[second as usize] = 1;
    assert_eq!(read_all(&fresh, root_two), expected(&changed));
    let mut changed = versions;
    changed[7] = 2;
    assert_eq!(read_all(&fresh, same_one), expected(&changed));
    // Each new root branch names a base published before its Save began.
    let raw = rusqlite::Connection::open(temp.join("store.sqlite")).unwrap();
    let pack = |id: ObjectId| -> i64 {
        raw.query_row(
            "SELECT pack_id FROM object_location WHERE object_id=?1",
            [id.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap()
    };
    for root in [root_one, root_two, same_one] {
        assert!(
            pack(base) < pack(root),
            "base pack precedes its dependent's"
        );
    }
}
