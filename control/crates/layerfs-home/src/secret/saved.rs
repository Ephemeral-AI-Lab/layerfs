//! The home's secret folder, `secret/` (call surface 2.0, `pg.html`): one
//! private file per named secret, `secret/<kind>/<name>.json` (0600 files
//! in 0700 folders), so a record names a secret and never holds it. A
//! PostgreSQL login is kind `auth`; a bucket's key is kind `bucket`. Saving
//! again under a name replaces the secret whole, by one rename. Nothing
//! here logs or shows a value; a secret's value has no `Debug`.
use crate::names::name;
use crate::record::generation;
use crate::reply::outcome::{failed, refused};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The secret folder of `home`.
pub fn dir(home: &Path) -> PathBuf {
    home.join("secret")
}

/// The file of secret `name` of kind `kind`; either name not a valid
/// name is `InvalidInput`.
fn file(home: &Path, kind: &str, secret: &str) -> Result<PathBuf> {
    for (what, text) in [("secret kind", kind), (kind, secret)] {
        name(what, text).map_err(|why| refused("InvalidInput", why))?;
    }
    Ok(dir(home).join(kind).join(format!("{secret}.json")))
}

/// Secret `name` of kind `kind`, as it was saved, or `None` when there is
/// none.
pub fn read_secret(home: &Path, kind: &str, secret: &str) -> Result<Option<SdkValue>> {
    let path = file(home, kind, secret)?;
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(super::super::record::file::unreadable(&path, &error)),
    };
    let read = super::super::record::file::record_of(&path, &text);
    crate::secret::wipe::text(text);
    read.map(Some)
}

/// Saves `value` as secret `name` of kind `kind`, replacing the one there
/// whole (0600, in 0700 folders), and moves the home's `generation`.
pub fn save_secret(home: &Path, kind: &str, secret: &str, value: &SdkValue) -> Result<()> {
    super::super::record::file::write_record_as(&file(home, kind, secret)?, value, true)?;
    generation::moved(home);
    Ok(())
}

/// Removes secret `name` of kind `kind`: whether there was one. A removal
/// moves the home's `generation`.
pub fn remove_secret(home: &Path, kind: &str, secret: &str) -> Result<bool> {
    match std::fs::remove_file(file(home, kind, secret)?) {
        Ok(()) => {
            generation::moved(home);
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(failed(error)),
    }
}

/// The names of the secrets of kind `kind`, sorted.
pub fn secret_names(home: &Path, kind: &str) -> Result<Vec<String>> {
    name("secret kind", kind).map_err(|why| refused("InvalidInput", why))?;
    let entries = match std::fs::read_dir(dir(home).join(kind)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(failed(error)),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(failed)?;
        let file_name = entry.file_name();
        let secret = file_name.to_str().and_then(|n| n.strip_suffix(".json"));
        if let Some(secret) = secret.filter(|n| name(kind, n).is_ok()) {
            found.push(secret.to_owned());
        }
    }
    found.sort();
    Ok(found)
}
