use super::schedule_model::EditSpec;
use crate::stage1_fixture::EvalResult;
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ContentCounters {
    pub(crate) cdc_bytes_scanned: u64,
    pub(crate) payload_bytes_written: u64,
    pub(crate) unaffected_payload_reads: u64,
    pub(crate) unaffected_payload_writes: u64,
    pub(crate) rope_nodes_read: u64,
    pub(crate) rope_nodes_emitted: u64,
    pub(crate) content_directory_nodes_emitted: u64,
}
pub(crate) fn content_counters(
    operation: &crate::legacy_full::OperationDiagnostics,
) -> EvalResult<ContentCounters> {
    let payload_bytes_read = operation
        .content_payload_bytes_read()
        .ok_or_else(|| "metadata payload reads exceed aggregate reads".to_owned())?;
    let payload_bytes_written = operation
        .content_payload_bytes_written()
        .ok_or_else(|| "metadata payload writes exceed aggregate writes".to_owned())?;
    let cdc_bytes_scanned = operation
        .rope
        .cdc_bytes_scanned
        .checked_sub(operation.metadata_rope.cdc_bytes_scanned)
        .ok_or_else(|| "metadata CDC exceeds aggregate CDC".to_owned())?;
    Ok(ContentCounters {
        cdc_bytes_scanned,
        payload_bytes_written,
        unaffected_payload_reads: payload_bytes_read,
        unaffected_payload_writes: payload_bytes_written
            .checked_sub(cdc_bytes_scanned)
            .ok_or_else(|| "content payload writes are below content CDC input".to_owned())?,
        rope_nodes_read: operation
            .rope
            .nodes_read
            .checked_sub(operation.metadata_rope.nodes_read)
            .ok_or_else(|| "metadata rope reads exceed aggregate reads".to_owned())?,
        rope_nodes_emitted: operation
            .rope
            .nodes_created
            .checked_sub(operation.metadata_rope.nodes_created)
            .ok_or_else(|| "metadata rope emissions exceed aggregate emissions".to_owned())?,
        content_directory_nodes_emitted: operation.namespace.nodes_created,
    })
}
pub(crate) fn verify_locality(
    operation: &crate::legacy_full::OperationDiagnostics,
    replacement_bytes: u64,
    tree_level: u8,
) -> EvalResult<ContentCounters> {
    let counters = content_counters(operation)?;
    let read_bound = 16_u64
        .checked_mul(u64::from(tree_level) + 1)
        .ok_or_else(|| "rope read bound overflow".to_owned())?;
    let emitted_bound = read_bound
        .checked_add(replacement_bytes.div_ceil(8_192))
        .and_then(|value| value.checked_add(2))
        .ok_or_else(|| "rope emission bound overflow".to_owned())?;
    if counters.cdc_bytes_scanned != replacement_bytes
        || counters.payload_bytes_written != replacement_bytes
        || counters.unaffected_payload_reads != 0
        || counters.unaffected_payload_writes != 0
        || counters.content_directory_nodes_emitted != 0
        || counters.rope_nodes_read > read_bound
        || counters.rope_nodes_emitted > emitted_bound
    {
        return Err(format!(
            "locality equation failed: replacement={replacement_bytes} H={tree_level} counters={counters:?} read_bound={read_bound} emitted_bound={emitted_bound}"
        ));
    }
    Ok(counters)
}
pub(crate) fn verify_burst_locality(
    operation: &crate::legacy_full::OperationDiagnostics,
    edits: &[EditSpec],
    steps: &[crate::legacy_full::ManagedReplayStep],
) -> EvalResult<ContentCounters> {
    let replacement_bytes = edits.iter().try_fold(0_u64, |total, edit| {
        total
            .checked_add(edit.insert_bytes)
            .ok_or_else(|| "burst replacement bytes overflow".to_owned())
    })?;
    let counters = content_counters(operation)?;
    if steps.len() != edits.len() {
        return Err("burst aggregate step count".to_owned());
    }
    let mut exact = ContentCounters::default();
    for (edit, step) in edits.iter().zip(steps) {
        let tree_level = step
            .tree_level_before
            .ok_or_else(|| format!("{} missing actual H", edit.tag))?;
        let one = verify_locality(&step.counters, edit.insert_bytes, tree_level)?;
        exact.cdc_bytes_scanned = exact
            .cdc_bytes_scanned
            .checked_add(one.cdc_bytes_scanned)
            .ok_or_else(|| "burst exact CDC sum overflow".to_owned())?;
        exact.payload_bytes_written = exact
            .payload_bytes_written
            .checked_add(one.payload_bytes_written)
            .ok_or_else(|| "burst exact payload sum overflow".to_owned())?;
        exact.unaffected_payload_reads = exact
            .unaffected_payload_reads
            .checked_add(one.unaffected_payload_reads)
            .ok_or_else(|| "burst exact unaffected-read sum overflow".to_owned())?;
        exact.unaffected_payload_writes = exact
            .unaffected_payload_writes
            .checked_add(one.unaffected_payload_writes)
            .ok_or_else(|| "burst exact unaffected-write sum overflow".to_owned())?;
        exact.rope_nodes_read = exact
            .rope_nodes_read
            .checked_add(one.rope_nodes_read)
            .ok_or_else(|| "burst exact node-read sum overflow".to_owned())?;
        exact.rope_nodes_emitted = exact
            .rope_nodes_emitted
            .checked_add(one.rope_nodes_emitted)
            .ok_or_else(|| "burst exact node-emission sum overflow".to_owned())?;
        exact.content_directory_nodes_emitted = exact
            .content_directory_nodes_emitted
            .checked_add(one.content_directory_nodes_emitted)
            .ok_or_else(|| "burst exact directory-node sum overflow".to_owned())?;
    }
    if counters.cdc_bytes_scanned != replacement_bytes
        || counters.payload_bytes_written != replacement_bytes
        || counters.unaffected_payload_reads != 0
        || counters.unaffected_payload_writes != 0
        || counters.content_directory_nodes_emitted != 0
        || counters.cdc_bytes_scanned != exact.cdc_bytes_scanned
        || counters.payload_bytes_written != exact.payload_bytes_written
        || counters.unaffected_payload_reads != exact.unaffected_payload_reads
        || counters.unaffected_payload_writes != exact.unaffected_payload_writes
        || counters.rope_nodes_read != exact.rope_nodes_read
        || counters.rope_nodes_emitted != exact.rope_nodes_emitted
        || counters.content_directory_nodes_emitted != exact.content_directory_nodes_emitted
    {
        return Err(format!(
            "burst locality exact aggregate failed: replacement={replacement_bytes} edits={} aggregate={counters:?} exact_steps={exact:?}",
            edits.len(),
        ));
    }
    Ok(counters)
}
