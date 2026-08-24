use layerfs_sdk::{
    CompactionDiagnostics, Diagnostics, LayerFs, OperationDiagnostics, RootId, VfsError,
};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, UNIX_EPOCH};

const MAX_FIXTURE_FILE_BYTES: usize = 100 * 1024 * 1024;
const LARGE_FIXTURE_BYTES: usize = 3 * 1024 * 1024;
const MANAGED_FIXTURE_BYTES: usize = 1024 * 1024;
const _: () = assert!(LARGE_FIXTURE_BYTES <= MAX_FIXTURE_FILE_BYTES);
const _: () = assert!(MANAGED_FIXTURE_BYTES <= MAX_FIXTURE_FILE_BYTES);

#[derive(Clone, Debug)]
pub struct PocReceipt {
    pub root_a: RootId,
    pub root_final: RootId,
    pub wall: Duration,
    pub store_bytes: u64,
    pub residue: Vec<PathBuf>,
    pub page_size: i64,
    pub cache_pages: i64,
    pub database_bytes: Option<u64>,
    pub logical_engine_bytes: Option<u64>,
    pub diagnostics: Diagnostics,
    pub compaction: CompactionDiagnostics,
    pub fd_baseline: usize,
    pub fd_terminal: usize,
    pub stages: Vec<StageReceipt>,
    pub operations: Vec<OperationReceipt>,
    pub operation_q_bound_bytes: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct StageReceipt {
    pub label: &'static str,
    pub root: RootId,
    pub diagnostics: Diagnostics,
}

#[derive(Clone, Copy, Debug)]
pub struct OperationReceipt {
    pub operation: &'static str,
    pub api_route: &'static str,
    pub wall_ns: u128,
    pub root_before: RootId,
    pub root_after: RootId,
    pub diagnostics: Option<(Diagnostics, Diagnostics)>,
    pub operation_diagnostics: Option<OperationDiagnostics>,
}

fn operation_receipt(
    operation: &'static str,
    api_route: &'static str,
    wall_ns: u128,
    root_before: RootId,
    root_after: RootId,
    before: Diagnostics,
    after: Diagnostics,
) -> OperationReceipt {
    OperationReceipt {
        operation,
        api_route,
        wall_ns,
        root_before,
        root_after,
        diagnostics: Some((before, after)),
        operation_diagnostics: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn operation_receipt_observed(
    operation: &'static str,
    api_route: &'static str,
    wall_ns: u128,
    root_before: RootId,
    root_after: RootId,
    before: Diagnostics,
    after: Diagnostics,
    operation_diagnostics: OperationDiagnostics,
) -> OperationReceipt {
    OperationReceipt {
        operation,
        api_route,
        wall_ns,
        root_before,
        root_after,
        diagnostics: Some((before, after)),
        operation_diagnostics: Some(operation_diagnostics),
    }
}

fn operation_without_diagnostics(
    operation: &'static str,
    api_route: &'static str,
    wall_ns: u128,
    root_before: RootId,
    root_after: RootId,
) -> OperationReceipt {
    OperationReceipt {
        operation,
        api_route,
        wall_ns,
        root_before,
        root_after,
        diagnostics: None,
        operation_diagnostics: None,
    }
}

#[derive(Debug, Eq, PartialEq)]
struct TreeEntry {
    kind: u8,
    mode: u32,
    modified: (i64, i64),
    body: Vec<u8>,
    hard_link_group: Option<usize>,
}

fn run_workflow(base: &Path) -> Result<PocReceipt, Box<dyn std::error::Error>> {
    let fd_baseline = fd_count()?;
    fs::create_dir(base)?;
    let started = Instant::now();
    let store = base.join("store");
    let opened = LayerFs::open(&store)?;
    let mut stages = vec![stage_receipt("S0", opened.head, &opened.fs)?];
    let mut operations = Vec::new();
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))?;
    fs::create_dir_all(source.path().join("nested/scripts"))?;
    fs::write(source.path().join("empty"), [])?;
    let mut initial_large = vec![0_u8; LARGE_FIXTURE_BYTES];
    for (index, byte) in initial_large.iter_mut().enumerate() {
        *byte = deterministic_byte(index);
    }
    fs::write(source.path().join("nested/large.bin"), &initial_large)?;
    drop(initial_large);
    fs::write(
        source.path().join("nested/managed.bin"),
        vec![0x6d; MANAGED_FIXTURE_BYTES],
    )?;
    fs::write(
        source.path().join("nested/scripts/run.sh"),
        b"#!/bin/bash\nprintf layerfs-poc\n",
    )?;
    fs::set_permissions(
        source.path().join("nested/scripts/run.sh"),
        fs::Permissions::from_mode(0o755),
    )?;
    let script = source.path().join("nested/scripts/run.sh");
    command_ok(
        Command::new("chmod")
            .args(["+a", "everyone allow read"])
            .arg(&script),
    )?;
    command_ok(Command::new("chflags").arg("hidden").arg(&script))?;
    command_ok(
        Command::new("xattr")
            .args([
                "-wx",
                "com.apple.FinderInfo",
                "0000000000000000000000000000000000000000000000000000000000000001",
            ])
            .arg(&script),
    )?;
    command_ok(
        Command::new("xattr")
            .args(["-w", "com.apple.ResourceFork", "resource-fork"])
            .arg(&script),
    )?;
    fs::File::open(&script)?.set_times(
        fs::FileTimes::new().set_modified(UNIX_EPOCH.checked_sub(Duration::from_secs(2)).unwrap()),
    )?;
    fs::hard_link(
        source.path().join("nested/large.bin"),
        source.path().join("large-hardlink"),
    )?;
    symlink("nested/large.bin", source.path().join("relative-link"))?;
    symlink(
        "/private/tmp/layerfs-absolute-target",
        source.path().join("absolute-link"),
    )?;
    symlink("missing", source.path().join("dangling-link"))?;
    let before_capture = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let (root_a, operation_diagnostics) = source.capture_quiescent_observed()?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    let after_capture = opened.fs.diagnostics()?;
    assert_one_commit(before_capture, after_capture)?;
    operations.push(operation_receipt_observed(
        "import-capture",
        "external-capture",
        operation_wall_ns,
        opened.head,
        root_a,
        before_capture,
        after_capture,
        operation_diagnostics,
    ));
    let before_fork = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    assert_eq!(opened.fs.fork(root_a, "root-a")?, root_a);
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt(
        "fork",
        "api-fork",
        operation_wall_ns,
        root_a,
        root_a,
        before_fork,
        opened.fs.diagnostics()?,
    ));
    stages.push(stage_receipt("S1", root_a, &opened.fs)?);
    let source_path = source.path().to_owned();
    drop(source);

