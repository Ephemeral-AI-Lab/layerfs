//! A SQLite rebuild writes a retained pack that holds dead records again
//! without them: every kept object reads back byte for byte, a record kept
//! only as a delta base stays, the next compaction finds nothing to do, and a
//! run stopped before the rename leaves the old Store as it was.
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_content::{
    encode_whole_file_payload,
    file::mapping::{self, ExtentNode, ExtentSlice, FileState},
    filesystem::{
        attributes::PortableMetadata,
        directory::{encode_directory_page, DirectoryPage},
        inode::{encode_inode_page, InodePage},
        root::profile_id,
        scope_for_seed, FilesystemRoot, PathName,
    },
    inode_leaf::{InodeKind, InodeMetadata, InodeValue},
    FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_history::{HistoryCatalog, HistoryCatalogConfig, StackInitialization};
use layerfs_persistence::{
    CompactFailpoint, CompactPath, CompactRequest, PersistenceConfig, SqliteAcquisitionSchema,
    SqliteHandles, SqlitePackLayout, SqlitePersistenceProfile,
};
use layerfs_storage::{
    encoding::{encode_full, encode_prefix, raw_payload, CompressionWorkspace},
    location::{ObjectLocation, PackDomain},
    pack::{assemble, build_group, layout, layout::GroupCodec, PackLane},
    port::{ObjectKey, PackPersistence, Publication, PublishedPack, Reserve},
    save::SaveProfile,
    Storage, StorageCapacities, StoragePolicy,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use support::Temp;

const BINDING: &[u8] = b"sqlite-compact-repack-binding";

fn create(layout: SqlitePackLayout) -> (Temp, SqliteHandles, PathBuf) {
    let temp = Temp::new("sqlite-compact-repack");
    let path = temp.join("store.sqlite");
    let handles = SqliteHandles::create(
        PersistenceConfig::sqlite(path.clone())
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
            .with_sqlite_pack_layout(layout)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::default()),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: [71; 32],
            incarnation: 7,
        },
    )
    .unwrap();
    (temp, handles, path)
}

fn reopen(path: &Path, layout: SqlitePackLayout) -> SqliteHandles {
    SqliteHandles::open_writable(
        PersistenceConfig::sqlite(path.to_path_buf())
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
            .with_sqlite_pack_layout(layout)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::default()),
        BINDING,
        [71; 32],
    )
    .unwrap()
}

fn request(op: u8) -> CompactRequest {
    CompactRequest {
        op_id: [op; 16],
        ..CompactRequest::default()
    }
}

/// Deterministic bytes that do not compress.
fn noise(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut out = Vec::with_capacity(len + 8);
    while out.len() < len {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        out.extend_from_slice(&state.wrapping_mul(0x2545_F491_4F6C_DD1D).to_le_bytes());
    }
    out.truncate(len);
    out
}

fn whole(bytes: &[u8]) -> FinalizedObject {
    FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(bytes).unwrap(),
    )
    .unwrap()
}

fn chunk(bytes: &[u8]) -> FinalizedObject {
    FinalizedObject::new(
        ObjectRole::Chunk,
        mapping::encode_chunk_object(bytes).unwrap(),
    )
    .unwrap()
}

/// An extent leaf of `count` slices of one chunk, and the file state over it.
fn mapped_file(chunk: &FinalizedObject, bytes: u32, count: u64) -> [FinalizedObject; 2] {
    let leaf = FinalizedObject::new(
        ObjectRole::ExtentLeaf,
        mapping::encode_node(
            &ExtentNode::Leaf {
                subtree_logical_bytes: u64::from(bytes) * count,
                extents: (0..count)
                    .map(|_| ExtentSlice::new(chunk.id(), 0, bytes).unwrap())
                    .collect(),
            },
            true,
        )
        .unwrap(),
    )
    .unwrap()
    .with_references(vec![chunk.id()]);
    let state = FinalizedObject::new(
        ObjectRole::FileState,
        mapping::encode_file_state(FileState {
            logical_len: u64::from(bytes) * count,
            extent_count: count,
            tree_level: 0,
            profile_id: mapping::profile_id(),
            mapping_root: leaf.id(),
        })
        .unwrap(),
    )
    .unwrap()
    .with_references(vec![leaf.id()]);
    [leaf, state]
}

