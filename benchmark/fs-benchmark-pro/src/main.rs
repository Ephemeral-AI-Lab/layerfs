use layerfs_sdk::{
    AddLayerResult, BranchId, BranchStore, Client, ConnectionContext, ContainerId,
    CreateWorkspaceSession, EndWorkspaceMode, EntityName, ExecutionReceipt, LayerStackEndpoint,
    LayerStackInitialization, LayerStackStore, LocalForkSource, MonitorSnapshot, NonEmpty,
    OperationFamily, OperationReceipt, OutputStream, PushResult, RemotePlacement, StorageReceipt,
    StoreId, StoreRole, WorkspaceCommitResult, WorkspaceId, WorkspacePlacement,
    WorkspaceProjection,
};
use std::collections::BTreeMap;
use std::ffi::OsString;
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
    sdk_exec_to_terminal_ns: u64,
    sdk_exec_dispatch_ns: u64,
    sdk_output_handle_ns: u64,
    sdk_output_follow_ns: u64,
    sdk_exec_unattributed_ns: u64,
    execution_exit_code: Option<i32>,
    execution_stopped: bool,
    execution_stdout_bytes: u64,
    execution_stderr_bytes: u64,
    execution_elapsed_ns: u64,
    execution_total_wall_ns: u64,
    execution_spawn_ns: u64,
    execution_supervisor_queue_ns: u64,
    execution_runtime_ns: u64,
    execution_drain_ns: u64,
    execution_terminal_ns: u64,
    execution_unattributed_ns: u64,
    execution_direct_engine: bool,
    workspace_commit_api_ns: u64,
    push_api_ns: u64,
    workspace_end_ns: u64,
    workspace_create_receipt: Option<String>,
    workspace_exec_receipt: Option<String>,
    workspace_output_receipt: Option<String>,
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

    fn comparable_ns(&self) -> u64 {
        self.complete_turn_ns()
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
        [mode, container, results, diagnostic, sample] if mode == "diagnose" => {
            diagnose_execution(container, Path::new(results), DEFAULT_FILE_MIB, diagnostic, sample)
        }
        [mode, state, result] if mode == "verify" => verify(Path::new(state), Path::new(result)),
        _ => Err(
            "usage: fs-benchmark-pro self-check | measure CONTAINER RESULT_DIR [FILE_MIB EDIT_COUNT] | verify STATE_TSV RESULT_PATH | diagnose CONTAINER RESULT_DIR true|bash|helper|edit SAMPLE"
                .into(),
        ),
    }
}

