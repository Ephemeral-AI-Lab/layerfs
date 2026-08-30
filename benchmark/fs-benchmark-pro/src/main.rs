use layerfs_sdk::{
    AddLayerResult, BranchId, BranchStore, Client, ConnectionContext, ContainerId,
    CreateWorkspaceSession, EndWorkspaceMode, EntityName, LayerStackEndpoint,
    LayerStackInitialization, LayerStackStore, LocalForkSource, MonitorSnapshot, OperationReceipt,
    PushResult, RemotePlacement, StoreId, WorkspaceCommitResult, WorkspacePlacement,
    WorkspaceProjection,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_FILE_MIB: u64 = 32;
const DEFAULT_EDITS: usize = 16;
const ARTICLE_FIXTURE_SHA256: &str =
    "3d2fadd86ea3d8c52f8f3255bec470f2da7e31b7ed809cc0e97e1e9dc894cd8c";
const ARTICLE_EDIT_SHA256: &str =
    "30e8b6c71ab635057c32f0e509e6e0037b5781f94bf1b4c88fb438f41d76ca26";
const ARTICLE_FINAL_SHA256: &str =
    "7b86abcd0e9d2016bbb8b16722e1439475feff84e31fe9801a4ec74e99dc74c3";
const PREPEND_BYTES: u64 = 10;

type AnyResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Default)]
struct Phases {
    id: String,
    workspace_create_ns: u64,
    shell_ns: u64,
    workspace_commit_api_ns: u64,
    push_api_ns: u64,
    workspace_end_ns: u64,
    workspace_create_receipt: Option<String>,
    workspace_commit_receipt: Option<String>,
    push_receipt: Option<String>,
    workspace_end_receipt: Option<String>,
    storage_before: Option<StoreTotals>,
    storage_after: Option<StoreTotals>,
}

impl Phases {
    fn authority_checkpoint_ns(&self) -> u64 {
        self.shell_ns
            .saturating_add(self.workspace_commit_api_ns)
            .saturating_add(self.push_api_ns)
    }

    fn complete_turn_ns(&self) -> u64 {
        self.workspace_create_ns
            .saturating_add(self.authority_checkpoint_ns())
            .saturating_add(self.workspace_end_ns)
    }
}

#[derive(Clone, Copy, Default)]
struct StoreTotals {
    database_bytes: u64,
    wal_bytes: u64,
    shm_bytes: u64,
    durable_allocated_bytes: u64,
}

struct State {
    values: BTreeMap<String, String>,
    operations: Vec<Phases>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("fs-benchmark-pro: {error}");
        std::process::exit(1);
    }
}

fn run() -> AnyResult<()> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [mode] if mode == "self-check" => self_check(),
        [mode, container, results] if mode == "measure" => {
            measure(container, Path::new(results), DEFAULT_FILE_MIB, DEFAULT_EDITS)
        }
        [mode, container, results, file_mib, edits] if mode == "measure" => measure(
            container,
            Path::new(results),
            parse_positive(file_mib, "FILE_MIB")?,
            usize::try_from(parse_positive(edits, "EDIT_COUNT")?)?,
        ),
        [mode, state, result] if mode == "verify" => verify(Path::new(state), Path::new(result)),
        _ => Err(
            "usage: fs-benchmark-pro self-check | measure CONTAINER RESULT_DIR [FILE_MIB EDIT_COUNT] | verify STATE_TSV RESULT_PATH"
                .into(),
        ),
    }
}

fn self_check() -> AnyResult<()> {
    let status = Command::new("python3")
        .arg(workload_script())
        .arg("self-check")
        .status()?;
    if !status.success() {
        return Err("workload self-check failed".into());
    }
    let escaped = json_string("a\t\"b\\c\n");
    if escaped != "\"a\\t\\\"b\\\\c\\n\"" {
        return Err("JSON encoder self-check failed".into());
    }
    println!(
        "{{\"schema\":\"fs-benchmark-pro-self-check-v1\",\"candidate\":\"layerfs-reference\",\"status\":\"pass\"}}"
    );
    Ok(())
}

