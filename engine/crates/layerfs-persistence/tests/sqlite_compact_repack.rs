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
    inode_leaf::{
        encode_inode_value, pooled_body, InodeKind, InodeLeaf, InodeLeafRow, InodeMetadata,
        InodeValue, LEAF_ROW_BYTES,
    },
    FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_history::{HistoryCatalog, HistoryCatalogConfig, HistoryError, StackInitialization};
use layerfs_persistence::internals::{verify_repacked, Rewritten};
use layerfs_persistence::{
    CompactFailpoint, CompactPath, CompactRequest, PersistenceConfig, SqliteAcquisitionSchema,
    SqliteHandles, SqlitePackLayout, SqlitePersistenceProfile,
};
use layerfs_storage::{
    encoding::{encode_full, encode_prefix, raw_payload, CompressionWorkspace},
    location::{InodeValueRow, ObjectLocation, PackDomain},
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
    publish_pack_with_values(handles, lane, groups, Vec::new())
}

/// [`publish_pack`], with the inode values its leaves read.
fn publish_pack_with_values(
    handles: &SqliteHandles,
    lane: PackLane,
    groups: &[Vec<Stored>],
    inode_values: Vec<InodeValueRow>,
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
            inode_values,
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

/// §11: a rebuild's `keptWaste` is what a mark of the Store it installed
/// finds, in all three parts.
fn assert_kept_waste_is_the_rebuilt_stores(
    receipt: &layerfs_persistence::CompactReceipt,
    rebuilt: &SqliteHandles,
) {
    let mark = rebuilt.history.mark().unwrap();
    let waste = mark.objects().waste();
    assert_eq!(
        (
            receipt.kept_waste.dead_in_retained_packs_bytes,
            receipt.kept_waste.delta_bases_bytes,
            receipt.kept_waste.inode_blocks_bytes,
        ),
        (
            waste.dead_in_retained_packs_bytes(),
            waste.delta_bases_bytes(),
            waste.inode_blocks_bytes(),
        ),
        "the receipt's kept waste against a mark of the rebuilt Store"
    );
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
    assert_kept_waste_is_the_rebuilt_stores(&receipt, &reopened);
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
    assert_kept_waste_is_the_rebuilt_stores(&receipt, &reopened);
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

/// An inode leaf of one directory and `files` regular files holding
/// `content`, with its values under `ordinals`. `stamp` is the last file's
/// modification time: two leaves that differ only in it share every other
/// value, and so every other ordinal.
fn inode_leaf(
    directory: ObjectId,
    content: ObjectId,
    ordinals: &[u32],
    stamp: i64,
) -> (FinalizedObject, Vec<InodeValueRow>) {
    let mut values = Vec::new();
    let mut rows = Vec::new();
    for serial in 1..=ordinals.len() as u64 {
        let (kind, count, root, mode) = if serial == 1 {
            (InodeKind::Directory, 0, directory, 0o755)
        } else {
            (InodeKind::RegularFile, 1, content, 0o644)
        };
        let last = serial == ordinals.len() as u64;
        let value = encode_inode_value(InodeValue {
            kind,
            namespace_ref_count: count,
            content_root: root,
            metadata_root: InodeMetadata::Inline(PortableMetadata {
                mode,
                mtime_seconds: if last { stamp } else { serial as i64 },
                mtime_nanoseconds: 0,
            }),
        });
        values.push(InodeValueRow {
            ordinal: ordinals[serial as usize - 1],
            value,
        });
        rows.push(InodeLeafRow { serial, value });
    }
    let canonical = InodeLeaf {
        subtree_bytes: LEAF_ROW_BYTES as u64 * rows.len() as u64,
        rows,
    }
    .encode()
    .unwrap();
    (
        FinalizedObject::new(ObjectRole::InodeLeaf, canonical).unwrap(),
        values,
    )
}

/// A record in a compressed group is charged the group's whole decoded
/// length when it is kept only as a delta base. A rebuild that writes the
/// group again without its dead records changes that figure, so the receipt
/// has to state the new one, like the two parts beside it.
#[test]
fn a_rebuild_reports_the_kept_waste_of_the_store_it_installed() {
    const FILES: usize = 40;
    let (_temp, handles, path) = create(SqlitePackLayout::Monolithic);
    let mut encoder = Encoder::new();
    let content = whole(&noise(31, 3000));
    let directory = FinalizedObject::new(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf {
            entries: (0..FILES)
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
    publish_pack(
        &handles,
        PackLane::WholeFile,
        &[vec![encoder.full(&content)]],
    );
    publish_pack(
        &handles,
        PackLane::Ordinary,
        &[vec![encoder.full(&directory)]],
    );

    // Two versions of one inode leaf. The second differs in its last value
    // and is stored as a delta against the first, which nothing else names.
    let reserved = handles
        .storage
        .reserve(Reserve {
            packs: 0,
            ordinals: FILES + 2,
        })
        .unwrap();
    let old: Vec<u32> = (0..=FILES as u32)
        .map(|offset| reserved.first_ordinal + offset)
        .collect();
    let mut new = old.clone();
    *new.last_mut().unwrap() = reserved.first_ordinal + FILES as u32 + 1;
    let (base, mut values) = inode_leaf(directory.id(), content.id(), &old, 7);
    let (target, changed) = inode_leaf(directory.id(), content.id(), &new, 8);
    values.extend(changed.last().copied());
    let base_body = pooled_body(base.canonical(), &old).unwrap();
    let target_body = pooled_body(target.canonical(), &new).unwrap();
    let delta = layerfs_storage::encoding::pool::delta::build(
        base.id(),
        &base_body,
        &target_body,
        &mut usize::MAX.clone(),
    )
    .unwrap()
    .expect("the second leaf is a delta of the first");

    // One compressed group: dead metadata, the base and the delta.
    let mut group = Vec::new();
    for index in 0..6u64 {
        let [leaf, state] = mapped_file(&chunk(&noise(300 + index, 2048)), 2048, 24);
        group.push(encoder.full(&leaf));
        group.push(encoder.full(&state));
    }
    group.push(Stored {
        object: base.clone(),
        record: layerfs_storage::encoding::pool::leaf::encode_full(&base_body).unwrap(),
        located: true,
    });
    group.push(Stored {
        object: target.clone(),
        record: delta,
        located: true,
    });
    let (pack, codecs) = publish_pack_with_values(&handles, PackLane::Ordinary, &[group], values);
    assert_eq!(codecs, vec![GroupCodec::Zstandard]);
    // The dead records of a compressed group count for nothing in the
    // decision, so a wholly dead pack takes the Store over the threshold.
    publish_pack(
        &handles,
        PackLane::WholeFile,
        &[vec![encoder.full(&whole(&noise(32, 20_000)))]],
    );

    let root = FinalizedObject::new(
        ObjectRole::FilesystemRoot,
        FilesystemRoot::new(profile_id(), scope_for_seed([1; 32]), 1, target.id())
            .unwrap()
            .encode()
            .unwrap(),
    )
    .unwrap()
    .with_references(vec![target.id()]);
    publish_pack(&handles, PackLane::Ordinary, &[vec![encoder.full(&root)]]);
    initialize(&handles, root.id());
    let expected = object_bytes(&handles, target.id());
    assert_eq!(expected.as_slice(), target.canonical());

    let before = handles.history.mark().unwrap();
    let marked = before.objects().waste().clone();
    assert_eq!(marked.delta_base_references(), &[base.id()]);
    assert_eq!(
        before
            .objects()
            .partly_dead_packs()
            .iter()
            .map(|(pack, _)| *pack)
            .collect::<Vec<_>>(),
        vec![pack],
        "the pack of the compressed group holds dead records"
    );

    let receipt = handles.compact(&request(1)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(
        receipt.kept_waste.delta_base_references,
        vec![*base.id().as_bytes()]
    );
    let reopened = reopen(&path, SqlitePackLayout::Monolithic);
    assert_kept_waste_is_the_rebuilt_stores(&receipt, &reopened);
    assert!(
        receipt.kept_waste.delta_bases_bytes > 0
            && receipt.kept_waste.delta_bases_bytes < marked.delta_bases_bytes(),
        "the base's group lost its dead records: {} is charged less than {}",
        receipt.kept_waste.delta_bases_bytes,
        marked.delta_bases_bytes()
    );
    assert_eq!(object_bytes(&reopened, target.id()), expected);
    assert_eq!(object_bytes(&reopened, base.id()), base.canonical());
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

/// One artifact of a run beside the Store file.
fn artifact(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

#[test]
fn the_working_tables_live_in_the_visited_scratch_and_count_as_temp_bytes() {
    // A run stopped after the build leaves its artifacts to look at.
    let (_stopped_temp, stopped, stopped_path) = create(SqlitePackLayout::Monolithic);
    let fixture = mixed_store(&stopped);
    let halted = stopped.compact(&CompactRequest {
        op_id: [0x51; 16],
        failpoint: Some(CompactFailpoint::AfterBuild),
        ..CompactRequest::default()
    });
    assert!(halted.is_err());
    let staged = artifact(&stopped_path, ".compact");
    let scratch_path = artifact(&stopped_path, ".compact-visited");
    let scratch = rusqlite::Connection::open(&scratch_path).unwrap();
    let count = |sql: &str| -> i64 { scratch.query_row(sql, [], |row| row.get(0)).unwrap() };
    assert_eq!(
        count("SELECT count(*) FROM repack"),
        fixture.packs.len() as i64,
        "the scratch lists the packs written again"
    );
    assert_eq!(
        count("SELECT count(*) FROM live_pack WHERE pack_id IN (SELECT pack_id FROM repack)"),
        0,
        "and the whole-pack copy no longer selects them"
    );
    assert_eq!(
        count("SELECT count(*) FROM repack_kept"),
        count("SELECT count(*) FROM visited WHERE pack_id IN (SELECT pack_id FROM repack)"),
        "with one kept locator per reached object in them"
    );
    drop(scratch);
    let temp_bytes =
        std::fs::metadata(&staged).unwrap().len() + std::fs::metadata(&scratch_path).unwrap().len();
    for suffix in ["-journal", "-wal", "-shm"] {
        assert!(
            !artifact(&scratch_path, suffix).exists(),
            "the scratch has no {suffix} sidecar"
        );
    }

    // The same Store compacted to the end reports those two files.
    let (_temp, handles, _path) = create(SqlitePackLayout::Monolithic);
    mixed_store(&handles);
    let receipt = handles.compact(&request(0x52)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(
        receipt.peak_temp_bytes, temp_bytes,
        "peakTempBytes is the fresh file plus the scratch with its working tables"
    );
}

/// What a build reports for the packs it wrote again, read here from a staged
/// file that is known to be good.
fn rewritten(staged: &rusqlite::Connection, packs: &[i64]) -> Vec<Rewritten> {
    packs
        .iter()
        .map(|pack_id| {
            let (digest, length): (Vec<u8>, i64) = staged
                .query_row(
                    "SELECT digest,length FROM pack WHERE pack_id=?1",
                    [pack_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            let records: i64 = staged
                .query_row(
                    "SELECT count(*) FROM object_location WHERE pack_id=?1",
                    [pack_id],
                    |row| row.get(0),
                )
                .unwrap();
            Rewritten {
                pack_id: *pack_id,
                digest: digest.try_into().unwrap(),
                length: length as u64,
                records: records as u64,
            }
        })
        .collect()
}

#[test]
fn verification_reads_the_staged_packs_back_and_matches_their_locators() {
    let (temp, handles, path) = create(SqlitePackLayout::Monolithic);
    let fixture = mixed_store(&handles);
    let halted = handles.compact(&CompactRequest {
        op_id: [0x61; 16],
        failpoint: Some(CompactFailpoint::AfterBuild),
        ..CompactRequest::default()
    });
    assert!(halted.is_err());
    let staged = artifact(&path, ".compact");
    let good = rusqlite::Connection::open(&staged).unwrap();
    let packs = rewritten(&good, &fixture.packs);
    drop(good);

    // A copy of the staged file, changed by one statement, then verified.
    let mut case = 0u8;
    let mut tampered = |change: &str| -> Result<(), HistoryError> {
        case += 1;
        let copy = temp.path().join(format!("tampered-{case}.sqlite"));
        std::fs::copy(&staged, &copy).unwrap();
        let connection = rusqlite::Connection::open(&copy).unwrap();
        connection
            .execute_batch(&format!(
                "PRAGMA journal_mode=DELETE;DROP TRIGGER pack_immutable_update;{change}"
            ))
            .unwrap();
        verify_repacked(&connection, &packs)
    };
    let pack = fixture.packs[0];
    // One locator of the pack, and the last record of its group.
    let one = format!(
        "(SELECT object_id FROM object_location WHERE pack_id={pack} \
ORDER BY group_number,record_number LIMIT 1)"
    );
    assert!(tampered("SELECT 1").is_ok(), "the staged file as built");
    for (what, change) in [
        (
            "a locator past its group's records",
            format!(
                "UPDATE object_location SET record_number=record_number+4000 WHERE object_id={one}"
            ),
        ),
        (
            "a locator in a group the pack lacks",
            format!("UPDATE object_location SET group_number=200 WHERE object_id={one}"),
        ),
        (
            "two locators on one record",
            format!(
                "UPDATE object_location SET (group_number,record_number)=\
(SELECT group_number,record_number FROM object_location WHERE pack_id={pack} \
ORDER BY group_number DESC,record_number DESC LIMIT 1) WHERE object_id={one}"
            ),
        ),
        (
            "a record no locator names",
            format!("DELETE FROM object_location WHERE object_id={one}"),
        ),
        (
            "a locator the pack does not hold",
            format!(
                "INSERT INTO object_location \
SELECT x'{}',role,canonical_length,pack_id,group_number,record_number \
FROM object_location WHERE object_id={one}",
                "ab".repeat(32)
            ),
        ),
        (
            "a stored byte that differs from the digest",
            format!(
                "UPDATE pack SET body=CAST(substr(body,1,length(body)-1)||\
(CASE WHEN substr(body,length(body))=x'00' THEN x'01' ELSE x'00' END) AS BLOB) \
WHERE pack_id={pack}"
            ),
        ),
        (
            "a digest that is not the one the build computed",
            format!(
                "UPDATE pack SET digest=x'{}' WHERE pack_id={pack}",
                "cd".repeat(32)
            ),
        ),
    ] {
        let result = tampered(&change);
        assert!(
            matches!(result, Err(HistoryError::Integrity(_))),
            "{what}: {result:?}"
        );
    }
    // The build's own numbers are checked against the file, too.
    let mut wrong = packs.clone();
    wrong[0].records += 1;
    let connection = rusqlite::Connection::open(&staged).unwrap();
    assert!(verify_repacked(&connection, &wrong).is_err());
    let mut wrong = packs.clone();
    wrong[0].length -= 1;
    assert!(verify_repacked(&connection, &wrong).is_err());
    assert!(verify_repacked(&connection, &packs).is_ok());
}

/// Runs one statement batch against the Store file while no session is open.
fn raw(path: &Path, sql: &str) {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection.execute_batch(sql).unwrap();
}

fn hex(id: ObjectId) -> String {
    use std::fmt::Write as _;
    id.as_bytes().iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

/// A damaged Store must not turn a compaction that used to succeed into one
/// that fails: a pack this rebuild cannot rewrite is copied whole, as before.
#[test]
fn a_pack_two_objects_name_one_record_in_is_copied_whole() {
    let (_temp, handles, path) = create(SqlitePackLayout::Monolithic);
    let fixture = mixed_store(&handles);
    drop(handles);
    // The whole-file pack keeps records 0 and 2 of its first group. The
    // second live file's locator now names the first one's record.
    let (first, second) = (fixture.live[0].id(), fixture.live[1].id());
    raw(
        &path,
        &format!(
            "UPDATE object_location SET record_number=\
(SELECT record_number FROM object_location WHERE object_id=x'{}') WHERE object_id=x'{}'",
            hex(first),
            hex(second)
        ),
    );
    let before = pack_rows(&path);
    let damaged = fixture.packs[0];
    let old_body = pack_body(&path, damaged, SqlitePackLayout::Monolithic);

    let handles = reopen(&path, SqlitePackLayout::Monolithic);
    let receipt = handles.compact(&request(0x71)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert!(
        receipt.kept_waste.dead_in_retained_packs_bytes > 0,
        "the pack copied whole still holds its dead records, and the report says so"
    );
    let after = pack_rows(&path);
    for (old, new) in before.iter().zip(&after) {
        assert_eq!(old.0, new.0);
        if old.0 == damaged {
            assert_eq!(old, new, "the pack is copied whole");
        } else if fixture.packs.contains(&old.0) {
            assert!(new.1 < old.1, "pack {} is still written again", old.0);
        }
    }
    assert_eq!(
        pack_body(&path, damaged, SqlitePackLayout::Monolithic),
        old_body
    );
    let reopened = reopen(&path, SqlitePackLayout::Monolithic);
    for object in fixture.live.iter().filter(|object| object.id() != second) {
        assert_eq!(object_bytes(&reopened, object.id()), object.canonical());
    }
    // Both locators cross the rebuild as they were.
    let connection = rusqlite::Connection::open(&path).unwrap();
    let sharing: i64 = connection
        .query_row(
            &format!(
                "SELECT count(*) FROM object_location a JOIN object_location b \
ON a.pack_id=b.pack_id AND a.group_number=b.group_number AND a.record_number=b.record_number \
WHERE a.object_id=x'{}' AND b.object_id=x'{}'",
                hex(first),
                hex(second)
            ),
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(sharing, 1);
}

#[test]
fn a_pack_whose_bytes_do_not_match_its_digest_is_copied_whole() {
    let (_temp, handles, path) = create(SqlitePackLayout::Monolithic);
    let fixture = mixed_store(&handles);
    drop(handles);
    // The chunk pack's last byte is payload of its last record: the mark,
    // which reads record headers, does not meet it.
    let damaged = fixture.packs[1];
    let trigger: String = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='pack_immutable_update'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    raw(
        &path,
        &format!(
            "DROP TRIGGER pack_immutable_update;\
UPDATE pack SET body=CAST(substr(body,1,length(body)-1)||\
(CASE WHEN substr(body,length(body))=x'00' THEN x'01' ELSE x'00' END) AS BLOB) \
WHERE pack_id={damaged};{trigger};"
        ),
    );
    let before = pack_rows(&path);
    let old_body = pack_body(&path, damaged, SqlitePackLayout::Monolithic);
    assert_ne!(
        ObjectKey::for_bytes(&old_body).as_bytes().as_slice(),
        before.iter().find(|row| row.0 == damaged).unwrap().2,
        "the stored bytes no longer match the row's digest"
    );

    let handles = reopen(&path, SqlitePackLayout::Monolithic);
    let receipt = handles.compact(&request(0x72)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    let after = pack_rows(&path);
    assert_eq!(
        before.iter().find(|row| row.0 == damaged),
        after.iter().find(|row| row.0 == damaged),
        "the damaged pack keeps its row: no fresh digest is computed over damaged bytes"
    );
    assert_eq!(
        pack_body(&path, damaged, SqlitePackLayout::Monolithic),
        old_body,
        "and its bytes, for the doctor to find"
    );
    for pack in [fixture.packs[0], fixture.packs[2]] {
        let old = before.iter().find(|row| row.0 == pack).unwrap();
        let new = after.iter().find(|row| row.0 == pack).unwrap();
        assert!(new.1 < old.1, "pack {pack} is still written again");
    }
}