/// One record of a fixture pack: the object, its stored record and whether a
/// locator names it. A record without a locator is one a first-wins
/// publication lost: it is in the pack and nothing can reach it.
struct Stored {
    object: FinalizedObject,
    record: Vec<u8>,
    located: bool,
}

struct Encoder {
    capacities: StorageCapacities,
    workspace: CompressionWorkspace,
    profile: SaveProfile,
}

impl Encoder {
    fn new() -> Self {
        Self {
            capacities: StorageCapacities::from_policy(StoragePolicy::frozen_default()).unwrap(),
            workspace: CompressionWorkspace::new().unwrap(),
            profile: SaveProfile::default(),
        }
    }

    fn full(&mut self, object: &FinalizedObject) -> Stored {
        let record = encode_full(
            object.canonical(),
            object.role(),
            &self.capacities,
            &mut self.workspace,
            &mut self.profile,
        )
        .unwrap();
        Stored {
            object: object.clone(),
            record: record.record,
            located: true,
        }
    }

    fn prefix(&mut self, object: &FinalizedObject, base: &FinalizedObject) -> Stored {
        let record = encode_prefix(
            object.canonical(),
            object.role(),
            base.id(),
            raw_payload(base.canonical(), base.role()).unwrap(),
            &self.capacities,
            &mut self.workspace,
            &mut self.profile,
        )
        .unwrap();
        assert_eq!(record.base, Some(base.id()), "a PREFIX record");
        Stored {
            object: object.clone(),
            record: record.record,
            located: true,
        }
    }
}

/// Publishes one pack of `lane` whose groups hold exactly `groups`, and the
/// locators of the located records. Returns the pack id and each group's codec.
fn publish_pack(
    handles: &SqliteHandles,
    lane: PackLane,
    groups: &[Vec<Stored>],
) -> (i64, Vec<GroupCodec>) {
    let mut workspace = CompressionWorkspace::new().unwrap();
    let built: Vec<_> = groups
        .iter()
        .map(|group| {
            let records: Vec<Vec<u8>> = group.iter().map(|s| s.record.clone()).collect();
            build_group(lane, &records, Some(&mut workspace)).unwrap()
        })
        .collect();
    let codecs = built.iter().map(|group| group.codec).collect();
    let body = assemble(lane, &built).unwrap();
    let reserved = handles
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap();
    let mut objects = Vec::new();
    for (group_number, group) in groups.iter().enumerate() {
        for (record_number, stored) in group.iter().enumerate() {
            if stored.located {
                objects.push(ObjectLocation {
                    object_id: stored.object.id(),
                    role: stored.object.role(),
                    canonical_length: stored.object.canonical_len(),
                    pack_id: reserved.first_pack_id,
                    group_number,
                    record_number,
                });
            }
        }
    }
    handles
        .storage
        .publish(&Publication {
            packs: vec![PublishedPack::seal(
                reserved.first_pack_id,
                PackDomain::for_lane(lane),
                Arc::new(body),
            )],
            objects,
            ..Publication::default()
        })
        .unwrap();
    (reserved.first_pack_id, codecs)
}