fn measure(container: &str, results: &Path, file_mib: u64, edit_count: usize) -> AnyResult<()> {
    validate_container(container)?;
    if edit_count == 0 {
        return Err("EDIT_COUNT must be positive".into());
    }
    let file_bytes = file_mib
        .checked_mul(1024 * 1024)
        .ok_or("FILE_MIB overflow")?;
    if file_bytes <= PREPEND_BYTES {
        return Err("fixture must exceed the 10-byte edit marker".into());
    }
    fs::create_dir_all(results)?;
    let results = fs::canonicalize(results)?;
    let fixture = results.join("fixture.bin");
    let (fixture_bytes, fixture_digest) = host_digest(&fixture)?;
    if fixture_bytes != file_bytes {
        return Err(format!(
            "fixture size is {fixture_bytes}, expected {file_bytes}: {}",
            fixture.display()
        )
        .into());
    }
    if file_mib == DEFAULT_FILE_MIB && fixture_digest != ARTICLE_FIXTURE_SHA256 {
        return Err(format!(
            "32 MiB fixture SHA-256 is {fixture_digest}, expected {ARTICLE_FIXTURE_SHA256}"
        )
        .into());
    }
    let (after_edits_digest, final_oracle_digest) =
        host_oracles(&results, &fixture, file_bytes, edit_count)?;
    if file_mib == DEFAULT_FILE_MIB && edit_count == DEFAULT_EDITS {
        if after_edits_digest != ARTICLE_EDIT_SHA256 {
            return Err(format!(
                "article edit oracle is {after_edits_digest}, expected {ARTICLE_EDIT_SHA256}"
            )
            .into());
        }
        if final_oracle_digest != ARTICLE_FINAL_SHA256 {
            return Err(format!(
                "article final oracle is {final_oracle_digest}, expected {ARTICLE_FINAL_SHA256}"
            )
            .into());
        }
    }

    let evidence_path = results.join("layerfs-reference.jsonl");
    let state_path = results.join("layerfs-reference-state.tsv");
    let store_root = results.join("layerfs-reference-store");
    if evidence_path.exists() || state_path.exists() || store_root.exists() {
        return Err("LayerFS result files already exist; use a fresh RESULT_DIR".into());
    }
    fs::create_dir(&store_root)?;
    let fuse_helper = store_root.join("layerfs-fuse");
    copy_from_container(container, "/usr/local/bin/layerfs-fuse", &fuse_helper)?;
    make_executable(&fuse_helper)?;
    std::env::set_var("LAYERFS_FUSE_HELPER", &fuse_helper);
    let authority_path = store_root.join("authority.sqlite");
    let branch_path = store_root.join("branch.sqlite");
    let authority = Arc::new(LayerStackStore::create(&authority_path)?);
    let authority_store_id = authority.store_id();
    let branches = BranchStore::create(&branch_path, authority_store_id)?;
    let branch_parent_store_id = branches.parent_store_id();
    let client = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(authority.clone()),
        branches,
    })?;
    let genesis = client
        .initialize_layerstack(
            EntityName::new("fs-benchmark-pro")?,
            LayerStackInitialization::Empty,
        )?
        .genesis_layer_id;
    client.pull_layer(genesis, RemotePlacement::Reference)?;
    let branch_id = client.fork_branch(
        EntityName::new("fs-benchmark-pro-main")?,
        LocalForkSource::Layer { layer_id: genesis },
    )?;
    let initial = store_totals(&client.monitor_snapshot()?, &store_root)?;

    let self_target = std::env::var("LAYERFS_BENCH_SELF_TARGET").as_deref() == Ok("1");
    let (remote_script, remote_fixture) = if self_target {
        (
            workload_script().to_string_lossy().into_owned(),
            fixture.to_string_lossy().into_owned(),
        )
    } else {
        let script = format!("/tmp/fs-benchmark-pro-{}-workload.py", std::process::id());
        let fixture_path = format!("/tmp/fs-benchmark-pro-{}-fixture.bin", std::process::id());
        copy_to_container(container, &workload_script(), &script)?;
        copy_to_container(container, &fixture, &fixture_path)?;
        docker_checked(container, &["chmod", "0555", &script])?;
        docker_checked(container, &["chmod", "0444", &fixture_path])?;
        (script, fixture_path)
    };
    let mount_root = format!("/workspace/fs-benchmark-pro-{}", std::process::id());
    let payload = format!("{mount_root}/payload.bin");
    let runner = WorkspaceRun {
        client: &client,
        branch_id,
        container,
        mount_root: &mount_root,
        store_root: &store_root,
    };

    let mut operations = Vec::with_capacity(edit_count + 3);
    operations.push(runner.mutation(
        "create".to_owned(),
        &[
            "python3",
            &remote_script,
            "create",
            &remote_fixture,
            &payload,
        ],
    )?);

    for index in 0..edit_count {
        let id = format!("edit-{:02}", index + 1);
        let index = index.to_string();
        let bytes = file_bytes.to_string();
        operations.push(runner.mutation(
            id,
            &["python3", &remote_script, "edit", &payload, &index, &bytes],
        )?);
    }

    operations.push(runner.mutation(
        "prepend".to_owned(),
        &["python3", &remote_script, "prepend", &payload],
    )?);

    let (read, output) = runner.read(&["python3", &remote_script, "read", &payload])?;
    let (expected_bytes, expected_digest) = parse_digest(&output)?;
    if expected_bytes != file_bytes + PREPEND_BYTES {
        return Err(format!(
            "final size is {expected_bytes}, expected {}",
            file_bytes + PREPEND_BYTES
        )
        .into());
    }
    if expected_digest != final_oracle_digest {
        return Err(format!(
            "measured final SHA-256 is {expected_digest}, neutral oracle is {final_oracle_digest}"
        )
        .into());
    }
    let authority_checkpoint_storage = read.storage_after.expect("storage snapshot assigned");
    operations.push(read);

    let (add, add_ns, add_receipt) = observed(&client, || client.add_layer(branch_id))?;
    require_add(&add)?;
    let final_storage = store_totals(&client.monitor_snapshot()?, &store_root)?;
    let (commit, dirty) = source_provenance();
    write_evidence(&EvidenceWrite {
        path: &evidence_path,
        file_bytes,
        edit_count,
        fixture_digest: &fixture_digest,
        operations: &operations,
        add_ns,
        add_receipt: &add_receipt,
        source_commit: &commit,
        source_dirty: dirty,
    })?;
    write_state(
        &state_path,
        &StateWrite {
            container,
            authority_path: &authority_path,
            branch_path: &branch_path,
            fuse_helper: &fuse_helper,
            authority_store_id,
            branch_parent_store_id,
            branch_id,
            file_bytes,
            edit_count,
            fixture_digest: &fixture_digest,
            after_edits_digest: &after_edits_digest,
            expected_bytes,
            expected_digest: &expected_digest,
            mount_root: &mount_root,
            initial,
            authority_checkpoint_storage,
            final_storage,
            add_ns,
            operations: &operations,
            source_commit: &commit,
            source_dirty: dirty,
        },
    )?;
    println!("{}", state_path.display());
    Ok(())
}

