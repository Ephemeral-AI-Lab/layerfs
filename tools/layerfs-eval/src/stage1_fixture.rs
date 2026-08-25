use layerfs_sdk::{IntegrityMode, LayerFs, OpenedLayerFs, RefState, RootId};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(target_os = "macos")]
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[path = "stage1_fixture_cdc.rs"]
mod fixture_cdc;

pub type EvalResult<T> = Result<T, String>;

pub const FILE_BYTES: u64 = 104_857_600;
pub const BUFFER_BYTES: usize = 1_048_576;
pub const RANDOM_RANGE_BYTES: u64 = 65_536;
pub const EXPECTED_RAW_DIGEST: &str =
    "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7";
pub const EXPECTED_CDC_REFERENCES: u64 = 5_284;
pub const EXPECTED_CDC_SEQUENCE: &str =
    "5bb376c3c54d8724973a7b160acab599f2f5cee4b4a56e855ff0cbe987425994";
pub const FIXTURE_VERSION: &str = "single-100m-v1";
pub const FILE_PATH: &str = "S1-100.bin";
const RETAINED_SEED: u64 = 0x4c41_5945_5253_4653;
const LABEL: &str = "S1-100";
const BASES: &[&str] = &[
    "read-reconstruct",
    "import-genesis",
    "replace-existing",
    "overwrite",
    "insert",
    "delete",
    "append",
    "truncate",
    "refresh-a-b",
    "history",
];
static ATTEMPT_SERIAL: AtomicU64 = AtomicU64::new(0);
static PREPARATION_FAILURE_SERIAL: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
struct PreparationProgress {
    phase: &'static str,
    base: Option<String>,
}

impl PreparationProgress {
    fn new() -> Self {
        Self {
            phase: "admission",
            base: None,
        }
    }

    fn set(&mut self, phase: &'static str, base: Option<&str>) {
        self.phase = phase;
        self.base = base.map(str::to_owned);
    }
}

#[derive(Clone, Debug)]
pub struct BaseManifest {
    pub name: String,
    pub root: RootId,
    pub root_a: Option<RootId>,
    pub root_b: Option<RootId>,
    pub generation: u64,
    pub selector_generation: u64,
    pub store_id: String,
    pub profile_id: String,
    pub store_database_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct Master {
    pub raw_digest: String,
    pub replacement_digest: String,
    pub inventory_digest: String,
    pub new_file_aggregate_rope_references: u64,
    pub bases: BTreeMap<String, BaseManifest>,
}

#[derive(Clone, Debug, Default)]
pub struct CloneReceipt {
    /// Complete reset admission, including clone custody and selector checks.
    pub wall_ns: u128,
    /// `/bin/cp -cR` return wall inside the complete reset.
    pub clone_wall_ns: u128,
    pub source_logical_bytes: u64,
    pub destination_logical_bytes: u64,
    pub source_allocated_bytes: u64,
    pub destination_allocated_bytes: u64,
    pub distinct_regular_inodes: u64,
    pub clone_id: u64,
}

#[derive(Debug)]
pub struct Attempt {
    root: PathBuf,
    store: PathBuf,
    marker: String,
    pub clone: CloneReceipt,
}

#[derive(Clone, Debug)]
pub struct Selector {
    pub generation: u64,
    pub store_id: String,
    pub profile_id: String,
}

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("evaluator is under workspace/tools")
        .to_owned()
}

pub fn fixture_root() -> PathBuf {
    workspace_root()
        .join("target/layerfs-stage1-fixtures")
        .join(FIXTURE_VERSION)
}

pub fn input_path(replacement: bool) -> PathBuf {
    fixture_root().join("input").join(if replacement {
        "S1-replace-100.bin"
    } else {
        FILE_PATH
    })
}

