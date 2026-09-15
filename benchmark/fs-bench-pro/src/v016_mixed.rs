// v0.1.6 M1 host orchestrator: five-stage mixed development under retained history.
//
// One shared host Store, one Linux daemon/FUSE container and one active lease per
// branch. Every stage helper is a single `Client::exec_workspace_session` call
// that has finished before the following full-status Commit. The declared
// operation counters, the graph cardinality and the independent oracle come from
// `workload_source::v016_stages`, never from the mutated Store.
use super::*;
use std::collections::BTreeMap;
use std::sync::Mutex;
use workload_source::v016_common as v016;
use workload_source::v016_stages::{self as stages, MixedCase, Stage, Topology};
use workload_source::workspace_common::Case;

pub(crate) const FAMILY_MIXED: &str = "mixed_load_bearing";
pub(crate) const FAMILY_WORKSPACE: &str = "multi_workspace_development";
pub(crate) const FAMILY_BRANCH: &str = "branch_development";

/// The two-workspace discard/reopen event is fixed at local commit 5.
const DISCARD_COMMIT: usize = 5;
/// The trunk fork point of the branch topology.
pub(crate) const TRUNK_FORK_COMMIT: usize = 5;
pub(crate) const TRUNK_COMMITS: usize = 10;

pub(crate) fn is_mixed(case: &Case) -> bool {
    stages::mixed_case(&case.id).ok().flatten().is_some()
}

fn plan_of(case: &Case) -> AnyResult<MixedCase> {
    stages::mixed_case(&case.id)?.ok_or_else(|| format!("not a v0.1.6 M1 case: {}", case.id).into())
}

fn tier_of(name: &str) -> AnyResult<&'static v016::LoadTier> {
    Ok(match name {
        "L100" => &v016::L100,
        "L500" => &v016::L500,
        other => return Err(format!("v0.1.6 unknown tier {other}").into()),
    })
}

pub(crate) fn quote(value: &str) -> String {
    super::workspace_bench::quote(value)
}

pub(crate) fn emit(kind: &str, fields: &[(&str, String)]) {
    super::workspace_bench::emit(kind, fields)
}

// --------------------------------------------------------------- run state

pub(crate) struct RunPlan {
    pub(crate) mixed: MixedCase,
    pub(crate) seed: u8,
    pub(crate) fixture: v016::LoadFixture,
    /// Local commits each M1 worker executes.
    pub(crate) per_worker: usize,
    /// Live Workspaces of this invocation.
    pub(crate) workers: usize,
}

impl RunPlan {
    pub(crate) fn branch_topology(&self) -> bool {
        self.mixed.topology == Topology::Branch
    }
    pub(crate) fn concurrent(&self) -> bool {
        self.mixed.topology == Topology::Concurrent
    }
    /// Every commit the invocation is required to create.
    pub(crate) fn declared_commits(&self) -> usize {
        if self.branch_topology() {
            TRUNK_COMMITS + (self.workers - 1) * self.per_worker
        } else {
            self.workers * self.per_worker
        }
    }
}

/// Cumulative wall accounting for one selected invocation. The 15-second
/// complete-command deadline and its 12-second worker stop plus 3-second
/// cleanup reserve are the roadmap's declaration, not a measurement.
struct Budget {
    started: Instant,
    limit_ns: u64,
}

