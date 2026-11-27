//! #768 stage 2b size-boundary matrix: the shared runner of the
//! `commit_fused_sizes*` binaries.
//!
//! A group is one seed and a list of cells. For each backend (SQLite
//! always, PostgreSQL when `LAYERFS_TEST_POSTGRES_URL` is set) the group
//! initializes two Stores from the seed: the fused Store and its forced
//! twin. Each cell forks its own Branch on both from the initial Layer and
//! applies its rounds of changes to a Workspace bound on it, Committing
//! after every round: with defaults on the fused Store, with `force: true`
//! (the separate path) on the twin. One small round follows the cell's own
//! (the second Commit). Every cell's content names the cell, so one cell's
//! objects never make another's publication smaller by deduplication.
//!
//! Every cell checks, per backend and per round:
//!
//! * the default Commit is `Committed`;
//! * it was fused (`Store::fused_counts`: one fused Commit, no fallback),
//!   or, for a cell declared [`Path::Separate`], neither (the reason is
//!   the declaration's); the follow-up round is always fused;
//! * for a round declared to split the finish's last publication
//!   ([`Cell::splits`]), the fused Commit's `finish` phase published a
//!   batch of its own ([`split_seen`]: the Store gained object, pack or
//!   value rows while it ran, read on a separate connection at the phase's
//!   boundaries): the split is proved, not assumed from pack and wave
//!   policy;
//! * the forced twin's Commit is `Committed` and was not fused;
//! * both record the same root, the same logical size and the same new
//!   canonical bytes (`CommitStatistics`; a difference in new stored or
//!   metadata bytes, which are encoding choices, is printed as a note);
//!
//! and once, at the end: the Branch read back through a fresh Store
//! service equals the in-memory model of the seed and every change (every
//! path, kind, symlink target and length; the bytes of every file the
//! cell wrote or moved and of one in 64 untouched seed files), and its
//! first-parent chain is linear with one Commit per round.
//!
//! SQLite cells each get their own pair of Stores, copied from one Store
//! initialized from the seed. PostgreSQL cells get their own pair
//! ([`Stores::PerCell`]) or, for a large seed, share one per group
//! ([`Stores::Shared`]), each cell on its own Branch.
//!
//! Each cell's line also prints, per round, what the fused finish
//! published itself (objects / packs / values, and `split`).
//!
//! Failures are collected, never raised at the first: every failing cell
//! on every backend is printed (`FAIL ...` lines, with the exact refusal),
//! then the test fails listing them all. A panic inside a cell is caught
//! and reported the same way.
use super::adversary::{
    apply, commit, commit_forced, seed_tree, verdict, Kind, Node, Op, TestStore, Tree, Verdict,
};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_history::{BranchId, CommitOutcome, CommitStatistics};
use layerfs_persistence::StoreSelection;
use layerfs_store_service::filesystem::change::mutation::Mutation;
use layerfs_store_service::store::{CommitFailure, CommitSuccess};
use layerfs_store_service::telemetry::{mark_phases, Recording};
use std::cell::RefCell;
use std::fmt::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Which Commit path a cell's rounds must take on the fused Store.
#[derive(Clone, Copy, Debug)]
pub enum Path {
    /// One fused Commit per round, no race fallback.
    Fused,
    /// The separate path, with the reason it is legitimate.
    Separate(&'static str),
}

/// One cell of the matrix: rounds of changes, each Committed once.
pub struct Cell {
    pub name: String,
    /// The bound this cell probes, for the report.
    pub probe: String,
    pub rounds: Vec<Vec<Op>>,
    pub path: Path,
    /// Rounds whose default Commit must show a split finish, by backend.
    pub splits: Vec<(Kind, usize)>,
}

impl Cell {
    pub fn new(name: impl Into<String>, probe: impl Into<String>, ops: Vec<Op>) -> Self {
        Self {
            name: name.into(),
            probe: probe.into(),
            rounds: vec![ops],
            path: Path::Fused,
            splits: Vec::new(),
        }
    }

