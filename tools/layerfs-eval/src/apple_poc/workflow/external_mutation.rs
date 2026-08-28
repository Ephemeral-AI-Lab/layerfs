use super::super::receipt::{
    assert_one_commit, operation_receipt, operation_receipt_observed, stage_receipt,
    OperationReceipt, StageReceipt,
};
use crate::legacy_full::{
    Diagnostics, ExternalWorkspace, LayerFs, OpenedLayerFs, RootId, VfsError,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

#[allow(clippy::too_many_arguments)]
pub(super) fn external_mutation(
    base: &Path,
    store: &Path,
    opened: &OpenedLayerFs,
    cold: ExternalWorkspace,
    warm: ExternalWorkspace,
    root_s6: RootId,
    stages: &mut Vec<StageReceipt>,
    operations: &mut Vec<OperationReceipt>,
) -> Result<(RootId, Diagnostics, PathBuf), Box<dyn std::error::Error>> {
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
    if nonzero.code() != Some(7) || LayerFs::open(store)?.head != root_s6 {
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

    Ok((root_final, diagnostics, final_native_path))
}
