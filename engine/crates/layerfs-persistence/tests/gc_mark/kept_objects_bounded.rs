//! #41: a mark holds a bounded amount of memory whatever the Store's size.
//! It reads the Store's locators in blocks instead of holding them, keeps one
//! bit per object for what it reached, and sorts what does not fit its bounds
//! through unnamed files. These Stores are large enough to cross those
//! bounds: more locators than one read takes, and more reached objects than
//! a sorter holds.
#![cfg(any(target_os = "macos", target_os = "linux"))]
use crate::support;

use layerfs_content::{
    encode_whole_file_payload,
    file::mapping::{self, ExtentNode, ExtentSlice},
    FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_history::{
    BranchId, CommitMeta, CommitOutcome, CommitRequest, CommitVia, ForkRequest, ForkSource,
    HistoryCatalog, StackInitialization,
};
use layerfs_persistence::{
    CompactPath, CompactRequest, PersistenceConfig, SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_storage::{
    encoding::{encode_full, CompressionWorkspace},
    location::{ObjectLocation, PackDomain},
    pack::{assemble, build_group, PackLane},
    port::{PackPersistence, Publication, PublishedPack, Reserve},
    save::SaveProfile,
    StorageCapacities,
};
use std::collections::BTreeSet;
use std::sync::Arc;
use support::doctor::Fixture;
use support::{branch_id, name, root};

/// Records of one group of the packs these tests publish.
const GROUP_RECORDS: usize = 4_000;

/// One small whole-file object, distinct for each `seed`.
fn small_file(seed: u64) -> FinalizedObject {
    FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(&seed.to_le_bytes()).unwrap(),
    )
    .unwrap()
}

/// Publishes `objects` as whole-file packs of one Raw group each.
fn publish_files(f: &Fixture, objects: &[FinalizedObject]) -> Vec<i64> {
    let capacities = StorageCapacities::from_policy(f.policy).unwrap();
    let mut workspace = CompressionWorkspace::new().unwrap();
    let mut profile = SaveProfile::default();
    let mut packs = Vec::new();
    for group in objects.chunks(GROUP_RECORDS) {
        let records: Vec<_> = group
            .iter()
            .map(|object| {
                encode_full(
                    object.canonical(),
                    object.role(),
                    &capacities,
                    &mut workspace,
                    &mut profile,
                )
                .unwrap()
                .record
            })
            .collect();
        let encoded = build_group(PackLane::WholeFile, &records, None).unwrap();
        let body = assemble(PackLane::WholeFile, &[encoded]).unwrap();
        let reserved = f
            .handles
            .storage
            .reserve(Reserve {
                packs: 1,
                ordinals: 0,
            })
            .unwrap();
        f.handles
            .storage
            .publish(&Publication {
                packs: vec![PublishedPack::seal(
                    reserved.first_pack_id,
                    PackDomain::Payload,
                    Arc::new(body),
                )],
                objects: group
                    .iter()
                    .enumerate()
                    .map(|(record, object)| ObjectLocation {
                        object_id: object.id(),
                        role: object.role(),
                        canonical_length: object.canonical_len(),
                        pack_id: reserved.first_pack_id,
                        group_number: 0,
                        record_number: record,
                        base: None,
                    })
                    .collect(),
                ..Publication::default()
            })
            .unwrap();
        packs.push(reserved.first_pack_id);
    }
    packs
}

/// Makes every identity of `roots` the root of a retained Commit.
fn retain(handles: &SqliteHandles, roots: &[ObjectId]) {
    let branch: BranchId = branch_id(1);
    let stack = handles
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 10,
            stack: support::stack(1),
            name: name("project-1"),
            scope: root(200),
            profile: root(201),
            genesis_root: roots[0],
        })
        .unwrap();
    handles
        .history
        .fork(&ForkRequest {
            stack: stack.id,
            branch,
            name: name("branch-1"),
            source: ForkSource::Layer(stack.head_layer),
        })
        .unwrap();
    let mut head = None;
    let mut expected_root = roots[0];
    for candidate in &roots[1..] {
        match handles
            .history
            .commit_branch(&CommitRequest {
                branch,
                expected_head: head,
                expected_base: stack.head_layer,
                expected_root,
                candidate_root: *candidate,
                profile: root(201),
                scope: root(200),
                second_parent: None,
                meta: CommitMeta::new(CommitVia::Files),
                force: false,
                required_head: None,
            })
            .unwrap()
        {
            CommitOutcome::Committed { record, .. } => {
                head = Some(record.id);
                expected_root = record.root;
            }
            other => panic!("commit: {other:?}"),
        }
    }
}

/// The files beside the Store that a mark's sorting could have left.
fn sort_files(f: &Fixture) -> Vec<String> {
    std::fs::read_dir(f.temp.join("."))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".layerfs-mark-"))
        .collect()
}