    pub fn rounds(name: impl Into<String>, probe: impl Into<String>, rounds: Vec<Vec<Op>>) -> Self {
        Self {
            name: name.into(),
            probe: probe.into(),
            rounds,
            path: Path::Fused,
            splits: Vec::new(),
        }
    }

    /// Round `round` is meant to split the finish's last publication on
    /// each of `kinds`: its default Commit must show it ([`split_seen`]).
    pub fn splits_in(mut self, round: usize, kinds: &[Kind]) -> Self {
        self.splits.extend(kinds.iter().map(|kind| (*kind, round)));
        self
    }

    /// [`Self::splits_in`] round 0.
    pub fn splits(self, kinds: &[Kind]) -> Self {
        self.splits_in(0, kinds)
    }

    pub fn separate(mut self, reason: &'static str) -> Self {
        self.path = Path::Separate(reason);
        self
    }
}

/// 40 printable bytes naming the cell, the file and the round.
pub fn tiny(cell: usize, n: usize, round: usize) -> Vec<u8> {
    let mut bytes = format!("c{cell:03}r{round:02}n{n:06}:abcdefghijklmnopqrstuvwxyz").into_bytes();
    bytes.truncate(40);
    bytes
}

/// `length` incompressible bytes for file `n` of `cell`.
pub fn noise(cell: usize, n: usize, length: usize) -> Vec<u8> {
    super::adversary::random_bytes(((cell as u64) << 32) ^ (n as u64) ^ 0x51ce, length)
}

/// Files per directory of a seeded tree.
pub const PER_DIRECTORY: usize = 1_000;

/// The path of file `n` of a seeded tree: `PER_DIRECTORY` per directory.
pub fn seed_path(n: usize) -> String {
    format!("/s{:02}/f{n:05}", n / PER_DIRECTORY)
}

/// A seeded tree of `count` distinct 40-byte files.
pub fn seed_files(count: usize) -> Vec<(String, Vec<u8>)> {
    (0..count)
        .map(|n| (seed_path(n), tiny(999, n, 0)))
        .collect()
}

/// `count` distinct files of a `seeded`-file tree spread over all of it
/// (a prime stride, coprime with every seed size used).
pub fn spread(count: usize, seeded: usize) -> impl Iterator<Item = usize> {
    assert!(count <= seeded && seeded % 7_919 != 0);
    (0..count).map(move |k| (k * 7_919) % seeded)
}

/// The single-file seed of the groups that add everything they check.
pub fn one_file_seed() -> Vec<(String, Vec<u8>)> {
    vec![("/seed".to_owned(), b"seed\n".to_vec())]
}

/// A small deterministic generator (xorshift64*).
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Applies `op` to the model, as the Workspace applies it.
pub fn model(tree: &mut Tree, op: &Op) {
    match op {
        Op::Write(path, bytes) => {
            tree.insert(path.clone(), Node::File(bytes.clone()));
        }
        Op::Append(path, bytes) => match tree.get_mut(path) {
            Some(Node::File(old)) => old.extend_from_slice(bytes),
            other => panic!("model: append to {path}: {other:?}"),
        },
        Op::Mkdir(path) => {
            tree.insert(path.clone(), Node::Dir);
        }
        Op::Symlink(path, target) => {
            tree.insert(path.clone(), Node::Link(target.clone()));
        }
        Op::Rename(from, to) => {
            let prefix = format!("{from}/");
            let moved: Vec<String> = tree
                .range(from.clone()..)
                .take_while(|(path, _)| *path == from || path.starts_with(&prefix))
                .filter(|(path, _)| *path == from || path.starts_with(&prefix))
                .map(|(path, _)| path.clone())
                .collect();
            tree.remove(to);
            for path in moved {
                let node = tree.remove(&path).unwrap();
                tree.insert(format!("{to}{}", &path[from.len()..]), node);
            }
        }
        Op::Unlink(path) | Op::Rmdir(path) => {
            tree.remove(path);
        }
    }
}

/// What one Commit of one round answered.
struct Round {
    verdict: Verdict,
    root: Option<[u8; 32]>,
    statistics: Option<CommitStatistics>,
    /// `fused_counts` after less before: (fused Commits, race fallbacks).
    fused: (u64, u64),
    secs: f64,
    /// The default Save's (objects inserted, packs, canonical bytes).
    saved: Option<(u64, u64, u64)>,
    /// What the Store gained while the Commit's `finish` phase ran.
    finish: Published,
}

/// Rows a Store holds of what Saves publish: object locations, packs and
/// pooled inode values (summed over `inode_value_block` ranges).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Published {
    objects: i64,
    packs: i64,
    values: i64,
}

