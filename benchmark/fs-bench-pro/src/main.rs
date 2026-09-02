use layerfs_sdk::{
    BranchId, CandidateStats, Client, CommitId, ContainerId, CreateWorkspaceSession,
    EndWorkspaceMode, EntityName, ExecutionTransport, LayerStackInitialization, LayerStackStore,
    LocalForkSource, NonEmpty, OperationFamily, OutputPage, Query, QueryItem, QueryKind,
    WorkspaceCommitResult, WorkspaceId, WorkspacePlacement, WorkspaceProjection,
};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

type AnyResult<T> = Result<T, Box<dyn std::error::Error>>;

const MIB_32: u64 = 32 * 1024 * 1024;
const WORKSPACE_CREATE_HARD_NS: u64 = 20_000_000;
const SMALL_COMMIT_HARD_NS: u64 = 6_000_000;
const SMALL_COMPLETE_HARD_NS: u64 = 30_000_000;
const COLD_COMPLETE_HARD_NS: u64 = 150_000_000;
const EDIT16_HARD_NS: u64 = 200_000_000;
const PREPEND_HARD_NS: u64 = 250_000_000;
const READ_HARD_NS: u64 = 150_000_000;
const REGISTERED_TOTAL_HARD_NS: u64 = 700_000_000;
const INNER_WRITE_MIN_BYTES_PER_SECOND: f64 = 300.0 * 1024.0 * 1024.0;
const PAIRED_COLD_COMPLETE_HARD_NS: u64 = 200_000_000;
const PAIRED_EDIT16_HARD_NS: u64 = 250_000_000;
const PAIRED_PREPEND_HARD_NS: u64 = 350_000_000;
const PAIRED_READ_HARD_NS: u64 = 200_000_000;
const PAIRED_TOTAL_HARD_NS: u64 = 900_000_000;
const NAMESPACE_FILE_BYTES: u64 = 2_500;
const NAMESPACE_FILES_PER_DIRECTORY: u64 = 100;
const NAMESPACE_EDIT_MARKER: &[u8] = b"E000000001";

#[allow(dead_code)]
mod workload_source {
    include!("../workload.rs");

    pub(super) struct HarnessSha256(Sha256);

    impl HarnessSha256 {
        pub(super) fn new() -> Self {
            Self(Sha256::new())
        }

        pub(super) fn update(&mut self, bytes: &[u8]) {
            self.0.update(bytes);
        }