pub fn prepare_single_file() -> EvalResult<()> {
    regular_file_ceiling_preflight()?;
    let target = fixture_root();
    if target.exists() {
        return Err(format!(
            "refusing to overwrite prepared fixture {}",
            target.display()
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| "fixture root has no parent".to_owned())?;
    fs::create_dir_all(parent).map_err(io_error)?;
    let mut progress = PreparationProgress::new();
    progress.set("verify-apfs", None);
    if let Err(error) = assert_apfs(parent) {
        let receipt = write_preparation_failure(parent, &progress, &error);
        return match receipt {
            Ok(_) => Err(error),
            Err(receipt_error) => Err(format!(
                "{error}; preparation failure receipt failed: {receipt_error}"
            )),
        };
    }
    let temporary = parent.join(format!(
        ".{FIXTURE_VERSION}.preparing-{}",
        std::process::id()
    ));
    if temporary.exists() {
        return Err(format!(
            "owned preparation residue exists: {}",
            temporary.display()
        ));
    }
    fs::create_dir(&temporary).map_err(io_error)?;
    let result = prepare_into(&temporary, &mut progress);
    if let Err(error) = result {
        let receipt = write_preparation_failure(parent, &progress, &error);
        let _ = make_writable(&temporary);
        let _ = fs::remove_dir_all(&temporary);
        return match receipt {
            Ok(_) => Err(error),
            Err(receipt_error) => Err(format!(
                "{error}; preparation failure receipt failed: {receipt_error}"
            )),
        };
    }
    progress.set("atomic-install", None);
    if let Err(error) = fs::rename(&temporary, &target).map_err(io_error) {
        let receipt = write_preparation_failure(parent, &progress, &error);
        let _ = make_writable(&temporary);
        let _ = fs::remove_dir_all(&temporary);
        return match receipt {
            Ok(_) => Err(error),
            Err(receipt_error) => Err(format!(
                "{error}; preparation failure receipt failed: {receipt_error}"
            )),
        };
    }
    progress.set("sync-installed-parent", None);
    if let Err(error) = sync_directory(parent) {
        let receipt = write_preparation_failure(parent, &progress, &error);
        return match receipt {
            Ok(_) => Err(error),
            Err(receipt_error) => Err(format!(
                "{error}; preparation failure receipt failed: {receipt_error}"
            )),
        };
    }
    println!(
        "stage1-prepare status=PASS fixture={} bytes={} raw_blake3={} cdc_references={} cdc_sequence={}",
        target.display(),
        FILE_BYTES,
        EXPECTED_RAW_DIGEST,
        EXPECTED_CDC_REFERENCES,
        EXPECTED_CDC_SEQUENCE
    );
    Ok(())
}

pub fn regular_file_ceiling_preflight() -> EvalResult<()> {
    // Store authority files are reported separately. The 100 MiB ceiling is for
    // evaluator inputs/intermediates and native product outputs.
    if BUFFER_BYTES as u64 > FILE_BYTES {
        return Err("fixture stream buffer exceeds the frozen file ceiling".to_owned());
    }
    Ok(())
}

fn write_preparation_failure(
    parent: &Path,
    progress: &PreparationProgress,
    error: &str,
) -> EvalResult<PathBuf> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(display_error)?
        .as_nanos();
    let path = parent.join(format!(
        "stage1-preparation-failure-v2-{nonce}-{}-{}.json",
        std::process::id(),
        PREPARATION_FAILURE_SERIAL.fetch_add(1, Ordering::Relaxed),
    ));
    let source = crate::stage1::preparation_source_context_json().unwrap_or_else(|cause| {
        format!(
            "{{\"status\":\"Unavailable\",\"error\":\"{}\"}}",
            artifact_json_escape(&cause)
        )
    });
    let base = progress.base.as_ref().map_or_else(
        || "null".to_owned(),
        |base| format!("\"{}\"", artifact_json_escape(base)),
    );
    let json = format!(
        "{{\"schema\":\"layerfs-stage1-preparation-failure-v2\",\"status\":\"FAIL\",\"phase\":\"{}\",\"base\":{},\"error\":\"{}\",\"source\":{}}}\n",
        artifact_json_escape(progress.phase),
        base,
        artifact_json_escape(error),
        source,
    );
    let mut receipt = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(io_error)?;
    receipt.write_all(json.as_bytes()).map_err(io_error)?;
    receipt.sync_all().map_err(io_error)?;
    sync_directory(parent)?;
    Ok(path)
}

fn prepare_into(root: &Path, progress: &mut PreparationProgress) -> EvalResult<()> {
    let started = Instant::now();
    progress.set("create-layout", None);
    let input = root.join("input");
    let bases = root.join("bases");
    fs::create_dir(&input).map_err(io_error)?;
    fs::create_dir(&bases).map_err(io_error)?;

    let raw = input.join(FILE_PATH);
    let replacement = input.join("S1-replace-100.bin");
    progress.set("generate-input", Some(FILE_PATH));
    let raw_digest = generate_input(&raw, 0)?;
    if raw_digest != EXPECTED_RAW_DIGEST {
        return Err(format!(
            "S1-100 generator mismatch: expected {EXPECTED_RAW_DIGEST}, got {raw_digest}"
        ));
    }
    progress.set("verify-independent-cdc", Some(FILE_PATH));
    let cdc = fixture_cdc::scan_file(&raw)?;
    if cdc.bytes != FILE_BYTES
        || cdc.references != EXPECTED_CDC_REFERENCES
        || cdc.sequence != EXPECTED_CDC_SEQUENCE
    {
        return Err(format!(
            "S1-100 independent CDC mismatch: bytes {}/{FILE_BYTES}, references {}/{EXPECTED_CDC_REFERENCES}, sequence {}/{}",
            cdc.bytes, cdc.references, cdc.sequence, EXPECTED_CDC_SEQUENCE
        ));
    }
    progress.set("generate-input", Some("S1-replace-100.bin"));
    let replacement_digest = generate_input(&replacement, 0xa5)?;

    let read_base = bases.join("read-reconstruct");
    progress.set("populate-base", Some("read-reconstruct"));
    let (r100, new_file_aggregate_rope_references) = populate_store(&read_base, &raw, FILE_BYTES)?;
    for name in [
        "replace-existing",
        "overwrite",
        "delete",
        "truncate",
        "history",
    ] {
        progress.set("clone-base", Some(name));
        clone_directory(&read_base, &bases.join(name))?;
    }

    progress.set("populate-base", Some("import-genesis"));
    let import = LayerFs::open(&bases.join("import-genesis")).map_err(display_error)?;
    let import_root = import.ref_state.clone();
    drop(import);
    progress.set("populate-base", Some("insert"));
    let (insert_root, _) = populate_store(&bases.join("insert"), &raw, FILE_BYTES - 8_192)?;
    progress.set("populate-base", Some("append"));
    let (append_root, _) = populate_store(&bases.join("append"), &raw, FILE_BYTES - 4_096)?;

    progress.set("populate-base", Some("refresh-a-b"));
    clone_directory(&read_base, &bases.join("refresh-a-b"))?;
    let refresh =
        LayerFs::open_with_integrity(&bases.join("refresh-a-b"), IntegrityMode::TrustedLocalDev)
            .map_err(display_error)?;
    let refresh_a = refresh.ref_state.clone();
    let (refresh_b, _) = refresh
        .fs
        .replace_range_observed(
            &refresh_a,
            FILE_PATH,
            FILE_BYTES / 2 - 2_048,
            4_096,
            std::io::Cursor::new(edit_bytes(0x42, 4_096)),
        )
        .map_err(display_error)?;
    let refresh_prepared = refresh
        .fs
        .move_main(&refresh_b, refresh_a.root)
        .map_err(display_error)?;
    if refresh_prepared.root != refresh_a.root
        || refresh.fs.current_head("main").map_err(display_error)? != refresh_prepared
    {
        return Err("refresh-a-b must retain A+B while main starts at A".to_owned());
    }
    let mut refresh_probe = Vec::new();
    refresh
        .fs
        .read_range(
            refresh_b.root,
            FILE_PATH,
            FILE_BYTES / 2 - 2_048..FILE_BYTES / 2 + 2_048,
            &mut refresh_probe,
        )
        .map_err(display_error)?;
    if refresh_probe != edit_bytes(0x42, 4_096) {
        return Err("refresh-a-b retained B is unreadable or has wrong bytes".to_owned());
    }
    drop(refresh);

    progress.set("verify-user-file-ceiling", None);
    verify_user_file_ceiling(&input)?;

    let mut expected = BTreeMap::new();
    for name in BASES {
        progress.set("verify-base-manifest", Some(name));
        let base = bases.join(name);
        let opened = LayerFs::open_with_integrity(&base, IntegrityMode::TrustedLocalDev)
            .map_err(display_error)?;
        let state = opened.fs.current_head("main").map_err(display_error)?;
        drop(opened);
        let selector = read_selector(&base)?;
        let store_database_bytes = selected_database_bytes(&base, selector.generation)?;
        let wanted = match *name {
            "import-genesis" => import_root.root,
            "insert" => insert_root.root,
            "append" => append_root.root,
            "refresh-a-b" => refresh_prepared.root,
            _ => r100.root,
        };
        if state.root != wanted {
            return Err(format!("prepared root mismatch for {name}"));
        }
        expected.insert(
            (*name).to_owned(),
            BaseManifest {
                name: (*name).to_owned(),
                root: state.root,
                root_a: (*name == "refresh-a-b").then_some(refresh_a.root),
                root_b: (*name == "refresh-a-b").then_some(refresh_b.root),
                generation: state.generation,
                selector_generation: selector.generation,
                store_id: selector.store_id,
                profile_id: selector.profile_id,
                store_database_bytes,
            },
        );
    }

    progress.set("hash-inventory", None);
    let inventory_digest = tree_digest(root, Some(Path::new("master.json")))?;
    let master = Master {
        raw_digest,
        replacement_digest,
        inventory_digest,
        new_file_aggregate_rope_references,
        bases: expected,
    };
    progress.set("write-master", None);
    write_master(
        &root.join("master.json"),
        &master,
        started.elapsed().as_nanos(),
    )?;
    progress.set("seal-fixture", None);
    seal_tree(root)?;
    progress.set("verify-seal", None);
    verify_sealed(root)?;
    verify_fresh_reopens(root, &master, progress)?;
    Ok(())
}

fn populate_store(store: &Path, input: &Path, bytes: u64) -> EvalResult<(RefState, u64)> {
    let opened = LayerFs::open(store).map_err(display_error)?;
    let source = File::open(input).map_err(io_error)?.take(bytes);
    let (state, counters) = opened
        .fs
        .replace_file_observed(&opened.ref_state, FILE_PATH, source)
        .map_err(display_error)?;
    if opened.fs.current_head("main").map_err(display_error)? != state {
        return Err("prepared publication did not become exact main RefState".to_owned());
    }
    drop(opened);
    Ok((state, counters.rope.chunks_created))
}

pub fn fill_retained_buffer(buffer: &mut [u8], offset: u64) {
    let salt_hash = LABEL
        .bytes()
        .fold(0_u64, |value, byte| value.rotate_left(5) ^ u64::from(byte));
    let mut state = RETAINED_SEED ^ salt_hash ^ offset;
    for (index, byte) in buffer.iter_mut().enumerate() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let position = offset.wrapping_add(index as u64);
        *byte = if (position / 8_192) % 23 == 0 {
            (salt_hash as u8).wrapping_add((position / 8_192) as u8)
        } else {
            (state >> 24) as u8
        };
    }
}