fn self_check() -> AnyResult<()> {
    let status = Command::new(workload_binary()).arg("self-check").status()?;
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
    if file_mib != DEFAULT_FILE_MIB || edit_count != DEFAULT_EDITS {
        return Err("registered fs-benchmark-pro profile is fixed at 32 MiB and 16 edits".into());
    }
    let file_bytes = file_mib
        .checked_mul(1024 * 1024)
        .ok_or("FILE_MIB overflow")?;
    if file_bytes <= PREPEND_BYTES {
        return Err("fixture must exceed the 10-byte edit marker".into());
    }
    fs::create_dir_all(results)?;
    let results = fs::canonicalize(results)?;
    let fixture = std::env::var_os("LAYERFS_BENCH_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| results.join("fixture.bin"));
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
    let after_edits_digest = ARTICLE_EDIT_SHA256.to_owned();
    let final_oracle_digest = ARTICLE_FINAL_SHA256.to_owned();

    let evidence_path = results.join("layerfs-reference.jsonl");
    let state_path = results.join("layerfs-reference-state.tsv");
    let cold_store_root = results.join("layerfs-cold-create-store");
    let store_root = results.join("layerfs-reference-edit-store");
    let fuse_helper = results.join("layerfs-fuse");
    let mountinfo_path = results.join("edit-mountinfo.txt");
    if evidence_path.exists()
        || state_path.exists()
        || cold_store_root.exists()
        || store_root.exists()
        || fuse_helper.exists()
        || mountinfo_path.exists()
    {
        return Err("LayerFS result files already exist; use a fresh RESULT_DIR".into());
    }
    copy_from_container(container, "/usr/local/bin/layerfs-fuse", &fuse_helper)?;
    make_executable(&fuse_helper)?;
    std::env::set_var("LAYERFS_FUSE_HELPER", &fuse_helper);

    let self_target = std::env::var("LAYERFS_BENCH_SELF_TARGET").as_deref() == Ok("1");
    let (remote_workload, remote_fixture) = if self_target {
        (
            workload_binary().to_string_lossy().into_owned(),
            fixture.to_string_lossy().into_owned(),
        )
    } else {
        let workload = format!("/tmp/fs-benchmark-pro-{}-workload", std::process::id());
        let fixture_path = format!("/tmp/fs-benchmark-pro-{}-fixture.bin", std::process::id());
        copy_to_container(container, &workload_binary(), &workload)?;
        copy_to_container(container, &fixture, &fixture_path)?;
        docker_checked(container, &["chmod", "0555", &workload])?;
        docker_checked(container, &["chmod", "0444", &fixture_path])?;
        (workload, fixture_path)
    };
    let mount_root = format!("/workspace/fs-benchmark-pro-{}", std::process::id());
    let payload = format!("{mount_root}/payload.bin");
    let mut operations = Vec::with_capacity(edit_count + 3);

    fs::create_dir(&store_root)?;
    let authority_path = store_root.join("authority.sqlite");
    let branch_path = store_root.join("branch.sqlite");
    let authority = Arc::new(LayerStackStore::create(&authority_path)?);
    let authority_store_id = authority.store_id();
    let branches = BranchStore::create(&branch_path, authority_store_id)?;
    let branch_store_id = branches.store_id();
    let branch_parent_store_id = branches.parent_store_id();
    let client = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(authority.clone()),
        branches: branches.clone(),
    })?;
    let genesis = client
        .initialize_layerstack(
            EntityName::new("fs-benchmark-pro-edit")?,
            LayerStackInitialization::Directory(
                fixture
                    .parent()
                    .ok_or("fixture has no seed directory")?
                    .to_owned(),
            ),
        )?
        .genesis_layer_id;
    client.pull_layer(genesis, RemotePlacement::Reference)?;
    if inventory_totals(&branches)? != (0, 0) {
        return Err("Reference Pull copied authority base objects into BranchStore".into());
    }
    let branch_id = client.fork_branch(
        EntityName::new("fs-benchmark-pro-edit-main")?,
        LocalForkSource::Layer { layer_id: genesis },
    )?;
    if inventory_totals(&branches)? != (0, 0) {
        return Err("Reference Fork copied canonical objects".into());
    }
    drop(client);
    drop(branches);
    drop(authority);

    let authority = Arc::new(LayerStackStore::connect(&authority_path)?);
    if authority.store_id() != authority_store_id {
        return Err("seeded LayerStackStore identity changed on reopen".into());
    }
    let branches = BranchStore::connect(&branch_path, authority_store_id)?;
    if branches.store_id() != branch_store_id {
        return Err("seeded BranchStore identity changed on reopen".into());
    }
    let client = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(authority.clone()),
        branches,
    })?;
    let initial = store_totals(&client.monitor_snapshot()?, &store_root)?;
    let runner = WorkspaceRun {
        client: &client,
        branch_id,
        container,
        mount_root: &mount_root,
        store_root: &store_root,
        authority_store_id,
        branch_store_id,
    };

    let edits_before = store_totals(&client.monitor_snapshot()?, &store_root)?;
    let (edit_workspace, edit_create_ns, edit_create_receipt) = runner.create()?;
    retain_mountinfo(&branch_path, edit_workspace, &mount_root, &mountinfo_path)?;
    let first_edit = operations.len();
    for index in 0..edit_count {
        let id = format!("edit-{:02}", index + 1);
        let index = index.to_string();
        let bytes = file_bytes.to_string();
        operations.push(runner.mutation_open(
            edit_workspace,
            id,
            &[
                "/bin/bash",
                "-lc",
                "\"$@\"",
                "fs-bench-shell",
                &remote_workload,
                "edit",
                &payload,
                &index,
                &bytes,
            ],
        )?);
    }
    let (_, edit_end_ns, edit_end_receipt) = observed(&client, || {
        client.end_workspace_session(edit_workspace, EndWorkspaceMode::Clean)
    })?;
    let last_edit = operations.len() - 1;
    operations[first_edit].workspace_create_ns = edit_create_ns;
    operations[first_edit].workspace_create_receipt = Some(edit_create_receipt.to_json());
    operations[first_edit].storage_before = Some(edits_before);
    operations[last_edit].workspace_end_ns = edit_end_ns;
    operations[last_edit].workspace_end_receipt = Some(edit_end_receipt.to_json());
    operations[last_edit].storage_after =
        Some(store_totals(&client.monitor_snapshot()?, &store_root)?);

    operations.push(runner.mutation(
        "prepend".to_owned(),
        &[&remote_workload, "prepend", &payload],
    )?);

    let (read, output) = runner.read(&[&remote_workload, "read", &payload])?;
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

    fs::create_dir(&cold_store_root)?;
    let create = {
        let authority = Arc::new(LayerStackStore::create(
            cold_store_root.join("authority.sqlite"),
        )?);
        let authority_store_id = authority.store_id();
        let branches =
            BranchStore::create(cold_store_root.join("branch.sqlite"), authority_store_id)?;
        let branch_store_id = branches.store_id();
        let client = Client::connect(ConnectionContext {
            layerstack: LayerStackEndpoint::local(authority.clone()),
            branches,
        })?;
        let genesis = client
            .initialize_layerstack(
                EntityName::new("fs-benchmark-pro-cold")?,
                LayerStackInitialization::Empty,
            )?
            .genesis_layer_id;
        client.pull_layer(genesis, RemotePlacement::Reference)?;
        let branch_id = client.fork_branch(
            EntityName::new("fs-benchmark-pro-cold-main")?,
            LocalForkSource::Layer { layer_id: genesis },
        )?;
        WorkspaceRun {
            client: &client,
            branch_id,
            container,
            mount_root: &mount_root,
            store_root: &cold_store_root,
            authority_store_id,
            branch_store_id,
        }
        .mutation(
            "create".to_owned(),
            &[&remote_workload, "create", &remote_fixture, &payload],
        )?
    };
    operations.insert(0, create);
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
            branch_store_id,
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

fn retain_mountinfo(
    branch_path: &Path,
    workspace: WorkspaceId,
    mount_root: &str,
    destination: &Path,
) -> AnyResult<()> {
    let source = workspace_mountinfo_path(branch_path, &workspace.to_string());
    let mountinfo = fs::read_to_string(&source)?;
    let fields = mountinfo.split_whitespace().collect::<Vec<_>>();
    if fields.get(4) != Some(&mount_root)
        || !fields
            .windows(3)
            .any(|fields| fields[0] == "-" && fields[1].starts_with("fuse"))
    {
        return Err("captured Workspace mountinfo does not identify the live FUSE mount".into());
    }
    fs::write(destination, mountinfo)?;
    Ok(())
}

fn workspace_mountinfo_path(branch_path: &Path, workspace: &str) -> PathBuf {
    branch_path
        .with_extension("sqlite.runtime")
        .join("workspaces")
        .join("workspaces")
        .join(workspace)
        .join("mountinfo.txt")
}