struct WorkspaceRun<'a> {
    client: &'a Client,
    branch_id: BranchId,
    container: &'a str,
    mount_root: &'a str,
    store_root: &'a Path,
}

impl WorkspaceRun<'_> {
    fn create(&self) -> AnyResult<(layerfs_sdk::WorkspaceId, u64, OperationReceipt)> {
        observed(self.client, || {
            self.client
                .create_workspace_session(CreateWorkspaceSession {
                    branch_id: self.branch_id,
                    placement: WorkspacePlacement::Container {
                        container_id: ContainerId(self.container.to_owned()),
                        root: self.mount_root.into(),
                    },
                    projection: Some(WorkspaceProjection::Fuse),
                })
        })
    }

    fn mutation(&self, id: String, command: &[&str]) -> AnyResult<Phases> {
        let before = store_totals(&self.client.monitor_snapshot()?, self.store_root)?;
        let (workspace, workspace_create_ns, create_receipt) = self.create()?;
        let (shell_ns, _) = timed_docker(self.container, command)?;
        let (commit, workspace_commit_api_ns, commit_receipt) = observed(self.client, || {
            self.client.commit_workspace_session(workspace)
        })?;
        require_commit_created(&commit)?;
        let (push, push_api_ns, push_receipt) =
            observed(self.client, || self.client.push_branch(self.branch_id))?;
        require_push(&push)?;
        let (_, workspace_end_ns, end_receipt) = observed(self.client, || {
            self.client
                .end_workspace_session(workspace, EndWorkspaceMode::Clean)
        })?;
        Ok(Phases {
            id,
            workspace_create_ns,
            shell_ns,
            workspace_commit_api_ns,
            push_api_ns,
            workspace_end_ns,
            workspace_create_receipt: Some(create_receipt.to_json()),
            workspace_commit_receipt: Some(commit_receipt.to_json()),
            push_receipt: Some(push_receipt.to_json()),
            workspace_end_receipt: Some(end_receipt.to_json()),
            storage_before: Some(before),
            storage_after: Some(store_totals(
                &self.client.monitor_snapshot()?,
                self.store_root,
            )?),
        })
    }

    fn read(&self, command: &[&str]) -> AnyResult<(Phases, Vec<u8>)> {
        let before = store_totals(&self.client.monitor_snapshot()?, self.store_root)?;
        let (workspace, workspace_create_ns, create_receipt) = self.create()?;
        let (shell_ns, output) = timed_docker(self.container, command)?;
        let (_, workspace_end_ns, end_receipt) = observed(self.client, || {
            self.client
                .end_workspace_session(workspace, EndWorkspaceMode::Clean)
        })?;
        Ok((
            Phases {
                id: "read".to_owned(),
                workspace_create_ns,
                shell_ns,
                workspace_end_ns,
                workspace_create_receipt: Some(create_receipt.to_json()),
                workspace_end_receipt: Some(end_receipt.to_json()),
                storage_before: Some(before),
                storage_after: Some(store_totals(
                    &self.client.monitor_snapshot()?,
                    self.store_root,
                )?),
                ..Phases::default()
            },
            output,
        ))
    }
}