    let before_cold = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let (cold, operation_diagnostics) = opened
        .fs
        .materialize_external_observed(root_a, &base.join("cold"))?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt_observed(
        "cold-materialize",
        "external-materialize",
        operation_wall_ns,
        root_a,
        root_a,
        before_cold,
        opened.fs.diagnostics()?,
        operation_diagnostics,
    ));
    let before_warm = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let (mut warm, operation_diagnostics) = opened
        .fs
        .materialize_external_observed(root_a, cold.path())?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt_observed(
        "live-warm-no-op",
        "external-materialize",
        operation_wall_ns,
        root_a,
        root_a,
        before_warm,
        opened.fs.diagnostics()?,
        operation_diagnostics,
    ));
    assert_tree_equal(&source_path, cold.path())?;
    assert_apple_metadata(&cold.path().join("nested/scripts/run.sh"))?;
    if warm.capture_quiescent()? != root_a {
        return Err("S2 exact-live no-op changed the canonical root".into());
    }
    stages.push(stage_receipt("S2", root_a, &opened.fs)?);

    let before_edit = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let mut managed = opened.fs.materialize_managed(root_a)?;
    let operation_diagnostics =
        managed.replace_observed("nested/managed.bin", 4096, 4096, &vec![0xa5; 4096])?;
    let (root_s3, capture_diagnostics) = managed.capture_observed()?;
    let operation_diagnostics = operation_diagnostics.merge(capture_diagnostics)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt_observed(
        "managed-overwrite",
        "managed-replace-capture",
        operation_wall_ns,
        root_a,
        root_s3,
        before_edit,
        opened.fs.diagnostics()?,
        operation_diagnostics,
    ));
    stages.push(stage_receipt("S3", root_s3, &opened.fs)?);
    let before_edit = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let mut managed = opened.fs.materialize_managed(root_s3)?;
    let operation_diagnostics =
        managed.replace_observed("nested/managed.bin", 8192, 0, &vec![0x3c; 8192])?;
    let (root_s4, capture_diagnostics) = managed.capture_observed()?;
    let operation_diagnostics = operation_diagnostics.merge(capture_diagnostics)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt_observed(
        "managed-insert",
        "managed-replace-capture",
        operation_wall_ns,
        root_s3,
        root_s4,
        before_edit,
        opened.fs.diagnostics()?,
        operation_diagnostics,
    ));
    stages.push(stage_receipt("S4", root_s4, &opened.fs)?);
    let before_edit = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let mut managed = opened.fs.materialize_managed(root_s4)?;
    let operation_diagnostics =
        managed.replace_observed("nested/managed.bin", 16_384, 4096, &[])?;
    let (root_s5, capture_diagnostics) = managed.capture_observed()?;
    let operation_diagnostics = operation_diagnostics.merge(capture_diagnostics)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt_observed(
        "managed-delete",
        "managed-replace-capture",
        operation_wall_ns,
        root_s4,
        root_s5,
        before_edit,
        opened.fs.diagnostics()?,
        operation_diagnostics,
    ));
    stages.push(stage_receipt("S5", root_s5, &opened.fs)?);
    let before_edit = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let mut managed = opened.fs.materialize_managed(root_s5)?;
    let operation_diagnostics =
        managed.replace_observed("nested/managed.bin", 1_048_576, 4096, &[])?;
    let (root_s6_truncated, capture_diagnostics) = managed.capture_observed()?;
    let operation_diagnostics = operation_diagnostics.merge(capture_diagnostics)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt_observed(
        "managed-truncate",
        "managed-replace-capture",
        operation_wall_ns,
        root_s5,
        root_s6_truncated,
        before_edit,
        opened.fs.diagnostics()?,
        operation_diagnostics,
    ));
    let before_edit = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let mut managed = opened.fs.materialize_managed(root_s6_truncated)?;
    let operation_diagnostics = managed.rename_observed("relative-link", "managed-link")?;
    let (root_s6, capture_diagnostics) = managed.capture_observed()?;
    let operation_diagnostics = operation_diagnostics.merge(capture_diagnostics)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt_observed(
        "managed-rename",
        "managed-rename-capture",
        operation_wall_ns,
        root_s6_truncated,
        root_s6,
        before_edit,
        opened.fs.diagnostics()?,
        operation_diagnostics,
    ));
    let s6_oracle = opened
        .fs
        .materialize_external(root_s6, &base.join("s6-oracle"))?;
    assert_managed_root(s6_oracle.path(), 6)?;
    drop(s6_oracle);
    stages.push(stage_receipt("S6", root_s6, &opened.fs)?);

    let mut external = opened
        .fs
        .materialize_external(root_s6, &base.join("external"))?;
    let before_execute = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let execute = Command::new("/bin/bash")
        .current_dir(external.path())
        .arg("nested/scripts/run.sh")
        .status()?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    assert!(execute.success());
    operations.push(operation_receipt(
        "bash-execute",
        "native-bash",
        operation_wall_ns,
        root_s6,
        root_s6,
        before_execute,
        opened.fs.diagnostics()?,
    ));
    stages.push(stage_receipt("S7", root_s6, &opened.fs)?);
    let writer = external.register_writer()?;
    assert!(matches!(
        external.capture_quiescent(),
        Err(VfsError::WorkspaceBusy)
    ));
    drop(writer);
    let nonzero = Command::new("/bin/bash")
        .current_dir(external.path())
        .arg("-c")
        .arg("printf uncommitted > nonzero.txt; exit 7")
        .status()?;
    if nonzero.code() != Some(7) || LayerFs::open(&store)?.head != root_s6 {
        return Err("nonzero shell status was lost or implicitly published".into());
    }
    let before_mutation = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let mutation = Command::new("/bin/bash")
        .current_dir(external.path())
        .arg("-c")
        .arg("printf shell > add.txt; mkdir made; mv add.txt made/moved.txt; rm empty; chmod 700 nested/scripts/run.sh; ln -s ../made/moved.txt nested/shell-link; dd if=/dev/zero of=nested/large.bin bs=4096 count=1 conv=notrunc 2>/dev/null; printf append > append.txt; printf tail >> append.txt; truncate -s 6 append.txt; ln nested/large.bin bash-hardlink; xattr -w com.layerfs.eval exact nested/large.bin")
        .status()?;
    if !mutation.success() {
        return Err("S8 Bash mutation failed".into());
    }
    let mmap = Command::new("/usr/bin/python3")
        .current_dir(external.path())
        .arg("-c")
        .arg("import mmap; f=open('nested/large.bin','r+b'); m=mmap.mmap(f.fileno(),0); m[32768:32772]=b'MMAP'; m.flush(); m.close(); f.close()")
        .status()?;
    if !mmap.success() {
        return Err("S8 mmap mutation failed".into());
    }
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt(
        "bash-mmap-mutation",
        "native-bash-python-mmap",
        operation_wall_ns,
        root_s6,
        root_s6,
        before_mutation,
        opened.fs.diagnostics()?,
    ));
    stages.push(stage_receipt("S8", root_s6, &opened.fs)?);
    let before_capture = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let (root_final, operation_diagnostics) = external.capture_quiescent_observed()?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    let after_capture = opened.fs.diagnostics()?;
    assert_one_commit(before_capture, after_capture)?;
    if after_capture.objects_reused == before_capture.objects_reused {
        return Err("S9 external capture did not reuse any authenticated object".into());
    }
    operations.push(operation_receipt_observed(
        "external-capture",
        "external-capture",
        operation_wall_ns,
        root_s6,
        root_final,
        before_capture,
        after_capture,
        operation_diagnostics,
    ));
    let final_native_path = external.path().to_owned();
    stages.push(stage_receipt("S9", root_final, &opened.fs)?);
    let diagnostics = opened.fs.diagnostics()?;
    drop(external);
    drop(cold);
    drop(warm);
    drop(opened);

    let operation_started = Instant::now();
    let reopened = LayerFs::open(&store)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_without_diagnostics(
        "reopen",
        "store-open",
        operation_wall_ns,
        root_final,
        reopened.head,
    ));
    if reopened.head != root_final {
        return Err("S10 reopened head mismatch".into());
    }
    let historical = reopened
        .fs
        .materialize_external(root_a, &base.join("historical"))?;
    let before_range = reopened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let historical_range = read_range(&historical.path().join("nested/large.bin"), 4096, 4096)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    assert_eq!(
        historical_range,
        (4096..8192).map(deterministic_byte).collect::<Vec<_>>()
    );
    operations.push(operation_receipt(
        "historical-range-read",
        "native-file-read",
        operation_wall_ns,
        root_a,
        root_a,
        before_range,
        reopened.fs.diagnostics()?,
    ));
    let reopened_final = reopened
        .fs
        .materialize_external(root_final, &base.join("reopened-final"))?;
    assert_tree_equal(&final_native_path, reopened_final.path())?;
    assert_eq!(
        fs::read(reopened_final.path().join("made/moved.txt"))?,
        b"shell"
    );
    assert_eq!(
        &fs::read(reopened_final.path().join("nested/large.bin"))?[32768..32772],
        b"MMAP"
    );
    assert_eq!(
        fs::read(reopened_final.path().join("append.txt"))?,
        b"append"
    );
    assert_eq!(
        fs::metadata(reopened_final.path().join("nested/large.bin"))?.ino(),
        fs::metadata(reopened_final.path().join("bash-hardlink"))?.ino()
    );
    let xattr = Command::new("xattr")
        .args(["-p", "com.layerfs.eval"])
        .arg(reopened_final.path().join("nested/large.bin"))
        .output()?;
    if !xattr.status.success() || xattr.stdout != b"exact\n" {
        return Err("S10 xattr oracle mismatch".into());
    }
    assert_apple_metadata(&reopened_final.path().join("nested/scripts/run.sh"))?;
    for (index, root) in [root_s3, root_s4, root_s5, root_s6].into_iter().enumerate() {
        let retained = reopened
            .fs
            .materialize_external(root, &base.join(format!("retained-s{}", index + 3)))?;
        assert_managed_root(retained.path(), index + 3)?;
        drop(retained);
    }
    stages.push(stage_receipt("S10", root_final, &reopened.fs)?);
    let before_fork = reopened.fs.diagnostics()?;
    let operation_started = Instant::now();
    assert_eq!(reopened.fs.fork(root_final, "root-final")?, root_final);
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt(
        "fork",
        "api-fork",
        operation_wall_ns,
        root_final,
        root_final,
        before_fork,
        reopened.fs.diagnostics()?,
    ));
    let before_rollback = reopened.fs.diagnostics()?;
    let operation_started = Instant::now();
    assert_eq!(reopened.fs.rollback(root_a)?, root_a);
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt(
        "rollback",
        "api-rollback",
        operation_wall_ns,
        root_final,
        root_a,
        before_rollback,
        reopened.fs.diagnostics()?,
    ));
    let before_edit = reopened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let mut diverged = reopened.fs.materialize_managed(root_a)?;
    let operation_diagnostics = diverged.replace_observed("nested/managed.bin", 0, 1, b"D")?;
    let (root_diverged, capture_diagnostics) = diverged.capture_observed()?;
    let operation_diagnostics = operation_diagnostics.merge(capture_diagnostics)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt_observed(
        "divergent-edit",
        "managed-replace-capture",
        operation_wall_ns,
        root_a,
        root_diverged,
        before_edit,
        reopened.fs.diagnostics()?,
        operation_diagnostics,
    ));
    if root_diverged == root_a || root_diverged == root_final {
        return Err("S11 divergent branch reused a non-divergent root".into());
    }
    let before_fork = reopened.fs.diagnostics()?;
    let operation_started = Instant::now();
    assert_eq!(
        reopened.fs.fork(root_diverged, "root-diverged")?,
        root_diverged
    );
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt(
        "fork",
        "api-fork",
        operation_wall_ns,
        root_diverged,
        root_diverged,
        before_fork,
        reopened.fs.diagnostics()?,
    ));
    let diverged_oracle = reopened
        .fs
        .materialize_external(root_diverged, &base.join("root-diverged"))?;
    assert_eq!(
        read_range(&diverged_oracle.path().join("nested/managed.bin"), 0, 1)?,
        b"D"
    );
    drop(diverged_oracle);
    let before_rollback = reopened.fs.diagnostics()?;
    let operation_started = Instant::now();
    assert_eq!(reopened.fs.rollback(root_final)?, root_final);
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt(
        "rollback",
        "api-rollback",
        operation_wall_ns,
        root_diverged,
        root_final,
        before_rollback,
        reopened.fs.diagnostics()?,
    ));
    stages.push(stage_receipt("S11", root_final, &reopened.fs)?);
    let reopened_final_path = reopened_final.path().to_owned();
    drop(historical);
    drop(reopened_final);

    let operation_started = Instant::now();
    let compacted = reopened.fs.compact(&store)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_without_diagnostics(
        "compaction",
        "offline-compaction",
        operation_wall_ns,
        root_final,
        compacted.head,
    ));
    if compacted.head != root_final {
        return Err("S12 compacted head mismatch".into());
    }
    let compacted_diagnostics = compacted.fs.diagnostics()?;
    let compaction = compacted_diagnostics
        .compaction
        .ok_or("missing compaction observation")?;
    drop(compacted);

    let operation_started = Instant::now();
    let post_compact = LayerFs::open(&store)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_without_diagnostics(
        "post-compaction-reopen",
        "store-open",
        operation_wall_ns,
        root_final,
        post_compact.head,
    ));
    if post_compact.head != root_final {
        return Err("S12 fresh post-compaction reopen head mismatch".into());
    }
    let retained = post_compact
        .fs
        .materialize_external(root_a, &base.join("post-compact-root-a"))?;
    assert_tree_equal(&source_path, retained.path())?;
    drop(retained);
    let retained_final = post_compact
        .fs
        .materialize_external(root_final, &base.join("post-compact-root-final"))?;
    assert_tree_equal(&reopened_final_path, retained_final.path())?;
    assert_apple_metadata(&retained_final.path().join("nested/scripts/run.sh"))?;
    let retained_xattr = Command::new("xattr")
        .args(["-p", "com.layerfs.eval"])
        .arg(retained_final.path().join("nested/large.bin"))
        .output()?;
    if !retained_xattr.status.success() || retained_xattr.stdout != b"exact\n" {
        return Err("S12 xattr oracle mismatch".into());
    }
    assert_eq!(
        fs::read(retained_final.path().join("made/moved.txt"))?,
        b"shell"
    );
    assert_eq!(
        read_range(&retained_final.path().join("nested/large.bin"), 32768, 4)?,
        b"MMAP"
    );
    drop(retained_final);
    let retained_diverged = post_compact
        .fs
        .materialize_external(root_diverged, &base.join("post-compact-root-diverged"))?;
    assert_eq!(
        read_range(&retained_diverged.path().join("nested/managed.bin"), 0, 1)?,
        b"D"
    );
    drop(retained_diverged);
    for (index, root) in [root_s3, root_s4, root_s5, root_s6].into_iter().enumerate() {
        let retained = post_compact.fs.materialize_external(
            root,
            &base.join(format!("post-compact-retained-s{}", index + 3)),
        )?;
        assert_managed_root(retained.path(), index + 3)?;
        drop(retained);
    }
    stages.push(stage_receipt("S12", root_final, &post_compact.fs)?);
    drop(post_compact);

    if stages.iter().any(|stage| {
        stage.diagnostics.operation_q_current_bytes != 0
            || stage.diagnostics.operation_q_high_water_bytes
                > stage.diagnostics.operation_q_bound_bytes
    }) {
        return Err(
            "operation-owned Q reservation did not return to zero or exceeded its bound".into(),
        );
    }

    let store_bytes = tree_bytes(&store)?;
    let residue = owned_residue(base)?;
    let fd_terminal = fd_count()?;
    if fd_terminal != fd_baseline {
        return Err(format!(
            "file descriptor leak: baseline={fd_baseline}, terminal={fd_terminal}"
        )
        .into());
    }
    Ok(PocReceipt {
        root_a,
        root_final,
        wall: started.elapsed(),
        store_bytes,
        residue,
        page_size: compacted_diagnostics.page_size,
        cache_pages: compacted_diagnostics.cache_pages,
        database_bytes: compacted_diagnostics.database_bytes,
        logical_engine_bytes: compacted_diagnostics.logical_engine_bytes,
        diagnostics,
        compaction,
        fd_baseline,
        fd_terminal,
        stages,
        operations,
        operation_q_bound_bytes: compacted_diagnostics.operation_q_bound_bytes,
    })
}

