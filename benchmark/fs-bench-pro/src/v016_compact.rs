// v0.1.6 compact branch-control orchestrator and its independent proof.
//
// The two controls of `benchmark-families.md` §"Two compact branch controls" are
// fixture-S graph controls, not load-bearing M1 schedules: one live workspace at a
// time, one 256 B public SDK overwrite per local commit on the first medium file,
// and a declared fork graph (trunk10; A10 and B10 forked from trunk commit 5;
// the descendant control's C10 forked from A's local commit 5).
//
// Nothing here reads the mutated Store to decide what a commit should contain:
// every offset, payload and state comes from `workload_source::v016_stages` and
// `workload_source::branch_development`, which the fixture preparation and the
// register-time self-check read too.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use workload_source::branch_development as branch;
use workload_source::v016_compact as s;
use workload_source::v016_stages::{self as stages, CompactCase, CompactRole};
use workload_source::workspace_common::Case;

pub(crate) fn is_compact(case: &Case) -> bool {
    stages::compact_case(&case.id).ok().flatten().is_some()
}

fn plan_of(case: &Case) -> AnyResult<CompactCase> {
    stages::compact_case(&case.id)?
        .ok_or_else(|| format!("not a v0.1.6 compact control: {}", case.id).into())
}

fn quote(value: &str) -> String {
    super::workspace_bench::quote(value)
}

fn emit(kind: &str, fields: &[(&str, String)]) {
    super::workspace_bench::emit(kind, fields)
}

/// Cumulative wall accounting for one selected invocation, against the case's
/// own declared complete-command budget.
struct Budget {
    started: Instant,
    limit_ns: u64,
}

impl Budget {
    fn check(&self, phase: &str) -> AnyResult<()> {
        if super::elapsed_ns(self.started) > self.limit_ns {
            return Err(format!(
                "v0.1.6 compact complete-command deadline exceeded during {phase}"
            )
            .into());
        }
        Ok(())
    }
}

/// The published commits and roots of one branch, in local order.
struct BranchRun {
    branch: BranchId,
    mount: String,
    commits: Vec<CommitId>,
    roots: Vec<layerfs_content::ObjectId>,
    edit_ns: u64,
    commit_ns: u64,
}

struct Runtime {
    client: Arc<Client>,
    store: Arc<LayerStackStore>,
    case_id: String,
    compact: CompactCase,
    seed: u8,
    budget: Budget,
}

impl Runtime {
    fn deadline(&self) -> AnyResult<()> {
        self.budget.check("deadline")
    }
}

fn expect_created(status: &layerfs_sdk::WorkspaceCommitStatus, label: &str) -> AnyResult<CommitId> {
    if status.presentation_failed {
        return Err(format!("v0.1.6 compact commit {label} failed FUSE presentation").into());
    }
    match &status.result {
        WorkspaceCommitResult::Created { commit_id, .. } => Ok(*commit_id),
        other => {
            Err(format!("v0.1.6 compact commit {label} must be Created, observed {other:?}").into())
        }
    }
}

fn mount_of(role: CompactRole) -> String {
    format!("/workspace/{}", role.of())
}