fn verify(state_path: &Path, result_path: &Path) -> AnyResult<()> {
    let state = read_state(state_path)?;
    require_state(&state, "schema", "fs-benchmark-pro-state-v1")?;
    let container = value(&state, "container")?;
    validate_container(container)?;
    let authority_path = PathBuf::from(value(&state, "authority_db")?);
    let branch_path = PathBuf::from(value(&state, "branch_db")?);
    let fuse_helper = PathBuf::from(value(&state, "fuse_helper")?);
    if !fuse_helper.is_file() {
        return Err(format!("reopen FUSE helper is missing: {}", fuse_helper.display()).into());
    }
    std::env::set_var("LAYERFS_FUSE_HELPER", &fuse_helper);
    let expected_store_id = value(&state, "authority_store_id")?.parse::<StoreId>()?;
    let expected_parent = value(&state, "branch_parent_store_id")?.parse::<StoreId>()?;
    let branch_id = value(&state, "branch_id")?.parse::<BranchId>()?;
    let expected_bytes = parse_positive(value(&state, "expected_bytes")?, "expected_bytes")?;
    let expected_digest = value(&state, "expected_digest")?;
    let authority = Arc::new(LayerStackStore::connect(&authority_path)?);
    if authority.store_id() != expected_store_id {
        return Err("reopened LayerStackStore identity changed".into());
    }
    let branches = BranchStore::connect(&branch_path, expected_store_id)?;
    if branches.parent_store_id() != expected_parent || expected_parent != expected_store_id {
        return Err("reopened BranchStore parent binding changed".into());
    }
    let client = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(authority),
        branches,
    })?;
    let remote_script = if std::env::var("LAYERFS_BENCH_SELF_TARGET").as_deref() == Ok("1") {
        workload_script().to_string_lossy().into_owned()
    } else {
        let script = format!("/tmp/fs-benchmark-pro-{}-verify.py", std::process::id());
        copy_to_container(container, &workload_script(), &script)?;
        docker_checked(container, &["chmod", "0555", &script])?;
        script
    };
    let mount_root = format!("/workspace/fs-benchmark-pro-verify-{}", std::process::id());
    let payload = format!("{mount_root}/payload.bin");
    let verify_started = Instant::now();
    let workspace = client.create_workspace_session(CreateWorkspaceSession {
        branch_id,
        placement: WorkspacePlacement::Container {
            container_id: ContainerId(container.to_owned()),
            root: mount_root.into(),
        },
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let output = docker_checked(
        container,
        &[
            "python3",
            &remote_script,
            "verify",
            &payload,
            &expected_bytes.to_string(),
            expected_digest,
        ],
    )?;
    let (actual_bytes, actual_digest) = parse_digest(&output.stdout)?;
    client.end_workspace_session(workspace, EndWorkspaceMode::Discard)?;
    let reopen_verify_ns = elapsed_ns(verify_started);
    let final_snapshot = store_totals(
        &client.monitor_snapshot()?,
        authority_path
            .parent()
            .ok_or("authority DB has no parent")?,
    )?;
    let summary = summary_json(
        &state,
        actual_bytes,
        &actual_digest,
        reopen_verify_ns,
        final_snapshot,
    )?;
    if let Some(parent) = result_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(result_path, format!("{summary}\n"))?;
    println!("{}", result_path.display());
    Ok(())
}

struct StateWrite<'a> {
    container: &'a str,
    authority_path: &'a Path,
    branch_path: &'a Path,
    fuse_helper: &'a Path,
    authority_store_id: StoreId,
    branch_parent_store_id: StoreId,
    branch_id: BranchId,
    file_bytes: u64,
    edit_count: usize,
    fixture_digest: &'a str,
    after_edits_digest: &'a str,
    expected_bytes: u64,
    expected_digest: &'a str,
    mount_root: &'a str,
    initial: StoreTotals,
    authority_checkpoint_storage: StoreTotals,
    final_storage: StoreTotals,
    add_ns: u64,
    operations: &'a [Phases],
    source_commit: &'a str,
    source_dirty: bool,
}

fn write_state(path: &Path, state: &StateWrite<'_>) -> AnyResult<()> {
    let mut lines = vec![
        "schema\tfs-benchmark-pro-state-v1".to_owned(),
        format!("container\t{}", state.container),
        format!("authority_db\t{}", state.authority_path.display()),
        format!("branch_db\t{}", state.branch_path.display()),
        format!("fuse_helper\t{}", state.fuse_helper.display()),
        format!("authority_store_id\t{}", state.authority_store_id),
        format!("branch_parent_store_id\t{}", state.branch_parent_store_id),
        format!("branch_id\t{}", state.branch_id),
        format!("file_bytes\t{}", state.file_bytes),
        format!("edit_count\t{}", state.edit_count),
        format!("fixture_digest\t{}", state.fixture_digest),
        format!("after_edits_digest\t{}", state.after_edits_digest),
        format!("expected_bytes\t{}", state.expected_bytes),
        format!("expected_digest\t{}", state.expected_digest),
        format!("mount_root\t{}", state.mount_root),
        format!("add_ns\t{}", state.add_ns),
        format!("source_commit\t{}", state.source_commit),
        format!("source_dirty\t{}", state.source_dirty),
    ];
    append_storage(&mut lines, "initial", state.initial);
    append_storage(
        &mut lines,
        "authority_checkpoint",
        state.authority_checkpoint_storage,
    );
    append_storage(&mut lines, "final", state.final_storage);
    for operation in state.operations {
        lines.push(format!(
            "operation\t{}\t{}\t{}\t{}\t{}\t{}",
            operation.id,
            operation.workspace_create_ns,
            operation.shell_ns,
            operation.workspace_commit_api_ns,
            operation.push_api_ns,
            operation.workspace_end_ns,
        ));
    }
    fs::write(path, format!("{}\n", lines.join("\n")))?;
    Ok(())
}