/// Saves a filesystem whose regular files have `contents` as their content
/// roots and returns the root's identity.
fn tree(handles: &SqliteHandles, contents: &[ObjectId]) -> ObjectId {
    let directory = FinalizedObject::new(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf {
            entries: (0..contents.len())
                .map(|offset| {
                    (
                        PathName::new(&format!("file-{offset:03}")).unwrap(),
                        offset as u64 + 2,
                    )
                })
                .collect(),
        })
        .unwrap(),
    )
    .unwrap();
    let metadata = |mode| {
        InodeMetadata::Inline(PortableMetadata {
            mode,
            mtime_seconds: 1,
            mtime_nanoseconds: 0,
        })
    };
    let mut entries = vec![(
        1,
        InodeValue {
            kind: InodeKind::Directory,
            namespace_ref_count: 0,
            content_root: directory.id(),
            metadata_root: metadata(0o755),
        },
    )];
    for (offset, content) in contents.iter().enumerate() {
        entries.push((
            offset as u64 + 2,
            InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: *content,
                metadata_root: metadata(0o644),
            },
        ));
    }
    let mut references = vec![directory.id()];
    references.extend_from_slice(contents);
    let inode = FinalizedObject::new(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf { entries }).unwrap(),
    )
    .unwrap()
    .with_references(references);
    let root = FilesystemRoot::new(profile_id(), scope_for_seed([1; 32]), 1, inode.id()).unwrap();
    let object = FinalizedObject::new(ObjectRole::FilesystemRoot, root.encode().unwrap())
        .unwrap()
        .with_references(vec![inode.id()]);
    let id = object.id();
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    for object in [directory, inode, object] {
        save.accept(object).unwrap();
    }
    save.finish().unwrap();
    id
}

fn initialize(handles: &SqliteHandles, genesis: ObjectId) {
    handles
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 10,
            stack: support::stack(1),
            name: support::name("project-1"),
            scope: scope_for_seed([1; 32]).object(),
            profile: profile_id(),
            genesis_root: genesis,
        })
        .unwrap();
}

fn located(handles: &SqliteHandles, id: ObjectId) -> bool {
    let mut found = Vec::new();
    handles.storage.locate(&[id], &mut found).unwrap();
    !found.is_empty()
}

fn object_bytes(handles: &SqliteHandles, id: ObjectId) -> Vec<u8> {
    let storage = Storage::new(handles.storage.clone()).unwrap();
    storage
        .reader()
        .unwrap()
        .read_objects(&[id])
        .unwrap()
        .into_iter()
        .next()
        .expect("one object")
}

fn clean(handles: &SqliteHandles) {
    let mut out = Vec::new();
    handles
        .diagnose(
            layerfs_persistence::diagnostic::Options::default(),
            &mut |observation| out.push(observation),
        )
        .unwrap();
    out.retain(|observation| {
        matches!(
            observation.outcome,
            layerfs_persistence::diagnostic::Outcome::Fail
                | layerfs_persistence::diagnostic::Outcome::Unknown
        )
    });
    assert!(out.is_empty(), "{out:?}");
}

/// `(pack_id, length, digest)` of every pack row.
fn pack_rows(path: &Path) -> Vec<(i64, i64, Vec<u8>)> {
    let connection = rusqlite::Connection::open(path).unwrap();
    let mut statement = connection
        .prepare("SELECT pack_id,length,digest FROM pack ORDER BY pack_id")
        .unwrap();
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap();
    rows.map(Result::unwrap).collect()
}

/// The complete stored bytes of one pack, in either pack layout.
fn pack_body(path: &Path, pack_id: i64, layout: SqlitePackLayout) -> Vec<u8> {
    let connection = rusqlite::Connection::open(path).unwrap();
    if layout == SqlitePackLayout::Monolithic {
        return connection
            .query_row("SELECT body FROM pack WHERE pack_id=?1", [pack_id], |row| {
                row.get(0)
            })
            .unwrap();
    }
    let mut body: Vec<u8> = connection
        .query_row(
            "SELECT control FROM pack WHERE pack_id=?1",
            [pack_id],
            |row| row.get(0),
        )
        .unwrap();
    let mut statement = connection
        .prepare("SELECT body FROM pack_unit WHERE pack_id=?1 ORDER BY group_number")
        .unwrap();
    for unit in statement
        .query_map([pack_id], |row| row.get::<_, Vec<u8>>(0))
        .unwrap()
    {
        body.extend_from_slice(&unit.unwrap());
    }
    body
}

/// A Store whose three payload and metadata packs each mix live and dead
/// records, in every shape a rebuild has to rewrite.
struct Mixed {
    live: Vec<FinalizedObject>,
    dead: Vec<ObjectId>,
    packs: Vec<i64>,
}

