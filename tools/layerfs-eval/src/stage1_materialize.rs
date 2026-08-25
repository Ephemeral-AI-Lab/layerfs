use layerfs_sdk::{
    Diagnostics, ExternalWorkspace, IntegrityMode, LayerFs, NativeMetadata, NativeXattrs,
    OperationDiagnostics, ProjectionCallFacts, ProjectionCleanupFacts, ProjectionFacts,
    ProjectionReplaceFacts, ProjectionSyncFacts, ProjectionTimer, ProjectionTimerAvailability,
    ProjectionWriteFacts,
};
use std::ffi::OsStr;
use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const FILE_PATH: &str = "data/payload.bin";
const BUFFER_BYTES: usize = 1024 * 1024;
const FIXTURE_MODE: u32 = 0o644;
const FIXTURE_MTIME_SECONDS: u64 = 1_700_000_123;
const FIXTURE_MTIME_NANOSECONDS: u32 = 456_789_123;
const PRESERVED_24_MIB_DIGEST: &str =
    "89dcf8d2f5ce72728b9ef7c9e955de6299738140f35686015ec9bfef5f598ca5";

type EvalResult<T> = Result<T, String>;

pub fn prepare() -> EvalResult<()> {
    let destination = fixture_root();
    if destination.exists() {
        crate::stage1_fixture::verify_sealed(&destination)?;
        verify_fixture_sources(&destination)?;
        println!(
            "stage1m-materialize-prepare status=PASS reused=true fixture={}",
            destination.display()
        );
        return Ok(());
    }
    let parent = destination
        .parent()
        .ok_or_else(|| "fixture root has no parent".to_owned())?;
    fs::create_dir_all(parent).map_err(io_error)?;
    let temporary = parent.join(format!(
        ".full-materialization-v1-preparing-{}-{}",
        std::process::id(),
        unix_ns()?
    ));
    fs::create_dir(&temporary).map_err(io_error)?;
    match prepare_into(&temporary) {
        Ok(()) => {
            fs::rename(&temporary, &destination).map_err(io_error)?;
            crate::stage1_fixture::sync_directory(parent)?;
            println!(
                "stage1m-materialize-prepare status=PASS reused=false fixture={}",
                destination.display()
            );
            Ok(())
        }
        Err(error) => {
            let failure = parent.join(format!(
                "full-materialization-v1-preparation-failure-{}",
                unix_ns()?
            ));
            let _ = fs::rename(&temporary, &failure);
            Err(format!(
                "{error}; preparation evidence preserved at {}",
                failure.display()
            ))
        }
    }
}

fn fixture_root() -> PathBuf {
    crate::stage1_fixture::workspace_root()
        .join("target/layerfs-stage1m-fixtures/full-materialization-v1")
}

fn prepare_into(root: &Path) -> EvalResult<()> {
    let started = Instant::now();
    let apfs_identity = crate::stage1_fixture::assert_apfs(root)?;
    let mut entries = Vec::new();
    for size_mib in [0_u64, 24, 96] {
        entries.push(prepare_size(root, size_mib)?);
    }
    let inventory_digest =
        crate::stage1_fixture::tree_digest(root, Some(Path::new("fixture-manifest.json")))?;
    let manifest = format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1m-full-materialization-fixture-v1\",",
            "\"status\":\"PASS\",\"maximum_user_file_bytes\":100663296,",
            "\"buffer_bound_bytes\":1048576,\"apfs_identity\":\"{}\",",
            "\"inventory_blake3\":\"{}\",\"preparation_wall_ns\":{},",
            "\"sizes\":[{}]}}\n"
        ),
        json_escape(&apfs_identity),
        inventory_digest,
        started.elapsed().as_nanos(),
        entries.join(","),
    );
    durable_write(&root.join("fixture-manifest.json"), manifest.as_bytes())?;
    crate::stage1_fixture::seal_tree(root)?;
    crate::stage1_fixture::verify_sealed(root)?;
    verify_fixture_sources(root)
}

fn prepare_size(root: &Path, size_mib: u64) -> EvalResult<String> {
    let logical_bytes = size_mib
        .checked_mul(1024 * 1024)
        .ok_or_else(|| "fixture length overflow".to_owned())?;
    let size_root = root.join("sizes").join(size_mib.to_string());
    let source = size_root.join("source-native").join(FILE_PATH);
    fs::create_dir_all(
        source
            .parent()
            .ok_or_else(|| "source fixture has no parent".to_owned())?,
    )
    .map_err(io_error)?;
    let source_digest = generate_source(&source, logical_bytes)?;
    if size_mib == 24 && source_digest != PRESERVED_24_MIB_DIGEST {
        return Err(format!(
            "24 MiB generator mismatch: expected {PRESERVED_24_MIB_DIGEST}, got {source_digest}"
        ));
    }

    let store = size_root.join("bases/base");
    fs::create_dir_all(
        store
            .parent()
            .ok_or_else(|| "fixture Store has no parent".to_owned())?,
    )
    .map_err(io_error)?;
    let opened = LayerFs::open(&store).map_err(display_error)?;
    let capture_path = size_root.join("capture-source");
    let mut external = opened
        .fs
        .materialize_external(opened.head, &capture_path)
        .map_err(display_error)?;
    let native = external.path().join(FILE_PATH);
    fs::create_dir_all(
        native
            .parent()
            .ok_or_else(|| "capture file has no parent".to_owned())?,
    )
    .map_err(io_error)?;
    copy_file_bounded(&source, &native)?;
    set_fixture_metadata(&native)?;
    let root_id = external.capture_quiescent().map_err(display_error)?;
    let state = opened.fs.current_head("main").map_err(display_error)?;
    if state.root != root_id {
        return Err("fixture capture returned the wrong main root".to_owned());
    }
    let store_id = hex(&opened.fs.store_id().map_err(display_error)?);
    drop(external);
    fs::remove_dir_all(&capture_path).map_err(io_error)?;
    drop(opened);

    let reopened = LayerFs::open(&store).map_err(display_error)?;
    if reopened.ref_state != state
        || hex(&reopened.fs.store_id().map_err(display_error)?) != store_id
    {
        return Err("Verified fixture reopen changed authority".to_owned());
    }
    let mut sink = DigestWriter::default();
    let read = reopened
        .fs
        .read_to(root_id, FILE_PATH, &mut sink)
        .map_err(display_error)?;
    if sink.bytes != logical_bytes || sink.digest() != source_digest {
        return Err("Verified fixture read differs from source".to_owned());
    }
    if read.operation_q_terminal_bytes != 0 {
        return Err("fixture verification left nonzero operation Q".to_owned());
    }
    let diagnostics = reopened.fs.diagnostics().map_err(display_error)?;
    let store_bytes = tree_bytes(&store)?;
    drop(reopened);

    Ok(format!(
        concat!(
            "{{\"size_mib\":{},\"logical_bytes\":{},\"source_blake3\":\"{}\",",
            "\"root\":\"{}\",\"generation\":{},\"store_id\":\"{}\",",
            "\"store_bytes\":{},\"logical_engine_bytes\":{},",
            "\"payload_batch_maximum\":{},\"mode\":{},\"mtime_seconds\":{},",
            "\"mtime_nanoseconds\":{}}}"
        ),
        size_mib,
        logical_bytes,
        source_digest,
        root_id,
        state.generation,
        store_id,
        store_bytes,
        diagnostics.logical_engine_bytes.unwrap_or(0),
        diagnostics.payload_batch_maximum,
        FIXTURE_MODE,
        FIXTURE_MTIME_SECONDS,
        FIXTURE_MTIME_NANOSECONDS,
    ))
}

fn generate_source(path: &Path, bytes: u64) -> EvalResult<String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut written = 0_u64;
    while written < bytes {
        crate::stage1_fixture::fill_retained_buffer(&mut buffer, written);
        let take =
            usize::try_from((bytes - written).min(BUFFER_BYTES as u64)).map_err(display_error)?;
        file.write_all(&buffer[..take]).map_err(io_error)?;
        hasher.update(&buffer[..take]);
        written += take as u64;
    }
    file.sync_all().map_err(io_error)?;
    set_fixture_metadata(path)?;
    Ok(hasher.finalize().to_hex().to_string())
}

fn set_fixture_metadata(path: &Path) -> EvalResult<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(FIXTURE_MODE)).map_err(io_error)?;
    let modified = UNIX_EPOCH
        .checked_add(Duration::new(
            FIXTURE_MTIME_SECONDS,
            FIXTURE_MTIME_NANOSECONDS,
        ))
        .ok_or_else(|| "fixture mtime overflow".to_owned())?;
    File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_times(FileTimes::new().set_modified(modified)))
        .map_err(io_error)
}

fn copy_file_bounded(source: &Path, destination: &Path) -> EvalResult<()> {
    let mut input = File::open(source).map_err(io_error)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(io_error)?;
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    loop {
        let count = input.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        output.write_all(&buffer[..count]).map_err(io_error)?;
    }
    output.sync_all().map_err(io_error)
}

#[derive(Default)]
struct DigestWriter {
    hasher: blake3::Hasher,
    bytes: u64,
}

impl DigestWriter {
    fn digest(&self) -> String {
        self.hasher.clone().finalize().to_hex().to_string()
    }
}

impl Write for DigestWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.hasher.update(bytes);
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| std::io::Error::other("digest length overflow"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn tree_bytes(root: &Path) -> EvalResult<u64> {
    let mut total = 0_u64;
    let mut stack = vec![root.to_owned()];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(path).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let metadata = entry.metadata().map_err(io_error)?;
            if metadata.is_dir() {
                stack.push(entry.path());
            } else {
                total = total
                    .checked_add(metadata.len())
                    .ok_or_else(|| "fixture Store byte total overflow".to_owned())?;
            }
        }
    }
    Ok(total)
}

fn durable_write(path: &Path, bytes: &[u8]) -> EvalResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    crate::stage1_fixture::sync_directory(
        path.parent()
            .ok_or_else(|| "artifact has no parent".to_owned())?,
    )
}

fn verify_fixture_sources(root: &Path) -> EvalResult<()> {
    for size_mib in [0_u64, 24, 96] {
        let source = root
            .join("sizes")
            .join(size_mib.to_string())
            .join("source-native")
            .join(FILE_PATH);
        let expected = size_mib * 1024 * 1024;
        if fs::metadata(&source).map_err(io_error)?.len() != expected {
            return Err(format!("sealed {size_mib} MiB source length mismatch"));
        }
        if size_mib == 24 && digest_file(&source)? != PRESERVED_24_MIB_DIGEST {
            return Err("sealed 24 MiB source digest mismatch".to_owned());
        }
    }
    Ok(())
}

fn unix_ns() -> EvalResult<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .map_err(display_error)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

pub fn hash(path: &Path) -> EvalResult<()> {
    let output = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err("shasum failed".to_owned());
    }
    let sha256 = String::from_utf8(output.stdout)
        .map_err(display_error)?
        .split_whitespace()
        .next()
        .ok_or_else(|| "shasum returned no digest".to_owned())?
        .to_owned();
    println!(
        "{{\"path\":\"{}\",\"bytes\":{},\"sha256\":\"{}\",\"blake3\":\"{}\"}}",
        path.display(),
        fs::metadata(path).map_err(io_error)?.len(),
        sha256,
        digest_file(path)?,
    );
    Ok(())
}

pub fn manifest(
    role: &OsStr,
    commit: &OsStr,
    executable: &Path,
    build_target: &Path,
    build_log: &Path,
    output: &Path,
) -> EvalResult<()> {
    let role = ascii_argument(role, "role")?;
    let requested_commit = ascii_argument(commit, "commit")?;
    if output.exists() {
        return Err(format!(
            "refusing to replace source manifest {}",
            output.display()
        ));
    }
    let json =
        source_build_manifest_json(role, requested_commit, executable, build_target, build_log)?;
    durable_write(output, json.as_bytes())?;
    println!(
        "stage1-manifest status=PASS role={} commit={} output={}",
        role,
        resolve_commit(requested_commit)?,
        output.display()
    );
    Ok(())
}

fn source_build_manifest_json(
    role: &str,
    requested_commit: &str,
    executable: &Path,
    build_target: &Path,
    build_log: &Path,
) -> EvalResult<String> {
    let executable = executable.canonicalize().map_err(io_error)?;
    let running_executable = std::env::current_exe()
        .map_err(io_error)?
        .canonicalize()
        .map_err(io_error)?;
    let build_target = build_target.canonicalize().map_err(io_error)?;
    let expected_executable = build_target
        .join("release/layerfs-eval")
        .canonicalize()
        .map_err(io_error)?;
    if executable != running_executable || executable != expected_executable {
        return Err("manifest executable is not the running clean-build output".to_owned());
    }
    let (commit, workspace_root) = clean_head_custody()?;
    let resolved_commit = resolve_commit(requested_commit)?;
    if resolved_commit != commit {
        return Err(format!(
            "manifest commit {resolved_commit} is not current HEAD {commit}"
        ));
    }
    let build_log = build_log.canonicalize().map_err(io_error)?;
    let build_log_bytes = fs::read(&build_log).map_err(io_error)?;
    let build_log_text = String::from_utf8_lossy(&build_log_bytes);
    let build_command = format!(
        "CARGO_NET_OFFLINE=true CARGO_TARGET_DIR={} cargo build --release --locked -p layerfs-eval",
        build_target.display()
    );
    let required_build_log = [
        "schema=layerfs-build-log-v1".to_owned(),
        format!("source_head_before={commit}"),
        "source_status_before=clean".to_owned(),
        format!("build_command={build_command}"),
        "build_exit_code=0".to_owned(),
        format!("source_head_after={commit}"),
        "source_status_after=clean".to_owned(),
        "Finished `release` profile".to_owned(),
    ];
    if required_build_log
        .iter()
        .any(|required| !build_log_text.contains(required))
    {
        return Err("build log does not contain the exact successful release command".to_owned());
    }
    let listed = Command::new("git")
        .args(["ls-tree", "-r", "--name-only", "-z", &commit])
        .output()
        .map_err(io_error)?;
    if !listed.status.success() {
        return Err("git ls-tree failed".to_owned());
    }
    let mut paths = listed
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8(path.to_vec()).map_err(display_error))
        .collect::<EvalResult<Vec<_>>>()?;
    paths.retain(|path| {
        path.ends_with(".rs")
            || path == "Cargo.toml"
            || path == "Cargo.lock"
            || path.ends_with("/Cargo.toml")
    });
    paths.sort();
    let mut entries = Vec::with_capacity(paths.len());
    let mut aggregate = Vec::new();
    let mut product_aggregate = Vec::new();
    let mut product_files = 0_u64;
    for path in paths {
        let shown = Command::new("git")
            .args(["show", &format!("{commit}:{path}")])
            .output()
            .map_err(io_error)?;
        if !shown.status.success() {
            return Err(format!("git show failed for {path}"));
        }
        let sha256 = sha256_bytes(&shown.stdout)?;
        let blake3 = blake3::hash(&shown.stdout).to_hex().to_string();
        append_manifest_line(&mut aggregate, &path, shown.stdout.len(), &sha256, &blake3);
        let product = is_product_source(&path);
        if product {
            product_files = product_files
                .checked_add(1)
                .ok_or_else(|| "product source count overflow".to_owned())?;
            append_manifest_line(
                &mut product_aggregate,
                &path,
                shown.stdout.len(),
                &sha256,
                &blake3,
            );
        }
        entries.push(format!(
            "{{\"path\":\"{}\",\"bytes\":{},\"sha256\":\"{}\",\"blake3\":\"{}\",\"product\":{}}}",
            json_escape(&path),
            shown.stdout.len(),
            sha256,
            blake3,
            product,
        ));
    }
    let executable_sha256 = sha256_file(&executable)?;
    let executable_blake3 = digest_file(&executable)?;
    let build_log_sha256 = sha256_bytes(&build_log_bytes)?;
    let build_log_blake3 = blake3::hash(&build_log_bytes).to_hex().to_string();
    let rustc = command_version("rustc")?;
    let cargo = command_version("cargo")?;
    let json = format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1-source-build-manifest-v2\",",
            "\"status\":\"PASS\",\"role\":\"{}\",\"commit\":\"{}\",",
            "\"head_matches_commit\":true,\"dirty_tree\":false,",
            "\"workspace_root\":\"{}\",\"file_count\":{},",
            "\"aggregate_sha256\":\"{}\",\"aggregate_blake3\":\"{}\",",
            "\"product_file_count\":{},\"product_aggregate_sha256\":\"{}\",",
            "\"product_aggregate_blake3\":\"{}\",",
            "\"executable_path\":\"{}\",\"executable_sha256\":\"{}\",",
            "\"executable_blake3\":\"{}\",",
            "\"build_target\":\"{}\",\"build_command\":\"{}\",",
            "\"build_log_path\":\"{}\",\"build_log_bytes\":{},",
            "\"build_log_sha256\":\"{}\",\"build_log_blake3\":\"{}\",",
            "\"deterministic_build_claim\":false,",
            "\"executable\":{{\"path\":\"{}\",\"bytes\":{},",
            "\"sha256\":\"{}\",\"blake3\":\"{}\"}},",
            "\"build\":{{\"cwd\":\"{}\",",
            "\"environment\":{{\"CARGO_NET_OFFLINE\":\"true\",",
            "\"CARGO_TARGET_DIR\":\"{}\"}},",
            "\"argv\":[\"cargo\",\"build\",\"--release\",\"--locked\",",
            "\"-p\",\"layerfs-eval\"],\"log_sha256\":\"{}\"}},",
            "\"rustc\":\"{}\",\"cargo\":\"{}\",\"files\":[{}]}}\n"
        ),
        json_escape(role),
        json_escape(&commit),
        json_escape(&workspace_root.display().to_string()),
        entries.len(),
        sha256_bytes(&aggregate)?,
        blake3::hash(&aggregate).to_hex(),
        product_files,
        sha256_bytes(&product_aggregate)?,
        blake3::hash(&product_aggregate).to_hex(),
        json_escape(&executable.display().to_string()),
        executable_sha256,
        executable_blake3,
        json_escape(&build_target.display().to_string()),
        json_escape(&build_command),
        json_escape(&build_log.display().to_string()),
        build_log_bytes.len(),
        build_log_sha256,
        build_log_blake3,
        json_escape(&executable.display().to_string()),
        fs::metadata(&executable).map_err(io_error)?.len(),
        executable_sha256,
        executable_blake3,
        json_escape(&workspace_root.display().to_string()),
        json_escape(&build_target.display().to_string()),
        build_log_sha256,
        json_escape(&rustc),
        json_escape(&cargo),
        entries.join(","),
    );
    Ok(json)
}

fn git_stdout(arguments: &[&str]) -> EvalResult<String> {
    let output = Command::new("git")
        .args(arguments)
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err(format!("git {} failed", arguments.join(" ")));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(display_error)
}

fn resolve_commit(commit: &str) -> EvalResult<String> {
    git_stdout(&["rev-parse", "--verify", &format!("{commit}^{{commit}}")])
}

fn clean_head_custody() -> EvalResult<(String, PathBuf)> {
    let workspace_root = PathBuf::from(git_stdout(&["rev-parse", "--show-toplevel"])?);
    let current = std::env::current_dir()
        .map_err(io_error)?
        .canonicalize()
        .map_err(io_error)?;
    let workspace_root = workspace_root.canonicalize().map_err(io_error)?;
    if current != workspace_root {
        return Err("source/build custody must run at the clean workspace root".to_owned());
    }
    let status = git_stdout(&[
        "status",
        "--porcelain=v1",
        "--untracked-files=all",
        "--ignore-submodules=none",
    ])?;
    if !status.is_empty() {
        return Err("source/build custody requires a completely clean worktree".to_owned());
    }
    Ok((resolve_commit("HEAD")?, workspace_root))
}