/// The reached objects are found among many blocks of locators, most of them
/// dead: the listed set is exactly the retained roots, and a rebuild writes
/// exactly those to its scratch and keeps them.
#[test]
fn a_mark_finds_its_objects_among_many_blocks_of_locators() {
    let f = Fixture::new();
    let files: Vec<_> = (0..20_000u64).map(small_file).collect();
    let packs = publish_files(&f, &files);
    // One object in sixty-seven is retained, from every pack and from the
    // first and the last block of identities.
    let mut by_identity: Vec<ObjectId> = files.iter().map(FinalizedObject::id).collect();
    by_identity.sort_unstable();
    let mut kept: BTreeSet<ObjectId> = by_identity.iter().copied().step_by(67).collect();
    kept.insert(by_identity[0]);
    kept.insert(by_identity[by_identity.len() - 1]);
    retain(
        &f.handles,
        &files
            .iter()
            .map(FinalizedObject::id)
            .filter(|id| kept.contains(id))
            .collect::<Vec<_>>(),
    );

    let mark = f.handles.history.mark().unwrap();
    assert_eq!(
        mark.objects().objects(),
        kept.iter().copied().collect::<Vec<_>>(),
        "exactly the retained roots, in identity order"
    );
    assert_eq!(mark.objects().len(), kept.len());
    assert_eq!(mark.objects().packs(), packs, "every pack holds a root");
    assert_eq!(mark.scratch_bytes(), 0, "this much is sorted in memory");

    let store = f.temp.join("store.sqlite");
    let Fixture { handles, temp, .. } = f;
    let receipt = handles
        .compact(&CompactRequest {
            op_id: [7; 16],
            ..CompactRequest::default()
        })
        .unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    let reopened = SqliteHandles::open_writable(
        PersistenceConfig::sqlite(store).with_sqlite_profile(SqlitePersistenceProfile::Normal),
        b"doctor-adversarial",
        [71; 32],
    )
    .unwrap();
    let after = reopened.history.mark().unwrap();
    assert_eq!(
        after.objects().objects(),
        kept.iter().copied().collect::<Vec<_>>(),
        "the rebuilt Store keeps the same objects"
    );
    let located: i64 = rusqlite::Connection::open(temp.join("store.sqlite"))
        .unwrap()
        .query_row("SELECT count(*) FROM object_location", [], |row| row.get(0))
        .unwrap();
    assert_eq!(located as usize, kept.len(), "and locates no other");
}

/// More objects are reached than a sorter holds: the mark writes sorted runs
/// to its unnamed files, merges them, and keeps the same set. Each root is an
/// extent page naming 120 payload objects, so the wave after the roots holds
/// every payload object.
#[test]
fn a_mark_that_sorts_through_files_keeps_the_same_set() {
    const LEAVES: usize = 920;
    const EXTENTS: usize = 120;
    let f = Fixture::new();
    let files: Vec<_> = (0..(LEAVES * EXTENTS) as u64).map(small_file).collect();
    let packs = publish_files(&f, &files);
    // One payload object in every hundred is named by two pages.
    let leaves: Vec<ObjectId> = files
        .chunks(EXTENTS)
        .enumerate()
        .map(|(index, group)| {
            let mut extents: Vec<ExtentSlice> = group
                .iter()
                .map(|file| ExtentSlice::new(file.id(), 0, 8).unwrap())
                .collect();
            if index % 100 == 1 {
                extents[0] = ExtentSlice::new(files[0].id(), 0, 8).unwrap();
            }
            f.put(
                ObjectRole::ExtentLeaf,
                mapping::encode_node(
                    &ExtentNode::Leaf {
                        subtree_logical_bytes: 8 * extents.len() as u64,
                        extents,
                    },
                    true,
                )
                .unwrap(),
            )
            .id()
        })
        .collect();
    retain(&f.handles, &leaves);
    // The first object of each page that names `files[0]` instead is dead.
    let dead: BTreeSet<ObjectId> = files
        .chunks(EXTENTS)
        .enumerate()
        .filter(|(index, _)| index % 100 == 1)
        .map(|(_, group)| group[0].id())
        .collect();
    let mut expected: BTreeSet<ObjectId> = files
        .iter()
        .map(FinalizedObject::id)
        .filter(|id| !dead.contains(id))
        .collect();
    expected.extend(leaves.iter().copied());

    let mark = f.handles.history.mark().unwrap();
    assert!(
        mark.scratch_bytes() > 0,
        "the reached records did not fit a sorter and were written out"
    );
    assert_eq!(mark.objects().len(), expected.len());
    assert_eq!(
        mark.objects().objects(),
        expected.iter().copied().collect::<Vec<_>>(),
        "exactly the pages and the payload objects they name"
    );
    for pack in &packs {
        assert!(mark.objects().packs().contains(pack));
    }
    assert_eq!(
        mark.objects().waste().dead_in_retained_packs_bytes() > 0,
        !dead.is_empty(),
        "the unnamed payload objects are dead records of retained packs"
    );
    assert!(sort_files(&f).is_empty(), "no sort file keeps a name");

    // A second mark sorts through files again and answers the same.
    let again = f.handles.history.mark().unwrap();
    assert_eq!(again.objects(), mark.objects());
    assert_eq!(again.mark_bytes(), mark.mark_bytes());

    // The compaction's own mark does not list the objects; it takes the
    // `none` path here and reports what its sorting wrote.
    let Fixture { handles, temp, .. } = f;
    let receipt = handles
        .compact(&CompactRequest {
            op_id: [9; 16],
            ..CompactRequest::default()
        })
        .unwrap();
    assert_eq!(receipt.path, CompactPath::None);
    assert!(receipt.peak_temp_bytes > 0);
    let left: Vec<_> = std::fs::read_dir(temp.join("."))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name != "store.sqlite")
        .collect();
    assert!(
        left.is_empty(),
        "nothing is left beside the Store: {left:?}"
    );
}