impl Budget {
    fn check(&self, phase: &str) -> AnyResult<()> {
        if super::elapsed_ns(self.started) > self.limit_ns {
            return Err(format!("v0.1.6 complete-command deadline exceeded during {phase}").into());
        }
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct Shared {
    /// (worker, commit start ns, commit end ns) on one monotonic origin.
    intervals: Mutex<Vec<(usize, u64, u64)>>,
    pub(crate) counters: Mutex<BTreeMap<String, usize>>,
    sdk_calls: Mutex<usize>,
    /// Pure product-call wall: stage helper executions, SDK range edits and
    /// full-status Commits, summed over every worker.
    pure_call_ns: Mutex<u64>,
    commit_ns: Mutex<Vec<u64>>,
    execution_ns: Mutex<Vec<u64>>,
}

impl Shared {
    fn add(&self, rows: &BTreeMap<&'static str, usize>) -> AnyResult<()> {
        let mut counters = self.counters.lock().map_err(|_| "v0.1.6 counter lock")?;
        for (name, value) in rows {
            *counters.entry((*name).to_owned()).or_default() += *value;
        }
        Ok(())
    }
    fn record(&self, worker: usize, start: u64, end: u64) -> AnyResult<()> {
        self.intervals
            .lock()
            .map_err(|_| "v0.1.6 interval lock")?
            .push((worker, start, end));
        Ok(())
    }
    fn overlap_ns(&self) -> AnyResult<u64> {
        let rows = self.intervals.lock().map_err(|_| "v0.1.6 interval lock")?;
        let mut best = 0u64;
        for (index, (left_worker, left_start, left_end)) in rows.iter().enumerate() {
            for (right_worker, right_start, right_end) in rows.iter().skip(index + 1) {
                if left_worker == right_worker {
                    continue;
                }
                best = best.max(
                    (*left_end)
                        .min(*right_end)
                        .saturating_sub((*left_start).max(*right_start)),
                );
            }
        }
        Ok(best)
    }
    fn overlap_rounds(&self) -> AnyResult<usize> {
        let rows = self.intervals.lock().map_err(|_| "v0.1.6 interval lock")?;
        let mut rounds = 0usize;
        for (index, (left_worker, left_start, left_end)) in rows.iter().enumerate() {
            if rows
                .iter()
                .skip(index + 1)
                .any(|(right_worker, right_start, right_end)| {
                    left_worker != right_worker
                        && (*left_end).min(*right_end) > (*left_start).max(*right_start)
                })
            {
                rounds += 1;
            }
        }
        Ok(rounds)
    }
    fn count_sdk(&self, rows: usize) -> AnyResult<()> {
        let mut calls = self.sdk_calls.lock().map_err(|_| "v0.1.6 sdk lock")?;
        *calls += rows;
        Ok(())
    }
    fn add_pure_call(&self, elapsed_ns: u64) -> AnyResult<()> {
        let mut total = self.pure_call_ns.lock().map_err(|_| "v0.1.6 timer lock")?;
        *total = total
            .checked_add(elapsed_ns)
            .ok_or("v0.1.6 pure-call sum overflow")?;
        Ok(())
    }
    fn record_stage(&self, kind: StageKind, elapsed_ns: u64) -> AnyResult<()> {
        let target = match kind {
            StageKind::Commit => &self.commit_ns,
            StageKind::Execution => &self.execution_ns,
        };
        target
            .lock()
            .map_err(|_| "v0.1.6 timer lock")?
            .push(elapsed_ns);
        Ok(())
    }
    fn pure_call_sum_ns(&self) -> AnyResult<u64> {
        Ok(*self.pure_call_ns.lock().map_err(|_| "v0.1.6 timer lock")?)
    }
    fn samples(&self) -> AnyResult<(Vec<u64>, Vec<u64>)> {
        Ok((
            self.commit_ns
                .lock()
                .map_err(|_| "v0.1.6 timer lock")?
                .clone(),
            self.execution_ns
                .lock()
                .map_err(|_| "v0.1.6 timer lock")?
                .clone(),
        ))
    }
}

#[derive(Clone, Copy)]
enum StageKind {
    Commit,
    Execution,
}

/// Publication and execution sample statistics: count, median, min and max.
fn sample_receipt(name: &str, samples: &[u64]) -> String {
    if samples.is_empty() {
        return format!("{{\"{name}_samples\":0}}");
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2];
    format!(
        "{{\"{name}_samples\":{},\"{name}_median_ns\":{median},\"{name}_min_ns\":{},\"{name}_max_ns\":{}}}",
        sorted.len(),
        sorted[0],
        sorted[sorted.len() - 1]
    )
}

pub(crate) struct Worker {
    pub(crate) index: usize,
    pub(crate) branch: BranchId,
    /// The live lease, or `None` for a branch whose lease was already released
    /// (the branch topology's trunk keeps its published commit list).
    session: Option<WorkspaceId>,
    pub(crate) branch_salt: String,
    pub(crate) branch_tag: u64,
    pub(crate) cycle_start: usize,
    pub(crate) mount: String,
    pub(crate) commits: Vec<CommitId>,
    pub(crate) roots: Vec<layerfs_content::ObjectId>,
    /// The live mode of every directory the stage helpers created on this
    /// branch, as measured by the helper itself. The independent oracle
    /// declares the observed mode rather than an assumed runtime default.
    pub(crate) created_directory_modes: BTreeMap<String, u32>,
}

pub(crate) struct Runtime {
    client: Arc<Client>,
    pub(crate) store: Arc<LayerStackStore>,
    container: ContainerId,
    root: PathBuf,
    pub(crate) case_id: String,
    pub(crate) plan: RunPlan,
    pub(crate) shared: Arc<Shared>,
    budget: Arc<Mutex<Budget>>,
    pub(crate) genesis: layerfs_layerstack_store::LayerId,
    pub(crate) genesis_root: layerfs_content::ObjectId,
}

impl Runtime {
    pub(crate) fn deadline(&self) -> AnyResult<()> {
        self.budget
            .lock()
            .map_err(|_| "v0.1.6 budget lock".into())
            .and_then(|budget| budget.check("deadline"))
    }
    fn elapsed(&self) -> u64 {
        self.budget
            .lock()
            .map(|budget| super::elapsed_ns(budget.started))
            .unwrap_or(0)
    }
}

// ------------------------------------------------------------------ helpers

fn helper_argv(
    tier: &str,
    seed: u8,
    cycle: usize,
    branch_tag: u64,
    branch_salt: &str,
    stage: Stage,
) -> Vec<OsString> {
    vec![
        OsString::from("/usr/local/bin/fs-benchmark-workload"),
        OsString::from("v016-m1-stage"),
        OsString::from(tier),
        OsString::from(seed.to_string()),
        OsString::from(cycle.to_string()),
        OsString::from(branch_tag.to_string()),
        OsString::from(branch_salt),
        OsString::from((stage as usize).to_string()),
    ]
}

fn parse_helper_receipt(text: &str) -> AnyResult<(BTreeMap<String, String>, usize)> {
    let mut rows = BTreeMap::new();
    let mut evidence = 0usize;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key == "m1_evidence" {
            evidence += 1;
            continue;
        }
        rows.insert(key.to_owned(), value.to_owned());
    }
    Ok((rows, evidence))
}

fn expect_counter(rows: &BTreeMap<String, String>, name: &str) -> AnyResult<usize> {
    let value = rows
        .get(&format!("m1_observed_{name}"))
        .ok_or_else(|| format!("v0.1.6 helper receipt is missing {name}"))?;
    Ok(value.parse::<usize>()?)
}

/// The live modes of the directories one stage created, measured by the helper
/// and reported as `path:mode` pairs. An empty list means the stage created no
/// directory; a malformed entry is a hard error.
fn parse_created_directory_modes(rows: &BTreeMap<String, String>) -> AnyResult<Vec<(String, u32)>> {
    let raw = rows
        .get("m1_created_directory_modes")
        .ok_or("v0.1.6 helper receipt is missing m1_created_directory_modes")?;
    let mut parsed = Vec::new();
    if raw.is_empty() {
        return Ok(parsed);
    }
    for row in raw.split(',') {
        let (path, mode) = row
            .rsplit_once(':')
            .ok_or_else(|| format!("v0.1.6 created-directory row {row}"))?;
        if path.is_empty() {
            return Err("v0.1.6 created-directory path".into());
        }
        parsed.push((path.to_owned(), u32::from_str_radix(mode, 8)?));
    }
    Ok(parsed)
}

/// The helper's own counters must equal the declared per-stage table, including
/// the POSIX/SDK split of the sixteen stage-2 targets: the helper names the
/// fourteen POSIX targets and the host's two SDK calls complete the declared
/// sixteen.
fn check_stage_counters(rows: &BTreeMap<String, String>, stage: Stage) -> AnyResult<()> {
    for (name, expected) in stages::stage_counters(stage) {
        if stage == Stage::Edits && name == "main_edit_targets" {
            continue;
        }
        if stages::is_host_counter(stage, name) {
            continue;
        }
        let observed = expect_counter(rows, name)?;
        if observed != expected {
            return Err(format!(
                "v0.1.6 stage {stage:?} counter {name}: helper observed {observed}, declared {expected}"
            )
            .into());
        }
    }
    if stage == Stage::Edits {
        let observed = expect_counter(rows, "main_edit_targets")?;
        if observed != stages::STAGE2_POSIX_TARGETS {
            return Err(format!(
                "v0.1.6 POSIX stage-2 targets: helper observed {observed}, declared {}",
                stages::STAGE2_POSIX_TARGETS
            )
            .into());
        }
    }
    if rows.get("m1_oracle_reads").map(String::as_str) != Some("0") {
        return Err("v0.1.6 helper performed benchmark oracle reads".into());
    }
    Ok(())
}

fn expect_created(status: &layerfs_sdk::WorkspaceCommitStatus, step: usize) -> AnyResult<CommitId> {
    if status.presentation_failed {
        return Err(format!("v0.1.6 commit {step} published with failed FUSE presentation").into());
    }
    match &status.result {
        WorkspaceCommitResult::Created { commit_id, .. } => Ok(*commit_id),
        other => Err(format!("v0.1.6 commit {step} must be Created, observed {other:?}").into()),
    }
}

fn open_session(runtime: &Runtime, branch: BranchId, mount: &str) -> AnyResult<WorkspaceId> {
    Ok(runtime
        .client
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: branch,
            placement: WorkspacePlacement::Container {
                container_id: runtime.container.clone(),
                root: PathBuf::from(mount),
            },
            projection: Some(WorkspaceProjection::Fuse),
        })?
        .id)
}