fn diagnose_execution(
    container: &str,
    results: &Path,
    file_mib: u64,
    diagnostic: &str,
    sample: &str,
) -> AnyResult<()> {
    validate_container(container)?;
    if !matches!(diagnostic, "true" | "bash" | "helper" | "edit") {
        return Err("invalid execution diagnostic".into());
    }
    let sample_number = sample.parse::<u8>()?;
    if !(1..=9).contains(&sample_number) || sample != format!("{sample_number:03}") {
        return Err("invalid execution diagnostic sample".into());
    }
    let results = fs::canonicalize(results)?;
    let diagnostics_path = results.join(format!("execution-diagnostic-{diagnostic}-{sample}.json"));
    let store_root = results.join(format!(
        "layerfs-execution-diagnostic-{diagnostic}-{sample}-store"
    ));
    if diagnostics_path.exists() || store_root.exists() {
        return Err("execution diagnostics already exist".into());
    }
    let fixture = std::env::var_os("LAYERFS_BENCH_FIXTURE")
        .map(PathBuf::from)
        .ok_or("execution diagnostics require LAYERFS_BENCH_FIXTURE")?;
    let file_bytes = file_mib
        .checked_mul(1024 * 1024)
        .ok_or("FILE_MIB overflow")?;
    if fs::metadata(&fixture)?.len() != file_bytes {
        return Err("execution diagnostic fixture size mismatch".into());
    }
    fs::create_dir(&store_root)?;
    let authority = Arc::new(LayerStackStore::create(
        store_root.join("authority.sqlite"),
    )?);
    let branches = BranchStore::create(store_root.join("branch.sqlite"), authority.store_id())?;
    let branch_store_id = branches.store_id();
    let client = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(authority.clone()),
        branches,
    })?;
    let genesis = client
        .initialize_layerstack(
            EntityName::new("fs-benchmark-pro-diagnostic")?,
            LayerStackInitialization::Directory(
                fixture
                    .parent()
                    .ok_or("diagnostic fixture parent")?
                    .to_owned(),
            ),
        )?
        .genesis_layer_id;
    client.pull_layer(genesis, RemotePlacement::Reference)?;
    let branch_id = client.fork_branch(
        EntityName::new("fs-benchmark-pro-diagnostic-main")?,
        LocalForkSource::Layer { layer_id: genesis },
    )?;
    let mount_root = format!(
        "/workspace/fs-benchmark-pro-diagnostic-{}",
        std::process::id()
    );
    let payload = format!("{mount_root}/payload.bin");
    let workload = workload_binary().to_string_lossy().into_owned();
    let runner = WorkspaceRun {
        client: &client,
        branch_id,
        container,
        mount_root: &mount_root,
        store_root: &store_root,
        authority_store_id: authority.store_id(),
        branch_store_id,
    };
    let (workspace, workspace_create_ns, _) = runner.create()?;
    let file_bytes = file_bytes.to_string();
    let argv = match diagnostic {
        "true" => vec!["/bin/true"],
        "bash" => vec!["/bin/bash", "-lc", ":"],
        "helper" => vec![
            "/bin/bash",
            "-lc",
            "\"$@\"",
            "fs-bench-shell",
            &workload,
            "noop",
        ],
        "edit" => vec![
            "/bin/bash",
            "-lc",
            "\"$@\"",
            "fs-bench-shell",
            &workload,
            "edit",
            &payload,
            "0",
            &file_bytes,
        ],
        _ => unreachable!(),
    };
    let execution = execute_workspace_command(&client, workspace, &argv)?;
    let (_, workspace_end_ns, _) = observed(&client, || {
        client.end_workspace_session(workspace, EndWorkspaceMode::Discard)
    })?;
    write_execution_diagnostic(
        &diagnostics_path,
        diagnostic,
        sample,
        &argv,
        workspace_create_ns,
        workspace_end_ns,
        &execution,
    )
}