fn generate_input(path: &Path, xor: u8) -> EvalResult<String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut written = 0_u64;
    while written < FILE_BYTES {
        fill_retained_buffer(&mut buffer, written);
        if xor != 0 {
            buffer.iter_mut().for_each(|byte| *byte ^= xor);
        }
        let take = usize::try_from((FILE_BYTES - written).min(BUFFER_BYTES as u64))
            .map_err(|_| "fixture length overflow".to_owned())?;
        file.write_all(&buffer[..take]).map_err(io_error)?;
        hasher.update(&buffer[..take]);
        written += take as u64;
    }
    file.sync_all().map_err(io_error)?;
    Ok(hasher.finalize().to_hex().to_string())
}

pub fn expected_bytes(start: u64, length: usize) -> EvalResult<Vec<u8>> {
    let end = start
        .checked_add(length as u64)
        .ok_or_else(|| "oracle range overflow".to_owned())?;
    if end > FILE_BYTES || length > BUFFER_BYTES {
        return Err("oracle request exceeds the bounded S1-100 range".to_owned());
    }
    let mut output = Vec::with_capacity(length);
    let mut position = start;
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    while position < end {
        let block = position / BUFFER_BYTES as u64 * BUFFER_BYTES as u64;
        fill_retained_buffer(&mut buffer, block);
        let within = usize::try_from(position - block).map_err(|_| "range overflow".to_owned())?;
        let take = usize::try_from((end - position).min((BUFFER_BYTES - within) as u64))
            .map_err(|_| "range overflow".to_owned())?;
        output.extend_from_slice(&buffer[within..within + take]);
        position += take as u64;
    }
    Ok(output)
}

pub fn stream_expected<W: Write>(start: u64, length: u64, output: &mut W) -> EvalResult<()> {
    let end = start
        .checked_add(length)
        .ok_or_else(|| "oracle range overflow".to_owned())?;
    if end > FILE_BYTES {
        return Err("oracle stream exceeds S1-100".to_owned());
    }
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut position = start;
    while position < end {
        let block = position / BUFFER_BYTES as u64 * BUFFER_BYTES as u64;
        fill_retained_buffer(&mut buffer, block);
        let within = usize::try_from(position - block).map_err(|_| "range overflow".to_owned())?;
        let take = usize::try_from((end - position).min((BUFFER_BYTES - within) as u64))
            .map_err(|_| "range overflow".to_owned())?;
        output
            .write_all(&buffer[within..within + take])
            .map_err(io_error)?;
        position += take as u64;
    }
    Ok(())
}

pub fn edit_bytes(tag: u8, length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| tag.wrapping_add((index as u8).wrapping_mul(31)))
        .collect()
}

