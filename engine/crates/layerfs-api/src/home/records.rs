//! Record files in the home: one JSON value each, private to this user
//! (0600 files, 0700 folders), replaced whole by a rename so a reader sees
//! the old record or the new one, never a part.
use crate::calls::json;
use crate::host::outcome::{failed, refused};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;

/// `path` as a private folder, with its parents.
pub fn private_dir(path: &Path) -> Result<(), SdkRefusal> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|error| refused("Failed", format!("{}: {error}", path.display())))
}

/// The record at `path`, or `None` when there is no file.
pub fn read_record(path: &Path) -> Result<Option<SdkValue>, SdkRefusal> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(refused("Failed", format!("{}: {error}", path.display()))),
    };
    json::parse(&text)
        .map(Some)
        .map_err(|error| refused("Failed", format!("{}: {}", path.display(), error.0)))
}

/// Writes `value` as the record at `path`: a private temporary file in the
/// same folder, then one rename.
pub fn write_record(path: &Path, value: &SdkValue) -> Result<(), SdkRefusal> {
    let parent = path
        .parent()
        .ok_or_else(|| refused("Failed", "a record needs a folder"))?;
    private_dir(parent)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("record");
    let temporary = parent.join(format!(".{name}.{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(failed)?;
    let mut text = json::print(value);
    text.push('\n');
    let written = file.write_all(text.as_bytes()).map_err(failed);
    drop(file);
    match written.and_then(|()| fs::rename(&temporary, path).map_err(failed)) {
        Ok(()) => Ok(()),
        Err(refusal) => {
            let _ = fs::remove_file(&temporary);
            Err(refusal)
        }
    }
}