        pub(super) fn finish(self) -> String {
            hex(&self.0.finish())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NamespaceScenario {
    id: &'static str,
    regular_files: u64,
    data_directories: u64,
}

const NAMESPACE_SCENARIOS: [NamespaceScenario; 4] = [
    NamespaceScenario {
        id: "namespace-100",
        regular_files: 100,
        data_directories: 1,
    },
    NamespaceScenario {
        id: "namespace-1000",
        regular_files: 1_000,
        data_directories: 10,
    },
    NamespaceScenario {
        id: "namespace-10000",
        regular_files: 10_000,
        data_directories: 100,
    },
    NamespaceScenario {
        id: "namespace-100000",
        regular_files: 100_000,
        data_directories: 1_000,
    },
];

#[derive(Clone, Debug, Eq, PartialEq)]
struct NamespaceManifest {
    regular_files: u64,
    data_directories: u64,
    logical_bytes: u64,
    digest: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct NamespaceSample {
    layerstack_init_ns: u64,
    branch_fork_ns: u64,
    workspace_create_ns: u64,
    edit_ns: u64,
    commit_ns: u64,
    workspace_end_ns: u64,
    reopen_verify_ns: u64,
    complete_product_ns: u64,
}

impl NamespaceSample {
    fn validate(&self) -> AnyResult<()> {
        let phases = [
            self.layerstack_init_ns,
            self.branch_fork_ns,
            self.workspace_create_ns,
            self.edit_ns,
            self.commit_ns,
            self.workspace_end_ns,
            self.reopen_verify_ns,
        ]
        .into_iter()
        .try_fold(0_u64, u64::checked_add)
        .ok_or("namespace phase overflow")?;
        if phases != self.complete_product_ns || self.reopen_verify_ns == 0 {
            return Err("namespace phase equation".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
struct LifecycleSample {
    workspace_create_ns: u64,
    execution_ns: u64,
    commit_api_ns: u64,
    layerstack_visible_ns: u64,
    workspace_end_ns: u64,
    complete_lifecycle_ns: u64,
    inner_write_ns: Option<u64>,
}

struct LifecycleRun {
    sample: LifecycleSample,
    output: OutputPage,
    head: Option<CommitId>,
}

struct ProofCase {
    store: PathBuf,
    branch: BranchId,
    head: Option<CommitId>,
    placement: WorkspacePlacement,
    expected: ProofExpected,
}

#[derive(Clone, Copy)]
enum ProofExpected {
    Fixture,
    Prepend,
}

impl LifecycleSample {
    fn validate(&self) -> AnyResult<()> {
        if self.layerstack_visible_ns
            != self
                .workspace_create_ns
                .saturating_add(self.execution_ns)
                .saturating_add(self.commit_api_ns)
            || self.complete_lifecycle_ns
                != self
                    .layerstack_visible_ns
                    .saturating_add(self.workspace_end_ns)
        {
            return Err("lifecycle phase equation".into());
        }
        Ok(())
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("fs-benchmark-pro: {error}");
        std::process::exit(1);
    }
}

fn run() -> AnyResult<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [command] if command == "self-check" => self_check(),
        [command, fixture, scenario] if command == "namespace-fixture" => {
            let scenario = namespace_scenario(&scenario.to_string_lossy())?;
            let manifest = create_namespace_fixture(Path::new(fixture), scenario)?;
            emit_namespace_manifest(scenario, &manifest);
            Ok(())
        }
        [command, root, fixture, container, scenario, iteration] if command == "namespace" => {
            namespace_case(
                Path::new(root),
                Path::new(fixture),
                ContainerId(container.to_string_lossy().into_owned()),
                namespace_scenario(&scenario.to_string_lossy())?,
                iteration.to_string_lossy().parse()?,
            )
        }
        [command, root, fixture] if command == "run" => {
            campaign(Path::new(root), Path::new(fixture), None, 3)
        }
        [command, root, fixture, container] if command == "run" => campaign(
            Path::new(root),
            Path::new(fixture),
            Some(ContainerId(container.to_string_lossy().into_owned())),
            3,
        ),
        [command, root, fixture, container, iterations] if command == "run" => campaign(
            Path::new(root),
            Path::new(fixture),
            Some(ContainerId(container.to_string_lossy().into_owned())),
            iterations.to_string_lossy().parse()?,
        ),
        _ => Err("usage: fs-benchmark-pro self-check | namespace-fixture FIXTURE SCENARIO | namespace ROOT FIXTURE CONTAINER SCENARIO ITERATION | run ROOT FIXTURE [CONTAINER ITERATIONS]".into()),
    }
}

fn self_check() -> AnyResult<()> {
    LifecycleSample {
        workspace_create_ns: 1,
        execution_ns: 2,
        commit_api_ns: 3,
        layerstack_visible_ns: 6,
        workspace_end_ns: 4,
        complete_lifecycle_ns: 10,
        inner_write_ns: Some(1),
    }
    .validate()?;
    namespace_self_check()?;
    println!("PASS fs-bench-pro one-Store lifecycle equations");
    Ok(())
}

fn namespace_scenario(id: &str) -> AnyResult<NamespaceScenario> {
    NAMESPACE_SCENARIOS
        .into_iter()
        .find(|scenario| scenario.id == id)
        .ok_or_else(|| format!("unknown namespace scenario: {id}").into())
}

fn namespace_path(index: u64) -> String {
    format!("d{:04}/f{index:06}", index / NAMESPACE_FILES_PER_DIRECTORY)
}

fn namespace_content(path: &str) -> Vec<u8> {
    let mut seed = b"layerfs/fs-bench-pro/namespace-content/v1\0".to_vec();
    seed.extend_from_slice(path.as_bytes());
    seed.push(0);
    (0..NAMESPACE_FILE_BYTES as usize)
        .map(|index| seed[index % seed.len()] ^ ((index / seed.len()) as u8).wrapping_mul(0x9d))
        .collect()
}

fn namespace_edit_offset() -> usize {
    (2_654_435_761_u64 % (NAMESPACE_FILE_BYTES - NAMESPACE_EDIT_MARKER.len() as u64)) as usize
}

fn expected_namespace_manifest(scenario: NamespaceScenario, edited: bool) -> NamespaceManifest {
    let mut digest = workload_source::HarnessSha256::new();
    digest.update(b"layerfs/fs-bench-pro/namespace-tree/v1\0");
    for directory in 0..scenario.data_directories {
        let directory_path = format!("d{directory:04}");
        digest.update(b"D\0");
        digest.update(directory_path.as_bytes());
        digest.update(b"\0");
        let first = directory * NAMESPACE_FILES_PER_DIRECTORY;
        for index in first..first + NAMESPACE_FILES_PER_DIRECTORY {
            let path = namespace_path(index);
            let mut content = namespace_content(&path);
            if edited && index == 0 {
                let offset = namespace_edit_offset();
                content[offset..offset + NAMESPACE_EDIT_MARKER.len()]
                    .copy_from_slice(NAMESPACE_EDIT_MARKER);
            }
            digest.update(b"F\0");
            digest.update(path.as_bytes());
            digest.update(b"\0");
            digest.update(content.len().to_string().as_bytes());
            digest.update(b"\0");
            digest.update(&content);
            digest.update(b"\0");
        }
    }
    NamespaceManifest {
        regular_files: scenario.regular_files,
        data_directories: scenario.data_directories,
        logical_bytes: scenario.regular_files * NAMESPACE_FILE_BYTES,
        digest: digest.finish(),
    }
}

fn create_namespace_fixture(
    root: &Path,
    scenario: NamespaceScenario,
) -> AnyResult<NamespaceManifest> {
    std::fs::create_dir(root)?;
    let created = (|| -> AnyResult<NamespaceManifest> {
        for directory in 0..scenario.data_directories {
            std::fs::create_dir(root.join(format!("d{directory:04}")))?;
        }
        for index in 0..scenario.regular_files {
            let path = namespace_path(index);
            std::fs::write(root.join(&path), namespace_content(&path))?;
        }
        let manifest = inspect_namespace(root)?;
        validate_namespace_manifest(&manifest, scenario, false)?;
        Ok(manifest)
    })();
    if created.is_err() {
        let _ = std::fs::remove_dir_all(root);
    }
    created
}

fn inspect_namespace(root: &Path) -> AnyResult<NamespaceManifest> {
    if !root.is_dir() {
        return Err("namespace fixture root".into());
    }
    let mut pending = vec![root.to_owned()];
    let mut entries = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            let relative = path
                .strip_prefix(root)?
                .to_str()
                .ok_or("namespace path is not UTF-8")?
                .replace(std::path::MAIN_SEPARATOR, "/");
            if metadata.file_type().is_dir() {
                entries.push((relative, None, path.clone()));
                pending.push(path);
            } else if metadata.file_type().is_file() {
                entries.push((relative, Some(metadata.len()), path));
            } else {
                return Err("namespace fixture entry type".into());
            }
        }
    }
    entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let mut digest = workload_source::HarnessSha256::new();
    digest.update(b"layerfs/fs-bench-pro/namespace-tree/v1\0");
    let mut regular_files = 0_u64;
    let mut data_directories = 0_u64;
    let mut logical_bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    for (relative, size, path) in entries {
        match size {
            None => {
                data_directories = data_directories
                    .checked_add(1)
                    .ok_or("namespace directory count")?;
                digest.update(b"D\0");
                digest.update(relative.as_bytes());
                digest.update(b"\0");
            }
            Some(size) => {
                regular_files = regular_files.checked_add(1).ok_or("namespace file count")?;
                logical_bytes = logical_bytes
                    .checked_add(size)
                    .ok_or("namespace logical bytes")?;
                digest.update(b"F\0");
                digest.update(relative.as_bytes());
                digest.update(b"\0");
                digest.update(size.to_string().as_bytes());
                digest.update(b"\0");
                let mut file = std::fs::File::open(path)?;
                loop {
                    let read = file.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    digest.update(&buffer[..read]);
                }
                digest.update(b"\0");
            }
        }
    }
    Ok(NamespaceManifest {
        regular_files,
        data_directories,
        logical_bytes,
        digest: digest.finish(),
    })
}

fn validate_namespace_manifest(
    actual: &NamespaceManifest,
    scenario: NamespaceScenario,
    edited: bool,
) -> AnyResult<()> {
    let expected = expected_namespace_manifest(scenario, edited);
    validate_namespace_manifest_against(actual, &expected)
}

fn validate_namespace_manifest_against(
    actual: &NamespaceManifest,
    expected: &NamespaceManifest,
) -> AnyResult<()> {
    if actual != expected {
        return Err(format!(
            "namespace manifest mismatch: actual={actual:?} expected={expected:?}"
        )
        .into());
    }
    Ok(())
}

fn emit_namespace_manifest(scenario: NamespaceScenario, manifest: &NamespaceManifest) {
    println!(
        "{{\"schema\":\"fs-bench-pro-namespace-fixture-v1\",\"scenario\":\"{}\",\"regular_files\":{},\"data_directories\":{},\"logical_bytes\":{},\"fixture_digest\":\"{}\"}}",
        scenario.id,
        manifest.regular_files,
        manifest.data_directories,
        manifest.logical_bytes,
        manifest.digest,
    );
}

fn apply_namespace_edit(path: &Path) -> AnyResult<()> {
    let mut bytes = std::fs::read(path)?;
    if bytes.len() != NAMESPACE_FILE_BYTES as usize {
        return Err("namespace edit target length".into());
    }
    let offset = namespace_edit_offset();
    bytes[offset..offset + NAMESPACE_EDIT_MARKER.len()].copy_from_slice(NAMESPACE_EDIT_MARKER);
    std::fs::write(path, bytes)?;
    Ok(())
}

fn namespace_self_check() -> AnyResult<()> {
    let scenario = namespace_scenario("namespace-100")?;
    let root = std::env::temp_dir().join(format!(
        "fs-benchmark-pro-namespace-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let first = root.join("first");
    let second = root.join("second");
    std::fs::create_dir(&root)?;
    let checked = (|| -> AnyResult<()> {
        if namespace_edit_offset() != 1_141 {
            return Err("namespace edit oracle offset".into());
        }
        let first_manifest = create_namespace_fixture(&first, scenario)?;
        let second_manifest = create_namespace_fixture(&second, scenario)?;
        if first_manifest != second_manifest {
            return Err("namespace fixture is not deterministic".into());
        }
        apply_namespace_edit(&first.join(namespace_path(0)))?;
        let edited = inspect_namespace(&first)?;
        validate_namespace_manifest(&edited, scenario, true)?;
        let parsed = parse_namespace_verification_text(&format!(
            "regular_files={}\ndata_directories={}\nlogical_bytes={}\nnamespace_digest={}\n",
            edited.regular_files, edited.data_directories, edited.logical_bytes, edited.digest
        ))?;
        if parsed != edited {
            return Err("namespace reopen verification parser".into());
        }
        std::fs::write(first.join("extra"), b"extra")?;
        if validate_namespace_manifest(&inspect_namespace(&first)?, scenario, true).is_ok() {
            return Err("namespace extra path was accepted".into());
        }
        std::fs::remove_file(second.join(namespace_path(0)))?;
        if validate_namespace_manifest(&inspect_namespace(&second)?, scenario, false).is_ok() {
            return Err("namespace missing path was accepted".into());
        }
        NamespaceSample {
            layerstack_init_ns: 1,
            branch_fork_ns: 2,
            workspace_create_ns: 3,
            edit_ns: 4,
            commit_ns: 5,
            workspace_end_ns: 6,
            reopen_verify_ns: 7,
            complete_product_ns: 28,
        }
        .validate()?;
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(root);
    checked
}

fn namespace_placement(
    container_id: &ContainerId,
    scenario: NamespaceScenario,
    iteration: usize,
    phase: &str,
) -> WorkspacePlacement {
    WorkspacePlacement::Container {
        container_id: container_id.clone(),
        root: PathBuf::from(format!(
            "/workspace/layerfs-{}-{iteration}-{phase}-{}",
            scenario.id,
            std::process::id()
        )),
    }
}

fn operation_candidate(
    snapshot: &layerfs_sdk::MonitorSnapshot,
    family: OperationFamily,
) -> AnyResult<CandidateStats> {
    let mut candidates = snapshot.operations.iter().filter_map(|operation| {
        (operation.operation.family == family)
            .then_some(operation.candidate)
            .flatten()
    });
    let candidate = candidates.next().ok_or("namespace candidate receipt")?;
    if candidates.next().is_some() || !candidate.validate_for(family) {
        return Err("namespace candidate receipt cardinality or equation".into());
    }
    Ok(candidate)
}

fn sum_metric(left: u64, right: u64, name: &'static str) -> AnyResult<u64> {
    left.checked_add(right).ok_or_else(|| name.into())
}

fn parse_namespace_verification(output: &OutputPage) -> AnyResult<NamespaceManifest> {
    let bytes = output
        .chunks
        .iter()
        .flat_map(|chunk| chunk.bytes.iter().copied())
        .collect::<Vec<_>>();
    parse_namespace_verification_text(std::str::from_utf8(&bytes)?)
}

fn parse_namespace_verification_text(output: &str) -> AnyResult<NamespaceManifest> {
    let mut fields = std::collections::BTreeMap::new();
    for line in output.lines() {
        let (name, value) = line
            .split_once('=')
            .ok_or("namespace verification output")?;
        if fields.insert(name, value).is_some() {
            return Err("duplicate namespace verification field".into());
        }
    }
    if fields.len() != 4 {
        return Err("namespace verification field count".into());
    }
    let digest = fields
        .remove("namespace_digest")
        .ok_or("namespace verification digest")?;
    if digest.len() != 64
        || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        || digest.bytes().any(|byte| byte.is_ascii_uppercase())
    {
        return Err("namespace verification digest encoding".into());
    }
    let manifest = NamespaceManifest {
        regular_files: fields
            .remove("regular_files")
            .ok_or("namespace verification files")?
            .parse()?,
        data_directories: fields
            .remove("data_directories")
            .ok_or("namespace verification directories")?
            .parse()?,
        logical_bytes: fields
            .remove("logical_bytes")
            .ok_or("namespace verification bytes")?
            .parse()?,
        digest: digest.to_owned(),
    };
    if !fields.is_empty() {
        return Err("extra namespace verification field".into());
    }
    Ok(manifest)
}

fn namespace_case(
    root: &Path,
    fixture: &Path,
    container_id: ContainerId,
    scenario: NamespaceScenario,
    iteration: usize,
) -> AnyResult<()> {
    if iteration == 0 {
        return Err("namespace iteration must be positive".into());
    }
    let setup_started = Instant::now();
    let fixture_manifest = inspect_namespace(fixture)?;
    validate_namespace_manifest(&fixture_manifest, scenario, false)?;
    let expected_edited_manifest = expected_namespace_manifest(scenario, true);
    std::fs::create_dir(root)?;
    let store_path = root.join("store.sqlite");
    let store = Arc::new(LayerStackStore::create(&store_path)?);
    let client = Client::connect(store.clone())?;
    let setup_ns = elapsed_ns(setup_started);

    let stale = store.take_layerstack_initialization_receipts();
    if !stale.is_empty() {
        return Err("stale LayerStack initialization receipt".into());
    }
    let t0 = Instant::now();
    let initialized = client.initialize_layerstack(
        EntityName::new(format!("{}-{iteration}", scenario.id))?,
        LayerStackInitialization::Directory(fixture.to_owned()),
    )?;
    let receipts = store.take_layerstack_initialization_receipts();
    let [scan] = receipts.as_slice() else {
        return Err("LayerStack initialization receipt cardinality".into());
    };
    if scan.layer_stack_id != initialized.layer_stack_id
        || scan.scanned_files != fixture_manifest.regular_files
        || scan.scanned_bytes != fixture_manifest.logical_bytes
    {
        return Err(format!("LayerStack initialization scan receipt mismatch: {scan:?}").into());
    }
    let t1 = Instant::now();

    let branch = client.fork_branch(
        EntityName::new("main")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;
    let t2 = Instant::now();

    let workspace = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch,
        placement: namespace_placement(&container_id, scenario, iteration, "edit"),
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let t3 = Instant::now();

    let workload = std::env::var_os("LAYERFS_BENCH_WORKLOAD")
        .unwrap_or_else(|| OsString::from("fs-benchmark-workload"));
    execute_workload(
        &client,
        workspace.id,
        vec![
            workload.clone(),
            OsString::from("edit"),
            OsString::from(namespace_path(0)),
            OsString::from("0"),
            OsString::from(NAMESPACE_FILE_BYTES.to_string()),
        ],
    )?;
    let t4 = Instant::now();

    let commit = client.commit_workspace_session(workspace.id);
    let t5 = Instant::now();
    let commit_failure = |error: String| -> AnyResult<()> {
        let ended = client.end_workspace_session(workspace.id, EndWorkspaceMode::Discard);
        let t6 = Instant::now();
        println!(
            "{{\"schema\":\"fs-bench-pro-namespace-failure-v1\",\"scenario\":\"{}\",\"iteration\":{iteration},\"failed_phase\":\"commit\",\"error\":{:?},\"layerstack_init_ns\":{},\"branch_fork_ns\":{},\"workspace_create_ns\":{},\"edit_ns\":{},\"commit_ns\":{},\"workspace_end_ns\":{},\"regular_files\":{},\"data_directories\":{},\"logical_bytes\":{},\"fixture_digest\":\"{}\",\"scanned_files\":{},\"scanned_bytes\":{}}}",
            scenario.id,
            error,
            nanos(t0, t1),
            nanos(t1, t2),
            nanos(t2, t3),
            nanos(t3, t4),
            nanos(t4, t5),
            nanos(t5, t6),
            fixture_manifest.regular_files,
            fixture_manifest.data_directories,
            fixture_manifest.logical_bytes,
            fixture_manifest.digest,
            scan.scanned_files,
            scan.scanned_bytes,
        );
        eprintln!(
            "NAMESPACE_DIAGNOSTIC operations={:?}",
            client.monitor_snapshot()
        );
        ended?;
        Err(error.into())
    };
    let head = match commit {
        Ok(WorkspaceCommitResult::Created { commit_id, .. }) => Some(commit_id),
        Ok(result) => {
            return commit_failure(format!(
                "namespace Commit did not create a Commit: {result:?}"
            ));
        }
        Err(error) => {
            return commit_failure(format!("namespace Commit failed: {error}"));
        }
    };

    client.end_workspace_session(workspace.id, EndWorkspaceMode::Clean)?;
    let t6 = Instant::now();
    let snapshot = client.monitor_snapshot()?;
    let initialize_candidate =
        operation_candidate(&snapshot, OperationFamily::LayerStackInitialize)?;
    let commit_candidate = operation_candidate(&snapshot, OperationFamily::WorkspaceCommit)?;
    drop(client);
    drop(store);

    let reopened_store = Arc::new(LayerStackStore::connect(&store_path)?);
    let reopened = Client::connect(reopened_store)?;
    visible_head(&reopened, branch, head)?;
    let verification = reopened
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: branch,
            placement: namespace_placement(&container_id, scenario, iteration, "verify"),
            projection: Some(WorkspaceProjection::Fuse),
        })
        .map_err(|error| format!("namespace reopen Workspace create failed: {error}"))?;
    let output = execute(
        &reopened,
        verification.id,
        vec![
            workload,
            OsString::from("namespace-verify"),
            OsString::from("."),
        ],
    )
    .map_err(|error| format!("namespace reopen verification execution failed: {error}"))?;
    let verified = parse_namespace_verification(&output)?;
    validate_namespace_manifest_against(&verified, &expected_edited_manifest)?;
    reopened
        .end_workspace_session(verification.id, EndWorkspaceMode::Clean)
        .map_err(|error| format!("namespace reopen Workspace End failed: {error}"))?;
    visible_head(&reopened, branch, head)?;
    if reopened.active_workspace_count()? != 0 || reopened.active_execution_count()? != 0 {
        return Err("namespace reopen verification leaked runtime state".into());
    }
    let t7 = Instant::now();
    eprintln!("NAMESPACE_DIAGNOSTIC scan={scan:?}");
    eprintln!("NAMESPACE_DIAGNOSTIC operations={snapshot:?}");

    let sample = NamespaceSample {
        layerstack_init_ns: nanos(t0, t1),
        branch_fork_ns: nanos(t1, t2),
        workspace_create_ns: nanos(t2, t3),
        edit_ns: nanos(t3, t4),
        commit_ns: nanos(t4, t5),
        workspace_end_ns: nanos(t5, t6),
        reopen_verify_ns: nanos(t6, t7),
        complete_product_ns: nanos(t0, t7),
    };
    sample.validate()?;

    let candidate_objects = sum_metric(
        initialize_candidate.candidate_objects,
        commit_candidate.candidate_objects,
        "namespace candidate object overflow",
    )?;
    let candidate_bytes = sum_metric(
        initialize_candidate.candidate_bytes,
        commit_candidate.candidate_bytes,
        "namespace candidate byte overflow",
    )?;
    let inserted_objects = sum_metric(
        initialize_candidate.inserted_objects,
        commit_candidate.inserted_objects,
        "namespace inserted object overflow",
    )?;
    let inserted_bytes = sum_metric(
        initialize_candidate.inserted_bytes,
        commit_candidate.inserted_bytes,
        "namespace inserted byte overflow",
    )?;
    let reused_objects = sum_metric(
        initialize_candidate.reused_objects,
        commit_candidate.reused_objects,
        "namespace reused object overflow",
    )?;
    let reused_bytes = sum_metric(
        initialize_candidate.reused_bytes,
        commit_candidate.reused_bytes,
        "namespace reused byte overflow",
    )?;
    if candidate_objects
        != sum_metric(
            inserted_objects,
            reused_objects,
            "namespace combined object equation overflow",
        )?
        || candidate_bytes
            != sum_metric(
                inserted_bytes,
                reused_bytes,
                "namespace combined byte equation overflow",
            )?
    {
        return Err("namespace combined candidate equation".into());
    }

    println!(
        "{{\"schema\":\"fs-bench-pro-namespace-v1\",\"scenario\":\"{}\",\"iteration\":{iteration},\"setup_ns\":{setup_ns},\"layerstack_init_ns\":{},\"branch_fork_ns\":{},\"workspace_create_ns\":{},\"edit_ns\":{},\"commit_ns\":{},\"workspace_end_ns\":{},\"reopen_verify_ns\":{},\"complete_product_ns\":{},\"regular_files\":{},\"data_directories\":{},\"logical_bytes\":{},\"fixture_digest\":\"{}\",\"verified_digest\":\"{}\",\"scanned_files\":{},\"scanned_bytes\":{},\"candidate_objects\":{candidate_objects},\"candidate_bytes\":{candidate_bytes},\"inserted_objects\":{inserted_objects},\"inserted_bytes\":{inserted_bytes},\"reused_objects\":{reused_objects},\"reused_bytes\":{reused_bytes},\"max_transaction_objects\":{},\"max_transaction_bytes\":{}}}",
        scenario.id,
        sample.layerstack_init_ns,
        sample.branch_fork_ns,
        sample.workspace_create_ns,
        sample.edit_ns,
        sample.commit_ns,
        sample.workspace_end_ns,
        sample.reopen_verify_ns,
        sample.complete_product_ns,
        fixture_manifest.regular_files,
        fixture_manifest.data_directories,
        fixture_manifest.logical_bytes,
        fixture_manifest.digest,
        verified.digest,
        scan.scanned_files,
        scan.scanned_bytes,
        initialize_candidate
            .max_transaction_objects
            .max(commit_candidate.max_transaction_objects),
        initialize_candidate
            .max_transaction_bytes
            .max(commit_candidate.max_transaction_bytes),
    );
    Ok(())
}

fn campaign(
    root: &Path,
    fixture: &Path,
    container: Option<ContainerId>,
    iterations: usize,
) -> AnyResult<()> {
    if iterations == 0 || !fixture.is_file() || std::fs::metadata(fixture)?.len() != MIB_32 {
        return Err("campaign arguments".into());
    }
    std::fs::create_dir_all(root)?;
    let workload = std::env::var_os("LAYERFS_BENCH_WORKLOAD")
        .unwrap_or_else(|| OsString::from("fs-benchmark-workload"));
    let oracle_workload =
        std::env::var_os("LAYERFS_BENCH_ORACLE_WORKLOAD").unwrap_or_else(|| workload.clone());
    let fixture_exec =
        std::env::var_os("LAYERFS_BENCH_FIXTURE").unwrap_or_else(|| fixture.as_os_str().to_owned());
    let mut cold = Vec::new();
    let mut small = Vec::new();
    let mut edit16 = Vec::new();
    let mut prepend = Vec::new();
    let mut read = Vec::new();
    let mut proofs = Vec::new();

    for iteration in 0..iterations {
        let iteration_root = root.join(format!("iteration-{iteration:03}"));
        std::fs::create_dir_all(&iteration_root)?;
        let seed = iteration_root.join("seed");
        std::fs::create_dir(&seed)?;
        std::fs::copy(fixture, seed.join("payload.bin"))?;

        let cold_root = iteration_root.join("cold");
        let (cold_client, cold_branch) = case_client(
            &cold_root,
            &format!("cold-{iteration}"),
            LayerStackInitialization::Empty,
        )?;

        let cold_run = lifecycle(
            &cold_client,
            cold_branch,
            case_placement(&container, &cold_root, iteration, "cold"),
            vec![
                workload.clone(),
                OsString::from("create"),
                fixture_exec.clone(),
                OsString::from("payload.bin"),
            ],
        )?;
        emit_sample("cold-create-32m", iteration, &cold_run.sample);
        emit_execution_receipt("cold-create-32m", iteration, &cold_run.output);
        emit_diagnostics(&cold_client, "cold-create-32m", iteration)?;
        proofs.push(ProofCase {
            store: cold_root.join("store.sqlite"),
            branch: cold_branch,
            head: cold_run.head,
            placement: case_placement(&container, &cold_root, iteration, "cold-proof"),
            expected: ProofExpected::Fixture,
        });
        cold.push(cold_run.sample);
        drop(cold_client);
        reopen_visible(&cold_root.join("store.sqlite"), cold_branch, cold_run.head)?;
        emit_store_census("cold-create-32m", iteration, &cold_root)?;

        let small_root = iteration_root.join("small");
        let (small_client, small_branch) = case_client(
            &small_root,
            &format!("small-{iteration}"),
            LayerStackInitialization::Directory(seed.clone()),
        )?;

        let small_run = lifecycle(
            &small_client,
            small_branch,
            case_placement(&container, &small_root, iteration, "small"),
            vec![
                workload.clone(),
                OsString::from("edit"),
                OsString::from("payload.bin"),
                OsString::from("0"),
                OsString::from(MIB_32.to_string()),
            ],
        )?;
        emit_sample("small-edit", iteration, &small_run.sample);
        emit_execution_receipt("small-edit", iteration, &small_run.output);
        emit_diagnostics(&small_client, "small-edit", iteration)?;
        small.push(small_run.sample);
        drop(small_client);
        reopen_visible(
            &small_root.join("store.sqlite"),
            small_branch,
            small_run.head,
        )?;
        emit_store_census("small-edit", iteration, &small_root)?;

        let edit_root = iteration_root.join("edit16");
        let (edit_client, edit_branch) = case_client(
            &edit_root,
            &format!("edit-{iteration}"),
            LayerStackInitialization::Directory(seed.clone()),
        )?;

        let edit_started = Instant::now();
        let edit_workspace = edit_client.create_workspace_session(CreateWorkspaceSession {
            branch_id: edit_branch,
            placement: case_placement(&container, &edit_root, iteration, "edit16"),
            projection: Some(WorkspaceProjection::Fuse),
        })?;
        let mut edit_receipts = Vec::with_capacity(16);
        let mut edit_head = None;
        for edit in 0..16 {
            let output = execute_workload(
                &edit_client,
                edit_workspace.id,
                vec![
                    workload.clone(),
                    OsString::from("edit"),
                    OsString::from("payload.bin"),
                    OsString::from((edit + 1).to_string()),
                    OsString::from(MIB_32.to_string()),
                ],
            )?;
            edit_receipts.push(output.receipt.ok_or("EDIT16 execution receipt")?);
            let result = edit_client.commit_workspace_session(edit_workspace.id)?;
            match result {
                WorkspaceCommitResult::Created { commit_id, .. } => {
                    edit_head = Some(commit_id);
                }
                result => {
                    emit_diagnostics(&edit_client, "edit16-failed", iteration)?;
                    return Err(format!(
                        "EDIT16 edit {} did not create a Commit: {result:?}",
                        edit + 1
                    )
                    .into());
                }
            }
        }
        edit_client.end_workspace_session(edit_workspace.id, EndWorkspaceMode::Clean)?;
        let edit_ns = elapsed_ns(edit_started);
        let snapshot = edit_client.monitor_snapshot()?;
        let commit_receipts = snapshot
            .operations
            .iter()
            .filter(|receipt| {
                receipt.operation.family == OperationFamily::WorkspaceCommit
                    && receipt.operation.workspace_id == Some(edit_workspace.id)
            })
            .collect::<Vec<_>>();
        if commit_receipts.len() != 16 {
            return Err("EDIT16 Monitor receipt cardinality".into());
        }
        println!(
            "{{\"schema\":\"fs-bench-pro-v4\",\"case\":\"edit16\",\"iteration\":{iteration},\"complete_lifecycle_ns\":{edit_ns}}}"
        );
        for receipt in edit_receipts {
            println!("DIAGNOSTIC case=edit16 execution={receipt:?}");
        }
        for receipt in commit_receipts {
            println!("DIAGNOSTIC case=edit16 commit={receipt:?}");
        }
        edit16.push(edit_ns);
        drop(edit_client);
        reopen_visible(&edit_root.join("store.sqlite"), edit_branch, edit_head)?;
        emit_store_census("edit16", iteration, &edit_root)?;

        let prepend_root = iteration_root.join("prepend");
        let (prepend_client, prepend_branch) = case_client(
            &prepend_root,
            &format!("prepend-{iteration}"),
            LayerStackInitialization::Directory(seed.clone()),
        )?;
        let prepend_run = lifecycle(
            &prepend_client,
            prepend_branch,
            case_placement(&container, &prepend_root, iteration, "prepend"),
            vec![
                workload.clone(),
                OsString::from("prepend"),
                OsString::from("payload.bin"),
            ],
        )?;
        emit_sample("prepend-temp-copy-rename", iteration, &prepend_run.sample);
        emit_execution_receipt("prepend-temp-copy-rename", iteration, &prepend_run.output);
        emit_diagnostics(&prepend_client, "prepend-temp-copy-rename", iteration)?;
        proofs.push(ProofCase {
            store: prepend_root.join("store.sqlite"),
            branch: prepend_branch,
            head: prepend_run.head,
            placement: case_placement(&container, &prepend_root, iteration, "prepend-proof"),
            expected: ProofExpected::Prepend,
        });
        prepend.push(prepend_run.sample);
        drop(prepend_client);
        reopen_visible(
            &prepend_root.join("store.sqlite"),
            prepend_branch,
            prepend_run.head,
        )?;
        emit_store_census("prepend-temp-copy-rename", iteration, &prepend_root)?;

        let read_root = iteration_root.join("read");
        let (read_client, read_branch) = case_client(
            &read_root,
            &format!("read-{iteration}"),
            LayerStackInitialization::Directory(seed),
        )?;
        let read_run = lifecycle(
            &read_client,
            read_branch,
            case_placement(&container, &read_root, iteration, "read"),
            vec![
                workload.clone(),
                OsString::from("read"),
                OsString::from("payload.bin"),
            ],
        )?;
        if parse_read_bytes(&read_run.output)? != MIB_32 {
            return Err("read output size".into());
        }
        emit_sample("read-32m", iteration, &read_run.sample);
        emit_execution_receipt("read-32m", iteration, &read_run.output);
        emit_diagnostics(&read_client, "read-32m", iteration)?;
        proofs.push(ProofCase {
            store: read_root.join("store.sqlite"),
            branch: read_branch,
            head: read_run.head,
            placement: case_placement(&container, &read_root, iteration, "read-proof"),
            expected: ProofExpected::Fixture,
        });
        read.push(read_run.sample);
        drop(read_client);
        reopen_visible(&read_root.join("store.sqlite"), read_branch, read_run.head)?;
        emit_store_census("read-32m", iteration, &read_root)?;
    }

    let (fixture_size, fixture_digest) = process_digest(&oracle_workload, fixture.as_os_str())?;
    if fixture_size != MIB_32 {
        return Err("fixture digest size".into());
    }
    let prepend_oracle = root.join("prepend-oracle.bin");
    build_prepend_oracle(fixture, &prepend_oracle)?;
    let (prepend_size, prepend_digest) =
        process_digest(&oracle_workload, prepend_oracle.as_os_str())?;
    if prepend_size != MIB_32 + 10 {
        return Err("prepend digest size".into());
    }
    for proof in proofs {
        let (size, digest) = match proof.expected {
            ProofExpected::Fixture => (fixture_size, fixture_digest.as_str()),
            ProofExpected::Prepend => (prepend_size, prepend_digest.as_str()),
        };
        prove_case(&workload, proof, size, digest)?;
    }
    std::fs::remove_file(prepend_oracle)?;

    let cold_commit = median(cold.iter().map(|sample| sample.commit_api_ns).collect());
    let cold_complete = median(
        cold.iter()
            .map(|sample| sample.complete_lifecycle_ns)
            .collect(),
    );
    let create = median(
        cold.iter()
            .chain(&small)
            .chain(&prepend)
            .chain(&read)
            .map(|sample| sample.workspace_create_ns)
            .collect(),
    );
    let small_commit = median(small.iter().map(|sample| sample.commit_api_ns).collect());
    let small_complete = median(
        small
            .iter()
            .map(|sample| sample.complete_lifecycle_ns)
            .collect(),
    );
    let edit16 = median(edit16);
    let prepend_complete = median(
        prepend
            .iter()
            .map(|sample| sample.complete_lifecycle_ns)
            .collect(),
    );
    let read_complete = median(
        read.iter()
            .map(|sample| sample.complete_lifecycle_ns)
            .collect(),
    );
    let registered_total = cold_complete
        .saturating_add(edit16)
        .saturating_add(prepend_complete)
        .saturating_add(read_complete);
    let inner_write_ns = median(
        cold.iter()
            .filter_map(|sample| sample.inner_write_ns)
            .collect(),
    );
    let throughput = if inner_write_ns == 0 {
        0.0
    } else {
        MIB_32 as f64 * 1_000_000_000.0 / inner_write_ns as f64
    };
    let process_peak_rss_bytes = linux_process_peak_rss_bytes();
    let cgroup_peak_bytes = read_u64("/sys/fs/cgroup/memory.peak");
    let cgroup_swap_bytes = read_u64("/sys/fs/cgroup/memory.swap.current");
    let paired_shell = std::env::var("LAYERFS_BENCH_SHELL").as_deref() == Ok("1");
    let execution_profile = if paired_shell {
        "fresh-sh-c"
    } else {
        "fresh-direct-argv"
    };
    println!(
        "{{\"schema\":\"fs-bench-pro-v4-summary\",\"execution_profile\":\"{execution_profile}\",\"acknowledgement_profile\":\"memory-off-live-process\",\"workspace_create_ns\":{create},\"small_commit_ns\":{small_commit},\"small_complete_ns\":{small_complete},\"cold_commit_ns\":{cold_commit},\"cold_complete_ns\":{cold_complete},\"edit16_ns\":{edit16},\"prepend_complete_ns\":{prepend_complete},\"read_complete_ns\":{read_complete},\"registered_total_ns\":{registered_total},\"inner_write_bytes_per_second\":{throughput:.3},\"process_peak_rss_bytes\":{process_peak_rss_bytes},\"cgroup_peak_bytes\":{cgroup_peak_bytes},\"cgroup_swap_bytes\":{cgroup_swap_bytes}}}"
    );
    let failed = if paired_shell {
        cold_complete > PAIRED_COLD_COMPLETE_HARD_NS
            || edit16 > PAIRED_EDIT16_HARD_NS
            || prepend_complete > PAIRED_PREPEND_HARD_NS
            || read_complete > PAIRED_READ_HARD_NS
            || registered_total > PAIRED_TOTAL_HARD_NS
            || throughput < INNER_WRITE_MIN_BYTES_PER_SECOND
    } else {
        create > WORKSPACE_CREATE_HARD_NS
            || small_commit > SMALL_COMMIT_HARD_NS
            || small_complete > SMALL_COMPLETE_HARD_NS
            || cold_complete > COLD_COMPLETE_HARD_NS
            || edit16 > EDIT16_HARD_NS
            || prepend_complete > PREPEND_HARD_NS
            || read_complete > READ_HARD_NS
            || registered_total > REGISTERED_TOTAL_HARD_NS
            || throughput < INNER_WRITE_MIN_BYTES_PER_SECOND
    };
    if failed {
        return Err("one or more hard performance gates failed".into());
    }
    Ok(())
}

fn case_client(
    root: &Path,
    name: &str,
    source: LayerStackInitialization,
) -> AnyResult<(Client, BranchId)> {
    std::fs::create_dir(root)?;
    let path = root.join("store.sqlite");
    let store = Arc::new(LayerStackStore::create(&path)?);
    let client = Client::connect(store.clone())?;
    let initialized = client.initialize_layerstack(EntityName::new(name)?, source)?;
    let branch = client.fork_branch(
        EntityName::new("main")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;
    drop(client);
    drop(store);
    let store = Arc::new(LayerStackStore::connect(path)?);
    Ok((Client::connect(store)?, branch))
}

fn case_placement(
    container: &Option<ContainerId>,
    root: &Path,
    iteration: usize,
    case: &str,
) -> WorkspacePlacement {
    match container {
        Some(container_id) => WorkspacePlacement::Container {
            container_id: container_id.clone(),
            root: PathBuf::from(format!("/workspace/layerfs-bench-{iteration}-{case}")),
        },
        None => WorkspacePlacement::Host {
            root: root.join("mount"),
        },
    }
}

fn reopen_visible(path: &Path, branch: BranchId, expected: Option<CommitId>) -> AnyResult<()> {
    let store = Arc::new(LayerStackStore::connect(path)?);
    let client = Client::connect(store)?;
    visible_head(&client, branch, expected)
}

fn lifecycle(
    client: &Client,
    branch_id: BranchId,
    placement: WorkspacePlacement,
    argv: Vec<OsString>,
) -> AnyResult<LifecycleRun> {
    let t0 = Instant::now();
    let session = client.create_workspace_session(CreateWorkspaceSession {
        branch_id,
        placement,
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let t1 = Instant::now();
    let output = execute_workload(client, session.id, argv)?;
    let t2 = Instant::now();
    let commit_id = match client.commit_workspace_session(session.id)? {
        WorkspaceCommitResult::Created { commit_id, .. } => Some(commit_id),
        WorkspaceCommitResult::UpToDate { head } => head,
        result => return Err(format!("Commit failed: {result:?}").into()),
    };
    let t3 = Instant::now();
    client.end_workspace_session(session.id, EndWorkspaceMode::Clean)?;
    let t4 = Instant::now();
    visible_head(client, branch_id, commit_id)?;
    let sample = LifecycleSample {
        workspace_create_ns: nanos(t0, t1),
        execution_ns: nanos(t1, t2),
        commit_api_ns: nanos(t2, t3),
        layerstack_visible_ns: nanos(t0, t3),
        workspace_end_ns: nanos(t3, t4),
        complete_lifecycle_ns: nanos(t0, t4),
        inner_write_ns: parse_inner_write_ns(&output),
    };
    sample.validate()?;
    Ok(LifecycleRun {
        sample,
        output,
        head: commit_id,
    })
}

fn execute(
    client: &Client,
    workspace_id: WorkspaceId,
    argv: Vec<OsString>,
) -> AnyResult<layerfs_sdk::OutputPage> {
    let execution = client.exec_workspace_session(workspace_id, NonEmpty::new(argv)?)?;
    let reader = client.workspace_output(execution.id)?;
    let mut output = reader.read(0, true)?;
    while !output.exited {
        let next = reader.read(output.next_sequence, true)?;
        output.chunks.extend(next.chunks);
        output.next_sequence = next.next_sequence;
        output.truncated |= next.truncated;
        output.exited = next.exited;
        output.receipt = next.receipt;
    }
    if !output.exited
        || output.truncated
        || output
            .receipt
            .as_ref()
            .and_then(|receipt| receipt.exit_code)
            != Some(0)
    {
        return Err(format!("fresh-process execution failed: {output:?}").into());
    }
    if std::env::var("LAYERFS_EXEC_TRANSPORT").as_deref() == Ok("daemon") {
        let receipt = output.receipt.as_ref().ok_or("daemon execution receipt")?;
        if receipt.transport != ExecutionTransport::Daemon
            || receipt.daemon_timing.is_none()
            || receipt.docker_engine_calls != 0
            || !receipt.timing_balanced()
        {
            return Err(format!("invalid daemon execution receipt: {receipt:?}").into());
        }
    }
    Ok(output)
}

fn execute_workload(
    client: &Client,
    workspace_id: WorkspaceId,
    argv: Vec<OsString>,
) -> AnyResult<layerfs_sdk::OutputPage> {
    if std::env::var("LAYERFS_BENCH_SHELL").as_deref() != Ok("1") {
        return execute(client, workspace_id, argv);
    }
    let command = argv
        .iter()
        .map(|value| format!("'{}'", value.to_string_lossy().replace('\'', "'\"'\"'")))
        .collect::<Vec<_>>()
        .join(" ");
    execute(
        client,
        workspace_id,
        vec![
            OsString::from("/bin/sh"),
            OsString::from("-c"),
            OsString::from(command),
        ],
    )
}

fn visible_head(client: &Client, branch_id: BranchId, expected: Option<CommitId>) -> AnyResult<()> {
    let mut query = Query::new(QueryKind::Branches).limit(512);
    loop {
        let page = client.query(query.clone())?;
        if page.items.iter().any(|item| {
            matches!(item, QueryItem::Branch(branch) if branch.id == branch_id && branch.head_commit_id == expected)
        }) {
            return Ok(());
        }
        let Some(next) = page.into_next_query(&query) else {
            return Err("Commit not visible from public SDK query".into());
        };
        query = next;
    }
}

fn parse_inner_write_ns(output: &layerfs_sdk::OutputPage) -> Option<u64> {
    output
        .chunks
        .iter()
        .flat_map(|chunk| {
            String::from_utf8_lossy(&chunk.bytes)
                .into_owned()
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .find_map(|line| line.strip_prefix("inner_write_ns=")?.parse().ok())
}

fn parse_digest(output: &OutputPage) -> AnyResult<(u64, String)> {
    let bytes = output
        .chunks
        .iter()
        .flat_map(|chunk| chunk.bytes.iter().copied())
        .collect::<Vec<_>>();
    parse_digest_text(std::str::from_utf8(&bytes)?)
}

fn parse_read_bytes(output: &OutputPage) -> AnyResult<u64> {
    let bytes = output
        .chunks
        .iter()
        .flat_map(|chunk| chunk.bytes.iter().copied())
        .collect::<Vec<_>>();
    std::str::from_utf8(&bytes)?
        .lines()
        .find_map(|line| line.strip_prefix("read_bytes=")?.parse().ok())
        .ok_or_else(|| "read output".into())
}

fn parse_digest_text(output: &str) -> AnyResult<(u64, String)> {
    for line in output.lines() {
        let Some((size, digest)) = line.split_once('\t') else {
            continue;
        };
        if digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Ok((size.parse()?, digest.to_ascii_lowercase()));
        }
    }
    Err("digest output".into())
}

fn process_digest(workload: &OsString, path: &std::ffi::OsStr) -> AnyResult<(u64, String)> {
    let output = Command::new(workload).arg("digest").arg(path).output()?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err("oracle digest process".into());
    }
    parse_digest_text(std::str::from_utf8(&output.stdout)?)
}

fn build_prepend_oracle(fixture: &Path, output: &Path) -> AnyResult<()> {
    let mut target = std::fs::File::create(output)?;
    target.write_all(b"PREPEND010")?;
    std::io::copy(&mut std::fs::File::open(fixture)?, &mut target)?;
    target.sync_all()?;
    Ok(())
}

fn prove_case(
    workload: &OsString,
    proof: ProofCase,
    expected_size: u64,
    expected_digest: &str,
) -> AnyResult<()> {
    let store = Arc::new(LayerStackStore::connect(&proof.store)?);
    let client = Client::connect(store)?;
    visible_head(&client, proof.branch, proof.head)?;
    let session = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: proof.branch,
        placement: proof.placement,
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let output = execute_workload(
        &client,
        session.id,
        vec![
            workload.clone(),
            OsString::from("verify"),
            OsString::from("payload.bin"),
            OsString::from(expected_size.to_string()),
            OsString::from(expected_digest),
        ],
    )?;
    if parse_digest(&output)? != (expected_size, expected_digest.to_owned()) {
        return Err("proof digest output".into());
    }
    client.end_workspace_session(session.id, EndWorkspaceMode::Clean)?;
    visible_head(&client, proof.branch, proof.head)
}

fn emit_sample(case: &str, iteration: usize, sample: &LifecycleSample) {
    println!(
        "{{\"schema\":\"fs-bench-pro-v4\",\"case\":\"{case}\",\"iteration\":{iteration},\"workspace_create_ns\":{},\"execution_ns\":{},\"commit_api_ns\":{},\"layerstack_visible_ns\":{},\"workspace_end_ns\":{},\"complete_lifecycle_ns\":{},\"inner_write_ns\":{}}}",
        sample.workspace_create_ns,
        sample.execution_ns,
        sample.commit_api_ns,
        sample.layerstack_visible_ns,
        sample.workspace_end_ns,
        sample.complete_lifecycle_ns,
        sample.inner_write_ns.map_or("null".to_owned(), |value| value.to_string()),
    );
}

fn emit_execution_receipt(case: &str, iteration: usize, output: &OutputPage) {
    println!(
        "DIAGNOSTIC case={case} iteration={iteration} execution={:?}",
        output.receipt
    );
}

fn emit_diagnostics(client: &Client, case: &str, iteration: usize) -> AnyResult<()> {
    let snapshot = client.monitor_snapshot()?;
    for operation in snapshot.operations.iter().rev().take(6).rev() {
        println!("DIAGNOSTIC case={case} iteration={iteration} operation={operation:?}");
    }
    Ok(())
}

fn emit_store_census(case: &str, iteration: usize, root: &Path) -> AnyResult<()> {
    let mut files = std::fs::read_dir(root)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    files.sort();
    if files != [OsString::from("store.sqlite")] {
        return Err(format!("Store file census: {files:?}").into());
    }
    let metadata = std::fs::metadata(root.join("store.sqlite"))?;
    let connection = rusqlite::Connection::open_with_flags(
        root.join("store.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let (canonical_objects, canonical_bytes): (i64, i64) = connection.query_row(
        "SELECT count(*), coalesce(sum(length(bytes)), 0) FROM objects",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let commits: i64 =
        connection.query_row("SELECT count(*) FROM commits", [], |row| row.get(0))?;
    let canonical_objects = u64::try_from(canonical_objects)?;
    let canonical_bytes = u64::try_from(canonical_bytes)?;
    let commits = u64::try_from(commits)?;
    #[cfg(unix)]
    let allocated_bytes = {
        use std::os::unix::fs::MetadataExt;
        metadata.blocks().saturating_mul(512)
    };
    #[cfg(not(unix))]
    let allocated_bytes = metadata.len();
    println!(
        "{{\"schema\":\"fs-bench-pro-v4-store\",\"case\":\"{case}\",\"iteration\":{iteration},\"database_bytes\":{},\"allocated_bytes\":{allocated_bytes},\"page_count\":{},\"canonical_objects\":{canonical_objects},\"canonical_bytes\":{canonical_bytes},\"commits\":{commits}}}",
        metadata.len(),
        metadata.len() / (64 * 1024)
    );
    Ok(())
}

fn median(mut values: Vec<u64>) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[values.len() / 2]
}

fn read_u64(path: &str) -> u64 {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn linux_process_peak_rss_bytes() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                let kib = line.strip_prefix("VmHWM:")?.split_whitespace().next()?;
                kib.parse::<u64>().ok()?.checked_mul(1024)
            })
        })
        .unwrap_or(0)
}

#[cfg(not(target_os = "linux"))]
fn linux_process_peak_rss_bytes() -> u64 {
    0
}

fn nanos(start: Instant, end: Instant) -> u64 {
    end.duration_since(start)
        .as_nanos()
        .min(u128::from(u64::MAX)) as u64
}

fn elapsed_ns(start: Instant) -> u64 {
    start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_equations_and_median_are_exact() {
        self_check().unwrap();
        assert_eq!(median(vec![5, 1, 3]), 3);
    }
}
