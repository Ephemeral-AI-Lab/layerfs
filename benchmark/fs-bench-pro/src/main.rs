use layerfs_sdk::{
    BranchId, CandidateStats, Client, CommitId, ContainerId, CreateWorkspaceSession,
    EndWorkspaceMode, EntityName, ExecutionTransport, LayerStackInitialization, LayerStackStore,
    LocalForkSource, NonEmpty, OperationFamily, OutputPage, Query, QueryItem, QueryKind,
    WorkspaceCommitResult, WorkspaceId, WorkspacePlacement, WorkspaceProjection,
};
use std::ffi::OsString;
use std::io::Write;
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
#[allow(dead_code)]
mod workload_source {
    include!("../workload.rs");
}

type NamespaceScenario = workload_source::NamespaceScenario;

#[derive(Clone, Debug, Eq, PartialEq)]
struct NamespaceManifest {
    regular_files: u64,
    data_directories: u64,
    logical_bytes: u64,
    empty_files: u64,
    tiny_files: u64,
    small_files: u64,
    medium_files: u64,
    anchor_files: u64,
    anchor_bytes: u64,
    file_mode: u32,
    directory_mode: u32,
    mtime_seconds: i64,
    mtime_nanoseconds: u32,
    digest: String,
}

struct GeneratedNamespaceFixture {
    manifest: NamespaceManifest,
    edited_digest: String,
    edit_path: String,
    edit_size: u64,
    fixture_plan_ns: u64,
    fixture_generate_ns: u64,
    fixture_manifest_ns: u64,
    maximum_fixture_write_buffer_bytes: u64,
    fixture_write_calls: u64,
    fixture_open_calls: u64,
    fixture_content_bytes_generated: u64,
    fixture_content_bytes_written: u64,
    fixture_content_hash_input_bytes: u64,
    fixture_plan_bytes: u64,
    fixture_path_state_bytes: u64,
    fixture_digest_record_bytes: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct NamespaceSample {
    layerstack_init_ns: u64,
    branch_fork_ns: u64,
    workspace_create_ns: u64,
    edit_ns: u64,
    commit_ns: u64,
    workspace_end_ns: u64,
    reconnect_ns: u64,
    reopen_workspace_create_ns: u64,
    reopen_workspace_end_ns: u64,
    product_lifecycle_ns: u64,
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
            self.reconnect_ns,
            self.reopen_workspace_create_ns,
            self.reopen_workspace_end_ns,
        ]
        .into_iter()
        .try_fold(0_u64, u64::checked_add)
        .ok_or("namespace phase overflow")?;
        if phases != self.product_lifecycle_ns
            || self.reconnect_ns == 0
            || self.reopen_workspace_create_ns == 0
            || self.reopen_workspace_end_ns == 0
        {
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
        [
            command,
            root,
            fixture,
            container,
            scenario,
            iteration,
            fixture_digest,
            edited_digest,
            edit_path,
            edit_size,
            fixture_cache_profile,
        ] if command == "namespace" => {
            namespace_case(
                Path::new(root),
                Path::new(fixture),
                ContainerId(container.to_string_lossy().into_owned()),
                namespace_scenario(&scenario.to_string_lossy())?,
                iteration.to_string_lossy().parse()?,
                &fixture_digest.to_string_lossy(),
                &edited_digest.to_string_lossy(),
                &edit_path.to_string_lossy(),
                edit_size.to_string_lossy().parse()?,
                &fixture_cache_profile.to_string_lossy(),
            )
        }
        [
            command,
            root,
            fixture,
            scenario,
            iteration,
            fixture_digest,
            fixture_cache_profile,
        ] if command == "namespace-init-diagnostic" => namespace_init_diagnostic(
            Path::new(root),
            Path::new(fixture),
            namespace_scenario(&scenario.to_string_lossy())?,
            iteration.to_string_lossy().parse()?,
            &fixture_digest.to_string_lossy(),
            &fixture_cache_profile.to_string_lossy(),
        ),
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
        _ => Err("usage: fs-benchmark-pro self-check | namespace-fixture FIXTURE SCENARIO | namespace ROOT FIXTURE CONTAINER SCENARIO ITERATION FIXTURE_DIGEST EDITED_DIGEST EDIT_PATH EDIT_SIZE FIXTURE_CACHE_PROFILE | namespace-init-diagnostic ROOT FIXTURE SCENARIO ITERATION FIXTURE_DIGEST FIXTURE_CACHE_PROFILE | run ROOT FIXTURE [CONTAINER ITERATIONS]".into()),
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
    workload_source::namespace_scenario(id)
}

fn create_namespace_fixture(
    root: &Path,
    scenario: NamespaceScenario,
) -> AnyResult<GeneratedNamespaceFixture> {
    if root.exists() {
        return Err("namespace fixture already exists".into());
    }
    let plan_started = Instant::now();
    let plan = workload_source::namespace_plan(scenario.id)?;
    let fixture_plan_ns = elapsed_ns(plan_started);
    let fixture_plan_bytes = workload_source::namespace_plan_owned_bytes(&plan)?;
    let fixture_path_state_bytes = u64::try_from(
        plan.files
            .iter()
            .try_fold(plan.edit_path.capacity(), |total, file| {
                total.checked_add(file.relative_path.capacity())
            })
            .ok_or("namespace fixture path ownership")?,
    )?;
    let parent = root.parent().ok_or("namespace fixture parent")?;
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("namespace fixture name")?;
    let partial = parent.join(format!(
        ".{name}.partial-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let generated = (|| -> AnyResult<GeneratedNamespaceFixture> {
        std::fs::create_dir(&partial)?;
        for directory in 0..scenario.data_directories {
            std::fs::create_dir(partial.join(format!("d{directory:04}")))?;
        }
        let generated_started = Instant::now();
        let mut buffer = vec![0_u8; workload_source::NAMESPACE_SCRATCH_BYTES];
        let maximum_fixture_write_buffer_bytes = u64::try_from(buffer.capacity())?;
        let mut content_digests = vec![None; plan.files.len()];
        let fixture_digest_record_bytes = u64::try_from(
            content_digests
                .capacity()
                .checked_mul(std::mem::size_of::<Option<[u8; 32]>>())
                .ok_or("namespace fixture digest ownership")?,
        )?;
        let edit_index = plan
            .files
            .iter()
            .position(|file| file.relative_path == plan.edit_path)
            .ok_or("namespace fixture edit index")?;
        let edit_offset = workload_source::namespace_edit_offset(plan.edit_size)?;
        let mut edited_content_digest = None;
        let mut fixture_write_calls = 0_u64;
        let mut fixture_open_calls = 0_u64;
        let mut fixture_content_bytes_generated = 0_u64;
        let mut fixture_content_bytes_written = 0_u64;
        let mut fixture_content_hash_input_bytes = 0_u64;
        for (index, file) in plan.files.iter().enumerate() {
            let path = partial.join(&file.relative_path);
            let mut output = std::fs::File::create(&path)?;
            fixture_open_calls = fixture_open_calls
                .checked_add(1)
                .ok_or("namespace fixture open calls")?;
            let mut stream = workload_source::NamespaceContentStream::new(scenario, file);
            let mut content_hash = workload_source::Sha256::new();
            let mut edited_hash = (index == edit_index).then(workload_source::Sha256::new);
            let mut offset = 0_u64;
            while offset < file.size {
                let count =
                    usize::try_from((file.size - offset).min(u64::try_from(buffer.len())?))?;
                stream.fill(&mut buffer[..count]);
                fixture_content_bytes_generated = fixture_content_bytes_generated
                    .checked_add(u64::try_from(count)?)
                    .ok_or("namespace fixture generated bytes")?;
                output.write_all(&buffer[..count])?;
                fixture_write_calls = fixture_write_calls
                    .checked_add(1)
                    .ok_or("namespace fixture write calls")?;
                fixture_content_bytes_written = fixture_content_bytes_written
                    .checked_add(u64::try_from(count)?)
                    .ok_or("namespace fixture written bytes")?;
                content_hash.update(&buffer[..count]);
                fixture_content_hash_input_bytes = fixture_content_hash_input_bytes
                    .checked_add(u64::try_from(count)?)
                    .ok_or("namespace fixture hash input bytes")?;
                if let Some(hash) = edited_hash.as_mut() {
                    update_edited_hash(hash, &buffer[..count], offset, edit_offset)?;
                    fixture_content_hash_input_bytes = fixture_content_hash_input_bytes
                        .checked_add(u64::try_from(count)?)
                        .ok_or("namespace fixture edited hash input bytes")?;
                }
                offset = offset
                    .checked_add(u64::try_from(count)?)
                    .ok_or("namespace fixture write offset")?;
            }
            if output.metadata()?.len() != file.size {
                return Err("namespace fixture generated size".into());
            }
            drop(output);
            workload_source::set_namespace_metadata(&path, false)?;
            content_digests[index] = Some(content_hash.finish());
            if let Some(hash) = edited_hash {
                edited_content_digest = Some(hash.finish());
            }
        }
        for directory in 0..scenario.data_directories {
            workload_source::set_namespace_metadata(
                &partial.join(format!("d{directory:04}")),
                true,
            )?;
        }
        workload_source::set_namespace_metadata(&partial, true)?;
        let fixture_generate_ns = elapsed_ns(generated_started);
        let manifest_started = Instant::now();
        let fixture_digest = workload_source::namespace_tree_digest(&plan, &content_digests)?;
        let original_edit_digest = content_digests[edit_index]
            .replace(edited_content_digest.ok_or("namespace edited content digest")?)
            .ok_or("namespace original edit digest")?;
        let edited_digest = workload_source::namespace_tree_digest(&plan, &content_digests)?;
        content_digests[edit_index] = Some(original_edit_digest);
        let manifest = manifest_from_plan(&plan, fixture_digest);
        std::fs::rename(&partial, root)?;
        let fixture_manifest_ns = elapsed_ns(manifest_started);
        Ok(GeneratedNamespaceFixture {
            manifest,
            edited_digest,
            edit_path: plan.edit_path.clone(),
            edit_size: plan.edit_size,
            fixture_plan_ns,
            fixture_generate_ns,
            fixture_manifest_ns,
            maximum_fixture_write_buffer_bytes,
            fixture_write_calls,
            fixture_open_calls,
            fixture_content_bytes_generated,
            fixture_content_bytes_written,
            fixture_content_hash_input_bytes,
            fixture_plan_bytes,
            fixture_path_state_bytes,
            fixture_digest_record_bytes,
        })
    })();
    if generated.is_err() {
        let _ = std::fs::remove_dir_all(&partial);
    }
    generated
}

fn update_edited_hash(
    hash: &mut workload_source::Sha256,
    bytes: &[u8],
    chunk_offset: u64,
    edit_offset: u64,
) -> AnyResult<()> {
    let chunk_end = chunk_offset
        .checked_add(u64::try_from(bytes.len())?)
        .ok_or("namespace edited chunk end")?;
    let edit_end = edit_offset
        .checked_add(u64::try_from(workload_source::NAMESPACE_EDIT_MARKER.len())?)
        .ok_or("namespace edit end")?;
    if chunk_end <= edit_offset || chunk_offset >= edit_end {
        hash.update(bytes);
        return Ok(());
    }
    let overlap_start = chunk_offset.max(edit_offset);
    let overlap_end = chunk_end.min(edit_end);
    let before = usize::try_from(overlap_start - chunk_offset)?;
    let after = usize::try_from(overlap_end - chunk_offset)?;
    let marker_start = usize::try_from(overlap_start - edit_offset)?;
    let marker_end = usize::try_from(overlap_end - edit_offset)?;
    hash.update(&bytes[..before]);
    hash.update(&workload_source::NAMESPACE_EDIT_MARKER[marker_start..marker_end]);
    hash.update(&bytes[after..]);
    Ok(())
}

fn manifest_from_plan(plan: &workload_source::NamespacePlan, digest: String) -> NamespaceManifest {
    NamespaceManifest {
        regular_files: plan.scenario.regular_files,
        data_directories: plan.scenario.data_directories,
        logical_bytes: plan.scenario.logical_bytes,
        empty_files: plan.empty_files,
        tiny_files: plan.tiny_files,
        small_files: plan.small_files,
        medium_files: plan.medium_files,
        anchor_files: plan.anchor_files,
        anchor_bytes: plan.anchor_bytes,
        file_mode: workload_source::NAMESPACE_FILE_MODE,
        directory_mode: workload_source::NAMESPACE_DIRECTORY_MODE,
        mtime_seconds: workload_source::NAMESPACE_MTIME_SECONDS,
        mtime_nanoseconds: workload_source::NAMESPACE_MTIME_NANOSECONDS,
        digest,
    }
}

fn emit_namespace_manifest(scenario: NamespaceScenario, fixture: &GeneratedNamespaceFixture) {
    let manifest = &fixture.manifest;
    let files_per_second = rate(manifest.regular_files, fixture.fixture_generate_ns);
    let bytes_per_second = rate(manifest.logical_bytes, fixture.fixture_generate_ns);
    println!(
        "{{\"schema\":\"{}\",\"scenario\":\"{}\",\"fixture_profile\":\"{}\",\"fixture_digest_profile\":\"{}\",\"edit_contract\":\"{}\",\"regular_files\":{},\"data_directories\":{},\"logical_bytes\":{},\"empty_files\":{},\"tiny_files\":{},\"small_files\":{},\"medium_files\":{},\"anchor_files\":{},\"anchor_bytes\":{},\"file_mode\":{},\"directory_mode\":{},\"mtime_seconds\":{},\"mtime_nanoseconds\":{},\"fixture_digest\":\"{}\",\"edited_fixture_digest\":\"{}\",\"edit_path\":\"{}\",\"edit_size\":{},\"fixture_plan_ns\":{},\"fixture_generate_ns\":{},\"fixture_manifest_ns\":{},\"fixture_files_per_second\":{},\"fixture_bytes_per_second\":{},\"fixture_worker_count\":1,\"fixture_cache_profile\":\"generated-warm-uncontrolled\",\"maximum_fixture_write_buffer_bytes\":{},\"fixture_plan_bytes\":{},\"fixture_path_state_bytes\":{},\"fixture_digest_record_bytes\":{},\"fixture_open_calls\":{},\"fixture_write_calls\":{},\"fixture_content_bytes_generated\":{},\"fixture_content_bytes_written\":{},\"fixture_content_hash_input_bytes\":{},\"post_generation_content_rereads\":0,\"complete_file_vec_allocations\":0,\"per_file_fsyncs\":0,\"atomic_publish\":true}}",
        workload_source::NAMESPACE_FIXTURE_SCHEMA,
        scenario.id,
        workload_source::NAMESPACE_FIXTURE_PROFILE,
        workload_source::NAMESPACE_DIGEST_PROFILE,
        workload_source::NAMESPACE_EDIT_CONTRACT,
        manifest.regular_files,
        manifest.data_directories,
        manifest.logical_bytes,
        manifest.empty_files,
        manifest.tiny_files,
        manifest.small_files,
        manifest.medium_files,
        manifest.anchor_files,
        manifest.anchor_bytes,
        manifest.file_mode,
        manifest.directory_mode,
        manifest.mtime_seconds,
        manifest.mtime_nanoseconds,
        manifest.digest,
        fixture.edited_digest,
        fixture.edit_path,
        fixture.edit_size,
        fixture.fixture_plan_ns,
        fixture.fixture_generate_ns,
        fixture.fixture_manifest_ns,
        files_per_second,
        bytes_per_second,
        fixture.maximum_fixture_write_buffer_bytes,
        fixture.fixture_plan_bytes,
        fixture.fixture_path_state_bytes,
        fixture.fixture_digest_record_bytes,
        fixture.fixture_open_calls,
        fixture.fixture_write_calls,
        fixture.fixture_content_bytes_generated,
        fixture.fixture_content_bytes_written,
        fixture.fixture_content_hash_input_bytes,
    );
}

fn rate(units: u64, elapsed_ns: u64) -> u64 {
    u128::from(units)
        .checked_mul(1_000_000_000)
        .and_then(|value| value.checked_div(u128::from(elapsed_ns.max(1))))
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or(u64::MAX)
}

fn namespace_self_check() -> AnyResult<()> {
    let expected = [
        ("namespace-100", [1, 78, 15, 5, 1], 125_000_000),
        ("namespace-1000", [10, 789, 150, 50, 1], 200_000_000),
        ("namespace-10000", [100, 7_899, 1_500, 500, 1], 300_000_000),
        (
            "namespace-100000",
            [1_000, 78_998, 15_000, 5_000, 2],
            500_000_000,
        ),
    ];
    for (id, counts, logical_bytes) in expected {
        let first = workload_source::namespace_plan(id)?;
        let second = workload_source::namespace_plan(id)?;
        if first != second
            || [
                first.empty_files,
                first.tiny_files,
                first.small_files,
                first.medium_files,
                first.anchor_files,
            ] != counts
            || first.scenario.logical_bytes != logical_bytes
        {
            return Err("namespace-v2 planner self-check".into());
        }
    }
    NamespaceSample {
        layerstack_init_ns: 1,
        branch_fork_ns: 2,
        workspace_create_ns: 3,
        edit_ns: 4,
        commit_ns: 5,
        workspace_end_ns: 6,
        reconnect_ns: 7,
        reopen_workspace_create_ns: 8,
        reopen_workspace_end_ns: 9,
        product_lifecycle_ns: 45,
    }
    .validate()?;
    Ok(())
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

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        && value.bytes().all(|byte| !byte.is_ascii_uppercase())
}

fn namespace_manifest(scenario: NamespaceScenario, digest: &str) -> AnyResult<NamespaceManifest> {
    Ok(NamespaceManifest {
        regular_files: scenario.regular_files,
        data_directories: scenario.data_directories,
        logical_bytes: scenario.logical_bytes,
        empty_files: scenario.empty_files,
        tiny_files: scenario.tiny_files,
        small_files: scenario.small_files,
        medium_files: scenario.medium_files,
        anchor_files: scenario.anchor_files,
        anchor_bytes: scenario
            .anchor_files
            .checked_mul(workload_source::NAMESPACE_ANCHOR_BYTES)
            .ok_or("namespace anchor bytes")?,
        file_mode: workload_source::NAMESPACE_FILE_MODE,
        directory_mode: workload_source::NAMESPACE_DIRECTORY_MODE,
        mtime_seconds: workload_source::NAMESPACE_MTIME_SECONDS,
        mtime_nanoseconds: workload_source::NAMESPACE_MTIME_NANOSECONDS,
        digest: digest.to_owned(),
    })
}

#[allow(clippy::too_many_arguments)]
fn namespace_case(
    root: &Path,
    fixture: &Path,
    container_id: ContainerId,
    scenario: NamespaceScenario,
    iteration: usize,
    fixture_digest: &str,
    edited_digest: &str,
    edit_path: &str,
    edit_size: u64,
    fixture_cache_profile: &str,
) -> AnyResult<()> {
    if iteration == 0 {
        return Err("namespace iteration must be positive".into());
    }
    if !fixture.is_dir()
        || !valid_digest(fixture_digest)
        || !valid_digest(edited_digest)
        || fixture_digest == edited_digest
        || !matches!(
            fixture_cache_profile,
            "generated-first-use-uncontrolled"
                | "generated-post-first-use-uncontrolled"
                | "reused-first-use-uncontrolled"
                | "reused-post-first-use-uncontrolled"
        )
        || edit_path.starts_with('/')
        || edit_path.contains("..")
        || edit_size <= u64::try_from(workload_source::NAMESPACE_EDIT_MARKER.len())?
    {
        return Err("namespace fixture manifest arguments".into());
    }
    let setup_started = Instant::now();
    let fixture_manifest = namespace_manifest(scenario, fixture_digest)?;
    std::fs::create_dir(root)?;
    let store_path = root.join("store.sqlite");
    let store = Arc::new(LayerStackStore::create(&store_path)?);
    let client = Client::connect(store.clone())?;
    let store_baseline_bytes = std::fs::metadata(&store_path)?.len();
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
            OsString::from("namespace-edit"),
            OsString::from(edit_path),
        ],
    )?;
    let t4 = Instant::now();

    let commit = client.commit_workspace_session(workspace.id);
    let t5 = Instant::now();
    let commit_failure = |error: String| -> AnyResult<()> {
        let ended = client.end_workspace_session(workspace.id, EndWorkspaceMode::Discard);
        let t6 = Instant::now();
        println!(
            "{{\"schema\":\"{}\",\"scenario\":\"{}\",\"iteration\":{iteration},\"fixture_profile\":\"{}\",\"fixture_digest_profile\":\"{}\",\"edit_contract\":\"{}\",\"result_profile\":\"{}\",\"measurement_mode\":\"product-lifecycle\",\"fixture_cache_profile\":\"{}\",\"failed_phase\":\"commit\",\"error\":{:?},\"layerstack_init_ns\":{},\"branch_fork_ns\":{},\"workspace_create_ns\":{},\"edit_ns\":{},\"commit_ns\":{},\"workspace_end_ns\":{},\"regular_files\":{},\"data_directories\":{},\"logical_bytes\":{},\"empty_files\":{},\"tiny_files\":{},\"small_files\":{},\"medium_files\":{},\"anchor_files\":{},\"anchor_bytes\":{},\"file_mode\":{},\"directory_mode\":{},\"mtime_seconds\":{},\"mtime_nanoseconds\":{},\"fixture_digest\":\"{}\",\"scanned_files\":{},\"scanned_bytes\":{}}}",
            workload_source::NAMESPACE_FAILURE_SCHEMA,
            scenario.id,
            workload_source::NAMESPACE_FIXTURE_PROFILE,
            workload_source::NAMESPACE_DIGEST_PROFILE,
            workload_source::NAMESPACE_EDIT_CONTRACT,
            workload_source::NAMESPACE_LIFECYCLE_PROFILE,
            fixture_cache_profile,
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
            fixture_manifest.empty_files,
            fixture_manifest.tiny_files,
            fixture_manifest.small_files,
            fixture_manifest.medium_files,
            fixture_manifest.anchor_files,
            fixture_manifest.anchor_bytes,
            fixture_manifest.file_mode,
            fixture_manifest.directory_mode,
            fixture_manifest.mtime_seconds,
            fixture_manifest.mtime_nanoseconds,
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
    let reconnect_started = Instant::now();
    drop(client);
    drop(store);

    let reopened_store = Arc::new(LayerStackStore::connect(&store_path)?);
    let reopened = Client::connect(reopened_store.clone())?;
    visible_head(&reopened, branch, head)?;
    let t7 = Instant::now();
    let reopened_workspace = reopened
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: branch,
            placement: namespace_placement(&container_id, scenario, iteration, "reopen"),
            projection: Some(WorkspaceProjection::Fuse),
        })
        .map_err(|error| format!("namespace reopened Workspace create failed: {error}"))?;
    let t8 = Instant::now();
    reopened
        .end_workspace_session(reopened_workspace.id, EndWorkspaceMode::Clean)
        .map_err(|error| format!("namespace reopened Workspace End failed: {error}"))?;
    let t9 = Instant::now();
    if reopened.active_workspace_count()? != 0 || reopened.active_execution_count()? != 0 {
        return Err("namespace reopened Workspace leaked runtime state".into());
    }
    let reopen_snapshot = reopened.monitor_snapshot()?;
    let store_storage = reopened_store.storage_snapshot()?;
    let canonical_storage = reopened_store.canonical_storage()?;
    let store_database_bytes = store_storage.database_bytes;
    let store_growth_bytes = store_database_bytes.saturating_sub(store_baseline_bytes);
    eprintln!("NAMESPACE_DIAGNOSTIC scan={scan:?}");
    eprintln!("NAMESPACE_DIAGNOSTIC operations={snapshot:?}");
    eprintln!("NAMESPACE_DIAGNOSTIC reopen_operations={reopen_snapshot:?}");

    let sample = NamespaceSample {
        layerstack_init_ns: nanos(t0, t1),
        branch_fork_ns: nanos(t1, t2),
        workspace_create_ns: nanos(t2, t3),
        edit_ns: nanos(t3, t4),
        commit_ns: nanos(t4, t5),
        workspace_end_ns: nanos(t5, t6),
        reconnect_ns: nanos(reconnect_started, t7),
        reopen_workspace_create_ns: nanos(t7, t8),
        reopen_workspace_end_ns: nanos(t8, t9),
        product_lifecycle_ns: 0,
    };
    let sample = NamespaceSample {
        product_lifecycle_ns: [
            sample.layerstack_init_ns,
            sample.branch_fork_ns,
            sample.workspace_create_ns,
            sample.edit_ns,
            sample.commit_ns,
            sample.workspace_end_ns,
            sample.reconnect_ns,
            sample.reopen_workspace_create_ns,
            sample.reopen_workspace_end_ns,
        ]
        .into_iter()
        .try_fold(0_u64, u64::checked_add)
        .ok_or("namespace product lifecycle overflow")?,
        ..sample
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

    let init_bytes_per_second = rate(fixture_manifest.logical_bytes, sample.layerstack_init_ns);
    let init_files_per_second = rate(fixture_manifest.regular_files, sample.layerstack_init_ns);
    println!(
        "{{\"schema\":\"{}\",\"scenario\":\"{}\",\"iteration\":{iteration},\"fixture_profile\":\"{}\",\"fixture_digest_profile\":\"{}\",\"edit_contract\":\"{}\",\"result_profile\":\"{}\",\"measurement_mode\":\"product-lifecycle\",\"fixture_cache_profile\":\"{}\",\"setup_ns\":{setup_ns},\"layerstack_init_ns\":{},\"branch_fork_ns\":{},\"workspace_create_ns\":{},\"edit_ns\":{},\"commit_ns\":{},\"workspace_end_ns\":{},\"reconnect_ns\":{},\"reopen_workspace_create_ns\":{},\"reopen_workspace_end_ns\":{},\"product_lifecycle_ns\":{},\"init_bytes_per_second\":{init_bytes_per_second},\"init_files_per_second\":{init_files_per_second},\"regular_files\":{},\"data_directories\":{},\"logical_bytes\":{},\"empty_files\":{},\"tiny_files\":{},\"small_files\":{},\"medium_files\":{},\"anchor_files\":{},\"anchor_bytes\":{},\"file_mode\":{},\"directory_mode\":{},\"mtime_seconds\":{},\"mtime_nanoseconds\":{},\"edit_path\":\"{}\",\"edit_size\":{},\"fixture_digest\":\"{}\",\"scanned_files\":{},\"scanned_bytes\":{},\"candidate_objects\":{candidate_objects},\"candidate_bytes\":{candidate_bytes},\"inserted_objects\":{inserted_objects},\"inserted_bytes\":{inserted_bytes},\"reused_objects\":{reused_objects},\"reused_bytes\":{reused_bytes},\"max_transaction_objects\":{},\"max_transaction_bytes\":{},\"initialize_candidate_objects\":{},\"initialize_candidate_bytes\":{},\"initialize_inserted_objects\":{},\"initialize_inserted_bytes\":{},\"initialize_reused_objects\":{},\"initialize_reused_bytes\":{},\"initialize_batch_inserted_objects\":{},\"initialize_batch_inserted_bytes\":{},\"initialize_final_inserted_objects\":{},\"initialize_final_inserted_bytes\":{},\"initialize_preexisting_reused_objects\":{},\"initialize_preexisting_reused_bytes\":{},\"initialize_admission_transactions\":{},\"initialize_max_transaction_objects\":{},\"initialize_max_transaction_bytes\":{},\"commit_candidate_objects\":{},\"commit_candidate_bytes\":{},\"commit_inserted_objects\":{},\"commit_inserted_bytes\":{},\"commit_reused_objects\":{},\"commit_reused_bytes\":{},\"commit_admission_transactions\":{},\"commit_max_transaction_objects\":{},\"commit_max_transaction_bytes\":{},\"store_baseline_bytes\":{store_baseline_bytes},\"store_database_bytes\":{store_database_bytes},\"store_growth_bytes\":{store_growth_bytes},\"store_canonical_objects\":{},\"store_canonical_bytes\":{}}}",
        workload_source::NAMESPACE_SCHEMA,
        scenario.id,
        workload_source::NAMESPACE_FIXTURE_PROFILE,
        workload_source::NAMESPACE_DIGEST_PROFILE,
        workload_source::NAMESPACE_EDIT_CONTRACT,
        workload_source::NAMESPACE_LIFECYCLE_PROFILE,
        fixture_cache_profile,
        sample.layerstack_init_ns,
        sample.branch_fork_ns,
        sample.workspace_create_ns,
        sample.edit_ns,
        sample.commit_ns,
        sample.workspace_end_ns,
        sample.reconnect_ns,
        sample.reopen_workspace_create_ns,
        sample.reopen_workspace_end_ns,
        sample.product_lifecycle_ns,
        fixture_manifest.regular_files,
        fixture_manifest.data_directories,
        fixture_manifest.logical_bytes,
        fixture_manifest.empty_files,
        fixture_manifest.tiny_files,
        fixture_manifest.small_files,
        fixture_manifest.medium_files,
        fixture_manifest.anchor_files,
        fixture_manifest.anchor_bytes,
        fixture_manifest.file_mode,
        fixture_manifest.directory_mode,
        fixture_manifest.mtime_seconds,
        fixture_manifest.mtime_nanoseconds,
        edit_path,
        edit_size,
        fixture_manifest.digest,
        scan.scanned_files,
        scan.scanned_bytes,
        initialize_candidate
            .max_transaction_objects
            .max(commit_candidate.max_transaction_objects),
        initialize_candidate
            .max_transaction_bytes
            .max(commit_candidate.max_transaction_bytes),
        initialize_candidate.candidate_objects,
        initialize_candidate.candidate_bytes,
        initialize_candidate.inserted_objects,
        initialize_candidate.inserted_bytes,
        initialize_candidate.reused_objects,
        initialize_candidate.reused_bytes,
        initialize_candidate.batch_inserted_objects,
        initialize_candidate.batch_inserted_bytes,
        initialize_candidate.final_inserted_objects,
        initialize_candidate.final_inserted_bytes,
        initialize_candidate.preexisting_reused_objects,
        initialize_candidate.preexisting_reused_bytes,
        initialize_candidate.admission_transactions,
        initialize_candidate.max_transaction_objects,
        initialize_candidate.max_transaction_bytes,
        commit_candidate.candidate_objects,
        commit_candidate.candidate_bytes,
        commit_candidate.inserted_objects,
        commit_candidate.inserted_bytes,
        commit_candidate.reused_objects,
        commit_candidate.reused_bytes,
        commit_candidate.admission_transactions,
        commit_candidate.max_transaction_objects,
        commit_candidate.max_transaction_bytes,
        canonical_storage.objects,
        canonical_storage.encoded_bytes,
    );
    Ok(())
}