struct WorkspaceRun<'a> {
    client: &'a Client,
    branch_id: BranchId,
    container: &'a str,
    mount_root: &'a str,
    store_root: &'a Path,
    authority_store_id: StoreId,
    branch_store_id: StoreId,
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
        let mut phases = self.mutation_open(workspace, id, command)?;
        let (_, workspace_end_ns, end_receipt) = observed(self.client, || {
            self.client
                .end_workspace_session(workspace, EndWorkspaceMode::Clean)
        })?;
        phases.workspace_create_ns = workspace_create_ns;
        phases.workspace_end_ns = workspace_end_ns;
        phases.workspace_create_receipt = Some(create_receipt.to_json());
        phases.workspace_end_receipt = Some(end_receipt.to_json());
        phases.storage_before = Some(before);
        phases.storage_after = Some(store_totals(
            &self.client.monitor_snapshot()?,
            self.store_root,
        )?);
        Ok(phases)
    }

    fn mutation_open(
        &self,
        workspace: WorkspaceId,
        id: String,
        command: &[&str],
    ) -> AnyResult<Phases> {
        let execution = execute_workspace_command(self.client, workspace, command)?;
        let (commit, workspace_commit_api_ns, commit_receipt) = observed(self.client, || {
            self.client.commit_workspace_session(workspace)
        })?;
        require_commit_created(&commit)?;
        require_normal_commit_rebased(&commit_receipt)?;
        let (push, push_api_ns, push_receipt) =
            observed(self.client, || self.client.push_branch(self.branch_id))?;
        require_push(&push)?;
        require_push_durability(&push_receipt, self.authority_store_id, self.branch_store_id)?;
        Ok(Phases {
            id,
            shell_ns: execution.sdk_exec_to_terminal_ns,
            sdk_exec_to_terminal_ns: execution.sdk_exec_to_terminal_ns,
            sdk_exec_dispatch_ns: execution.sdk_exec_dispatch_ns,
            sdk_output_handle_ns: execution.sdk_output_handle_ns,
            sdk_output_follow_ns: execution.sdk_output_follow_ns,
            sdk_exec_unattributed_ns: execution.sdk_exec_unattributed_ns,
            execution_exit_code: execution.receipt.exit_code,
            execution_stopped: execution.receipt.stopped,
            execution_stdout_bytes: execution.receipt.stdout_bytes,
            execution_stderr_bytes: execution.receipt.stderr_bytes,
            execution_elapsed_ns: execution.receipt.elapsed_ns,
            execution_total_wall_ns: execution.receipt.total_wall_ns,
            execution_spawn_ns: execution.receipt.spawn_ns,
            execution_supervisor_queue_ns: execution.receipt.supervisor_queue_ns,
            execution_runtime_ns: execution.receipt.runtime_ns,
            execution_drain_ns: execution.receipt.drain_ns,
            execution_terminal_ns: execution.receipt.terminal_publication_ns,
            execution_unattributed_ns: execution.receipt.unattributed_ns,
            execution_direct_engine: execution.receipt.direct_engine,
            workspace_commit_api_ns,
            push_api_ns,
            workspace_exec_receipt: Some(execution.exec_receipt.to_json()),
            workspace_output_receipt: Some(execution.output_receipt.to_json()),
            workspace_commit_receipt: Some(commit_receipt.to_json()),
            push_receipt: Some(push_receipt.to_json()),
            ..Phases::default()
        })
    }

    fn read(&self, command: &[&str]) -> AnyResult<(Phases, Vec<u8>)> {
        let before = store_totals(&self.client.monitor_snapshot()?, self.store_root)?;
        let (workspace, workspace_create_ns, create_receipt) = self.create()?;
        let execution = execute_workspace_command(self.client, workspace, command)?;
        let (_, workspace_end_ns, end_receipt) = observed(self.client, || {
            self.client
                .end_workspace_session(workspace, EndWorkspaceMode::Clean)
        })?;
        Ok((
            Phases {
                id: "read".to_owned(),
                workspace_create_ns,
                shell_ns: execution.sdk_exec_to_terminal_ns,
                sdk_exec_to_terminal_ns: execution.sdk_exec_to_terminal_ns,
                sdk_exec_dispatch_ns: execution.sdk_exec_dispatch_ns,
                sdk_output_handle_ns: execution.sdk_output_handle_ns,
                sdk_output_follow_ns: execution.sdk_output_follow_ns,
                sdk_exec_unattributed_ns: execution.sdk_exec_unattributed_ns,
                execution_exit_code: execution.receipt.exit_code,
                execution_stopped: execution.receipt.stopped,
                execution_stdout_bytes: execution.receipt.stdout_bytes,
                execution_stderr_bytes: execution.receipt.stderr_bytes,
                execution_elapsed_ns: execution.receipt.elapsed_ns,
                execution_total_wall_ns: execution.receipt.total_wall_ns,
                execution_spawn_ns: execution.receipt.spawn_ns,
                execution_supervisor_queue_ns: execution.receipt.supervisor_queue_ns,
                execution_runtime_ns: execution.receipt.runtime_ns,
                execution_drain_ns: execution.receipt.drain_ns,
                execution_terminal_ns: execution.receipt.terminal_publication_ns,
                execution_unattributed_ns: execution.receipt.unattributed_ns,
                execution_direct_engine: execution.receipt.direct_engine,
                workspace_end_ns,
                workspace_create_receipt: Some(create_receipt.to_json()),
                workspace_exec_receipt: Some(execution.exec_receipt.to_json()),
                workspace_output_receipt: Some(execution.output_receipt.to_json()),
                workspace_end_receipt: Some(end_receipt.to_json()),
                storage_before: Some(before),
                storage_after: Some(store_totals(
                    &self.client.monitor_snapshot()?,
                    self.store_root,
                )?),
                ..Phases::default()
            },
            execution.stdout,
        ))
    }
}

struct WorkspaceCommand {
    sdk_exec_to_terminal_ns: u64,
    sdk_exec_dispatch_ns: u64,
    sdk_output_handle_ns: u64,
    sdk_output_follow_ns: u64,
    sdk_exec_unattributed_ns: u64,
    stdout: Vec<u8>,
    receipt: ExecutionReceipt,
    exec_receipt: OperationReceipt,
    output_receipt: OperationReceipt,
}

fn write_execution_diagnostic(
    path: &Path,
    name: &str,
    sample: &str,
    argv: &[&str],
    workspace_create_ns: u64,
    workspace_end_ns: u64,
    command: &WorkspaceCommand,
) -> AnyResult<()> {
    let argv = argv
        .iter()
        .map(|value| json_string(value))
        .collect::<Vec<_>>()
        .join(",");
    fs::write(
        path,
        format!(
            "{{\"schema\":\"fs-benchmark-pro-execution-diagnostic-v3\",\"headline\":false,\"fresh_container\":true,\"fresh_store_pair\":true,\"diagnostic_prewarm\":false,\"os_page_cache\":\"uncontrolled\",\"name\":{},\"sample\":{},\"exact_argv\":[{}],\"workspace_create_ns\":{},\"workspace_end_ns\":{},\"sdk_exec_to_terminal_ns\":{},\"sdk_exec_dispatch_ns\":{},\"sdk_output_handle_ns\":{},\"sdk_output_follow_ns\":{},\"sdk_exec_unattributed_ns\":{},\"execution\":{{\"elapsed_ns\":{},\"total_wall_ns\":{},\"spawn_ns\":{},\"supervisor_queue_ns\":{},\"runtime_ns\":{},\"drain_ns\":{},\"terminal_publication_ns\":{},\"unattributed_ns\":{},\"timing_balanced\":{},\"direct_engine\":{},\"stdout_bytes\":{},\"stderr_bytes\":{}}},\"bash_path\":\"/bin/bash\"}}\n",
            json_string(name),
            json_string(sample),
            argv,
            workspace_create_ns,
            workspace_end_ns,
            command.sdk_exec_to_terminal_ns,
            command.sdk_exec_dispatch_ns,
            command.sdk_output_handle_ns,
            command.sdk_output_follow_ns,
            command.sdk_exec_unattributed_ns,
            command.receipt.elapsed_ns,
            command.receipt.total_wall_ns,
            command.receipt.spawn_ns,
            command.receipt.supervisor_queue_ns,
            command.receipt.runtime_ns,
            command.receipt.drain_ns,
            command.receipt.terminal_publication_ns,
            command.receipt.unattributed_ns,
            command.receipt.timing_balanced(),
            command.receipt.direct_engine,
            command.receipt.stdout_bytes,
            command.receipt.stderr_bytes,
        ),
    )?;
    Ok(())
}

