//! Record files in the home: one JSON value each, private to this user
//! (0600 files, 0700 folders), replaced whole by a rename so a reader sees
//! the old record or the new one, never a part. Bytes are written as
//! `{"hex": "..."}` and read back as bytes, so a record keeps its types.
use crate::common::json;
use crate::common::outcome::{failed, refused};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// `path` as a private folder, with its parents.
pub fn private_dir(path: &Path) -> Result<(), SdkRefusal> {
    #[cfg(unix)]
    let made = {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().recursive(true).mode(0o700).create(path)
    };
    // Windows: privacy is the account's DACL, not mode bits.
    #[cfg(not(unix))]
    let made = fs::DirBuilder::new().recursive(true).create(path);
    made.map_err(|error| refused("Failed", format!("{}: {error}", path.display())))
}

/// Lets others pass through (o+x, never read) `dir` and each folder from
/// `home` down to it: a namespaces Sandbox's commands run as another
/// identity and must reach their Workspace below `dir`. The folders stay
/// unlistable by others; folders above `home` are not changed.
#[cfg(unix)]
pub fn traversable(home: &Path, dir: &Path) -> Result<(), SdkRefusal> {
    use std::os::unix::fs::PermissionsExt;
    for folder in dir.ancestors().take_while(|f| f.starts_with(home)) {
        let mode = fs::metadata(folder).map_err(failed)?.permissions().mode();
        if mode & 0o001 == 0 {
            fs::set_permissions(folder, fs::Permissions::from_mode((mode | 0o001) & 0o7777))
                .map_err(failed)?;
        }
    }
    Ok(())
}

/// Windows: folder traversal is governed by the account's ACLs; the
/// namespaces identity traversal this mirrors is a unix concept.
#[cfg(not(unix))]
pub fn traversable(_home: &Path, _dir: &Path) -> Result<(), SdkRefusal> {
    Ok(())
}

/// The record at `path`, or `None` when there is no file.
pub fn read_record(path: &Path) -> Result<Option<SdkValue>, SdkRefusal> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(refused("Failed", format!("{}: {error}", path.display()))),
    };
    json::parse(&text)
        .map(|value| Some(from_hex(value)))
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
    // one name per write: threads of one process never share a temporary
    let unique = NEXT.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(".{name}.{}.{unique}.tmp", std::process::id()));
    #[cfg(unix)]
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(failed)?;
    #[cfg(not(unix))]
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temporary)
        .map_err(failed)?;
    let mut text = json::print(&to_hex(value.clone()));
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

fn to_hex(value: SdkValue) -> SdkValue {
    match value {
        SdkValue::Bytes(bytes) => SdkValue::map([(
            "hex",
            SdkValue::Text(layerfs_bridge::sdk_values::hex(&bytes)),
        )]),
        SdkValue::List(items) => SdkValue::List(items.into_iter().map(to_hex).collect()),
        SdkValue::Map(fields) => {
            SdkValue::Map(fields.into_iter().map(|(n, v)| (n, to_hex(v))).collect())
        }
        other => other,
    }
}

fn from_hex(value: SdkValue) -> SdkValue {
    match value {
        SdkValue::Map(fields) => {
            if let [(name, SdkValue::Text(text))] = fields.as_slice() {
                if name == "hex" {
                    if let Some(bytes) = unhex(text) {
                        return SdkValue::Bytes(bytes);
                    }
                }
            }
            SdkValue::Map(fields.into_iter().map(|(n, v)| (n, from_hex(v))).collect())
        }
        SdkValue::List(items) => SdkValue::List(items.into_iter().map(from_hex).collect()),
        other => other,
    }
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

/// The version every entity record carries, `"v": 1` (layout 3, #536 §5).
pub const RECORD_VERSION: i64 = 1;

/// Writes entity record `value` at `path` ([`write_record`]) with its
/// version first, `"v": 1`.
pub fn write_entity(path: &Path, value: &SdkValue) -> Result<(), SdkRefusal> {
    write_record(path, &versioned(value))
}

/// `value` (a record) with `"v": 1` as its first field.
pub fn versioned(value: &SdkValue) -> SdkValue {
    let mut fields = vec![("v".to_owned(), SdkValue::Int(RECORD_VERSION))];
    if let SdkValue::Map(old) = value {
        fields.extend(old.iter().filter(|(n, _)| n != "v").cloned());
    }
    SdkValue::Map(fields)
}

/// `value` (a record) with field `name` set to `field`, added when absent.
pub fn with_field(value: SdkValue, name: &str, field: SdkValue) -> SdkValue {
    let mut fields = match value {
        SdkValue::Map(fields) => fields,
        _ => Vec::new(),
    };
    match fields.iter_mut().find(|(n, _)| n == name) {
        Some((_, slot)) => *slot = field,
        None => fields.push((name.to_owned(), field)),
    }
    SdkValue::Map(fields)
}

/// Appends one JSON line to `log/<name>`.
pub fn log_line(home: &Path, name: &str, value: &SdkValue) -> Result<(), SdkRefusal> {
    let dir = super::paths::log_dir(home);
    private_dir(&dir)?;
    #[cfg(unix)]
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(dir.join(name))
        .map_err(failed)?;
    #[cfg(not(unix))]
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(dir.join(name))
        .map_err(failed)?;
    let mut text = json::print(&to_hex(value.clone()));
    text.push('\n');
    file.write_all(text.as_bytes()).map_err(failed)
}

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}