pub fn hash_file(path: &Path) -> EvalResult<String> {
    let mut file = File::open(path).map_err(io_error)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

pub fn read_master(root: &Path) -> EvalResult<Master> {
    let json = fs::read_to_string(root.join("master.json")).map_err(io_error)?;
    let raw_digest = json_string(&json, "raw_blake3")?;
    let replacement_digest = json_string(&json, "replacement_blake3")?;
    let inventory_digest = json_string(&json, "inventory_blake3")?;
    let new_file_aggregate_rope_references = json_u64(&json, "new_file_aggregate_rope_references")?;
    if raw_digest != EXPECTED_RAW_DIGEST {
        return Err("master raw digest does not match frozen S1-100".to_owned());
    }
    if json_u64(&json, "bytes")? != FILE_BYTES
        || json_u64(&json, "cdc_references")? != EXPECTED_CDC_REFERENCES
        || json_string(&json, "cdc_sequence_blake3")? != EXPECTED_CDC_SEQUENCE
        || json_string(&json, "cdc_counter_scope")? != "independent_streamed_file_oracle"
        || json_u64(&json, "logical_file_mode")? != 0o644
        || json_u64(&json, "logical_mtime_seconds")? != 0
        || new_file_aggregate_rope_references < EXPECTED_CDC_REFERENCES
    {
        return Err("master generator population does not match poc/14".to_owned());
    }
    let bases_object = json_object(&json, "bases")?;
    let mut bases = BTreeMap::new();
    for name in BASES {
        let object = json_object(bases_object, name)?;
        let root = json_string(object, "root")?
            .parse::<RootId>()
            .map_err(display_error)?;
        let root_a = if *name == "refresh-a-b" {
            Some(
                json_string(object, "root_a")?
                    .parse::<RootId>()
                    .map_err(display_error)?,
            )
        } else {
            None
        };
        let root_b = if *name == "refresh-a-b" {
            Some(
                json_string(object, "root_b")?
                    .parse::<RootId>()
                    .map_err(display_error)?,
            )
        } else {
            None
        };
        bases.insert(
            (*name).to_owned(),
            BaseManifest {
                name: (*name).to_owned(),
                root,
                root_a,
                root_b,
                generation: json_u64(object, "generation")?,
                selector_generation: json_u64(object, "selector_generation")?,
                store_id: json_string(object, "store_id")?,
                profile_id: json_string(object, "profile_id")?,
                store_database_bytes: json_u64(object, "store_database_bytes")?,
            },
        );
    }
    Ok(Master {
        raw_digest,
        replacement_digest,
        inventory_digest,
        new_file_aggregate_rope_references,
        bases,
    })
}

fn write_master(path: &Path, master: &Master, preparation_wall_ns: u128) -> EvalResult<()> {
    let mut json = String::from("{\n");
    json.push_str("  \"schema\":\"layerfs-stage1-single-file-master-v1\",\n");
    json.push_str(&format!(
        "  \"generator\":{{\"version\":\"phase4-fill-retained-buffer-v1\",\"label\":\"S1-100\",\"seed\":81,\"bytes\":{FILE_BYTES},\"raw_blake3\":\"{}\",\"cdc_references\":{EXPECTED_CDC_REFERENCES},\"cdc_sequence_blake3\":\"{EXPECTED_CDC_SEQUENCE}\",\"cdc_counter_scope\":\"independent_streamed_file_oracle\",\"new_file_aggregate_rope_references\":{}}},\n",
        master.raw_digest,
        master.new_file_aggregate_rope_references,
    ));
    json.push_str(&format!(
        "  \"replacement_blake3\":\"{}\",\n  \"inventory_blake3\":\"{}\",\n  \"logical_file_mode\":420,\n  \"logical_mtime_seconds\":0,\n  \"preparation_wall_ns\":{preparation_wall_ns},\n  \"bases\":{{\n",
        master.replacement_digest, master.inventory_digest
    ));
    for (index, base) in master.bases.values().enumerate() {
        if index != 0 {
            json.push_str(",\n");
        }
        json.push_str(&format!(
            "    \"{}\":{{\"root\":\"{}\",{}\"generation\":{},\"selector_generation\":{},\"store_id\":\"{}\",\"profile_id\":\"{}\",\"store_database_bytes\":{}}}",
            base.name,
            base.root,
            match (base.root_a, base.root_b) {
                (Some(root_a), Some(root_b)) => {
                    format!("\"root_a\":\"{root_a}\",\"root_b\":\"{root_b}\",")
                }
                _ => String::new(),
            },
            base.generation,
            base.selector_generation,
            base.store_id,
            base.profile_id,
            base.store_database_bytes
        ));
    }
    json.push_str("\n  }\n}\n");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(json.as_bytes()).map_err(io_error)?;
    file.sync_all().map_err(io_error)
}

pub fn verify_master(root: &Path, master: &Master, full_hash: bool) -> EvalResult<String> {
    verify_sealed(root)?;
    if fs::metadata(root.join("input").join(FILE_PATH))
        .map_err(io_error)?
        .len()
        != FILE_BYTES
        || fs::metadata(root.join("input/S1-replace-100.bin"))
            .map_err(io_error)?
            .len()
            != FILE_BYTES
    {
        return Err("fixture input size mismatch".to_owned());
    }
    if full_hash {
        if hash_file(&root.join("input").join(FILE_PATH))? != master.raw_digest
            || hash_file(&root.join("input/S1-replace-100.bin"))? != master.replacement_digest
        {
            return Err("fixture input digest mismatch".to_owned());
        }
        let inventory = tree_digest(root, Some(Path::new("master.json")))?;
        if inventory != master.inventory_digest {
            return Err(format!(
                "sealed fixture inventory mismatch: expected {}, got {inventory}",
                master.inventory_digest
            ));
        }
    }
    tree_digest(root, None)
}

pub fn read_selector(store: &Path) -> EvalResult<Selector> {
    let bytes = fs::read(store.join("CURRENT")).map_err(io_error)?;
    if bytes.len() != 154
        || &bytes[..8] != b"LFSCUR1\0"
        || u16::from_be_bytes(bytes[8..10].try_into().unwrap()) != 1
        || u16::from_be_bytes(bytes[18..20].try_into().unwrap()) != 34
    {
        return Err("invalid Store selector framing".to_owned());
    }
    let generation = u64::from_be_bytes(bytes[10..18].try_into().unwrap());
    if std::str::from_utf8(&bytes[20..54]).map_err(display_error)?
        != format!("generation-{generation:016x}.sqlite")
    {
        return Err("Store selector filename mismatch".to_owned());
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/store-current/v1\0");
    hasher.update(&bytes[..122]);
    if hasher.finalize().as_bytes() != &bytes[122..] {
        return Err("Store selector checksum mismatch".to_owned());
    }
    Ok(Selector {
        generation,
        store_id: hex(&bytes[58..90]),
        profile_id: hex(&bytes[90..122]),
    })
}

pub fn selected_database_bytes(store: &Path, generation: u64) -> EvalResult<u64> {
    fs::metadata(store.join(format!("generation-{generation:016x}.sqlite")))
        .map(|metadata| metadata.len())
        .map_err(io_error)
}

impl Attempt {
    pub fn create(base: &str, expected: &BaseManifest) -> EvalResult<Self> {
        Self::create_from(&fixture_root(), base, expected)
    }

    pub fn create_from(fixture: &Path, base: &str, expected: &BaseManifest) -> EvalResult<Self> {
        let reset_started = Instant::now();
        let source = resolved_base_source(fixture, base)?;
        let attempts = workspace_root().join("target/layerfs-stage1-attempts");
        fs::create_dir_all(&attempts).map_err(io_error)?;
        let attempts = attempts.canonicalize().map_err(io_error)?;
        let serial = ATTEMPT_SERIAL.fetch_add(1, Ordering::Relaxed);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(display_error)?
            .as_nanos();
        let root = attempts.join(format!("attempt-{}-{nonce}-{serial}", std::process::id()));
        fs::create_dir(&root).map_err(io_error)?;
        let marker = format!(
            "layerfs-stage1-attempt:{}:{nonce}:{serial}",
            std::process::id()
        );
        let store = root.join("store");
        if let Err(error) = fs::write(root.join("OWNED"), marker.as_bytes()) {
            let _ = fs::remove_dir(&root);
            return Err(io_error(error));
        }
        let mut attempt = Self {
            root,
            store,
            marker,
            clone: CloneReceipt::default(),
        };
        let started = Instant::now();
        let status = Command::new("/bin/cp")
            .arg("-cR")
            .arg(&source)
            .arg(&attempt.store)
            .status()
            .map_err(io_error)?;
        let clone_wall_ns = started.elapsed().as_nanos();
        if !status.success() {
            return Err(format!(
                "APFS clone reset unavailable: /bin/cp -cR exited {status}"
            ));
        }
        let (source_logical_bytes, source_allocated_bytes) = tree_sizes(&source)?;
        let (destination_logical_bytes, destination_allocated_bytes) = tree_sizes(&attempt.store)?;
        if source_logical_bytes != destination_logical_bytes {
            return Err("clone reset logical-size mismatch".to_owned());
        }
        let distinct_regular_inodes = prove_distinct_inodes(&source, &attempt.store)?;
        let clone_id = strict_clone_id(&source, &attempt.store)?;
        make_writable(&attempt.root)?;
        let selector = read_selector(&attempt.store)?;
        if selector.store_id != expected.store_id
            || selector.profile_id != expected.profile_id
            || selector.generation != expected.selector_generation
        {
            return Err("clone reset StoreId/profile/CURRENT mismatch".to_owned());
        }
        attempt.clone = CloneReceipt {
            wall_ns: reset_started.elapsed().as_nanos(),
            clone_wall_ns,
            source_logical_bytes,
            destination_logical_bytes,
            source_allocated_bytes,
            destination_allocated_bytes,
            distinct_regular_inodes,
            clone_id,
        };
        Ok(attempt)
    }

    pub fn store(&self) -> &Path {
        &self.store
    }

    pub fn open(&self, expected: &BaseManifest, mode: IntegrityMode) -> EvalResult<OpenedLayerFs> {
        let selector = read_selector(&self.store)?;
        if selector.store_id != expected.store_id
            || selector.profile_id != expected.profile_id
            || selector.generation != expected.selector_generation
        {
            return Err("attempt selector identity mismatch".to_owned());
        }
        let opened = LayerFs::open_with_integrity(&self.store, mode).map_err(display_error)?;
        let head = opened.ref_state.clone();
        if head.root != expected.root || head.generation != expected.generation {
            return Err(format!(
                "attempt expected RefState mismatch for {}",
                expected.name
            ));
        }
        Ok(opened)
    }

    pub fn cleanup(mut self) -> EvalResult<()> {
        self.cleanup_inner()?;
        self.root = PathBuf::new();
        Ok(())
    }

    fn cleanup_inner(&self) -> EvalResult<()> {
        if self.root.as_os_str().is_empty() || !self.root.exists() {
            return Ok(());
        }
        let parent = workspace_root()
            .join("target/layerfs-stage1-attempts")
            .canonicalize()
            .map_err(io_error)?;
        let root = self.root.canonicalize().map_err(io_error)?;
        if root.parent() != Some(parent.as_path())
            || fs::read_to_string(root.join("OWNED")).map_err(io_error)? != self.marker
        {
            return Err(format!(
                "refusing unowned attempt cleanup: {}",
                root.display()
            ));
        }
        make_writable(&root)?;
        fs::remove_dir_all(root).map_err(io_error)
    }
}

impl Drop for Attempt {
    fn drop(&mut self) {
        let _ = self.cleanup_inner();
    }
}

pub fn assert_apfs(path: &Path) -> EvalResult<String> {
    if !cfg!(target_os = "macos") {
        return Err("strict Stage One reset requires macOS APFS".to_owned());
    }
    let path = path.canonicalize().map_err(io_error)?;
    let df = Command::new("/bin/df")
        .arg("-P")
        .arg(&path)
        .output()
        .map_err(io_error)?;
    if !df.status.success() {
        return Err("df could not identify fixture volume".to_owned());
    }
    let df = String::from_utf8(df.stdout).map_err(display_error)?;
    let mut rows = df.lines().skip(1).filter(|line| !line.trim().is_empty());
    let device = rows
        .next()
        .and_then(|line| line.split_whitespace().next())
        .filter(|device| device.starts_with("/dev/"))
        .ok_or_else(|| "df did not return a local fixture volume device".to_owned())?;
    if rows.next().is_some() {
        return Err("df returned multiple fixture volume devices".to_owned());
    }
    let output = Command::new("/usr/sbin/diskutil")
        .arg("info")
        .arg(device)
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err("diskutil could not identify fixture volume".to_owned());
    }
    let text = String::from_utf8(output.stdout).map_err(display_error)?;
    apfs_identity(device, &text)
}

fn apfs_identity(device: &str, text: &str) -> EvalResult<String> {
    fn value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
        text.lines().find_map(|line| {
            let (candidate, value) = line.split_once(':')?;
            (candidate.trim() == key)
                .then_some(value.trim())
                .filter(|value| !value.is_empty())
        })
    }

    let node = value(text, "Device Node")
        .ok_or_else(|| "diskutil did not report the fixture device node".to_owned())?;
    let identifier = value(text, "Device Identifier")
        .ok_or_else(|| "diskutil did not report the fixture device identifier".to_owned())?;
    let personality = value(text, "File System Personality")
        .ok_or_else(|| "diskutil did not report the fixture filesystem".to_owned())?;
    let bundle = value(text, "Type (Bundle)")
        .ok_or_else(|| "diskutil did not report the fixture filesystem bundle".to_owned())?;
    let mount = value(text, "Mount Point")
        .ok_or_else(|| "diskutil did not report the fixture mount point".to_owned())?;
    if node != device
        || !personality.eq_ignore_ascii_case("apfs")
        || !bundle.eq_ignore_ascii_case("apfs")
    {
        return Err("fixture volume is not identified as APFS".to_owned());
    }
    let volume_uuid = value(text, "Volume UUID")
        .ok_or_else(|| "diskutil did not report the fixture volume UUID".to_owned())?;
    let partition_uuid = value(text, "Disk / Partition UUID")
        .ok_or_else(|| "diskutil did not report the fixture partition UUID".to_owned())?;
    Ok(format!(
        "device_identifier={identifier};device_node={node};volume_uuid={volume_uuid};partition_uuid={partition_uuid};personality=apfs;type=apfs;mount_point={mount}"
    ))
}