fn execute_workspace_command(
    client: &Client,
    workspace: WorkspaceId,
    command: &[&str],
) -> AnyResult<WorkspaceCommand> {
    let argv = NonEmpty::new(command.iter().map(OsString::from).collect())?;
    let outer_started = Instant::now();
    let started = Instant::now();
    let execution = client.exec_workspace_session(workspace, argv)?;
    let sdk_exec_dispatch_ns = elapsed_ns(started);
    let started = Instant::now();
    let output = client.workspace_output(execution.id)?;
    let sdk_output_handle_ns = elapsed_ns(started);
    let output_started = Instant::now();
    let mut after = 0;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let receipt = loop {
        let page = output.read(after, true)?;
        if page.truncated {
            return Err("required Workspace output was truncated".into());
        }
        after = page.next_sequence;
        for chunk in page.chunks {
            match chunk.stream {
                OutputStream::Stdout => stdout.extend_from_slice(&chunk.bytes),
                OutputStream::Stderr => stderr.extend_from_slice(&chunk.bytes),
            }
        }
        if page.exited {
            break page
                .receipt
                .ok_or("terminal Workspace receipt is missing")?;
        }
    };
    let sdk_output_follow_ns = elapsed_ns(output_started);
    let sdk_exec_to_terminal_ns = elapsed_ns(outer_started);
    let attributed = sdk_exec_dispatch_ns
        .saturating_add(sdk_output_handle_ns)
        .saturating_add(sdk_output_follow_ns);
    let sdk_exec_unattributed_ns = sdk_exec_to_terminal_ns.saturating_sub(attributed);
    if attributed > sdk_exec_to_terminal_ns
        || sdk_exec_to_terminal_ns != attributed.saturating_add(sdk_exec_unattributed_ns)
    {
        return Err("SDK Exec-to-terminal timing is unbalanced".into());
    }
    if !receipt.timing_balanced() {
        return Err("Workspace execution receipt timing is unbalanced".into());
    }
    if std::env::var("LAYERFS_BENCH_REQUIRE_DIRECT_ENGINE").as_deref() == Ok("1")
        && !receipt.direct_engine
    {
        return Err("Workspace execution did not use the required direct Engine transport".into());
    }
    if receipt.exit_code != Some(0) || receipt.stopped {
        return Err(format!(
            "Workspace execution failed: exit={:?} stopped={} stderr={}",
            receipt.exit_code,
            receipt.stopped,
            String::from_utf8_lossy(&stderr)
        )
        .into());
    }
    let snapshot = client.monitor_snapshot()?;
    let operation = |family| {
        snapshot
            .operations
            .iter()
            .rev()
            .find(|receipt| {
                receipt.operation.family == family
                    && receipt.operation.execution_id == Some(execution.id)
            })
            .cloned()
            .ok_or("Workspace execution operation receipt is missing")
    };
    let exec_receipt = operation(OperationFamily::WorkspaceExec)?;
    let output_receipt = operation(OperationFamily::WorkspaceOutput)?;
    Ok(WorkspaceCommand {
        sdk_exec_to_terminal_ns,
        sdk_exec_dispatch_ns,
        sdk_output_handle_ns,
        sdk_output_follow_ns,
        sdk_exec_unattributed_ns,
        stdout,
        receipt,
        exec_receipt,
        output_receipt,
    })
}

