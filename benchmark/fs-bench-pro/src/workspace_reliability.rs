//! Public Workspace reliability proofs. No entry point is used by performance mode.
use super::*;
use crate::workload_source::{workspace_common as common, workspace_reliability as family};
use layerfs_layerstack_store::{
    arm_verification_store_fault, take_verification_store_fault_receipt, VerificationStoreFault,
};
use layerfs_sdk::{ExecutionId, SdkError, WorkspaceError};
use layerfs_workspace::{
    arm_verification_fault, take_verification_fault_receipt, VerificationFault,
};
use std::time::Duration;

fn record(kind: &str, value: impl std::fmt::Debug) {
    super::workspace_bench::emit(
        kind,
        &[(
            "detail",
            super::workspace_bench::quote(&format!("{value:?}")),
        )],
    );
}
thread_local! {static OBSERVED_OPERATION:std::cell::Cell<u64>=const {std::cell::Cell::new(0)};}
fn observed(client: &Client) -> AnyResult<()> {
    OBSERVED_OPERATION.with(|cursor| {
        let mut after = cursor.get();
        let result = super::workspace_bench::observed(client, &mut after);
        cursor.set(after);
        result
    })
}
fn physical_state(client: &Client, id: WorkspaceId, phase: &str) -> AnyResult<()> {
    let state = client.verification_workspace_state(id)?;
    super::workspace_bench::emit(
        "workspace-physical-spool",
        &[
            ("phase", super::workspace_bench::quote(phase)),
            (
                "allocated_bytes",
                state
                    .physical_spool_allocated_bytes
                    .map_or("null".into(), |n| n.to_string()),
            ),
            (
                "peak_bytes",
                state
                    .physical_spool_peak_bytes
                    .map_or("null".into(), |n| n.to_string()),
            ),
            (
                "observation_errors",
                state.physical_spool_observation_errors.to_string(),
            ),
            (
                "observation_count",
                state.physical_spool_observation_count.to_string(),
            ),
            (
                "precision",
                super::workspace_bench::quote("mutation-event-aggregate-allocation"),
            ),
            (
                "method",
                super::workspace_bench::quote("verification_workspace_state"),
            ),
        ],
    );
    let current = state
        .physical_spool_allocated_bytes
        .ok_or("physical spool current allocation unavailable")?;
    let peak = state
        .physical_spool_peak_bytes
        .ok_or("physical spool peak allocation unavailable")?;
    if state.physical_spool_observation_errors != 0
        || current > 2 * 1024 * 1024 * 1024
        || peak > 2 * 1024 * 1024 * 1024
    {
        return Err("physical spool observation/resource gate".into());
    }
    Ok(())
}
fn runtime_files() -> AnyResult<std::collections::BTreeSet<PathBuf>> {
    let root = std::env::temp_dir().join("layerfs-runtime");
    let mut result = std::collections::BTreeSet::new();
    if !root.exists() {
        return Ok(result);
    }
    let prefix = format!("{}-", std::process::id());
    let mut pending = std::fs::read_dir(root)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    while let Some(path) = pending.pop() {
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            for entry in std::fs::read_dir(path)? {
                pending.push(entry?.path());
            }
        } else {
            result.insert(path);
        }
    }
    Ok(result)
}
fn require_unmounted(container: &ContainerId, mount: &Path) -> AnyResult<()> {
    let path = mount.to_str().ok_or("mount path UTF8")?;
    let state = docker(container, &["findmnt", "-rn", "-M", path])?;
    if state.status.code() != Some(1) {
        return Err(format!("owned FUSE mount remains or observation failed: {state:?}").into());
    }
    record("mount-cleanup", path);
    Ok(())
}
fn integrity_error(error: &layerfs_layerstack_store::StoreError) -> bool {
    matches!(
        error,
        layerfs_layerstack_store::StoreError::Integrity(_)
            | layerfs_layerstack_store::StoreError::Core(
                layerfs_content::CoreError::MissingObject
                    | layerfs_content::CoreError::IdentityMismatch
                    | layerfs_content::CoreError::ChunkIdentityMismatch
            )
    )
}
fn argv(case: &common::Case, action: &str, n: u64) -> Vec<OsString> {
    vec![
        "/usr/local/bin/fs-benchmark-workload".into(),
        "workspace-reliability-workload".into(),
        case.id.clone().into(),
        action.into(),
        n.to_string().into(),
    ]
}
fn workload(
    client: &Client,
    id: WorkspaceId,
    case: &common::Case,
    action: &str,
    n: u64,
) -> AnyResult<layerfs_sdk::OutputPage> {
    let result = execute(client, id, argv(case, action, n));
    let observation = observed(client);
    let spool = super::workspace_bench::spool_observation("after-workload-before-commit");
    let out = result?;
    observation?;
    spool?;
    record(action, &out);
    physical_state(client, id, "after-workload")?;
    Ok(out)
}
fn live(
    client: &Client,
    id: WorkspaceId,
    case: &common::Case,
    state: &str,
    n: u64,
) -> AnyResult<()> {
    let result = execute(
        client,
        id,
        vec![
            "/usr/local/bin/fs-benchmark-workload".into(),
            "workspace-reliability-verify".into(),
            case.id.clone().into(),
            state.into(),
            n.to_string().into(),
        ],
    );
    let observation = observed(client);
    let out = result?;
    observation?;
    record("live-full-tree", out);
    physical_state(client, id, "live-checkpoint")?;
    Ok(())
}
fn canonical(
    store: &LayerStackStore,
    branch: BranchId,
    case: &common::Case,
    state: &str,
    n: u64,
) -> AnyResult<workspace_verify::SnapshotEvidence> {
    let pin = store.pin_branch(branch)?;
    let result =
        workspace_verify::verify_root(&pin.reader, pin.root, &family::expected(case, state, n)?)?;
    record("canonical-full-tree", &result);
    Ok(result)
}
fn created(client: &Client, id: WorkspaceId) -> AnyResult<CommitId> {
    let result = client.commit_workspace_session_with_status(id);
    let observation = observed(client);
    let status = result?;
    observation?;
    record("commit", &status);
    physical_state(client, id, "after-commit")?;
    super::workspace_bench::spool_observation("after-commit")?;
    match status.result {
        WorkspaceCommitResult::Created { commit_id, .. } if !status.presentation_failed => {
            Ok(commit_id)
        }
        _ => Err("required ordinary Created acknowledgement".into()),
    }
}
fn uptodate(client: &Client, id: WorkspaceId) -> AnyResult<()> {
    let result = client.commit_workspace_session(id);
    let observation = observed(client);
    let out = result?;
    observation?;
    record("unchanged-commit", &out);
    physical_state(client, id, "after-up-to-date")?;
    if !matches!(out, WorkspaceCommitResult::UpToDate { .. }) {
        return Err("required UpToDate acknowledgement".into());
    }
    Ok(())
}
fn docker(container: &ContainerId, args: &[&str]) -> AnyResult<std::process::Output> {
    Ok(std::process::Command::new("docker")
        .arg("exec")
        .arg(&container.0)
        .args(args)
        .output()?)
}
fn reset_barrier(container: &ContainerId, action: &str) -> AnyResult<()> {
    let names = [
        format!("/tmp/layerfs-{action}-ready"),
        format!("/tmp/layerfs-{action}-release"),
        format!("/tmp/layerfs-{action}-child"),
    ];
    if !docker(container, &["rm", "-f", &names[0], &names[1], &names[2]])?
        .status
        .success()
    {
        return Err("barrier artifact cleanup".into());
    }
    Ok(())
}
fn require_child_terminated(container: &ContainerId, action: &str) -> AnyResult<()> {
    let pid = docker(container, &["cat", &format!("/tmp/layerfs-{action}-child")])?;
    if !pid.status.success() {
        return Err("owned child PID receipt missing".into());
    }
    let pid: String = String::from_utf8(pid.stdout)?;
    let pid: u32 = pid.trim().parse()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while docker(container, &["test", "-d", &format!("/proc/{pid}")])?
        .status
        .success()
    {
        if Instant::now() >= deadline {
            return Err("owned child survived cancellation/disconnect".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    record("owned-child-terminated", pid);
    reset_barrier(container, action)
}
fn resource_snapshot(container: &ContainerId, label: &str) -> AnyResult<()> {
    let resource = process_resource_snapshot()?;
    record(label,format!("user_cpu_ns={} system_cpu_ns={} resident_bytes={} peak_resident_bytes={} physical_footprint_bytes={} disk_read_bytes={} disk_write_bytes={} swaps={}",resource.user_cpu_ns,resource.system_cpu_ns,resource.resident_bytes,resource.peak_resident_bytes,resource.physical_footprint_bytes,resource.disk_read_bytes,resource.disk_write_bytes,resource.swaps));
    let observed=docker(container,&["sh","-c","for key in memory.current memory.peak memory.swap.current memory.events pids.current; do printf '%s=' \"$key\"; cat \"/sys/fs/cgroup/$key\" || exit; done"])?;
    if !observed.status.success() {
        return Err("proof cgroup resource observation unavailable".into());
    }
    record(
        "proof-cgroup-resources",
        String::from_utf8(observed.stdout)?,
    );
    Ok(())
}
fn barrier(container: &ContainerId, action: &str) -> AnyResult<()> {
    let path = format!("/tmp/layerfs-{action}-ready");
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        if docker(container, &["test", "-f", &path])?.status.success() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("public workload barrier timeout".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn release(container: &ContainerId, action: &str) -> AnyResult<()> {
    let path = format!("/tmp/layerfs-{action}-release");
    if !docker(container, &["touch", &path])?.status.success() {
        return Err("barrier release failed".into());
    }
    Ok(())
}
fn finish(client: &Client, execution: ExecutionId) -> AnyResult<layerfs_sdk::OutputPage> {
    let reader = client.workspace_output(execution)?;
    let mut out = reader.read(0, true)?;
    while !out.exited {
        let next = reader.read(out.next_sequence, true)?;
        out.chunks.extend(next.chunks);
        out.next_sequence = next.next_sequence;
        out.exited = next.exited;
        out.truncated |= next.truncated;
        out.receipt = next.receipt;
    }
    if out.truncated {
        return Err("proof execution output truncated".into());
    }
    record("execution-finished", &out);
    observed(client)?;
    Ok(out)
}
fn unchanged(
    store: &LayerStackStore,
    branch: BranchId,
    old: &layerfs_layerstack_store::BranchRecord,
    commits: u64,
) -> AnyResult<()> {
    let after = store.branch(branch)?.ok_or("missing Branch")?;
    if &after != old || store.store_counts()?.commits != commits {
        return Err("failed operation changed head/base/Commit count".into());
    }
    Ok(())
}
fn verify_fault(fault: VerificationFault) -> AnyResult<()> {
    let receipt = take_verification_fault_receipt()?.ok_or("missing fault receipt")?;
    record("fault-reachability", &receipt);
    if receipt.fault != fault || receipt.hit_count != 1 {
        return Err("Workspace fault never reached exactly once".into());
    }
    Ok(())
}

pub(crate) fn run(
    root: &Path,
    prepared: &Path,
    subcase: &str,
    container: ContainerId,
) -> AnyResult<()> {
    let case = family::resolve(subcase)?;
    let runtime_before = runtime_files()?;
    record("proof-start", &case.id);
    resource_snapshot(&container, "proof-resource-start")?;
    if !root.exists() {
        std::fs::create_dir_all(root)?;
    }
    let path = root.join("store.sqlite");
    if !path.exists() {
        std::fs::copy(prepared.join("store.sqlite"), &path)?;
        std::fs::copy(prepared.join("branch-id"), root.join("branch-id"))?;
    }
    if prepared.join("store.sqlite").exists() {
        use std::os::unix::fs::MetadataExt;
        let source = std::fs::metadata(prepared.join("store.sqlite"))?;
        let sample = std::fs::metadata(&path)?;
        if (source.dev(), source.ino()) == (sample.dev(), sample.ino()) {
            return Err("reliability mutable sample aliases pristine master".into());
        }
    }
    let branch: BranchId = std::fs::read_to_string(root.join("branch-id"))?
        .trim()
        .parse()?;
    if matches!(case.kind, "corrupt-descendant" | "missing-descendant") {
        let result = integrity(root, &case, branch, container.clone());
        record("integrity-outcome-before-cleanup", &result);
        require_unmounted(
            &container,
            &PathBuf::from(format!("/workspace/reliability-{}", case.kind)),
        )?;
        if runtime_files()?
            .difference(&runtime_before)
            .next()
            .is_some()
        {
            return Err("integrity proof leaked owned runtime files".into());
        }
        super::workspace_bench::spool_observation("final-client-drop-cleanup")?;
        resource_snapshot(&container, "proof-resource-end")?;
        match result {
            Ok(()) => {
                record("proof-complete", format!("{} pass", case.id));
                return Ok(());
            }
            Err(error) => {
                record("proof-failed", error.to_string());
                return Err(error);
            }
        }
    }
    let store = Arc::new(LayerStackStore::connect(&path)?);
    let client = Client::connect(store.clone())?;
    let placement = WorkspacePlacement::Container {
        container_id: container.clone(),
        root: PathBuf::from(format!("/workspace/reliability-{}", case.kind)),
    };
    let request = || CreateWorkspaceSession {
        branch_id: branch,
        placement: placement.clone(),
        projection: Some(WorkspaceProjection::Fuse),
    };
    let result = client.create_workspace_session(request());
    let observation = observed(&client);
    let session = result?;
    observation?;
    super::workspace_bench::spool_observation("after-create")?;
    let mut final_verification_deadline = None;
    let mut live_session = Some(session.id);
    let before = store.branch(branch)?.ok_or("missing initial branch")?;
    let before_commits = store.store_counts()?.commits;
    let old = store.pin_branch(branch)?;
    let mut final_state = "done";
    let mut final_ordinal = 1;
    let mut retained = Vec::new();
    let result = (|| -> AnyResult<()> {
        live(&client, session.id, &case, "initial", 0)?;
        match case.kind {
            "lease-lifecycle" => {
                let second = Client::connect(store.clone())?;
                let error = second.create_workspace_session(request());
                observed(&second)?;
                if !matches!(
                    error,
                    Err(SdkError::Workspace(WorkspaceError::WorkspaceBusy))
                ) {
                    return Err(format!("lease expected Busy: {error:?}").into());
                }
                client.end_workspace_session(session.id, EndWorkspaceMode::Clean)?;
                observed(&client)?;
                live_session = None;
                let created = second.create_workspace_session(request())?;
                live(&second, created.id, &case, "initial", 0)?;
                drop(second);
                let again = client.create_workspace_session(request())?;
                live_session = Some(again.id);
                live(&client, again.id, &case, "initial", 0)?;
                final_state = "initial";
            }
            "invalid-sdk-edit" => {
                workload(&client, session.id, &case, "prior", 0)?;
                live(&client, session.id, &case, "prior", 0)?;
                let invalid = client.edit_workspace_file_range(WorkspaceFileRangeEdit {
                    workspace_id: session.id,
                    path: "sentinels/f0001.dat".into(),
                    start: 32769,
                    delete_len: 1,
                    replacement: WorkspaceFileReplacement::Inline(vec![1]),
                });
                if !matches!(
                    invalid,
                    Err(SdkError::Workspace(WorkspaceError::Storage(
                        layerfs_layerstack_store::StoreError::InvalidInput("file range")
                    )))
                ) {
                    return Err(format!("unexpected invalid-edit outcome: {invalid:?}").into());
                }
                unchanged(&store, branch, &before, before_commits)?;
                live(&client, session.id, &case, "prior", 0)?;
                created(&client, session.id)?;
            }
            "invalid-namespace" => {
                workload(&client, session.id, &case, "prior", 0)?;
                workload(&client, session.id, &case, "invalid-namespace", 0)?;
                unchanged(&store, branch, &before, before_commits)?;
                live(&client, session.id, &case, "done", 0)?;
                created(&client, session.id)?;
            }
            "candidate-failure-retry"
            | "admission-batch-failure-retry"
            | "final-publication-failure-retry" => {
                workload(
                    &client,
                    session.id,
                    &case,
                    if case.kind == "candidate-failure-retry" {
                        "candidate-dirty"
                    } else {
                        "large-dirty"
                    },
                    0,
                )?;
                live(&client, session.id, &case, "done", 0)?;
                if case.kind == "candidate-failure-retry" {
                    arm_verification_fault(branch, VerificationFault::Candidate)?;
                } else {
                    arm_verification_store_fault(
                        branch,
                        if case.kind == "admission-batch-failure-retry" {
                            VerificationStoreFault::LaterAdmissionBatch
                        } else {
                            VerificationStoreFault::FinalPublication
                        },
                    )?;
                }
                let failed = client.commit_workspace_session(session.id);
                record("injected-commit-result", &failed);
                let expected_message = if case.kind == "candidate-failure-retry" {
                    "injected Workspace candidate failure"
                } else {
                    "injected qualified Workspace transaction failure"
                };
                if !matches!(&failed,Err(SdkError::Workspace(WorkspaceError::Storage(layerfs_layerstack_store::StoreError::Integrity(message)))) if *message==expected_message)
                {
                    return Err("faulted Commit did not surface exact injected error".into());
                }
                if case.kind == "candidate-failure-retry" {
                    verify_fault(VerificationFault::Candidate)?;
                } else {
                    let r = take_verification_store_fault_receipt()
                        .ok_or("missing transaction fault receipt")?;
                    record("transaction-fault-reachability", &r);
                    if r.hit_count != 1
                        || r.candidate_spill_count == 0
                        || r.committed_early_transactions == 0
                    {
                        return Err(
                            "required production spill/early-admission/fault boundary not reached"
                                .into(),
                        );
                    }
                }
                unchanged(&store, branch, &before, before_commits)?;
                workspace_verify::verify_root(&old.reader, old.root, &family::fixture()?)?;
                live(&client, session.id, &case, "done", 0)?;
                created(&client, session.id)?;
                uptodate(&client, session.id)?;
            }
            "published-presentation-failure" => {
                workload(&client, session.id, &case, "published-dirty", 0)?;
                arm_verification_fault(branch, VerificationFault::PresentationResume)?;
                let result = client.commit_workspace_session_with_status(session.id)?;
                record("presentation-failure", &result);
                verify_fault(VerificationFault::PresentationResume)?;
                if !matches!(result.result, WorkspaceCommitResult::Created { .. })
                    || !result.presentation_failed
                    || store.store_counts()?.commits != before_commits + 1
                {
                    return Err("publication/presentation boundary".into());
                }
                canonical(&store, branch, &case, "done", 0)?;
                client.recover_workspace_presentation(session.id)?;
                live(&client, session.id, &case, "done", 0)?;
                uptodate(&client, session.id)?;
            }
            "dirty-end-discard" => {
                workload(&client, session.id, &case, "write-A", 0)?;
                created(&client, session.id)?;
                workload(&client, session.id, &case, "write-B", 0)?;
                if !matches!(
                    client.end_workspace_session(session.id, EndWorkspaceMode::Clean),
                    Err(SdkError::Workspace(WorkspaceError::WorkspaceDirty))
                ) {
                    return Err("Clean End did not reject dirty state".into());
                }
                live(&client, session.id, &case, "done", 0)?;
                client.end_workspace_session(session.id, EndWorkspaceMode::Discard)?;
                live_session = None;
                final_state = "published";
            }
            "dirty-net-zero" => {
                workload(&client, session.id, &case, "netzero", 0)?;
                live(&client, session.id, &case, "initial", 0)?;
                uptodate(&client, session.id)?;
                unchanged(&store, branch, &before, before_commits)?;
                final_state = "initial";
            }
            "short-spool-write" | "deferred-nospace" => {
                workload(&client, session.id, &case, "prepare-failure", 0)?;
                live(&client, session.id, &case, "done", 0)?;
                let fault = if case.kind == "short-spool-write" {
                    VerificationFault::ShortAppend
                } else {
                    VerificationFault::NoSpace
                };
                let state_before = client.verification_workspace_state(session.id)?;
                arm_verification_fault(branch, fault)?;
                let out = client.exec_workspace_session(
                    session.id,
                    NonEmpty::new(argv(&case, "fail-write", 0))?,
                )?;
                let result = finish(&client, out.id)?;
                if result.receipt.as_ref().and_then(|r| r.exit_code) != Some(0) {
                    return Err("faulted write did not produce its exact errno proof".into());
                }
                if case.kind == "deferred-nospace" {
                    let text = String::from_utf8(
                        result
                            .chunks
                            .iter()
                            .flat_map(|c| c.bytes.iter().copied())
                            .collect(),
                    )?;
                    if !text.lines().any(|line| line == "error_boundary=fsync")
                        || !text
                            .lines()
                            .any(|line| line == "write_acknowledged_bytes=4096")
                    {
                        return Err("NoSpace did not exercise deferred proxy error boundary".into());
                    }
                }
                verify_fault(fault)?;
                let state_after = client.verification_workspace_state(session.id)?;
                record("write-rollback-accounting", (&state_before, &state_after));
                if (
                    state_before.spool_bytes,
                    state_before.spool_peak_bytes,
                    state_before.mutation_generation,
                ) != (
                    state_after.spool_bytes,
                    state_after.spool_peak_bytes,
                    state_after.mutation_generation,
                ) {
                    return Err(
                        "failed write changed spool accounting or mutation generation".into(),
                    );
                }
                live(&client, session.id, &case, "done", 0)?;
                unchanged(&store, branch, &before, before_commits)?;
                client.end_workspace_session(session.id, EndWorkspaceMode::Discard)?;
                live_session = None;
                final_state = "initial";
            }
            "open-writer-busy" => {
                let WorkspacePlacement::Container { root: mount, .. } = &placement else {
                    unreachable!()
                };
                reset_barrier(&container, "hold-writer")?;
                let mut holder = std::process::Command::new("docker")
                    .args(["exec", "-w"])
                    .arg(mount)
                    .arg(&container.0)
                    .args(argv(&case, "hold-writer", 0))
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()?;
                let writer_result = (|| -> AnyResult<()> {
                    barrier(&container, "hold-writer")?;
                    let outcome = client.commit_workspace_session(session.id)?;
                    record("open-writer-commit", &outcome);
                    if !matches!(outcome, WorkspaceCommitResult::Busy)
                        || client.active_execution_count()? != 0
                    {
                        return Err("open writer Busy was not independently reached".into());
                    }
                    unchanged(&store, branch, &before, before_commits)?;
                    live(&client, session.id, &case, "done", 0)?;
                    Ok(())
                })();
                let release_result = release(&container, "hold-writer");
                if release_result.is_err() {
                    let _ = holder.kill();
                }
                let holder_output = holder.wait_with_output()?;
                super::workspace_bench::emit(
                    "external-execution",
                    &[
                        ("scope", super::workspace_bench::quote("open-writer-holder")),
                        (
                            "stdout_hex",
                            super::workspace_bench::quote(&workload_source::hex(
                                &holder_output.stdout,
                            )),
                        ),
                        (
                            "stderr_hex",
                            super::workspace_bench::quote(&workload_source::hex(
                                &holder_output.stderr,
                            )),
                        ),
                        (
                            "exit_status",
                            super::workspace_bench::quote(&format!("{}", holder_output.status)),
                        ),
                    ],
                );
                reset_barrier(&container, "hold-writer")?;
                writer_result?;
                release_result?;
                if !holder_output.status.success() {
                    return Err("writer holder failed".into());
                }
                live(&client, session.id, &case, "done", 0)?;
                created(&client, session.id)?;
            }
            "live-execution-busy" | "workload-cancel" | "dirty-runtime-disconnect" => {
                let action = match case.kind {
                    "live-execution-busy" => "hold-execution",
                    "workload-cancel" => "hold-cancel",
                    _ => "hold-disconnect",
                };
                reset_barrier(&container, action)?;
                let execution = client
                    .exec_workspace_session(session.id, NonEmpty::new(argv(&case, action, 0))?)?;
                barrier(&container, action)?;
                live(&client, session.id, &case, "done", 0)?;
                if case.kind == "live-execution-busy" {
                    let outcome = client.commit_workspace_session(session.id)?;
                    if !matches!(outcome, WorkspaceCommitResult::Busy) {
                        return Err("live execution Commit was not Busy".into());
                    }
                    release(&container, action)?;
                    let out = finish(&client, execution.id)?;
                    if out.receipt.as_ref().and_then(|r| r.exit_code) != Some(0) {
                        return Err("barrier execution failed".into());
                    }
                } else if case.kind == "workload-cancel" {
                    client.stop_workspace_execution(execution.id)?;
                    finish(&client, execution.id)?;
                } else {
                    client.verification_disconnect_workspace_execution(execution.id)?;
                    let reader = client.workspace_output(execution.id)?;
                    let mut cursor = 0;
                    loop {
                        match reader.read(cursor, true) {
                            Err(WorkspaceError::InfrastructureLost) => break,
                            Err(e) => {
                                return Err(format!("disconnect unexpected error: {e:?}").into())
                            }
                            Ok(p) => {
                                if p.exited {
                                    return Err(
                                        "disconnect falsely reported normal completion".into()
                                    );
                                }
                                cursor = p.next_sequence;
                            }
                        }
                    }
                }
                if case.kind != "live-execution-busy" {
                    require_child_terminated(&container, action)?;
                } else {
                    reset_barrier(&container, action)?;
                }
                unchanged(&store, branch, &before, before_commits)?;
                live(&client, session.id, &case, "done", 0)?;
                if case.kind == "live-execution-busy" {
                    created(&client, session.id)?;
                } else {
                    client.end_workspace_session(session.id, EndWorkspaceMode::Discard)?;
                    live_session = None;
                    final_state = "initial";
                }
            }
            "exec-500" => {
                for k in 0..500 {
                    let out = workload(&client, session.id, &case, "exec-one", k)?;
                    if out.receipt.is_none()
                        || client.active_execution_count()? != 0
                        || out
                            .chunks
                            .iter()
                            .flat_map(|c| c.bytes.iter().copied())
                            .collect::<Vec<_>>()
                            != format!("{k}\n").as_bytes()
                    {
                        return Err("Exec receipt/reader/output completion".into());
                    }
                }
                final_ordinal = 499;
                created(&client, session.id)?;
            }
            "repeat-publication" => {
                for k in 0..3 {
                    workload(&client, session.id, &case, "stage", k)?;
                    let id = created(&client, session.id)?;
                    let record = store.commit(id)?.ok_or("missing retained Commit")?;
                    let parent = retained
                        .last()
                        .map(
                            |(record, _): &(layerfs_layerstack_store::CommitRecord, u64)| record.id,
                        )
                        .or(before.head_commit_id);
                    if record.parent_commit_id != parent
                        || store
                            .branch(branch)?
                            .ok_or("repeat Branch missing")?
                            .head_commit_id
                            != Some(record.id)
                    {
                        return Err("repeated publication head/parent order".into());
                    }
                    workspace_verify::verify_root(
                        &store.snapshot_reader(record.root_id),
                        record.root_id,
                        &family::expected(&case, "done", k)?,
                    )?;
                    retained.push((record, k));
                }
                uptodate(&client, session.id)?;
                final_ordinal = 2;
            }
            _ => {
                let action = match case.kind {
                    "parallel-read-write" => "parallel",
                    "shared-path-contention" => "contention",
                    "hardlink-alias" => "hardlink",
                    "symlink-semantics" => "symlink",
                    "open-rename-unlink" => "open-handles",
                    "metadata-chmod" => "chmod",
                    "metadata-mtime" => "mtime",
                    "metadata-xattr" => "xattr",
                    "sustained-600s" => "sustained",
                    _ => return Err("unknown reliability host body".into()),
                };
                let active_deadline = if case.kind == "sustained-600s" {
                    Some(super::workspace_bench::PhaseDeadline::start(
                        "sustained-active-workload",
                        900,
                    ))
                } else {
                    None
                };
                let result = workload(&client, session.id, &case, action, 0);
                drop(active_deadline);
                if case.kind == "sustained-600s" {
                    final_verification_deadline =
                        Some(super::workspace_bench::PhaseDeadline::start(
                            "sustained-final-verification",
                            600,
                        ));
                }
                let out = result?;
                if case.kind == "sustained-600s" {
                    let text = String::from_utf8(
                        out.chunks.iter().flat_map(|c| c.bytes.clone()).collect(),
                    )?;
                    final_ordinal = text
                        .lines()
                        .find_map(|l| l.strip_prefix("completed_cycles="))
                        .ok_or("sustained cycle receipt")?
                        .parse()?;
                    let active_ns: u64 = text
                        .lines()
                        .find_map(|line| line.strip_prefix("active_elapsed_ns="))
                        .ok_or("sustained active-duration receipt")?
                        .parse()?;
                    if final_ordinal == 0 || active_ns < 600_000_000_000 {
                        return Err("sustained receipt lacks 600 seconds of active work".into());
                    }
                }
                if case.kind == "metadata-xattr" {
                    uptodate(&client, session.id)?;
                    final_state = "initial";
                } else {
                    created(&client, session.id)?;
                }
            }
        }
        if let Some(id) = live_session {
            live(&client, id, &case, final_state, final_ordinal)?;
            client.end_workspace_session(id, EndWorkspaceMode::Clean)?;
            live_session = None;
        }
        canonical(&store, branch, &case, final_state, final_ordinal)?;
        if client.active_workspace_count()? != 0 || client.active_execution_count()? != 0 {
            return Err("owned session/execution leak".into());
        }
        Ok(())
    })();
    record("proof-outcome-before-cleanup", &result);
    let mut observation_failed = observed(&client).is_err();
    if let Err(error) = super::workspace_bench::spool_observation("proof-before-cleanup") {
        record("spool-observation-failure", error.to_string());
        observation_failed = true;
    }
    if let Some(id) = live_session {
        let cleanup = client.end_workspace_session(id, EndWorkspaceMode::Discard);
        record("failure-cleanup", &cleanup);
    }
    observation_failed |= observed(&client).is_err();
    let fault = take_verification_fault_receipt()?;
    let mut unconsumed_fault = fault.is_some();
    if fault.is_some() {
        record("unconsumed-workspace-fault", fault);
    }
    if let Some(fault) = take_verification_store_fault_receipt() {
        unconsumed_fault = true;
        record("unconsumed-store-fault", fault);
    }
    drop(old);
    drop(client);
    drop(store);
    super::workspace_bench::spool_observation("after-client-drop-cleanup")?;
    if let WorkspacePlacement::Container { root: mount, .. } = &placement {
        require_unmounted(&container, mount)?;
    }
    let runtime_after = runtime_files()?;
    if runtime_after.difference(&runtime_before).next().is_some() {
        record("leaked-runtime-files", &runtime_after);
        return Err("owned spool/output files remain after Client drop".into());
    }
    if let Err(error) = result {
        record("proof-failed", error.to_string());
        return Err(error);
    }
    if observation_failed {
        return Err("missing mandatory proof observation".into());
    }
    if unconsumed_fault {
        record("proof-failed", "unconsumed verification fault");
        return Err("unconsumed fault is a qualification gap".into());
    }
    let reopened = Arc::new(LayerStackStore::connect(&path)?);
    let client = Client::connect(reopened.clone())?;
    let result = client.create_workspace_session(request());
    let observation = observed(&client);
    let session = result?;
    observation?;
    live(&client, session.id, &case, final_state, final_ordinal)?;
    client.end_workspace_session(session.id, EndWorkspaceMode::Clean)?;
    canonical(&reopened, branch, &case, final_state, final_ordinal)?;
    for (record, n) in retained {
        workspace_verify::verify_root(
            &reopened.snapshot_reader(record.root_id),
            record.root_id,
            &family::expected(&case, "done", n)?,
        )?;
    }
    observed(&client)?;
    drop(client);
    drop(reopened);
    if let WorkspacePlacement::Container { root: mount, .. } = &placement {
        require_unmounted(&container, mount)?;
    }
    if runtime_files()?
        .difference(&runtime_before)
        .next()
        .is_some()
    {
        return Err("reopen leaked owned runtime files".into());
    }
    super::workspace_bench::spool_observation("final-client-drop-cleanup")?;
    resource_snapshot(&container, "proof-resource-end")?;
    drop(final_verification_deadline);
    record("proof-complete", format!("{} pass", case.id));
    Ok(())
}

fn integrity(
    root: &Path,
    case: &common::Case,
    branch: BranchId,
    container: ContainerId,
) -> AnyResult<()> {
    let path = root.join("store.sqlite");
    let store = LayerStackStore::connect(&path)?;
    let pin = store.pin_branch(branch)?;
    let evidence = workspace_verify::verify_root(&pin.reader, pin.root, &family::fixture()?)?;
    let id = evidence
        .extents
        .get("sentinels/f0003.dat")
        .and_then(|e| e.first())
        .ok_or("integrity descendant transcript")?
        .id;
    record("fault-target-payload", id);
    drop(pin);
    drop(store);
    let db = rusqlite::Connection::open(&path)?;
    if case.kind == "missing-descendant" {
        if db.execute(
            "DELETE FROM objects WHERE object_id=?1",
            [id.as_bytes().as_slice()],
        )? != 1
        {
            return Err("missing descendant fault target".into());
        }
    } else {
        let mut bytes: Vec<u8> = db.query_row(
            "SELECT bytes FROM objects WHERE object_id=?1",
            [id.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        let last = bytes.last_mut().ok_or("empty canonical payload")?;
        *last ^= 1;
        db.execute(
            "UPDATE objects SET bytes=?2 WHERE object_id=?1",
            rusqlite::params![id.as_bytes().as_slice(), bytes],
        )?;
    }
    drop(db);
    let store = match LayerStackStore::connect(&path) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            record("integrity-detected-store-open", &e);
            if integrity_error(&e) {
                record("integrity-detection-qualified", &case.id);
                return Ok(());
            }
            return Err(e.into());
        }
    };
    let client = Client::connect(store)?;
    let create = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch,
        placement: WorkspacePlacement::Container {
            container_id: container,
            root: PathBuf::from(format!("/workspace/reliability-{}", case.kind)),
        },
        projection: Some(WorkspaceProjection::Fuse),
    });
    observed(&client)?;
    let session = match create {
        Ok(s) => s,
        Err(SdkError::Workspace(WorkspaceError::Storage(e))) if integrity_error(&e) => {
            record("integrity-detected-create", e);
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    let execution = client.exec_workspace_session(
        session.id,
        NonEmpty::new(vec![
            "/usr/local/bin/fs-benchmark-workload".into(),
            "workspace-reliability-workload".into(),
            case.id.clone().into(),
            "read-corrupt".into(),
            "0".into(),
        ])?,
    )?;
    let out = finish(&client, execution.id)?;
    client.end_workspace_session(session.id, EndWorkspaceMode::Discard)?;
    observed(&client)?;
    if out.receipt.as_ref().and_then(|r| r.exit_code) != Some(0) {
        return Err("corrupt descendant read did not produce exact EIO proof".into());
    }
    record("integrity-detected-read", out);
    record("integrity-detection-qualified", &case.id);
    Ok(())
}