fn verify_fresh_reopens(
    root: &Path,
    master: &Master,
    progress: &mut PreparationProgress,
) -> EvalResult<()> {
    for name in BASES {
        progress.set("verify-fresh-reopen", Some(name));
        let expected = master
            .bases
            .get(*name)
            .ok_or_else(|| format!("missing base {name}"))?;
        let attempt = Attempt::create_from(root, name, expected)?;
        let opened = LayerFs::open(attempt.store()).map_err(display_error)?;
        let head = opened.fs.current_head("main").map_err(display_error)?;
        if head.root != expected.root || head.generation != expected.generation {
            return Err(format!("fresh reopen mismatch for {name}"));
        }
        drop(opened);
        attempt.cleanup()?;
    }
    verify_sealed(root)
}

fn resolved_base_source(fixture: &Path, base: &str) -> EvalResult<PathBuf> {
    let fixture = fixture.canonicalize().map_err(io_error)?;
    let source = fixture.join("bases").join(base);
    let source = source.canonicalize().map_err(io_error)?;
    if source != fixture.join("bases").join(base) {
        return Err("base is not the exact sealed fixture path".to_owned());
    }
    Ok(source)
}

pub(crate) fn clone_directory(source: &Path, destination: &Path) -> EvalResult<()> {
    if destination.exists() {
        return Err(format!("refusing to overwrite {}", destination.display()));
    }
    let status = Command::new("/bin/cp")
        .arg("-cR")
        .arg(source)
        .arg(destination)
        .status()
        .map_err(io_error)?;
    if !status.success() {
        return Err(format!("APFS fixture clone failed with {status}"));
    }
    prove_distinct_inodes(source, destination)?;
    strict_clone_id(source, destination)?;
    Ok(())
}