fn verify(state_path: &Path, result_path: &Path) -> AnyResult<()> {
    let state = read_state(state_path)?;
    require_state(&state, "schema", "fs-benchmark-pro-state-v4")?;
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
    let expected_branch_store_id = value(&state, "branch_store_id")?.parse::<StoreId>()?;
    let expected_parent = value(&state, "branch_parent_store_id")?.parse::<StoreId>()?;
    let branch_id = value(&state, "branch_id")?.parse::<BranchId>()?;
    let expected_bytes = parse_positive(value(&state, "expected_bytes")?, "expected_bytes")?;
    let expected_digest = value(&state, "expected_digest")?;
    let authority = Arc::new(LayerStackStore::connect(&authority_path)?);
    if authority.store_id() != expected_store_id {
        return Err("reopened LayerStackStore identity changed".into());
    }
    let branches = BranchStore::connect(&branch_path, expected_store_id)?;
    if branches.store_id() != expected_branch_store_id {
        return Err("reopened BranchStore identity changed".into());
    }
    if branches.parent_store_id() != expected_parent || expected_parent != expected_store_id {
        return Err("reopened BranchStore parent binding changed".into());
    }
    let client = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(authority),
        branches,
    })?;
    let remote_workload = if std::env::var("LAYERFS_BENCH_SELF_TARGET").as_deref() == Ok("1") {
        workload_binary().to_string_lossy().into_owned()
    } else {
        let workload = format!("/tmp/fs-benchmark-pro-{}-verify", std::process::id());
        copy_to_container(container, &workload_binary(), &workload)?;
        docker_checked(container, &["chmod", "0555", &workload])?;
        workload
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
    let execution = execute_workspace_command(
        &client,
        workspace,
        &[
            &remote_workload,
            "verify",
            &payload,
            &expected_bytes.to_string(),
            expected_digest,
        ],
    )?;
    let (actual_bytes, actual_digest) = parse_digest(&execution.stdout)?;
    client.end_workspace_session(workspace, EndWorkspaceMode::Clean)?;
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
    branch_store_id: StoreId,
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
        "schema\tfs-benchmark-pro-state-v4".to_owned(),
        format!("container\t{}", state.container),
        format!("authority_db\t{}", state.authority_path.display()),
        format!("branch_db\t{}", state.branch_path.display()),
        format!("fuse_helper\t{}", state.fuse_helper.display()),
        format!("authority_store_id\t{}", state.authority_store_id),
        format!("branch_store_id\t{}", state.branch_store_id),
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
            "operation\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            operation.id,
            operation.workspace_create_ns,
            operation.shell_ns,
            operation.sdk_exec_to_terminal_ns,
            operation.sdk_exec_dispatch_ns,
            operation.sdk_output_handle_ns,
            operation.sdk_output_follow_ns,
            operation.sdk_exec_unattributed_ns,
            operation
                .execution_exit_code
                .map_or_else(|| "-".to_owned(), |code| code.to_string()),
            operation.execution_stopped,
            operation.execution_stdout_bytes,
            operation.execution_stderr_bytes,
            operation.execution_elapsed_ns,
            operation.execution_total_wall_ns,
            operation.execution_spawn_ns,
            operation.execution_supervisor_queue_ns,
            operation.execution_runtime_ns,
            operation.execution_drain_ns,
            operation.execution_terminal_ns,
            operation.execution_unattributed_ns,
            operation.execution_direct_engine,
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
            if fields.len() != 25 {
                return Err(format!("invalid operation at state line {}", line_number + 1).into());
            }
            operations.push(Phases {
                id: fields[1].to_owned(),
                workspace_create_ns: fields[2].parse()?,
                shell_ns: fields[3].parse()?,
                sdk_exec_to_terminal_ns: fields[4].parse()?,
                sdk_exec_dispatch_ns: fields[5].parse()?,
                sdk_output_handle_ns: fields[6].parse()?,
                sdk_output_follow_ns: fields[7].parse()?,
                sdk_exec_unattributed_ns: fields[8].parse()?,
                execution_exit_code: (fields[9] != "-").then(|| fields[9].parse()).transpose()?,
                execution_stopped: fields[10].parse()?,
                execution_stdout_bytes: fields[11].parse()?,
                execution_stderr_bytes: fields[12].parse()?,
                execution_elapsed_ns: fields[13].parse()?,
                execution_total_wall_ns: fields[14].parse()?,
                execution_spawn_ns: fields[15].parse()?,
                execution_supervisor_queue_ns: fields[16].parse()?,
                execution_runtime_ns: fields[17].parse()?,
                execution_drain_ns: fields[18].parse()?,
                execution_terminal_ns: fields[19].parse()?,
                execution_unattributed_ns: fields[20].parse()?,
                execution_direct_engine: fields[21].parse()?,
                workspace_commit_api_ns: fields[22].parse()?,
                push_api_ns: fields[23].parse()?,
                workspace_end_ns: fields[24].parse()?,
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
        sum.saturating_add(operation.comparable_ns())
    });
    let comparable_total = state.operations.iter().fold(0_u64, |sum, operation| {
        sum.saturating_add(operation.comparable_ns())
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
        "{{\"schema\":\"fs-benchmark-pro-sample-v2\",\"candidate\":\"layerfs-reference\",\"workload\":{{\"initial_bytes\":{},\"initial_sha256\":{},\"edit_count\":{},\"edit_size_bytes\":10,\"prepend_bytes\":10}},\"operations\":[{}],\"aggregates\":{{\"create_ns\":{},\"sixteen_edits_sum_ns\":{},\"prepend_ns\":{},\"read_ns\":{},\"workspace_create_ns\":{},\"workspace_end_ns\":{},\"add_ns\":{},\"comparable_total_ns\":{},\"complete_turn_total_ns\":{}}},\"verification\":{{\"initial_bytes\":{},\"initial_sha256\":{},\"after_edits_sha256\":{},\"final_bytes\":{},\"final_sha256\":{},\"final_digest\":{},\"reopen_passed\":true,\"reopen_verify_ns\":{}}},\"storage\":{{\"initial\":{},\"authority_checkpoint\":{},\"final\":{},\"unavailable\":{{\"logical_bytes\":\"not exposed by the public Store snapshot\",\"semantic_payload_bytes\":\"available by fact/object kind in exact JSONL operation receipts; no single lossless aggregate\",\"wire_bytes\":\"local endpoint has no network transport\"}}}},\"provenance\":{{\"source_commit\":{},\"source_dirty\":{},\"authority_store_id\":{},\"branch_store_id\":{},\"branch_parent_store_id\":{},\"branch_id\":{},\"measured_unix_ns\":{}}},\"status\":\"pass\"}}",
        value(state, "file_bytes")?,
        json_string(value(state, "fixture_digest")?),
        value(state, "edit_count")?,
        operations,
        create.comparable_ns(),
        edits_sum,
        prepend.comparable_ns(),
        read.comparable_ns(),
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
        json_string(value(state, "branch_store_id")?),
        json_string(value(state, "branch_parent_store_id")?),
        json_string(value(state, "branch_id")?),
        now_ns(),
    ))
}