pub fn parity_readiness(
    historical: &Path,
    instrumented: &Path,
    store: &Path,
    source: &Path,
    receipt: &Path,
) -> EvalResult<()> {
    if receipt.exists() {
        return Err(format!(
            "refusing to replace readiness {}",
            receipt.display()
        ));
    }
    let historical = historical.canonicalize().map_err(io_error)?;
    let instrumented = instrumented.canonicalize().map_err(io_error)?;
    let store = store.canonicalize().map_err(io_error)?;
    let source = source.canonicalize().map_err(io_error)?;
    if fs::metadata(&source).map_err(io_error)?.len() != 24 * 1024 * 1024 {
        return Err("parity source is not exactly 24 MiB".to_owned());
    }
    let source_digest = digest_file(&source)?;
    let historical_sha256 = sha256_file(&historical)?;
    let instrumented_sha256 = sha256_file(&instrumented)?;
    let parent = receipt
        .parent()
        .ok_or_else(|| "readiness receipt has no parent".to_owned())?;
    fs::create_dir_all(parent).map_err(io_error)?;
    let reset = parent.join(format!(
        ".stage1m-parity-readiness-reset-{}-{}",
        std::process::id(),
        unix_ns()?
    ));
    let started = Instant::now();
    let output = Command::new(&historical)
        .args(["stage1", "materialize", "parity-row"])
        .arg(&store)
        .arg(&source)
        .arg("24")
        .arg(&reset)
        .arg("readiness-historical")
        .output()
        .map_err(io_error)?;
    let reset_wall_ns = started.elapsed().as_nanos();
    if !output.status.success() {
        return Err(format!(
            "historical readiness reset failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(display_error)?;
    let rows = stdout.lines().collect::<Vec<_>>();
    if rows.len() != 2
        || !rows[0].contains("\"row_kind\":\"warmup\"")
        || !rows[1].contains("\"row_kind\":\"measured\"")
        || rows.iter().any(|row| !row.contains("\"status\":\"PASS\""))
    {
        return Err("historical readiness reset returned invalid rows".to_owned());
    }
    let forecast_wall_ns = reset_wall_ns
        .checked_mul(8)
        .ok_or_else(|| "parity forecast overflow".to_owned())?;
    if forecast_wall_ns >= 10_000_000_000 {
        return Err(format!(
            "parity forecast {forecast_wall_ns}ns reaches the 10s hard wall"
        ));
    }
    let schedule = parity_schedule_json();
    let json = format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1m-parity-readiness-v1\",",
            "\"status\":\"PASS\",\"measured_rows_started\":false,",
            "\"historical_path\":\"{}\",\"historical_sha256\":\"{}\",",
            "\"instrumented_path\":\"{}\",\"instrumented_sha256\":\"{}\",",
            "\"store\":\"{}\",\"source\":\"{}\",\"source_blake3\":\"{}\",",
            "\"schedule_blake3\":\"{}\",\"reset_wall_ns\":{},",
            "\"forecast_wall_ns\":{},\"hard_wall_ns\":10000000000,",
            "\"expected_warmups\":8,\"expected_measured\":8}}\n"
        ),
        json_escape(&historical.display().to_string()),
        historical_sha256,
        json_escape(&instrumented.display().to_string()),
        instrumented_sha256,
        json_escape(&store.display().to_string()),
        json_escape(&source.display().to_string()),
        source_digest,
        blake3::hash(schedule.as_bytes()).to_hex(),
        reset_wall_ns,
        forecast_wall_ns,
    );
    durable_write(receipt, json.as_bytes())?;
    println!(
        "stage1m-parity-readiness status=PASS receipt={} reset_wall_ns={} forecast_wall_ns={}",
        receipt.display(),
        reset_wall_ns,
        forecast_wall_ns
    );
    Ok(())
}

fn parity_schedule_json() -> String {
    "{\"schema\":\"layerfs-stage1m-parity-schedule-v1\",\"size_mib\":24,\"pairs\":[[\"H\",\"I\"],[\"I\",\"H\"],[\"I\",\"H\"],[\"H\",\"I\"]],\"warmups\":8,\"measured\":8}\n".to_owned()
}

pub fn parity_run(
    historical: &Path,
    instrumented: &Path,
    store: &Path,
    source: &Path,
    readiness: &Path,
    run: &Path,
) -> EvalResult<()> {
    if run.exists() {
        return Err(format!("run directory already exists: {}", run.display()));
    }
    let historical = historical.canonicalize().map_err(io_error)?;
    let instrumented = instrumented.canonicalize().map_err(io_error)?;
    let store = store.canonicalize().map_err(io_error)?;
    let source = source.canonicalize().map_err(io_error)?;
    let readiness_bytes = fs::read(readiness).map_err(io_error)?;
    let readiness_text = std::str::from_utf8(&readiness_bytes).map_err(display_error)?;
    let historical_sha256 = sha256_file(&historical)?;
    let instrumented_sha256 = sha256_file(&instrumented)?;
    let source_digest = digest_file(&source)?;
    for binding in [
        historical_sha256.as_str(),
        instrumented_sha256.as_str(),
        source_digest.as_str(),
        "\"status\":\"PASS\"",
        "\"measured_rows_started\":false",
    ] {
        if !readiness_text.contains(binding) {
            return Err(format!("readiness does not bind {binding}"));
        }
    }
    let run_parent = run
        .parent()
        .ok_or_else(|| "run directory has no parent".to_owned())?;
    fs::create_dir_all(run_parent).map_err(io_error)?;
    fs::create_dir(run).map_err(io_error)?;
    let campaign_started = Instant::now();
    let schedule = parity_schedule_json();
    durable_write(&run.join("schedule.json"), schedule.as_bytes())?;
    let preregistration = concat!(
        "{\"schema\":\"layerfs-stage1m-parity-preregistration-v1\",",
        "\"status\":\"PASS\",\"sizes_mib\":[24],\"pairs\":4,",
        "\"warmups\":8,\"measured\":8,\"p50\":\"mean_positions_2_3\",",
        "\"p95\":\"position_4\",\"preferred_wall_ns\":5000000000,",
        "\"hard_wall_ns\":10000000000}\n"
    );
    durable_write(
        &run.join("preregistration.json"),
        preregistration.as_bytes(),
    )?;
    durable_write(&run.join("readiness.json"), &readiness_bytes)?;
    let fixture_manifest = find_fixture_manifest(&source)?;
    durable_write(
        &run.join("fixture-manifest.json"),
        &fs::read(fixture_manifest).map_err(io_error)?,
    )?;
    durable_write(
        &run.join("environment.json"),
        format!(
            "{{\"schema\":\"layerfs-stage1m-environment-v1\",\"network\":0,\"rows_serial\":true,\"cwd\":\"{}\"}}\n",
            json_escape(
                &std::env::current_dir()
                    .map_err(io_error)?
                    .display()
                    .to_string()
            )
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("executables.json"),
        format!(
            concat!(
                "{{\"historical_harness\":{{\"path\":\"{}\",\"sha256\":\"{}\",",
                "\"blake3\":\"{}\"}},\"instrumented_control\":{{\"path\":\"{}\",",
                "\"sha256\":\"{}\",\"blake3\":\"{}\"}}}}\n"
            ),
            json_escape(&historical.display().to_string()),
            historical_sha256,
            digest_file(&historical)?,
            json_escape(&instrumented.display().to_string()),
            instrumented_sha256,
            digest_file(&instrumented)?,
        )
        .as_bytes(),
    )?;
    copy_global_manifests(run)?;
    create_empty(&run.join("rows.jsonl"))?;
    create_empty(&run.join("commands.json"))?;
    append_sync(
        &run.join("failure-ledger.json"),
        "{\"sequence\":1,\"state\":\"OPEN\",\"preserved_failures\":0}",
    )?;

    let orders = [["H", "I"], ["I", "H"], ["I", "H"], ["H", "I"]];
    let mut historical_walls = Vec::new();
    let mut instrumented_walls = Vec::new();
    let mut command_sequence = 0_u64;
    for (pair_index, pair) in orders.iter().enumerate() {
        let pair_number = pair_index + 1;
        let mut pair_comparable = Vec::new();
        for (order_index, role) in pair.iter().enumerate() {
            command_sequence += 1;
            let order_number = order_index + 1;
            let executable = if *role == "H" {
                &historical
            } else {
                &instrumented
            };
            let executable_sha256 = if *role == "H" {
                &historical_sha256
            } else {
                &instrumented_sha256
            };
            let identity = format!("{role}-p{pair_number}-o{order_number}");
            let work = run.join(format!(".sample-{identity}"));
            let started = Instant::now();
            let output = Command::new(executable)
                .args(["stage1", "materialize", "parity-row"])
                .arg(&store)
                .arg(&source)
                .arg("24")
                .arg(&work)
                .arg(&identity)
                .output()
                .map_err(io_error)?;
            let command_wall_ns = started.elapsed().as_nanos();
            append_command(
                run,
                command_sequence,
                pair_number,
                order_number,
                role,
                executable_sha256,
                command_wall_ns,
                &output,
            )?;
            if !output.status.success() {
                append_sync(
                    &run.join("failure-ledger.json"),
                    &format!(
                        "{{\"sequence\":2,\"state\":\"FAIL\",\"pair\":{},\"order\":{},\"operand\":\"{}\"}}",
                        pair_number, order_number, role
                    ),
                )?;
                return Err(format!("parity operand {identity} failed"));
            }
            let stdout = String::from_utf8(output.stdout).map_err(display_error)?;
            let child_rows = stdout.lines().collect::<Vec<_>>();
            if child_rows.len() != 2 {
                return Err(format!(
                    "parity operand {identity} returned {} rows",
                    child_rows.len()
                ));
            }
            for child_row in &child_rows {
                append_sync(
                    &run.join("rows.jsonl"),
                    &enrich_row(
                        child_row,
                        pair_number,
                        order_number,
                        role,
                        executable_sha256,
                        command_wall_ns,
                    )?,
                )?;
            }
            let measured = child_rows[1];
            let wall = json_u128(measured, "product_operation_wall_ns")?;
            if *role == "H" {
                historical_walls.push(wall);
            } else {
                instrumented_walls.push(wall);
                validate_instrumented_row(measured)?;
            }
            pair_comparable.push(comparable_row(measured)?);
        }
        if pair_comparable[0] != pair_comparable[1] {
            return Err(format!("pair {pair_number} legacy work differs"));
        }
    }
    finish_parity(
        run,
        campaign_started.elapsed().as_nanos(),
        historical_walls,
        instrumented_walls,
    )
}

#[allow(clippy::too_many_arguments)]
fn append_command(
    run: &Path,
    sequence: u64,
    pair: usize,
    order: usize,
    operand: &str,
    executable_sha256: &str,
    wall_ns: u128,
    output: &std::process::Output,
) -> EvalResult<()> {
    append_sync(
        &run.join("commands.json"),
        &format!(
            concat!(
                "{{\"sequence\":{},\"pair\":{},\"order\":{},\"operand\":\"{}\",",
                "\"executable_sha256\":\"{}\",\"wall_ns\":{},\"status\":{},",
                "\"stderr\":\"{}\"}}"
            ),
            sequence,
            pair,
            order,
            operand,
            executable_sha256,
            wall_ns,
            output.status.code().unwrap_or(-1),
            json_escape(&String::from_utf8_lossy(&output.stderr)),
        ),
    )
}

fn finish_parity(
    run: &Path,
    campaign_wall_ns: u128,
    historical_walls: Vec<u128>,
    instrumented_walls: Vec<u128>,
) -> EvalResult<()> {
    let historical = four_stats(&historical_walls)?;
    let instrumented = four_stats(&instrumented_walls)?;
    let p50_allowance = 1_000_000_u128.max(historical.0 * 3 / 100);
    let p50_pass = instrumented.0 <= historical.0 + p50_allowance;
    let p95_pass = instrumented.1 <= historical.1 + 1_000_000;
    let wall_pass = campaign_wall_ns < 10_000_000_000;
    let status = if p50_pass && p95_pass && wall_pass {
        "PASS"
    } else {
        "REVISE"
    };
    let summary = format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1m-parity-summary-v1\",\"status\":\"{}\",",
            "\"warmup_rows\":8,\"measured_rows\":8,\"legacy_work_exact\":true,",
            "\"historical\":{{\"raw_ns\":{:?},\"p50_ns\":{},\"p95_ns\":{}}},",
            "\"instrumented\":{{\"raw_ns\":{:?},\"p50_ns\":{},\"p95_ns\":{}}},",
            "\"p50_allowance_ns\":{},\"p50_pass\":{},\"p95_pass\":{},",
            "\"campaign_wall_ns\":{},\"hard_wall_pass\":{}}}\n"
        ),
        status,
        historical_walls,
        historical.0,
        historical.1,
        instrumented_walls,
        instrumented.0,
        instrumented.1,
        p50_allowance,
        p50_pass,
        p95_pass,
        campaign_wall_ns,
        wall_pass,
    );
    durable_write(&run.join("summary.json"), summary.as_bytes())?;
    durable_write(
        &run.join("summary.md"),
        format!(
            "# Stage 1.1M parity\n\nStatus: **{status}**\n\nHistorical p50/p95: `{}/{}` ns. Instrumented p50/p95: `{}/{}` ns. Complete wall: `{campaign_wall_ns}` ns. All 16 rows and exact legacy work retained.\n",
            historical.0, historical.1, instrumented.0, instrumented.1,
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("campaign-time.txt"),
        format!(
            "schema=layerfs-stage1m-parity-campaign-time-v1\nstatus={status}\ncampaign_wall_ns={campaign_wall_ns}\nwarmups=8\nmeasured=8\nhard_wall_ns=10000000000\n"
        )
        .as_bytes(),
    )?;
    append_sync(
        &run.join("failure-ledger.json"),
        &format!(
            "{{\"sequence\":2,\"state\":\"CLOSE\",\"status\":\"{}\",\"preserved_failures\":0}}",
            status
        ),
    )?;
    println!(
        "stage1m-parity-run status={} run={} wall_ns={} p50_pass={} p95_pass={}",
        status,
        run.display(),
        campaign_wall_ns,
        p50_pass,
        p95_pass
    );
    if status == "PASS" {
        Ok(())
    } else {
        Err("instrumented parity requires repair".to_owned())
    }
}

fn find_fixture_manifest(source: &Path) -> EvalResult<PathBuf> {
    source
        .ancestors()
        .map(|ancestor| ancestor.join("fixture-manifest.json"))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| "fixture-manifest.json not found above source".to_owned())
}

fn copy_global_manifests(run: &Path) -> EvalResult<()> {
    let source = crate::stage1_fixture::workspace_root()
        .join("target/layerfs-stage1m-custody/source-manifests");
    for name in [
        "source-manifest-historical.json",
        "source-manifest-historical-harness.json",
        "source-manifest-control.json",
    ] {
        durable_write(
            &run.join(name),
            &fs::read(source.join(name)).map_err(io_error)?,
        )?;
    }
    durable_write(
        &run.join("source-manifest-candidate.json"),
        b"{\"status\":\"NotApplicable\",\"reason\":\"candidate does not exist during M1 parity\"}\n",
    )
}

fn create_empty(path: &Path) -> EvalResult<()> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(io_error)
}

fn append_sync(path: &Path, line: &str) -> EvalResult<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(line.as_bytes()).map_err(io_error)?;
    file.write_all(b"\n").map_err(io_error)?;
    file.sync_all().map_err(io_error)
}

fn enrich_row(
    row: &str,
    pair: usize,
    order: usize,
    operand: &str,
    executable_sha256: &str,
    command_wall_ns: u128,
) -> EvalResult<String> {
    let body = row
        .strip_suffix('}')
        .ok_or_else(|| "child row is not a JSON object".to_owned())?;
    Ok(format!(
        "{body},\"pair\":{pair},\"order\":{order},\"operand\":\"{operand}\",\"executable_sha256\":\"{executable_sha256}\",\"command_wall_ns\":{command_wall_ns}}}"
    ))
}

#[derive(Eq, PartialEq)]
struct ComparableRow(Vec<u128>);

fn comparable_row(row: &str) -> EvalResult<ComparableRow> {
    [
        "logical_bytes",
        "statements",
        "fetched_rows",
        "authentication_passes",
        "role_decode_passes",
        "object_bytes_read",
        "payload_batch_queries",
        "payload_batch_references",
        "payload_batch_maximum",
        "publication_commits",
        "tables",
        "rows",
        "high_water_bytes",
        "bytes_written",
        "temp_calls",
        "sync_calls",
        "replace_calls",
        "metadata_calls",
        "operation_q_terminal_bytes",
        "residue",
    ]
    .into_iter()
    .map(|key| json_u128(row, key))
    .collect::<EvalResult<Vec<_>>>()
    .map(ComparableRow)
}

fn validate_instrumented_row(row: &str) -> EvalResult<()> {
    for truth in [
        "\"engine_sql_exact\":true",
        "\"scratch_sql_exact\":true",
        "\"fetched_auth_decode_exact\":true",
    ] {
        if !row.contains(truth) {
            return Err(format!("instrumented row does not prove {truth}"));
        }
    }
    let wall = json_u128(row, "product_operation_wall_ns")?;
    let residual = json_i128(row, "operation_residual_ns")?.unsigned_abs();
    if residual > 500_000_u128.max(wall / 100) {
        return Err(format!(
            "instrumented operation residual {residual} exceeds tolerance"
        ));
    }
    Ok(())
}

fn four_stats(values: &[u128]) -> EvalResult<(u128, u128)> {
    if values.len() != 4 {
        return Err("n=4 statistic requires four values".to_owned());
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    Ok(((sorted[1] + sorted[2]) / 2, sorted[3]))
}

fn json_u128(json: &str, key: &str) -> EvalResult<u128> {
    json_number_text(json, key)?.parse().map_err(display_error)
}

fn json_i128(json: &str, key: &str) -> EvalResult<i128> {
    json_number_text(json, key)?.parse().map_err(display_error)
}

fn json_string_value(json: &str, key: &str) -> EvalResult<String> {
    let needle = format!("\"{key}\":\"");
    let rest = json
        .find(&needle)
        .and_then(|offset| json.get(offset + needle.len()..))
        .ok_or_else(|| format!("missing JSON string {key}"))?;
    let end = rest
        .find('"')
        .ok_or_else(|| format!("unterminated JSON string {key}"))?;
    rest.get(..end)
        .map(str::to_owned)
        .ok_or_else(|| format!("invalid JSON string {key}"))
}

fn json_number_text<'a>(json: &'a str, key: &str) -> EvalResult<&'a str> {
    let needle = format!("\"{key}\":");
    let rest = json
        .find(&needle)
        .and_then(|offset| json.get(offset + needle.len()..))
        .ok_or_else(|| format!("missing JSON number {key}"))?;
    let end = rest
        .find(|character: char| !character.is_ascii_digit() && character != '-')
        .unwrap_or(rest.len());
    rest.get(..end)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("invalid JSON number {key}"))
}

fn ascii_argument<'a>(value: &'a OsStr, name: &str) -> EvalResult<&'a str> {
    value
        .to_str()
        .filter(|value| {
            !value.is_empty()
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        })
        .ok_or_else(|| format!("{name} must be a nonempty ASCII identifier"))
}

fn append_manifest_line(
    output: &mut Vec<u8>,
    path: &str,
    bytes: usize,
    sha256: &str,
    blake3: &str,
) {
    output.extend_from_slice(path.as_bytes());
    output.push(0);
    output.extend_from_slice(bytes.to_string().as_bytes());
    output.push(0);
    output.extend_from_slice(sha256.as_bytes());
    output.push(0);
    output.extend_from_slice(blake3.as_bytes());
    output.push(b'\n');
}

fn is_product_source(path: &str) -> bool {
    path == "Cargo.toml"
        || path == "Cargo.lock"
        || [
            "crates/layerfs-core/",
            "crates/layerfs-engine/",
            "crates/layerfs-vfs/",
            "crates/layerfs-os/",
            "crates/layerfs-sdk/",
        ]
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

fn sha256_bytes(bytes: &[u8]) -> EvalResult<String> {
    let mut child = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(io_error)?;
    child
        .stdin
        .take()
        .ok_or_else(|| "shasum stdin unavailable".to_owned())?
        .write_all(bytes)
        .map_err(io_error)?;
    let output = child.wait_with_output().map_err(io_error)?;
    if !output.status.success() {
        return Err("shasum failed".to_owned());
    }
    String::from_utf8(output.stdout)
        .map_err(display_error)?
        .split_whitespace()
        .next()
        .map(str::to_owned)
        .ok_or_else(|| "shasum returned no digest".to_owned())
}

fn sha256_file(path: &Path) -> EvalResult<String> {
    let output = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err("shasum failed".to_owned());
    }
    String::from_utf8(output.stdout)
        .map_err(display_error)?
        .split_whitespace()
        .next()
        .map(str::to_owned)
        .ok_or_else(|| "shasum returned no digest".to_owned())
}

fn command_version(command: &str) -> EvalResult<String> {
    let output = Command::new(command)
        .arg("--version")
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err(format!("{command} --version failed"));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(display_error)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttributionArm {
    Complete,
    Null,
    Digest,
    Native,
}

impl AttributionArm {
    fn parse(value: &OsStr) -> EvalResult<Self> {
        match value.to_str() {
            Some("complete") => Ok(Self::Complete),
            Some("null") => Ok(Self::Null),
            Some("digest") => Ok(Self::Digest),
            Some("native") => Ok(Self::Native),
            _ => Err("arm must be complete, null, digest, or native".to_owned()),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Null => "null",
            Self::Digest => "digest",
            Self::Native => "native",
        }
    }

    const fn operation_label(self) -> &'static str {
        match self {
            Self::Complete => "same_open_warmed_source_fresh_destination",
            Self::Null => "warm_authenticated_null_sink",
            Self::Digest => "warm_authenticated_digest",
            Self::Native => "native_durable_output",
        }
    }
}

