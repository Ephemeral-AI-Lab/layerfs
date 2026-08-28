use super::artifact::{display_error, io_error};
use super::counter_validation::verify_operation_resources;
use super::model::{DigestSink, EditCase};
use crate::legacy_full::{LayerFs, OperationDiagnostics, RefState, RootId};
use crate::stage1_fixture::{
    edit_bytes, expected_bytes, stream_expected, BaseManifest, EvalResult, FILE_BYTES, FILE_PATH,
};
use std::io::Write;
pub(crate) fn expected_ref(base: &BaseManifest) -> RefState {
    RefState {
        name: "main".to_owned(),
        generation: base.generation,
        root: base.root,
    }
}
pub(crate) fn merge_counters(
    left: OperationDiagnostics,
    right: OperationDiagnostics,
) -> EvalResult<OperationDiagnostics> {
    let merged = left.merge(right).map_err(display_error)?;
    verify_operation_resources(&merged)?;
    Ok(merged)
}
pub(crate) fn canonical_digest(
    fs: &LayerFs,
    root: RootId,
) -> EvalResult<(u64, String, OperationDiagnostics)> {
    let mut sink = DigestSink::default();
    let counters = fs
        .read_to(root, FILE_PATH, &mut sink)
        .map_err(display_error)?;
    let (bytes, digest) = sink.finish();
    Ok((bytes, digest, counters))
}
pub(crate) fn edit_result_len(case: &EditCase) -> EvalResult<u64> {
    case.base_len
        .checked_sub(case.delete_len)
        .and_then(|value| value.checked_add(case.replacement.len() as u64))
        .filter(|value| *value <= FILE_BYTES)
        .ok_or_else(|| format!("{} result violates the 100 MiB ceiling", case.id))
}
pub(crate) fn splice_digest(case: &EditCase) -> EvalResult<String> {
    let suffix = case
        .base_len
        .checked_sub(case.start)
        .and_then(|value| value.checked_sub(case.delete_len))
        .ok_or_else(|| format!("{} splice is outside its base", case.id))?;
    let mut sink = DigestSink::default();
    stream_expected(0, case.start, &mut sink)?;
    sink.write_all(&case.replacement).map_err(io_error)?;
    stream_expected(case.start + case.delete_len, suffix, &mut sink)?;
    let (bytes, digest) = sink.finish();
    if bytes != edit_result_len(case)? {
        return Err(format!("{} oracle length mismatch", case.id));
    }
    Ok(digest)
}
pub(crate) fn verify_old_root_range(fs: &LayerFs, root: RootId, case: &EditCase) -> EvalResult<()> {
    let available = case
        .base_len
        .checked_sub(case.start)
        .ok_or_else(|| format!("{} old-root range is outside its base", case.id))?;
    let length = available.min(4_096) as usize;
    if length == 0 {
        return Ok(());
    }
    let mut actual = Vec::new();
    fs.read_range(
        root,
        FILE_PATH,
        case.start..case.start + length as u64,
        &mut actual,
    )
    .map_err(display_error)?;
    if actual != expected_bytes(case.start, length)? {
        return Err(format!("{} old root changed", case.id));
    }
    Ok(())
}
pub(crate) fn history_edit(tag: u8, start: u64) -> EditCase {
    EditCase {
        id: "A14",
        base: "history",
        base_len: FILE_BYTES,
        start,
        delete_len: 4_096,
        replacement: edit_bytes(0x60 + tag, 4_096),
    }
}
pub(crate) fn history_expected_range(
    revision: usize,
    start: u64,
    length: usize,
    edits: &[EditCase],
) -> EvalResult<Vec<u8>> {
    let mut output = expected_bytes(start, length)?;
    let end = start + length as u64;
    for edit in edits.iter().take(revision) {
        let edit_end = edit.start + edit.replacement.len() as u64;
        let overlap_start = start.max(edit.start);
        let overlap_end = end.min(edit_end);
        if overlap_start < overlap_end {
            let output_start = (overlap_start - start) as usize;
            let replacement_start = (overlap_start - edit.start) as usize;
            let overlap = (overlap_end - overlap_start) as usize;
            output[output_start..output_start + overlap]
                .copy_from_slice(&edit.replacement[replacement_start..replacement_start + overlap]);
        }
    }
    Ok(output)
}