/// The two ordered single-file SDK range edits of stage 2. The public batch API
/// is same-file only, so this pair is never one atomic transaction.
fn sdk_stage2_edits(runtime: &Runtime, session: WorkspaceId, cycle: usize) -> AnyResult<()> {
    let plan = stages::stage2_plan(&runtime.plan.fixture, cycle)?;
    let mut calls = 0usize;
    for (path, operation) in plan {
        if !stages::is_sdk_row(&operation) {
            continue;
        }
        let (target, kind, offset) =
            stages::sdk_edit_target(&runtime.plan.fixture, cycle, &operation)?;
        if target != path {
            return Err("v0.1.6 SDK edit target mismatch".into());
        }
        let len = runtime.plan.fixture.len(&target)?;
        let (start, delete_len, replacement_len) = if kind == "insert" {
            (offset, 0u64, 256u64)
        } else {
            (offset, 256u64, 0u64)
        };
        if start + delete_len > len {
            return Err("v0.1.6 SDK edit bounds".into());
        }
        let bytes = if replacement_len == 0 {
            Vec::new()
        } else {
            let content = workload_source::dedup_workloads::content(
                v016::FAMILY_MIXED,
                "v016-sdk-stage2",
                runtime.plan.seed,
                cycle * 2 + calls,
                &format!("{kind}-c{cycle}"),
                replacement_len,
            )?;
            let mut out = Vec::new();
            content.write_to(&mut out)?;
            out
        };
        let started = Instant::now();
        runtime
            .client
            .edit_workspace_file_range(WorkspaceFileRangeEdit {
                workspace_id: session,
                path: target.clone(),
                start,
                delete_len,
                replacement: WorkspaceFileReplacement::Inline(bytes),
            })?;
        let elapsed = super::elapsed_ns(started);
        runtime.shared.add_pure_call(elapsed)?;
        calls += 1;
        emit(
            "v016-sdk-range-edit",
            &[
                ("case", quote(&runtime.case_id)),
                ("cycle", cycle.to_string()),
                ("request", quote(kind)),
                ("path", quote(&target)),
                ("start", start.to_string()),
                ("delete_len", delete_len.to_string()),
                ("elapsed_ns", super::elapsed_ns(started).to_string()),
                (
                    "scope",
                    quote(
                        "public Client::edit_workspace_file_range; one of two ordered single-file calls",
                    ),
                ),
            ],
        );
    }
    if calls != stages::STAGE2_SDK_CALLS {
        return Err(format!("v0.1.6 SDK stage-2 call count {calls}").into());
    }
    runtime.shared.count_sdk(calls)?;
    Ok(())
}

