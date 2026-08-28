use super::super::encoding::json;
use super::request::SpliceRequest;
use layerfs_mount::workspace::{MountedLifecycle, MountedSpliceReceipt};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub(super) fn write_control_success(
    path: &Path,
    request: &SpliceRequest,
    receipt: &MountedSpliceReceipt,
) -> Result<(), String> {
    let counters = &receipt.counters;
    let body = format!(
        concat!(
            "{{\n",
            "  \"schema\": \"layerfs-mount-splice-v1\",\n",
            "  \"status\": \"PASS\",\n",
            "  \"path\": \"{}\",\n",
            "  \"start\": {},\n",
            "  \"delete_bytes\": {},\n",
            "  \"insert_bytes\": {},\n",
            "  \"before\": {{\"generation\":{},\"root\":\"{}\"}},\n",
            "  \"after\": {{\"generation\":{},\"root\":\"{}\"}},\n",
            "  \"remount_required\": {},\n",
            "  \"locality\": {{\"cdc_bytes_scanned\":{},\"content_payload_bytes_read\":{},\"content_payload_bytes_written\":{},\"rope_nodes_created\":{},\"namespace_nodes_created\":{},\"inode_nodes_created\":{}}},\n",
            "  \"operation_q\": {{\"terminal_bytes\":{},\"high_water_bytes\":{}}}\n",
            "}}\n"
        ),
        json(&request.path_text),
        request.start,
        request.delete_len,
        request.replacement.len(),
        receipt.generation,
        receipt.before,
        receipt.generation,
        receipt.after,
        receipt.remount_required,
        counters.rope.cdc_bytes_scanned,
        counters.rope.payload_bytes_read,
        counters.rope.payload_bytes_written,
        counters.rope.nodes_created,
        counters.namespace.nodes_created,
        counters.inode_table.nodes_created,
        receipt.operation_q_terminal_bytes,
        receipt.operation_q_high_water_bytes,
    );
    write_new(path, &body)
}

pub(super) fn write_control_failure(
    path: &Path,
    lifecycle: MountedLifecycle,
    error: &str,
) -> Result<(), String> {
    let body = format!(
        concat!(
            "{{\n",
            "  \"schema\": \"layerfs-mount-splice-v1\",\n",
            "  \"status\": \"FAIL\",\n",
            "  \"lifecycle\": \"{:?}\",\n",
            "  \"error\": \"{}\",\n",
            "  \"remount_required\": true\n",
            "}}\n"
        ),
        lifecycle,
        json(error),
    );
    write_new(path, &body)
}

fn write_new(path: &Path, body: &str) -> Result<(), String> {
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    output
        .write_all(body.as_bytes())
        .and_then(|()| output.sync_all())
        .map_err(|error| error.to_string())
}
