//! A SQLite rebuild copies only the inode value blocks a kept leaf reads: a
//! block of a collected project is left behind, a block a kept and a dead leaf
//! share is kept whole, every kept leaf reads back, no ordinal is handed out
//! twice, and the verification refuses a staged file that misses a block.
#![cfg(any(target_os = "macos", target_os = "linux"))]
use crate::support;

use layerfs_content::{
    encode_whole_file_payload,
    filesystem::{
        attributes::PortableMetadata,
        directory::{encode_directory_page, DirectoryPage},
        root::profile_id,
        scope_for_seed, FilesystemRoot, PathName,
    },
    inode_leaf::{
        encode_inode_value, pooled_body, InodeKind, InodeLeaf, InodeLeafRow, InodeMetadata,
        InodeValue, LEAF_ROW_BYTES,
    },
    FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_history::{HistoryCatalog, StackInitialization};
use layerfs_persistence::internals::verify_inode_blocks;
use layerfs_persistence::{
    diagnostic::{Options, Outcome},
    CompactFailpoint, CompactPath, CompactRequest, PersistenceConfig, SqliteHandles,
    SqlitePersistenceProfile,
};
use layerfs_storage::{
    location::InodeValueRow,
    pack::PackLane,
    port::{PackPersistence, Reserve},
    Storage,
};
use std::path::{Path, PathBuf};
use support::doctor::Fixture;

const SCOPE_SEED: [u8; 32] = [11; 32];
const REMOVED_AT: i64 = 1_800_000_000_000;

/// Deterministic bytes that do not compress.
fn noise(len: usize, seed: u8) -> Vec<u8> {
    let mut state = u64::from(seed).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut out = Vec::with_capacity(len + 8);
    while out.len() < len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.extend_from_slice(&state.to_le_bytes());
    }
    out.truncate(len);
    out
}

fn whole_file(f: &Fixture, bytes: &[u8]) -> ObjectId {
    f.put(
        ObjectRole::WholeFile,
        encode_whole_file_payload(bytes).unwrap(),
    )
    .id()
}