fn mixed_store(handles: &SqliteHandles) -> Mixed {
    let mut encoder = Encoder::new();
    let mut live = Vec::new();
    let mut dead = Vec::new();
    let keep = |object: &FinalizedObject, live: &mut Vec<FinalizedObject>| {
        live.push(object.clone());
    };

    // Whole-file lane: a mixed group, a wholly dead group, and a group whose
    // first record is dead, so both group and record numbers move.
    let files: Vec<FinalizedObject> = (0..7u64)
        .map(|index| whole(&noise(100 + index, 3000 + 500 * index as usize)))
        .collect();
    let (whole_pack, _) = publish_pack(
        handles,
        PackLane::WholeFile,
        &[
            vec![
                encoder.full(&files[0]),
                encoder.full(&files[1]),
                encoder.full(&files[2]),
            ],
            vec![encoder.full(&files[3]), encoder.full(&files[4])],
            vec![encoder.full(&files[5]), encoder.full(&files[6])],
        ],
    );
    for index in [0, 2, 6] {
        keep(&files[index], &mut live);
    }
    for index in [1, 3, 4, 5] {
        dead.push(files[index].id());
    }

    // Chunk lane: live and dead chunks share one group.
    let chunks: Vec<FinalizedObject> = (0..6u64)
        .map(|index| chunk(&noise(200 + index, 2048)))
        .collect();
    let (chunk_pack, _) = publish_pack(
        handles,
        PackLane::Chunk,
        &[
            chunks.iter().take(4).map(|c| encoder.full(c)).collect(),
            chunks.iter().skip(4).map(|c| encoder.full(c)).collect(),
        ],
    );

    // Ordinary lane: one compressed group of extent leaves and file states,
    // most of them dead.
    let mut ordinary = Vec::new();
    let mut live_states = Vec::new();
    for (index, chunk) in chunks.iter().enumerate() {
        let [leaf, state] = mapped_file(chunk, 2048, 24);
        ordinary.push(encoder.full(&leaf));
        ordinary.push(encoder.full(&state));
        if index == 1 || index == 4 {
            live_states.push(state.id());
            keep(chunk, &mut live);
            keep(&leaf, &mut live);
            keep(&state, &mut live);
        } else {
            dead.extend([chunk.id(), leaf.id(), state.id()]);
        }
    }
    let (ordinary_pack, codecs) = publish_pack(handles, PackLane::Ordinary, &[ordinary]);
    assert_eq!(
        codecs,
        vec![GroupCodec::Zstandard],
        "the ordinary group is stored compressed"
    );

    let mut contents: Vec<ObjectId> = [0, 2, 6].iter().map(|index| files[*index].id()).collect();
    contents.extend(live_states);
    initialize(handles, tree(handles, &contents));
    Mixed {
        live,
        dead,
        packs: vec![whole_pack, chunk_pack, ordinary_pack],
    }
}

