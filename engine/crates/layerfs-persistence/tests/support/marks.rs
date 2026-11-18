//! Stores for the tests of the bounded mark: small whole-file objects in
//! packs of a chosen size, extent pages naming them, and a Branch whose
//! Commits retain a list of roots.
#![allow(dead_code)]
use super::doctor::Fixture;
use super::{branch_id, name, root};
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
    CompactReceipt, CompactRequest, MarkResult, PersistenceConfig, SqliteHandles,
    SqlitePersistenceProfile,
};
use layerfs_storage::{
    encoding::{encode_full, CompressionWorkspace},
    location::{ObjectLocation, PackDomain},
    pack::{assemble, build_group, PackLane},
    port::{PackPersistence, Publication, PublishedPack, Reserve},
    save::SaveProfile,
    StorageCapacities,
};
use std::sync::Arc;

/// Records of one pack [`publish_files`] publishes.
pub const GROUP_RECORDS: usize = 4_000;

/// One small whole-file object, distinct for each `seed`.
pub fn small_file(seed: u64) -> FinalizedObject {
    FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(&seed.to_le_bytes()).unwrap(),
    )
    .unwrap()
}

/// Publishes `objects` as whole-file packs of one Raw group each.
pub fn publish_files(f: &Fixture, objects: &[FinalizedObject]) -> Vec<i64> {
    publish_packs(f, objects, GROUP_RECORDS)
}

/// Publishes `objects` as whole-file packs of one Raw group of `records`
/// objects each.
pub fn publish_packs(f: &Fixture, objects: &[FinalizedObject], records: usize) -> Vec<i64> {
    let capacities = StorageCapacities::from_policy(f.policy).unwrap();
    let mut workspace = CompressionWorkspace::new().unwrap();
    let mut profile = SaveProfile::default();
    let mut packs = Vec::new();
    for group in objects.chunks(records) {
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
pub fn retain(handles: &SqliteHandles, roots: &[ObjectId]) {
    let branch: BranchId = branch_id(1);
    let stack = handles
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 10,
            stack: super::stack(1),
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

/// Stores one extent page naming `files` and returns its identity.
pub fn leaf(f: &Fixture, files: &[ObjectId]) -> ObjectId {
    let extents: Vec<ExtentSlice> = files
        .iter()
        .map(|id| ExtentSlice::new(*id, 0, 8).unwrap())
        .collect();
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
}

/// What a mark left beside the Store, other than the Store's own files.
pub fn left_beside(f: &Fixture) -> Vec<String> {
    std::fs::read_dir(f.temp.join("."))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| {
            name != "store.sqlite" && name != "store.sqlite-wal" && name != "store.sqlite-shm"
        })
        .collect()
}

/// The least budget a mark takes, and one far above what any product run
/// has ([`layerfs_persistence::internals::BUDGET_BYTES`]).
pub const LEAST_BUDGET: u64 = 1;
pub const WIDE_BUDGET: u64 = 8 * layerfs_persistence::internals::BUDGET_BYTES;

/// What one copy of a Store gave under one budget.
pub struct UnderBudget {
    /// The mark before the compaction, listing what it kept.
    pub mark: MarkResult,
    pub receipt: CompactReceipt,
    /// The mark of the Store the compaction left.
    pub after: MarkResult,
    /// Every row of every table of that Store.
    pub rows: Vec<(String, Vec<Vec<rusqlite::types::Value>>)>,
}

fn open(path: &std::path::Path) -> SqliteHandles {
    SqliteHandles::open_writable(
        PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        b"doctor-adversarial",
        [71; 32],
    )
    .unwrap()
}

fn rows(path: &std::path::Path) -> Vec<(String, Vec<Vec<rusqlite::types::Value>>)> {
    let sql = rusqlite::Connection::open(path).unwrap();
    let tables: Vec<String> = sql
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    tables
        .into_iter()
        .map(|table| {
            let mut statement = sql.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
            let columns = statement.column_count();
            let rows = statement
                .query_map([], |row| {
                    (0..columns)
                        .map(|column| row.get::<_, rusqlite::types::Value>(column))
                        .collect::<Result<Vec<_>, _>>()
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            (table, rows)
        })
        .collect()
}

/// Marks and compacts a copy of the closed Store at `store` under `budget`,
/// in a directory of its own under `beside`.
pub fn under_budget(store: &std::path::Path, beside: &std::path::Path, budget: u64) -> UnderBudget {
    let directory = beside.join(format!("budget-{budget}"));
    std::fs::create_dir(&directory).unwrap();
    let copy = directory.join("store.sqlite");
    std::fs::copy(store, &copy).unwrap();
    let handles = open(&copy);
    handles.history.set_mark_budget(budget);
    let mark = handles.history.mark().unwrap();
    let receipt = handles
        .compact(&CompactRequest {
            op_id: [21; 16],
            ..CompactRequest::default()
        })
        .unwrap();
    let reopened = open(&copy);
    let after = reopened.history.mark().unwrap();
    drop(reopened);
    let left: Vec<String> = std::fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with("store.sqlite"))
        .collect();
    assert!(
        left.is_empty(),
        "nothing is left beside the Store: {left:?}"
    );
    UnderBudget {
        mark,
        receipt,
        after,
        rows: rows(&copy),
    }
}

/// A mark and a compaction of the closed Store at `store` find the same
/// under the least budget and under a wide one: the kept set, the receipt
/// apart from the temporary bytes it reports, and every row of the Store
/// the compaction leaves. Returns what the least budget gave.
pub fn same_under_both_budgets(store: &std::path::Path, beside: &std::path::Path) -> UnderBudget {
    let least = under_budget(store, beside, LEAST_BUDGET);
    let wide = under_budget(store, beside, WIDE_BUDGET);
    assert_eq!(least.mark.history(), wide.mark.history());
    assert_eq!(least.mark.objects(), wide.mark.objects());
    assert_eq!(
        least.mark.live_heads_digest(),
        wide.mark.live_heads_digest()
    );
    assert_eq!(
        wide.mark.paging()[1..3],
        [0, 0],
        "a wide budget pages nothing out"
    );
    let same = |receipt: &CompactReceipt| CompactReceipt {
        peak_temp_bytes: 0,
        ..receipt.clone()
    };
    assert_eq!(same(&least.receipt), same(&wide.receipt));
    assert_eq!(least.after.history(), wide.after.history());
    assert_eq!(least.after.objects(), wide.after.objects());
    for ((table, least), (_, wide)) in least.rows.iter().zip(&wide.rows) {
        assert_eq!(least.len(), wide.len(), "rows of {table}");
        assert!(least == wide, "the rows of {table} differ");
    }
    assert_eq!(least.rows.len(), wide.rows.len());
    least
}