impl Published {
    fn since(self, before: Self) -> Self {
        Self {
            objects: self.objects - before.objects,
            packs: self.packs - before.packs,
            values: self.values - before.values,
        }
    }
}

const PUBLISHED_SQL: &str = "SELECT (SELECT count(*) FROM object_location), \
     (SELECT count(*) FROM pack), \
     (SELECT coalesce(sum(last_ordinal - first_ordinal + 1), 0) FROM inode_value_block)";

/// A connection of its own that reads [`Published`] from a Store while a
/// Commit runs (SQLite read-only beside the writer; PostgreSQL admin).
enum Observer {
    Sqlite(rusqlite::Connection),
    Postgres(postgres::Client),
}

impl Observer {
    fn open(store: &TestStore) -> Self {
        match &store.selection {
            StoreSelection::Sqlite(config) => Self::Sqlite(
                rusqlite::Connection::open_with_flags(
                    &config.path,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )
                .unwrap(),
            ),
            StoreSelection::Postgres(_) => Self::Postgres(store.admin()),
        }
    }

    fn published(&mut self) -> Published {
        let (objects, packs, values) = match self {
            Self::Sqlite(connection) => connection
                .query_row(PUBLISHED_SQL, [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .unwrap(),
            Self::Postgres(client) => {
                // PostgreSQL's sum of bigint is numeric: cast it back.
                let sql = PUBLISHED_SQL.replace("0) FROM", "0)::bigint FROM");
                let row = client.query_one(sql.as_str(), &[]).unwrap();
                (row.get(0), row.get(1), row.get(2))
            }
        };
        Published {
            objects,
            packs,
            values,
        }
    }
}

/// Whether a fused Commit's `finish` phase published a batch of its own:
/// the Save's last publication was split, its tail held and the rest
/// acknowledged before it (`register_ready` with `hold`; a batch of rows
/// alone, such as inode values past `TRANSACTION_ROW_LIMIT`, counts too).
/// Seen from a separate connection that reads the Store's object, pack and
/// inode value rows when `finish` starts and ends
/// (`telemetry::mark_phases`): without a split a fused finish publishes
/// nothing, so the Store gains no row (a pack or ordinal reservation adds
/// none). Meaningful only for a fused round: an unfused finish publishes
/// every batch itself.
pub fn split_seen(finish: &Published) -> bool {
    finish.objects > 0 || finish.packs > 0 || finish.values > 0
}

/// What one side (fused or forced) of a cell did.
struct Side {
    branch: BranchId,
    rounds: Vec<Round>,
    /// A change that could not be applied.
    stopped: Option<String>,
    /// Whether every round was Committed.
    complete: bool,
}

fn root_bytes(result: &Result<CommitSuccess, Box<CommitFailure>>) -> Option<[u8; 32]> {
    match result {
        Ok(CommitSuccess {
            history: Some(CommitOutcome::Committed { record, .. }),
            ..
        }) => Some(*record.root.as_bytes()),
        _ => None,
    }
}

fn statistics(result: &Result<CommitSuccess, Box<CommitFailure>>) -> Option<CommitStatistics> {
    match result {
        Ok(CommitSuccess {
            history: Some(CommitOutcome::Committed { record, .. }),
            ..
        }) => Some(record.meta.statistics),
        _ => None,
    }
}

/// Runs every round of `cell` on `store`'s Branch `name`: forced or not.
fn side(store: &TestStore, name: &str, rounds: &[Vec<Op>], forced: bool) -> Side {
    let branch = store.fork(name);
    let service = store.open();
    let workspace = service.bind(branch);
    let mut out = Side {
        branch,
        rounds: Vec::new(),
        stopped: None,
        complete: false,
    };
    for (r, ops) in rounds.iter().enumerate() {
        if let Some(error) = ops.iter().find_map(|op| {
            apply(&workspace, op)
                .err()
                .map(|error| format!("{op:?}: {error}"))
        }) {
            out.stopped = Some(format!("round {r}: change not applied: {error}"));
            break;
        }
        let before = service.service.store.fused_counts();
        // The Store's published rows when `finish` starts and ends, read on
        // the Commit's own thread at the phase boundaries.
        let recording = Recording::start();
        let observer = RefCell::new(Observer::open(store));
        let marks = Rc::new(RefCell::new((Published::default(), Published::default())));
        let seen = marks.clone();
        mark_phases(Some(Box::new(move |name, end| {
            if name == "finish" {
                let now = observer.borrow_mut().published();
                let mut marks = seen.borrow_mut();
                match end {
                    false => marks.0 = now,
                    true => {
                        let gained = now.since(marks.0);
                        marks.1.objects += gained.objects;
                        marks.1.packs += gained.packs;
                        marks.1.values += gained.values;
                    }
                }
            }
        })));
        let started = Instant::now();
        let result = if forced {
            commit_forced(&workspace)
        } else {
            commit(&workspace)
        };
        let secs = started.elapsed().as_secs_f64();
        mark_phases(None);
        drop(recording);
        let finish = marks.borrow().1;
        let after = service.service.store.fused_counts();
        let round = Round {
            verdict: verdict(&result),
            root: root_bytes(&result),
            statistics: statistics(&result),
            fused: (after.0 - before.0, after.1 - before.1),
            secs,
            saved: result.as_ref().ok().map(|done| {
                (
                    done.saved.inserted,
                    done.saved.packs,
                    done.saved.canonical_bytes,
                )
            }),
            finish,
        };
        let committed = matches!(round.verdict, Verdict::Committed(_));
        out.rounds.push(round);
        if !committed {
            break;
        }
    }
    out.complete = out.stopped.is_none() && out.rounds.len() == rounds.len();
    if !out.complete {
        let _ = service.try_unmount(workspace);
        service.abandon();
    } else {
        service.unmount(workspace);
        service.stop();
    }
    out
}

/// The tree Branch `branch` of `store` holds, read through a Workspace
/// bound on it by a fresh Store service: every entry, with the bytes of
/// each file whose model entry differs from the seed's (and of every 64th
/// other file); any other file is the model's bytes when its length
/// matches, so a difference shows in the comparison.
fn read_back(store: &TestStore, branch: BranchId, expected: &Tree, seed: &Tree) -> Tree {
    let service = store.open();
    let workspace = service.bind(branch);
    let m = Mutation::new(&workspace).unwrap();
    let mut out = Tree::new();
    let mut untouched = 0usize;
    let mut stack = vec![(String::new(), m.resolve("/").unwrap().serial)];
    while let Some((path, serial)) = stack.pop() {
        let mut after: Option<Vec<u8>> = None;
        loop {
            let page = m
                .view
                .list(m.op.overlay(), serial, after.as_deref())
                .unwrap();
            for (entry, child) in page.entries {
                let child_path = format!("{path}/{}", String::from_utf8(entry).unwrap());
                let stat = m.view.stat(m.op.overlay(), child).unwrap();
                let node = match stat.kind {
                    InodeKind::Directory => {
                        stack.push((child_path.clone(), child));
                        Node::Dir
                    }
                    InodeKind::Symlink => Node::Link(
                        m.view
                            .readlink(m.op.overlay(), child)
                            .unwrap()
                            .as_bytes()
                            .to_vec(),
                    ),
                    _ => {
                        let model = expected.get(&child_path);
                        let changed = model != seed.get(&child_path);
                        let sampled = !changed && {
                            untouched += 1;
                            untouched % 64 == 0
                        };
                        match model {
                            Some(Node::File(bytes))
                                if !changed
                                    && !sampled
                                    && bytes.len() as u64 == stat.logical_len =>
                            {
                                Node::File(bytes.clone())
                            }
                            _ => {
                                let mut bytes = Vec::with_capacity(stat.logical_len as usize);
                                while (bytes.len() as u64) < stat.logical_len {
                                    let piece =
                                        m.read(child, bytes.len() as u64, 64 * 1024).unwrap();
                                    assert!(!piece.is_empty(), "{child_path}: short read");
                                    bytes.extend_from_slice(&piece);
                                }
                                Node::File(bytes)
                            }
                        }
                    }
                };
                out.insert(child_path, node);
            }
            after = page.continuation;
            if after.is_none() {
                break;
            }
        }
    }
    m.finish().unwrap();
    service.unmount(workspace);
    service.stop();
    out
}

/// The first few differences between two trees, as text.
fn tree_difference(expected: &Tree, actual: &Tree) -> String {
    let mut out = Vec::new();
    for (path, node) in expected {
        match actual.get(path) {
            None => out.push(format!("missing {path}")),
            Some(other) if other != node => out.push(format!(
                "differs {path}: expected {}, read {}",
                describe(node),
                describe(other)
            )),
            _ => {}
        }
    }
    for path in actual.keys() {
        if !expected.contains_key(path) {
            out.push(format!("unexpected {path}"));
        }
    }
    let total = out.len();
    out.truncate(5);
    format!("{total} differences: {}", out.join("; "))
}

fn describe(node: &Node) -> String {
    match node {
        Node::File(bytes) => format!("file of {} bytes", bytes.len()),
        Node::Dir => "directory".into(),
        Node::Link(target) => format!("symlink to {:?}", String::from_utf8_lossy(target)),
    }
}

/// Runs one cell on one backend; returns its failures and a summary.
fn run_cell(
    fused: &TestStore,
    forced: &TestStore,
    index: usize,
    cell: &Cell,
    seed: &Tree,
    tainted: bool,
    kind: Kind,
) -> Report {
    let name = format!("c{index}");
    let mut notes = Vec::new();
    let mut rounds = cell.rounds.clone();
    rounds.push(vec![Op::Write(
        format!("/after-c{index}"),
        tiny(index, 0, 99),
    )]);
    let last = rounds.len() - 1;
    let mut failures = Vec::new();
    let started = Instant::now();
    let ours = side(fused, &name, &rounds, false);
    let twin = side(forced, &name, &rounds, true);
    if let Some(stop) = &ours.stopped {
        failures.push(format!("default: {stop}"));
    }
    if let Some(stop) = &twin.stopped {
        failures.push(format!("forced twin: {stop}"));
    }
    let mut paths = Vec::new();
    let mut roots_differ = Vec::new();
    for r in 0..rounds.len() {
        let round = if r == last {
            "second Commit".to_owned()
        } else {
            format!("round {r}")
        };
        let (a, b) = (ours.rounds.get(r), twin.rounds.get(r));
        if let Some(b) = b.filter(|b| !matches!(b.verdict, Verdict::Committed(_))) {
            failures.push(format!(
                "{round}: forced twin not Committed: {:?}",
                b.verdict
            ));
        }
        let Some(a) = a else {
            continue;
        };
        if !matches!(a.verdict, Verdict::Committed(_)) {
            failures.push(format!(
                "{round}: default Commit not Committed: {:?} (fused counts {:?})",
                a.verdict, a.fused
            ));
            continue;
        }
        let expect = if r == last { Path::Fused } else { cell.path };
        let path = match a.fused {
            (1, 0) => "fused",
            (0, 0) => "separate",
            (0, 1) => "race-fallback",
            _ => "unexpected",
        };
        paths.push(path);
        match (expect, a.fused) {
            (Path::Fused, (1, 0)) | (Path::Separate(_), (0, 0)) => {}
            (expect, counts) => failures.push(format!(
                "{round}: expected {expect:?}, fused counts (fused, fallback) {counts:?}"
            )),
        }
        if cell.splits.contains(&(kind, r)) && !split_seen(&a.finish) {
            failures.push(format!(
                "{round}: meant to split the finish's last publication, but the fused \
                 finish published nothing of its own (the Store gained {:?} during it)",
                a.finish
            ));
        }
        let Some(b) = b.filter(|b| matches!(b.verdict, Verdict::Committed(_))) else {
            continue;
        };
        if b.fused != (0, 0) {
            failures.push(format!("{round}: forced twin counted fused {:?}", b.fused));
        }
        if a.root != b.root {
            let text = format!(
                "{round}: root differs from the forced twin's: {} vs {}",
                hex(a.root),
                hex(b.root)
            );
            // After a failed cell the two Stores have assigned different
            // pooled value ordinals, which the inode leaves name: the twin
            // is then compared by its read-back tree instead (below).
            match tainted {
                true => roots_differ.push(text),
                false => failures.push(text),
            }
        }
        // The logical size and the inserted canonical bytes are facts of
        // the tree and of the object set: they must be equal. The stored
        // and metadata bytes are encoding choices (a delta base found or
        // not), which may follow Store and cache state: a difference there
        // is a note.
        let facts = |s: Option<CommitStatistics>| s.map(|s| (s.logical, s.new_canonical));
        if facts(a.statistics) != facts(b.statistics) {
            let text = format!(
                "{round}: logical or canonical statistics differ from the forced twin's: \
                 {:?} vs {:?}",
                a.statistics, b.statistics
            );
            // After a failed cell on shared Stores the two Stores no
            // longer hold the same objects, so a Save may insert others.
            match tainted {
                true => notes.push(format!("{text} (an earlier cell failed on these Stores)")),
                false => failures.push(text),
            }
        } else if a.statistics != b.statistics {
            notes.push(format!(
                "{round}: stored or metadata bytes differ from the forced twin's: {:?} vs {:?}",
                a.statistics, b.statistics
            ));
        }
    }
    let commit_secs: Vec<String> = ours
        .rounds
        .iter()
        .zip(twin.rounds.iter().map(Some).chain(std::iter::repeat(None)))
        .map(|(a, b)| match b {
            Some(b) => format!("{:.2}/{:.2}", a.secs, b.secs),
            None => format!("{:.2}/-", a.secs),
        })
        .collect();
    let saves: Vec<String> = ours
        .rounds
        .iter()
        .map(|a| match a.saved {
            Some((inserted, packs, bytes)) => format!("{inserted}/{packs}/{bytes}"),
            None => "-".into(),
        })
        .collect();
    let mut expected = seed.clone();
    for op in rounds.iter().flatten() {
        model(&mut expected, op);
    }
    if !roots_differ.is_empty() {
        let twin_read = match twin.complete {
            true => forced.tree(twin.branch),
            false => Tree::new(),
        };
        let note = match twin_read == expected {
            true => "the twin's tree reads back equal to the model",
            false => "the twin's tree does not read back as the model either",
        };
        for text in roots_differ {
            let text = format!("{text} (an earlier cell failed on these Stores; {note})");
            match twin_read == expected {
                true => notes.push(text),
                false => failures.push(text),
            }
        }
    }
    // Read back only when every round of the default side committed:
    // every path, kind and length, and the bytes of every file the cell
    // wrote or moved (and of one in 64 untouched seed files).
    if ours.complete {
        let read = read_back(fused, ours.branch, &expected, seed);
        if read != expected {
            failures.push(format!(
                "read back through a fresh service: {}",
                tree_difference(&expected, &read)
            ));
        }
        let chain = fused.chain(ours.branch);
        let linear = chain
            .iter()
            .enumerate()
            .all(|(n, record)| record.parent == chain.get(n + 1).map(|parent| parent.id));
        if chain.len() != rounds.len() || !linear {
            failures.push(format!(
                "chain: {} Commits (expected {}), linear {linear}",
                chain.len(),
                rounds.len()
            ));
        }
    }
    // What each fused finish published itself: objects / packs / values,
    // and `split` when it published a batch before its held tail.
    let finishes: Vec<String> = ours
        .rounds
        .iter()
        .map(|a| {
            let f = &a.finish;
            let seen = match a.fused == (1, 0) && split_seen(f) {
                true => " split",
                false => "",
            };
            format!("{}/{}/{}{seen}", f.objects, f.packs, f.values)
        })
        .collect();
    let split_rounds: Vec<usize> = ours
        .rounds
        .iter()
        .enumerate()
        .filter(|(_, a)| a.fused == (1, 0) && split_seen(&a.finish))
        .map(|(r, _)| r)
        .collect();
    let shown = |items: Vec<String>| match items.len() > 3 {
        true => format!("{} .. ({} rounds)", items[..2].join(" "), items.len()),
        false => items.join(" "),
    };
    let summary = format!(
        "path {} saved objects/packs/canonical {} finish published objects/packs/values {} \
         split rounds {split_rounds:?} commit secs default/forced {} total {:.1}s",
        shown(paths.iter().map(|p| p.to_string()).collect()),
        shown(saves),
        finishes.join(" "),
        shown(commit_secs),
        started.elapsed().as_secs_f64()
    );
    Report {
        failures,
        notes,
        summary,
    }
}

fn hex(root: Option<[u8; 32]>) -> String {
    match root {
        Some(bytes) => bytes[..8].iter().fold(String::new(), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        }),
        None => "none".into(),
    }
}

/// What one cell found on one backend.
struct Report {
    failures: Vec<String>,
    notes: Vec<String>,
    summary: String,
}

/// How a group's PostgreSQL cells get their Stores. SQLite cells always
/// get their own pair: copies of one Store initialized from the seed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stores {
    /// One pair per cell: cells cannot influence each other.
    PerCell,
    /// One pair per group, initialized once, each cell on its own Branch:
    /// for a large seed. After a failed cell, a statistics or root
    /// difference is a note when the twin's tree reads back equal.
    Shared,
}