fn repack_round_trip(layout: SqlitePackLayout) {
    let (_temp, handles, path) = create(layout);
    let fixture = mixed_store(&handles);
    let expected: Vec<Vec<u8>> = fixture
        .live
        .iter()
        .map(|object| object_bytes(&handles, object.id()))
        .collect();
    for (object, bytes) in fixture.live.iter().zip(&expected) {
        assert_eq!(bytes.as_slice(), object.canonical());
    }
    let mark = handles.history.mark().unwrap();
    let named: Vec<i64> = mark
        .objects()
        .partly_dead_packs()
        .iter()
        .map(|(pack, _)| *pack)
        .collect();
    assert_eq!(named, fixture.packs, "the mark names the three mixed packs");
    assert!(mark.objects().waste().dead_in_retained_packs_bytes() > 0);
    let before = pack_rows(&path);
    let generation = handles.history.incarnation();

    let receipt = handles.compact(&request(1)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(
        receipt.dead_bytes_bound, 0,
        "no pack is wholly dead: the dead records inside retained packs chose the rebuild"
    );
    assert_eq!(
        receipt.kept_waste.dead_in_retained_packs_bytes, 0,
        "the rebuild keeps none of the dead records"
    );
    assert!(receipt.physical_shrink > 0);

    let reopened = reopen(&path, layout);
    assert_eq!(reopened.history.incarnation(), generation + 1);
    for (object, bytes) in fixture.live.iter().zip(&expected) {
        assert!(located(&reopened, object.id()));
        assert_eq!(
            &object_bytes(&reopened, object.id()),
            bytes,
            "object {} reads back byte for byte",
            object.id()
        );
    }
    for id in &fixture.dead {
        assert!(!located(&reopened, *id), "dead object {id} is collected");
    }
    let after = pack_rows(&path);
    assert_eq!(
        after.iter().map(|row| row.0).collect::<Vec<_>>(),
        before.iter().map(|row| row.0).collect::<Vec<_>>(),
        "every retained pack keeps its id"
    );
    for ((pack, old_length, old_digest), (_, length, digest)) in before.iter().zip(&after) {
        let body = pack_body(&path, *pack, layout);
        assert_eq!(body.len() as i64, *length, "pack {pack} length");
        assert_eq!(
            ObjectKey::for_bytes(&body).as_bytes().as_slice(),
            digest.as_slice(),
            "pack {pack} digest is the SHA-256 of its stored bytes"
        );
        let records = layout::parse_header(&body).unwrap();
        assert!(records.group_count >= 1);
        if fixture.packs.contains(pack) {
            assert!(length < old_length, "mixed pack {pack} shrank");
            assert_ne!(digest, old_digest, "mixed pack {pack} has a new digest");
        } else {
            assert_eq!((length, digest), (old_length, old_digest));
        }
    }
    let mark = reopened.history.mark().unwrap();
    assert_eq!(mark.objects().waste().dead_in_retained_packs_bytes(), 0);
    assert!(mark.objects().partly_dead_packs().is_empty());
    clean(&reopened);

    // Nothing is left to free, so the next compaction writes nothing.
    let installed = std::fs::read(&path).unwrap();
    let receipt = reopened.compact(&request(2)).unwrap();
    assert_eq!(receipt.path, CompactPath::None);
    assert_eq!(receipt.dead_bytes_bound, 0);
    assert_eq!(receipt.kept_waste.dead_in_retained_packs_bytes, 0);
    let again = reopen(&path, layout);
    assert_eq!(again.history.incarnation(), generation + 1);
    drop(again);
    assert_eq!(
        std::fs::metadata(&path).unwrap().len(),
        installed.len() as u64,
        "the second compaction leaves the Store file its size"
    );
}

#[test]
fn a_partly_dead_pack_is_written_again_and_every_kept_object_reads_back() {
    repack_round_trip(SqlitePackLayout::Monolithic);
}

#[test]
fn group_rows_packs_are_written_again_as_complete_unit_sets() {
    repack_round_trip(SqlitePackLayout::GroupRows);
}

#[test]
fn group_rows_indexed_packs_are_written_again_as_complete_unit_sets() {
    repack_round_trip(SqlitePackLayout::GroupRowsIndexed);
}

#[test]
fn a_record_kept_only_as_a_delta_base_survives_a_repack() {
    let (_temp, handles, path) = create(SqlitePackLayout::Monolithic);
    let mut encoder = Encoder::new();
    let payload = noise(7, 6000);
    let base = whole(&payload);
    let mut extended = payload.clone();
    extended.extend_from_slice(&noise(8, 400));
    let target = whole(&extended);
    let dead = whole(&noise(9, 5000));
    // The dead record sits first, so the base and the target both move.
    let (pack, _) = publish_pack(
        &handles,
        PackLane::WholeFile,
        &[vec![
            encoder.full(&dead),
            encoder.full(&base),
            encoder.prefix(&target, &base),
        ]],
    );
    initialize(&handles, tree(&handles, &[target.id()]));
    let expected = object_bytes(&handles, target.id());
    assert_eq!(expected.as_slice(), target.canonical());
    let before = pack_rows(&path);

    let receipt = handles.compact(&request(1)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(
        receipt.kept_waste.delta_base_references,
        vec![*base.id().as_bytes()],
        "the base is kept only because the stored delta names it"
    );
    assert!(receipt.kept_waste.delta_bases_bytes > 0);

    let reopened = reopen(&path, SqlitePackLayout::Monolithic);
    assert!(located(&reopened, base.id()), "the delta base survives");
    assert!(!located(&reopened, dead.id()));
    assert_eq!(
        object_bytes(&reopened, target.id()),
        expected,
        "the delta still reconstructs against its moved base"
    );
    assert_eq!(object_bytes(&reopened, base.id()), base.canonical());
    let after = pack_rows(&path);
    let old = before.iter().find(|row| row.0 == pack).unwrap();
    let new = after.iter().find(|row| row.0 == pack).unwrap();
    assert!(new.1 < old.1, "the dead record's bytes left the pack");
    clean(&reopened);
    assert_eq!(
        reopened.compact(&request(2)).unwrap().path,
        CompactPath::None
    );
}

#[test]
fn a_record_no_locator_names_is_dropped() {
    let (_temp, handles, path) = create(SqlitePackLayout::Monolithic);
    let mut encoder = Encoder::new();
    let live = whole(&noise(21, 4000));
    let lost = whole(&noise(22, 9000));
    let mut unnamed = encoder.full(&lost);
    unnamed.located = false;
    let (pack, _) = publish_pack(
        &handles,
        PackLane::WholeFile,
        &[vec![unnamed, encoder.full(&live)]],
    );
    initialize(&handles, tree(&handles, &[live.id()]));
    let mark = handles.history.mark().unwrap();
    assert_eq!(mark.objects().partly_dead_packs().len(), 1);
    assert_eq!(mark.objects().partly_dead_packs()[0].0, pack);
    let before = pack_rows(&path);

    let receipt = handles.compact(&request(1)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.kept_waste.dead_in_retained_packs_bytes, 0);
    let reopened = reopen(&path, SqlitePackLayout::Monolithic);
    assert_eq!(object_bytes(&reopened, live.id()), live.canonical());
    let after = pack_rows(&path);
    let old = before.iter().find(|row| row.0 == pack).unwrap();
    let new = after.iter().find(|row| row.0 == pack).unwrap();
    assert!(old.1 - new.1 >= 9000, "the unnamed record left the pack");
    clean(&reopened);
    assert_eq!(
        reopened.compact(&request(2)).unwrap().path,
        CompactPath::None
    );
}

#[test]
fn a_run_stopped_before_the_rename_leaves_the_old_store_valid() {
    for (op, failpoint) in [
        (0x31u8, CompactFailpoint::AfterBuild),
        (0x32, CompactFailpoint::DuringVerify),
        (0x33, CompactFailpoint::BeforeInstall),
    ] {
        let (_temp, handles, path) = create(SqlitePackLayout::Monolithic);
        let fixture = mixed_store(&handles);
        let generation = handles.history.incarnation();
        let before = pack_rows(&path);
        let stopped = handles.compact(&CompactRequest {
            op_id: [op; 16],
            failpoint: Some(failpoint),
            ..CompactRequest::default()
        });
        assert!(stopped.is_err(), "the run stops at {failpoint:?}");

        // The staged file holds the rewritten packs; the Store does not.
        assert_eq!(
            pack_rows(&path),
            before,
            "every pack row is unchanged at {failpoint:?}"
        );
        let old = reopen(&path, SqlitePackLayout::Monolithic);
        assert_eq!(old.history.incarnation(), generation);
        for object in &fixture.live {
            assert_eq!(
                object_bytes(&old, object.id()),
                object.canonical(),
                "live object readable at {failpoint:?}"
            );
        }
        for id in &fixture.dead {
            assert!(located(&old, *id), "the old Store keeps {id}");
        }
        clean(&old);

        // The next run removes the leftover and rebuilds from the old Store.
        let receipt = old.compact(&request(0x40)).unwrap();
        assert_eq!(receipt.path, CompactPath::Rebuild);
        assert_eq!(receipt.generation_after, generation + 1);
        let rebuilt = reopen(&path, SqlitePackLayout::Monolithic);
        for object in &fixture.live {
            assert_eq!(object_bytes(&rebuilt, object.id()), object.canonical());
        }
        for id in &fixture.dead {
            assert!(!located(&rebuilt, *id));
        }
        clean(&rebuilt);
    }
}