const ATTRIBUTION_SCHEDULE: [(AttributionArm, u64); 12] = [
    (AttributionArm::Complete, 24),
    (AttributionArm::Null, 0),
    (AttributionArm::Digest, 96),
    (AttributionArm::Native, 24),
    (AttributionArm::Null, 96),
    (AttributionArm::Digest, 24),
    (AttributionArm::Native, 0),
    (AttributionArm::Complete, 96),
    (AttributionArm::Digest, 0),
    (AttributionArm::Native, 96),
    (AttributionArm::Complete, 0),
    (AttributionArm::Null, 24),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AcceptanceBlock {
    pair: u64,
    size_mib: u64,
    order: [char; 2],
}

const fn acceptance_block(pair: u64, size_mib: u64, order: [char; 2]) -> AcceptanceBlock {
    AcceptanceBlock {
        pair,
        size_mib,
        order,
    }
}

const ACCEPTANCE_SCHEDULE: [AcceptanceBlock; 12] = [
    acceptance_block(1, 0, ['A', 'B']),
    acceptance_block(1, 24, ['A', 'B']),
    acceptance_block(1, 96, ['A', 'B']),
    acceptance_block(2, 96, ['B', 'A']),
    acceptance_block(2, 24, ['B', 'A']),
    acceptance_block(2, 0, ['B', 'A']),
    acceptance_block(3, 24, ['B', 'A']),
    acceptance_block(3, 0, ['B', 'A']),
    acceptance_block(3, 96, ['B', 'A']),
    acceptance_block(4, 0, ['A', 'B']),
    acceptance_block(4, 96, ['A', 'B']),
    acceptance_block(4, 24, ['A', 'B']),
];

fn acceptance_schedule_json() -> String {
    let blocks = ACCEPTANCE_SCHEDULE
        .iter()
        .enumerate()
        .map(|(index, block)| {
            format!(
                "{{\"block\":{},\"pair\":{},\"size_mib\":{},\"order\":[\"{}\",\"{}\"]}}",
                index + 1,
                block.pair,
                block.size_mib,
                block.order[0],
                block.order[1],
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"schema\":\"layerfs-stage1m-acceptance-schedule-v1\",\"blocks\":[{blocks}],\"paired_warmups\":24,\"measured\":24,\"rows\":48}}\n"
    )
}

fn attribution_schedule_json() -> String {
    let blocks = ATTRIBUTION_SCHEDULE
        .iter()
        .enumerate()
        .map(|(index, (arm, size))| {
            format!(
                "{{\"block\":{},\"arm\":\"{}\",\"size_mib\":{},\"warmups\":1,\"measured\":3}}",
                index + 1,
                arm.name(),
                size
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"schema\":\"layerfs-stage1m-attribution-schedule-v1\",\"blocks\":[{blocks}],\"warmups\":12,\"measured\":36,\"rows\":48}}\n"
    )
}

pub fn attribution_run(control: &Path, fixture: &Path, run: &Path) -> EvalResult<()> {
    if run.exists() {
        return Err(format!("run directory already exists: {}", run.display()));
    }
    let control = control.canonicalize().map_err(io_error)?;
    let fixture = fixture.canonicalize().map_err(io_error)?;
    crate::stage1_fixture::verify_sealed(&fixture)?;
    verify_fixture_sources(&fixture)?;
    let fixture_manifest = fs::read(fixture.join("fixture-manifest.json")).map_err(io_error)?;
    let control_sha256 = sha256_file(&control)?;
    let control_blake3 = digest_file(&control)?;
    let schedule = attribution_schedule_json();
    let schedule_blake3 = blake3::hash(schedule.as_bytes()).to_hex().to_string();
    fs::create_dir(run).map_err(io_error)?;
    let campaign_started = Instant::now();
    durable_write(&run.join("schedule.json"), schedule.as_bytes())?;
    durable_write(
        &run.join("preregistration.json"),
        concat!(
            "{\"schema\":\"layerfs-stage1m-attribution-preregistration-v1\",",
            "\"status\":\"PASS\",\"warmups\":12,\"measured\":36,",
            "\"estimator\":\"n3_position2_position3\",",
            "\"preferred_wall_ns\":15000000000,\"hard_wall_ns\":30000000000}\n"
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("readiness.json"),
        format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1m-attribution-readiness-v1\",",
                "\"status\":\"PASS\",\"measured_rows_started\":false,",
                "\"control_sha256\":\"{}\",\"fixture_blake3\":\"{}\",",
                "\"schedule_blake3\":\"{}\",\"expected_rows\":48}}\n"
            ),
            control_sha256,
            blake3::hash(&fixture_manifest).to_hex(),
            schedule_blake3,
        )
        .as_bytes(),
    )?;
    durable_write(&run.join("fixture-manifest.json"), &fixture_manifest)?;
    durable_write(
        &run.join("environment.json"),
        format!(
            "{{\"schema\":\"layerfs-stage1m-environment-v1\",\"network\":0,\"rows_serial\":true,\"cwd\":\"{}\"}}\n",
            json_escape(&std::env::current_dir().map_err(io_error)?.display().to_string())
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("executables.json"),
        format!(
            "{{\"instrumented_control\":{{\"path\":\"{}\",\"sha256\":\"{}\",\"blake3\":\"{}\"}}}}\n",
            json_escape(&control.display().to_string()),
            control_sha256,
            control_blake3,
        )
        .as_bytes(),
    )?;
    copy_attribution_manifests(run, &fixture)?;
    create_empty(&run.join("rows.jsonl"))?;
    create_empty(&run.join("commands.jsonl"))?;
    append_sync(
        &run.join("failure-ledger.json"),
        "{\"sequence\":1,\"state\":\"OPEN\",\"preserved_failures\":0}",
    )?;

    let campaign_setup_ns = campaign_started.elapsed().as_nanos();
    let mut populations = Vec::new();
    let mut population_data = Vec::new();
    let mut command_wall_sum_ns = 0_u128;
    let mut row_sequence = 0_u64;
    let mut maximum_rss_peak_bytes = 0_u128;
    let mut maximum_q_high_water_bytes = 0_u128;
    let mut maximum_scratch_connections = 0_u128;
    let mut maximum_total_connections = 0_u128;
    let mut maximum_row_cpu_ns = 0_u128;
    for (block_index, (arm, size_mib)) in ATTRIBUTION_SCHEDULE.iter().enumerate() {
        if campaign_started.elapsed().as_nanos() >= 30_000_000_000 {
            return attribution_campaign_failure(run, block_index + 1, "hard wall reached");
        }
        let size = fixture.join("sizes").join(size_mib.to_string());
        let store = size.join("bases/base");
        let source = size.join("source-native").join(FILE_PATH);
        let identity = format!("b{:02}-{}-{}", block_index + 1, arm.name(), size_mib);
        let work = run.join(format!(".block-{identity}"));
        let size_mib_arg = size_mib.to_string();
        let argv = [
            control.display().to_string(),
            "stage1".to_owned(),
            "materialize".to_owned(),
            "attribution-block".to_owned(),
            store.display().to_string(),
            source.display().to_string(),
            size_mib_arg.clone(),
            arm.name().to_owned(),
            work.display().to_string(),
            identity.clone(),
        ];
        let argv_json = argv
            .iter()
            .map(|argument| format!("\"{}\"", json_escape(argument)))
            .collect::<Vec<_>>()
            .join(",");
        let command_start_unix_ns = unix_ns()?;
        let started = Instant::now();
        let output = Command::new(&control)
            .args(["stage1", "materialize", "attribution-block"])
            .arg(&store)
            .arg(&source)
            .arg(&size_mib_arg)
            .arg(arm.name())
            .arg(&work)
            .arg(&identity)
            .output()
            .map_err(io_error)?;
        let command_wall_ns = started.elapsed().as_nanos();
        command_wall_sum_ns = command_wall_sum_ns
            .checked_add(command_wall_ns)
            .ok_or_else(|| "command wall sum overflow".to_owned())?;
        let command_end_unix_ns = unix_ns()?;
        append_sync(
            &run.join("commands.jsonl"),
            &format!(
                concat!(
                    "{{\"sequence\":{},\"block\":{},\"arm\":\"{}\",",
                    "\"size_mib\":{},\"identity\":\"{}\",",
                    "\"executable\":\"{}\",\"executable_sha256\":\"{}\",",
                    "\"fixture_root\":\"{}\",\"store\":\"{}\",\"source\":\"{}\",",
                    "\"work\":\"{}\",\"cwd\":\"{}\",\"argv\":[{}],",
                    "\"start_unix_ns\":{},\"end_unix_ns\":{},\"wall_ns\":{},",
                    "\"exit_code\":{},\"stderr\":\"{}\"}}"
                ),
                block_index + 1,
                block_index + 1,
                arm.name(),
                size_mib,
                identity,
                json_escape(&control.display().to_string()),
                control_sha256,
                json_escape(&fixture.display().to_string()),
                json_escape(&store.display().to_string()),
                json_escape(&source.display().to_string()),
                json_escape(&work.display().to_string()),
                json_escape(
                    &std::env::current_dir()
                        .map_err(io_error)?
                        .display()
                        .to_string()
                ),
                argv_json,
                command_start_unix_ns,
                command_end_unix_ns,
                command_wall_ns,
                output.status.code().unwrap_or(-1),
                json_escape(&String::from_utf8_lossy(&output.stderr)),
            ),
        )?;
        if !output.status.success() {
            return attribution_campaign_failure(run, block_index + 1, "block command failed");
        }
        let stdout = String::from_utf8(output.stdout).map_err(display_error)?;
        let rows = stdout.lines().collect::<Vec<_>>();
        if rows.len() != 4
            || !rows[0].contains("\"row_kind\":\"warmup\"")
            || rows[1..]
                .iter()
                .any(|row| !row.contains("\"row_kind\":\"measured\""))
        {
            return attribution_campaign_failure(run, block_index + 1, "invalid block population");
        }
        let mut measured = Vec::new();
        for row in rows {
            validate_attribution_json(row)?;
            maximum_rss_peak_bytes = maximum_rss_peak_bytes.max(json_u128(row, "rss_peak_bytes")?);
            maximum_q_high_water_bytes =
                maximum_q_high_water_bytes.max(json_u128(row, "operation_q_high_water_bytes")?);
            maximum_scratch_connections =
                maximum_scratch_connections.max(json_u128(row, "scratch_connections_peak")?);
            maximum_total_connections =
                maximum_total_connections.max(json_u128(row, "total_connections_peak")?);
            maximum_row_cpu_ns = maximum_row_cpu_ns.max(
                json_u128(row, "user_cpu_ns")?
                    .checked_add(json_u128(row, "system_cpu_ns")?)
                    .ok_or_else(|| "row CPU total overflow".to_owned())?,
            );
            row_sequence += 1;
            append_sync(
                &run.join("rows.jsonl"),
                &enrich_attribution_row(
                    row,
                    row_sequence,
                    block_index + 1,
                    &control_sha256,
                    &schedule_blake3,
                    command_wall_ns,
                )?,
            )?;
            if row.contains("\"row_kind\":\"measured\"") {
                measured.push(json_u128(row, "product_operation_wall_ns")?);
            }
        }
        let stats = three_stats(&measured)?;
        population_data.push((*arm, *size_mib, stats.0, stats.1));
        populations.push(format!(
            "{{\"arm\":\"{}\",\"size_mib\":{},\"raw_ns\":{:?},\"p50_ns\":{},\"p95_ns\":{}}}",
            arm.name(),
            size_mib,
            measured,
            stats.0,
            stats.1
        ));
        if campaign_started.elapsed().as_nanos() >= 30_000_000_000 {
            return attribution_campaign_failure(run, block_index + 1, "hard wall reached");
        }
    }
    if row_sequence != 48 {
        return attribution_campaign_failure(run, 12, "row population is not 48");
    }
    let commands = fs::read_to_string(run.join("commands.jsonl")).map_err(io_error)?;
    let command_records = commands.lines().collect::<Vec<_>>();
    if command_records.len() != 12 {
        return attribution_campaign_failure(run, 12, "command population is not 12");
    }
    durable_write(
        &run.join("commands.json"),
        format!("[{}]\n", command_records.join(",")).as_bytes(),
    )?;
    let campaign_wall_ns = campaign_started.elapsed().as_nanos();
    let campaign_coordinator_ns = campaign_wall_ns
        .checked_sub(campaign_setup_ns)
        .and_then(|wall| wall.checked_sub(command_wall_sum_ns))
        .ok_or_else(|| "campaign wall equation underflow".to_owned())?;
    let models = attribution_models_json(&population_data)?;
    let rows_sha256 = sha256_file(&run.join("rows.jsonl"))?;
    let rows_blake3 = digest_file(&run.join("rows.jsonl"))?;
    let commands_sha256 = sha256_file(&run.join("commands.json"))?;
    let commands_blake3 = digest_file(&run.join("commands.json"))?;
    durable_write(
        &run.join("artifact-hashes.json"),
        format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1m-attribution-hashes-v1\",",
                "\"rows\":{{\"sha256\":\"{}\",\"blake3\":\"{}\"}},",
                "\"commands\":{{\"sha256\":\"{}\",\"blake3\":\"{}\"}},",
                "\"schedule_sha256\":\"{}\",\"fixture_manifest_sha256\":\"{}\",",
                "\"executable_sha256\":\"{}\"}}\n"
            ),
            rows_sha256,
            rows_blake3,
            commands_sha256,
            commands_blake3,
            sha256_file(&run.join("schedule.json"))?,
            sha256_file(&run.join("fixture-manifest.json"))?,
            control_sha256,
        )
        .as_bytes(),
    )?;
    let preferred_wall_pass = campaign_wall_ns < 15_000_000_000;
    durable_write(
        &run.join("summary.json"),
        format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1m-attribution-summary-v1\",",
                "\"status\":\"PASS\",\"warmup_rows\":12,\"measured_rows\":36,",
                "\"population_exact\":true,\"preferred_wall_pass\":{},",
                "\"hard_wall_pass\":true,\"campaign_wall_ns\":{},",
                "\"campaign_setup_ns\":{},\"command_wall_sum_ns\":{},",
                "\"campaign_coordinator_ns\":{},\"campaign_wall_equation_exact\":true,",
                "\"resources\":{{\"maximum_rss_peak_bytes\":{},",
                "\"maximum_q_high_water_bytes\":{},\"maximum_scratch_connections\":{},",
                "\"maximum_total_connections\":{},\"maximum_row_cpu_ns\":{},",
                "\"terminal_primary_connections\":0,\"terminal_scratch_connections\":0,",
                "\"terminal_total_connections\":0,\"terminal_q_bytes\":0,\"residue\":0}},",
                "\"models\":{},\"populations\":[{}]}}\n"
            ),
            preferred_wall_pass,
            campaign_wall_ns,
            campaign_setup_ns,
            command_wall_sum_ns,
            campaign_coordinator_ns,
            maximum_rss_peak_bytes,
            maximum_q_high_water_bytes,
            maximum_scratch_connections,
            maximum_total_connections,
            maximum_row_cpu_ns,
            models,
            populations.join(","),
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("summary.md"),
        format!(
            "# Stage 1.1M control attribution\n\nStatus: **PASS**. Exact population: 12 warmups + 36 measured rows. Complete wall: `{campaign_wall_ns}` ns; preferred wall pass: `{preferred_wall_pass}`.\n"
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("campaign-time.txt"),
        format!(
            "schema=layerfs-stage1m-attribution-campaign-time-v1\nstatus=PASS\ncampaign_wall_ns={campaign_wall_ns}\ncampaign_setup_ns={campaign_setup_ns}\ncommand_wall_sum_ns={command_wall_sum_ns}\ncampaign_coordinator_ns={campaign_coordinator_ns}\ncampaign_wall_equation_exact=true\nwarmups=12\nmeasured=36\npreferred_wall_ns=15000000000\nhard_wall_ns=30000000000\n"
        )
        .as_bytes(),
    )?;
    append_sync(
        &run.join("failure-ledger.json"),
        "{\"sequence\":2,\"state\":\"CLOSE\",\"status\":\"PASS\",\"preserved_failures\":0}",
    )?;
    durable_write(
        &run.join("terminal-receipt.json"),
        format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1m-attribution-terminal-receipt-v1\",",
                "\"status\":\"PASS\",\"rows_sha256\":\"{}\",",
                "\"commands_sha256\":\"{}\",\"summary_sha256\":\"{}\",",
                "\"campaign_time_sha256\":\"{}\",\"failure_ledger_sha256\":\"{}\",",
                "\"artifact_hashes_sha256\":\"{}\",\"executable_sha256\":\"{}\"}}\n"
            ),
            sha256_file(&run.join("rows.jsonl"))?,
            sha256_file(&run.join("commands.json"))?,
            sha256_file(&run.join("summary.json"))?,
            sha256_file(&run.join("campaign-time.txt"))?,
            sha256_file(&run.join("failure-ledger.json"))?,
            sha256_file(&run.join("artifact-hashes.json"))?,
            control_sha256,
        )
        .as_bytes(),
    )?;
    println!(
        "stage1m-attribution-run status=PASS run={} wall_ns={} preferred_wall_pass={}",
        run.display(),
        campaign_wall_ns,
        preferred_wall_pass,
    );
    Ok(())
}

const TRUSTED_SCHEDULE: [u64; 3] = [0, 24, 96];

fn trusted_schedule_json() -> String {
    let blocks = TRUSTED_SCHEDULE
        .iter()
        .enumerate()
        .map(|(index, size)| {
            format!(
                "{{\"block\":{},\"integrity_mode\":\"TrustedLocalDev\",\"arm\":\"complete\",\"size_mib\":{},\"warmups\":1,\"measured\":3}}",
                index + 1,
                size
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"schema\":\"layerfs-stage1t-schedule-v1\",\"blocks\":[{blocks}],\"warmups\":3,\"measured\":9,\"rows\":12}}\n"
    )
}

pub fn trusted_run(fixture: &Path, source_manifest: &Path, run: &Path) -> EvalResult<()> {
    if run.exists() {
        return Err(format!("run directory already exists: {}", run.display()));
    }
    let fixture = fixture.canonicalize().map_err(io_error)?;
    let source_manifest = source_manifest.canonicalize().map_err(io_error)?;
    crate::stage1_fixture::verify_sealed(&fixture)?;
    verify_fixture_sources(&fixture)?;
    let executable = std::env::current_exe()
        .map_err(io_error)?
        .canonicalize()
        .map_err(io_error)?;
    let executable_sha256 = sha256_file(&executable)?;
    let executable_blake3 = digest_file(&executable)?;
    let source_manifest_bytes = fs::read(&source_manifest).map_err(io_error)?;
    let source_manifest_text =
        std::str::from_utf8(&source_manifest_bytes).map_err(display_error)?;
    let (head, _) = clean_head_custody()?;
    let build_target = PathBuf::from(json_string_value(source_manifest_text, "build_target")?);
    let build_log = PathBuf::from(json_string_value(source_manifest_text, "build_log_path")?)
        .canonicalize()
        .map_err(io_error)?;
    let build_log_bytes = fs::read(&build_log).map_err(io_error)?;
    let expected_manifest = source_build_manifest_json(
        "stage1t-trusted-measurement-final",
        &head,
        &executable,
        &build_target,
        &build_log,
    )?;
    if source_manifest_bytes != expected_manifest.as_bytes() {
        return Err(
            "source manifest bytes do not exactly match clean HEAD, build log, and running executable"
                .to_owned()
        );
    }
    let fixture_manifest = fs::read(fixture.join("fixture-manifest.json")).map_err(io_error)?;
    let schedule = trusted_schedule_json();
    let schedule_blake3 = blake3::hash(schedule.as_bytes()).to_hex().to_string();

    fs::create_dir(run).map_err(io_error)?;
    let campaign_started = Instant::now();
    durable_write(&run.join("schedule.json"), schedule.as_bytes())?;
    durable_write(
        &run.join("preregistration.json"),
        b"{\"schema\":\"layerfs-stage1t-preregistration-v1\",\"status\":\"PASS\",\"integrity_mode\":\"TrustedLocalDev\",\"warmups\":3,\"measured\":9,\"preferred_wall_ns\":15000000000,\"hard_wall_ns\":30000000000}\n",
    )?;
    durable_write(&run.join("fixture-manifest.json"), &fixture_manifest)?;
    durable_write(&run.join("source-manifest.json"), &source_manifest_bytes)?;
    durable_write(&run.join("build.log"), &build_log_bytes)?;
    durable_write(
        &run.join("environment.json"),
        format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1t-environment-v1\",",
                "\"integrity_mode\":\"TrustedLocalDev\",\"network\":0,",
                "\"rows_serial\":true,\"cwd\":\"{}\",",
                "\"git_commit\":\"{}\",\"dirty_tree\":false,",
                "\"executable\":\"{}\",\"executable_sha256\":\"{}\",",
                "\"executable_blake3\":\"{}\",\"build_log_sha256\":\"{}\"}}\n"
            ),
            json_escape(
                &std::env::current_dir()
                    .map_err(io_error)?
                    .display()
                    .to_string()
            ),
            head,
            json_escape(&executable.display().to_string()),
            executable_sha256,
            executable_blake3,
            sha256_bytes(&build_log_bytes)?,
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("readiness.json"),
        format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1t-readiness-v1\",\"status\":\"PASS\",",
                "\"measured_rows_started\":false,\"integrity_mode\":\"TrustedLocalDev\",",
                "\"expected_rows\":12,\"warmups\":3,\"measured\":9,",
                "\"git_commit\":\"{}\",\"dirty_tree\":false,",
                "\"executable_sha256\":\"{}\",\"source_manifest_sha256\":\"{}\",",
                "\"build_log_sha256\":\"{}\",",
                "\"fixture_manifest_sha256\":\"{}\",\"schedule_blake3\":\"{}\"}}\n"
            ),
            head,
            executable_sha256,
            sha256_bytes(&source_manifest_bytes)?,
            sha256_bytes(&build_log_bytes)?,
            sha256_bytes(&fixture_manifest)?,
            schedule_blake3,
        )
        .as_bytes(),
    )?;
    create_empty(&run.join("rows.jsonl"))?;
    create_empty(&run.join("commands.jsonl"))?;
    append_sync(
        &run.join("failure-ledger.json"),
        "{\"sequence\":1,\"state\":\"OPEN\",\"preserved_failures\":0}",
    )?;

    let campaign_setup_ns = campaign_started.elapsed().as_nanos();
    let mut populations = Vec::new();
    let mut statistics = Vec::new();
    let mut command_wall_sum_ns = 0_u128;
    let mut sequence = 0_u64;
    let mut maximum_rss_bytes = 0_u128;
    let mut maximum_q_bytes = 0_u128;
    let mut maximum_total_connections = 0_u128;
    let mut maximum_scratch_connections = 0_u128;
    let mut maximum_fd = 0_u128;
    let mut maximum_cpu_ns = 0_u128;

    for (block_index, size_mib) in TRUSTED_SCHEDULE.iter().enumerate() {
        if campaign_started.elapsed().as_nanos() >= 30_000_000_000 {
            return Err("TrustedLocalDev campaign hard wall reached".to_owned());
        }
        let size = fixture.join("sizes").join(size_mib.to_string());
        let store = size.join("bases/base");
        let source = size.join("source-native").join(FILE_PATH);
        let identity = format!("trusted-b{:02}-{size_mib}", block_index + 1);
        let work = run.join(format!(".block-{identity}"));
        let size_arg = size_mib.to_string();
        let argv = [
            executable.display().to_string(),
            "stage1".to_owned(),
            "materialize".to_owned(),
            "trusted-block".to_owned(),
            store.display().to_string(),
            source.display().to_string(),
            size_arg.clone(),
            work.display().to_string(),
            identity.clone(),
        ];
        let argv_json = argv
            .iter()
            .map(|argument| format!("\"{}\"", json_escape(argument)))
            .collect::<Vec<_>>()
            .join(",");
        let started_unix_ns = unix_ns()?;
        let started = Instant::now();
        let output = Command::new(&executable)
            .args(["stage1", "materialize", "trusted-block"])
            .arg(&store)
            .arg(&source)
            .arg(&size_arg)
            .arg(&work)
            .arg(&identity)
            .output()
            .map_err(io_error)?;
        let command_wall_ns = started.elapsed().as_nanos();
        command_wall_sum_ns = command_wall_sum_ns
            .checked_add(command_wall_ns)
            .ok_or_else(|| "command wall sum overflow".to_owned())?;
        let completed_unix_ns = unix_ns()?;
        append_sync(
            &run.join("commands.jsonl"),
            &format!(
                concat!(
                    "{{\"sequence\":{},\"block\":{},\"integrity_mode\":\"TrustedLocalDev\",",
                    "\"size_mib\":{},\"executable_sha256\":\"{}\",\"argv\":[{}],",
                    "\"start_unix_ns\":{},\"end_unix_ns\":{},\"wall_ns\":{},",
                    "\"exit_code\":{},\"stderr\":\"{}\"}}"
                ),
                block_index + 1,
                block_index + 1,
                size_mib,
                executable_sha256,
                argv_json,
                started_unix_ns,
                completed_unix_ns,
                command_wall_ns,
                output.status.code().unwrap_or(-1),
                json_escape(&String::from_utf8_lossy(&output.stderr)),
            ),
        )?;
        if !output.status.success() {
            append_sync(
                &run.join("failure-ledger.json"),
                &format!(
                    "{{\"sequence\":2,\"state\":\"FAIL\",\"block\":{},\"reason\":\"trusted block failed\"}}",
                    block_index + 1
                ),
            )?;
            return Err(format!("TrustedLocalDev block {} failed", block_index + 1));
        }
        let stdout = String::from_utf8(output.stdout).map_err(display_error)?;
        let rows = stdout.lines().collect::<Vec<_>>();
        if rows.len() != 4
            || !rows[0].contains("\"row_kind\":\"warmup\"")
            || rows[1..]
                .iter()
                .any(|row| !row.contains("\"row_kind\":\"measured\""))
        {
            return Err("TrustedLocalDev block population is not 1+3".to_owned());
        }
        let terminal = rows[3];
        if json_u128(terminal, "fd_terminal")? != json_u128(terminal, "process_fd_baseline")?
            || json_u128(terminal, "connections_terminal")? != 0
            || json_u128(terminal, "scratch_connections_terminal")? != 0
            || json_u128(terminal, "total_connections_terminal")? != 0
            || json_u128(terminal, "operation_q_terminal_bytes")? != 0
            || json_u128(terminal, "residue")? != 0
        {
            return Err("TrustedLocalDev block terminal resources did not close".to_owned());
        }
        let mut measured = Vec::new();
        for row in rows {
            validate_attribution_json(row)?;
            if !row.contains("\"integrity_mode\":\"TrustedLocalDev\"")
                || json_u128(row, "authentication_passes")? != 0
                || json_u128(row, "identity_authentication_ns")? != 0
                || json_u128(row, "fetched_rows")? != json_u128(row, "role_decode_passes")?
            {
                return Err("TrustedLocalDev row trust equation failed".to_owned());
            }
            maximum_rss_bytes = maximum_rss_bytes.max(json_u128(row, "rss_peak_bytes")?);
            maximum_q_bytes = maximum_q_bytes.max(json_u128(row, "operation_q_high_water_bytes")?);
            maximum_total_connections =
                maximum_total_connections.max(json_u128(row, "total_connections_peak")?);
            maximum_scratch_connections =
                maximum_scratch_connections.max(json_u128(row, "scratch_connections_peak")?);
            maximum_fd = maximum_fd
                .max(json_u128(row, "fd_before")?)
                .max(json_u128(row, "fd_after")?);
            maximum_cpu_ns = maximum_cpu_ns.max(
                json_u128(row, "user_cpu_ns")?
                    .checked_add(json_u128(row, "system_cpu_ns")?)
                    .ok_or_else(|| "CPU total overflow".to_owned())?,
            );
            sequence += 1;
            append_sync(
                &run.join("rows.jsonl"),
                &enrich_attribution_row(
                    row,
                    sequence,
                    block_index + 1,
                    &executable_sha256,
                    &schedule_blake3,
                    command_wall_ns,
                )?,
            )?;
            if row.contains("\"row_kind\":\"measured\"") {
                measured.push(json_u128(row, "product_operation_wall_ns")?);
            }
        }
        let (p50, p95) = three_stats(&measured)?;
        statistics.push((*size_mib, p50, p95));
        populations.push(format!(
            "{{\"size_mib\":{},\"raw_ns\":{:?},\"p50_ns\":{},\"p95_ns\":{}}}",
            size_mib, measured, p50, p95
        ));
    }
    if sequence != 12 {
        return Err("TrustedLocalDev campaign row population is not 12".to_owned());
    }
    let commands = fs::read_to_string(run.join("commands.jsonl")).map_err(io_error)?;
    let command_records = commands.lines().collect::<Vec<_>>();
    if command_records.len() != 3 {
        return Err("TrustedLocalDev command population is not 3".to_owned());
    }
    durable_write(
        &run.join("commands.json"),
        format!("[{}]\n", command_records.join(",")).as_bytes(),
    )?;

    let time = |size| {
        statistics
            .iter()
            .find(|(candidate, _, _)| *candidate == size)
            .map(|(_, p50, p95)| (*p50, *p95))
            .ok_or_else(|| format!("missing TrustedLocalDev {size} MiB population"))
    };
    let (t0, t0_p95) = time(0)?;
    let (t24, t24_p95) = time(24)?;
    let (t96, t96_p95) = time(96)?;
    let slope = (t96 as f64 - t24 as f64) / 72.0;
    if slope <= 0.0 {
        return Err("TrustedLocalDev fitted slope is not positive".to_owned());
    }
    let fitted_intercept = t24 as f64 - 24.0 * slope;
    let sustained_mib_s = 1_000_000_000_f64 / slope;
    let residual0 = t0 as f64 - fitted_intercept;
    let model_valid = residual0.abs() <= 2_000_000_f64.max(t0 as f64 * 0.05);
    let fixed_target_pass = fitted_intercept < 20_000_000.0;
    let sustained_target_pass = sustained_mib_s >= 500.0;
    let p50_24_mib_s = 24_000_000_000_f64 / t24 as f64;
    let p95_24_mib_s = 24_000_000_000_f64 / t24_p95 as f64;
    let p50_96_mib_s = 96_000_000_000_f64 / t96 as f64;
    let p95_96_mib_s = 96_000_000_000_f64 / t96_p95 as f64;
    let primary_target_pass = p50_24_mib_s >= 450.0
        && p95_24_mib_s >= 24_000.0 / 58.667
        && p50_96_mib_s >= 450.0
        && p95_96_mib_s >= 96_000.0 / 234.667;
    let resource_gates_pass = maximum_rss_bytes < 32 * 1024 * 1024
        && maximum_q_bytes < 8 * 1024 * 1024
        && maximum_scratch_connections <= 1
        && maximum_total_connections <= 2;
    let campaign_wall_ns = campaign_started.elapsed().as_nanos();
    let campaign_coordinator_ns = campaign_wall_ns
        .checked_sub(campaign_setup_ns)
        .and_then(|wall| wall.checked_sub(command_wall_sum_ns))
        .ok_or_else(|| "campaign wall equation underflow".to_owned())?;
    if campaign_wall_ns >= 30_000_000_000 {
        return Err("TrustedLocalDev campaign exceeded the hard wall".to_owned());
    }
    let preferred_wall_pass = campaign_wall_ns < 15_000_000_000;
    let summary = format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1t-summary-v1\",\"status\":\"PASS\",",
            "\"integrity_mode\":\"TrustedLocalDev\",\"warmup_rows\":3,",
            "\"measured_rows\":9,\"population_exact\":true,",
            "\"campaign_wall_ns\":{},\"campaign_setup_ns\":{},",
            "\"command_wall_sum_ns\":{},\"campaign_coordinator_ns\":{},",
            "\"campaign_wall_equation_exact\":true,\"preferred_wall_pass\":{},",
            "\"hard_wall_pass\":true,\"primary_target_pass\":{},",
            "\"fixed_target_pass\":{},\"sustained_target_pass\":{},",
            "\"performance\":{{\"zero_p50_ns\":{},\"zero_p95_ns\":{},",
            "\"24_mib\":{{\"p50_ns\":{},\"p95_ns\":{},\"p50_mib_s\":{},\"p95_mib_s\":{}}},",
            "\"96_mib\":{{\"p50_ns\":{},\"p95_ns\":{},\"p50_mib_s\":{},\"p95_mib_s\":{}}},",
            "\"fitted_intercept_ns\":{},\"slope_ns_per_mib\":{},",
            "\"fitted_sustained_mib_s\":{},\"zero_residual_ns\":{},",
            "\"model_valid\":{}}},",
            "\"resources\":{{\"maximum_rss_bytes\":{},\"maximum_q_bytes\":{},",
            "\"maximum_scratch_connections\":{},\"maximum_total_connections\":{},",
            "\"maximum_fd\":{},\"maximum_row_cpu_ns\":{},",
            "\"terminal_primary_connections\":0,\"terminal_scratch_connections\":0,",
            "\"terminal_total_connections\":0,\"terminal_q_bytes\":0,",
            "\"residue\":0,\"resource_gates_pass\":{}}},",
            "\"populations\":[{}]}}\n"
        ),
        campaign_wall_ns,
        campaign_setup_ns,
        command_wall_sum_ns,
        campaign_coordinator_ns,
        preferred_wall_pass,
        primary_target_pass,
        fixed_target_pass,
        sustained_target_pass,
        t0,
        t0_p95,
        t24,
        t24_p95,
        p50_24_mib_s,
        p95_24_mib_s,
        t96,
        t96_p95,
        p50_96_mib_s,
        p95_96_mib_s,
        fitted_intercept,
        slope,
        sustained_mib_s,
        residual0,
        model_valid,
        maximum_rss_bytes,
        maximum_q_bytes,
        maximum_scratch_connections,
        maximum_total_connections,
        maximum_fd,
        maximum_cpu_ns,
        resource_gates_pass,
        populations.join(","),
    );
    durable_write(&run.join("summary.json"), summary.as_bytes())?;
    durable_write(
        &run.join("summary.md"),
        format!(
            concat!(
                "# Stage 1.1T TrustedLocalDev materialization\n\n",
                "Status: **PASS evidence population**. This is not Verified.\n\n",
                "| Size | p50 ms | p95 ms | p50 MiB/s | p95 MiB/s |\n",
                "|---:|---:|---:|---:|---:|\n",
                "| 0 MiB | {:.6} | {:.6} | N/A | N/A |\n",
                "| 24 MiB | {:.6} | {:.6} | {:.3} | {:.3} |\n",
                "| 96 MiB | {:.6} | {:.6} | {:.3} | {:.3} |\n\n",
                "Fitted intercept: `{:.6} ms`; fitted sustained: `{:.3} MiB/s`. ",
                "Primary target pass: `{}`. Campaign wall: `{}` ns.\n"
            ),
            t0 as f64 / 1_000_000.0,
            t0_p95 as f64 / 1_000_000.0,
            t24 as f64 / 1_000_000.0,
            t24_p95 as f64 / 1_000_000.0,
            p50_24_mib_s,
            p95_24_mib_s,
            t96 as f64 / 1_000_000.0,
            t96_p95 as f64 / 1_000_000.0,
            p50_96_mib_s,
            p95_96_mib_s,
            fitted_intercept / 1_000_000.0,
            sustained_mib_s,
            primary_target_pass,
            campaign_wall_ns,
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("campaign-time.txt"),
        format!(
            "schema=layerfs-stage1t-campaign-time-v1\nstatus=PASS\ncampaign_wall_ns={campaign_wall_ns}\ncampaign_setup_ns={campaign_setup_ns}\ncommand_wall_sum_ns={command_wall_sum_ns}\ncampaign_coordinator_ns={campaign_coordinator_ns}\ncampaign_wall_equation_exact=true\nwarmups=3\nmeasured=9\npreferred_wall_ns=15000000000\nhard_wall_ns=30000000000\n"
        )
        .as_bytes(),
    )?;
    append_sync(
        &run.join("failure-ledger.json"),
        "{\"sequence\":2,\"state\":\"CLOSE\",\"status\":\"PASS\",\"preserved_failures\":0}",
    )?;
    durable_write(
        &run.join("terminal-receipt.json"),
        format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1t-terminal-receipt-v1\",\"status\":\"PASS\",",
                "\"integrity_mode\":\"TrustedLocalDev\",\"rows_sha256\":\"{}\",",
                "\"commands_sha256\":\"{}\",\"summary_sha256\":\"{}\",",
                "\"campaign_time_sha256\":\"{}\",\"source_manifest_sha256\":\"{}\",",
                "\"build_log_sha256\":\"{}\",\"git_commit\":\"{}\",",
                "\"executable_sha256\":\"{}\"}}\n"
            ),
            sha256_file(&run.join("rows.jsonl"))?,
            sha256_file(&run.join("commands.json"))?,
            sha256_file(&run.join("summary.json"))?,
            sha256_file(&run.join("campaign-time.txt"))?,
            sha256_file(&run.join("source-manifest.json"))?,
            sha256_file(&run.join("build.log"))?,
            head,
            executable_sha256,
        )
        .as_bytes(),
    )?;
    println!(
        "stage1t-trusted-run status=PASS run={} wall_ns={} primary_target_pass={}",
        run.display(),
        campaign_wall_ns,
        primary_target_pass,
    );
    Ok(())
}