/// Run one branch: `local` declared commits, each one ordered SDK overwrite and
/// one Created Commit, on its own live workspace.
#[allow(clippy::too_many_arguments, reason = "explicit schedule inputs")]
fn run_branch(
    runtime: &Runtime,
    container: &ContainerId,
    branch_id: BranchId,
    role: CompactRole,
    local: usize,
    inherited: Option<CommitId>,
) -> AnyResult<BranchRun> {
    let compact = runtime.compact;
    let mount = mount_of(role);
    let session = {
        let client = runtime.client.clone();
        let root = PathBuf::from(&mount);
        Ok::<WorkspaceId, Box<dyn std::error::Error>>(
            client
                .create_workspace_session(CreateWorkspaceSession {
                    branch_id,
                    placement: WorkspacePlacement::Container {
                        container_id: container.clone(),
                        root,
                    },
                    projection: Some(WorkspaceProjection::Fuse),
                })?
                .id,
        )?
    };
    emit(
        "v016-compact-branch",
        &[
            ("case", quote(&runtime.case_id)),
            ("role", quote(role.of())),
            ("branch_id", quote(&branch_id.to_string())),
            ("mount", quote(&mount)),
            ("workspace_id", quote(&session.to_string())),
            ("declared_local_commits", local.to_string()),
            (
                "inherited_commit",
                quote(
                    &inherited
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "genesis".into()),
                ),
            ),
        ],
    );
    let salt = stages::compact_salt(compact, role);
    let steps = stages::compact_steps(compact)
        .into_iter()
        .filter(|row| row.role == role)
        .take(local)
        .collect::<Vec<_>>();
    let mut run = BranchRun {
        branch: branch_id,
        mount: mount.clone(),
        commits: Vec::new(),
        roots: Vec::new(),
        edit_ns: 0,
        commit_ns: 0,
    };
    for step in &steps {
        runtime.deadline()?;
        let offset = stages::compact_offset(step.ancestral)?;
        let payload =
            stages::compact_payload(branch::FAMILY_ID, runtime.seed, step.ancestral, salt)?;
        let mut replacement =
            Vec::with_capacity(workload_source::dedup_workloads::BRANCH_REGION_LEN as usize);
        payload.write_to(&mut replacement)?;
        if replacement.len() as u64 != workload_source::dedup_workloads::BRANCH_REGION_LEN {
            return Err("v0.1.6 compact payload length".into());
        }
        let request = WorkspaceFileRangeEdit {
            workspace_id: session,
            path: s::s_medium_path(0),
            start: offset,
            delete_len: workload_source::dedup_workloads::BRANCH_REGION_LEN,
            replacement: WorkspaceFileReplacement::Inline(replacement),
        };
        let edit_started = Instant::now();
        runtime.client.edit_workspace_file_range(request)?;
        let edit_ns = super::elapsed_ns(edit_started);
        run.edit_ns = run.edit_ns.saturating_add(edit_ns);
        let commit_started = Instant::now();
        let status = runtime
            .client
            .commit_workspace_session_with_status(session)?;
        let commit_ns = super::elapsed_ns(commit_started);
        run.commit_ns = run.commit_ns.saturating_add(commit_ns);
        let commit_id = expect_created(&status, &format!("{}#{}", role.of(), step.local))?;
        let pinned = runtime.store.pin_branch(branch_id)?;
        emit(
            "v016-compact-commit",
            &[
                ("case", quote(&runtime.case_id)),
                ("role", quote(role.of())),
                ("local_commit", step.local.to_string()),
                ("ancestral_ordinal", step.ancestral.to_string()),
                ("offset", offset.to_string()),
                ("commit_id", quote(&commit_id.to_string())),
                ("root", quote(&pinned.root.to_string())),
                ("edit_ns", edit_ns.to_string()),
                ("commit_ns", commit_ns.to_string()),
            ],
        );
        run.commits.push(commit_id);
        run.roots.push(pinned.root);
    }
    runtime
        .client
        .end_workspace_session(session, EndWorkspaceMode::Clean)?;
    if run.commits.len() != local {
        return Err("v0.1.6 compact branch commit count".into());
    }
    Ok(run)
}

/// The declared state targets of the proof: the trunk's fork point and every
/// branch head, each proved by its own complete retained-namespace verification.
struct Target {
    label: String,
    role: CompactRole,
    local: usize,
    branch: BranchId,
    /// The head's medium-file content root, when the target is a live head.
    live: bool,
}