struct StageOutcome {
    commit_id: CommitId,
    root: layerfs_content::ObjectId,
    commit_ns: u64,
    execution_ns: u64,
}

fn run_stage(
    runtime: &Runtime,
    worker: &mut Worker,
    cycle: usize,
    ordinal: usize,
    local_commit: usize,
) -> AnyResult<StageOutcome> {
    let stage = Stage::from_ordinal(ordinal)?;
    let argv = helper_argv(
        runtime.plan.mixed.tier,
        runtime.plan.seed,
        cycle,
        worker.branch_tag,
        &worker.branch_salt,
        stage,
    );
    let started = Instant::now();
    let output = match super::execute(
        runtime.client.as_ref(),
        worker.session.ok_or("v0.1.6 stage session")?,
        argv.clone(),
    ) {
        Ok(output) => output,
        Err(error) => {
            emit(
                "v016-stage-failure",
                &[
                    ("case", quote(&runtime.case_id)),
                    ("branch", worker.index.to_string()),
                    ("cycle", cycle.to_string()),
                    ("stage", (stage as usize).to_string()),
                    ("phase", quote("posix-helper-execution")),
                    (
                        "argv",
                        format!(
                            "[{}]",
                            argv.iter()
                                .map(|value| quote(&value.to_string_lossy()))
                                .collect::<Vec<_>>()
                                .join(",")
                        ),
                    ),
                    ("error", quote(&error.to_string())),
                ],
            );
            return Err(error);
        }
    };
    let execution_ns = super::elapsed_ns(started);
    let text = String::from_utf8(
        output
            .chunks
            .iter()
            .flat_map(|chunk| chunk.bytes.iter().copied())
            .collect(),
    )?;
    let (rows, evidence) = parse_helper_receipt(&text)?;
    let created_modes = match parse_created_directory_modes(&rows) {
        Ok(created) => created,
        Err(error) => {
            emit(
                "v016-stage-receipt-failure",
                &[
                    ("case", quote(&runtime.case_id)),
                    ("branch", worker.index.to_string()),
                    ("cycle", cycle.to_string()),
                    ("stage", (stage as usize).to_string()),
                    ("error", quote(&error.to_string())),
                    ("helper_receipt", quote(&text)),
                ],
            );
            return Err(error);
        }
    };
    for (path, mode) in &created_modes {
        if mode & !0o1777u32 != 0 {
            return Err(format!("v0.1.6 created directory {path} carries {mode:o}").into());
        }
        worker.created_directory_modes.insert(path.clone(), *mode);
    }
    if let Err(error) = check_stage_counters(&rows, stage) {
        emit(
            "v016-stage-receipt-failure",
            &[
                ("case", quote(&runtime.case_id)),
                ("branch", worker.index.to_string()),
                ("cycle", cycle.to_string()),
                ("stage", (stage as usize).to_string()),
                ("error", quote(&error.to_string())),
                ("helper_receipt", quote(&text)),
            ],
        );
        return Err(error);
    }
    emit(
        "v016-stage",
        &[
            ("case", quote(&runtime.case_id)),
            ("branch", worker.index.to_string()),
            ("cycle", cycle.to_string()),
            ("local_commit", local_commit.to_string()),
            ("stage", (stage as usize).to_string()),
            ("execution_ns", execution_ns.to_string()),
            ("evidence_records", evidence.to_string()),
            ("helper_receipt", quote(&text)),
        ],
    );
    if stage == Stage::Edits {
        sdk_stage2_edits(runtime, worker.session.ok_or("v0.1.6 sdk session")?, cycle)?;
    }
    let commit_start = runtime.elapsed();
    let commit_started = Instant::now();
    let session = worker.session.ok_or("v0.1.6 commit session")?;
    let status = runtime
        .client
        .commit_workspace_session_with_status(session)?;
    let commit_ns = super::elapsed_ns(commit_started);
    let commit_id = expect_created(&status, local_commit)?;
    // The declared per-cycle counters count one published Commit and one POSIX
    // helper execution per stage, both observed here rather than in a receipt.
    bump_observed_counter("created_commits", 1)?;
    bump_observed_counter("posix_helper_executions", 1)?;
    let pinned = runtime.store.pin_branch(worker.branch)?;
    let commit_end = runtime.elapsed();
    runtime
        .shared
        .record(worker.index, commit_start, commit_end)?;
    emit(
        "v016-commit",
        &[
            ("case", quote(&runtime.case_id)),
            ("branch", worker.index.to_string()),
            ("branch_id", quote(&worker.branch.to_string())),
            ("local_commit", local_commit.to_string()),
            ("cycle", cycle.to_string()),
            ("stage", (stage as usize).to_string()),
            ("commit_id", quote(&commit_id.to_string())),
            ("root", quote(&pinned.root.to_string())),
            ("commit_ns", commit_ns.to_string()),
            ("execution_ns", execution_ns.to_string()),
            ("outcome", quote("Created")),
            ("presentation_failed", "false".into()),
        ],
    );
    runtime.shared.add(&stages::stage_counters(stage))?;
    add_observed_counters(&stages::stage_counters(stage))?;
    runtime.shared.add_pure_call(execution_ns)?;
    runtime.shared.add_pure_call(commit_ns)?;
    runtime
        .shared
        .record_stage(StageKind::Execution, execution_ns)?;
    runtime.shared.record_stage(StageKind::Commit, commit_ns)?;
    Ok(StageOutcome {
        commit_id,
        root: pinned.root,
        commit_ns,
        execution_ns,
    })
}