fn append_storage(lines: &mut Vec<String>, name: &str, storage: StoreTotals) {
    lines.extend([
        format!("storage_{name}_database_bytes\t{}", storage.database_bytes),
        format!("storage_{name}_wal_bytes\t{}", storage.wal_bytes),
        format!("storage_{name}_shm_bytes\t{}", storage.shm_bytes),
        format!(
            "storage_{name}_durable_allocated_bytes\t{}",
            storage.durable_allocated_bytes
        ),
    ]);
}

fn read_state(path: &Path) -> AnyResult<State> {
    let mut values = BTreeMap::new();
    let mut operations = Vec::new();
    for (line_number, line) in fs::read_to_string(path)?.lines().enumerate() {
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.first() == Some(&"operation") {
            if fields.len() != 7 {
                return Err(format!("invalid operation at state line {}", line_number + 1).into());
            }
            operations.push(Phases {
                id: fields[1].to_owned(),
                workspace_create_ns: fields[2].parse()?,
                shell_ns: fields[3].parse()?,
                workspace_commit_api_ns: fields[4].parse()?,
                push_api_ns: fields[5].parse()?,
                workspace_end_ns: fields[6].parse()?,
                ..Phases::default()
            });
        } else if let [key, item] = fields.as_slice() {
            if values
                .insert((*key).to_owned(), (*item).to_owned())
                .is_some()
            {
                return Err(format!("duplicate state key {key}").into());
            }
        } else {
            return Err(format!("invalid state line {}", line_number + 1).into());
        }
    }
    Ok(State { values, operations })
}

fn summary_json(
    state: &State,
    final_bytes: u64,
    final_digest: &str,
    reopen_verify_ns: u64,
    final_snapshot: StoreTotals,
) -> AnyResult<String> {
    if final_bytes != parse_positive(value(state, "expected_bytes")?, "expected_bytes")?
        || final_digest != value(state, "expected_digest")?
    {
        return Err("reopen verification differs from measured oracle".into());
    }
    let create = operation(state, "create")?;
    let prepend = operation(state, "prepend")?;
    let read = operation(state, "read")?;
    let edits = state
        .operations
        .iter()
        .filter(|operation| operation.id.starts_with("edit-"))
        .collect::<Vec<_>>();
    let expected_edits = parse_positive(value(state, "edit_count")?, "edit_count")? as usize;
    if edits.len() != expected_edits {
        return Err(format!("state has {} edits, expected {expected_edits}", edits.len()).into());
    }
    let edits_sum = edits.iter().fold(0_u64, |sum, operation| {
        sum.saturating_add(operation.authority_checkpoint_ns())
    });
    let comparable_total = state.operations.iter().fold(0_u64, |sum, operation| {
        sum.saturating_add(operation.authority_checkpoint_ns())
    });
    let complete_turn_total = state.operations.iter().fold(0_u64, |sum, operation| {
        sum.saturating_add(operation.complete_turn_ns())
    });
    let operations = state
        .operations
        .iter()
        .map(operation_summary_json)
        .collect::<Vec<_>>()
        .join(",");
    let initial = state_storage(state, "initial")?;
    let authority_checkpoint = state_storage(state, "authority_checkpoint")?;
    let add_ns = parse_positive_or_zero(value(state, "add_ns")?, "add_ns")?;
    Ok(format!(
        "{{\"schema\":\"fs-benchmark-pro-sample-v1\",\"candidate\":\"layerfs-reference\",\"workload\":{{\"initial_bytes\":{},\"initial_sha256\":{},\"edit_count\":{},\"edit_size_bytes\":10,\"prepend_bytes\":10}},\"operations\":[{}],\"aggregates\":{{\"create_ns\":{},\"sixteen_edits_sum_ns\":{},\"prepend_ns\":{},\"read_ns\":{},\"workspace_create_ns\":{},\"workspace_end_ns\":{},\"add_ns\":{},\"comparable_total_ns\":{},\"complete_turn_total_ns\":{}}},\"verification\":{{\"initial_bytes\":{},\"initial_sha256\":{},\"after_edits_sha256\":{},\"final_bytes\":{},\"final_sha256\":{},\"final_digest\":{},\"reopen_passed\":true,\"reopen_verify_ns\":{}}},\"storage\":{{\"initial\":{},\"authority_checkpoint\":{},\"final\":{},\"unavailable\":{{\"logical_bytes\":\"not exposed by the public Store snapshot\",\"semantic_payload_bytes\":\"available by fact/object kind in exact JSONL operation receipts; no single lossless aggregate\",\"wire_bytes\":\"local endpoint has no network transport\"}}}},\"provenance\":{{\"source_commit\":{},\"source_dirty\":{},\"authority_store_id\":{},\"branch_parent_store_id\":{},\"branch_id\":{},\"measured_unix_ns\":{}}},\"status\":\"pass\"}}",
        value(state, "file_bytes")?,
        json_string(value(state, "fixture_digest")?),
        value(state, "edit_count")?,
        operations,
        create.authority_checkpoint_ns(),
        edits_sum,
        prepend.authority_checkpoint_ns(),
        read.shell_ns,
        create.workspace_create_ns,
        read.workspace_end_ns,
        add_ns,
        comparable_total,
        complete_turn_total,
        value(state, "file_bytes")?,
        json_string(value(state, "fixture_digest")?),
        json_string(value(state, "after_edits_digest")?),
        final_bytes,
        json_string(final_digest),
        json_string(final_digest),
        reopen_verify_ns,
        storage_json(initial),
        storage_json(authority_checkpoint),
        storage_json(final_snapshot),
        json_string(value(state, "source_commit")?),
        value(state, "source_dirty")?,
        json_string(value(state, "authority_store_id")?),
        json_string(value(state, "branch_parent_store_id")?),
        json_string(value(state, "branch_id")?),
        now_ns(),
    ))
}

