use super::artifact::{
    display_error, durable_write, hex, io_error, json_escape, json_string, json_u128, unix_ns,
};
use super::engine_counters::FixtureMaster;
use super::limits::{
    BUFFER_BYTES, FILE_PATH, FIXTURE_MODE, FIXTURE_MTIME_NANOSECONDS, FIXTURE_MTIME_SECONDS,
    FIXTURE_VERSION, INITIAL_BYTES, MAXIMUM_BYTES, PREPARATION_LIMIT_NS,
};
use crate::legacy_full::{Diagnostics, LayerFs, RootId};
use crate::stage1_fixture::{self, EvalResult};
use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, Instant, UNIX_EPOCH};
pub(crate) fn fixture_root() -> PathBuf {
    stage1_fixture::workspace_root().join("target/layerfs-stage1-fixtures/apple-edge-v1")
}
pub(crate) fn readiness_path() -> PathBuf {
    stage1_fixture::workspace_root().join("target/layerfs-stage1-apple-edge-readiness.json")
}
pub(crate) fn prepare() -> EvalResult<()> {
    let destination = fixture_root();
    if destination.exists() {
        let master = read_master(&destination)?;
        verify_fixture(&destination, &master, true)?;
        println!(
            "stage1.1-prepare status=PASS fixture={} reused=true wall_ns={}",
            destination.display(),
            master.preparation_wall_ns
        );
        return Ok(());
    }
    let started = Instant::now();
    let parent = destination
        .parent()
        .ok_or_else(|| "fixture has no parent".to_owned())?;
    fs::create_dir_all(parent).map_err(io_error)?;
    let temporary = parent.join(format!(
        ".apple-edge-v1-preparing-{}-{}",
        std::process::id(),
        unix_ns()?
    ));
    fs::create_dir(&temporary).map_err(io_error)?;
    let result = prepare_into(&temporary, started);
    match result {
        Ok(master) => {
            fs::rename(&temporary, &destination).map_err(io_error)?;
            stage1_fixture::sync_directory(parent)?;
            println!(
                "stage1.1-prepare status=PASS fixture={} reused=false wall_ns={}",
                destination.display(),
                master.preparation_wall_ns
            );
            Ok(())
        }
        Err(error) => {
            let failure = parent.join(format!("apple-edge-v1-preparation-failure-{}", unix_ns()?));
            let _ = fs::rename(&temporary, &failure);
            Err(format!(
                "{error}; preparation evidence preserved at {}",
                failure.display()
            ))
        }
    }
}
pub(crate) fn prepare_into(root: &Path, started: Instant) -> EvalResult<FixtureMaster> {
    let source = root.join("source-native/data/payload.bin");
    fs::create_dir_all(
        source
            .parent()
            .ok_or_else(|| "source has no parent".to_owned())?,
    )
    .map_err(io_error)?;
    let raw_digest = generate_source(&source)?;
    let base = root.join("bases/base");
    fs::create_dir_all(
        base.parent()
            .ok_or_else(|| "base has no parent".to_owned())?,
    )
    .map_err(io_error)?;
    let opened = LayerFs::open(&base).map_err(display_error)?;
    let store_id = hex(&opened.fs.store_id().map_err(display_error)?);
    let capture_source = root.join("capture-source");
    let mut external = opened
        .fs
        .materialize_external(opened.head, &capture_source)
        .map_err(display_error)?;
    let native_file = external.path().join(FILE_PATH);
    fs::create_dir_all(
        native_file
            .parent()
            .ok_or_else(|| "native file has no parent".to_owned())?,
    )
    .map_err(io_error)?;
    copy_file_bounded(&source, &native_file)?;
    set_fixture_metadata(&native_file)?;
    let root_id = external.capture_quiescent().map_err(display_error)?;
    let ref_state = opened.fs.current_head("main").map_err(display_error)?;
    if ref_state.root != root_id {
        return Err("fixture capture did not publish exact root".to_owned());
    }
    let diagnostics = opened.fs.counter_snapshot().map_err(display_error)?;
    validate_profile(&diagnostics)?;
    compare_canonical_source(&opened.fs, root_id, &source)?;
    drop(external);
    fs::remove_dir_all(&capture_source).map_err(io_error)?;
    drop(opened);
    let reopened = LayerFs::open(&base).map_err(display_error)?;
    if reopened.ref_state != ref_state
        || hex(&reopened.fs.store_id().map_err(display_error)?) != store_id
    {
        return Err("fresh Verified fixture reopen changed authority".to_owned());
    }
    compare_canonical_source(&reopened.fs, root_id, &source)?;
    validate_profile(&reopened.fs.counter_snapshot().map_err(display_error)?)?;
    drop(reopened);
    let apfs_identity = stage1_fixture::assert_apfs(root)?;
    let mut master = FixtureMaster {
        raw_digest,
        root: root_id,
        generation: ref_state.generation,
        store_id,
        profile: "page=4096;cache=1280;spill=1280;DELETE/FULL/FILE/mmap=0".to_owned(),
        apfs_identity,
        fixture_blake3: String::new(),
        preparation_wall_ns: 0,
    };
    master.fixture_blake3 = stage1_fixture::tree_digest(root, Some(Path::new("master.json")))?;
    master.preparation_wall_ns = started.elapsed().as_nanos();
    if master.preparation_wall_ns > PREPARATION_LIMIT_NS {
        return Err(format!(
            "fixture preparation {}ns exceeds {}ns",
            master.preparation_wall_ns, PREPARATION_LIMIT_NS
        ));
    }
    durable_write(&root.join("master.json"), &master_json(&master))?;
    stage1_fixture::seal_tree(root)?;
    verify_fixture(root, &master, true)?;
    Ok(master)
}
pub(crate) fn generate_source(path: &Path) -> EvalResult<String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut offset = 0_u64;
    while offset < INITIAL_BYTES {
        stage1_fixture::fill_retained_buffer(&mut buffer, offset);
        let take = usize::try_from((INITIAL_BYTES - offset).min(BUFFER_BYTES as u64))
            .map_err(display_error)?;
        file.write_all(&buffer[..take]).map_err(io_error)?;
        hasher.update(&buffer[..take]);
        offset += take as u64;
    }
    file.sync_all().map_err(io_error)?;
    set_fixture_metadata(path)?;
    Ok(hasher.finalize().to_hex().to_string())
}
pub(crate) fn set_fixture_metadata(path: &Path) -> EvalResult<()> {
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
pub(crate) fn copy_file_bounded(source: &Path, destination: &Path) -> EvalResult<()> {
    let mut input = File::open(source).map_err(io_error)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(io_error)?;
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut total = 0_u64;
    loop {
        let read = input.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read]).map_err(io_error)?;
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| "copy length overflow".to_owned())?;
    }
    if total != INITIAL_BYTES {
        return Err(format!("fixture copy wrote {total} bytes"));
    }
    output.sync_all().map_err(io_error)
}
pub(crate) fn compare_canonical_source(
    fs: &LayerFs,
    root: RootId,
    source: &Path,
) -> EvalResult<()> {
    let input = File::open(source).map_err(io_error)?;
    let mut sink = FileCompareWriter::new(input);
    fs.read_to(root, FILE_PATH, &mut sink)
        .map_err(display_error)?;
    sink.finish(INITIAL_BYTES)
}
pub(crate) struct FileCompareWriter<R> {
    pub(crate) input: R,
    pub(crate) compared: u64,
}
impl<R: Read> FileCompareWriter<R> {
    pub(crate) fn new(input: R) -> Self {
        Self { input, compared: 0 }
    }
    pub(crate) fn finish(mut self, expected: u64) -> EvalResult<()> {
        let mut extra = [0_u8; 1];
        if self.compared != expected || self.input.read(&mut extra).map_err(io_error)? != 0 {
            return Err("canonical/source comparison length mismatch".to_owned());
        }
        Ok(())
    }
}
impl<R: Read> Write for FileCompareWriter<R> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let mut expected = vec![0_u8; bytes.len()];
        self.input.read_exact(&mut expected)?;
        if expected != bytes {
            return Err(std::io::Error::other("canonical/source byte mismatch"));
        }
        self.compared = self
            .compared
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| std::io::Error::other("comparison length overflow"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(crate) fn master_json(master: &FixtureMaster) -> String {
    format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1.1-master-v1\",",
            "\"fixture_version\":\"{}\",\"file_path\":\"{}\",",
            "\"initial_bytes\":{},\"maximum_bytes\":{},\"terminal_bytes\":{},",
            "\"mode\":{},\"mtime_seconds\":{},\"mtime_nanoseconds\":{},",
            "\"raw_digest\":\"{}\",\"root\":\"{}\",\"generation\":{},",
            "\"store_id\":\"{}\",\"profile\":\"{}\",",
            "\"apfs_identity\":\"{}\",\"fixture_blake3\":\"{}\",",
            "\"preparation_wall_ns\":{}}}\n"
        ),
        FIXTURE_VERSION,
        FILE_PATH,
        INITIAL_BYTES,
        MAXIMUM_BYTES,
        INITIAL_BYTES,
        FIXTURE_MODE,
        FIXTURE_MTIME_SECONDS,
        FIXTURE_MTIME_NANOSECONDS,
        master.raw_digest,
        master.root,
        master.generation,
        master.store_id,
        json_escape(&master.profile),
        json_escape(&master.apfs_identity),
        master.fixture_blake3,
        master.preparation_wall_ns,
    )
}
pub(crate) fn read_master(root: &Path) -> EvalResult<FixtureMaster> {
    let json = fs::read_to_string(root.join("master.json")).map_err(io_error)?;
    if json_string(&json, "schema")? != "layerfs-stage1.1-master-v1"
        || json_string(&json, "fixture_version")? != FIXTURE_VERSION
        || json_string(&json, "file_path")? != FILE_PATH
        || json_u128(&json, "initial_bytes")? != u128::from(INITIAL_BYTES)
        || json_u128(&json, "maximum_bytes")? != u128::from(MAXIMUM_BYTES)
        || json_u128(&json, "terminal_bytes")? != u128::from(INITIAL_BYTES)
        || json_u128(&json, "mode")? != u128::from(FIXTURE_MODE)
        || json_u128(&json, "mtime_seconds")? != u128::from(FIXTURE_MTIME_SECONDS)
        || json_u128(&json, "mtime_nanoseconds")? != u128::from(FIXTURE_MTIME_NANOSECONDS)
    {
        return Err("fixture master frozen constants mismatch".to_owned());
    }
    Ok(FixtureMaster {
        raw_digest: json_string(&json, "raw_digest")?,
        root: RootId::from_str(&json_string(&json, "root")?).map_err(display_error)?,
        generation: u64::try_from(json_u128(&json, "generation")?).map_err(display_error)?,
        store_id: json_string(&json, "store_id")?,
        profile: json_string(&json, "profile")?,
        apfs_identity: json_string(&json, "apfs_identity")?,
        fixture_blake3: json_string(&json, "fixture_blake3")?,
        preparation_wall_ns: json_u128(&json, "preparation_wall_ns")?,
    })
}
pub(crate) fn verify_fixture(root: &Path, master: &FixtureMaster, full: bool) -> EvalResult<()> {
    stage1_fixture::verify_sealed(root)?;
    if master.preparation_wall_ns > PREPARATION_LIMIT_NS
        || master.profile != "page=4096;cache=1280;spill=1280;DELETE/FULL/FILE/mmap=0"
        || stage1_fixture::assert_apfs(root)? != master.apfs_identity
    {
        return Err("fixture master custody/profile mismatch".to_owned());
    }
    let source = root.join("source-native/data/payload.bin");
    let metadata = fs::metadata(&source).map_err(io_error)?;
    if metadata.len() != INITIAL_BYTES || metadata.permissions().mode() & 0o777 != 0o444 {
        return Err("fixture source size/seal mismatch".to_owned());
    }
    if full && stage1_fixture::hash_file(&source)? != master.raw_digest {
        return Err("fixture source digest mismatch".to_owned());
    }
    if stage1_fixture::tree_digest(root, Some(Path::new("master.json")))? != master.fixture_blake3 {
        return Err("fixture tree digest mismatch".to_owned());
    }
    let opened = LayerFs::open(&root.join("bases/base")).map_err(display_error)?;
    if opened.ref_state.root != master.root
        || opened.ref_state.generation != master.generation
        || hex(&opened.fs.store_id().map_err(display_error)?) != master.store_id
    {
        return Err("fixture Store authority mismatch".to_owned());
    }
    validate_profile(&opened.fs.counter_snapshot().map_err(display_error)?)?;
    if full {
        compare_canonical_source(&opened.fs, master.root, &source)?;
    }
    drop(opened);
    Ok(())
}
pub(crate) fn validate_profile(diagnostics: &Diagnostics) -> EvalResult<()> {
    if diagnostics.page_size != 4_096
        || diagnostics.cache_pages != 1_280
        || diagnostics.cache_spill_pages != 1_280
    {
        return Err(format!(
            "Store profile mismatch: page={} cache={} spill={}",
            diagnostics.page_size, diagnostics.cache_pages, diagnostics.cache_spill_pages
        ));
    }
    Ok(())
}
#[derive(Clone, Debug)]
pub(crate) struct SourceIdentity {
    pub(crate) git_commit: String,
    pub(crate) dirty_tree: bool,
    pub(crate) tree_blake3: String,
    pub(crate) manifest_sha256: String,
    pub(crate) executable_path: PathBuf,
    pub(crate) executable_sha256: String,
    pub(crate) executable_blake3: String,
}