static COPIES: AtomicU64 = AtomicU64::new(1);

/// An independent copy of a sealed SQLite Store nothing has opened since.
fn copy_sqlite(template: &TestStore, side: &str) -> TestStore {
    let StoreSelection::Sqlite(source) = &template.selection else {
        unreachable!("a SQLite template")
    };
    let n = COPIES.fetch_add(1, Ordering::Relaxed);
    let dir = template.dir.with_extension(format!("{side}{n}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = source.clone();
    config.path = dir.join("store.sqlite");
    for suffix in ["", "-wal", "-shm"] {
        let from = format!("{}{suffix}", source.path.display());
        if std::path::Path::new(&from).exists() {
            std::fs::copy(&from, format!("{}{suffix}", config.path.display())).unwrap();
        }
    }
    TestStore {
        kind: Kind::Sqlite,
        binding: template.binding.clone(),
        dir,
        selection: StoreSelection::Sqlite(config),
        postgres: None,
        stack: template.stack,
        layer: template.layer,
    }
}

/// A PostgreSQL pair, or `None` (NOT_RUN) without a server.
fn postgres_pair(label: &str, seed: &[(&str, &[u8])]) -> Option<(TestStore, TestStore)> {
    let fused = TestStore::postgres(&format!("{label}_d"), seed)?;
    let forced = TestStore::postgres(&format!("{label}_f"), seed)?;
    Some((fused, forced))
}

fn panic_text(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_else(|| "a panic".into())
}

/// Runs the group's cells on `kind` and fails listing every failing cell.
/// `LAYERFS_SIZES_CELL`, when set, runs only the cells whose name contains
/// it (for diagnosis; each keeps its own index and content).
pub fn run(kind: Kind, label: &str, seed: &[(String, Vec<u8>)], cells: &[Cell], stores: Stores) {
    let only = std::env::var("LAYERFS_SIZES_CELL").ok();
    let seed: Vec<(&str, &[u8])> = seed
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    let model_seed = seed_tree(&seed);
    let mut failures = Vec::new();
    let started = Instant::now();
    let label = &format!(
        "{label}{}",
        match kind {
            Kind::Sqlite => "s",
            Kind::Postgres => "p",
        }
    );
    {
        let begun = Instant::now();
        // SQLite: one template, copied for every cell. PostgreSQL: one
        // shared pair, or none here and one per cell.
        let mut template = None;
        let mut shared = None;
        match (kind, stores) {
            (Kind::Sqlite, _) => template = Some(TestStore::sqlite(&format!("{label}-t"), &seed)),
            (Kind::Postgres, Stores::Shared) => match postgres_pair(label, &seed) {
                Some(made) => shared = Some(made),
                None => {
                    println!(
                        "GROUP {label} {kind:?}: NOT_RUN (LAYERFS_TEST_POSTGRES_URL is unset)"
                    );
                    return;
                }
            },
            (Kind::Postgres, Stores::PerCell) => {}
        }
        println!(
            "GROUP {label} {kind:?}: Stores from {} seed files in {:.1}s",
            seed.len(),
            begun.elapsed().as_secs_f64()
        );
        let mut tainted = false;
        for (index, cell) in cells.iter().enumerate() {
            if only
                .as_ref()
                .is_some_and(|only| !cell.name.contains(only.as_str()))
            {
                continue;
            }
            let own = match (&template, &shared) {
                (Some(template), _) => {
                    Some((copy_sqlite(template, "d"), copy_sqlite(template, "f")))
                }
                (None, Some(_)) => None,
                (None, None) => match postgres_pair(&format!("{label}c{index}"), &seed) {
                    Some(made) => Some(made),
                    None => {
                        println!("CELL {label}/{} {kind:?}: NOT_RUN", cell.name);
                        continue;
                    }
                },
            };
            let (fused, forced) = shared.as_ref().or(own.as_ref()).unwrap();
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                run_cell(fused, forced, index, cell, &model_seed, tainted, kind)
            }));
            let report = outcome.unwrap_or_else(|panic| Report {
                failures: vec![format!("panicked: {}", panic_text(panic))],
                notes: Vec::new(),
                summary: "panicked".into(),
            });
            let verdict = match report.failures.is_empty() {
                true => "PASS",
                false => "FAIL",
            };
            println!(
                "CELL {label}/{} {kind:?} [{}] {verdict}: {}",
                cell.name, cell.probe, report.summary
            );
            for note in report.notes {
                println!("NOTE {label}/{} {kind:?}: {note}", cell.name);
            }
            tainted |= !report.failures.is_empty() && shared.is_some();
            for failure in report.failures {
                let line = format!("FAIL {label}/{} {kind:?}: {failure}", cell.name);
                println!("{line}");
                failures.push(line);
            }
        }
    }
    println!(
        "GROUP {label}: {} failing checks in {:.1}s",
        failures.len(),
        started.elapsed().as_secs_f64()
    );
    assert!(
        failures.is_empty(),
        "{} failing checks:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