#[derive(Clone)]
struct AcceptanceSample {
    pair: u64,
    size_mib: u64,
    operand: char,
    wall_ns: u128,
    cpu_ns: u128,
    rss_bytes: u128,
    q_bytes: u128,
    fd_peak: u128,
    primary_connections: u128,
    scratch_connections: u128,
    total_connections: u128,
    sync_calls: u128,
    residue: u128,
}

pub fn acceptance_run(
    control: &Path,
    candidate: &Path,
    fixture: &Path,
    run: &Path,
) -> EvalResult<()> {
    if run.exists() {
        return Err(format!("run directory already exists: {}", run.display()));
    }
    let control = control.canonicalize().map_err(io_error)?;
    let candidate = candidate.canonicalize().map_err(io_error)?;
    let fixture = fixture.canonicalize().map_err(io_error)?;
    crate::stage1_fixture::verify_sealed(&fixture)?;
    verify_fixture_sources(&fixture)?;
    let fixture_manifest = fs::read(fixture.join("fixture-manifest.json")).map_err(io_error)?;
    let control_sha256 = sha256_file(&control)?;
    let candidate_sha256 = sha256_file(&candidate)?;
    let control_blake3 = digest_file(&control)?;
    let candidate_blake3 = digest_file(&candidate)?;
    if control_sha256 == candidate_sha256 {
        return Err("control and candidate executables are identical".to_owned());
    }
    let manifest_directory = fixture
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "fixture is not below the target directory".to_owned())?
        .join("layerfs-stage1m-custody/source-manifests");
    for (name, executable_sha256) in [
        ("source-manifest-control.json", control_sha256.as_str()),
        ("source-manifest-candidate.json", candidate_sha256.as_str()),
    ] {
        let manifest = fs::read_to_string(manifest_directory.join(name)).map_err(io_error)?;
        if !manifest.contains(executable_sha256)
            || !manifest.contains("\"dirty_tree\":false")
            || !manifest.contains("\"status\":\"PASS\"")
        {
            return Err(format!("{name} does not bind its clean executable"));
        }
    }
    let schedule = acceptance_schedule_json();
    let schedule_blake3 = blake3::hash(schedule.as_bytes()).to_hex().to_string();
    fs::create_dir(run).map_err(io_error)?;
    let campaign_started = Instant::now();
    durable_write(&run.join("schedule.json"), schedule.as_bytes())?;
    durable_write(
        &run.join("preregistration.json"),
        concat!(
            "{\"schema\":\"layerfs-stage1m-acceptance-preregistration-v1\",",
            "\"status\":\"PASS\",\"paired_warmups\":24,\"measured\":24,",
            "\"p50\":\"mean_positions_2_3\",\"p95\":\"position_4\",",
            "\"preferred_wall_ns\":15000000000,\"hard_wall_ns\":30000000000,",
            "\"wins_required_24\":3,\"wins_required_96\":3,",
            "\"fixed_cost_allowance_ns\":1000000,\"p95_allowance_ns\":1000000}\n"
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("readiness.json"),
        format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1m-acceptance-readiness-v1\",",
                "\"status\":\"PASS\",\"measured_rows_started\":false,",
                "\"control_sha256\":\"{}\",\"candidate_sha256\":\"{}\",",
                "\"fixture_blake3\":\"{}\",\"schedule_blake3\":\"{}\",",
                "\"expected_rows\":48,\"control_resource_derivations\":",
                "{{\"q_high_water_bytes\":\"source_bound_8_mib\",",
                "\"scratch_connections_peak\":\"scratch.tables\",",
                "\"total_connections_peak\":\"active_connections_plus_scratch.tables\"}}}}\n"
            ),
            control_sha256,
            candidate_sha256,
            blake3::hash(&fixture_manifest).to_hex(),
            schedule_blake3,
        )
        .as_bytes(),
    )?;
    durable_write(&run.join("fixture-manifest.json"), &fixture_manifest)?;
    let cwd = std::env::current_dir().map_err(io_error)?;
    durable_write(
        &run.join("environment.json"),
        format!(
            "{{\"schema\":\"layerfs-stage1m-environment-v1\",\"network\":0,\"rows_serial\":true,\"cwd\":\"{}\"}}\n",
            json_escape(&cwd.display().to_string())
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("executables.json"),
        format!(
            concat!(
                "{{\"control\":{{\"path\":\"{}\",\"sha256\":\"{}\",",
                "\"blake3\":\"{}\"}},\"candidate\":{{\"path\":\"{}\",",
                "\"sha256\":\"{}\",\"blake3\":\"{}\"}}}}\n"
            ),
            json_escape(&control.display().to_string()),
            control_sha256,
            control_blake3,
            json_escape(&candidate.display().to_string()),
            candidate_sha256,
            candidate_blake3,
        )
        .as_bytes(),
    )?;
    copy_acceptance_manifests(run, &fixture)?;
    create_empty(&run.join("rows.jsonl"))?;
    create_empty(&run.join("commands.jsonl"))?;
    append_sync(
        &run.join("failure-ledger.json"),
        "{\"sequence\":1,\"state\":\"OPEN\",\"preserved_failures\":0}",
    )?;

    let setup_wall_ns = campaign_started.elapsed().as_nanos();
    let mut command_wall_sum_ns = 0_u128;
    let mut row_sequence = 0_u64;
    let mut command_sequence = 0_u64;
    let mut samples = Vec::with_capacity(24);
    for (block_index, block) in ACCEPTANCE_SCHEDULE.iter().enumerate() {
        let mut signatures = Vec::with_capacity(2);
        for (order_index, operand) in block.order.iter().enumerate() {
            if campaign_started.elapsed().as_nanos() >= 30_000_000_000 {
                return acceptance_campaign_failure(run, block_index + 1, "hard wall reached");
            }
            command_sequence += 1;
            let (executable, executable_sha256, source_role) = if *operand == 'A' {
                (&control, &control_sha256, "control")
            } else {
                (&candidate, &candidate_sha256, "candidate")
            };
            let size = fixture.join("sizes").join(block.size_mib.to_string());
            let store = size.join("bases/base");
            let source = size.join("source-native").join(FILE_PATH);
            let identity = format!(
                "p{}-s{}-o{}-{}",
                block.pair,
                block.size_mib,
                order_index + 1,
                operand
            );
            let work = run.join(format!(".sample-{identity}"));
            let size_arg = block.size_mib.to_string();
            let argv = [
                executable.display().to_string(),
                "stage1".to_owned(),
                "materialize".to_owned(),
                "parity-row".to_owned(),
                store.display().to_string(),
                source.display().to_string(),
                size_arg.clone(),
                work.display().to_string(),
                identity.clone(),
            ];
            let argv_json = argv
                .iter()
                .map(|argument| format!("\"{}\"", json_escape(argument)))
                .collect::<Vec<_>>()
                .join(",");
            let start_unix_ns = unix_ns()?;
            let started = Instant::now();
            let output = Command::new(executable)
                .args(["stage1", "materialize", "parity-row"])
                .arg(&store)
                .arg(&source)
                .arg(&size_arg)
                .arg(&work)
                .arg(&identity)
                .output()
                .map_err(io_error)?;
            let command_wall_ns = started.elapsed().as_nanos();
            command_wall_sum_ns = command_wall_sum_ns
                .checked_add(command_wall_ns)
                .ok_or_else(|| "acceptance command wall overflow".to_owned())?;
            append_sync(
                &run.join("commands.jsonl"),
                &format!(
                    concat!(
                        "{{\"sequence\":{},\"block\":{},\"pair\":{},",
                        "\"pair_size_order\":{},\"operand\":\"{}\",",
                        "\"source_role\":\"{}\",\"size_mib\":{},",
                        "\"executable\":\"{}\",\"executable_sha256\":\"{}\",",
                        "\"fixture_root\":\"{}\",\"store\":\"{}\",",
                        "\"source\":\"{}\",\"work\":\"{}\",\"cwd\":\"{}\",",
                        "\"argv\":[{}],\"start_unix_ns\":{},\"end_unix_ns\":{},",
                        "\"wall_ns\":{},\"exit_code\":{},\"stderr\":\"{}\"}}"
                    ),
                    command_sequence,
                    block_index + 1,
                    block.pair,
                    order_index + 1,
                    operand,
                    source_role,
                    block.size_mib,
                    json_escape(&executable.display().to_string()),
                    executable_sha256,
                    json_escape(&fixture.display().to_string()),
                    json_escape(&store.display().to_string()),
                    json_escape(&source.display().to_string()),
                    json_escape(&work.display().to_string()),
                    json_escape(&cwd.display().to_string()),
                    argv_json,
                    start_unix_ns,
                    unix_ns()?,
                    command_wall_ns,
                    output.status.code().unwrap_or(-1),
                    json_escape(&String::from_utf8_lossy(&output.stderr)),
                ),
            )?;
            let stdout = String::from_utf8(output.stdout).map_err(display_error)?;
            let rows = stdout.lines().collect::<Vec<_>>();
            for (child_index, row) in rows.iter().enumerate() {
                row_sequence += 1;
                append_sync(
                    &run.join("rows.jsonl"),
                    &enrich_acceptance_row(
                        row,
                        row_sequence,
                        block_index + 1,
                        block,
                        order_index + 1,
                        *operand,
                        source_role,
                        executable_sha256,
                        &schedule_blake3,
                        command_wall_ns,
                    )?,
                )?;
                if output.status.success() {
                    if let Err(error) = validate_acceptance_row(row, *operand == 'B') {
                        return acceptance_campaign_failure(
                            run,
                            block_index + 1,
                            &format!("row validation failed: {error}"),
                        );
                    }
                    if child_index == 1 {
                        let measured = (|| {
                            let signature = acceptance_semantic_signature(row)?;
                            let cpu_ns = json_u128(row, "user_cpu_ns")?
                                .checked_add(json_u128(row, "system_cpu_ns")?)
                                .ok_or_else(|| "acceptance CPU overflow".to_owned())?;
                            let wall_ns = json_u128(row, "product_operation_wall_ns")?;
                            Ok::<_, String>((
                                signature,
                                cpu_ns,
                                wall_ns,
                                json_u128(row, "rss_peak_bytes")?,
                                if *operand == 'A' {
                                    8 * 1024 * 1024
                                } else {
                                    json_u128(row, "operation_q_high_water_bytes")?
                                },
                                json_u128(row, "fd_before")?.max(json_u128(row, "fd_after")?),
                                json_u128(row, "active_connections")?,
                                if *operand == 'A' {
                                    json_u128(row, "tables")?
                                } else {
                                    json_u128(row, "scratch_connections_peak")?
                                },
                                if *operand == 'A' {
                                    json_u128(row, "active_connections")?
                                        .checked_add(json_u128(row, "tables")?)
                                        .ok_or_else(|| {
                                            "control connection peak overflow".to_owned()
                                        })?
                                } else {
                                    json_u128(row, "total_connections_peak")?
                                },
                                json_u128(row, "sync_calls")?,
                                json_u128(row, "residue")?,
                            ))
                        })();
                        let (
                            signature,
                            cpu_ns,
                            wall_ns,
                            rss_bytes,
                            q_bytes,
                            fd_peak,
                            primary_connections,
                            scratch_connections,
                            total_connections,
                            sync_calls,
                            residue,
                        ) = match measured {
                            Ok(measured) => measured,
                            Err(error) => {
                                return acceptance_campaign_failure(
                                    run,
                                    block_index + 1,
                                    &format!("measured row parsing failed: {error}"),
                                );
                            }
                        };
                        signatures.push(signature);
                        samples.push(AcceptanceSample {
                            pair: block.pair,
                            size_mib: block.size_mib,
                            operand: *operand,
                            wall_ns,
                            cpu_ns,
                            rss_bytes,
                            q_bytes,
                            fd_peak,
                            primary_connections,
                            scratch_connections,
                            total_connections,
                            sync_calls,
                            residue,
                        });
                    }
                }
            }
            if !output.status.success() {
                return acceptance_campaign_failure(
                    run,
                    block_index + 1,
                    "operand command failed; partial rows preserved",
                );
            }
            if rows.len() != 2
                || !rows[0].contains("\"row_kind\":\"warmup\"")
                || !rows[1].contains("\"row_kind\":\"measured\"")
            {
                return acceptance_campaign_failure(
                    run,
                    block_index + 1,
                    "operand population is not one warmup plus one measured",
                );
            }
        }
        if signatures.len() != 2 || signatures[0] != signatures[1] {
            return acceptance_campaign_failure(
                run,
                block_index + 1,
                "adjacent semantic work differs",
            );
        }
    }
    if row_sequence != 48 || command_sequence != 24 || samples.len() != 24 {
        return acceptance_campaign_failure(run, 12, "acceptance population mismatch");
    }
    let commands = fs::read_to_string(run.join("commands.jsonl")).map_err(io_error)?;
    durable_write(
        &run.join("commands.json"),
        format!("[{}]\n", commands.lines().collect::<Vec<_>>().join(",")).as_bytes(),
    )?;
    let disposition = match acceptance_disposition(&samples) {
        Ok(disposition) => disposition,
        Err(error) => {
            return acceptance_campaign_failure(run, 12, &format!("disposition failed: {error}"));
        }
    };
    let campaign_wall_ns = campaign_started.elapsed().as_nanos();
    if campaign_wall_ns >= 30_000_000_000 {
        return acceptance_campaign_failure(run, 12, "hard wall reached");
    }
    let coordinator_wall_ns = campaign_wall_ns
        .checked_sub(setup_wall_ns)
        .and_then(|wall| wall.checked_sub(command_wall_sum_ns))
        .ok_or_else(|| "acceptance campaign wall equation underflow".to_owned())?;
    let rows_sha256 = sha256_file(&run.join("rows.jsonl"))?;
    let commands_sha256 = sha256_file(&run.join("commands.json"))?;
    durable_write(
        &run.join("summary.json"),
        acceptance_summary_json(
            &disposition,
            campaign_wall_ns,
            setup_wall_ns,
            command_wall_sum_ns,
            coordinator_wall_ns,
        )?
        .as_bytes(),
    )?;
    durable_write(
        &run.join("summary.md"),
        format!(
            "# Stage 1.1M paired acceptance\n\nStatus: **{}**. Exact population: 24 paired warmups + 24 measured complete-public rows. 24/96 wins: `{}/{}`. Complete wall: `{campaign_wall_ns}` ns.\n",
            disposition.status, disposition.wins24, disposition.wins96
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("campaign-time.txt"),
        format!(
            "schema=layerfs-stage1m-acceptance-campaign-time-v1\nstatus={}\ncampaign_wall_ns={campaign_wall_ns}\nsetup_wall_ns={setup_wall_ns}\ncommand_wall_sum_ns={command_wall_sum_ns}\ncoordinator_wall_ns={coordinator_wall_ns}\ncampaign_wall_equation_exact=true\npaired_warmups=24\nmeasured=24\npreferred_wall_ns=15000000000\nhard_wall_ns=30000000000\n",
            disposition.status
        )
        .as_bytes(),
    )?;
    append_sync(
        &run.join("failure-ledger.json"),
        &format!(
            "{{\"sequence\":2,\"state\":\"CLOSE\",\"status\":\"{}\",\"preserved_failures\":0}}",
            disposition.status
        ),
    )?;
    durable_write(
        &run.join("terminal-receipt.json"),
        format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1m-acceptance-terminal-receipt-v1\",",
                "\"status\":\"{}\",\"rows_sha256\":\"{}\",",
                "\"commands_sha256\":\"{}\",\"summary_sha256\":\"{}\",",
                "\"campaign_time_sha256\":\"{}\",\"failure_ledger_sha256\":\"{}\",",
                "\"schedule_sha256\":\"{}\",\"environment_sha256\":\"{}\",",
                "\"fixture_manifest_sha256\":\"{}\",\"executables_sha256\":\"{}\",",
                "\"control_manifest_sha256\":\"{}\",\"candidate_manifest_sha256\":\"{}\",",
                "\"control_sha256\":\"{}\",\"candidate_sha256\":\"{}\"}}\n"
            ),
            disposition.status,
            rows_sha256,
            commands_sha256,
            sha256_file(&run.join("summary.json"))?,
            sha256_file(&run.join("campaign-time.txt"))?,
            sha256_file(&run.join("failure-ledger.json"))?,
            sha256_file(&run.join("schedule.json"))?,
            sha256_file(&run.join("environment.json"))?,
            sha256_file(&run.join("fixture-manifest.json"))?,
            sha256_file(&run.join("executables.json"))?,
            sha256_file(&run.join("source-manifest-control.json"))?,
            sha256_file(&run.join("source-manifest-candidate.json"))?,
            control_sha256,
            candidate_sha256,
        )
        .as_bytes(),
    )?;
    println!(
        "stage1m-acceptance-run status={} run={} wall_ns={} wins24={} wins96={}",
        disposition.status,
        run.display(),
        campaign_wall_ns,
        disposition.wins24,
        disposition.wins96,
    );
    if disposition.status == "PASS" {
        Ok(())
    } else {
        Err("paired acceptance requires repair; complete evidence preserved".to_owned())
    }
}

fn acceptance_campaign_failure(run: &Path, block: usize, reason: &str) -> EvalResult<()> {
    append_sync(
        &run.join("failure-ledger.json"),
        &format!(
            "{{\"sequence\":2,\"state\":\"FAIL\",\"block\":{block},\"reason\":\"{}\"}}",
            json_escape(reason)
        ),
    )?;
    Err(format!(
        "acceptance campaign stopped at block {block}: {reason}"
    ))
}