fn run_worker_commits(
    runtime: &Runtime,
    worker: &mut Worker,
    first: usize,
    last: usize,
) -> AnyResult<()> {
    for local_commit in first..=last {
        runtime.deadline()?;
        let offset = local_commit - 1;
        let cycle = worker.cycle_start + offset / stages::STAGES;
        let ordinal = offset % stages::STAGES + 1;
        let _ = runtime.plan.concurrent() && worker.index == 0 && local_commit == DISCARD_COMMIT;
        let outcome = run_stage(runtime, worker, cycle, ordinal, local_commit)?;
        worker.commits.push(outcome.commit_id);
        worker.roots.push(outcome.root);
    }
    Ok(())
}

// ------------------------------------------------------------------ witness

/// Every commit one registered M1 case is required to create, given the number
/// of live workspaces the schedule uses.
pub(crate) fn declared_commits_for(mixed: MixedCase, workers: usize) -> usize {
    if mixed.topology == Topology::Branch {
        TRUNK_COMMITS + (workers - 1) * mixed.k
    } else {
        workers * mixed.k
    }
}

/// The declared operation counters of the selected invocation, as observed by
/// the stage helpers.
static OBSERVED: std::sync::OnceLock<Mutex<BTreeMap<String, usize>>> = std::sync::OnceLock::new();

fn observed_counters_cell() -> &'static Mutex<BTreeMap<String, usize>> {
    OBSERVED.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// The pristine Layer root of the selected invocation, published for the graph
/// proof so the initial state is part of the retained-root cardinality.
static GENESIS_ROOT: std::sync::OnceLock<layerfs_content::ObjectId> = std::sync::OnceLock::new();

pub(crate) fn publish_genesis_root(root: layerfs_content::ObjectId) {
    let _ = GENESIS_ROOT.set(root);
}

pub(crate) fn v016_genesis_root() -> Option<layerfs_content::ObjectId> {
    GENESIS_ROOT.get().copied()
}

pub(crate) fn observed_counters() -> AnyResult<BTreeMap<String, usize>> {
    Ok(observed_counters_cell()
        .lock()
        .map_err(|_| "v0.1.6 counter lock")?
        .clone())
}

/// One declared cycle counter that no stage receipt carries: the host itself
/// counts the Commits it published and the helper executions it launched.
fn bump_observed_counter(name: &str, delta: usize) -> AnyResult<()> {
    let mut counters = observed_counters_cell()
        .lock()
        .map_err(|_| "v0.1.6 observed counters lock")?;
    let entry = counters.entry(name.to_owned()).or_default();
    *entry = entry.checked_add(delta).ok_or("v0.1.6 counter overflow")?;
    Ok(())
}

fn add_observed_counters(rows: &BTreeMap<&'static str, usize>) -> AnyResult<()> {
    let mut counters = observed_counters_cell()
        .lock()
        .map_err(|_| "v0.1.6 counter lock")?;
    for (name, value) in rows {
        *counters.entry((*name).to_owned()).or_default() += *value;
    }
    Ok(())
}

/// The witness path A mutates and discards: a refresh-pool member of cohort 1
/// that neither worker's stage 1 rewrites in this cycle.
pub(crate) fn discard_witness_path_for_oracle(fixture: &v016::LoadFixture) -> AnyResult<String> {
    let base = v016::REFRESH_FILES_PER_COHORT;
    if written_by_stage1(1, base) {
        return Err("v0.1.6 discard witness must survive cycle 1's stage 1".into());
    }
    fixture
        .roles
        .refresh_pool
        .get(base)
        .cloned()
        .ok_or_else(|| "v0.1.6 discard witness pool".into())
}

/// Whether cycle `cycle`'s stage 1 writes refresh-pool index `index`.
fn written_by_stage1(cycle: usize, index: usize) -> bool {
    let Ok(cohort) = v016::cohort_for_cycle(cycle) else {
        return true;
    };
    let base = cohort * v016::REFRESH_FILES_PER_COHORT;
    (base..base + v016::REFRESH_FILES_PER_COHORT).contains(&index)
}