fn operation_summary_json(operation: &Phases) -> String {
    format!(
        "{{\"id\":{},\"workspace_create_ns\":{},\"shell_ns\":{},\"workspace_commit_api_ns\":{},\"push_api_ns\":{},\"workspace_end_ns\":{},\"authority_checkpoint_ns\":{},\"comparable_ns\":{},\"complete_turn_ns\":{}}}",
        json_string(&operation.id),
        operation.workspace_create_ns,
        operation.shell_ns,
        operation.workspace_commit_api_ns,
        operation.push_api_ns,
        operation.workspace_end_ns,
        operation.authority_checkpoint_ns(),
        operation.authority_checkpoint_ns(),
        operation.complete_turn_ns(),
    )
}

struct EvidenceWrite<'a> {
    path: &'a Path,
    file_bytes: u64,
    edit_count: usize,
    fixture_digest: &'a str,
    operations: &'a [Phases],
    add_ns: u64,
    add_receipt: &'a OperationReceipt,
    source_commit: &'a str,
    source_dirty: bool,
}

fn write_evidence(evidence: &EvidenceWrite<'_>) -> AnyResult<()> {
    let EvidenceWrite {
        path,
        file_bytes,
        edit_count,
        fixture_digest,
        operations,
        add_ns,
        add_receipt,
        source_commit,
        source_dirty,
    } = evidence;
    let mut rows = vec![format!(
        "{{\"schema\":\"fs-benchmark-pro-evidence-v1\",\"candidate\":\"layerfs-reference\",\"record\":\"provenance\",\"file_bytes\":{file_bytes},\"edit_count\":{edit_count},\"fixture_sha256\":{},\"source_commit\":{},\"source_dirty\":{},\"unix_ns\":{}}}",
        json_string(fixture_digest),
        json_string(source_commit),
        *source_dirty,
        now_ns(),
    )];
    for operation in *operations {
        rows.push(format!(
            "{{\"schema\":\"fs-benchmark-pro-evidence-v1\",\"candidate\":\"layerfs-reference\",\"record\":\"operation\",\"id\":{},\"workspace_create_ns\":{},\"shell_ns\":{},\"workspace_commit_api_ns\":{},\"push_api_ns\":{},\"workspace_end_ns\":{},\"authority_checkpoint_ns\":{},\"complete_turn_ns\":{},\"storage_before\":{},\"storage_after\":{},\"receipts\":{{\"workspace_create\":{},\"workspace_commit\":{},\"push\":{},\"workspace_end\":{}}}}}",
            json_string(&operation.id),
            operation.workspace_create_ns,
            operation.shell_ns,
            operation.workspace_commit_api_ns,
            operation.push_api_ns,
            operation.workspace_end_ns,
            operation.authority_checkpoint_ns(),
            operation.complete_turn_ns(),
            optional_storage_json(operation.storage_before),
            optional_storage_json(operation.storage_after),
            optional_receipt_json(operation.workspace_create_receipt.as_deref(), "not applicable to this operation"),
            optional_receipt_json(operation.workspace_commit_receipt.as_deref(), "read-only operation has no Workspace Commit"),
            optional_receipt_json(operation.push_receipt.as_deref(), "read-only operation has no Push"),
            optional_receipt_json(operation.workspace_end_receipt.as_deref(), "Workspace remains active"),
        ));
    }
    rows.push(format!(
        "{{\"schema\":\"fs-benchmark-pro-evidence-v1\",\"candidate\":\"layerfs-reference\",\"record\":\"add\",\"elapsed_ns\":{add_ns},\"excluded_from_comparable_total\":true,\"operation_receipt\":{}}}",
        add_receipt.to_json()
    ));
    fs::write(path, format!("{}\n", rows.join("\n")))?;
    Ok(())
}