fn copy_acceptance_manifests(run: &Path, fixture: &Path) -> EvalResult<()> {
    let source = fixture
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "fixture is not below the target directory".to_owned())?
        .join("layerfs-stage1m-custody/source-manifests");
    for name in [
        "source-manifest-historical.json",
        "source-manifest-historical-harness.json",
        "source-manifest-control.json",
        "source-manifest-candidate.json",
    ] {
        durable_write(
            &run.join(name),
            &fs::read(source.join(name)).map_err(io_error)?,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn enrich_acceptance_row(
    row: &str,
    sequence: u64,
    block_index: usize,
    block: &AcceptanceBlock,
    order: usize,
    operand: char,
    source_role: &str,
    executable_sha256: &str,
    schedule_blake3: &str,
    command_wall_ns: u128,
) -> EvalResult<String> {
    let body = row
        .strip_suffix('}')
        .ok_or_else(|| "child row is not a JSON object".to_owned())?;
    Ok(format!(
        "{},\"sequence\":{},\"block\":{},\"pair\":{},\"pair_size_order\":{},\"operand\":\"{}\",\"source_role\":\"{}\",\"executable_sha256\":\"{}\",\"schedule_blake3\":\"{}\",\"command_wall_ns\":{}}}",
        body,
        sequence,
        block_index,
        block.pair,
        order,
        operand,
        source_role,
        executable_sha256,
        schedule_blake3,
        command_wall_ns,
    ))
}

fn validate_acceptance_row(row: &str, candidate: bool) -> EvalResult<()> {
    validate_instrumented_row(row)?;
    validate_row_wall_json(row)?;
    for exact in [
        "\"schema\":\"layerfs-stage1m-attribution-row-v2\"",
        "\"status\":\"PASS\"",
        "\"engine_sql_exact\":true",
        "\"scratch_sql_exact\":true",
        "\"fetched_auth_decode_exact\":true",
        "\"operation_q_terminal_bytes\":0",
        "\"residue\":0",
    ] {
        if !row.contains(exact) {
            return Err(format!("acceptance row does not prove {exact}"));
        }
    }
    let resources_pass = json_u128(row, "rss_peak_bytes")? <= 32 * 1024 * 1024
        && json_u128(row, "rss_current_bytes")? <= 32 * 1024 * 1024
        && json_u128(row, "fd_before")? <= 24
        && json_u128(row, "fd_after")? <= 24
        && (!candidate
            || json_u128(row, "operation_q_high_water_bytes")? < 8 * 1024 * 1024
                && json_u128(row, "owned_temp_terminal")? == 0
                && json_u128(row, "descriptor_spool_bytes_terminal")? == 0
                && json_u128(row, "scratch_connections_peak")? <= 1
                && json_u128(row, "total_connections_peak")? <= 2);
    if !resources_pass {
        return Err("acceptance row resource gate failed".to_owned());
    }
    if row.contains("\"row_kind\":\"measured\"")
        && (json_u128(row, "fd_terminal")? != json_u128(row, "process_fd_baseline")?
            || json_u128(row, "connections_terminal")? != 0
            || candidate
                && (json_u128(row, "scratch_connections_terminal")? != 0
                    || json_u128(row, "total_connections_terminal")? != 0))
    {
        return Err("acceptance row terminal resource closure failed".to_owned());
    }
    Ok(())
}

fn acceptance_semantic_signature(row: &str) -> EvalResult<Vec<u128>> {
    [
        "logical_bytes",
        "fetched_rows",
        "authentication_passes",
        "role_decode_passes",
        "object_bytes_read",
        "payload_batch_references",
        "payload_batch_maximum",
        "publication_commits",
        "bytes_written",
        "temp_calls",
        "replace_calls",
        "operation_q_terminal_bytes",
        "residue",
    ]
    .into_iter()
    .map(|key| json_u128(row, key))
    .collect()
}

#[derive(Clone)]
struct AcceptanceStats {
    raw: Vec<u128>,
    minimum: u128,
    p50: u128,
    p95: u128,
    maximum: u128,
}

fn acceptance_stats(values: &[u128]) -> EvalResult<AcceptanceStats> {
    if values.len() != 4 {
        return Err("acceptance n=4 statistic requires four values".to_owned());
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    Ok(AcceptanceStats {
        raw: values.to_vec(),
        minimum: sorted[0],
        p50: sorted[1]
            .checked_add(sorted[2])
            .ok_or_else(|| "acceptance p50 overflow".to_owned())?
            / 2,
        p95: sorted[3],
        maximum: sorted[3],
    })
}

struct AbsoluteClass {
    name: &'static str,
    p50_24: u128,
    p95_24: u128,
    p50_96: u128,
    p95_96: u128,
}

// Section 16.1's displayed millisecond gates converted exactly to nanoseconds.
const ABSOLUTE_CLASSES: [AbsoluteClass; 5] = [
    AbsoluteClass {
        name: "375",
        p50_24: 64_000_000,
        p95_24: 70_400_000,
        p50_96: 256_000_000,
        p95_96: 281_600_000,
    },
    AbsoluteClass {
        name: "400",
        p50_24: 60_000_000,
        p95_24: 66_000_000,
        p50_96: 240_000_000,
        p95_96: 264_000_000,
    },
    AbsoluteClass {
        name: "450",
        p50_24: 53_333_000,
        p95_24: 58_667_000,
        p50_96: 213_333_000,
        p95_96: 234_667_000,
    },
    AbsoluteClass {
        name: "500",
        p50_24: 48_000_000,
        p95_24: 52_800_000,
        p50_96: 192_000_000,
        p95_96: 211_200_000,
    },
    AbsoluteClass {
        name: "800",
        p50_24: 30_000_000,
        p95_24: 33_000_000,
        p50_96: 120_000_000,
        p95_96: 132_000_000,
    },
];

struct AcceptanceDisposition {
    status: &'static str,
    populations: Vec<(u64, AcceptanceStats, AcceptanceStats)>,
    wins24: u64,
    wins96: u64,
    fixed_cost_pass: bool,
    p95_relative_pass: bool,
    higher_absolute_class: bool,
    absolute_classes_json: String,
    primary_class_pass: bool,
    primary_nonmaterial_microvariance: bool,
    fitted_fixed_ns: f64,
    fitted_bandwidth_mib_s: f64,
    model_valid: bool,
    cpu_scaling_pass: bool,
    cpu_regression_pass: bool,
    no_resource_regression: bool,
    no_sync_regression: bool,
    no_residue_regression: bool,
}

fn acceptance_disposition(samples: &[AcceptanceSample]) -> EvalResult<AcceptanceDisposition> {
    let mut populations = Vec::new();
    for size in [0_u64, 24, 96] {
        let operand = |wanted| {
            samples
                .iter()
                .filter(|sample| sample.size_mib == size && sample.operand == wanted)
                .map(|sample| sample.wall_ns)
                .collect::<Vec<_>>()
        };
        populations.push((
            size,
            acceptance_stats(&operand('A'))?,
            acceptance_stats(&operand('B'))?,
        ));
    }
    let stats = |size: u64, candidate: bool| {
        populations
            .iter()
            .find(|(candidate_size, _, _)| *candidate_size == size)
            .map(
                |(_, control, candidate_stats)| {
                    if candidate {
                        candidate_stats
                    } else {
                        control
                    }
                },
            )
            .ok_or_else(|| format!("missing acceptance size {size}"))
    };
    let wins = |size| {
        (1..=4)
            .filter(|pair| {
                let wall = |operand| {
                    samples
                        .iter()
                        .find(|sample| {
                            sample.size_mib == size
                                && sample.pair == *pair
                                && sample.operand == operand
                        })
                        .map(|sample| sample.wall_ns)
                };
                matches!((wall('A'), wall('B')), (Some(control), Some(candidate)) if candidate < control)
            })
            .count() as u64
    };
    let wins24 = wins(24);
    let wins96 = wins(96);
    let class_pass = |class: &AbsoluteClass, candidate: bool| -> EvalResult<bool> {
        let s24 = stats(24, candidate)?;
        let s96 = stats(96, candidate)?;
        Ok(s24.p50 <= class.p50_24
            && s24.p95 <= class.p95_24
            && s96.p50 <= class.p50_96
            && s96.p95 <= class.p95_96)
    };
    let mut control_highest = None;
    let mut candidate_highest = None;
    let mut classes = Vec::new();
    for (index, class) in ABSOLUTE_CLASSES.iter().enumerate() {
        let control_pass = class_pass(class, false)?;
        let candidate_pass = class_pass(class, true)?;
        if control_pass {
            control_highest = Some(index);
        }
        if candidate_pass {
            candidate_highest = Some(index);
        }
        classes.push(format!(
            "{{\"class_mib_s\":{},\"control_pass\":{},\"candidate_pass\":{}}}",
            class.name, control_pass, candidate_pass
        ));
    }
    let higher_absolute_class = candidate_highest
        .is_some_and(|candidate| control_highest.is_none_or(|control| candidate > control));
    let fixed_cost_pass = stats(0, true)?.p50 <= stats(0, false)?.p50 + 1_000_000;
    let mut p95_relative_pass = true;
    for size in [0_u64, 24, 96] {
        p95_relative_pass &=
            stats(size, true)?.p95 <= stats(size, false)?.p95 + 1_000_000 || higher_absolute_class;
    }
    let t0 = stats(0, true)?.p50 as f64;
    let t24 = stats(24, true)?.p50 as f64;
    let t96 = stats(96, true)?.p50 as f64;
    let slope = (t96 - t24) / 72.0;
    let residual24 = t24 - (t0 + 24.0 * slope);
    let residual96 = t96 - (t0 + 96.0 * slope);
    let model_valid = slope > 0.0
        && residual24.abs() <= 2_000_000_f64.max(t24 * 0.05)
        && residual96.abs() <= 2_000_000_f64.max(t96 * 0.05);
    let fitted_bandwidth_mib_s = if slope > 0.0 {
        1_000_000_000.0 / slope
    } else {
        0.0
    };
    let cpu = |size| {
        acceptance_stats(
            &samples
                .iter()
                .filter(|sample| sample.size_mib == size && sample.operand == 'B')
                .map(|sample| sample.cpu_ns)
                .collect::<Vec<_>>(),
        )
    };
    let cpu0 = cpu(0)?.p50;
    let cpu24 = cpu(24)?.p50;
    let cpu96 = cpu(96)?.p50;
    let cpu_scaling_pass = cpu24 > cpu0
        && cpu96 > cpu0
        && (cpu96 - cpu0) as f64 / 96.0 <= 1.25 * (cpu24 - cpu0) as f64 / 24.0;
    let mut cpu_regression_pass = true;
    let mut no_resource_regression = true;
    let mut no_sync_regression = true;
    let mut no_residue_regression = true;
    for candidate in samples.iter().filter(|sample| sample.operand == 'B') {
        let control = samples
            .iter()
            .find(|sample| {
                sample.operand == 'A'
                    && sample.pair == candidate.pair
                    && sample.size_mib == candidate.size_mib
            })
            .ok_or_else(|| "candidate has no adjacent control".to_owned())?;
        cpu_regression_pass &= candidate.cpu_ns <= control.cpu_ns;
        no_resource_regression &= candidate.rss_bytes <= control.rss_bytes
            && candidate.q_bytes <= control.q_bytes
            && candidate.fd_peak <= control.fd_peak
            && candidate.primary_connections <= control.primary_connections
            && candidate.scratch_connections <= control.scratch_connections
            && candidate.total_connections <= control.total_connections;
        no_sync_regression &= candidate.sync_calls <= control.sync_calls;
        no_residue_regression &= candidate.residue == 0 && control.residue == 0;
    }
    let primary_class_pass = class_pass(&ABSOLUTE_CLASSES[2], true)?;
    let primary = &ABSOLUTE_CLASSES[2];
    let primary_miss_ns = [
        stats(24, true)?.p50.saturating_sub(primary.p50_24),
        stats(24, true)?.p95.saturating_sub(primary.p95_24),
        stats(96, true)?.p50.saturating_sub(primary.p50_96),
        stats(96, true)?.p95.saturating_sub(primary.p95_96),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    let pass = wins24 >= 3
        && wins96 >= 3
        && fixed_cost_pass
        && p95_relative_pass
        && primary_class_pass
        && model_valid
        && t0 < 20_000_000.0
        && fitted_bandwidth_mib_s >= 500.0
        && cpu_scaling_pass
        && cpu_regression_pass
        && no_resource_regression
        && no_sync_regression
        && no_residue_regression;
    Ok(AcceptanceDisposition {
        status: if pass { "PASS" } else { "REVISE" },
        populations,
        wins24,
        wins96,
        fixed_cost_pass,
        p95_relative_pass,
        higher_absolute_class,
        absolute_classes_json: format!("[{}]", classes.join(",")),
        primary_class_pass,
        primary_nonmaterial_microvariance: !primary_class_pass && primary_miss_ns < 1_000_000,
        fitted_fixed_ns: t0,
        fitted_bandwidth_mib_s,
        model_valid,
        cpu_scaling_pass,
        cpu_regression_pass,
        no_resource_regression,
        no_sync_regression,
        no_residue_regression,
    })
}

fn acceptance_summary_json(
    disposition: &AcceptanceDisposition,
    campaign_wall_ns: u128,
    setup_wall_ns: u128,
    command_wall_sum_ns: u128,
    coordinator_wall_ns: u128,
) -> EvalResult<String> {
    let populations = disposition
        .populations
        .iter()
        .map(|(size, control, candidate)| {
            format!(
                concat!(
                    "{{\"size_mib\":{},\"control\":{{\"raw_ns\":{:?},",
                    "\"minimum_ns\":{},\"p50_ns\":{},\"p95_ns\":{},\"maximum_ns\":{}}},",
                    "\"candidate\":{{\"raw_ns\":{:?},\"minimum_ns\":{},",
                    "\"p50_ns\":{},\"p95_ns\":{},\"maximum_ns\":{}}}}}"
                ),
                size,
                control.raw,
                control.minimum,
                control.p50,
                control.p95,
                control.maximum,
                candidate.raw,
                candidate.minimum,
                candidate.p50,
                candidate.p95,
                candidate.maximum,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1m-acceptance-summary-v1\",",
            "\"status\":\"{}\",\"paired_warmups\":24,\"measured_rows\":24,",
            "\"population_exact\":true,\"semantic_exact\":true,",
            "\"wins_24\":{},\"wins_96\":{},",
            "\"control_resource_derivations\":{{\"q_high_water_bytes\":\"source_bound_8_mib\",",
            "\"scratch_connections_peak\":\"scratch.tables\",",
            "\"total_connections_peak\":\"active_connections_plus_scratch.tables\"}},",
            "\"wins_24_pass\":{},\"wins_96_pass\":{},\"fixed_cost_pass\":{},",
            "\"p95_relative_pass\":{},\"higher_absolute_class\":{},",
            "\"absolute_classes\":{},\"primary_450_class_pass\":{},",
            "\"primary_nonmaterial_microvariance\":{},",
            "\"model\":{{\"fitted_fixed_ns\":{},",
            "\"fitted_bandwidth_mib_s\":{},\"valid\":{}}},",
            "\"cpu_scaling_pass\":{},\"cpu_regression_pass\":{},",
            "\"no_resource_regression\":{},\"no_sync_regression\":{},",
            "\"no_residue_regression\":{},\"preferred_wall_pass\":{},",
            "\"hard_wall_pass\":true,\"campaign_wall_ns\":{},",
            "\"setup_wall_ns\":{},\"command_wall_sum_ns\":{},",
            "\"coordinator_wall_ns\":{},\"campaign_wall_equation_exact\":true,",
            "\"populations\":[{}]}}\n"
        ),
        disposition.status,
        disposition.wins24,
        disposition.wins96,
        disposition.wins24 >= 3,
        disposition.wins96 >= 3,
        disposition.fixed_cost_pass,
        disposition.p95_relative_pass,
        disposition.higher_absolute_class,
        disposition.absolute_classes_json,
        disposition.primary_class_pass,
        disposition.primary_nonmaterial_microvariance,
        disposition.fitted_fixed_ns,
        disposition.fitted_bandwidth_mib_s,
        disposition.model_valid,
        disposition.cpu_scaling_pass,
        disposition.cpu_regression_pass,
        disposition.no_resource_regression,
        disposition.no_sync_regression,
        disposition.no_residue_regression,
        campaign_wall_ns < 15_000_000_000,
        campaign_wall_ns,
        setup_wall_ns,
        command_wall_sum_ns,
        coordinator_wall_ns,
        populations,
    ))
}

fn attribution_campaign_failure(run: &Path, block: usize, reason: &str) -> EvalResult<()> {
    append_sync(
        &run.join("failure-ledger.json"),
        &format!(
            "{{\"sequence\":2,\"state\":\"FAIL\",\"block\":{block},\"reason\":\"{}\"}}",
            json_escape(reason)
        ),
    )?;
    Err(format!(
        "attribution campaign stopped at block {block}: {reason}"
    ))
}

fn copy_attribution_manifests(run: &Path, fixture: &Path) -> EvalResult<()> {
    let target = fixture
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "fixture is not below the target directory".to_owned())?;
    let source = target.join("layerfs-stage1m-custody/source-manifests");
    for name in [
        "source-manifest-historical.json",
        "source-manifest-historical-harness.json",
        "source-manifest-control.json",
    ] {
        durable_write(
            &run.join(name),
            &fs::read(source.join(name)).map_err(io_error)?,
        )?;
    }
    durable_write(
        &run.join("source-manifest-candidate.json"),
        b"{\"status\":\"NotApplicable\",\"reason\":\"candidate does not exist during M2 control attribution\"}\n",
    )
}

fn enrich_attribution_row(
    row: &str,
    sequence: u64,
    block: usize,
    executable_sha256: &str,
    schedule_blake3: &str,
    command_wall_ns: u128,
) -> EvalResult<String> {
    let body = row
        .strip_suffix('}')
        .ok_or_else(|| "child row is not a JSON object".to_owned())?;
    Ok(format!(
        "{body},\"sequence\":{sequence},\"block\":{block},\"executable_sha256\":\"{executable_sha256}\",\"schedule_blake3\":\"{schedule_blake3}\",\"command_wall_ns\":{command_wall_ns}}}"
    ))
}

fn validate_attribution_json(row: &str) -> EvalResult<()> {
    for exact in [
        "\"status\":\"PASS\"",
        "\"engine_sql_exact\":true",
        "\"scratch_sql_exact\":true",
        "\"fetched_auth_decode_exact\":true",
        "\"trust_work_exact\":true",
        "\"resource_gates_pass\":true",
        "\"byte_equations_pass\":true",
    ] {
        if !row.contains(exact) {
            return Err(format!("attribution row does not prove {exact}"));
        }
    }
    validate_row_wall_json(row)
}

fn validate_row_wall_json(row: &str) -> EvalResult<()> {
    let product = json_u128(row, "product_operation_wall_ns")?;
    let oracle = json_u128(row, "oracle_wall_ns")?;
    let cleanup = json_u128(row, "cleanup_wall_ns")?;
    let expected_row_wall = product
        .checked_add(oracle)
        .and_then(|value| value.checked_add(cleanup))
        .ok_or_else(|| "attribution row wall overflow".to_owned())?;
    let residual = json_i128(row, "row_wall_residual_ns")?;
    let observed_row_wall =
        i128::try_from(json_u128(row, "row_wall_ns")?).map_err(display_error)?;
    let expected_row_wall = i128::try_from(expected_row_wall).map_err(display_error)?;
    if residual < 0 || observed_row_wall != expected_row_wall + residual {
        return Err("row wall = product + oracle + cleanup + residual".to_owned());
    }
    Ok(())
}

fn three_stats(values: &[u128]) -> EvalResult<(u128, u128)> {
    if values.len() != 3 {
        return Err("n=3 statistic requires three values".to_owned());
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    Ok((sorted[1], sorted[2]))
}

fn attribution_models_json(
    populations: &[(AttributionArm, u64, u128, u128)],
) -> EvalResult<String> {
    let mut models = Vec::new();
    for arm in [
        AttributionArm::Complete,
        AttributionArm::Null,
        AttributionArm::Digest,
        AttributionArm::Native,
    ] {
        let time = |size| {
            populations
                .iter()
                .find(|(candidate, candidate_size, _, _)| {
                    *candidate == arm && *candidate_size == size
                })
                .map(|(_, _, p50, _)| *p50)
                .ok_or_else(|| format!("missing {} {size} MiB population", arm.name()))
        };
        let t0 = time(0)? as f64;
        let t24 = time(24)? as f64;
        let t96 = time(96)? as f64;
        let slope = (t96 - t24) / 72.0;
        if slope <= 0.0 {
            return Err(format!("{} fitted slope is not positive", arm.name()));
        }
        let modeled24 = t0 + 24.0 * slope;
        let modeled96 = t0 + 96.0 * slope;
        let residual24 = t24 - modeled24;
        let residual96 = t96 - modeled96;
        let valid = residual24.abs() <= 2_000_000_f64.max(t24 * 0.05)
            && residual96.abs() <= 2_000_000_f64.max(t96 * 0.05);
        models.push(format!(
            concat!(
                "{{\"arm\":\"{}\",\"fixed_cost_ns\":{},",
                "\"slope_ns_per_mib\":{},\"sustained_bandwidth_mib_per_s\":{},",
                "\"residual_24_ns\":{},\"residual_96_ns\":{},",
                "\"predicted_t100_ns\":{},\"model_valid\":{}}}"
            ),
            arm.name(),
            t0,
            slope,
            1_000_000_000_f64 / slope,
            residual24,
            residual96,
            t0 + 100.0 * slope,
            valid,
        ));
    }
    Ok(format!("[{}]", models.join(",")))
}

pub fn attribution_block(
    store: &Path,
    source: &Path,
    size_mib: &OsStr,
    arm: &OsStr,
    work: &Path,
    identity: &OsStr,
) -> EvalResult<()> {
    attribution_block_with_mode(
        store,
        source,
        size_mib,
        arm,
        work,
        identity,
        IntegrityMode::Verified,
    )
}

pub fn trusted_block(
    store: &Path,
    source: &Path,
    size_mib: &OsStr,
    work: &Path,
    identity: &OsStr,
) -> EvalResult<()> {
    attribution_block_with_mode(
        store,
        source,
        size_mib,
        OsStr::new("complete"),
        work,
        identity,
        IntegrityMode::TrustedLocalDev,
    )
}

fn attribution_block_with_mode(
    store: &Path,
    source: &Path,
    size_mib: &OsStr,
    arm: &OsStr,
    work: &Path,
    identity: &OsStr,
    mode: IntegrityMode,
) -> EvalResult<()> {
    let size_mib = size_mib
        .to_str()
        .ok_or_else(|| "size-mib is not UTF-8".to_owned())?
        .parse::<u64>()
        .map_err(|error| format!("invalid size-mib: {error}"))?;
    if !matches!(size_mib, 0 | 24 | 96) {
        return Err("size-mib must be exactly 0, 24, or 96".to_owned());
    }
    let arm = AttributionArm::parse(arm)?;
    let identity = ascii_argument(identity, "identity")?;
    let expected_bytes = size_mib
        .checked_mul(1024 * 1024)
        .ok_or_else(|| "fixture byte length overflow".to_owned())?;
    let source_metadata = fs::metadata(source).map_err(io_error)?;
    if !source_metadata.is_file() || source_metadata.len() != expected_bytes {
        return Err("source fixture length mismatch".to_owned());
    }
    let source_digest = digest_file(source)?;
    fs::create_dir(work).map_err(io_error)?;
    let store_clone = work.join("store");
    clone_store(store, &store_clone)?;
    let process_fd_baseline = fd_count()?;
    let opened = LayerFs::open_with_integrity(&store_clone, mode).map_err(display_error)?;
    let root = opened.ref_state.root;

    let primer = AttributionObservation {
        row: run_one(
            &opened.fs,
            root,
            source,
            &source_digest,
            &source_metadata,
            expected_bytes,
            &work.join("primer"),
            process_fd_baseline,
        )?,
        sink_write_calls: 0,
        sink_write_ns: 0,
        digest_sink_hash_bytes: None,
    };
    validate_attribution_observation(AttributionArm::Complete, expected_bytes, &primer, mode)?;
    println!(
        "{}",
        attribution_row_json(
            "warmup",
            arm,
            AttributionArm::Complete,
            0,
            identity,
            size_mib,
            root,
            &source_digest,
            &primer,
            mode,
        )?
    );
    std::io::stdout().flush().map_err(io_error)?;

    let mut measured = Vec::with_capacity(3);
    for ordinal in 1..=3 {
        let observation = run_attribution_one(
            &opened.fs,
            root,
            arm,
            source,
            &source_digest,
            &source_metadata,
            expected_bytes,
            &work.join(format!("measured-{ordinal}")),
            process_fd_baseline,
        )?;
        validate_attribution_observation(arm, expected_bytes, &observation, mode)?;
        measured.push(observation);
    }
    drop(opened);
    fs::remove_dir_all(&store_clone).map_err(io_error)?;
    fs::remove_dir(work).map_err(io_error)?;
    let terminal_fd = fd_count()?;
    let last = measured
        .last_mut()
        .ok_or_else(|| "missing measured rows".to_owned())?;
    last.row.fd_terminal = Some(terminal_fd);
    last.row.connections_terminal = Some(0);
    last.row.scratch_connections_terminal = Some(0);
    last.row.total_connections_terminal = Some(0);
    for (index, observation) in measured.iter().enumerate() {
        println!(
            "{}",
            attribution_row_json(
                "measured",
                arm,
                arm,
                index + 1,
                identity,
                size_mib,
                root,
                &source_digest,
                observation,
                mode,
            )?
        );
    }
    std::io::stdout().flush().map_err(io_error)
}

struct AttributionObservation {
    row: Row,
    sink_write_calls: u64,
    sink_write_ns: u64,
    digest_sink_hash_bytes: Option<u64>,
}

#[allow(clippy::too_many_arguments)]
fn run_attribution_one(
    fs: &LayerFs,
    root: layerfs_sdk::RootId,
    arm: AttributionArm,
    source: &Path,
    source_digest: &str,
    source_metadata: &fs::Metadata,
    expected_bytes: u64,
    destination: &Path,
    process_fd_baseline: u64,
) -> EvalResult<AttributionObservation> {
    if arm == AttributionArm::Complete {
        return Ok(AttributionObservation {
            row: run_one(
                fs,
                root,
                source,
                source_digest,
                source_metadata,
                expected_bytes,
                destination,
                process_fd_baseline,
            )?,
            sink_write_calls: 0,
            sink_write_ns: 0,
            digest_sink_hash_bytes: None,
        });
    }
    if destination.exists() {
        return Err(format!(
            "fresh destination already exists: {}",
            destination.display()
        ));
    }
    let row_started = Instant::now();
    let before = fs.counter_snapshot().map_err(display_error)?;
    let projection_before = fs.projection_facts();
    let usage_before = process_usage()?;
    let fd_before = fd_count()?;
    let mut sink = TimedSink::new(arm == AttributionArm::Digest);
    let mut native_source = (arm == AttributionArm::Native)
        .then(|| File::open(source).map_err(io_error))
        .transpose()?;
    let native_metadata = NativeMetadata {
        mode: FIXTURE_MODE,
        mtime_seconds: source_metadata.mtime(),
        mtime_nanoseconds: source_metadata.mtime_nsec() as u32,
        xattrs: NativeXattrs::new(),
        acl: None,
        bsd_flags: 0,
    };
    let product_started = Instant::now();
    let operation = match arm {
        AttributionArm::Null | AttributionArm::Digest => fs
            .materialize_authenticated_to(root, &mut sink)
            .map_err(display_error)?,
        AttributionArm::Native => fs
            .native_durable_output(
                destination,
                b"payload.bin",
                &native_metadata,
                expected_bytes,
                native_source
                    .take()
                    .ok_or_else(|| "native source is unavailable".to_owned())?,
            )
            .map_err(display_error)?,
        AttributionArm::Complete => unreachable!(),
    };
    let product_wall_ns = product_started.elapsed().as_nanos();
    let usage_after = process_usage()?;
    let fd_after = fd_count()?;
    let rss_current_bytes = current_rss_bytes()?;
    let after = fs.counter_snapshot().map_err(display_error)?;
    let engine = EngineDelta::between(&before, &after)?;

    let oracle_started = Instant::now();
    let output_digest = match arm {
        AttributionArm::Null => {
            if sink.bytes != expected_bytes {
                return Err("null sink byte equation failed".to_owned());
            }
            "NotApplicable".to_owned()
        }
        AttributionArm::Digest => {
            let digest = sink.digest()?;
            if sink.bytes != expected_bytes || digest != source_digest {
                return Err("digest sink oracle failed".to_owned());
            }
            digest
        }
        AttributionArm::Native => verify_native_destination(
            fs,
            destination,
            source_digest,
            &native_metadata,
            expected_bytes,
        )?,
        AttributionArm::Complete => unreachable!(),
    };
    let oracle_wall_ns = oracle_started.elapsed().as_nanos();
    let cleanup_started = Instant::now();
    if destination.exists() {
        fs::remove_dir_all(destination).map_err(io_error)?;
    }
    let cleanup_wall_ns = cleanup_started.elapsed().as_nanos();
    if destination.exists() {
        return Err("attribution cleanup left residue".to_owned());
    }
    let projection_total = fs
        .projection_facts()
        .checked_delta(projection_before)
        .ok_or_else(|| "projection facts moved backwards".to_owned())?;
    let scratch_connections_peak = operation.scratch_tables;
    let total_connections_peak = after
        .active_connections
        .checked_add(scratch_connections_peak)
        .ok_or_else(|| "peak connection count overflow".to_owned())?;
    let row_wall_ns = row_started.elapsed().as_nanos();
    Ok(AttributionObservation {
        row: Row {
            product_wall_ns,
            row_wall_ns,
            oracle_wall_ns,
            cleanup_wall_ns,
            output_digest,
            engine,
            operation,
            user_cpu_ns: usage_after
                .user_ns
                .checked_sub(usage_before.user_ns)
                .ok_or_else(|| "user CPU moved backwards".to_owned())?,
            system_cpu_ns: usage_after
                .system_ns
                .checked_sub(usage_before.system_ns)
                .ok_or_else(|| "system CPU moved backwards".to_owned())?,
            rss_peak_bytes: usage_after.maximum_rss_bytes,
            rss_current_bytes,
            fd_before,
            fd_after,
            active_connections: after.active_connections,
            scratch_connections_current: 0,
            scratch_connections_peak,
            total_connections_current: after.active_connections,
            total_connections_peak,
            projection_total,
            fd_terminal: None,
            connections_terminal: None,
            scratch_connections_terminal: None,
            total_connections_terminal: None,
            process_fd_baseline,
        },
        sink_write_calls: sink.write_calls,
        sink_write_ns: sink.write_ns,
        digest_sink_hash_bytes: (arm == AttributionArm::Digest).then_some(sink.bytes),
    })
}

fn verify_native_destination(
    fs: &LayerFs,
    destination: &Path,
    source_digest: &str,
    expected_metadata: &NativeMetadata,
    expected_bytes: u64,
) -> EvalResult<String> {
    let output = destination.join("payload.bin");
    let metadata = fs::metadata(&output).map_err(io_error)?;
    if !metadata.is_file() || metadata.len() != expected_bytes {
        return Err("native durable output length mismatch".to_owned());
    }
    let digest = digest_file(&output)?;
    if digest != source_digest {
        return Err("native durable output digest mismatch".to_owned());
    }
    let external = fs.open_external(destination).map_err(display_error)?;
    let actual = external
        .read_metadata("payload.bin")
        .map_err(display_error)?;
    if &actual != expected_metadata {
        return Err("native durable output metadata mismatch".to_owned());
    }
    drop(external);
    Ok(digest)
}

struct TimedSink {
    hasher: Option<blake3::Hasher>,
    bytes: u64,
    write_calls: u64,
    write_ns: u64,
}

impl TimedSink {
    fn new(digest: bool) -> Self {
        Self {
            hasher: digest.then(blake3::Hasher::new),
            bytes: 0,
            write_calls: 0,
            write_ns: 0,
        }
    }

    fn digest(&self) -> EvalResult<String> {
        self.hasher
            .as_ref()
            .map(|hasher| hasher.clone().finalize().to_hex().to_string())
            .ok_or_else(|| "digest requested from null sink".to_owned())
    }
}

impl Write for TimedSink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let started = Instant::now();
        if let Some(hasher) = self.hasher.as_mut() {
            hasher.update(bytes);
        } else {
            std::hint::black_box(bytes);
        }
        self.write_ns = self
            .write_ns
            .checked_add(started.elapsed().as_nanos() as u64)
            .ok_or_else(|| std::io::Error::other("sink timer overflow"))?;
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| std::io::Error::other("sink byte overflow"))?;
        self.write_calls = self
            .write_calls
            .checked_add(1)
            .ok_or_else(|| std::io::Error::other("sink call overflow"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn validate_attribution_observation(
    arm: AttributionArm,
    expected_bytes: u64,
    observation: &AttributionObservation,
    mode: IntegrityMode,
) -> EvalResult<()> {
    let row = &observation.row;
    let operation = &row.operation;
    let engine_sql = engine_sql(&row.engine)?;
    let scratch_sql = scratch_sql(operation)?;
    let content_bytes = operation
        .content_payload_bytes_read()
        .ok_or_else(|| "content payload accounting underflow".to_owned())?;
    let trust_exact = trust_equation(mode, &row.engine);
    let common = engine_sql == row.engine.statements
        && scratch_sql == operation.scratch_statements
        && successful_projection_facts_exact(operation.projection)
        && successful_projection_facts_exact(row.projection_total)
        && trust_exact
        && row.engine.busy_events == 0
        && row.engine.locked_events == 0
        && row.engine.publication_commits == 0
        && operation.rope.cdc_bytes_scanned == 0
        && operation.rematerializations == 0
        && operation.full_fallback_files == 0
        && operation.operation_q_high_water_bytes < 8 * 1024 * 1024
        && operation.operation_q_terminal_bytes == 0
        && operation.owned_temp_terminal == 0
        && operation.descriptor_spool_bytes_terminal == 0
        && row.active_connections == 1
        && row.scratch_connections_peak <= 1
        && row.total_connections_peak <= 2
        && row.total_connections_current
            == row
                .active_connections
                .checked_add(row.scratch_connections_current)
                .ok_or_else(|| "current connection equation overflow".to_owned())?
        && row.fd_before <= 24
        && row.fd_after <= 24
        && row.rss_peak_bytes <= 32 * 1024 * 1024
        && row.rss_current_bytes <= 32 * 1024 * 1024
        && row.engine.payload_batch_maximum <= 64;
    let arm_exact = match arm {
        AttributionArm::Complete => {
            content_bytes == expected_bytes
                && operation.native.bytes_written == expected_bytes
                && operation.projection.content_write.bytes == expected_bytes
                && observation.digest_sink_hash_bytes.is_none()
        }
        AttributionArm::Null => {
            content_bytes == expected_bytes
                && operation.native == Default::default()
                && operation.projection.content_write.attempts == 0
                && observation.digest_sink_hash_bytes.is_none()
                && (expected_bytes == 0 || observation.sink_write_calls > 0)
        }
        AttributionArm::Digest => {
            content_bytes == expected_bytes
                && operation.native == Default::default()
                && operation.projection.content_write.attempts == 0
                && observation.digest_sink_hash_bytes == Some(expected_bytes)
                && (expected_bytes == 0 || observation.sink_write_calls > 0)
        }
        AttributionArm::Native => {
            content_bytes == 0
                && row.engine.fetched_rows == 0
                && operation.scratch_statements == 0
                && operation.native.bytes_written == expected_bytes
                && operation.projection.content_write.bytes == expected_bytes
                && observation.digest_sink_hash_bytes.is_none()
        }
    };
    if !common || !arm_exact {
        return Err(format!(
            "{} attribution equation failed (common={common}, arm={arm_exact})",
            arm.name()
        ));
    }
    let (leaf_ns, _, residual_ns) = attribution_timer_equation(arm, observation)?;
    let tolerance = 500_000_u128.max(row.product_wall_ns / 100);
    if residual_ns.unsigned_abs() > tolerance || leaf_ns == 0 && row.product_wall_ns != 0 {
        return Err(format!(
            "{} attribution timer equation failed: residual={residual_ns}",
            arm.name()
        ));
    }
    Ok(())
}

fn trust_equation(mode: IntegrityMode, engine: &EngineDelta) -> bool {
    engine.fetched_rows == engine.role_decode_passes
        && match mode {
            IntegrityMode::Verified => engine.fetched_rows == engine.authentication_passes,
            IntegrityMode::TrustedLocalDev => {
                engine.authentication_passes == 0 && engine.identity_authentication_ns == 0
            }
        }
}

#[allow(clippy::too_many_arguments)]
fn attribution_row_json(
    row_kind: &str,
    block_arm: AttributionArm,
    observed_arm: AttributionArm,
    measured_ordinal: usize,
    identity: &str,
    size_mib: u64,
    root: layerfs_sdk::RootId,
    source_digest: &str,
    observation: &AttributionObservation,
    mode: IntegrityMode,
) -> EvalResult<String> {
    let row = &observation.row;
    let operation = &row.operation;
    let expected_bytes = size_mib * 1024 * 1024;
    let content_bytes = operation
        .content_payload_bytes_read()
        .ok_or_else(|| "content payload accounting underflow".to_owned())?;
    let metadata_bytes = operation.metadata_rope.payload_bytes_read;
    let engine_sql = engine_sql(&row.engine)?;
    let scratch_sql = scratch_sql(operation)?;
    let trust_exact = trust_equation(mode, &row.engine);
    let byte_equations_pass = match observed_arm {
        AttributionArm::Complete | AttributionArm::Null | AttributionArm::Digest => {
            content_bytes == expected_bytes
                && operation.rope.payload_bytes_read
                    == content_bytes
                        .checked_add(metadata_bytes)
                        .ok_or_else(|| "payload byte equation overflow".to_owned())?
        }
        AttributionArm::Native => {
            content_bytes == 0 && operation.native.bytes_written == expected_bytes
        }
    };
    let resource_gates_pass = operation.operation_q_high_water_bytes < 8 * 1024 * 1024
        && operation.operation_q_terminal_bytes == 0
        && row.scratch_connections_peak <= 1
        && row.total_connections_peak <= 2
        && row.total_connections_current
            == row
                .active_connections
                .checked_add(row.scratch_connections_current)
                .ok_or_else(|| "current connection equation overflow".to_owned())?
        && row.fd_before <= 24
        && row.fd_after <= 24
        && row.rss_peak_bytes <= 32 * 1024 * 1024
        && row.rss_current_bytes <= 32 * 1024 * 1024;
    let (leaf_ns, vfs_dispatch_ns, operation_residual_ns) =
        attribution_timer_equation(observed_arm, observation)?;
    let digest_fact = observation.digest_sink_hash_bytes.map_or_else(
        || "{\"applicability\":\"NotApplicable\"}".to_owned(),
        |bytes| format!("{{\"applicability\":\"Applicable\",\"bytes\":{bytes}}}"),
    );
    let source_applicability = if observed_arm == AttributionArm::Native {
        "NotApplicable"
    } else {
        "Applicable"
    };
    let native_applicability = if matches!(
        observed_arm,
        AttributionArm::Complete | AttributionArm::Native
    ) {
        "Applicable"
    } else {
        "NotApplicable"
    };
    let projection_through_row =
        if matches!(observed_arm, AttributionArm::Null | AttributionArm::Digest) {
            "{\"applicability\":\"NotApplicable\"}".to_owned()
        } else {
            format!(
                "{{\"applicability\":\"Applicable\",\"facts\":{}}}",
                projection_json(row.projection_total)
            )
        };
    let materialize_inclusive = if observed_arm == AttributionArm::Complete {
        format!(
            "{{\"applicability\":\"Applicable\",\"nanoseconds\":{}}}",
            operation.materialize_inclusive_ns
        )
    } else {
        "{\"applicability\":\"NotApplicable\"}".to_owned()
    };
    let payload_batch_maximum = if observed_arm == AttributionArm::Native {
        "{\"applicability\":\"NotApplicable\",\"value\":0}".to_owned()
    } else {
        format!(
            "{{\"applicability\":\"Applicable\",\"value\":{}}}",
            row.engine.payload_batch_maximum
        )
    };
    let operation_label = match (mode, row_kind) {
        (IntegrityMode::TrustedLocalDev, "warmup") => {
            "trusted_localdev_first_open_fresh_destination"
        }
        (IntegrityMode::TrustedLocalDev, _) => {
            "trusted_localdev_same_open_warmed_source_fresh_destination"
        }
        (IntegrityMode::Verified, "warmup") => "first_open_fresh_destination",
        (IntegrityMode::Verified, _) => observed_arm.operation_label(),
    };
    let source_conditioning = match (mode, row_kind) {
        (IntegrityMode::TrustedLocalDev, "warmup") => "explicit_trusted_open",
        (IntegrityMode::TrustedLocalDev, _) => "same_trusted_open_after_primer",
        (IntegrityMode::Verified, "warmup") => "fresh_open_after_scrub",
        (IntegrityMode::Verified, _) => "same_open_after_primer",
    };
    let authenticated_bytes = if mode == IntegrityMode::Verified {
        row.engine.object_bytes_read
    } else {
        0
    };
    let mode_label = match mode {
        IntegrityMode::Verified => "Verified",
        IntegrityMode::TrustedLocalDev => "TrustedLocalDev",
    };
    let named_row_wall_ns = row
        .product_wall_ns
        .checked_add(row.oracle_wall_ns)
        .and_then(|value| value.checked_add(row.cleanup_wall_ns))
        .ok_or_else(|| "row wall overflow".to_owned())?;
    let row_wall_residual_ns = i128::try_from(row.row_wall_ns).map_err(display_error)?
        - i128::try_from(named_row_wall_ns).map_err(display_error)?;
    Ok(format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1m-attribution-row-v2\",\"status\":\"PASS\",",
            "\"integrity_mode\":\"{}\",\"row_kind\":\"{}\",\"block_identity\":\"{}\",",
            "\"requested_arm\":\"{}\",\"executed_arm\":\"{}\",",
            "\"measured_ordinal\":{},\"operation_label\":\"{}\",",
            "\"source_conditioning\":\"{}\",\"controlled_device_cold\":false,",
            "\"incremental_refresh\":false,\"size_mib\":{},\"logical_bytes\":{},",
            "\"root\":\"{}\",\"source_digest\":\"{}\",\"output_digest\":\"{}\",",
            "\"oracle\":{{\"status\":\"PASS\",\"kind\":\"{}\"}},",
            "\"product_operation_wall_ns\":{},\"row_wall_ns\":{},\"oracle_wall_ns\":{},",
            "\"cleanup_wall_ns\":{},\"source\":{{\"applicability\":\"{}\",",
            "\"content_payload_bytes\":{},\"metadata_payload_bytes\":{},",
            "\"canonical_bytes_authenticated\":{},\"identity_hash_bytes\":{},",
            "\"sink_write_calls\":{},",
            "\"sink_write_ns\":{},\"digest_sink_hash\":{}}},",
            "\"native_applicability\":\"{}\",\"engine\":{},\"scratch\":{},",
            "\"projection\":{},\"projection_through_row\":{},",
            "\"native\":{{\"bytes_written\":{},\"temp_calls\":{},",
            "\"sync_calls\":{},\"replace_calls\":{},\"metadata_calls\":{}}},",
            "\"resources\":{{\"user_cpu_ns\":{},\"system_cpu_ns\":{},",
            "\"rss_peak_bytes\":{},\"rss_current_bytes\":{},",
            "\"process_fd_baseline\":{},\"fd_before\":{},\"fd_after\":{},",
            "\"active_connections\":{},\"primary_connections_current\":{},",
            "\"scratch_connections_current\":{},\"scratch_connections_peak\":{},",
            "\"total_connections_current\":{},\"total_connections_peak\":{},",
            "\"fd_terminal\":{},\"connections_terminal\":{},",
            "\"primary_connections_terminal\":{},\"scratch_connections_terminal\":{},",
            "\"total_connections_terminal\":{},\"operation_q_high_water_bytes\":{},",
            "\"operation_q_terminal_bytes\":{}}},",
            "\"equations\":{{\"engine_sql_sum\":{},\"engine_sql_exact\":{},",
            "\"scratch_sql_sum\":{},\"scratch_sql_exact\":{},",
            "\"fetched_auth_decode_exact\":{},\"trust_work_exact\":{},",
            "\"byte_equations_pass\":{},\"resource_gates_pass\":{},",
            "\"canonical_store_writer_transactions\":0,\"publication_commits\":{},",
            "\"canonical_cdc_bytes\":{},\"store_id_queries\":{},",
            "\"payload_batch_maximum\":{},\"materialize_inclusive\":{},",
            "\"payload_callback_timer_class\":\"inclusive_report_only\",",
            "\"exclusive_leaf_ns\":{},",
            "\"vfs_dispatch_ns\":{},\"operation_residual_ns\":{},",
            "\"row_wall_residual_ns\":{}}},",
            "\"operation_q_terminal_bytes\":{},\"residue\":0}}"
        ),
        mode_label,
        row_kind,
        identity,
        block_arm.name(),
        observed_arm.name(),
        measured_ordinal,
        operation_label,
        source_conditioning,
        size_mib,
        expected_bytes,
        root,
        source_digest,
        row.output_digest,
        match observed_arm {
            AttributionArm::Complete => "exact_public_complete",
            AttributionArm::Null => "exact_source_byte_equation",
            AttributionArm::Digest => "exact_source_digest",
            AttributionArm::Native => "exact_native_bytes_metadata",
        },
        row.product_wall_ns,
        row.row_wall_ns,
        row.oracle_wall_ns,
        row.cleanup_wall_ns,
        source_applicability,
        content_bytes,
        metadata_bytes,
        authenticated_bytes,
        authenticated_bytes,
        observation.sink_write_calls,
        observation.sink_write_ns,
        digest_fact,
        native_applicability,
        engine_delta_json(&row.engine),
        scratch_observation_json(operation),
        projection_json(operation.projection),
        projection_through_row,
        operation.native.bytes_written,
        operation.native.temp_calls,
        operation.native.sync_calls,
        operation.native.replace_calls,
        operation.native.metadata_calls,
        row.user_cpu_ns,
        row.system_cpu_ns,
        row.rss_peak_bytes,
        row.rss_current_bytes,
        row.process_fd_baseline,
        row.fd_before,
        row.fd_after,
        row.active_connections,
        row.active_connections,
        row.scratch_connections_current,
        row.scratch_connections_peak,
        row.total_connections_current,
        row.total_connections_peak,
        json_optional_u64(row.fd_terminal),
        json_optional_u64(row.connections_terminal),
        json_optional_u64(row.connections_terminal),
        json_optional_u64(row.scratch_connections_terminal),
        json_optional_u64(row.total_connections_terminal),
        operation.operation_q_high_water_bytes,
        operation.operation_q_terminal_bytes,
        engine_sql,
        engine_sql == row.engine.statements,
        scratch_sql,
        scratch_sql == operation.scratch_statements,
        trust_exact,
        trust_exact && row.engine.busy_events == 0 && row.engine.locked_events == 0,
        byte_equations_pass,
        resource_gates_pass,
        row.engine.publication_commits,
        operation.rope.cdc_bytes_scanned,
        row.engine.store_id_queries,
        payload_batch_maximum,
        materialize_inclusive,
        leaf_ns,
        vfs_dispatch_ns,
        operation_residual_ns,
        row_wall_residual_ns,
        operation.operation_q_terminal_bytes,
    ))
}

fn engine_delta_json(engine: &EngineDelta) -> String {
    format!(
        concat!(
            "{{\"statements\":{},\"integrity_statements\":{},\"busy_events\":{},",
            "\"locked_events\":{},\"fetched_rows\":{},\"authentication_passes\":{},",
            "\"role_decode_passes\":{},\"object_bytes_read\":{},",
            "\"payload_batch_queries\":{},\"payload_batch_references\":{},",
            "\"payload_batch_maximum\":{},\"publication_commits\":{},",
            "\"publication_statements\":{},\"live_verified_integrity_statements\":{},",
            "\"primary_read_statements\":{},\"reconciliation_statements\":{},",
            "\"compaction_statements\":{},\"connection_mutex_wait_ns\":{},",
            "\"trust_guard_ns\":{},\"nonpayload_query_ns\":{},",
            "\"payload_query_ns\":{},\"identity_authentication_ns\":{},",
            "\"role_decode_ns\":{},\"payload_callback_inclusive_ns\":{},",
            "\"counter_merge_ns\":{},\"store_id_queries\":{}}}"
        ),
        engine.statements,
        engine.integrity_statements,
        engine.busy_events,
        engine.locked_events,
        engine.fetched_rows,
        engine.authentication_passes,
        engine.role_decode_passes,
        engine.object_bytes_read,
        engine.payload_batch_queries,
        engine.payload_batch_references,
        engine.payload_batch_maximum,
        engine.publication_commits,
        engine.publication_statements,
        engine.live_verified_integrity_statements,
        engine.primary_read_statements,
        engine.reconciliation_statements,
        engine.compaction_statements,
        engine.connection_mutex_wait_ns,
        engine.trust_guard_ns,
        engine.nonpayload_query_ns,
        engine.payload_query_ns,
        engine.identity_authentication_ns,
        engine.role_decode_ns,
        engine.payload_callback_inclusive_ns,
        engine.counter_merge_ns,
        engine.store_id_queries,
    )
}

fn scratch_observation_json(operation: &OperationDiagnostics) -> String {
    format!(
        concat!(
            "{{\"tables\":{},\"statements\":{},\"rows\":{},\"high_water_bytes\":{},",
            "\"owner_setup_statements\":{},\"derived_setup_statements\":{},",
            "\"operation_statements\":{},\"store_reopens\":{},",
            "\"store_inspection_statements\":{},\"store_inspection_wall_ns\":{},",
            "\"setup_wall_ns\":{},\"operation_wall_ns\":{}}}"
        ),
        operation.scratch_tables,
        operation.scratch_statements,
        operation.scratch_rows,
        operation.scratch_high_water_bytes,
        operation.scratch_owner_setup_statements,
        operation.scratch_derived_setup_statements,
        operation.scratch_operation_statements,
        operation.scratch_store_reopens,
        operation.scratch_store_inspection_statements,
        operation.scratch_store_inspection_wall_ns,
        operation.scratch_setup_wall_ns,
        operation.scratch_operation_wall_ns,
    )
}

fn engine_sql(engine: &EngineDelta) -> EvalResult<u64> {
    engine
        .publication_statements
        .checked_add(engine.live_verified_integrity_statements)
        .and_then(|value| value.checked_add(engine.primary_read_statements))
        .and_then(|value| value.checked_add(engine.reconciliation_statements))
        .and_then(|value| value.checked_add(engine.compaction_statements))
        .ok_or_else(|| "Engine SQL equation overflow".to_owned())
}

fn scratch_sql(operation: &OperationDiagnostics) -> EvalResult<u64> {
    operation
        .scratch_owner_setup_statements
        .checked_add(operation.scratch_derived_setup_statements)
        .and_then(|value| value.checked_add(operation.scratch_operation_statements))
        .ok_or_else(|| "scratch SQL equation overflow".to_owned())
}

fn fact_count_exact(attempts: u64, successes: u64, failures: u64) -> bool {
    successes.checked_add(failures) == Some(attempts)
}

fn fact_sum_exact(expected: u64, mut values: impl Iterator<Item = u64>) -> bool {
    values.try_fold(0_u64, u64::checked_add) == Some(expected)
}

fn fact_timer_sum_exact(
    aggregate: ProjectionTimer,
    mut owners: impl Iterator<Item = ProjectionTimer>,
) -> bool {
    owners.try_fold(0_u64, |total, owner| {
        (owner.availability == aggregate.availability)
            .then(|| total.checked_add(owner.nanoseconds))
            .flatten()
    }) == Some(aggregate.nanoseconds)
}

fn sync_fact_exact(fact: ProjectionSyncFacts) -> bool {
    fact_count_exact(fact.attempts, fact.successes, fact.failures)
        && [
            fact.requested.process_crash_reconciled,
            fact.requested.host_crash_ordered,
            fact.requested.device_flush_requested,
            fact.requested.power_loss_qualified,
        ]
        .into_iter()
        .try_fold(0_u64, u64::checked_add)
            == Some(fact.attempts)
        && [
            fact.achieved.process_crash_reconciled,
            fact.achieved.host_crash_ordered,
            fact.achieved.device_flush_requested,
            fact.achieved.power_loss_qualified,
        ]
        .into_iter()
        .try_fold(0_u64, u64::checked_add)
            == Some(fact.successes)
}

fn sync_aggregate_exact(aggregate: ProjectionSyncFacts, owners: &[ProjectionSyncFacts]) -> bool {
    sync_fact_exact(aggregate)
        && owners.iter().copied().all(sync_fact_exact)
        && fact_sum_exact(
            aggregate.attempts,
            owners.iter().map(|owner| owner.attempts),
        )
        && fact_sum_exact(
            aggregate.successes,
            owners.iter().map(|owner| owner.successes),
        )
        && fact_sum_exact(
            aggregate.failures,
            owners.iter().map(|owner| owner.failures),
        )
        && fact_sum_exact(
            aggregate.requested.process_crash_reconciled,
            owners
                .iter()
                .map(|owner| owner.requested.process_crash_reconciled),
        )
        && fact_sum_exact(
            aggregate.requested.host_crash_ordered,
            owners
                .iter()
                .map(|owner| owner.requested.host_crash_ordered),
        )
        && fact_sum_exact(
            aggregate.requested.device_flush_requested,
            owners
                .iter()
                .map(|owner| owner.requested.device_flush_requested),
        )
        && fact_sum_exact(
            aggregate.requested.power_loss_qualified,
            owners
                .iter()
                .map(|owner| owner.requested.power_loss_qualified),
        )
        && fact_sum_exact(
            aggregate.achieved.process_crash_reconciled,
            owners
                .iter()
                .map(|owner| owner.achieved.process_crash_reconciled),
        )
        && fact_sum_exact(
            aggregate.achieved.host_crash_ordered,
            owners.iter().map(|owner| owner.achieved.host_crash_ordered),
        )
        && fact_sum_exact(
            aggregate.achieved.device_flush_requested,
            owners
                .iter()
                .map(|owner| owner.achieved.device_flush_requested),
        )
        && fact_sum_exact(
            aggregate.achieved.power_loss_qualified,
            owners
                .iter()
                .map(|owner| owner.achieved.power_loss_qualified),
        )
        && fact_timer_sum_exact(aggregate.wall, owners.iter().map(|owner| owner.wall))
}

fn write_aggregate_exact(aggregate: ProjectionWriteFacts, owners: &[ProjectionWriteFacts]) -> bool {
    fact_count_exact(aggregate.attempts, aggregate.successes, aggregate.failures)
        && owners
            .iter()
            .all(|owner| fact_count_exact(owner.attempts, owner.successes, owner.failures))
        && fact_sum_exact(
            aggregate.attempts,
            owners.iter().map(|owner| owner.attempts),
        )
        && fact_sum_exact(
            aggregate.successes,
            owners.iter().map(|owner| owner.successes),
        )
        && fact_sum_exact(
            aggregate.failures,
            owners.iter().map(|owner| owner.failures),
        )
        && fact_sum_exact(aggregate.bytes, owners.iter().map(|owner| owner.bytes))
        && fact_timer_sum_exact(aggregate.wall, owners.iter().map(|owner| owner.wall))
}

fn projection_facts_exact(facts: ProjectionFacts) -> bool {
    [
        facts.workspace_setup,
        facts.workspace_root_create_open,
        facts.staging_create_open,
        facts.recovery_marker_create,
        facts.name_preflight,
        facts.temp_create,
        facts.content_flush,
        facts.metadata_validate,
        facts.metadata_apply,
        facts.metadata_preinstall_verify,
        facts.metadata_postinstall_verify,
        facts.root_binding_revalidate,
        facts.authority_completion,
    ]
    .iter()
    .all(|fact| fact_count_exact(fact.attempts, fact.successes, fact.failures))
        && write_aggregate_exact(
            facts.aggregate_native_write,
            &[
                facts.workspace_marker_write,
                facts.content_write,
                facts.metadata_value_write,
            ],
        )
        && sync_aggregate_exact(
            facts.regular_file_sync,
            &[
                facts.recovery_marker_file_sync,
                facts.content_temp_file_sync,
                facts.post_hardlink_file_sync,
            ],
        )
        && sync_aggregate_exact(
            facts.directory_sync,
            &[
                facts.staging_directory_sync,
                facts.root_parent_directory_sync,
                facts.install_parent_directory_sync,
                facts.dirty_tree_directory_sync,
                facts.final_root_directory_sync,
            ],
        )
        && fact_count_exact(
            facts.replace.attempts,
            facts.replace.successes,
            facts.replace.failures,
        )
        && fact_count_exact(
            facts.cleanup.attempts,
            facts.cleanup.successes,
            facts.cleanup.failures,
        )
        && facts.cleanup.residue == facts.cleanup.failures
}

fn successful_projection_facts_exact(facts: ProjectionFacts) -> bool {
    projection_facts_exact(facts) && facts.cleanup.failures == 0 && facts.cleanup.residue == 0
}

fn attribution_timer_equation(
    arm: AttributionArm,
    observation: &AttributionObservation,
) -> EvalResult<(u64, u64, i128)> {
    if arm == AttributionArm::Complete {
        let (leaf_ns, dispatch_ns) = exclusive_leaf_ns(&observation.row)?;
        let residual = i128::try_from(observation.row.product_wall_ns).map_err(display_error)?
            - i128::from(leaf_ns);
        return Ok((leaf_ns, dispatch_ns, residual));
    }
    let wall = u64::try_from(observation.row.product_wall_ns).map_err(display_error)?;
    let named = if matches!(arm, AttributionArm::Null | AttributionArm::Digest) {
        let engine = &observation.row.engine;
        [
            engine.connection_mutex_wait_ns,
            engine.trust_guard_ns,
            engine.nonpayload_query_ns,
            engine.payload_query_ns,
            engine.identity_authentication_ns,
            engine.role_decode_ns,
            engine.counter_merge_ns,
            observation.row.operation.scratch_store_inspection_wall_ns,
            observation.row.operation.scratch_setup_wall_ns,
            observation.row.operation.scratch_operation_wall_ns,
        ]
        .into_iter()
        .try_fold(0_u64, |total, value| total.checked_add(value))
        .ok_or_else(|| "source attribution timer overflow".to_owned())?
    } else {
        projection_leaf_ns(observation.row.operation.projection)?
    };
    let dispatch = wall
        .checked_sub(named)
        .ok_or_else(|| "named attribution timers exceed operation wall".to_owned())?;
    Ok((wall, dispatch, 0))
}

fn projection_leaf_ns(projection: ProjectionFacts) -> EvalResult<u64> {
    [
        timer_ns(projection.workspace_root_create_open.wall)?,
        timer_ns(projection.staging_create_open.wall)?,
        timer_ns(projection.recovery_marker_create.wall)?,
        timer_ns(projection.name_preflight.wall)?,
        timer_ns(projection.temp_create.wall)?,
        timer_ns(projection.workspace_marker_write.wall)?,
        timer_ns(projection.content_write.wall)?,
        timer_ns(projection.metadata_value_write.wall)?,
        timer_ns(projection.content_flush.wall)?,
        timer_ns(projection.metadata_validate.wall)?,
        timer_ns(projection.metadata_apply.wall)?,
        timer_ns(projection.metadata_preinstall_verify.wall)?,
        timer_ns(projection.metadata_postinstall_verify.wall)?,
        timer_ns(projection.root_binding_revalidate.wall)?,
        timer_ns(projection.recovery_marker_file_sync.wall)?,
        timer_ns(projection.content_temp_file_sync.wall)?,
        timer_ns(projection.post_hardlink_file_sync.wall)?,
        timer_ns(projection.staging_directory_sync.wall)?,
        timer_ns(projection.root_parent_directory_sync.wall)?,
        timer_ns(projection.install_parent_directory_sync.wall)?,
        timer_ns(projection.dirty_tree_directory_sync.wall)?,
        timer_ns(projection.final_root_directory_sync.wall)?,
        timer_ns(projection.replace.wall)?,
        timer_ns(projection.cleanup.wall)?,
    ]
    .into_iter()
    .try_fold(0_u64, |total, value| total.checked_add(value))
    .ok_or_else(|| "projection attribution timer overflow".to_owned())
}

pub fn parity_row(
    store: &Path,
    source: &Path,
    size_mib: &OsStr,
    work: &Path,
    identity: &OsStr,
) -> EvalResult<()> {
    let size_mib = size_mib
        .to_str()
        .ok_or_else(|| "size-mib is not UTF-8".to_owned())?
        .parse::<u64>()
        .map_err(|error| format!("invalid size-mib: {error}"))?;
    let identity = identity
        .to_str()
        .filter(|value| {
            !value.is_empty()
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
        .ok_or_else(|| "identity must be a nonempty ASCII identifier".to_owned())?;
    let expected_bytes = size_mib
        .checked_mul(1024 * 1024)
        .ok_or_else(|| "fixture byte length overflow".to_owned())?;
    if !matches!(size_mib, 0 | 24 | 96) {
        return Err("size-mib must be exactly 0, 24, or 96".to_owned());
    }
    let source_metadata = fs::metadata(source).map_err(io_error)?;
    if !source_metadata.is_file() || source_metadata.len() != expected_bytes {
        return Err(format!(
            "source fixture length mismatch: expected {expected_bytes}, got {}",
            source_metadata.len()
        ));
    }
    let source_digest = digest_file(source)?;
    fs::create_dir(work).map_err(io_error)?;
    let store_clone = work.join("store");
    clone_store(store, &store_clone)?;

    let process_fd_baseline = fd_count()?;
    let opened = LayerFs::open(&store_clone).map_err(display_error)?;
    let root = opened.ref_state.root;
    let primer = run_one(
        &opened.fs,
        root,
        source,
        &source_digest,
        &source_metadata,
        expected_bytes,
        &work.join("primer"),
        process_fd_baseline,
    )?;
    print_row("warmup", identity, size_mib, root, &source_digest, &primer)?;
    std::io::stdout().flush().map_err(io_error)?;

    let mut measured = run_one(
        &opened.fs,
        root,
        source,
        &source_digest,
        &source_metadata,
        expected_bytes,
        &work.join("measured"),
        process_fd_baseline,
    )?;
    drop(opened);
    fs::remove_dir_all(&store_clone).map_err(io_error)?;
    fs::remove_dir(work).map_err(io_error)?;
    measured.fd_terminal = Some(fd_count()?);
    measured.connections_terminal = Some(0);
    measured.scratch_connections_terminal = Some(0);
    measured.total_connections_terminal = Some(0);
    print_row(
        "measured",
        identity,
        size_mib,
        root,
        &source_digest,
        &measured,
    )?;
    std::io::stdout().flush().map_err(io_error)?;
    Ok(())
}

fn clone_store(source: &Path, destination: &Path) -> EvalResult<()> {
    let status = Command::new("/bin/cp")
        .arg("-cR")
        .arg(source)
        .arg(destination)
        .status()
        .map_err(io_error)?;
    if !status.success() {
        return Err(format!("APFS Store clone exited {status}"));
    }
    crate::stage1_fixture::make_writable(destination)
}

struct Row {
    product_wall_ns: u128,
    row_wall_ns: u128,
    oracle_wall_ns: u128,
    cleanup_wall_ns: u128,
    output_digest: String,
    engine: EngineDelta,
    operation: OperationDiagnostics,
    user_cpu_ns: u64,
    system_cpu_ns: u64,
    rss_peak_bytes: u64,
    rss_current_bytes: u64,
    fd_before: u64,
    fd_after: u64,
    active_connections: u64,
    scratch_connections_current: u64,
    scratch_connections_peak: u64,
    total_connections_current: u64,
    total_connections_peak: u64,
    projection_total: ProjectionFacts,
    fd_terminal: Option<u64>,
    connections_terminal: Option<u64>,
    scratch_connections_terminal: Option<u64>,
    total_connections_terminal: Option<u64>,
    process_fd_baseline: u64,
}

#[allow(clippy::too_many_arguments)]
fn run_one(
    fs: &LayerFs,
    root: layerfs_sdk::RootId,
    source: &Path,
    source_digest: &str,
    source_metadata: &fs::Metadata,
    expected_bytes: u64,
    destination: &Path,
    process_fd_baseline: u64,
) -> EvalResult<Row> {
    if destination.exists() {
        return Err(format!(
            "fresh destination already exists: {}",
            destination.display()
        ));
    }
    let row_started = Instant::now();
    let before = fs.counter_snapshot().map_err(display_error)?;
    let projection_before = fs.projection_facts();
    let usage_before = process_usage()?;
    let fd_before = fd_count()?;
    let product_started = Instant::now();
    let (mut external, mut operation) = fs
        .materialize_external_observed(root, destination)
        .map_err(display_error)?;
    let product_wall_ns = product_started.elapsed().as_nanos();
    let usage_after = process_usage()?;
    let fd_after = fd_count()?;
    let rss_current_bytes = current_rss_bytes()?;
    let after = fs.counter_snapshot().map_err(display_error)?;
    let engine = EngineDelta::between(&before, &after)?;
    let scratch_connections_current = external.scratch_connection_count();
    let scratch_connections_peak = operation.scratch_tables;
    let total_connections_current = after
        .active_connections
        .checked_add(scratch_connections_current)
        .ok_or_else(|| "current connection count overflow".to_owned())?;
    let total_connections_peak = after
        .active_connections
        .checked_add(scratch_connections_peak)
        .ok_or_else(|| "peak connection count overflow".to_owned())?;

    let oracle_started = Instant::now();
    let output_digest = verify_destination(
        &external,
        source,
        source_digest,
        source_metadata,
        expected_bytes,
    )?;
    let oracle_wall_ns = oracle_started.elapsed().as_nanos();

    let cleanup_started = Instant::now();
    operation = merge_terminal_cleanup(
        operation,
        external.discard_observed().map_err(display_error)?,
    )?;
    drop(external);
    let projection_total = fs
        .projection_facts()
        .checked_delta(projection_before)
        .ok_or_else(|| "projection facts moved backwards".to_owned())?;
    fs::remove_dir_all(destination).map_err(io_error)?;
    let cleanup_wall_ns = cleanup_started.elapsed().as_nanos();
    if destination.exists() {
        return Err("destination cleanup left residue".to_owned());
    }
    let row_wall_ns = row_started.elapsed().as_nanos();
    Ok(Row {
        product_wall_ns,
        row_wall_ns,
        oracle_wall_ns,
        cleanup_wall_ns,
        output_digest,
        engine,
        operation,
        user_cpu_ns: usage_after
            .user_ns
            .checked_sub(usage_before.user_ns)
            .ok_or_else(|| "user CPU moved backwards".to_owned())?,
        system_cpu_ns: usage_after
            .system_ns
            .checked_sub(usage_before.system_ns)
            .ok_or_else(|| "system CPU moved backwards".to_owned())?,
        rss_peak_bytes: usage_after.maximum_rss_bytes,
        rss_current_bytes,
        fd_before,
        fd_after,
        active_connections: after.active_connections,
        scratch_connections_current,
        scratch_connections_peak,
        total_connections_current,
        total_connections_peak,
        projection_total,
        fd_terminal: None,
        connections_terminal: None,
        scratch_connections_terminal: None,
        total_connections_terminal: None,
        process_fd_baseline,
    })
}

fn merge_terminal_cleanup(
    operation: OperationDiagnostics,
    cleanup: OperationDiagnostics,
) -> EvalResult<OperationDiagnostics> {
    operation.merge(cleanup).map_err(display_error)
}

fn verify_destination(
    external: &ExternalWorkspace,
    source: &Path,
    source_digest: &str,
    source_metadata: &fs::Metadata,
    expected_bytes: u64,
) -> EvalResult<String> {
    let output = external.path().join(FILE_PATH);
    let output_metadata = fs::metadata(&output).map_err(io_error)?;
    if !output_metadata.is_file() || output_metadata.len() != expected_bytes {
        return Err("materialized output length mismatch".to_owned());
    }
    let output_digest = digest_file(&output)?;
    if output_digest != source_digest || digest_file(source)? != source_digest {
        return Err("materialized output digest mismatch".to_owned());
    }
    let native = external.read_metadata(FILE_PATH).map_err(display_error)?;
    if native.mode != FIXTURE_MODE
        || native.mtime_seconds != source_metadata.mtime()
        || native.mtime_nanoseconds != source_metadata.mtime_nsec() as u32
        || !native.xattrs.is_empty()
        || native.acl.is_some()
        || native.bsd_flags != 0
    {
        return Err("materialized output metadata mismatch".to_owned());
    }
    let data = external.path().join("data");
    if fs::read_dir(external.path())
        .map_err(io_error)?
        .filter_map(Result::ok)
        .count()
        != 1
        || fs::read_dir(data)
            .map_err(io_error)?
            .filter_map(Result::ok)
            .count()
            != 1
    {
        return Err("materialized destination contains extra user entries".to_owned());
    }
    Ok(output_digest)
}

fn digest_file(path: &Path) -> EvalResult<String> {
    let mut file = File::open(path).map_err(io_error)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    loop {
        let count = file.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[derive(Clone, Copy, Default)]
struct EngineDelta {
    statements: u64,
    integrity_statements: u64,
    busy_events: u64,
    locked_events: u64,
    fetched_rows: u64,
    authentication_passes: u64,
    role_decode_passes: u64,
    object_bytes_read: u64,
    payload_batch_queries: u64,
    payload_batch_references: u64,
    payload_batch_maximum: u64,
    publication_commits: u64,
    publication_statements: u64,
    live_verified_integrity_statements: u64,
    primary_read_statements: u64,
    reconciliation_statements: u64,
    compaction_statements: u64,
    connection_mutex_wait_ns: u64,
    trust_guard_ns: u64,
    nonpayload_query_ns: u64,
    payload_query_ns: u64,
    identity_authentication_ns: u64,
    role_decode_ns: u64,
    payload_callback_inclusive_ns: u64,
    counter_merge_ns: u64,
    store_id_queries: u64,
}

impl EngineDelta {
    fn between(before: &Diagnostics, after: &Diagnostics) -> EvalResult<Self> {
        let payload_batch_queries = delta(
            after.payload_batch_queries,
            before.payload_batch_queries,
            "payload_batch_queries",
        )?;
        Ok(Self {
            statements: delta(after.statements, before.statements, "statements")?,
            integrity_statements: delta(
                after.integrity_statements,
                before.integrity_statements,
                "integrity_statements",
            )?,
            busy_events: delta(after.busy_events, before.busy_events, "busy_events")?,
            locked_events: delta(after.locked_events, before.locked_events, "locked_events")?,
            fetched_rows: delta(after.fetched_rows, before.fetched_rows, "fetched_rows")?,
            authentication_passes: delta(
                after.fetched_row_authentication_passes,
                before.fetched_row_authentication_passes,
                "authentication_passes",
            )?,
            role_decode_passes: delta(
                after.fetched_row_role_decode_passes,
                before.fetched_row_role_decode_passes,
                "role_decode_passes",
            )?,
            object_bytes_read: delta(
                after.object_bytes_read,
                before.object_bytes_read,
                "object_bytes_read",
            )?,
            payload_batch_queries,
            payload_batch_references: delta(
                after.payload_batch_references,
                before.payload_batch_references,
                "payload_batch_references",
            )?,
            payload_batch_maximum: if payload_batch_queries == 0 {
                0
            } else {
                after.payload_batch_maximum
            },
            publication_commits: delta(
                after.publication_commits,
                before.publication_commits,
                "publication_commits",
            )?,
            publication_statements: delta(
                after.publication_statements,
                before.publication_statements,
                "publication_statements",
            )?,
            live_verified_integrity_statements: delta(
                after.live_verified_integrity_statements,
                before.live_verified_integrity_statements,
                "live_verified_integrity_statements",
            )?,
            primary_read_statements: delta(
                after.primary_read_statements,
                before.primary_read_statements,
                "primary_read_statements",
            )?,
            reconciliation_statements: delta(
                after.reconciliation_statements,
                before.reconciliation_statements,
                "reconciliation_statements",
            )?,
            compaction_statements: delta(
                after.compaction_statements,
                before.compaction_statements,
                "compaction_statements",
            )?,
            connection_mutex_wait_ns: delta(
                after.connection_mutex_wait_ns,
                before.connection_mutex_wait_ns,
                "connection_mutex_wait_ns",
            )?,
            trust_guard_ns: delta(
                after.trust_guard_ns,
                before.trust_guard_ns,
                "trust_guard_ns",
            )?,
            nonpayload_query_ns: delta(
                after.nonpayload_query_ns,
                before.nonpayload_query_ns,
                "nonpayload_query_ns",
            )?,
            payload_query_ns: delta(
                after.payload_query_ns,
                before.payload_query_ns,
                "payload_query_ns",
            )?,
            identity_authentication_ns: delta(
                after.identity_authentication_ns,
                before.identity_authentication_ns,
                "identity_authentication_ns",
            )?,
            role_decode_ns: delta(
                after.role_decode_ns,
                before.role_decode_ns,
                "role_decode_ns",
            )?,
            payload_callback_inclusive_ns: delta(
                after.payload_callback_inclusive_ns,
                before.payload_callback_inclusive_ns,
                "payload_callback_inclusive_ns",
            )?,
            counter_merge_ns: delta(
                after.counter_merge_ns,
                before.counter_merge_ns,
                "counter_merge_ns",
            )?,
            store_id_queries: delta(
                after.store_id_queries,
                before.store_id_queries,
                "store_id_queries",
            )?,
        })
    }
}

fn print_row(
    kind: &str,
    identity: &str,
    size_mib: u64,
    root: layerfs_sdk::RootId,
    source_digest: &str,
    row: &Row,
) -> EvalResult<()> {
    let native = row.operation.native;
    let (operation_label, source_conditioning) = if kind == "warmup" {
        ("first_open_fresh_destination", "fresh_open_after_scrub")
    } else {
        (
            "same_open_warmed_source_fresh_destination",
            "same_open_after_primer",
        )
    };
    let engine = format!(
        concat!(
            "{{\"statements\":{},\"integrity_statements\":{},\"busy_events\":{},",
            "\"locked_events\":{},\"fetched_rows\":{},\"authentication_passes\":{},",
            "\"role_decode_passes\":{},\"object_bytes_read\":{},",
            "\"payload_batch_queries\":{},\"payload_batch_references\":{},",
            "\"payload_batch_maximum\":{},\"publication_commits\":{},",
            "\"publication_statements\":{},\"live_verified_integrity_statements\":{},",
            "\"primary_read_statements\":{},\"reconciliation_statements\":{},",
            "\"compaction_statements\":{},\"connection_mutex_wait_ns\":{},",
            "\"trust_guard_ns\":{},\"nonpayload_query_ns\":{},",
            "\"payload_query_ns\":{},\"identity_authentication_ns\":{},",
            "\"role_decode_ns\":{},\"payload_callback_inclusive_ns\":{},",
            "\"counter_merge_ns\":{},\"store_id_queries\":{}}}"
        ),
        row.engine.statements,
        row.engine.integrity_statements,
        row.engine.busy_events,
        row.engine.locked_events,
        row.engine.fetched_rows,
        row.engine.authentication_passes,
        row.engine.role_decode_passes,
        row.engine.object_bytes_read,
        row.engine.payload_batch_queries,
        row.engine.payload_batch_references,
        row.engine.payload_batch_maximum,
        row.engine.publication_commits,
        row.engine.publication_statements,
        row.engine.live_verified_integrity_statements,
        row.engine.primary_read_statements,
        row.engine.reconciliation_statements,
        row.engine.compaction_statements,
        row.engine.connection_mutex_wait_ns,
        row.engine.trust_guard_ns,
        row.engine.nonpayload_query_ns,
        row.engine.payload_query_ns,
        row.engine.identity_authentication_ns,
        row.engine.role_decode_ns,
        row.engine.payload_callback_inclusive_ns,
        row.engine.counter_merge_ns,
        row.engine.store_id_queries,
    );
    let scratch = format!(
        concat!(
            "{{\"tables\":{},\"statements\":{},\"rows\":{},\"high_water_bytes\":{},",
            "\"owner_setup_statements\":{},\"derived_setup_statements\":{},",
            "\"operation_statements\":{},\"store_reopens\":{},",
            "\"store_inspection_statements\":{},\"store_inspection_wall_ns\":{},",
            "\"setup_wall_ns\":{},\"operation_wall_ns\":{}}}"
        ),
        row.operation.scratch_tables,
        row.operation.scratch_statements,
        row.operation.scratch_rows,
        row.operation.scratch_high_water_bytes,
        row.operation.scratch_owner_setup_statements,
        row.operation.scratch_derived_setup_statements,
        row.operation.scratch_operation_statements,
        row.operation.scratch_store_reopens,
        row.operation.scratch_store_inspection_statements,
        row.operation.scratch_store_inspection_wall_ns,
        row.operation.scratch_setup_wall_ns,
        row.operation.scratch_operation_wall_ns,
    );
    let projection = projection_json(row.operation.projection);
    let projection_total = projection_json(row.projection_total);
    let engine_sql = row
        .engine
        .publication_statements
        .checked_add(row.engine.live_verified_integrity_statements)
        .and_then(|value| value.checked_add(row.engine.primary_read_statements))
        .and_then(|value| value.checked_add(row.engine.reconciliation_statements))
        .and_then(|value| value.checked_add(row.engine.compaction_statements))
        .ok_or_else(|| "Engine SQL equation overflow".to_owned())?;
    let scratch_sql = row
        .operation
        .scratch_owner_setup_statements
        .checked_add(row.operation.scratch_derived_setup_statements)
        .and_then(|value| value.checked_add(row.operation.scratch_operation_statements))
        .ok_or_else(|| "scratch SQL equation overflow".to_owned())?;
    let (leaf_ns, vfs_dispatch_ns) = exclusive_leaf_ns(row)?;
    let operation_residual_ns =
        i128::try_from(row.product_wall_ns).map_err(display_error)? - i128::from(leaf_ns);
    println!(
        concat!(
            "{{\"schema\":\"layerfs-stage1m-parity-row-v1\",\"status\":\"PASS\",",
            "\"row_kind\":\"{}\",\"identity\":\"{}\",\"operation_label\":\"{}\",",
            "\"source_conditioning\":\"{}\",\"controlled_device_cold\":false,",
            "\"incremental_refresh\":false,\"size_mib\":{},\"logical_bytes\":{},",
            "\"root\":\"{}\",\"source_digest\":\"{}\",\"output_digest\":\"{}\",",
            "\"product_operation_wall_ns\":{},\"oracle_wall_ns\":{},",
            "\"cleanup_wall_ns\":{},\"engine\":{},\"scratch\":{},",
            "\"projection\":{},\"projection_through_cleanup\":{},",
            "\"native\":{{\"bytes_written\":{},\"temp_calls\":{},\"sync_calls\":{},",
            "\"replace_calls\":{},\"metadata_calls\":{}}},",
            "\"resources\":{{\"user_cpu_ns\":{},\"system_cpu_ns\":{},",
            "\"rss_peak_bytes\":{},\"rss_current_bytes\":{},\"process_fd_baseline\":{},",
            "\"fd_before\":{},",
            "\"fd_after\":{},\"active_connections\":{},",
            "\"scratch_connections_current\":{},\"scratch_connections_peak\":{},",
            "\"total_connections_current\":{},\"total_connections_peak\":{},",
            "\"fd_terminal\":{},\"connections_terminal\":{},",
            "\"scratch_connections_terminal\":{},\"total_connections_terminal\":{},",
            "\"operation_q_high_water_bytes\":{},\"owned_temp_terminal\":{},",
            "\"descriptor_spool_bytes_terminal\":{}}},",
            "\"equations\":{{\"engine_sql_sum\":{},\"engine_sql_exact\":{},",
            "\"scratch_sql_sum\":{},\"scratch_sql_exact\":{},",
            "\"fetched_auth_decode_exact\":{},\"materialize_inclusive_ns\":{},",
            "\"vfs_dispatch_ns\":{},\"exclusive_leaf_ns\":{},",
            "\"operation_residual_ns\":{}}},",
            "\"operation_q_terminal_bytes\":{},\"residue\":0}}"
        ),
        kind,
        identity,
        operation_label,
        source_conditioning,
        size_mib,
        size_mib * 1024 * 1024,
        root,
        source_digest,
        row.output_digest,
        row.product_wall_ns,
        row.oracle_wall_ns,
        row.cleanup_wall_ns,
        engine,
        scratch,
        projection,
        projection_total,
        native.bytes_written,
        native.temp_calls,
        native.sync_calls,
        native.replace_calls,
        native.metadata_calls,
        row.user_cpu_ns,
        row.system_cpu_ns,
        row.rss_peak_bytes,
        row.rss_current_bytes,
        row.process_fd_baseline,
        row.fd_before,
        row.fd_after,
        row.active_connections,
        row.scratch_connections_current,
        row.scratch_connections_peak,
        row.total_connections_current,
        row.total_connections_peak,
        json_optional_u64(row.fd_terminal),
        json_optional_u64(row.connections_terminal),
        json_optional_u64(row.scratch_connections_terminal),
        json_optional_u64(row.total_connections_terminal),
        row.operation.operation_q_high_water_bytes,
        row.operation.owned_temp_terminal,
        row.operation.descriptor_spool_bytes_terminal,
        engine_sql,
        engine_sql == row.engine.statements,
        scratch_sql,
        scratch_sql == row.operation.scratch_statements,
        row.engine.fetched_rows == row.engine.authentication_passes
            && row.engine.fetched_rows == row.engine.role_decode_passes,
        row.operation.materialize_inclusive_ns,
        vfs_dispatch_ns,
        leaf_ns,
        operation_residual_ns,
        row.operation.operation_q_terminal_bytes,
    );
    Ok(())
}

fn json_optional_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn exclusive_leaf_ns(row: &Row) -> EvalResult<(u64, u64)> {
    let projection = row.operation.projection;
    let content_write_ns = timer_ns(projection.content_write.wall)?;
    let mut total = 0_u64;
    for value in [
        row.engine.connection_mutex_wait_ns,
        row.engine.trust_guard_ns,
        row.engine.nonpayload_query_ns,
        row.engine.payload_query_ns,
        row.engine.identity_authentication_ns,
        row.engine.role_decode_ns,
        row.engine.counter_merge_ns,
        row.operation.scratch_store_inspection_wall_ns,
        row.operation.scratch_setup_wall_ns,
        row.operation.scratch_operation_wall_ns,
        timer_ns(projection.workspace_root_create_open.wall)?,
        timer_ns(projection.staging_create_open.wall)?,
        timer_ns(projection.recovery_marker_create.wall)?,
        timer_ns(projection.name_preflight.wall)?,
        timer_ns(projection.temp_create.wall)?,
        timer_ns(projection.workspace_marker_write.wall)?,
        content_write_ns,
        timer_ns(projection.metadata_value_write.wall)?,
        timer_ns(projection.content_flush.wall)?,
        timer_ns(projection.metadata_validate.wall)?,
        timer_ns(projection.metadata_apply.wall)?,
        timer_ns(projection.metadata_preinstall_verify.wall)?,
        timer_ns(projection.metadata_postinstall_verify.wall)?,
        timer_ns(projection.root_binding_revalidate.wall)?,
        timer_ns(projection.recovery_marker_file_sync.wall)?,
        timer_ns(projection.content_temp_file_sync.wall)?,
        timer_ns(projection.post_hardlink_file_sync.wall)?,
        timer_ns(projection.staging_directory_sync.wall)?,
        timer_ns(projection.root_parent_directory_sync.wall)?,
        timer_ns(projection.install_parent_directory_sync.wall)?,
        timer_ns(projection.dirty_tree_directory_sync.wall)?,
        timer_ns(projection.final_root_directory_sync.wall)?,
        timer_ns(projection.replace.wall)?,
        timer_ns(projection.cleanup.wall)?,
    ] {
        total = total
            .checked_add(value)
            .ok_or_else(|| "exclusive timer equation overflow".to_owned())?;
    }
    let vfs_dispatch_ns = row
        .operation
        .materialize_inclusive_ns
        .checked_sub(total)
        .ok_or_else(|| "named children exceed VFS materialization parent".to_owned())?;
    Ok((
        total
            .checked_add(vfs_dispatch_ns)
            .ok_or_else(|| "VFS timer equation overflow".to_owned())?,
        vfs_dispatch_ns,
    ))
}

fn timer_ns(timer: ProjectionTimer) -> EvalResult<u64> {
    match timer.availability {
        ProjectionTimerAvailability::Available => Ok(timer.nanoseconds),
        ProjectionTimerAvailability::Unavailable => {
            Err("required Apple timer unavailable".to_owned())
        }
    }
}

fn projection_json(facts: ProjectionFacts) -> String {
    format!(
        concat!(
            "{{\"workspace_setup\":{},\"workspace_root_create_open\":{},",
            "\"staging_create_open\":{},\"recovery_marker_create\":{},",
            "\"name_preflight\":{},\"temp_create\":{},",
            "\"workspace_marker_write\":{},\"content_write\":{},",
            "\"metadata_value_write\":{},\"aggregate_native_write\":{},",
            "\"content_flush\":{},\"metadata_validate\":{},\"metadata_apply\":{},",
            "\"metadata_preinstall_verify\":{},\"metadata_postinstall_verify\":{},",
            "\"root_binding_revalidate\":{},\"regular_file_sync\":{},",
            "\"directory_sync\":{},\"recovery_marker_file_sync\":{},",
            "\"content_temp_file_sync\":{},\"post_hardlink_file_sync\":{},",
            "\"staging_directory_sync\":{},\"root_parent_directory_sync\":{},",
            "\"install_parent_directory_sync\":{},\"dirty_tree_directory_sync\":{},",
            "\"final_root_directory_sync\":{},\"replace\":{},",
            "\"authority_completion\":{},\"cleanup\":{}}}"
        ),
        call_json(facts.workspace_setup),
        call_json(facts.workspace_root_create_open),
        call_json(facts.staging_create_open),
        call_json(facts.recovery_marker_create),
        call_json(facts.name_preflight),
        call_json(facts.temp_create),
        write_json(facts.workspace_marker_write),
        write_json(facts.content_write),
        write_json(facts.metadata_value_write),
        write_json(facts.aggregate_native_write),
        call_json(facts.content_flush),
        call_json(facts.metadata_validate),
        call_json(facts.metadata_apply),
        call_json(facts.metadata_preinstall_verify),
        call_json(facts.metadata_postinstall_verify),
        call_json(facts.root_binding_revalidate),
        sync_json(facts.regular_file_sync),
        sync_json(facts.directory_sync),
        sync_json(facts.recovery_marker_file_sync),
        sync_json(facts.content_temp_file_sync),
        sync_json(facts.post_hardlink_file_sync),
        sync_json(facts.staging_directory_sync),
        sync_json(facts.root_parent_directory_sync),
        sync_json(facts.install_parent_directory_sync),
        sync_json(facts.dirty_tree_directory_sync),
        sync_json(facts.final_root_directory_sync),
        replace_json(facts.replace),
        call_json(facts.authority_completion),
        cleanup_json(facts.cleanup),
    )
}

fn timer_json(timer: ProjectionTimer) -> String {
    match timer.availability {
        ProjectionTimerAvailability::Available => format!(
            "{{\"availability\":\"Available\",\"nanoseconds\":{}}}",
            timer.nanoseconds
        ),
        ProjectionTimerAvailability::Unavailable => {
            "{\"availability\":\"Unavailable\",\"nanoseconds\":null}".to_owned()
        }
    }
}

fn call_json(facts: ProjectionCallFacts) -> String {
    format!(
        "{{\"attempts\":{},\"successes\":{},\"failures\":{},\"wall\":{}}}",
        facts.attempts,
        facts.successes,
        facts.failures,
        timer_json(facts.wall),
    )
}

fn write_json(facts: ProjectionWriteFacts) -> String {
    format!(
        "{{\"attempts\":{},\"successes\":{},\"failures\":{},\"bytes\":{},\"wall\":{}}}",
        facts.attempts,
        facts.successes,
        facts.failures,
        facts.bytes,
        timer_json(facts.wall),
    )
}

fn sync_json(facts: ProjectionSyncFacts) -> String {
    format!(
        concat!(
            "{{\"attempts\":{},\"successes\":{},\"failures\":{},",
            "\"requested\":{{\"process_crash_reconciled\":{},",
            "\"host_crash_ordered\":{},\"device_flush_requested\":{},",
            "\"power_loss_qualified\":{}}},",
            "\"achieved\":{{\"process_crash_reconciled\":{},",
            "\"host_crash_ordered\":{},\"device_flush_requested\":{},",
            "\"power_loss_qualified\":{}}},\"wall\":{}}}"
        ),
        facts.attempts,
        facts.successes,
        facts.failures,
        facts.requested.process_crash_reconciled,
        facts.requested.host_crash_ordered,
        facts.requested.device_flush_requested,
        facts.requested.power_loss_qualified,
        facts.achieved.process_crash_reconciled,
        facts.achieved.host_crash_ordered,
        facts.achieved.device_flush_requested,
        facts.achieved.power_loss_qualified,
        timer_json(facts.wall),
    )
}

fn replace_json(facts: ProjectionReplaceFacts) -> String {
    format!(
        concat!(
            "{{\"attempts\":{},\"successes\":{},\"failures\":{},",
            "\"requested_visible\":{},\"prior_visible\":{},",
            "\"visibility_ambiguous\":{},\"durability_ambiguous\":{},\"wall\":{}}}"
        ),
        facts.attempts,
        facts.successes,
        facts.failures,
        facts.requested_visible,
        facts.prior_visible,
        facts.visibility_ambiguous,
        facts.durability_ambiguous,
        timer_json(facts.wall),
    )
}

fn cleanup_json(facts: ProjectionCleanupFacts) -> String {
    format!(
        concat!(
            "{{\"attempts\":{},\"successes\":{},\"failures\":{},",
            "\"residue\":{},\"wall\":{}}}"
        ),
        facts.attempts,
        facts.successes,
        facts.failures,
        facts.residue,
        timer_json(facts.wall),
    )
}

fn delta(after: u64, before: u64, name: &str) -> EvalResult<u64> {
    after
        .checked_sub(before)
        .ok_or_else(|| format!("counter {name} moved backwards"))
}

fn io_error(error: std::io::Error) -> String {
    error.to_string()
}

fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[derive(Clone, Copy)]
struct ProcessUsage {
    user_ns: u64,
    system_ns: u64,
    maximum_rss_bytes: u64,
}

#[cfg(target_os = "macos")]
fn process_usage() -> EvalResult<ProcessUsage> {
    use std::ffi::c_int;

    #[repr(C)]
    #[derive(Default)]
    struct TimeVal {
        seconds: i64,
        microseconds: i64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct RUsage {
        user: TimeVal,
        system: TimeVal,
        maximum_resident_set_bytes: i64,
        remaining: [i64; 13],
    }

    unsafe extern "C" {
        fn getrusage(who: c_int, usage: *mut RUsage) -> c_int;
    }

    let mut usage = RUsage::default();
    // SAFETY: usage is a live Darwin-compatible rusage buffer for this call.
    if unsafe { getrusage(0, &mut usage) } != 0 || usage.maximum_resident_set_bytes < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(ProcessUsage {
        user_ns: timeval_ns(usage.user.seconds, usage.user.microseconds)?,
        system_ns: timeval_ns(usage.system.seconds, usage.system.microseconds)?,
        maximum_rss_bytes: usage.maximum_resident_set_bytes as u64,
    })
}

#[cfg(target_os = "macos")]
fn timeval_ns(seconds: i64, microseconds: i64) -> EvalResult<u64> {
    let seconds = u64::try_from(seconds).map_err(display_error)?;
    let microseconds = u64::try_from(microseconds).map_err(display_error)?;
    seconds
        .checked_mul(1_000_000_000)
        .and_then(|total| total.checked_add(microseconds * 1_000))
        .ok_or_else(|| "CPU time overflow".to_owned())
}

#[cfg(not(target_os = "macos"))]
fn process_usage() -> EvalResult<ProcessUsage> {
    Ok(ProcessUsage {
        user_ns: 0,
        system_ns: 0,
        maximum_rss_bytes: current_rss_bytes()?,
    })
}

fn current_rss_bytes() -> EvalResult<u64> {
    let output = Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err("ps RSS observation failed".to_owned());
    }
    String::from_utf8(output.stdout)
        .map_err(display_error)?
        .trim()
        .parse::<u64>()
        .map_err(display_error)?
        .checked_mul(1024)
        .ok_or_else(|| "RSS conversion overflow".to_owned())
}

fn fd_count() -> EvalResult<u64> {
    u64::try_from(fs::read_dir("/dev/fd").map_err(io_error)?.count()).map_err(display_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_rejects_backwards_counters() {
        assert_eq!(delta(9, 4, "x").unwrap(), 5);
        assert!(delta(4, 9, "x").is_err());
    }

    #[test]
    fn terminal_scratch_cleanup_closes_the_complete_row_sql_equation() {
        let operation = OperationDiagnostics {
            scratch_tables: 1,
            scratch_statements: 19,
            scratch_owner_setup_statements: 15,
            scratch_derived_setup_statements: 2,
            scratch_operation_statements: 2,
            ..OperationDiagnostics::default()
        };
        let cleanup = OperationDiagnostics {
            scratch_statements: 1,
            scratch_derived_setup_statements: 1,
            ..OperationDiagnostics::default()
        };
        let terminal = merge_terminal_cleanup(operation, cleanup).unwrap();
        assert_eq!(terminal.scratch_tables, 1);
        assert_eq!(terminal.scratch_statements, 20);
        assert_eq!(terminal.scratch_owner_setup_statements, 15);
        assert_eq!(terminal.scratch_derived_setup_statements, 3);
        assert_eq!(terminal.scratch_operation_statements, 2);
        assert_eq!(scratch_sql(&terminal).unwrap(), terminal.scratch_statements);
    }

    #[test]
    fn trusted_schedule_and_counter_equation_are_explicit() {
        assert_eq!(TRUSTED_SCHEDULE, [0, 24, 96]);
        let schedule = trusted_schedule_json();
        assert!(schedule.contains("\"integrity_mode\":\"TrustedLocalDev\""));
        assert!(schedule.contains("\"warmups\":3"));
        assert!(schedule.contains("\"measured\":9"));

        let trusted = EngineDelta {
            fetched_rows: 7,
            role_decode_passes: 7,
            ..EngineDelta::default()
        };
        assert!(trust_equation(IntegrityMode::TrustedLocalDev, &trusted));
        assert!(!trust_equation(IntegrityMode::Verified, &trusted));
        let verified = EngineDelta {
            authentication_passes: 7,
            identity_authentication_ns: 1,
            ..trusted
        };
        assert!(trust_equation(IntegrityMode::Verified, &verified));
        assert!(!trust_equation(IntegrityMode::TrustedLocalDev, &verified));
    }

    #[test]
    fn attribution_row_wall_requires_product_oracle_cleanup_and_residual() {
        let valid = concat!(
            "{\"schema\":\"layerfs-stage1m-attribution-row-v2\",",
            "\"product_operation_wall_ns\":100,\"oracle_wall_ns\":20,",
            "\"cleanup_wall_ns\":30,\"row_wall_residual_ns\":10,",
            "\"row_wall_ns\":160}"
        );
        assert!(validate_row_wall_json(valid).is_ok());
        assert!(validate_row_wall_json(
            &valid.replace("\"row_wall_ns\":160", "\"row_wall_ns\":159")
        )
        .is_err());
        assert!(validate_row_wall_json(
            &valid.replace("\"row_wall_residual_ns\":10", "\"row_wall_residual_ns\":-1")
        )
        .is_err());
    }

    #[test]
    fn projection_fact_mutation_rejects_hidden_or_missing_syncs() {
        let mut one_sync = ProjectionSyncFacts::available();
        one_sync.attempts = 1;
        one_sync.successes = 1;
        one_sync.requested.process_crash_reconciled = 1;
        one_sync.achieved.process_crash_reconciled = 1;

        let mut facts = ProjectionFacts::available();
        facts.recovery_marker_file_sync = one_sync;
        facts.content_temp_file_sync = one_sync;
        facts.regular_file_sync = one_sync;
        facts.regular_file_sync.attempts = 2;
        facts.regular_file_sync.successes = 2;
        facts.regular_file_sync.requested.process_crash_reconciled = 2;
        facts.regular_file_sync.achieved.process_crash_reconciled = 2;
        for owner in [
            &mut facts.staging_directory_sync,
            &mut facts.root_parent_directory_sync,
            &mut facts.dirty_tree_directory_sync,
            &mut facts.final_root_directory_sync,
        ] {
            *owner = one_sync;
        }
        facts.directory_sync = one_sync;
        facts.directory_sync.attempts = 4;
        facts.directory_sync.successes = 4;
        facts.directory_sync.requested.process_crash_reconciled = 4;
        facts.directory_sync.achieved.process_crash_reconciled = 4;

        assert!(projection_facts_exact(facts));
        facts.regular_file_sync.attempts -= 1;
        assert!(!projection_facts_exact(facts));
        facts.regular_file_sync.attempts += 1;
        facts
            .content_temp_file_sync
            .requested
            .process_crash_reconciled = 0;
        assert!(!projection_facts_exact(facts));
        facts
            .content_temp_file_sync
            .requested
            .process_crash_reconciled = 1;
        facts
            .content_temp_file_sync
            .achieved
            .process_crash_reconciled = 0;
        assert!(!projection_facts_exact(facts));
        facts
            .content_temp_file_sync
            .achieved
            .process_crash_reconciled = 1;
        facts.content_write.bytes = 1;
        assert!(!projection_facts_exact(facts));
        facts.content_write.bytes = 0;
        facts.content_temp_file_sync.wall.nanoseconds = 1;
        assert!(!projection_facts_exact(facts));
        facts.content_temp_file_sync.wall.nanoseconds = 0;
        facts.cleanup.attempts = 1;
        facts.cleanup.failures = 1;
        facts.cleanup.residue = 1;
        assert!(projection_facts_exact(facts));
        assert!(!successful_projection_facts_exact(facts));
    }

    #[test]
    fn attribution_schedule_is_the_frozen_interleaved_population() {
        let actual = ATTRIBUTION_SCHEDULE
            .iter()
            .map(|(arm, size)| (arm.name(), *size))
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                ("complete", 24),
                ("null", 0),
                ("digest", 96),
                ("native", 24),
                ("null", 96),
                ("digest", 24),
                ("native", 0),
                ("complete", 96),
                ("digest", 0),
                ("native", 96),
                ("complete", 0),
                ("null", 24),
            ]
        );
        let schedule = attribution_schedule_json();
        assert!(schedule.contains("\"warmups\":12"));
        assert!(schedule.contains("\"measured\":36"));
        assert!(schedule.contains("\"rows\":48"));
    }

    #[test]
    fn attribution_uses_the_frozen_n3_estimator_and_taxonomy() {
        assert_eq!(three_stats(&[30, 10, 20]).unwrap(), (20, 30));
        assert_eq!(
            AttributionArm::Complete.operation_label(),
            "same_open_warmed_source_fresh_destination"
        );
        assert_eq!(
            AttributionArm::Null.operation_label(),
            "warm_authenticated_null_sink"
        );
        assert_eq!(
            AttributionArm::Digest.operation_label(),
            "warm_authenticated_digest"
        );
        assert_eq!(
            AttributionArm::Native.operation_label(),
            "native_durable_output"
        );
    }

    #[test]
    fn attribution_model_uses_t0_and_the_frozen_24_to_96_slope() {
        let populations = [
            AttributionArm::Complete,
            AttributionArm::Null,
            AttributionArm::Digest,
            AttributionArm::Native,
        ]
        .into_iter()
        .flat_map(|arm| {
            [
                (arm, 0, 10_000_000, 10_000_000),
                (arm, 24, 34_000_000, 34_000_000),
                (arm, 96, 106_000_000, 106_000_000),
            ]
        })
        .collect::<Vec<_>>();
        let models = attribution_models_json(&populations).unwrap();
        assert!(models.contains("\"fixed_cost_ns\":10000000"));
        assert!(models.contains("\"slope_ns_per_mib\":1000000"));
        assert!(models.contains("\"sustained_bandwidth_mib_per_s\":1000"));
        assert!(models.contains("\"residual_24_ns\":0"));
        assert!(models.contains("\"residual_96_ns\":0"));
        assert!(models.contains("\"model_valid\":true"));
    }

    #[test]
    fn acceptance_schedule_statistics_and_gates_are_frozen() {
        assert_eq!(
            ACCEPTANCE_SCHEDULE,
            [
                acceptance_block(1, 0, ['A', 'B']),
                acceptance_block(1, 24, ['A', 'B']),
                acceptance_block(1, 96, ['A', 'B']),
                acceptance_block(2, 96, ['B', 'A']),
                acceptance_block(2, 24, ['B', 'A']),
                acceptance_block(2, 0, ['B', 'A']),
                acceptance_block(3, 24, ['B', 'A']),
                acceptance_block(3, 0, ['B', 'A']),
                acceptance_block(3, 96, ['B', 'A']),
                acceptance_block(4, 0, ['A', 'B']),
                acceptance_block(4, 96, ['A', 'B']),
                acceptance_block(4, 24, ['A', 'B']),
            ]
        );
        let schedule = acceptance_schedule_json();
        assert!(schedule.contains("\"paired_warmups\":24"));
        assert!(schedule.contains("\"measured\":24"));
        let stats = acceptance_stats(&[40, 10, 30, 20]).unwrap();
        assert_eq!(
            (stats.minimum, stats.p50, stats.p95, stats.maximum),
            (10, 25, 40, 40)
        );
        let mut samples = Vec::new();
        for pair in 1..=4 {
            for (size, control_wall, candidate_wall, candidate_cpu) in [
                (0, 12_000_000, 10_000_000, 1_000_000),
                (24, 70_000_000, 53_200_000, 25_000_000),
                (96, 250_000_000, 182_800_000, 97_000_000),
            ] {
                samples.push(AcceptanceSample {
                    pair,
                    size_mib: size,
                    operand: 'A',
                    wall_ns: control_wall,
                    cpu_ns: candidate_cpu + 1_000_000,
                    rss_bytes: 10_000_000,
                    q_bytes: 8 * 1024 * 1024,
                    fd_peak: 12,
                    primary_connections: 1,
                    scratch_connections: 3,
                    total_connections: 4,
                    sync_calls: 4,
                    residue: 0,
                });
                samples.push(AcceptanceSample {
                    pair,
                    size_mib: size,
                    operand: 'B',
                    wall_ns: candidate_wall,
                    cpu_ns: candidate_cpu,
                    rss_bytes: 9_000_000,
                    q_bytes: 8 * 1024 * 1024 - 1,
                    fd_peak: 10,
                    primary_connections: 1,
                    scratch_connections: 1,
                    total_connections: 2,
                    sync_calls: 3,
                    residue: 0,
                });
            }
        }
        let disposition = acceptance_disposition(&samples).unwrap();
        assert_eq!(disposition.status, "PASS");
        assert_eq!((disposition.wins24, disposition.wins96), (4, 4));
        assert!(disposition.fixed_cost_pass);
        assert!(disposition.p95_relative_pass);
        assert!(disposition.higher_absolute_class);
        assert!(disposition.primary_class_pass);
        assert!(disposition.model_valid);
        assert!(disposition.fitted_bandwidth_mib_s >= 500.0);
        assert!(disposition.cpu_scaling_pass);
        assert!(disposition.cpu_regression_pass);
        assert!(disposition.no_resource_regression);
        assert!(disposition.no_sync_regression);
        assert!(disposition.no_residue_regression);

        for sample in &mut samples {
            if sample.operand == 'B' && sample.pair <= 2 {
                sample.wall_ns = sample.wall_ns.saturating_add(100_000_000);
            }
        }
        assert_eq!(acceptance_disposition(&samples).unwrap().status, "REVISE");
    }
}
