use layerfs_content::filesystem::{diff_roots, ContentChange, DiffEntry};
use layerfs_layerstack_store::{
    apply_changes, BranchId, CommitId, CommitOutcome, CoreReader, DiffRequest, EntityName,
    LayerStackInitialization, LayerStackStore, LocalForkSource,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEPTHS: [usize; 4] = [1, 10, 100, 1000];
const WARMUPS: usize = 5;
const SAMPLES: usize = 31;
const FIXED_DISTANCE: usize = 10;
const COMMIT_ID_BYTES: u64 = 33;
const ORDINAL_BYTES: u64 = 8;
const DISTANCE_BYTES: u64 = 8;

type BenchResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Strategy {
    Fixed10,
    Multiscale,
}

impl Strategy {
    fn name(self) -> &'static str {
        match self {
            Self::Fixed10 => "fixed-10",
            Self::Multiscale => "multiscale",
        }
    }
}

struct HistoryIndex {
    commits: Vec<CommitId>,
    ordinals: BTreeMap<CommitId, usize>,
    strategy: Strategy,
    extra_anchors: usize,
}

impl HistoryIndex {
    fn build(commits: &[CommitId], strategy: Strategy) -> Self {
        let ordinals = commits
            .iter()
            .copied()
            .enumerate()
            .map(|(ordinal, id)| (id, ordinal))
            .collect();
        let extra_anchors = match strategy {
            Strategy::Fixed10 => (1..commits.len())
                .filter(|ordinal| ordinal % FIXED_DISTANCE == 0)
                .count(),
            Strategy::Multiscale => (1..commits.len())
                .filter(|ordinal| lowbit(*ordinal) > 1)
                .count(),
        };
        Self {
            commits: commits.to_vec(),
            ordinals,
            strategy,
            extra_anchors,
        }
    }

    fn contains(&self, head: CommitId, target: CommitId) -> (bool, u64) {
        let Some(&mut_current) = self.ordinals.get(&head) else {
            return (false, 0);
        };
        let Some(&target_ordinal) = self.ordinals.get(&target) else {
            return (false, 1);
        };
        let mut current = mut_current;
        let mut visited = 1_u64;
        if target_ordinal > current {
            return (false, visited);
        }
        while current > target_ordinal {
            let distance = match self.strategy {
                Strategy::Fixed10
                    if current % FIXED_DISTANCE == 0
                        && current - FIXED_DISTANCE >= target_ordinal =>
                {
                    FIXED_DISTANCE
                }
                Strategy::Multiscale
                    if lowbit(current) > 1 && current - lowbit(current) >= target_ordinal =>
                {
                    lowbit(current)
                }
                _ => 1,
            };
            current -= distance;
            visited += 1;
        }
        (self.commits[current] == target, visited)
    }

    fn metadata_bytes(&self) -> u64 {
        let base = self.commits.len() as u64 * (COMMIT_ID_BYTES + ORDINAL_BYTES);
        let anchors = self.extra_anchors as u64 * (COMMIT_ID_BYTES + DISTANCE_BYTES);
        base + anchors
    }
}

struct Fixture {
    root: PathBuf,
    database: PathBuf,
    store: LayerStackStore,
    branch_id: BranchId,
    commits: Vec<CommitId>,
    fixture_wall: Duration,
    initial_database_bytes: u64,
}