fn observed<T>(
    client: &Client,
    operation: impl FnOnce() -> layerfs_sdk::Result<T>,
) -> AnyResult<(T, u64, OperationReceipt)> {
    let before = client.monitor_snapshot()?.operations.len();
    let started = Instant::now();
    let result = operation()?;
    let elapsed = elapsed_ns(started);
    let snapshot = client.monitor_snapshot()?;
    let new = snapshot
        .operations
        .get(before..)
        .ok_or("Monitor receipt cursor moved")?;
    if new.len() != 1 {
        return Err(format!("SDK operation emitted {} receipts, expected one", new.len()).into());
    }
    Ok((result, elapsed, new[0].clone()))
}

fn require_commit_created(result: &WorkspaceCommitResult) -> AnyResult<()> {
    match result {
        WorkspaceCommitResult::Created { .. } => Ok(()),
        other => Err(format!("Workspace Commit did not create a durable Commit: {other:?}").into()),
    }
}

fn require_push(result: &PushResult) -> AnyResult<()> {
    match result {
        PushResult::Created { .. } | PushResult::Advanced { .. } => Ok(()),
        other => Err(format!("Push did not advance authority: {other:?}").into()),
    }
}

fn require_add(result: &AddLayerResult) -> AnyResult<()> {
    match result {
        AddLayerResult::Added { .. } | AddLayerResult::UpToDate { .. } => Ok(()),
        other => Err(format!("Add did not accept the final Commit: {other:?}").into()),
    }
}

fn store_totals(snapshot: &MonitorSnapshot, store_root: &Path) -> AnyResult<StoreTotals> {
    let storage = snapshot
        .databases
        .iter()
        .fold(StoreTotals::default(), |mut total, db| {
            total.database_bytes = total
                .database_bytes
                .saturating_add(db.storage.database_bytes);
            total.wal_bytes = total.wal_bytes.saturating_add(db.storage.wal_bytes);
            total.shm_bytes = total.shm_bytes.saturating_add(db.storage.shm_bytes);
            total
        });
    Ok(StoreTotals {
        durable_allocated_bytes: allocated_database_bytes(store_root)?,
        ..storage
    })
}

#[cfg(unix)]
fn allocated_database_bytes(root: &Path) -> AnyResult<u64> {
    use std::os::unix::fs::MetadataExt;
    let mut total = 0_u64;
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".sqlite") || name.contains(".sqlite-"))
        {
            total = total.saturating_add(fs::metadata(path)?.blocks().saturating_mul(512));
        }
    }
    Ok(total)
}

#[cfg(not(unix))]
fn allocated_database_bytes(_root: &Path) -> AnyResult<u64> {
    Err("durable allocated-byte accounting requires Unix st_blocks".into())
}

fn state_storage(state: &State, name: &str) -> AnyResult<StoreTotals> {
    Ok(StoreTotals {
        database_bytes: value(state, &format!("storage_{name}_database_bytes"))?.parse()?,
        wal_bytes: value(state, &format!("storage_{name}_wal_bytes"))?.parse()?,
        shm_bytes: value(state, &format!("storage_{name}_shm_bytes"))?.parse()?,
        durable_allocated_bytes: value(state, &format!("storage_{name}_durable_allocated_bytes"))?
            .parse()?,
    })
}

fn storage_json(storage: StoreTotals) -> String {
    format!(
        "{{\"logical_bytes\":null,\"database_bytes\":{},\"wal_bytes\":{},\"shm_bytes\":{},\"durable_allocated_bytes\":{},\"semantic_payload_bytes\":null,\"wire_bytes\":null}}",
        storage.database_bytes,
        storage.wal_bytes,
        storage.shm_bytes,
        storage.durable_allocated_bytes,
    )
}

fn optional_storage_json(storage: Option<StoreTotals>) -> String {
    storage
        .map(storage_json)
        .unwrap_or_else(|| "null".to_owned())
}

fn optional_receipt_json(receipt: Option<&str>, reason: &str) -> String {
    receipt.map(str::to_owned).unwrap_or_else(|| {
        format!(
            "{{\"value\":null,\"unavailable_reason\":{}}}",
            json_string(reason)
        )
    })
}

fn operation<'a>(state: &'a State, id: &str) -> AnyResult<&'a Phases> {
    state
        .operations
        .iter()
        .find(|operation| operation.id == id)
        .ok_or_else(|| format!("state operation {id} is missing").into())
}

fn value<'a>(state: &'a State, key: &str) -> AnyResult<&'a str> {
    state
        .values
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("state key {key} is missing").into())
}

fn require_state(state: &State, key: &str, expected: &str) -> AnyResult<()> {
    let actual = value(state, key)?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!("state {key} is {actual}, expected {expected}").into())
    }
}

fn workload_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("workload.py")
}

fn host_digest(path: &Path) -> AnyResult<(u64, String)> {
    if !path.is_file() {
        return Err(format!("neutral fixture is missing: {}", path.display()).into());
    }
    let output = Command::new("python3")
        .arg(workload_script())
        .arg("digest")
        .arg(path)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "fixture digest failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    parse_digest(&output.stdout)
}

