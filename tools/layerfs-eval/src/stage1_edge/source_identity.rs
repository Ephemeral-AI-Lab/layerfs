use super::artifact::{
    command_bytes, command_output, display_error, io_error, option_u8_json, option_usize_json,
    sha256_bytes, sha256_file,
};
use super::fixture::SourceIdentity;
use super::schedule_model::{replacement_bytes, FrozenSchedule};
use crate::stage1_fixture::{self, EvalResult};
use std::fs;
pub(crate) fn rust_cargo_source_paths() -> EvalResult<Vec<String>> {
    let listed = command_bytes(
        "git",
        &[
            "ls-files",
            "-co",
            "--exclude-standard",
            "-z",
            "--",
            "*.rs",
            "Cargo.toml",
            ":(glob)**/Cargo.toml",
            "Cargo.lock",
        ],
    )?;
    let mut paths = listed
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8(path.to_vec()).map_err(display_error))
        .collect::<EvalResult<Vec<_>>>()?;
    paths.sort();
    paths.dedup();
    Ok(paths)
}
pub(crate) fn source_identity() -> EvalResult<SourceIdentity> {
    let root = stage1_fixture::workspace_root();
    let git_commit = command_output("git", &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    let dirty_tree = !command_output("git", &["status", "--porcelain"])?
        .trim()
        .is_empty();
    let paths = rust_cargo_source_paths()?;
    let mut tree = blake3::Hasher::new();
    let mut manifest = String::new();
    for path in paths {
        let bytes = fs::read(root.join(&path)).map_err(io_error)?;
        tree.update(path.as_bytes());
        tree.update(&[0]);
        tree.update(&bytes);
        manifest.push_str(&sha256_bytes(&bytes)?);
        manifest.push_str("  ");
        manifest.push_str(&path);
        manifest.push('\n');
    }
    let executable_path = std::env::current_exe().map_err(io_error)?;
    Ok(SourceIdentity {
        git_commit,
        dirty_tree,
        tree_blake3: tree.finalize().to_hex().to_string(),
        manifest_sha256: sha256_bytes(manifest.as_bytes())?,
        executable_sha256: sha256_file(&executable_path)?,
        executable_blake3: stage1_fixture::hash_file(&executable_path)?,
        executable_path,
    })
}
pub(crate) fn schedule_json(schedule: &FrozenSchedule) -> EvalResult<String> {
    let edits = schedule
        .edits
        .iter()
        .map(|edit| {
            let replacement = replacement_bytes(
                edit.serial,
                usize::try_from(edit.insert_bytes).expect("frozen insert length fits usize"),
            );
            format!(
                concat!(
                    "{{\"tag\":\"{}\",\"serial\":{},\"epoch\":{},",
                    "\"kind\":\"{}\",\"size_band\":\"{}\",\"offset\":{},",
                    "\"delete_bytes\":{},\"insert_bytes\":{},\"before_bytes\":{},",
                    "\"after_bytes\":{},\"replacement_offset\":{},",
                    "\"replacement_digest\":\"{}\"}}"
                ),
                edit.tag,
                edit.serial,
                edit.epoch,
                edit.kind.as_str(),
                edit.size_band,
                edit.offset,
                edit.delete_bytes,
                edit.insert_bytes,
                edit.before_bytes,
                edit.after_bytes,
                edit.replacement_offset,
                blake3::hash(&replacement).to_hex(),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let rows = schedule
        .rows
        .iter()
        .map(|row| {
            let pre_ref_slot = row.transition_root.map(|root| format!("R{}", root - 1));
            let post_ref_slot = row.transition_root.map(|root| format!("R{root}"));
            format!(
                concat!(
                    "{{\"row_index\":{},\"row_id\":\"{}\",\"row_group\":\"{}\",",
                    "\"sequence\":{},\"epoch\":{},\"direction\":\"{}\",",
                    "\"operation\":\"{}\",\"size_band\":\"{}\",",
                    "\"edit_index\":{},\"burst_index\":{},\"history_session\":{},",
                    "\"milestone_root\":{},\"transition_root\":{},",
                    "\"pre_ref_slot\":{},\"post_ref_slot\":{}}}"
                ),
                row.row_index,
                row.row_id,
                row.row_group,
                row.sequence,
                row.epoch,
                row.direction,
                row.operation,
                row.size_band,
                option_usize_json(row.edit_index),
                option_usize_json(row.burst_index),
                option_u8_json(row.history_session),
                option_u8_json(row.milestone_root),
                option_u8_json(row.transition_root),
                pre_ref_slot.map_or_else(|| "null".to_owned(), |value| format!("\"{value}\"")),
                post_ref_slot.map_or_else(|| "null".to_owned(), |value| format!("\"{value}\"")),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1.1-schedule-v1\",",
            "\"row_count\":47,\"edit_suboperation_count\":51,",
            "\"transition_count\":34,\"snapshot_count\":35,",
            "\"replacement_backing_bytes\":{},",
            "\"replacement_generator\":\"tag_serial*17+index*31 modulo 256\",",
            "\"initial_generator\":\"stage1_fixture::fill_retained_buffer\",",
            "\"row_order\":\"execution-order-with-history-after-each-five-edit-epoch\",",
            "\"edits\":[{}],\"rows\":[{}]}}\n"
        ),
        schedule.replacement_backing.len(),
        edits,
        rows,
    ))
}