fn strict_clone_id(source: &Path, destination: &Path) -> EvalResult<u64> {
    let selector = read_selector(source)?;
    let file = format!("generation-{:016x}.sqlite", selector.generation);
    let source_id = clone_id(&source.join(&file))?;
    let destination_id = clone_id(&destination.join(file))?;
    if source_id == 0 || source_id != destination_id {
        return Err(format!(
            "strict APFS clone proof failed: source clone ID {source_id}, destination clone ID {destination_id}"
        ));
    }
    Ok(source_id)
}

#[cfg(target_os = "macos")]
fn clone_id(path: &Path) -> EvalResult<u64> {
    use std::ffi::{c_char, c_int, c_ulong, c_void, CString};

    const FSOPT_ATTR_CMN_EXTENDED: c_ulong = 0x0000_0020;

    #[repr(C)]
    struct AttrList {
        bitmap_count: u16,
        reserved: u16,
        common: u32,
        volume: u32,
        directory: u32,
        file: u32,
        fork: u32,
    }

    unsafe extern "C" {
        fn getattrlist(
            path: *const c_char,
            attributes: *mut c_void,
            buffer: *mut c_void,
            size: usize,
            options: c_ulong,
        ) -> c_int;
    }

    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| "clone-ID path contains NUL".to_owned())?;
    let mut attributes = AttrList {
        bitmap_count: 5,
        reserved: 0,
        common: 0,
        volume: 0,
        directory: 0,
        file: 0,
        fork: 0x0000_0100,
    };
    let mut buffer = [0_u8; 12];
    // SAFETY: all pointers refer to live, correctly sized C-compatible values for
    // the duration of the call; getattrlist writes at most buffer.len() bytes.
    let result = unsafe {
        getattrlist(
            path.as_ptr(),
            (&mut attributes as *mut AttrList).cast(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            FSOPT_ATTR_CMN_EXTENDED,
        )
    };
    if result != 0 || u32::from_ne_bytes(buffer[..4].try_into().unwrap()) != 12 {
        return Err(format!(
            "ATTR_CMNEXT_CLONEID unavailable for {}: {}",
            path.to_string_lossy(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(u64::from_ne_bytes(buffer[4..12].try_into().unwrap()))
}

#[cfg(not(target_os = "macos"))]
fn clone_id(_path: &Path) -> EvalResult<u64> {
    Err("ATTR_CMNEXT_CLONEID requires macOS".to_owned())
}

fn prove_distinct_inodes(source: &Path, destination: &Path) -> EvalResult<u64> {
    let mut files = Vec::new();
    collect_paths(source, source, &mut files)?;
    let mut count = 0_u64;
    for relative in files {
        let source_metadata = fs::symlink_metadata(source.join(&relative)).map_err(io_error)?;
        let destination_metadata =
            fs::symlink_metadata(destination.join(&relative)).map_err(io_error)?;
        if source_metadata.file_type().is_symlink() || destination_metadata.file_type().is_symlink()
        {
            return Err("fixture/reset may not contain symlinks".to_owned());
        }
        if source_metadata.is_file() {
            if !destination_metadata.is_file()
                || source_metadata.len() != destination_metadata.len()
                || (source_metadata.dev(), source_metadata.ino())
                    == (destination_metadata.dev(), destination_metadata.ino())
            {
                return Err(format!(
                    "clone inode/size proof failed for {}",
                    relative.display()
                ));
            }
            count += 1;
        }
    }
    if count == 0 {
        return Err("clone proof found no regular files".to_owned());
    }
    Ok(count)
}

pub(crate) fn tree_sizes(root: &Path) -> EvalResult<(u64, u64)> {
    fn walk(path: &Path, logical: &mut u64, allocated: &mut u64) -> EvalResult<()> {
        let mut entries = fs::read_dir(path)
            .map_err(io_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(io_error)?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
            if metadata.file_type().is_symlink() {
                return Err("fixture/reset may not contain symlinks".to_owned());
            }
            if metadata.is_dir() {
                walk(&entry.path(), logical, allocated)?;
            } else if metadata.is_file() {
                *logical = logical
                    .checked_add(metadata.len())
                    .ok_or_else(|| "tree logical size overflow".to_owned())?;
                *allocated = allocated
                    .checked_add(metadata.blocks().saturating_mul(512))
                    .ok_or_else(|| "tree allocated size overflow".to_owned())?;
            }
        }
        Ok(())
    }
    let mut logical = 0;
    let mut allocated = 0;
    walk(root, &mut logical, &mut allocated)?;
    Ok((logical, allocated))
}

pub fn tree_digest(root: &Path, exclude: Option<&Path>) -> EvalResult<String> {
    let mut paths = Vec::new();
    collect_paths(root, root, &mut paths)?;
    paths.sort();
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    for relative in paths {
        if exclude == Some(relative.as_path()) {
            continue;
        }
        let path = root.join(&relative);
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        if metadata.file_type().is_symlink() {
            return Err("fixture may not contain symlinks".to_owned());
        }
        hasher.update(relative.as_os_str().as_encoded_bytes());
        hasher.update(&[if metadata.is_dir() { b'd' } else { b'f' }]);
        hasher.update(&(if metadata.is_dir() { 0o555_u32 } else { 0o444 }).to_be_bytes());
        hasher.update(&metadata.len().to_be_bytes());
        if metadata.is_file() {
            let mut file = File::open(path).map_err(io_error)?;
            loop {
                let read = file.read(&mut buffer).map_err(io_error)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
        }
    }
    Ok(hasher.finalize().to_hex().to_string())
}

pub fn maximum_regular_file(root: &Path) -> EvalResult<Option<(PathBuf, u64)>> {
    let mut paths = Vec::new();
    collect_paths(root, root, &mut paths)?;
    let mut maximum = None;
    for relative in paths {
        let metadata = fs::symlink_metadata(root.join(&relative)).map_err(io_error)?;
        if metadata.is_file()
            && maximum
                .as_ref()
                .is_none_or(|(_, bytes)| metadata.len() > *bytes)
        {
            maximum = Some((relative, metadata.len()));
        }
    }
    Ok(maximum)
}

pub fn verify_user_file_ceiling(root: &Path) -> EvalResult<()> {
    if let Some((path, bytes)) = maximum_regular_file(root)? {
        if bytes > FILE_BYTES {
            return Err(format!(
                "user input/intermediate/output {} is {bytes} bytes (> {FILE_BYTES})",
                path.display()
            ));
        }
    }
    Ok(())
}

fn collect_paths(root: &Path, path: &Path, output: &mut Vec<PathBuf>) -> EvalResult<()> {
    let mut entries = fs::read_dir(path)
        .map_err(io_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_error)?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let entry_path = entry.path();
        let relative = entry_path
            .strip_prefix(root)
            .map_err(display_error)?
            .to_owned();
        output.push(relative);
        let metadata = fs::symlink_metadata(&entry_path).map_err(io_error)?;
        if metadata.is_dir() {
            collect_paths(root, &entry_path, output)?;
        }
    }
    Ok(())
}

pub(crate) fn seal_tree(root: &Path) -> EvalResult<()> {
    fn walk(path: &Path) -> EvalResult<()> {
        for entry in fs::read_dir(path).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
            if metadata.file_type().is_symlink() {
                return Err("fixture may not contain symlinks".to_owned());
            }
            if metadata.is_dir() {
                walk(&path)?;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o555)).map_err(io_error)?;
            } else {
                fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).map_err(io_error)?;
            }
        }
        Ok(())
    }
    walk(root)?;
    fs::set_permissions(root, fs::Permissions::from_mode(0o555)).map_err(io_error)
}