fn host_oracles(
    results: &Path,
    fixture: &Path,
    file_bytes: u64,
    edit_count: usize,
) -> AnyResult<(String, String)> {
    let oracle = results.join(".fs-benchmark-pro-oracle.bin");
    let status = Command::new("python3")
        .arg(workload_script())
        .args(["create"])
        .arg(fixture)
        .arg(&oracle)
        .status()?;
    if !status.success() {
        return Err("neutral oracle copy failed".into());
    }
    for index in 0..edit_count {
        let status = Command::new("python3")
            .arg(workload_script())
            .arg("edit")
            .arg(&oracle)
            .arg(index.to_string())
            .arg(file_bytes.to_string())
            .status()?;
        if !status.success() {
            return Err(format!("neutral oracle edit {} failed", index + 1).into());
        }
    }
    let (_, after_edits) = host_digest(&oracle)?;
    let status = Command::new("python3")
        .arg(workload_script())
        .arg("prepend")
        .arg(&oracle)
        .status()?;
    if !status.success() {
        return Err("neutral oracle prepend failed".into());
    }
    let (_, final_digest) = host_digest(&oracle)?;
    fs::remove_file(oracle)?;
    Ok((after_edits, final_digest))
}

fn parse_digest(bytes: &[u8]) -> AnyResult<(u64, String)> {
    let line = std::str::from_utf8(bytes)?.trim();
    let (size, digest) = line
        .split_once('\t')
        .ok_or("invalid workload digest output")?;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid workload SHA-256".into());
    }
    Ok((size.parse()?, digest.to_ascii_lowercase()))
}

fn copy_to_container(container: &str, source: &Path, destination: &str) -> AnyResult<()> {
    let output = Command::new("docker")
        .arg("cp")
        .arg(source)
        .arg(format!("{container}:{destination}"))
        .output()?;
    checked_output(output, "docker cp").map(|_| ())
}

fn copy_from_container(container: &str, source: &str, destination: &Path) -> AnyResult<()> {
    let output = Command::new("docker")
        .arg("cp")
        .arg(format!("{container}:{source}"))
        .arg(destination)
        .output()?;
    checked_output(output, "docker cp").map(|_| ())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> AnyResult<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o555))?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> AnyResult<()> {
    Err("FUSE helper extraction requires Unix permissions".into())
}

fn docker_checked(container: &str, arguments: &[&str]) -> AnyResult<Output> {
    let output = Command::new("docker")
        .arg("exec")
        .arg(container)
        .args(arguments)
        .output()?;
    checked_output(output, "docker exec")
}

fn timed_docker(container: &str, arguments: &[&str]) -> AnyResult<(u64, Vec<u8>)> {
    let started = Instant::now();
    let output = docker_checked(container, arguments)?;
    Ok((elapsed_ns(started), output.stdout))
}

fn checked_output(output: Output, action: &str) -> AnyResult<Output> {
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!(
            "{action} failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

fn validate_container(value: &str) -> AnyResult<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        Err("CONTAINER must contain only ASCII letters, digits, '.', '_' or '-'".into())
    } else {
        Ok(())
    }
}

fn parse_positive(value: &str, name: &str) -> AnyResult<u64> {
    let parsed = value.parse::<u64>()?;
    if parsed == 0 {
        Err(format!("{name} must be positive").into())
    } else {
        Ok(parsed)
    }
}

fn parse_positive_or_zero(value: &str, name: &str) -> AnyResult<u64> {
    value
        .parse::<u64>()
        .map_err(|error| format!("invalid {name}: {error}").into())
}

fn elapsed_ns(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}

fn now_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn source_provenance() -> (String, bool) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let git_commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&root)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_owned());
    let commit = git_commit
        .or_else(|| std::env::var("LAYERFS_SOURCE_COMMIT").ok())
        .filter(|value| value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .unwrap_or_else(|| "unavailable".to_owned());
    let git_clean = Command::new("git")
        .args(["diff", "--quiet", "--ignore-submodules", "HEAD", "--"])
        .current_dir(root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()
        .map(|status| status.success());
    let dirty = git_clean.map_or_else(
        || {
            std::env::var("LAYERFS_SOURCE_DIRTY")
                .ok()
                .is_none_or(|value| matches!(value.as_str(), "1" | "true" | "yes"))
        },
        |clean| !clean,
    );
    (commit, dirty)
}

fn json_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            value if value < ' ' => {
                use std::fmt::Write as _;
                let _ = write!(output, "\\u{:04x}", value as u32);
            }
            value => output.push(value),
        }
    }
    output.push('"');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_and_phase_equations_are_stable() {
        assert_eq!(json_string("a\n\"b\\"), "\"a\\n\\\"b\\\\\"");
        let phases = Phases {
            workspace_create_ns: 2,
            shell_ns: 3,
            workspace_commit_api_ns: 5,
            push_api_ns: 7,
            workspace_end_ns: 11,
            ..Phases::default()
        };
        assert_eq!(phases.authority_checkpoint_ns(), 15);
        assert_eq!(phases.complete_turn_ns(), 28);
    }
}