fn read_range(path: &Path, offset: u64, length: usize) -> Result<Vec<u8>, std::io::Error> {
    let mut file = fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn command_ok(command: &mut Command) -> Result<(), Box<dyn std::error::Error>> {
    if command.status()?.success() {
        Ok(())
    } else {
        Err("native metadata command failed".into())
    }
}

fn assert_one_commit(
    before: Diagnostics,
    after: Diagnostics,
) -> Result<(), Box<dyn std::error::Error>> {
    if after.transactions_started != before.transactions_started + 1
        || after.transactions_committed != before.transactions_committed + 1
    {
        return Err("state-changing capture did not use one transaction/COMMIT".into());
    }
    Ok(())
}

fn stage_receipt(
    label: &'static str,
    root: RootId,
    layerfs: &LayerFs,
) -> Result<StageReceipt, VfsError> {
    Ok(StageReceipt {
        label,
        root,
        diagnostics: layerfs.diagnostics()?,
    })
}

fn assert_apple_metadata(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let metadata = fs::metadata(path)?;
    let finder = Command::new("xattr")
        .args(["-px", "com.apple.FinderInfo"])
        .arg(path)
        .output()?;
    let finder = String::from_utf8_lossy(&finder.stdout)
        .split_whitespace()
        .collect::<String>();
    if metadata.mtime() != -2
        || !String::from_utf8_lossy(&Command::new("ls").arg("-le").arg(path).output()?.stdout)
            .contains("everyone allow read")
        || !String::from_utf8_lossy(
            &Command::new("stat")
                .args(["-f", "%Sf"])
                .arg(path)
                .output()?
                .stdout,
        )
        .contains("hidden")
        || Command::new("xattr")
            .args(["-p", "com.apple.ResourceFork"])
            .arg(path)
            .output()?
            .stdout
            != b"resource-fork\n"
        || finder != "0000000000000000000000000000000000000000000000000000000000000001"
    {
        return Err("Apple metadata oracle mismatch".into());
    }
    Ok(())
}

fn assert_managed_root(root: &Path, stage: usize) -> Result<(), Box<dyn std::error::Error>> {
    let mut expected = vec![0x6d; 1024 * 1024];
    expected[4096..8192].fill(0xa5);
    if stage >= 4 {
        expected.splice(8192..8192, std::iter::repeat_n(0x3c, 8192));
    }
    if stage >= 5 {
        expected.drain(16_384..20_480);
    }
    if stage >= 6 {
        expected.truncate(1_048_576);
    }
    let link = if stage >= 6 {
        "managed-link"
    } else {
        "relative-link"
    };
    if fs::read(root.join("nested/managed.bin"))? != expected
        || fs::read_link(root.join(link))? != Path::new("nested/large.bin")
    {
        return Err(format!("retained S{stage} oracle mismatch").into());
    }
    Ok(())
}

fn deterministic_byte(index: usize) -> u8 {
    (index as u64).wrapping_mul(0x9e37_79b9) as u8
}

fn assert_tree_equal(left: &Path, right: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let left_tree = snapshot_tree(left)?;
    let right_tree = snapshot_tree(right)?;
    if left_tree != right_tree {
        return Err("exact tree metadata/topology oracle mismatch".into());
    }
    for (relative, entry) in left_tree {
        if entry.kind == 2 && !files_equal(&left.join(&relative), &right.join(relative))? {
            return Err("exact tree file-byte oracle mismatch".into());
        }
    }
    Ok(())
}

fn files_equal(left: &Path, right: &Path) -> Result<bool, std::io::Error> {
    let mut left = fs::File::open(left)?;
    let mut right = fs::File::open(right)?;
    let mut left_buffer = vec![0; 1024 * 1024];
    let mut right_buffer = vec![0; 1024 * 1024];
    loop {
        let left_count = left.read(&mut left_buffer)?;
        let right_count = right.read(&mut right_buffer)?;
        if left_count != right_count || left_buffer[..left_count] != right_buffer[..right_count] {
            return Ok(false);
        }
        if left_count == 0 {
            return Ok(true);
        }
    }
}

fn snapshot_tree(root: &Path) -> Result<BTreeMap<PathBuf, TreeEntry>, std::io::Error> {
    fn walk(
        root: &Path,
        relative: &Path,
        links: &mut BTreeMap<(u64, u64), usize>,
        output: &mut BTreeMap<PathBuf, TreeEntry>,
    ) -> Result<(), std::io::Error> {
        let path = root.join(relative);
        let metadata = fs::symlink_metadata(&path)?;
        let file_type = metadata.file_type();
        let (kind, body, hard_link_group) = if file_type.is_dir() {
            (0, Vec::new(), None)
        } else if file_type.is_symlink() {
            (
                1,
                fs::read_link(&path)?.as_os_str().as_bytes().to_vec(),
                None,
            )
        } else {
            let key = (metadata.dev(), metadata.ino());
            let next = links.len();
            let group = *links.entry(key).or_insert(next);
            (2, Vec::new(), Some(group))
        };
        output.insert(
            relative.to_path_buf(),
            TreeEntry {
                kind,
                mode: metadata.mode(),
                modified: (metadata.mtime(), metadata.mtime_nsec()),
                body,
                hard_link_group,
            },
        );
        if file_type.is_dir() {
            let mut children = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
            children.sort_by_key(|entry| entry.file_name());
            for child in children {
                walk(root, &relative.join(child.file_name()), links, output)?;
            }
        }
        Ok(())
    }

    let mut output = BTreeMap::new();
    walk(root, Path::new(""), &mut BTreeMap::new(), &mut output)?;
    Ok(output)
}

fn counter_delta(operation: &OperationReceipt, field: fn(Diagnostics) -> u64) -> String {
    operation
        .diagnostics
        .and_then(|(before, after)| field(after).checked_sub(field(before)))
        .map_or_else(|| "Unavailable".to_owned(), |value| value.to_string())
}

fn signed_delta(operation: &OperationReceipt, field: fn(Diagnostics) -> u64) -> String {
    operation.diagnostics.map_or_else(
        || "Unavailable".to_owned(),
        |(before, after)| (i128::from(field(after)) - i128::from(field(before))).to_string(),
    )
}

fn optional_signed_delta(
    operation: &OperationReceipt,
    field: fn(Diagnostics) -> Option<u64>,
) -> String {
    operation
        .diagnostics
        .and_then(|(before, after)| Some((field(before)?, field(after)?)))
        .map_or_else(
            || "Unavailable".to_owned(),
            |(before, after)| (i128::from(after) - i128::from(before)).to_string(),
        )
}

fn operation_counter(
    operation: &OperationReceipt,
    field: fn(OperationDiagnostics) -> u64,
) -> String {
    operation.operation_diagnostics.map_or_else(
        || "Unavailable".to_owned(),
        |value| field(value).to_string(),
    )
}

fn native_route(operation: &OperationReceipt) -> String {
    operation
        .operation_diagnostics
        .and_then(|value| value.native.route)
        .map_or_else(|| "Unavailable".to_owned(), |route| format!("{route:?}"))
}

pub fn run(directory: &Path) -> Result<(), String> {
    let receipt = run_workflow(directory).map_err(|error| error.to_string())?;
    for operation in &receipt.operations {
        println!(
            "apple-poc-operation status=PASS operation={} api_route={} native_route={} wall_ns={} root_before={} root_after={} transactions_delta={} commits_delta={} rollbacks_delta={} statements_delta={} busy_delta={} locked_delta={} objects_validated_delta={} objects_created_delta={} objects_reused_delta={} object_bytes_read_delta={} object_bytes_written_delta={} range_bytes_requested_delta={} range_bytes_returned_delta={} logical_object_bytes_delta={} logical_root_bytes_delta={} logical_delta_bytes_delta={} retained_union_scrubs_delta={} root_verifications_delta={} root_verification_objects_delta={} root_verification_bytes_delta={} database_bytes_delta={} rollback_journal_bytes_delta={} temporary_file_bytes_delta={} logical_engine_bytes_delta={} active_connections_delta={} q_current_delta={} q_high_water_delta={} payload_bytes_read={} payload_bytes_written={} cdc_bytes_scanned={} chunks_created={} rope_nodes_read={} rope_nodes_created={} namespace_nodes_read={} namespace_nodes_created={} inode_nodes_read={} inode_nodes_created={} clone_attempts={} clone_successes={} clone_fallbacks={} native_bytes_read={} native_bytes_written={} native_patch_bytes={} native_suffix_bytes_shifted={}",
            operation.operation,
            operation.api_route,
            native_route(operation),
            operation.wall_ns,
            operation.root_before,
            operation.root_after,
            counter_delta(operation, |value| value.transactions_started),
            counter_delta(operation, |value| value.transactions_committed),
            counter_delta(operation, |value| value.transactions_rolled_back),
            counter_delta(operation, |value| value.statements),
            counter_delta(operation, |value| value.busy_events),
            counter_delta(operation, |value| value.locked_events),
            counter_delta(operation, |value| value.objects_validated),
            counter_delta(operation, |value| value.objects_created),
            counter_delta(operation, |value| value.objects_reused),
            counter_delta(operation, |value| value.object_bytes_read),
            counter_delta(operation, |value| value.object_bytes_written),
            counter_delta(operation, |value| value.range_bytes_requested),
            counter_delta(operation, |value| value.range_bytes_returned),
            counter_delta(operation, |value| value.logical_object_bytes),
            counter_delta(operation, |value| value.logical_root_bytes),
            counter_delta(operation, |value| value.logical_delta_bytes),
            counter_delta(operation, |value| value.retained_union_scrubs),
            counter_delta(operation, |value| value.root_verifications),
            counter_delta(operation, |value| value.root_verification_objects),
            counter_delta(operation, |value| value.root_verification_bytes),
            optional_signed_delta(operation, |value| value.database_bytes),
            optional_signed_delta(operation, |value| value.rollback_journal_bytes),
            optional_signed_delta(operation, |value| value.temporary_file_bytes),
            optional_signed_delta(operation, |value| value.logical_engine_bytes),
            signed_delta(operation, |value| value.active_connections),
            signed_delta(operation, |value| value.operation_q_current_bytes),
            signed_delta(operation, |value| value.operation_q_high_water_bytes),
            operation_counter(operation, |value| value.rope.payload_bytes_read),
            operation_counter(operation, |value| value.rope.payload_bytes_written),
            operation_counter(operation, |value| value.rope.cdc_bytes_scanned),
            operation_counter(operation, |value| value.rope.chunks_created),
            operation_counter(operation, |value| value.rope.nodes_read),
            operation_counter(operation, |value| value.rope.nodes_created),
            operation_counter(operation, |value| value.namespace.nodes_read),
            operation_counter(operation, |value| value.namespace.nodes_created),
            operation_counter(operation, |value| value.inode_table.nodes_read),
            operation_counter(operation, |value| value.inode_table.nodes_created),
            operation_counter(operation, |value| value.native.clone_attempts),
            operation_counter(operation, |value| value.native.clone_successes),
            operation_counter(operation, |value| value.native.clone_fallbacks),
            operation_counter(operation, |value| value.native.bytes_read),
            operation_counter(operation, |value| value.native.bytes_written),
            operation_counter(operation, |value| value.native.patch_bytes),
            operation_counter(operation, |value| value.native.suffix_bytes_shifted),
        );
    }
    for stage in &receipt.stages {
        println!(
            "apple-poc-stage stage={} root={} transactions={} commits={} objects_created={} objects_reused={} connections={} q_current={} q_high_water={}",
            stage.label,
            stage.root,
            stage.diagnostics.transactions_started,
            stage.diagnostics.transactions_committed,
            stage.diagnostics.objects_created,
            stage.diagnostics.objects_reused,
            stage.diagnostics.active_connections,
            stage.diagnostics.operation_q_current_bytes,
            stage.diagnostics.operation_q_high_water_bytes,
        );
    }
    println!(
        "apple-poc stages=S0-S12 root_a={} root_final={} wall_ms={} store_bytes={} page_size={} cache_pages={} database_bytes={:?} logical_bytes={:?} objects_created={} objects_reused={} compact_old={} compact_new={} compact_mark={} compact_aux_peak={} compact_verify_scratch={} compact_selector={} compact_total_peak={} q_structural_bound={} q_current={} q_high_water={} fd_baseline={} fd_terminal={} residue={}",
        receipt.root_a,
        receipt.root_final,
        receipt.wall.as_millis(),
        receipt.store_bytes,
        receipt.page_size,
        receipt.cache_pages,
        receipt.database_bytes,
        receipt.logical_engine_bytes,
        receipt.diagnostics.objects_created,
        receipt.diagnostics.objects_reused,
        receipt.compaction.old_generation_bytes,
        receipt.compaction.new_generation_bytes,
        receipt.compaction.mark_database_bytes,
        receipt.compaction.candidate_journal_temp_peak_bytes,
        receipt.compaction.verification_scratch_peak_bytes,
        receipt.compaction.selector_temporary_bytes,
        receipt.compaction.total_peak_bytes,
        receipt.operation_q_bound_bytes,
        receipt.diagnostics.operation_q_current_bytes,
        receipt.diagnostics.operation_q_high_water_bytes,
        receipt.fd_baseline,
        receipt.fd_terminal,
        receipt.residue.len()
    );
    if !receipt.residue.is_empty() {
        return Err(format!("owned residue: {:?}", receipt.residue));
    }
    fs::remove_dir_all(directory).map_err(|error| error.to_string())?;
    Ok(())
}

fn fd_count() -> Result<usize, std::io::Error> {
    Ok(fs::read_dir("/dev/fd")?
        .collect::<Result<Vec<_>, _>>()?
        .len())
}

fn owned_residue(root: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    fn walk(root: &Path, output: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(".layerfs-")
                || name == "CURRENT.tmp"
                || name.ends_with("-journal")
                || name.ends_with("-wal")
                || name.ends_with("-shm")
            {
                output.push(path.clone());
            }
            if entry.file_type()?.is_dir() {
                walk(&path, output)?;
            }
        }
        Ok(())
    }
    let mut output = Vec::new();
    walk(root, &mut output)?;
    Ok(output)
}

fn tree_bytes(root: &Path) -> Result<u64, std::io::Error> {
    let mut total = 0_u64;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        total = total.saturating_add(if metadata.is_dir() {
            tree_bytes(&entry.path())?
        } else {
            metadata.len()
        });
    }
    Ok(total)
}