/// The canonical inode leaf of a directory holding one file, and its values.
fn leaf_parts(f: &Fixture, content: ObjectId, seed: u8) -> (FinalizedObject, Vec<[u8; 73]>) {
    let directory = f
        .put(
            ObjectRole::DirectoryLeaf,
            encode_directory_page(&DirectoryPage::Leaf {
                entries: vec![(PathName::new(&format!("file-{seed}")).unwrap(), 2)],
            })
            .unwrap(),
        )
        .id();
    let mut values = Vec::new();
    let mut rows = Vec::new();
    for (serial, kind, count, root, mode) in [
        (1u64, InodeKind::Directory, 0u64, directory, 0o755),
        (2u64, InodeKind::RegularFile, 1u64, content, 0o644),
    ] {
        let value = encode_inode_value(InodeValue {
            kind,
            namespace_ref_count: count,
            content_root: root,
            metadata_root: InodeMetadata::Inline(PortableMetadata {
                mode,
                mtime_seconds: i64::from(seed),
                mtime_nanoseconds: 0,
            }),
        });
        values.push(value);
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

fn root_over(f: &Fixture, leaf: ObjectId) -> ObjectId {
    f.put(
        ObjectRole::FilesystemRoot,
        FilesystemRoot::new(profile_id(), scope_for_seed(SCOPE_SEED), 1, leaf)
            .unwrap()
            .encode()
            .unwrap(),
    )
    .id()
}

/// One tree whose leaf takes its own ordinals, so its values are one block.
/// Returns the root, the leaf and the leaf's canonical bytes.
fn tree(f: &Fixture, content: ObjectId, seed: u8) -> (ObjectId, ObjectId, Vec<u8>) {
    let (leaf, values) = leaf_parts(f, content, seed);
    let id = f.inode(leaf.canonical().to_vec(), values);
    assert_eq!(id, leaf.id());
    (root_over(f, id), id, leaf.canonical().to_vec())
}

/// Two trees whose leaves' values are written as one block of four ordinals.
fn trees_sharing_a_block(
    f: &Fixture,
    contents: [ObjectId; 2],
) -> [(ObjectId, ObjectId, Vec<u8>); 2] {
    let parts = [
        leaf_parts(f, contents[0], 21),
        leaf_parts(f, contents[1], 22),
    ];
    let reserved = f
        .handles
        .storage
        .reserve(Reserve {
            packs: 0,
            ordinals: 4,
        })
        .unwrap();
    let first = reserved.first_ordinal;
    let mut rows: Vec<InodeValueRow> = Vec::new();
    for (at, (_, values)) in parts.iter().enumerate() {
        for (offset, value) in values.iter().enumerate() {
            rows.push(InodeValueRow {
                ordinal: first + (2 * at + offset) as u32,
                value: *value,
            });
        }
    }
    let mut out = Vec::new();
    for (at, (leaf, _)) in parts.iter().enumerate() {
        let ordinals = [first + 2 * at as u32, first + 2 * at as u32 + 1];
        let pooled = pooled_body(leaf.canonical(), &ordinals).unwrap();
        let record = layerfs_storage::encoding::pool::leaf::encode_full(&pooled).unwrap();
        // Every value travels with the first leaf's publication: one block.
        let values = if at == 0 {
            std::mem::take(&mut rows)
        } else {
            Vec::new()
        };
        f.record(leaf, PackLane::Ordinary, record, values);
        out.push((
            root_over(f, leaf.id()),
            leaf.id(),
            leaf.canonical().to_vec(),
        ));
    }
    out.try_into().unwrap()
}

/// A leaf of one directory and `files` regular files that all hold `content`,
/// published under exactly `ordinals`.
fn wide_leaf(
    f: &Fixture,
    content: ObjectId,
    seed: u8,
    ordinals: &[u32],
) -> (ObjectId, ObjectId, Vec<u8>) {
    let directory = f
        .put(
            ObjectRole::DirectoryLeaf,
            encode_directory_page(&DirectoryPage::Leaf {
                entries: vec![(PathName::new(&format!("wide-{seed}")).unwrap(), 2)],
            })
            .unwrap(),
        )
        .id();
    let mut values = Vec::new();
    let mut rows = Vec::new();
    for serial in 1..=ordinals.len() as u64 {
        let (kind, count, root, mode) = if serial == 1 {
            (InodeKind::Directory, 0, directory, 0o755)
        } else {
            (InodeKind::RegularFile, 1, content, 0o644)
        };
        let value = encode_inode_value(InodeValue {
            kind,
            namespace_ref_count: count,
            content_root: root,
            metadata_root: InodeMetadata::Inline(PortableMetadata {
                mode,
                mtime_seconds: serial as i64 + i64::from(seed) * 1000,
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
    let leaf = FinalizedObject::new(ObjectRole::InodeLeaf, canonical).unwrap();
    let pooled = pooled_body(leaf.canonical(), ordinals).unwrap();
    let record = layerfs_storage::encoding::pool::leaf::encode_full(&pooled).unwrap();
    f.record(&leaf, PackLane::Ordinary, record, values);
    (
        root_over(f, leaf.id()),
        leaf.id(),
        leaf.canonical().to_vec(),
    )
}

/// Reserves `count` ordinals and returns the first.
fn reserve_ordinals(f: &Fixture, count: usize) -> u32 {
    f.handles
        .storage
        .reserve(Reserve {
            packs: 0,
            ordinals: count,
        })
        .unwrap()
        .first_ordinal
}

fn initialize(handles: &SqliteHandles, tag: u8, genesis: ObjectId) {
    handles
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 10,
            stack: support::stack(tag),
            name: support::name(&format!("project-{tag}")),
            scope: scope_for_seed(SCOPE_SEED).object(),
            profile: profile_id(),
            genesis_root: genesis,
        })
        .unwrap();
}

fn reopen(path: &Path) -> SqliteHandles {
    SqliteHandles::open_writable(
        PersistenceConfig::sqlite(path.to_path_buf())
            .with_sqlite_profile(SqlitePersistenceProfile::Normal),
        b"doctor-adversarial",
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

/// Every stored block: its first and last ordinal and its body.
fn blocks(path: &Path) -> Vec<(i64, i64, Vec<u8>)> {
    let connection = rusqlite::Connection::open(path).unwrap();
    let mut statement = connection
        .prepare("SELECT first_ordinal,last_ordinal,body FROM inode_value_block ORDER BY 1")
        .unwrap();
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap();
    rows.map(Result::unwrap).collect()
}

fn next_ordinal(path: &Path) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row("SELECT next_ordinal FROM store_policy", [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn object_bytes(handles: &SqliteHandles, id: ObjectId) -> Vec<u8> {
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let mut read = storage.reader().unwrap().read_objects(&[id]).unwrap();
    read.pop().expect("one object")
}

fn assert_doctor_clean(handles: &SqliteHandles) {
    let mut out = Vec::new();
    handles
        .diagnose(Options::default(), &mut |observation| out.push(observation))
        .unwrap();
    assert!(
        !out.iter()
            .any(|observation| matches!(observation.outcome, Outcome::Fail | Outcome::Unknown)),
        "the compacted Store must pass the doctor: {out:?}"
    );
}

fn staged(store: &Path) -> PathBuf {
    let mut name = store.as_os_str().to_os_string();
    name.push(".compact");
    PathBuf::from(name)
}

#[test]
fn a_rebuild_leaves_the_blocks_of_a_collected_project_behind() {
    let f = Fixture::new();
    let path = f.temp.join("store.sqlite");
    let (kept_root, kept_leaf, kept_bytes) = tree(&f, whole_file(&f, b"kept"), 1);
    let (dead_root, dead_leaf, _) = tree(&f, whole_file(&f, &noise(60_000, 9)), 2);
    initialize(&f.handles, 1, kept_root);
    initialize(&f.handles, 2, dead_root);
    f.handles
        .history
        .mark_stack_deleted(support::stack(2), REMOVED_AT)
        .unwrap();
    let before = blocks(&path);
    assert_eq!(before.len(), 2, "each leaf wrote its own block");
    let high_water = next_ordinal(&path);

    let mark = f.handles.history.mark().unwrap();
    assert_eq!(
        mark.objects().inode_ordinals(),
        &[(before[0].0 as u32, before[0].1 as u32)],
        "the mark read the kept leaf's ordinals and no other"
    );
    assert_eq!(
        mark.objects().waste().inode_blocks_bytes(),
        (before[0].2.len() + before[1].2.len()) as u64,
        "a mark alone still reports every block"
    );

    let Fixture { temp, handles, .. } = f;
    let receipt = handles.compact(&request(1)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(
        receipt.kept_waste.inode_blocks_bytes,
        before[0].2.len() as u64,
        "the report names the blocks the rebuilt Store holds"
    );

    assert_eq!(
        blocks(&path),
        vec![before[0].clone()],
        "the kept block is copied byte for byte and the dead one is gone"
    );
    assert_eq!(
        next_ordinal(&path),
        high_water,
        "the allocator does not move"
    );
    let reopened = reopen(&path);
    assert_eq!(object_bytes(&reopened, kept_leaf), kept_bytes);
    let mut found = Vec::new();
    reopened.storage.locate(&[dead_leaf], &mut found).unwrap();
    assert!(found.is_empty(), "the dead leaf is collected");
    assert_doctor_clean(&reopened);
    // An ordinal a dropped block held is never handed out again.
    let reserved = reopened
        .storage
        .reserve(Reserve {
            packs: 0,
            ordinals: 1,
        })
        .unwrap();
    assert_eq!(i64::from(reserved.first_ordinal), high_water);
    // The rebuilt Store has nothing left to collect.
    let again = reopened.compact(&request(2)).unwrap();
    assert_eq!(again.path, CompactPath::None);
    assert_eq!(
        again.kept_waste.inode_blocks_bytes,
        before[0].2.len() as u64
    );
    drop(temp);
}

#[test]
fn a_block_a_kept_and_a_dead_leaf_share_is_kept_whole() {
    let f = Fixture::new();
    let path = f.temp.join("store.sqlite");
    let [(kept_root, kept_leaf, kept_bytes), (dead_root, _, _)] = trees_sharing_a_block(
        &f,
        [whole_file(&f, b"kept"), whole_file(&f, &noise(60_000, 10))],
    );
    initialize(&f.handles, 1, kept_root);
    initialize(&f.handles, 2, dead_root);
    f.handles
        .history
        .mark_stack_deleted(support::stack(2), REMOVED_AT)
        .unwrap();
    let before = blocks(&path);
    assert_eq!(before.len(), 1, "both leaves' values are one block");
    let (first, last) = (before[0].0 as u32, before[0].1 as u32);
    assert_eq!(last - first, 3);

    let mark = f.handles.history.mark().unwrap();
    assert_eq!(
        mark.objects().inode_ordinals(),
        &[(first, first + 1)],
        "only the kept leaf's half of the block was read"
    );
    let Fixture { temp, handles, .. } = f;
    let receipt = handles.compact(&request(3)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(
        receipt.kept_waste.inode_blocks_bytes,
        before[0].2.len() as u64
    );
    assert_eq!(blocks(&path), before, "a block is one unit");
    let reopened = reopen(&path);
    assert_eq!(object_bytes(&reopened, kept_leaf), kept_bytes);
    assert_doctor_clean(&reopened);
    drop(temp);
}

#[test]
fn verification_requires_a_staged_block_for_every_ordinal_the_mark_read() {
    let f = Fixture::new();
    let path = f.temp.join("store.sqlite");
    // Two kept leaves with adjacent blocks, so one run spans two blocks.
    let (first_root, _, _) = tree(&f, whole_file(&f, b"one"), 1);
    let (second_root, _, _) = tree(&f, whole_file(&f, b"two"), 2);
    let (dead_root, _, _) = tree(&f, whole_file(&f, &noise(60_000, 11)), 3);
    initialize(&f.handles, 1, first_root);
    initialize(&f.handles, 2, second_root);
    initialize(&f.handles, 3, dead_root);
    f.handles
        .history
        .mark_stack_deleted(support::stack(3), REMOVED_AT)
        .unwrap();
    let before = blocks(&path);
    assert_eq!(before.len(), 3);
    let runs = f
        .handles
        .history
        .mark()
        .unwrap()
        .objects()
        .inode_ordinals()
        .to_vec();
    assert_eq!(
        runs,
        vec![(before[0].0 as u32, before[1].1 as u32)],
        "adjacent ordinals are one run"
    );

    let Fixture { temp, handles, .. } = f;
    let halted = handles.compact(&CompactRequest {
        op_id: [0x71; 16],
        failpoint: Some(CompactFailpoint::AfterBuild),
        ..CompactRequest::default()
    });
    assert!(halted.is_err());
    let staged = staged(&path);
    assert_eq!(
        blocks(&staged),
        before[..2].to_vec(),
        "the staged file holds the two read blocks"
    );

    let mut case = 0u8;
    let mut tampered = |change: &str, runs: &[(u32, u32)]| {
        case += 1;
        let copy = temp.path().join(format!("tampered-{case}.sqlite"));
        std::fs::copy(&staged, &copy).unwrap();
        let connection = rusqlite::Connection::open(&copy).unwrap();
        connection
            .execute_batch(&format!("PRAGMA journal_mode=DELETE;{change}"))
            .unwrap();
        // The mark hands its read ordinals over as a list of runs.
        let space = layerfs_persistence::internals::Space::new(temp.path().to_path_buf());
        let mut read = layerfs_persistence::Spans::new(&space);
        for (first, last) in runs {
            read.push(*first, *last).unwrap();
        }
        verify_inode_blocks(&connection, &read)
    };
    assert!(
        tampered("SELECT 1", &runs).is_ok(),
        "the staged file as built"
    );
    assert!(tampered("SELECT 1", &[]).is_ok(), "no read ordinal");
    let (first, second) = (before[0].0, before[1].0);
    for (what, change) in [
        (
            "the run's first block is missing",
            format!("DELETE FROM inode_value_block WHERE first_ordinal={first}"),
        ),
        (
            "the run's second block is missing",
            format!("DELETE FROM inode_value_block WHERE first_ordinal={second}"),
        ),
        (
            "a block stops short of the run's end",
            format!(
                "UPDATE inode_value_block SET last_ordinal=first_ordinal WHERE first_ordinal={second}"
            ),
        ),
        ("no block at all", "DELETE FROM inode_value_block".to_owned()),
    ] {
        assert!(tampered(&change, &runs).is_err(), "{what}");
    }
    let past = before[1].1 as u32 + 1;
    assert!(
        tampered("SELECT 1", &[(runs[0].0, past)]).is_err(),
        "a read ordinal past every staged block"
    );
    assert!(
        tampered("SELECT 1", &[(past + 7, past + 7)]).is_err(),
        "a read ordinal in the gap the dead block left"
    );
}

#[test]
fn the_read_ordinals_are_exact_across_word_and_page_boundaries() {
    let f = Fixture::new();
    let path = f.temp.join("store.sqlite");
    let content = whole_file(&f, b"shared");
    // One hundred consecutive ordinals around 65,536, where the mark's bitmap
    // changes page, and so across several of its 64-bit words.
    let start = 65_536 - 50;
    let skipped = reserve_ordinals(&f, 1);
    reserve_ordinals(&f, (start - skipped - 1) as usize);
    let first = reserve_ordinals(&f, 100);
    assert_eq!(first, start);
    let consecutive: Vec<u32> = (first..first + 100).collect();
    let (dense_root, dense_leaf, dense_bytes) = wide_leaf(&f, content, 1, &consecutive);
    // One hundred ordinals with a gap after each: one hundred runs of one.
    let first = reserve_ordinals(&f, 200);
    let alternate: Vec<u32> = (0..100).map(|step| first + 2 * step).collect();
    let (sparse_root, sparse_leaf, sparse_bytes) = wide_leaf(&f, content, 2, &alternate);
    let (dead_root, _, _) = tree(&f, whole_file(&f, &noise(60_000, 12)), 3);
    initialize(&f.handles, 1, dense_root);
    initialize(&f.handles, 2, sparse_root);
    initialize(&f.handles, 3, dead_root);
    f.handles
        .history
        .mark_stack_deleted(support::stack(3), REMOVED_AT)
        .unwrap();

    // The runs, the plain way: the first alternate ordinal follows the
    // consecutive ones directly and joins their run.
    let mut expected: Vec<(u32, u32)> = Vec::new();
    for ordinal in consecutive.iter().chain(&alternate) {
        match expected.last_mut() {
            Some(run) if run.1 + 1 == *ordinal => run.1 = *ordinal,
            _ => expected.push((*ordinal, *ordinal)),
        }
    }
    assert_eq!(expected.len(), 100);
    assert_eq!(expected[0], (65_486, 65_586));
    let mark = f.handles.history.mark().unwrap();
    assert_eq!(mark.objects().inode_ordinals(), expected.as_slice());

    let Fixture { temp, handles, .. } = f;
    let receipt = handles.compact(&request(4)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    let kept: Vec<(i64, i64)> = blocks(&path)
        .iter()
        .map(|(first, last, _)| (*first, *last))
        .collect();
    for ordinal in consecutive.iter().chain(&alternate) {
        let ordinal = i64::from(*ordinal);
        assert!(
            kept.iter()
                .any(|(first, last)| (*first..=*last).contains(&ordinal)),
            "ordinal {ordinal} has no block after the rebuild"
        );
    }
    let reopened = reopen(&path);
    assert_eq!(object_bytes(&reopened, dense_leaf), dense_bytes);
    assert_eq!(object_bytes(&reopened, sparse_leaf), sparse_bytes);
    assert_eq!(
        reopened.history.mark().unwrap().objects().inode_ordinals(),
        expected.as_slice(),
        "the rebuilt Store's leaves read the same ordinals"
    );
    drop(temp);
}

/// The ordinals a mark read are a list it sorts in bounded memory, and the
/// budget that sizes that memory changes nothing it finds: a Store of
/// leaves that read ordinals in long runs and in runs of one, with a
/// deleted project beside them, is marked and rebuilt to the same rows
/// under the least budget and under a wide one.
#[test]
fn inode_leaves_are_rebuilt_the_same_under_the_least_and_a_wide_budget() {
    let f = Fixture::new();
    let path = f.temp.join("store.sqlite");
    let content = whole_file(&f, b"shared");
    let first = reserve_ordinals(&f, 100);
    let consecutive: Vec<u32> = (first..first + 100).collect();
    let (dense_root, _, _) = wide_leaf(&f, content, 1, &consecutive);
    let first = reserve_ordinals(&f, 200);
    let alternate: Vec<u32> = (0..100).map(|step| first + 2 * step).collect();
    let (sparse_root, _, _) = wide_leaf(&f, content, 2, &alternate);
    let (dead_root, _, _) = tree(&f, whole_file(&f, &noise(60_000, 12)), 3);
    initialize(&f.handles, 1, dense_root);
    initialize(&f.handles, 2, sparse_root);
    initialize(&f.handles, 3, dead_root);
    f.handles
        .history
        .mark_stack_deleted(support::stack(3), REMOVED_AT)
        .unwrap();
    let Fixture { temp, handles, .. } = f;
    drop(handles);

    let least = support::marks::same_under_both_budgets(&path, temp.path());
    assert_eq!(least.receipt.path, CompactPath::Rebuild);
    let runs = least.mark.objects().inode_ordinals();
    assert_eq!(runs.len(), 100, "one long run joined by the first of 100");
    assert_eq!(
        runs.iter()
            .map(|(first, last)| u64::from(last - first) + 1)
            .sum::<u64>(),
        200
    );
    assert_eq!(least.after.objects().inode_ordinals(), runs);
}
