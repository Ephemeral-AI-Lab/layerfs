use super::apfs::{assert_apfs, clone_directory};
use super::cdc;
use super::contract::{
    Attempt, BaseManifest, EvalResult, Master, BASES, BUFFER_BYTES, EXPECTED_CDC_REFERENCES,
    EXPECTED_CDC_SEQUENCE, EXPECTED_RAW_DIGEST, FILE_BYTES, FILE_PATH, FIXTURE_VERSION,
};
use super::error::{display_error, io_error};
use super::location::fixture_root;
use super::master::write_master;
use super::oracle::{edit_bytes, generate_input};
use super::preparation_receipt::write_preparation_failure;
use super::selector::{read_selector, selected_database_bytes};
use super::tree::{
    make_writable, seal_tree, sync_directory, tree_digest, verify_sealed, verify_user_file_ceiling,
};
use crate::legacy_full::{IntegrityMode, LayerFs, RefState};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::time::Instant;

#[derive(Clone, Debug)]
pub(super) struct PreparationProgress {
    pub(super) phase: &'static str,
    pub(super) base: Option<String>,
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
    let cdc = cdc::scan_file(&raw)?;
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