fn operation_summary_json(operation: &Phases) -> String {
    format!(
        "{{\"id\":{},\"workspace_create_ns\":{},\"shell_ns\":{},\"sdk_exec_to_terminal_ns\":{},\"sdk_exec_dispatch_ns\":{},\"sdk_output_handle_ns\":{},\"sdk_output_follow_ns\":{},\"sdk_exec_unattributed_ns\":{},\"execution\":{{\"exit_code\":{},\"stopped\":{},\"stdout_bytes\":{},\"stderr_bytes\":{},\"elapsed_ns\":{},\"total_wall_ns\":{},\"spawn_ns\":{},\"supervisor_queue_ns\":{},\"runtime_ns\":{},\"drain_ns\":{},\"terminal_publication_ns\":{},\"unattributed_ns\":{},\"direct_engine\":{}}},\"workspace_commit_api_ns\":{},\"push_api_ns\":{},\"workspace_end_ns\":{},\"authority_checkpoint_ns\":{},\"comparable_ns\":{},\"complete_turn_ns\":{}}}",
        json_string(&operation.id),
        operation.workspace_create_ns,
        operation.shell_ns,
        operation.sdk_exec_to_terminal_ns,
        operation.sdk_exec_dispatch_ns,
        operation.sdk_output_handle_ns,
        operation.sdk_output_follow_ns,
        operation.sdk_exec_unattributed_ns,
        operation.execution_exit_code.map_or_else(|| "null".to_owned(), |code| code.to_string()),
        operation.execution_stopped,
        operation.execution_stdout_bytes,
        operation.execution_stderr_bytes,
        operation.execution_elapsed_ns,
        operation.execution_total_wall_ns,
        operation.execution_spawn_ns,
        operation.execution_supervisor_queue_ns,
        operation.execution_runtime_ns,
        operation.execution_drain_ns,
        operation.execution_terminal_ns,
        operation.execution_unattributed_ns,
        operation.execution_direct_engine,
        operation.workspace_commit_api_ns,
        operation.push_api_ns,
        operation.workspace_end_ns,
        operation.authority_checkpoint_ns(),
        operation.comparable_ns(),
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
        "{{\"schema\":\"fs-benchmark-pro-evidence-v3\",\"candidate\":\"layerfs-reference\",\"record\":\"provenance\",\"file_bytes\":{file_bytes},\"edit_count\":{edit_count},\"fixture_sha256\":{},\"source_commit\":{},\"source_dirty\":{},\"unix_ns\":{}}}",
        json_string(fixture_digest),
        json_string(source_commit),
        *source_dirty,
        now_ns(),
    )];
    for operation in *operations {
        rows.push(format!(
            "{{\"schema\":\"fs-benchmark-pro-evidence-v3\",\"candidate\":\"layerfs-reference\",\"record\":\"operation\",\"id\":{},\"workspace_create_ns\":{},\"shell_ns\":{},\"sdk_exec_to_terminal_ns\":{},\"sdk_exec_dispatch_ns\":{},\"sdk_output_handle_ns\":{},\"sdk_output_follow_ns\":{},\"sdk_exec_unattributed_ns\":{},\"execution\":{{\"exit_code\":{},\"stopped\":{},\"stdout_bytes\":{},\"stderr_bytes\":{},\"elapsed_ns\":{},\"total_wall_ns\":{},\"spawn_ns\":{},\"supervisor_queue_ns\":{},\"runtime_ns\":{},\"drain_ns\":{},\"terminal_publication_ns\":{},\"unattributed_ns\":{},\"direct_engine\":{}}},\"workspace_commit_api_ns\":{},\"push_api_ns\":{},\"workspace_end_ns\":{},\"authority_checkpoint_ns\":{},\"complete_turn_ns\":{},\"storage_before\":{},\"storage_after\":{},\"receipts\":{{\"workspace_create\":{},\"workspace_exec\":{},\"workspace_output\":{},\"workspace_commit\":{},\"push\":{},\"workspace_end\":{}}}}}",
            json_string(&operation.id),
            operation.workspace_create_ns,
            operation.shell_ns,
            operation.sdk_exec_to_terminal_ns,
            operation.sdk_exec_dispatch_ns,
            operation.sdk_output_handle_ns,
            operation.sdk_output_follow_ns,
            operation.sdk_exec_unattributed_ns,
            operation.execution_exit_code.map_or_else(|| "null".to_owned(), |code| code.to_string()),
            operation.execution_stopped,
            operation.execution_stdout_bytes,
            operation.execution_stderr_bytes,
            operation.execution_elapsed_ns,
            operation.execution_total_wall_ns,
            operation.execution_spawn_ns,
            operation.execution_supervisor_queue_ns,
            operation.execution_runtime_ns,
            operation.execution_drain_ns,
            operation.execution_terminal_ns,
            operation.execution_unattributed_ns,
            operation.execution_direct_engine,
            operation.workspace_commit_api_ns,
            operation.push_api_ns,
            operation.workspace_end_ns,
            operation.authority_checkpoint_ns(),
            operation.complete_turn_ns(),
            optional_storage_json(operation.storage_before),
            optional_storage_json(operation.storage_after),
            optional_receipt_json(operation.workspace_create_receipt.as_deref(), "not applicable to this operation"),
            optional_receipt_json(operation.workspace_exec_receipt.as_deref(), "not applicable to this operation"),
            optional_receipt_json(operation.workspace_output_receipt.as_deref(), "not applicable to this operation"),
            optional_receipt_json(operation.workspace_commit_receipt.as_deref(), "read-only operation has no Workspace Commit"),
            optional_receipt_json(operation.push_receipt.as_deref(), "read-only operation has no Push"),
            optional_receipt_json(operation.workspace_end_receipt.as_deref(), "Workspace remains active"),
        ));
    }
    rows.push(format!(
        "{{\"schema\":\"fs-benchmark-pro-evidence-v3\",\"candidate\":\"layerfs-reference\",\"record\":\"add\",\"elapsed_ns\":{add_ns},\"excluded_from_comparable_total\":true,\"operation_receipt\":{}}}",
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

fn require_normal_commit_rebased(receipt: &OperationReceipt) -> AnyResult<()> {
    let commits = receipt
        .storage
        .iter()
        .filter_map(|receipt| match receipt {
            StorageReceipt::WorkspaceCommit(receipt) => Some(receipt),
            _ => None,
        })
        .collect::<Vec<_>>();
    if commits.len() != 1
        || commits[0].capture_mode != Some(layerfs_sdk::CaptureMode::Live)
        || commits[0].in_place_rebase_ns == 0
        || commits[0].resume_ns == 0
        || receipt
            .storage
            .iter()
            .any(|receipt| matches!(receipt, StorageReceipt::WorkspaceLifecycle(_)))
    {
        return Err("ordinary Workspace Commit did not prove in-place Rebased transition".into());
    }
    Ok(())
}

fn require_push(result: &PushResult) -> AnyResult<()> {
    match result {
        PushResult::Created { .. } | PushResult::Advanced { .. } => Ok(()),
        other => Err(format!("Push did not advance authority: {other:?}").into()),
    }
}

fn require_push_durability(
    receipt: &OperationReceipt,
    authority_store_id: StoreId,
    branch_store_id: StoreId,
) -> AnyResult<()> {
    let durability = receipt
        .storage
        .iter()
        .filter_map(|receipt| match receipt {
            StorageReceipt::Durability(receipt) => Some(*receipt),
            _ => None,
        })
        .collect::<Vec<_>>();
    if durability.len() != 2 {
        return Err(format!(
            "Push emitted {} durability receipts, expected two",
            durability.len()
        )
        .into());
    }
    for receipt in &durability {
        receipt.validate()?;
    }
    if !durability.iter().any(|receipt| {
        receipt.role == StoreRole::LayerStack && receipt.store_id == authority_store_id
    }) || !durability
        .iter()
        .any(|receipt| receipt.role == StoreRole::Branch && receipt.store_id == branch_store_id)
    {
        return Err("Push durability receipts do not match the bound Stores".into());
    }
    Ok(())
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

fn inventory_totals(store: &BranchStore) -> AnyResult<(u64, u64)> {
    let mut after = None;
    let mut ids = 0_u64;
    let mut bytes = 0_u64;
    loop {
        let page = store.inventory_page(after, 512)?;
        ids = ids.saturating_add(page.entries.len() as u64);
        bytes = page.entries.iter().fold(bytes, |total, entry| {
            total.saturating_add(entry.encoded_length)
        });
        let Some(next) = page.continuation else {
            return Ok((ids, bytes));
        };
        after = Some(next);
    }
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

fn workload_binary() -> PathBuf {
    std::env::var_os("LAYERFS_BENCH_WORKLOAD")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .expect("current executable")
                .parent()
                .expect("executable directory")
                .join("fs-benchmark-workload")
        })
}

fn host_digest(path: &Path) -> AnyResult<(u64, String)> {
    if !path.is_file() {
        return Err(format!("neutral fixture is missing: {}", path.display()).into());
    }
    let output = Command::new("sha256sum").arg(path).output()?;
    if !output.status.success() {
        return Err(format!(
            "fixture digest failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let digest = std::str::from_utf8(&output.stdout)?
        .split_whitespace()
        .next()
        .ok_or("sha256sum output")?;
    Ok((fs::metadata(path)?.len(), digest.to_owned()))
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
        assert_eq!(phases.comparable_ns(), 28);
    }

    #[test]
    fn diagnostics_cannot_run_before_headline_and_recovery() {
        let source = include_str!("main.rs");
        let measure = source
            .split_once("fn measure(")
            .unwrap()
            .1
            .split_once("fn diagnose_execution(")
            .unwrap()
            .0;
        assert!(!measure.contains("write_execution_diagnostic("));
        assert!(
            measure.find("let first_edit").unwrap()
                < measure.find("operations.insert(0, create)").unwrap()
        );
        assert!(
            measure.find("retain_mountinfo(").unwrap() < measure.find("let first_edit").unwrap()
        );

        let runner = include_str!("../run.sh");
        let recovery = runner
            .find("/usr/local/bin/fs-benchmark-pro verify")
            .unwrap();
        let diagnostics = runner
            .find("for diagnostic in true bash helper edit")
            .unwrap();
        assert!(diagnostics > recovery);
    }

    #[test]
    fn registered_edits_keep_bash_and_native_odrdwr_semantics() {
        let source = include_str!("main.rs");
        assert!(source
            .contains("\"/bin/bash\",\n                \"-lc\",\n                \"\\\"$@\\\"\""));
        let workload = include_str!("../workload.rs");
        assert!(workload.contains("OpenOptions::new().read(true).write(true).open(path)?"));
        let computer = include_str!("../computer.mjs");
        assert!(computer.contains("/bin/bash -lc ${shellQuote("));
        assert!(computer.contains("fs-bench-shell ${shellQuote(WORKLOAD)} edit"));
    }

    #[test]
    fn mountinfo_custody_uses_the_workspace_runtime_layout() {
        assert_eq!(
            workspace_mountinfo_path(Path::new("/evidence/branch.sqlite"), "w:123"),
            Path::new("/evidence/branch.sqlite.runtime/workspaces/workspaces/w:123/mountinfo.txt")
        );
    }
}