fn verify_compact(
    runtime: &Runtime,
    runs: &BTreeMap<CompactRole, BranchRun>,
    genesis_root: layerfs_content::ObjectId,
    root: &Path,
) -> AnyResult<()> {
    let compact = runtime.compact;
    let declared_commits = compact.total_commits();
    let mut wanted: Vec<CommitId> = Vec::new();
    let fork_points: BTreeMap<CompactRole, Option<CommitId>> = BTreeMap::from([
        (CompactRole::Trunk, None),
        (
            CompactRole::A,
            runs.get(&CompactRole::Trunk)
                .and_then(|trunk| trunk.commits.get(stages::COMPACT_FORK_COMMIT - 1).copied()),
        ),
        (
            CompactRole::B,
            runs.get(&CompactRole::Trunk)
                .and_then(|trunk| trunk.commits.get(stages::COMPACT_FORK_COMMIT - 1).copied()),
        ),
        (
            CompactRole::C,
            runs.get(&CompactRole::A)
                .and_then(|a| a.commits.get(stages::COMPACT_DESCENDANT_FORK - 1).copied()),
        ),
    ]);
    let mut observed_commits = 0usize;
    for (role, run) in runs {
        let pinned = runtime.store.pin_branch(run.branch)?;
        if run.commits.last().copied() != pinned.branch.head_commit_id {
            return Err(format!(
                "v0.1.6 compact branch {} head differs from the published head",
                role.of()
            )
            .into());
        }
        if run.roots.last().copied() != Some(pinned.root) {
            return Err("v0.1.6 compact branch root differs from the published root".into());
        }
        observed_commits += run.commits.len();
        wanted.extend(run.commits.iter().copied());
    }
    if observed_commits != declared_commits {
        return Err(format!(
            "v0.1.6 compact graph commits {observed_commits} != declared {declared_commits}"
        )
        .into());
    }
    let records = super::workspace_verify::commit_records(runtime.client.as_ref(), &wanted)?;
    let distinct: BTreeSet<CommitId> = wanted.iter().copied().collect();
    // `retained_graph_roots` counts initial/commit state *references*, never
    // distinct root ObjectIds (`benchmark-families.md` §dedup_branch_history):
    // the convergent control's two children make identical content changes, and
    // the product's content-addressed identity therefore collapses each pair of
    // their commits onto one identity. The distinct counts are recorded beside
    // the declared reference count instead of being asserted equal to it.
    if distinct.len() > declared_commits {
        return Err(format!(
            "v0.1.6 compact observed {} distinct commits above the declared {declared_commits}",
            distinct.len()
        )
        .into());
    }
    for (role, run) in runs {
        let mut previous = fork_points.get(role).copied().flatten();
        for commit in &run.commits {
            let record = records
                .get(commit)
                .ok_or("v0.1.6 compact retained commit record absent")?;
            if record.parent_commit_id != previous {
                return Err(format!(
                    "v0.1.6 compact parent edge mismatch on {} at commit {commit}: declared {previous:?} published {:?}",
                    role.of(),
                    record.parent_commit_id
                )
                .into());
            }
            previous = Some(*commit);
        }
    }
    // Longest ancestry, walked from every head through the published parent
    // edges to the genesis.
    let mut longest = 0usize;
    for run in runs.values() {
        let mut depth = 0usize;
        let mut cursor = run.commits.last().copied();
        while let Some(commit) = cursor {
            depth += 1;
            if depth > declared_commits {
                return Err("v0.1.6 compact ancestry walk does not terminate".into());
            }
            cursor = records
                .get(&commit)
                .ok_or("v0.1.6 compact ancestry record absent")?
                .parent_commit_id;
        }
        longest = longest.max(depth);
    }
    if longest != compact.longest_ancestry() {
        return Err(format!(
            "v0.1.6 compact longest ancestry {longest} != declared {}",
            compact.longest_ancestry()
        )
        .into());
    }
    let mut retained: BTreeSet<layerfs_content::ObjectId> = BTreeSet::new();
    retained.insert(genesis_root);
    for run in runs.values() {
        retained.extend(run.roots.iter().copied());
    }
    // One reference for the pristine genesis plus one for every Created Commit.
    let references = 1 + declared_commits;
    if references != compact.roots() {
        return Err(format!(
            "v0.1.6 compact retained-root references {references} != declared {}",
            compact.roots()
        )
        .into());
    }
    if retained.len() > references {
        return Err("v0.1.6 compact holds more roots than the declared references".into());
    }
    emit(
        "v016-compact-graph-proof",
        &[
            ("case", quote(&runtime.case_id)),
            ("created_commits", declared_commits.to_string()),
            ("distinct_commits", distinct.len().to_string()),
            ("retained_root_references", references.to_string()),
            ("distinct_roots", retained.len().to_string()),
            ("longest_ancestry", longest.to_string()),
            ("branches", compact.branches().to_string()),
            ("genesis_root", quote(&genesis_root.to_string())),
            (
                "reference_scope",
                quote(
                    "retained_graph_roots counts initial/commit state references, not distinct root ObjectIds; the convergent control's identical children may share one identity",
                ),
            ),
            (
                "scope",
                quote(
                    "every retained commit identity and parent edge through the public query surface after the schedule, the declared fork points, the declared retained-root cardinality and the longest ancestry walked from every head",
                ),
            ),
        ],
    );

    // Complete retained-namespace proof of the trunk's fork point and of every
    // branch head, each against its own declared state.
    let mut targets: Vec<Target> = vec![Target {
        label: "trunk-fork-point".into(),
        role: CompactRole::Trunk,
        local: stages::COMPACT_FORK_COMMIT,
        branch: runs[&CompactRole::Trunk].branch,
        live: false,
    }];
    for (role, run) in runs {
        targets.push(Target {
            label: format!("head-{}", role.of()),
            role: *role,
            local: run.commits.len(),
            branch: run.branch,
            live: true,
        });
    }
    let mut content_roots: BTreeMap<CompactRole, layerfs_content::ObjectId> = BTreeMap::new();
    for target in &targets {
        let entries =
            branch::compact_expected_at(runtime.seed, compact, target.role, target.local)?;
        let branch_id = if target.live {
            target.branch
        } else {
            // A retained non-head state is proved by forking it and verifying
            // the fork's own published head.
            let commit = runs[&target.role]
                .commits
                .get(target.local - 1)
                .copied()
                .ok_or("v0.1.6 compact retained state commit")?;
            runtime.client.fork_branch(
                EntityName::new(format!("v016-proof-{}-{}", target.role.of(), target.local))?,
                LocalForkSource::Branch {
                    branch_id: target.branch,
                    commit_id: commit,
                },
            )?
        };
        let evidence = root.join(format!("compact-{}", target.label));
        let snapshot = super::workspace_verify::verify(
            runtime.store.as_ref(),
            branch_id,
            &entries,
            &evidence,
        )?;
        let root_of = |path: &str| -> AnyResult<layerfs_content::ObjectId> {
            snapshot
                .file_roots
                .get(path)
                .copied()
                .ok_or_else(|| format!("v0.1.6 compact proof is missing {path}").into())
        };
        let medium = root_of(&s::s_medium_path(0))?;
        if target.live {
            content_roots.insert(target.role, medium);
        }
        let fixture = branch::fixture(
            &Case {
                id: runtime.case_id.clone(),
                family: branch::FAMILY_ID,
                tier: 10,
                kind: "v016-compact-control",
            },
            runtime.seed,
        )?;
        let initial = fixture
            .iter()
            .find(|entry| entry.path == s::s_medium_path(0))
            .ok_or("v0.1.6 compact fixture target")?;
        if let Some(declared) = super::workspace_verify::declared_content_root(initial)? {
            if declared == medium && target.local > 0 {
                return Err(format!(
                    "v0.1.6 compact state {} did not change the medium file",
                    target.label
                )
                .into());
            }
        }
        emit(
            "v016-compact-state-proof",
            &[
                ("case", quote(&runtime.case_id)),
                ("label", quote(&target.label)),
                ("role", quote(target.role.of())),
                ("local_commits", target.local.to_string()),
                ("branch_id", quote(&branch_id.to_string())),
                ("medium_content_root", quote(&medium.to_string())),
                (
                    "receipt",
                    quote(&format!("{:?}", snapshot.receipt)),
                ),
                (
                    "scope",
                    quote(
                        "complete retained namespace, every declared path type/mode/mtime, independently recomputed content roots and declared payload extents",
                    ),
                ),
            ],
        );
    }
    // The control's own content contract: the convergent children must publish
    // equal file-content IDs; the descendant control must salt B and C by branch
    // while every sibling head survives.
    let a = content_roots
        .get(&CompactRole::A)
        .copied()
        .ok_or("v0.1.6 compact branch A content root")?;
    let b = content_roots
        .get(&CompactRole::B)
        .copied()
        .ok_or("v0.1.6 compact branch B content root")?;
    let c = content_roots.get(&CompactRole::C).copied();
    match (compact.descendant, c) {
        (false, None) => {
            if a != b {
                return Err(
                    "v0.1.6 convergent children must publish equal file-content IDs".into(),
                );
            }
        }
        (true, Some(c)) => {
            if a == b || a == c || b == c {
                return Err("v0.1.6 descendant control must salt B/C by branch".into());
            }
        }
        _ => return Err("v0.1.6 compact branch content cardinality".into()),
    }
    emit(
        "v016-compact-content-proof",
        &[
            ("case", quote(&runtime.case_id)),
            (
                "class",
                quote(if compact.descendant {
                    "descendant-branch-salted"
                } else {
                    "convergent-shared-content"
                }),
            ),
            ("branch_a_root", quote(&a.to_string())),
            ("branch_b_root", quote(&b.to_string())),
            (
                "branch_c_root",
                quote(&c.map(|value| value.to_string()).unwrap_or_else(|| "absent".into())),
            ),
            (
                "scope",
                quote(
                    "the first medium file's published file-content ID on every branch head, compared across branches by the control's own declaration",
                ),
            ),
        ],
    );
    let counters = stages::compact_counters(compact);
    if counters.get("created_commits") != Some(&declared_commits)
        || counters.get("sdk_batch_calls") != Some(&0)
        || counters.get("posix_helper_executions") != Some(&0)
    {
        return Err("v0.1.6 compact counter table".into());
    }
    emit(
        "v016-compact-counter-proof",
        &[
            ("case", quote(&runtime.case_id)),
            (
                "counters",
                format!(
                    "{{{}}}",
                    counters
                        .iter()
                        .map(|(key, value)| format!("{}:{value}", quote(key)))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            ),
            ("observed_sdk_edit_calls", observed_commits.to_string()),
            (
                "scope",
                quote(
                    "one public single-file range edit per local commit and one Created Commit per edit, with no cross-file batch and no POSIX helper execution",
                ),
            ),
        ],
    );
    emit(
        "v016-compact-verification",
        &[
            ("case", quote(&runtime.case_id)),
            ("status", quote("pass")),
            ("created_commits", declared_commits.to_string()),
        ],
    );
    Ok(())
}

/// Run the complete selected compact-control invocation. `verification` selects
/// the independent proof after the schedule, never a reduced schedule.
pub(crate) fn run_case(
    root: &Path,
    case: &Case,
    seed: u8,
    mode: &str,
    container: ContainerId,
    verification: bool,
) -> AnyResult<()> {
    if !matches!(mode, "performance" | "verify") {
        return Err("invalid v0.1.6 compact mode".into());
    }
    let compact = plan_of(case)?;
    let declared_allowance_seconds = compact.watchdog_seconds().saturating_sub(3);
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
    let store = Arc::new(LayerStackStore::connect(root.join("store.sqlite"))?);
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
        client: client.clone(),
        store: store.clone(),
        case_id: case.id.clone(),
        compact,
        seed,
        budget: Budget {
            started: Instant::now(),
            limit_ns,
        },
    };
    emit(
        "v016-compact-start",
        &[
            ("case", quote(&case.id)),
            ("family", quote(case.family)),
            ("fixture", quote("S")),
            ("seed", seed.to_string()),
            ("mode", quote(mode)),
            ("branches", compact.branches().to_string()),
            (
                "declared_total_commits",
                compact.total_commits().to_string(),
            ),
            ("declared_roots", compact.roots().to_string()),
            (
                "declared_longest_ancestry",
                compact.longest_ancestry().to_string(),
            ),
            (
                "declared_watchdog_seconds",
                compact.watchdog_seconds().to_string(),
            ),
            (
                "fork_point",
                format!(
                    "trunk#{}{}",
                    stages::COMPACT_FORK_COMMIT,
                    if compact.descendant {
                        format!(";a#{}", stages::COMPACT_DESCENDANT_FORK)
                    } else {
                        String::new()
                    }
                ),
            ),
            (
                "declared_live_envelope",
                format!("{}/{}", 20, 4 * 1024 * 1024),
            ),
        ],
    );
    let outcome = (|| -> AnyResult<BTreeMap<CompactRole, BranchRun>> {
        let mut runs = BTreeMap::new();
        let publish = |role: CompactRole,
                       run: BranchRun,
                       runs: &mut BTreeMap<CompactRole, BranchRun>|
         -> AnyResult<()> {
            if runs.insert(role, run).is_some() {
                return Err("v0.1.6 compact branch published twice".into());
            }
            Ok(())
        };
        let trunk_branch = client.fork_branch(
            EntityName::new(stages::compact_branch_name(&case.id, CompactRole::Trunk))?,
            LocalForkSource::Layer { layer_id: genesis },
        )?;
        let trunk = run_branch(
            &runtime,
            &container,
            trunk_branch,
            CompactRole::Trunk,
            stages::COMPACT_TRUNK_COMMITS,
            None,
        )?;
        let fork_point = trunk
            .commits
            .get(stages::COMPACT_FORK_COMMIT - 1)
            .copied()
            .ok_or("v0.1.6 compact trunk fork point")?;
        publish(CompactRole::Trunk, trunk, &mut runs)?;
        let mut child_a: Option<BranchRun> = None;
        for role in compact.roles().into_iter().skip(1) {
            runtime.deadline()?;
            let (source_branch, source_commit) = if role == CompactRole::C {
                let a = child_a
                    .as_ref()
                    .ok_or("v0.1.6 compact descendant requires branch A")?;
                (
                    a.branch,
                    a.commits
                        .get(stages::COMPACT_DESCENDANT_FORK - 1)
                        .copied()
                        .ok_or("v0.1.6 compact descendant fork point")?,
                )
            } else {
                (trunk_branch, fork_point)
            };
            let branch_id = client.fork_branch(
                EntityName::new(stages::compact_branch_name(&case.id, role))?,
                LocalForkSource::Branch {
                    branch_id: source_branch,
                    commit_id: source_commit,
                },
            )?;
            emit(
                "v016-compact-fork",
                &[
                    ("case", quote(&case.id)),
                    ("role", quote(role.of())),
                    ("branch_id", quote(&branch_id.to_string())),
                    ("source_branch", quote(&source_branch.to_string())),
                    ("source_commit", quote(&source_commit.to_string())),
                    (
                        "inherited_ancestry",
                        compact.fork_ancestry(role).to_string(),
                    ),
                    ("salt", quote(stages::compact_salt(compact, role))),
                ],
            );
            let run = run_branch(
                &runtime,
                &container,
                branch_id,
                role,
                stages::COMPACT_CHILD_COMMITS,
                Some(source_commit),
            )?;
            if role == CompactRole::A {
                child_a = Some(BranchRun {
                    branch: run.branch,
                    mount: run.mount.clone(),
                    commits: run.commits.clone(),
                    roots: run.roots.clone(),
                    edit_ns: run.edit_ns,
                    commit_ns: run.commit_ns,
                });
            }
            publish(role, run, &mut runs)?;
        }
        runtime.deadline()?;
        if client.active_workspace_count()? != 0 || client.active_execution_count()? != 0 {
            return Err("v0.1.6 compact owned runtime retained after End".into());
        }
        Ok(runs)
    })();
    let runs = match outcome {
        Ok(runs) => runs,
        Err(error) => {
            let _ = client.active_workspace_count();
            return Err(error);
        }
    };
    let total: usize = runs.values().map(|run| run.commits.len()).sum();
    if total != compact.total_commits() {
        return Err(format!(
            "v0.1.6 compact observed Created commits {total} != declared {}",
            compact.total_commits()
        )
        .into());
    }
    let edit_ns: u64 = runs.values().map(|run| run.edit_ns).sum();
    let commit_ns: u64 = runs.values().map(|run| run.commit_ns).sum();
    emit(
        "v016-compact-counters",
        &[
            ("case", quote(&case.id)),
            ("created_commits", total.to_string()),
            ("sdk_range_edit_calls", total.to_string()),
            ("sdk_batch_calls", "0".into()),
            ("posix_helper_executions", "0".into()),
            (
                "commit_outcomes",
                format!(
                    "{{Created:{total},UpToDate:0,Busy:0,HeadMoved:0,presentation_failures:0}}"
                ),
            ),
        ],
    );
    emit(
        "v016-compact-timers",
        &[
            ("case", quote(&case.id)),
            ("pure_call_sum_ns", (edit_ns + commit_ns).to_string()),
            ("sdk_edit_sum_ns", edit_ns.to_string()),
            ("commit_sum_ns", commit_ns.to_string()),
            (
                "scope",
                quote(
                    "summed public single-file SDK range edits and full-status Commits; excludes preparation, Store acquisition, receipt publication and teardown",
                ),
            ),
        ],
    );
    println!("{{\"pure_call_sum_ns\":{}}}", edit_ns + commit_ns);
    if verification {
        verify_compact(&runtime, &runs, genesis_root, root)?;
    }
    Ok(())
}
