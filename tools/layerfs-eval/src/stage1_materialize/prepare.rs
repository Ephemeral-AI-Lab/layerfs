use super::contract::{
    EvalResult, BUFFER_BYTES, FILE_PATH, FIXTURE_MODE, FIXTURE_MTIME_NANOSECONDS,
    FIXTURE_MTIME_SECONDS, PRESERVED_24_MIB_DIGEST,
};
use super::error::{display_error, io_error};
use super::evidence::digest::digest_file;
use crate::legacy_full::LayerFs;
use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

pub(in crate::stage1_materialize) fn fixture_root() -> PathBuf {
    crate::stage1_fixture::workspace_root()
        .join("target/layerfs-stage1m-fixtures/full-materialization-v1")
}

pub(in crate::stage1_materialize) fn prepare_into(root: &Path) -> EvalResult<()> {
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

pub(in crate::stage1_materialize) fn prepare_size(
    root: &Path,
    size_mib: u64,
) -> EvalResult<String> {
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

pub(in crate::stage1_materialize) fn generate_source(
    path: &Path,
    bytes: u64,
) -> EvalResult<String> {
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

pub(in crate::stage1_materialize) fn set_fixture_metadata(path: &Path) -> EvalResult<()> {
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

pub(in crate::stage1_materialize) fn copy_file_bounded(
    source: &Path,
    destination: &Path,
) -> EvalResult<()> {
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
pub(in crate::stage1_materialize) struct DigestWriter {
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

pub(in crate::stage1_materialize) fn tree_bytes(root: &Path) -> EvalResult<u64> {
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

pub(in crate::stage1_materialize) fn durable_write(path: &Path, bytes: &[u8]) -> EvalResult<()> {
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

pub(in crate::stage1_materialize) fn verify_fixture_sources(root: &Path) -> EvalResult<()> {
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

pub(in crate::stage1_materialize) fn unix_ns() -> EvalResult<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .map_err(display_error)
}

pub(in crate::stage1_materialize) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(in crate::stage1_materialize) fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
