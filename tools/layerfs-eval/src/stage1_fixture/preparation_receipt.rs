use super::contract::EvalResult;
use super::error::{display_error, io_error};
use super::preparation::PreparationProgress;
use super::tree::sync_directory;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static PREPARATION_FAILURE_SERIAL: AtomicU64 = AtomicU64::new(0);

pub(super) fn write_preparation_failure(
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

#[cfg(test)]
mod tests {
    use super::{write_preparation_failure, PreparationProgress};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn receipts_are_append_only_and_context_bound() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-eval-failure-receipts-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
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
}