fn namespace_init_diagnostic(
    root: &Path,
    fixture: &Path,
    scenario: NamespaceScenario,
    iteration: usize,
    fixture_digest: &str,
    fixture_cache_profile: &str,
) -> AnyResult<()> {
    if iteration == 0
        || !fixture.is_dir()
        || !valid_digest(fixture_digest)
        || !matches!(
            fixture_cache_profile,
            "generated-first-use-uncontrolled"
                | "generated-post-first-use-uncontrolled"
                | "reused-first-use-uncontrolled"
                | "reused-post-first-use-uncontrolled"
        )
    {
        return Err("namespace init-only diagnostic arguments".into());
    }
    let fixture_manifest = namespace_manifest(scenario, fixture_digest)?;
    let setup_started = Instant::now();
    std::fs::create_dir(root)?;
    let store_path = root.join("store.sqlite");
    let store = Arc::new(LayerStackStore::create(&store_path)?);
    let client = Client::connect(store.clone())?;
    let store_baseline_bytes = std::fs::metadata(&store_path)?.len();
    let setup_ns = elapsed_ns(setup_started);
    if !store.take_layerstack_initialization_receipts().is_empty() {
        return Err("stale LayerStack initialization receipt".into());
    }

    let t0 = Instant::now();
    let initialized = client.initialize_layerstack(
        EntityName::new(format!("{}-{iteration}-init-diagnostic", scenario.id))?,
        LayerStackInitialization::Directory(fixture.to_owned()),
    )?;
    let t1 = Instant::now();
    let receipts = store.take_layerstack_initialization_receipts();
    let [scan] = receipts.as_slice() else {
        return Err("LayerStack initialization receipt cardinality".into());
    };
    if scan.layer_stack_id != initialized.layer_stack_id
        || scan.scanned_files != fixture_manifest.regular_files
        || scan.scanned_bytes != fixture_manifest.logical_bytes
    {
        return Err("LayerStack initialization scan receipt mismatch".into());
    }
    let snapshot = client.monitor_snapshot()?;
    let candidate = operation_candidate(&snapshot, OperationFamily::LayerStackInitialize)?;
    let storage = store.storage_snapshot()?;
    let canonical = store.canonical_storage()?;
    if client.active_workspace_count()? != 0 || client.active_execution_count()? != 0 {
        return Err("init-only diagnostic created runtime state".into());
    }
    let store_database_bytes = storage.database_bytes;
    let store_growth_bytes = store_database_bytes.saturating_sub(store_baseline_bytes);
    let teardown_started = Instant::now();
    drop(client);
    drop(store);
    let teardown_ns = elapsed_ns(teardown_started);
    let layerstack_init_ns = nanos(t0, t1);
    let init_bytes_per_second = rate(fixture_manifest.logical_bytes, layerstack_init_ns);
    let init_files_per_second = rate(fixture_manifest.regular_files, layerstack_init_ns);
    println!(
        "{{\"schema\":\"{}\",\"scenario\":\"{}\",\"iteration\":{iteration},\"fixture_profile\":\"{}\",\"fixture_digest_profile\":\"{}\",\"edit_contract\":\"{}\",\"result_profile\":\"{}\",\"measurement_mode\":\"init-only-diagnostic\",\"nonterminal\":true,\"fixture_cache_profile\":\"{}\",\"setup_ns\":{setup_ns},\"layerstack_init_ns\":{layerstack_init_ns},\"teardown_ns\":{teardown_ns},\"init_bytes_per_second\":{init_bytes_per_second},\"init_files_per_second\":{init_files_per_second},\"regular_files\":{},\"data_directories\":{},\"logical_bytes\":{},\"empty_files\":{},\"tiny_files\":{},\"small_files\":{},\"medium_files\":{},\"anchor_files\":{},\"anchor_bytes\":{},\"file_mode\":{},\"directory_mode\":{},\"mtime_seconds\":{},\"mtime_nanoseconds\":{},\"fixture_digest\":\"{}\",\"scanned_files\":{},\"scanned_bytes\":{},\"candidate_objects\":{},\"candidate_bytes\":{},\"inserted_objects\":{},\"inserted_bytes\":{},\"reused_objects\":{},\"reused_bytes\":{},\"initialize_batch_inserted_objects\":{},\"initialize_batch_inserted_bytes\":{},\"initialize_final_inserted_objects\":{},\"initialize_final_inserted_bytes\":{},\"initialize_preexisting_reused_objects\":{},\"initialize_preexisting_reused_bytes\":{},\"initialize_admission_transactions\":{},\"initialize_max_transaction_objects\":{},\"initialize_max_transaction_bytes\":{},\"store_baseline_bytes\":{store_baseline_bytes},\"store_database_bytes\":{store_database_bytes},\"store_growth_bytes\":{store_growth_bytes},\"store_canonical_objects\":{},\"store_canonical_bytes\":{}}}",
        workload_source::NAMESPACE_SCHEMA,
        scenario.id,
        workload_source::NAMESPACE_FIXTURE_PROFILE,
        workload_source::NAMESPACE_DIGEST_PROFILE,
        workload_source::NAMESPACE_EDIT_CONTRACT,
        workload_source::NAMESPACE_INIT_DIAGNOSTIC_PROFILE,
        fixture_cache_profile,
        fixture_manifest.regular_files,
        fixture_manifest.data_directories,
        fixture_manifest.logical_bytes,
        fixture_manifest.empty_files,
        fixture_manifest.tiny_files,
        fixture_manifest.small_files,
        fixture_manifest.medium_files,
        fixture_manifest.anchor_files,
        fixture_manifest.anchor_bytes,
        fixture_manifest.file_mode,
        fixture_manifest.directory_mode,
        fixture_manifest.mtime_seconds,
        fixture_manifest.mtime_nanoseconds,
        fixture_manifest.digest,
        scan.scanned_files,
        scan.scanned_bytes,
        candidate.candidate_objects,
        candidate.candidate_bytes,
        candidate.inserted_objects,
        candidate.inserted_bytes,
        candidate.reused_objects,
        candidate.reused_bytes,
        candidate.batch_inserted_objects,
        candidate.batch_inserted_bytes,
        candidate.final_inserted_objects,
        candidate.final_inserted_bytes,
        candidate.preexisting_reused_objects,
        candidate.preexisting_reused_bytes,
        candidate.admission_transactions,
        candidate.max_transaction_objects,
        candidate.max_transaction_bytes,
        canonical.objects,
        canonical.encoded_bytes,
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