pub(crate) fn verify_sealed(root: &Path) -> EvalResult<()> {
    let mut paths = vec![PathBuf::new()];
    collect_paths(root, root, &mut paths)?;
    for relative in paths {
        let metadata = fs::symlink_metadata(root.join(&relative)).map_err(io_error)?;
        let expected = if metadata.is_dir() { 0o555 } else { 0o444 };
        if metadata.file_type().is_symlink() || metadata.permissions().mode() & 0o777 != expected {
            return Err(format!("fixture seal mismatch at {}", relative.display()));
        }
    }
    Ok(())
}

pub(crate) fn make_writable(root: &Path) -> EvalResult<()> {
    fn walk(path: &Path) -> EvalResult<()> {
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(io_error)?;
        for entry in fs::read_dir(path).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
            if metadata.file_type().is_symlink() {
                return Err("attempt may not contain symlinks".to_owned());
            }
            if metadata.is_dir() {
                walk(&path)?;
            } else {
                fs::set_permissions(path, fs::Permissions::from_mode(0o644)).map_err(io_error)?;
            }
        }
        Ok(())
    }
    walk(root)
}

pub(crate) fn sync_directory(path: &Path) -> EvalResult<()> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(io_error)
}

fn json_string(json: &str, key: &str) -> EvalResult<String> {
    let needle = format!("\"{key}\":\"");
    let start = json
        .find(&needle)
        .ok_or_else(|| format!("missing JSON string {key}"))?
        + needle.len();
    let end = json[start..]
        .find('"')
        .ok_or_else(|| format!("unterminated JSON string {key}"))?
        + start;
    Ok(json[start..end].to_owned())
}

fn json_u64(json: &str, key: &str) -> EvalResult<u64> {
    let needle = format!("\"{key}\":");
    let start = json
        .find(&needle)
        .ok_or_else(|| format!("missing JSON integer {key}"))?
        + needle.len();
    let end = json[start..]
        .find(|character: char| !character.is_ascii_digit())
        .map_or(json.len(), |offset| start + offset);
    json[start..end].parse::<u64>().map_err(display_error)
}

fn json_object<'a>(json: &'a str, key: &str) -> EvalResult<&'a str> {
    let needle = format!("\"{key}\":{{");
    let start = json
        .find(&needle)
        .ok_or_else(|| format!("missing JSON object {key}"))?
        + needle.len()
        - 1;
    let mut depth = 0_u64;
    for (offset, byte) in json.as_bytes()[start..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(&json[start..=start + offset]);
                }
            }
            _ => {}
        }
    }
    Err(format!("unterminated JSON object {key}"))
}

pub fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn artifact_json_escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect(),
            '\n' => "\\n".chars().collect(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            character if character.is_control() => "?".chars().collect(),
            character => vec![character],
        })
        .collect()
}

fn io_error(error: std::io::Error) -> String {
    error.to_string()
}

fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_directory(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "layerfs-eval-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn retained_generator_is_chunk_stable() {
        let mut complete = vec![0_u8; BUFFER_BYTES];
        fill_retained_buffer(&mut complete, 0);
        assert_eq!(
            expected_bytes(8_000, 20_000).unwrap(),
            complete[8_000..28_000]
        );
    }

    #[test]
    fn retained_generator_matches_frozen_digest() {
        let mut hasher = blake3::Hasher::new();
        let mut buffer = vec![0_u8; BUFFER_BYTES];
        let mut offset = 0_u64;
        while offset < FILE_BYTES {
            fill_retained_buffer(&mut buffer, offset);
            hasher.update(&buffer);
            offset += BUFFER_BYTES as u64;
        }
        assert_eq!(hasher.finalize().to_hex().as_str(), EXPECTED_RAW_DIGEST);
    }

    #[test]
    fn json_helpers_read_nested_objects() {
        let json = r#"{"bases":{"x":{"root":"abc","generation":2}}}"#;
        let bases = json_object(json, "bases").unwrap();
        let x = json_object(bases, "x").unwrap();
        assert_eq!(json_string(x, "root").unwrap(), "abc");
        assert_eq!(json_u64(x, "generation").unwrap(), 2);
    }

    #[test]
    fn preparation_reopen_source_uses_private_fixture_root() {
        let root = test_directory("private-fixture-root");
        let base = root.join("bases/read-reconstruct");
        fs::create_dir_all(&base).unwrap();
        assert_eq!(
            resolved_base_source(&root, "read-reconstruct").unwrap(),
            base.canonicalize().unwrap()
        );
        assert_ne!(root.canonicalize().unwrap(), fixture_root());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preparation_failure_receipts_are_append_only_and_context_bound() {
        let root = test_directory("failure-receipts");
        let obsolete = root.join("stage1-preparation-failure.json");
        fs::write(&obsolete, "obsolete\n").unwrap();
        let progress = PreparationProgress {
            phase: "verify-fresh-reopen",
            base: Some("read-reconstruct".to_owned()),
        };
        let first = write_preparation_failure(&root, &progress, "first failure").unwrap();
        let second = write_preparation_failure(&root, &progress, "second failure").unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read_to_string(obsolete).unwrap(), "obsolete\n");
        let receipt = fs::read_to_string(first).unwrap();
        assert!(receipt.contains("\"phase\":\"verify-fresh-reopen\""));
        assert!(receipt.contains("\"base\":\"read-reconstruct\""));
        assert!(receipt.contains("\"source_tree_blake3\""));
        assert!(receipt.contains("\"executable_blake3\""));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn inventory_digest_normalizes_the_sealed_permission_contract() {
        let root = test_directory("inventory-permissions");
        let file = root.join("entry");
        fs::write(&file, b"inventory").unwrap();
        let writable = tree_digest(&root, None).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o444)).unwrap();
        assert_eq!(writable, tree_digest(&root, None).unwrap());
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn clone_id_proves_apfs_copy_on_write_pair() {
        let root = test_directory("clone-id");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::write(&source, vec![0x42; 8_192]).unwrap();
        assert!(Command::new("/bin/cp")
            .arg("-c")
            .arg(&source)
            .arg(&destination)
            .status()
            .unwrap()
            .success());

        let source_metadata = fs::metadata(&source).unwrap();
        let destination_metadata = fs::metadata(&destination).unwrap();
        let source_id = clone_id(&source).unwrap();
        assert_ne!(source_id, 0);
        assert_eq!(source_id, clone_id(&destination).unwrap());
        assert_ne!(
            (source_metadata.dev(), source_metadata.ino()),
            (destination_metadata.dev(), destination_metadata.ino())
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn failed_attempt_admission_cleans_its_owned_clone() {
        let root = test_directory("attempt-cleanup").canonicalize().unwrap();
        let fixture = root.join("fixture");
        let base = fixture.join("bases/read-reconstruct");
        fs::create_dir_all(base.parent().unwrap()).unwrap();
        let opened = LayerFs::open(&base).unwrap();
        let state = opened.ref_state.clone();
        drop(opened);
        let selector = read_selector(&base).unwrap();
        let expected = BaseManifest {
            name: "read-reconstruct".to_owned(),
            root: state.root,
            root_a: None,
            root_b: None,
            generation: state.generation,
            selector_generation: selector.generation,
            store_id: "deliberately-wrong-store-id".to_owned(),
            profile_id: selector.profile_id,
            store_database_bytes: selected_database_bytes(&base, selector.generation).unwrap(),
        };
        let attempts = workspace_root().join("target/layerfs-stage1-attempts");
        fs::create_dir_all(&attempts).unwrap();
        let before = fs::read_dir(&attempts).unwrap().count();
        assert!(Attempt::create_from(&fixture, "read-reconstruct", &expected).is_err());
        assert_eq!(fs::read_dir(&attempts).unwrap().count(), before);

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sealed_root = attempts.join(format!(
            "attempt-{}-{nonce}-sealed-cleanup",
            std::process::id()
        ));
        let sealed_store = sealed_root.join("store/nested");
        fs::create_dir_all(&sealed_store).unwrap();
        let marker = format!("sealed-cleanup-{nonce}");
        fs::write(sealed_root.join("OWNED"), &marker).unwrap();
        fs::write(sealed_store.join("file"), b"sealed").unwrap();
        fs::set_permissions(&sealed_store, fs::Permissions::from_mode(0o555)).unwrap();
        fs::set_permissions(
            sealed_store.parent().unwrap(),
            fs::Permissions::from_mode(0o555),
        )
        .unwrap();
        drop(Attempt {
            root: sealed_root.clone(),
            store: sealed_root.join("store"),
            marker,
            clone: CloneReceipt::default(),
        });
        assert!(!sealed_root.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn apfs_preflight_resolves_the_containing_volume() {
        let root = test_directory("apfs-preflight");
        let nested = root.join("directory with spaces");
        fs::create_dir(&nested).unwrap();
        let identity = assert_apfs(&root).unwrap();
        assert!(identity.contains("personality=apfs;type=apfs"));
        assert_eq!(identity, assert_apfs(&nested).unwrap());
        let allocation = nested.join("temporary allocation");
        fs::write(&allocation, vec![0x42; 1024 * 1024]).unwrap();
        assert_eq!(identity, assert_apfs(&nested).unwrap());
        fs::remove_file(allocation).unwrap();
        assert_eq!(identity, assert_apfs(&nested).unwrap());
        assert!(assert_apfs(&root.join("missing")).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn apfs_parser_rejects_deceptive_personality_text() {
        let text = concat!(
            "Device Node: /dev/disk-test\n",
            "Device Identifier: disk-test\n",
            "Mount Point: /fixture\n",
            "File System Personality: Not APFS\n",
            "Type (Bundle): apfs\n",
            "Volume UUID: volume\n",
            "Disk / Partition UUID: partition\n",
        );
        assert!(apfs_identity("/dev/disk-test", text).is_err());
    }
}
