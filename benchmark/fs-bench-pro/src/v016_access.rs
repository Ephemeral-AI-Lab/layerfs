// v0.1.6 `historical_access`: mount one selected retained state of a sealed
// producer and read it.
//
// An invocation never builds history. The producer was sealed once during
// fixture preparation; here the sealed Store is opened read-only in spirit —
// one fork of the declared branch at the declared ordinal, one live workspace,
// the declared full reads, no Commit. Performance is `N/A` for every one of the
// six cases, so the route refuses to run in performance mode rather than
// publishing a zero.
//
// The independent derivation is the producer's own declaration recomputed from
// the seed (`historical_access::read_targets`). The reader pass records what the
// mount returned; the verifier pass repeats the same declared reads and compares
// them, byte for byte, with that derivation.
use super::*;
use std::sync::Arc;
use workload_source::historical_access as access_source;
use workload_source::workspace_common::Case;

const MOUNT: &str = "/workspace/access";

pub(crate) fn is_access(case: &Case) -> bool {
    access_source::access_case(&case.id)
        .ok()
        .flatten()
        .is_some()
}

fn quote(value: &str) -> String {
    super::workspace_bench::quote(value)
}

fn emit(kind: &str, fields: &[(&str, String)]) {
    super::workspace_bench::emit(kind, fields)
}

fn plan(case: &Case) -> AnyResult<access_source::AccessCase> {
    access_source::access_case(&case.id)?
        .ok_or_else(|| format!("not a v0.1.6 historical_access case: {}", case.id).into())
}

/// Seal the producer of one access case into the prepared Store at `root`.
/// This runs during fixture preparation only; it is never part of an access
/// invocation.
pub(crate) fn seal_producer(
    root: &Path,
    case: &Case,
    seed: u8,
    container: ContainerId,
) -> AnyResult<()> {
    let access = plan(case)?;
    let producer = access_source::producer_case(access)?;
    emit(
        "v016-access-seal",
        &[
            ("case", quote(&case.id)),
            ("producer", quote(&producer.id)),
            ("producer_family", quote(producer.family)),
            ("selected_ordinal", access.ordinal.to_string()),
            ("declared_roots", access.roots.to_string()),
            (
                "branch_role",
                quote(&match access_source::branch_name(access) {
                    Some(name) => name,
                    None => "<sealed-master-branch>".to_owned(),
                }),
            ),
            ("seed", seed.to_string()),
            (
                "scope",
                quote(
                    "one declared producer schedule published into the sealed history input; no access read is charged here",
                ),
            ),
        ],
    );
    match producer.family {
        workload_source::dedup_branch_history::FAMILY => super::workspace_bench::run_case(
            root,
            &root.join("input"),
            &producer,
            seed,
            "performance",
            container,
        ),
        workload_source::branch_development::FAMILY_ID => {
            super::v016_compact::run_case(root, &producer, seed, "performance", container, false)
        }
        other => {
            Err(format!("historical_access producer family {other} has no sealing route").into())
        }
    }
}

/// The producer branch of one access case, resolved from the sealed Store.
fn producer_branch(
    store: &LayerStackStore,
    root: &Path,
    access: access_source::AccessCase,
) -> AnyResult<BranchId> {
    let pristine = store.pin_branch(
        std::fs::read_to_string(root.join("branch-id"))?
            .trim()
            .parse()?,
    )?;
    match access_source::branch_name(access) {
        None => Ok(pristine.branch.id),
        Some(name) => {
            let stack = pristine.branch.layer_stack_id;
            let record = store
                .branch_by_name(stack, &EntityName::new(name.clone())?)?
                .ok_or_else(|| format!("historical_access producer branch absent: {name}"))?;
            Ok(record.id)
        }
    }
}

/// The commit at the declared ordinal of one branch. The ordinal counts the
/// branch's own local commits (an inherited ancestor prefix is excluded), so the
/// walk starts at the published head and steps back `local - ordinal` times.
fn selected_commit(
    store: &LayerStackStore,
    branch: BranchId,
    access: access_source::AccessCase,
) -> AnyResult<(CommitId, usize)> {
    let local = access_source::producer_local_commits(access)?;
    if access.ordinal == 0 || access.ordinal > local {
        return Err(format!(
            "historical_access selected ordinal {} outside the declared {local} local commits",
            access.ordinal
        )
        .into());
    }
    let pinned = store.pin_branch(branch)?;
    let mut cursor = pinned
        .branch
        .head_commit_id
        .ok_or("historical_access producer branch has no commit")?;
    for _ in 0..(local - access.ordinal) {
        let record = store
            .commit(cursor)?
            .ok_or("historical_access producer commit absent")?;
        cursor = record
            .parent_commit_id
            .ok_or("historical_access selected ordinal beyond the retained ancestry")?;
    }
    Ok((cursor, access.ordinal))
}