fn discard_witness_bytes(runtime: &Runtime) -> AnyResult<String> {
    let content = workload_source::dedup_workloads::content(
        v016::FAMILY_MIXED,
        "v016-discard",
        runtime.plan.seed,
        0,
        "discarded-mutation",
        256,
    )?;
    let mut bytes = Vec::new();
    content.write_to(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

// ---------------------------------------------------------------- execution

fn start_sessions(runtime: &Runtime, count: usize) -> AnyResult<Vec<Worker>> {
    let mixed = runtime.plan.mixed;
    let mut workers = Vec::with_capacity(count);
    for index in 0..count {
        let branch = runtime.client.fork_branch(
            EntityName::new(format!("v016-{}-{index}", runtime.case_id))?,
            LocalForkSource::Layer {
                layer_id: runtime.genesis,
            },
        )?;
        let mount = mount_for(mixed.topology, index)?;
        let session = open_session(runtime, branch, &mount)?;
        emit(
            "v016-worker",
            &[
                ("case", quote(&runtime.case_id)),
                ("branch", index.to_string()),
                ("branch_id", quote(&branch.to_string())),
                ("mount", quote(&mount)),
                ("workspace_id", quote(&session.to_string())),
                ("per_worker_commits", runtime.plan.per_worker.to_string()),
                ("role", quote(if index == 0 { "primary" } else { "peer" })),
            ],
        );
        workers.push(Worker {
            index,
            branch,
            session: Some(session),
            branch_salt: format!("branch{index}"),
            branch_tag: index as u64,
            cycle_start: 1,
            mount,
            commits: Vec::new(),
            roots: Vec::new(),
            created_directory_modes: BTreeMap::new(),
        });
    }
    Ok(workers)
}

fn mount_for(topology: Topology, index: usize) -> AnyResult<String> {
    Ok(match topology {
        Topology::Four => format!("/workspace/{}", ["a", "b", "c", "d"][index]),
        Topology::Concurrent => format!("/workspace/{}", ["a", "b"][index]),
        Topology::Branch => format!("/workspace/{}", ["trunk", "a", "b"][index]),
        Topology::Sequential => "/workspace/a".to_owned(),
    })
}

/// Run `first..=last` on every worker concurrently, released by a common
/// pre-stage barrier so the Commit requests really can overlap.
fn run_round(
    runtime: &Runtime,
    sessions: &mut [Worker],
    first: usize,
    last: usize,
) -> AnyResult<()> {
    let mut live: Vec<&mut Worker> = sessions
        .iter_mut()
        .filter(|worker| worker.session.is_some())
        .collect();
    if live.is_empty() {
        return Err("v0.1.6 M1 round has no live workspace".into());
    }
    let barrier = Arc::new(std::sync::Barrier::new(live.len()));
    std::thread::scope(|scope| -> AnyResult<()> {
        let mut handles = Vec::new();
        for worker in live.drain(..) {
            let barrier = Arc::clone(&barrier);
            handles.push(scope.spawn(move || -> std::result::Result<(), String> {
                barrier.wait();
                run_worker_commits(runtime, worker, first, last).map_err(|error| error.to_string())
            }));
        }
        let mut failure: Option<String> = None;
        for handle in handles {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => failure = failure.or(Some(error)),
                Err(_) => failure = failure.or(Some("v0.1.6 worker thread panicked".into())),
            }
        }
        match failure {
            Some(error) => Err(error.into()),
            None => Ok(()),
        }
    })
}

/// The concurrent discard/reopen event at local commit 5: A exits its helper,
/// mutates the witness, and discards the session without a Commit; B keeps its
/// already published stage. A then reopens the same branch.
fn run_discard_event(runtime: &Runtime, sessions: &mut [Worker]) -> AnyResult<()> {
    let path = discard_witness_path_for_oracle(&runtime.plan.fixture)?;
    let marker = discard_witness_bytes(runtime)?;
    let index = sessions
        .iter()
        .position(|worker| worker.index == 0)
        .ok_or("v0.1.6 discard worker")?;
    let receipt = {
        let worker = &sessions[index];
        let argv = vec![
            OsString::from("/usr/local/bin/fs-benchmark-workload"),
            OsString::from("v016-m1-witness"),
            OsString::from(&path),
            OsString::from(&marker),
        ];
        let session = worker.session.ok_or("v0.1.6 witness session")?;
        let output = super::execute(runtime.client.as_ref(), session, argv)?;
        String::from_utf8(
            output
                .chunks
                .iter()
                .flat_map(|chunk| chunk.bytes.iter().copied())
                .collect(),
        )?
    };
    emit(
        "v016-discard-witness",
        &[
            ("case", quote(&runtime.case_id)),
            ("path", quote(&path)),
            ("marker_sha256", quote(&marker)),
            ("receipt", quote(&receipt)),
        ],
    );
    let worker = &mut sessions[index];
    let session = worker.session.ok_or("v0.1.6 discard session")?;
    runtime
        .client
        .end_workspace_session(session, EndWorkspaceMode::Discard)?;
    let mount = worker.mount.clone();
    let branch = worker.branch;
    worker.session = Some(open_session(runtime, branch, &mount)?);
    emit(
        "v016-session-reopen",
        &[
            ("case", quote(&runtime.case_id)),
            ("branch", "0".to_string()),
            ("branch_id", quote(&branch.to_string())),
            ("mount", quote(&mount)),
            (
                "mode",
                quote("discard-without-commit-then-reopen-same-branch"),
            ),
        ],
    );
    Ok(())
}

/// The branch topology's trunk: ten M1 commits on one workspace before forking.
fn run_trunk(runtime: &Runtime, sessions: &mut [Worker]) -> AnyResult<(BranchId, CommitId)> {
    let trunk = sessions.first_mut().ok_or("v0.1.6 trunk worker")?;
    run_worker_commits(runtime, trunk, 1, TRUNK_COMMITS)?;
    let fork_point = trunk
        .commits
        .get(TRUNK_FORK_COMMIT - 1)
        .copied()
        .ok_or("v0.1.6 trunk fork commit")?;
    emit(
        "v016-trunk",
        &[
            ("case", quote(&runtime.case_id)),
            ("branch_id", quote(&trunk.branch.to_string())),
            ("commits", trunk.commits.len().to_string()),
            ("fork_point", quote(&fork_point.to_string())),
        ],
    );
    Ok((trunk.branch, fork_point))
}

/// Fork the children from trunk commit 5 and give them their own live workspace
/// with local cycle numbering continuing at 2.
fn start_children(
    runtime: &Runtime,
    trunk_branch: BranchId,
    fork_point: CommitId,
    total: usize,
) -> AnyResult<Vec<Worker>> {
    let mut workers = Vec::with_capacity(total.saturating_sub(1));
    for index in 1..total {
        let branch = runtime.client.fork_branch(
            EntityName::new(format!("{}-child-{}", runtime.case_id, index))?,
            LocalForkSource::Branch {
                branch_id: trunk_branch,
                commit_id: fork_point,
            },
        )?;
        let mount = mount_for(runtime.plan.mixed.topology, index)?;
        let session = open_session(runtime, branch, &mount)?;
        emit(
            "v016-fork",
            &[
                ("case", quote(&runtime.case_id)),
                ("branch", index.to_string()),
                ("branch_id", quote(&branch.to_string())),
                ("source_branch", quote(&trunk_branch.to_string())),
                ("source_commit", quote(&fork_point.to_string())),
                ("mount", quote(&mount)),
                ("workspace_id", quote(&session.to_string())),
                ("inherited_ancestry", TRUNK_FORK_COMMIT.to_string()),
                ("cycle_start", "2".to_string()),
                ("per_worker_commits", runtime.plan.per_worker.to_string()),
            ],
        );
        workers.push(Worker {
            index,
            branch,
            session: Some(session),
            branch_salt: format!("branch{index}"),
            branch_tag: index as u64,
            cycle_start: 2,
            mount,
            commits: Vec::new(),
            roots: Vec::new(),
            created_directory_modes: BTreeMap::new(),
        });
    }
    Ok(workers)
}

fn close_sessions(runtime: &Runtime, sessions: &[Worker]) -> AnyResult<()> {
    for worker in sessions {
        let Some(session) = worker.session else {
            continue;
        };
        runtime
            .client
            .end_workspace_session(session, EndWorkspaceMode::Clean)?;
    }
    if runtime.client.active_workspace_count()? != 0
        || runtime.client.active_execution_count()? != 0
    {
        return Err("v0.1.6 owned runtime retained after End".into());
    }
    Ok(())
}

fn report_heads(runtime: &Runtime, sessions: &[Worker]) -> AnyResult<()> {
    let mut total = 0usize;
    for worker in sessions {
        let pinned = runtime.store.pin_branch(worker.branch)?;
        total += worker.commits.len();
        emit(
            "v016-branch-head",
            &[
                ("case", quote(&runtime.case_id)),
                ("branch", worker.index.to_string()),
                ("branch_id", quote(&worker.branch.to_string())),
                ("local_commits", worker.commits.len().to_string()),
                (
                    "head",
                    quote(&format!("{:?}", pinned.branch.head_commit_id)),
                ),
                ("root", quote(&pinned.root.to_string())),
                (
                    "commit_ids",
                    format!(
                        "[{}]",
                        worker
                            .commits
                            .iter()
                            .map(|id| quote(&id.to_string()))
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                ),
            ],
        );
    }
    let mixed = runtime.plan.mixed;
    let declared = runtime.plan.declared_commits();
    if total != declared {
        return Err(
            format!("v0.1.6 observed Created commits {total} != declared {declared}").into(),
        );
    }
    if total != mixed.topology.total_commits(mixed.k) {
        return Err(format!(
            "v0.1.6 observed Created commits {total} != topology {}",
            mixed.topology.total_commits(mixed.k)
        )
        .into());
    }
    emit(
        "v016-counters",
        &[
            ("case", quote(&runtime.case_id)),
            (
                "counters",
                format!(
                    "{{{}}}",
                    runtime
                        .shared
                        .counters
                        .lock()
                        .map_err(|_| "v0.1.6 counter lock")?
                        .iter()
                        .map(|(key, value)| format!("{}:{value}", quote(key)))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            ),
            (
                "sdk_range_edit_calls",
                runtime
                    .shared
                    .sdk_calls
                    .lock()
                    .map(|value| value.to_string())
                    .unwrap_or_else(|_| "null".into()),
            ),
            (
                "observed_overlap_ns",
                runtime.shared.overlap_ns()?.to_string(),
            ),
            (
                "overlap_rounds",
                runtime.shared.overlap_rounds()?.to_string(),
            ),
            (
                "concurrency_claim",
                quote(
                    if runtime.plan.concurrent() || mixed.topology == Topology::Four {
                        "observed-overlap-required"
                    } else {
                        "not-a-concurrent-topology"
                    },
                ),
            ),
        ],
    );
    Ok(())
}

// ------------------------------------------------------------------- entry

/// Publish the selected-invocation timers: the complete pure product-call sum,
/// the per-Commit distribution and the declared counter identity.
fn report_timers(runtime: &Runtime) -> AnyResult<()> {
    let pure = runtime.shared.pure_call_sum_ns()?;
    let (commits, executions) = runtime.shared.samples()?;
    emit(
        "v016-timers",
        &[
            ("case", quote(&runtime.case_id)),
            ("pure_call_sum_ns", pure.to_string()),
            (
                "scope",
                quote(
                    "summed public stage-helper executions, SDK range edits and full-status Commits; excludes preparation, Store acquisition, receipt publication and teardown",
                ),
            ),
            ("commit_receipt", sample_receipt("commit", &commits)),
            ("execution_receipt", sample_receipt("execution", &executions)),
        ],
    );
    println!("{{\"pure_call_sum_ns\":{pure}}}");
    Ok(())
}

/// Run the complete selected M1 invocation. `verification` selects the
/// independent proof route after the schedule, never a reduced schedule.
pub(crate) fn run_case(
    root: &Path,
    case: &Case,
    seed: u8,
    mode: &str,
    container: ContainerId,
    verification: bool,
) -> AnyResult<()> {
    if !matches!(mode, "performance" | "verify") {
        return Err("invalid v0.1.6 M1 mode".into());
    }
    let mixed = plan_of(case)?;
    let tier = tier_of(mixed.tier)?;
    let fixture = v016::load_fixture(tier, seed)?;
    let workers = match mixed.topology {
        Topology::Branch => 3,
        other => other.branches(),
    };
    // The performance allowance is the roadmap's declared complete-command
    // allowance minus its three-second cleanup reserve. Verification replays
    // the same schedule and then proves it, so it carries its own wider cap and
    // never inherits the performance gate.
    let declared_allowance_seconds = mixed.watchdog_seconds().saturating_sub(3);
    let limit_ns = match std::env::var("LAYERFS_BENCH_PRODUCT_TIMEOUT_SECONDS") {
        Ok(seconds) if !verification => seconds
            .parse::<u64>()?
            .checked_mul(1_000_000_000)
            .filter(|value| *value > 0)
            .ok_or("invalid product timeout")?,
        Ok(_) | Err(std::env::VarError::NotPresent) => declared_allowance_seconds
            .max(if verification { 600 } else { 0 })
            .saturating_mul(1_000_000_000),
        Err(error) => return Err(error.into()),
    };
    let path = root.join("store.sqlite");
    let store = Arc::new(LayerStackStore::connect(&path)?);
    let binding = super::workspace_bench::sample_binding(root, &container)?;
    let client = Arc::new(super::workspace_bench::sample_client(
        store.clone(),
        &binding,
    )?);
    let pristine = store.pin_branch(
        std::fs::read_to_string(root.join("branch-id"))?
            .trim()
            .parse()?,
    )?;
    let genesis = pristine.branch.base_layer_id;
    let genesis_root = pristine.root;
    let runtime = Runtime {
        client,
        store: store.clone(),
        container,
        root: root.to_path_buf(),
        case_id: case.id.clone(),
        plan: RunPlan {
            mixed,
            seed,
            fixture,
            per_worker: mixed.k,
            workers,
        },
        shared: Arc::new(Shared::default()),
        budget: Arc::new(Mutex::new(Budget {
            started: Instant::now(),
            limit_ns,
        })),
        genesis,
        genesis_root,
    };
    emit(
        "v016-m1-start",
        &[
            ("case", quote(&case.id)),
            ("family", quote(case.family)),
            ("tier", quote(mixed.tier)),
            ("seed", seed.to_string()),
            ("mode", quote(mode)),
            ("local_commits_per_worker", mixed.k.to_string()),
            ("workers", workers.to_string()),
            (
                "declared_total_commits",
                mixed.topology.total_commits(mixed.k).to_string(),
            ),
            ("declared_roots", mixed.topology.roots(mixed.k).to_string()),
            (
                "declared_longest_ancestry",
                mixed.topology.longest_ancestry(mixed.k).to_string(),
            ),
            ("extended", mixed.extended.to_string()),
            (
                "extended_watchdog_seconds",
                mixed.watchdog_seconds().to_string(),
            ),
            (
                "performance_mode",
                quote(if mixed.exhaustive_verify_only() {
                    "N/A: verify-only exhaustive replay"
                } else {
                    "selected"
                }),
            ),
            (
                "declared_live_envelope",
                format!(
                    "{}/{}",
                    runtime.plan.fixture.tier.max_paths(),
                    runtime.plan.fixture.tier.max_bytes()
                ),
            ),
        ],
    );
    let mut sessions = if runtime.plan.branch_topology() {
        // The trunk holds the only lease until it has published commit 10; the
        // children fork from trunk commit 5 afterwards with their own leases.
        start_sessions(&runtime, 1)?
    } else {
        start_sessions(&runtime, workers)?
    };
    let outcome = (|| -> AnyResult<()> {
        publish_genesis_root(runtime.genesis_root);
        if runtime.plan.branch_topology() {
            let (trunk_branch, fork_point) = run_trunk(&runtime, &mut sessions)?;
            close_sessions(&runtime, &sessions)?;
            let trunk = sessions.remove(0);
            let mut children = start_children(&runtime, trunk_branch, fork_point, workers)?;
            let last = runtime.plan.per_worker;
            run_round(&runtime, &mut children, 1, last)?;
            // The trunk keeps its own published commit/root list for the graph
            // proof even though its lease was released before the fork.
            let mut trunk_record = trunk;
            trunk_record.session = None;
            children.insert(0, trunk_record);
            sessions = children;
        } else if runtime.plan.concurrent() {
            let half = runtime.plan.per_worker / 2;
            run_round(&runtime, &mut sessions, 1, half)?;
            run_discard_event(&runtime, &mut sessions)?;
            run_round(&runtime, &mut sessions, half + 1, runtime.plan.per_worker)?;
        } else {
            let last = runtime.plan.per_worker;
            run_round(&runtime, &mut sessions, 1, last)?;
        }
        report_heads(&runtime, &sessions)?;
        close_sessions(&runtime, &sessions)?;
        report_timers(&runtime)?;
        Ok(())
    })();
    if let Err(error) = outcome {
        for worker in &sessions {
            let Some(session) = worker.session else {
                continue;
            };
            let _ = runtime
                .client
                .end_workspace_session(session, EndWorkspaceMode::Discard);
        }
        return Err(error);
    }
    if verification {
        // The verification route runs inside the same selected process: the
        // workspace leases are released, and the graph and state proofs read the
        // published Store through a fresh container binding. Reopening the file
        // from a second process is unnecessary and would collide with this
        // invocation's live owner, so the independent proof runs where the
        // schedule's custody already is.
        let case_id = case.id.clone();
        super::v016_oracle::verify_case(
            &case_id,
            &mixed,
            seed,
            &runtime.store,
            &sessions,
            runtime.client.as_ref(),
        )?;
    }
    Ok(())
}
