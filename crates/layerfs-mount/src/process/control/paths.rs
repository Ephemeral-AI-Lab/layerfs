use super::super::path_validation::{canonical_target, same_existing_file, ValidatedPaths};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(in crate::process) struct ControlPaths {
    pub(super) request: PathBuf,
    pub(super) receipt: PathBuf,
}
pub(in crate::process) fn control_paths(
    arguments: &HashMap<String, String>,
    paths: &ValidatedPaths,
) -> Result<Option<ControlPaths>, Box<dyn std::error::Error>> {
    let (request, receipt) = match (
        arguments.get("control-request"),
        arguments.get("control-receipt"),
    ) {
        (None, None) => return Ok(None),
        (Some(request), Some(receipt)) => (request, receipt),
        _ => {
            return Err("--control-request and --control-receipt must be supplied together".into())
        }
    };
    let request = canonical_target(Path::new(request))?;
    let receipt = canonical_target(Path::new(receipt))?;
    if !request.is_file() {
        return Err(format!(
            "control request must be an existing file: {}",
            request.display()
        )
        .into());
    }
    if receipt.exists() {
        return Err(format!("control receipt already exists: {}", receipt.display()).into());
    }
    let existing = [&paths.store, &paths.spool, &paths.receipt];
    for path in [&request, &receipt] {
        if path.starts_with(&paths.mount) {
            return Err(format!("control path must be outside mount: {}", path.display()).into());
        }
        for other in existing.iter().copied() {
            if path == other || same_existing_file(path, other)? {
                return Err(format!("control path must be distinct: {}", path.display()).into());
            }
        }
    }
    if request == receipt || same_existing_file(&request, &receipt)? {
        return Err("control request and receipt must be distinct".into());
    }
    Ok(Some(ControlPaths { request, receipt }))
}