/// The complete retained commit count of the sealed Store: proof that the graph
/// the case declares is the graph it read.
fn retained_commits(
    store: &LayerStackStore,
    access: access_source::AccessCase,
) -> AnyResult<usize> {
    let mut cursor: Option<CommitId> = None;
    let mut total = 0usize;
    loop {
        let page = store.commit_record_page(cursor, 127)?;
        total += page.records.len();
        if total > access.roots {
            return Err(
                "historical_access sealed producer holds more commits than declared".into(),
            );
        }
        match page.continuation {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    Ok(total)
}

/// One declared read as the container helper reported it.
struct ObservedRead {
    path: String,
    declared: u64,
    bytes: u64,
    sha256: String,
    inode: (u64, u64),
    nlink: u64,
    payload: Vec<u8>,
}

struct ObservedPass {
    phase: String,
    reader_profile: String,
    pre_read: u64,
    commits: u64,
    reads: Vec<ObservedRead>,
    absent: Vec<String>,
}

fn parse_receipt(text: &str) -> AnyResult<ObservedPass> {
    let mut pass = ObservedPass {
        phase: String::new(),
        reader_profile: String::new(),
        pre_read: u64::MAX,
        commits: u64::MAX,
        reads: Vec::new(),
        absent: Vec::new(),
    };
    let mut current: Option<ObservedRead> = None;
    let mut status: Option<String> = None;
    let flush = |current: &mut Option<ObservedRead>, reads: &mut Vec<ObservedRead>| {
        if let Some(row) = current.take() {
            reads.push(row);
        }
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        fn row(current: &mut Option<ObservedRead>) -> AnyResult<&mut ObservedRead> {
            current
                .as_mut()
                .ok_or_else(|| "v0.1.6 access receipt target order".into())
        }
        match key {
            "access_status" => status = Some(value.to_owned()),
            "access_case" => {}
            "access_phase" => pass.phase = value.to_owned(),
            "access_reader_profile" => pass.reader_profile = value.to_owned(),
            "access_pre_read" => pass.pre_read = value.parse()?,
            "access_commits" => pass.commits = value.parse()?,
            "access_absent" => pass.absent.push(value.to_owned()),
            "access_target" => {
                flush(&mut current, &mut pass.reads);
                current = Some(ObservedRead {
                    path: value.to_owned(),
                    declared: 0,
                    bytes: 0,
                    sha256: String::new(),
                    inode: (0, 0),
                    nlink: 0,
                    payload: Vec::new(),
                });
            }
            "access_declared_bytes" => row(&mut current)?.declared = value.parse()?,
            "access_bytes" => row(&mut current)?.bytes = value.parse()?,
            "access_sha256" => row(&mut current)?.sha256 = value.to_owned(),
            "access_ino" => row(&mut current)?.inode.1 = value.parse()?,
            "access_dev" => row(&mut current)?.inode.0 = value.parse()?,
            "access_nlink" => row(&mut current)?.nlink = value.parse()?,
            "access_payload_hex" => {
                if value.len() % 2 != 0 {
                    return Err("v0.1.6 access receipt payload encoding".into());
                }
                row(&mut current)?.payload = (0..value.len())
                    .step_by(2)
                    .map(|index| u8::from_str_radix(&value[index..index + 2], 16))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
            }
            _ => {}
        }
    }
    flush(&mut current, &mut pass.reads);
    if status.as_deref() != Some("pass") {
        return Err("v0.1.6 access helper did not report pass".into());
    }
    if pass.pre_read != 0 || pass.commits != 0 {
        return Err("v0.1.6 access helper declared a pre-read or a commit".into());
    }
    Ok(pass)
}

/// One declared read pass inside the live mount. The helper never sees the
/// expected values; it reports what the mount returned.
fn read_pass(
    client: &Client,
    session: WorkspaceId,
    case_id: &str,
    seed: u8,
    phase: &str,
) -> AnyResult<ObservedPass> {
    let output = super::execute(
        client,
        session,
        vec![
            OsString::from("/usr/local/bin/fs-benchmark-workload"),
            OsString::from("v016-access-read"),
            OsString::from(case_id),
            OsString::from(seed.to_string()),
            OsString::from(phase),
        ],
    )?;
    let text = String::from_utf8(
        output
            .chunks
            .iter()
            .flat_map(|chunk| chunk.bytes.iter().copied())
            .collect(),
    )?;
    parse_receipt(&text)
}

/// Run the complete selected access invocation.
pub(crate) fn run_case(
    root: &Path,
    case: &Case,
    seed: u8,
    mode: &str,
    container: ContainerId,
    verification: bool,
) -> AnyResult<()> {
    let access = plan(case)?;
    if mode != "verify" {
        return Err(
            "historical_access is verify-only: performance is N/A, never zero and never PASS"
                .into(),
        );
    }
    if !verification {
        return Err("historical_access runs only in its declared verification mode".into());
    }
    let (derived, declared_absent) = access_source::read_targets(access, seed)?;
    let declared_bytes: u64 = derived.iter().map(|row| row.len).sum();
    if declared_bytes != access.declared_bytes {
        return Err("historical_access declared read bytes".into());
    }
    let store = Arc::new(LayerStackStore::connect(root.join("store.sqlite"))?);
    let binding = super::workspace_bench::sample_binding(root, &container)?;
    let client = Arc::new(super::workspace_bench::sample_client(
        store.clone(),
        &binding,
    )?);
    let branch = producer_branch(store.as_ref(), root, access)?;
    let commits = retained_commits(store.as_ref(), access)?;
    // `retained_graph_roots` is the producer's declared *reference* count
    // (pristine genesis plus one per Created Commit); the sealed Store holds one
    // row per distinct commit identity, and the convergent control's identical
    // children legitimately share that identity. The declared graph size is an
    // explicit input and the observed census is recorded beside it.
    if commits + 1 > access.roots {
        return Err(format!(
            "historical_access sealed producer holds {commits} commit rows, above the declared {} retained-root references",
            access.roots
        )
        .into());
    }
    let (commit, ordinal) = selected_commit(store.as_ref(), branch, access)?;
    let producer_head = store.pin_branch(branch)?.branch.head_commit_id;
    let selected = client.fork_branch(
        EntityName::new(format!("access-{}-{}", access.ordinal, case.id))?,
        LocalForkSource::Branch {
            branch_id: branch,
            commit_id: commit,
        },
    )?;
    emit(
        "v016-access-start",
        &[
            ("case", quote(&case.id)),
            ("family", quote(case.family)),
            ("seed", seed.to_string()),
            ("mode", quote(mode)),
            ("producer", quote(access.producer)),
            ("selected_ordinal", ordinal.to_string()),
            ("selected_commit", quote(&commit.to_string())),
            ("producer_branch", quote(&branch.to_string())),
            ("producer_head", quote(&format!("{producer_head:?}"))),
            ("retained_commit_rows", commits.to_string()),
            ("declared_root_references", access.roots.to_string()),
            (
                "collapsed_commit_identities",
                (access.roots - 1 - commits).to_string(),
            ),
            ("branch_role", quote(access.branch.of())),
            ("mount", quote(MOUNT)),
            ("reader_profile", quote(access_source::reader_profile())),
            ("pre_read", "false".into()),
            (
                "performance_mode",
                quote("N/A: access creates no commits and reports no latency distribution"),
            ),
            ("declared_full_read_bytes", declared_bytes.to_string()),
            (
                "declared_paths",
                format!(
                    "[{}]",
                    derived
                        .iter()
                        .map(|row| quote(&row.path))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            ),
        ],
    );
    let session = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: selected,
        placement: WorkspacePlacement::Container {
            container_id: container.clone(),
            root: PathBuf::from(MOUNT),
        },
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let outcome = (|| -> AnyResult<(ObservedPass, ObservedPass)> {
        let reader = read_pass(client.as_ref(), session.id, &case.id, seed, "reader")?;
        let verifier = read_pass(client.as_ref(), session.id, &case.id, seed, "verifier")?;
        Ok((reader, verifier))
    })();
    let passes = match outcome {
        Ok(passes) => passes,
        Err(error) => {
            let _ = client.end_workspace_session(session.id, EndWorkspaceMode::Discard);
            return Err(error);
        }
    };
    client.end_workspace_session(session.id, EndWorkspaceMode::Clean)?;
    if client.active_workspace_count()? != 0 || client.active_execution_count()? != 0 {
        return Err("historical_access owned runtime returned after End".into());
    }
    // No Commit was created: the producer's head and the sealed commit census
    // are exactly what they were before the invocation.
    let after_head = store.pin_branch(branch)?.branch.head_commit_id;
    let after_commits = retained_commits(store.as_ref(), access)?;
    if after_head != producer_head || after_commits != commits {
        return Err("historical_access invocation changed the sealed producer".into());
    }
    let mut problems: Vec<String> = Vec::new();
    let mut verified = 0u64;
    for (label, pass) in [("reader", &passes.0), ("verifier", &passes.1)] {
        if pass.phase != label || pass.reader_profile != access_source::reader_profile() {
            problems.push(format!("{label}: declared phase/reader profile"));
        }
        if pass.reads.len() != derived.len() || pass.absent != declared_absent {
            problems.push(format!(
                "{label}: observed {} reads and {} absences, declared {} and {}",
                pass.reads.len(),
                pass.absent.len(),
                derived.len(),
                declared_absent.len()
            ));
            continue;
        }
        for (row, target) in pass.reads.iter().zip(&derived) {
            if row.path != target.path || row.declared != target.len {
                problems.push(format!(
                    "{label}: {} declared {} bytes, target {} declared {}",
                    row.path, row.declared, target.path, target.len
                ));
                continue;
            }
            if row.bytes != target.len || row.payload.len() as u64 != target.len {
                problems.push(format!(
                    "{label}: {} returned {} bytes, declared {}",
                    row.path, row.bytes, target.len
                ));
                continue;
            }
            // The original derivation: the producer's own declaration,
            // recomputed on the host from the seed and the selected ordinal.
            if row.payload != target.content {
                problems.push(format!(
                    "{label}: {} bytes differ from the independent derivation",
                    row.path
                ));
            }
            let digest = workload_source::sdk_edit_common::sha256_hex(&row.payload);
            if digest != row.sha256
                || digest != workload_source::sdk_edit_common::sha256_hex(&target.content)
            {
                problems.push(format!("{label}: {} reported digest mismatch", row.path));
            }
            verified += row.bytes;
            emit(
                "v016-access-read",
                &[
                    ("case", quote(&case.id)),
                    ("phase", quote(label)),
                    ("path", quote(&row.path)),
                    ("bytes", row.bytes.to_string()),
                    ("sha256", quote(&row.sha256)),
                    ("inode", row.inode.1.to_string()),
                    ("device", row.inode.0.to_string()),
                    ("nlink", row.nlink.to_string()),
                    ("source", quote("live-fuse-mount")),
                ],
            );
        }
    }
    // The declared alias equivalence, on the alias-pair selection.
    if access.plan == access_source::ReadPlan::InodePair && passes.0.reads.len() == 2 {
        let rows = &passes.0.reads;
        if rows[0].inode != rows[1].inode || rows[0].nlink != 2 || rows[1].nlink != 2 {
            problems.push(format!(
                "alias equivalence: {} ino={:?} nlink={} vs {} ino={:?} nlink={}",
                rows[0].path,
                rows[0].inode,
                rows[0].nlink,
                rows[1].path,
                rows[1].inode,
                rows[1].nlink
            ));
        } else {
            emit(
                "v016-access-inode",
                &[
                    ("case", quote(&case.id)),
                    ("path", quote(&rows[0].path)),
                    ("alias", quote(&rows[1].path)),
                    ("shared_inode", rows[0].inode.1.to_string()),
                    ("shared_device", rows[0].inode.0.to_string()),
                    ("reference_count", rows[0].nlink.to_string()),
                    ("status", quote("equivalent")),
                ],
            );
        }
    }
    for path in &passes.0.absent {
        emit(
            "v016-access-absent",
            &[
                ("case", quote(&case.id)),
                ("path", quote(path)),
                ("status", quote("ENOENT")),
            ],
        );
    }
    if passes.0.reads.len() == passes.1.reads.len() {
        for (reader, verifier) in passes.0.reads.iter().zip(&passes.1.reads) {
            if reader.payload != verifier.payload
                || reader.sha256 != verifier.sha256
                || reader.inode != verifier.inode
            {
                problems.push(format!(
                    "{}: reader and verifier disagree on the repeated read",
                    reader.path
                ));
            }
        }
    }
    if verified != declared_bytes * 2 {
        problems.push(format!(
            "verified {verified} bytes across both passes, declared {}",
            declared_bytes * 2
        ));
    }
    if !problems.is_empty() {
        return Err(format!("historical_access verification: {}", problems.join("; ")).into());
    }
    emit(
        "v016-access-proof",
        &[
            ("case", quote(&case.id)),
            ("status", quote("pass")),
            ("producer", quote(access.producer)),
            ("selected_ordinal", ordinal.to_string()),
            ("selected_commit", quote(&commit.to_string())),
            ("retained_commit_rows", commits.to_string()),
            ("declared_root_references", access.roots.to_string()),
            ("verified_full_read_bytes", verified.to_string()),
            ("verified_paths", (derived.len() * 2).to_string()),
            ("reader_profile", quote(access_source::reader_profile())),
            ("commits_created", "0".into()),
            (
                "producer_head_unchanged",
                quote(&format!("{producer_head:?}")),
            ),
            (
                "scope",
                quote(
                    "one selected retained state of the sealed producer, one live workspace, the declared full reads executed twice inside the mount and compared byte for byte with the producer's own declaration, the declared alias equivalence or absence, and the unchanged sealed producer head and commit census",
                ),
            ),
            (
                "omissions",
                quote("producer construction latency and any exhaustive producer-graph census beyond the declared commit cardinality"),
            ),
        ],
    );
    Ok(())
}