impl Fixture {
    fn create(depth: usize) -> BenchResult<Self> {
        let started = Instant::now();
        let root = temp_dir(depth)?;
        let database = root.join("store.sqlite");
        let store = LayerStackStore::create(&database)?;
        let initialized = store.initialize_layerstack(
            EntityName::new("history-anchor-research")?,
            LayerStackInitialization::Empty,
        )?;
        let branch_id = store.fork_branch(
            EntityName::new("main")?,
            LocalForkSource::Layer {
                layer_id: initialized.genesis_layer_id,
            },
        )?;
        let initial_database_bytes = fs::metadata(&database)?.len();
        let mut commits = Vec::with_capacity(depth);
        for ordinal in 0..depth {
            let pinned = store.pin_branch(branch_id)?;
            let mut bytes = Vec::with_capacity(4096);
            for word in 0..512_u64 {
                let value = (ordinal as u64)
                    .wrapping_mul(0x9e37_79b9_7f4a_7c15)
                    .wrapping_add(word.wrapping_mul(0xbf58_476d_1ce4_e5b9));
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            let mut seed = [0_u8; 32];
            seed[..8].copy_from_slice(&(ordinal as u64).to_le_bytes());
            let built = apply_changes(
                &pinned.reader,
                pinned.root,
                &[ContentChange::Write {
                    path: "counter".to_owned(),
                    bytes,
                    mode: 0o644,
                }],
                seed,
            )?;
            let commit_id = match store.commit_candidate(
                &pinned.branch,
                pinned.root,
                pinned.branch.base_layer_id,
                built,
            )? {
                CommitOutcome::Committed { commit_id, .. } => commit_id,
                outcome => return Err(format!("unexpected Commit outcome: {outcome:?}").into()),
            };
            commits.push(commit_id);
        }
        Ok(Self {
            root,
            database,
            store,
            branch_id,
            commits,
            fixture_wall: started.elapsed(),
            initial_database_bytes,
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[derive(Clone)]
struct Row {
    depth: usize,
    strategy: String,
    operation: String,
    median_ns: u128,
    nodes_visited: u64,
    db_operations: u64,
    metadata_bytes: u64,
    database_bytes: u64,
    store_growth_bytes: u64,
    build_ns: u128,
    reconnect_rebuild_ns: u128,
    correct: bool,
    store_unchanged: bool,
    note: String,
}

fn main() -> BenchResult<()> {
    let output = parse_output()?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut rows = Vec::new();
    for depth in DEPTHS {
        eprintln!("history-anchor research: depth {depth}");
        rows.extend(run_depth(depth)?);
    }
    write_tsv(&output, &rows)?;
    eprintln!("wrote {} rows to {}", rows.len(), output.display());
    Ok(())
}

fn run_depth(depth: usize) -> BenchResult<Vec<Row>> {
    let fixture = Fixture::create(depth)?;
    let before_counts = fixture.store.store_counts()?;
    let before_canonical = fixture.store.canonical_storage()?;
    let before_database = fs::read(&fixture.database)?;
    let database_bytes = before_database.len() as u64;
    let store_growth_bytes = database_bytes.saturating_sub(fixture.initial_database_bytes);
    let first = fixture.commits[0];
    let middle = fixture.commits[depth / 2];
    let latest = fixture.commits[depth - 1];
    let adjacent = fixture.commits[depth.saturating_sub(2)];
    let canonical_history = load_history(&fixture.store, latest)?;
    let expected: Vec<_> = fixture.commits.iter().rev().copied().collect();
    if canonical_history.0 != expected {
        return Err("paginated canonical history mismatch".into());
    }

    let mut rows = vec![Row {
        depth,
        strategy: "fixture".to_owned(),
        operation: "build-fixture".to_owned(),
        median_ns: fixture.fixture_wall.as_nanos(),
        nodes_visited: 0,
        db_operations: 0,
        metadata_bytes: 0,
        database_bytes,
        store_growth_bytes,
        build_ns: 0,
        reconnect_rebuild_ns: 0,
        correct: true,
        store_unchanged: true,
        note: format!(
            "commits={};canonical_objects={};canonical_bytes={}",
            before_counts.commits, before_canonical.objects, before_canonical.encoded_bytes
        ),
    }];

    rows.push(timed_row(
        depth,
        "baseline",
        "latest-lookup",
        0,
        1,
        0,
        database_bytes,
        store_growth_bytes,
        || Ok(fixture.store.commit(latest)?.is_some()),
    )?);
    rows.push(timed_row(
        depth,
        "baseline",
        "early-lookup",
        0,
        1,
        0,
        database_bytes,
        store_growth_bytes,
        || Ok(fixture.store.commit(first)?.is_some()),
    )?);
    rows.push(timed_row(
        depth,
        "baseline",
        "middle-lookup",
        0,
        1,
        0,
        database_bytes,
        store_growth_bytes,
        || Ok(fixture.store.commit(middle)?.is_some()),
    )?);
    rows.push(timed_row(
        depth,
        "baseline",
        "early-ancestor",
        depth as u64,
        1,
        0,
        database_bytes,
        store_growth_bytes,
        || {
            fixture
                .store
                .branch_contains_commit(fixture.branch_id, first)
                .map_err(Into::into)
        },
    )?);
    rows.push(timed_diff_baseline(
        &fixture,
        "adjacent-diff",
        adjacent,
        latest,
        baseline_membership_nodes(depth, depth.saturating_sub(2)) + 1,
        database_bytes,
        store_growth_bytes,
    )?);
    rows.push(timed_diff_baseline(
        &fixture,
        "distant-diff",
        first,
        latest,
        depth as u64 + 1,
        database_bytes,
        store_growth_bytes,
    )?);
    rows.push(timed_history(
        &fixture,
        canonical_history.1,
        canonical_history.2,
        database_bytes,
        store_growth_bytes,
    )?);

    let baseline_distant = collect_public_diff(&fixture, first, latest)?;
    let baseline_adjacent = collect_public_diff(&fixture, adjacent, latest)?;
    for strategy in [Strategy::Fixed10, Strategy::Multiscale] {
        let build_started = Instant::now();
        let index = HistoryIndex::build(&fixture.commits, strategy);
        let build_ns = build_started.elapsed().as_nanos();
        let reconnect_started = Instant::now();
        let reconnect_database = fixture
            .root
            .join(format!("reconnect-{}.sqlite", strategy.name()));
        fs::copy(&fixture.database, &reconnect_database)?;
        let connected = LayerStackStore::connect(&reconnect_database)?;
        let branch = connected
            .branch(fixture.branch_id)?
            .ok_or("missing Branch")?;
        let rebuilt_history = load_history(
            &connected,
            branch.head_commit_id.ok_or("missing Branch head")?,
        )?;
        let rebuilt_oldest: Vec<_> = rebuilt_history.0.into_iter().rev().collect();
        let rebuilt = HistoryIndex::build(&rebuilt_oldest, strategy);
        let reconnect_rebuild_ns = reconnect_started.elapsed().as_nanos();
        if rebuilt.commits != index.commits {
            return Err("reconnected sidecar differs".into());
        }
        drop(connected);
        fs::remove_file(reconnect_database)?;

        let metadata = index.metadata_bytes();
        let (ancestor_ok, ancestor_nodes) = index.contains(latest, first);
        let mut ancestor = timed_row(
            depth,
            strategy.name(),
            "early-ancestor",
            ancestor_nodes,
            0,
            metadata,
            database_bytes,
            store_growth_bytes,
            || Ok(index.contains(latest, first).0 == ancestor_ok),
        )?;
        ancestor.build_ns = build_ns;
        ancestor.reconnect_rebuild_ns = reconnect_rebuild_ns;
        ancestor.correct &= ancestor_ok;
        rows.push(ancestor);

        let adjacent_nodes = index.contains(latest, adjacent).1 + index.contains(latest, latest).1;
        let mut adjacent_row = timed_diff_candidate(
            &fixture,
            &index,
            "adjacent-diff",
            adjacent,
            latest,
            adjacent_nodes,
            &baseline_adjacent,
            database_bytes,
            store_growth_bytes,
        )?;
        adjacent_row.build_ns = build_ns;
        adjacent_row.reconnect_rebuild_ns = reconnect_rebuild_ns;
        rows.push(adjacent_row);

        let distant_nodes = index.contains(latest, first).1 + index.contains(latest, latest).1;
        let mut distant_row = timed_diff_candidate(
            &fixture,
            &index,
            "distant-diff",
            first,
            latest,
            distant_nodes,
            &baseline_distant,
            database_bytes,
            store_growth_bytes,
        )?;
        distant_row.build_ns = build_ns;
        distant_row.reconnect_rebuild_ns = reconnect_rebuild_ns;
        rows.push(distant_row);
    }

    let after_counts = fixture.store.store_counts()?;
    let after_canonical = fixture.store.canonical_storage()?;
    let after_database = fs::read(&fixture.database)?;
    let unchanged = before_counts == after_counts
        && before_canonical == after_canonical
        && before_database == after_database
        && fixture.commits == expected.iter().rev().copied().collect::<Vec<_>>();
    for row in &mut rows {
        row.store_unchanged = unchanged;
    }
    if !rows.iter().all(|row| row.correct && row.store_unchanged) {
        return Err(format!("correctness or Store immutability failed at depth {depth}").into());
    }
    Ok(rows)
}

#[allow(clippy::too_many_arguments)]
fn timed_row(
    depth: usize,
    strategy: &str,
    operation: &str,
    nodes_visited: u64,
    db_operations: u64,
    metadata_bytes: u64,
    database_bytes: u64,
    store_growth_bytes: u64,
    mut operation_fn: impl FnMut() -> BenchResult<bool>,
) -> BenchResult<Row> {
    for _ in 0..WARMUPS {
        if !operation_fn()? {
            return Err(format!("warmup correctness failed: {strategy}/{operation}").into());
        }
    }
    let mut samples = Vec::with_capacity(SAMPLES);
    let mut correct = true;
    for _ in 0..SAMPLES {
        let started = Instant::now();
        correct &= operation_fn()?;
        samples.push(started.elapsed().as_nanos());
    }
    samples.sort_unstable();
    Ok(Row {
        depth,
        strategy: strategy.to_owned(),
        operation: operation.to_owned(),
        median_ns: samples[SAMPLES / 2],
        nodes_visited,
        db_operations,
        metadata_bytes,
        database_bytes,
        store_growth_bytes,
        build_ns: 0,
        reconnect_rebuild_ns: 0,
        correct,
        store_unchanged: true,
        note: String::new(),
    })
}

fn timed_diff_baseline(
    fixture: &Fixture,
    operation: &str,
    from: CommitId,
    to: CommitId,
    nodes: u64,
    database_bytes: u64,
    store_growth_bytes: u64,
) -> BenchResult<Row> {
    let oracle = collect_public_diff(fixture, from, to)?;
    timed_row(
        fixture.commits.len(),
        "baseline",
        operation,
        nodes,
        4,
        0,
        database_bytes,
        store_growth_bytes,
        || Ok(collect_public_diff(fixture, from, to)? == oracle),
    )
}

#[allow(clippy::too_many_arguments)]
fn timed_diff_candidate(
    fixture: &Fixture,
    index: &HistoryIndex,
    operation: &str,
    from: CommitId,
    to: CommitId,
    nodes: u64,
    oracle: &[DiffEntry],
    database_bytes: u64,
    store_growth_bytes: u64,
) -> BenchResult<Row> {
    timed_row(
        fixture.commits.len(),
        index.strategy.name(),
        operation,
        nodes,
        2,
        index.metadata_bytes(),
        database_bytes,
        store_growth_bytes,
        || Ok(collect_candidate_diff(fixture, index, from, to)? == oracle),
    )
}

fn timed_history(
    fixture: &Fixture,
    rows_visited: u64,
    queries: u64,
    database_bytes: u64,
    store_growth_bytes: u64,
) -> BenchResult<Row> {
    let expected: Vec<_> = fixture.commits.iter().rev().copied().collect();
    timed_row(
        fixture.commits.len(),
        "baseline",
        "full-history",
        rows_visited,
        queries,
        0,
        database_bytes,
        store_growth_bytes,
        || {
            Ok(
                load_history(&fixture.store, fixture.commits[fixture.commits.len() - 1])?.0
                    == expected,
            )
        },
    )
}

fn collect_public_diff(
    fixture: &Fixture,
    from: CommitId,
    to: CommitId,
) -> BenchResult<Vec<DiffEntry>> {
    let mut entries = Vec::new();
    fixture.store.visit_diff(
        DiffRequest::BranchCommits {
            branch_id: fixture.branch_id,
            from_commit_id: from,
            to_commit_id: to,
        },
        |entry| {
            entries.push(entry);
            Ok(())
        },
    )?;
    Ok(entries)
}

fn collect_candidate_diff(
    fixture: &Fixture,
    index: &HistoryIndex,
    from: CommitId,
    to: CommitId,
) -> BenchResult<Vec<DiffEntry>> {
    let head = fixture.commits[fixture.commits.len() - 1];
    if !index.contains(head, from).0 || !index.contains(head, to).0 {
        return Err("Commit outside indexed Branch history".into());
    }
    let from_record = fixture.store.commit(from)?.ok_or("missing from Commit")?;
    let to_record = fixture.store.commit(to)?.ok_or("missing to Commit")?;
    let mut entries = Vec::new();
    diff_roots(
        &CoreReader(&fixture.store),
        from_record.root_id,
        to_record.root_id,
        |entry| {
            entries.push(entry);
            Ok(())
        },
    )?;
    Ok(entries)
}

fn load_history(store: &LayerStackStore, head: CommitId) -> BenchResult<(Vec<CommitId>, u64, u64)> {
    let mut start = head;
    let mut commits = Vec::new();
    let mut visited_rows = 0_u64;
    let mut queries = 0_u64;
    loop {
        let page = store.commit_history_page(start, 128)?;
        queries += 1;
        visited_rows += page.records.len() as u64;
        for record in page.records {
            if commits.last() != Some(&record.id) {
                commits.push(record.id);
            }
        }
        let Some(continuation) = page.continuation else {
            break;
        };
        start = continuation;
    }
    Ok((commits, visited_rows, queries))
}

fn baseline_membership_nodes(depth: usize, target_ordinal: usize) -> u64 {
    (depth - target_ordinal) as u64
}

fn lowbit(value: usize) -> usize {
    value & value.wrapping_neg()
}

fn parse_output() -> BenchResult<PathBuf> {
    let mut args = std::env::args().skip(1);
    let mut output = None;
    while let Some(arg) = args.next() {
        if arg == "--output" {
            output = Some(PathBuf::from(
                args.next().ok_or("--output requires a path")?,
            ));
        } else {
            return Err(format!("unknown argument: {arg}").into());
        }
    }
    Ok(output.unwrap_or_else(|| PathBuf::from("results/comparison.tsv")))
}

fn write_tsv(path: &Path, rows: &[Row]) -> BenchResult<()> {
    let mut output = String::from(
        "depth\tstrategy\toperation\tmedian_ns\tnodes_visited\tdb_operations\tmetadata_bytes\tdatabase_bytes\tstore_growth_bytes\tbuild_ns\treconnect_rebuild_ns\tcorrect\tstore_unchanged\tnote\n",
    );
    for row in rows {
        let note = if row.note.is_empty() { "-" } else { &row.note };
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            row.depth,
            row.strategy,
            row.operation,
            row.median_ns,
            row.nodes_visited,
            row.db_operations,
            row.metadata_bytes,
            row.database_bytes,
            row.store_growth_bytes,
            row.build_ns,
            row.reconnect_rebuild_ns,
            row.correct,
            row.store_unchanged,
            note,
        ));
    }
    fs::write(path, output)?;
    Ok(())
}

fn temp_dir(depth: usize) -> BenchResult<PathBuf> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = std::env::temp_dir().join(format!(
        "layerfs-history-anchor-{}-{depth}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(count: usize) -> Vec<CommitId> {
        (0..count)
            .map(|ordinal| {
                let mut bytes = [0_u8; 33];
                bytes[0] = 0x12;
                bytes[25..].copy_from_slice(&(ordinal as u64).to_be_bytes());
                CommitId::from_bytes(bytes).unwrap()
            })
            .collect()
    }

    #[test]
    fn multiscale_uses_logarithmic_deterministic_path() {
        let commits = ids(1000);
        let index = HistoryIndex::build(&commits, Strategy::Multiscale);
        assert_eq!(index.contains(commits[999], commits[0]), (true, 9));
        assert_eq!(index.contains(commits[99], commits[0]), (true, 5));
    }

    #[test]
    fn fixed_checkpoint_retains_linear_remainder() {
        let commits = ids(100);
        let index = HistoryIndex::build(&commits, Strategy::Fixed10);
        assert_eq!(index.contains(commits[99], commits[0]), (true, 19));
    }

    #[test]
    fn forward_or_unknown_target_is_rejected() {
        let commits = ids(10);
        let index = HistoryIndex::build(&commits, Strategy::Multiscale);
        assert!(!index.contains(commits[2], commits[8]).0);
        let unknown = ids(11)[10];
        assert!(!index.contains(commits[9], unknown).0);
    }

    #[test]
    fn logical_metadata_is_bounded() {
        let commits = ids(1000);
        let index = HistoryIndex::build(&commits, Strategy::Multiscale);
        assert_eq!(index.extra_anchors, 499);
        assert_eq!(index.metadata_bytes(), 61_459);
    }
}
