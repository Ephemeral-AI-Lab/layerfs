//! Placing a 1.1.0 sqlite Store's file where 2.0 opens it (#533 step 4).
//!
//! 1.1.0 kept every sqlite Store in a Docker volume, at
//! `<volumes>/<volume>/_data/global/store.sqlite`, and named the volume in
//! `stores.json` (a shared Store) or in `.runtime/projects/<p>.json` (a
//! project's own Store). Where this host can read that file (Docker on
//! Linux, `/var/lib/docker/volumes`), the conversion links it to
//! `store/<name>/store.sqlite`, which makes the Store a `native` one. Where
//! the volumes are on another filesystem the file is copied and the copy's
//! sha256 checked before it is renamed into place. The 1.1.0 file is never
//! moved or deleted. A file with a write-ahead log (a process stopped
//! without closing it) is not placed: its log would not follow it.
//! Anything not placed keeps its `volume` and is reported.
use super::paths;
use super::records::{private_dir, write_record};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::remote::storage::pieces;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// 1.1.0's Docker volumes directory on Linux (its `settings.volumes`).
pub(super) const DOCKER_VOLUMES: &str = "/var/lib/docker/volumes";

/// The Store file 1.1.0 kept in Docker volume `volume` under `volumes`;
/// `None` for a name Docker would not give a volume.
pub(super) fn old_file(volumes: &Path, volume: &str) -> Option<PathBuf> {
    let valid = volume
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_alphanumeric())
        && volume
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b));
    valid.then(|| volumes.join(volume).join("_data/global/store.sqlite"))
}

/// Places sqlite Store `store` (its `record`) if its 1.1.0 file is
/// readable under `volumes`. Answers the record as it is now, and a note
/// when the file was not placed.
pub(super) fn place(
    home: &Path,
    volumes: &Path,
    store: &str,
    record: SdkValue,
) -> Result<(SdkValue, Option<String>), layerfs_bridge::control::SdkRefusal> {
    let target = paths::store_file(home, store);
    let native = |record: SdkValue, from: &Path| {
        let record = super::with_field(record, "machine", SdkValue::text("native"));
        super::with_field(
            record,
            "file_1_1_0",
            SdkValue::text(from.display().to_string()),
        )
    };
    let Some(volume) = record.get_text("volume").map(str::to_owned) else {
        return Ok((record, None));
    };
    let Some(source) = old_file(volumes, &volume) else {
        let note = format!("Store {store}: `{volume}` is not a Docker volume name");
        return Ok((record, Some(note)));
    };
    if record.get_text("machine") == Some("native") {
        return Ok((record, None));
    }
    // a run stopped after placing the file but before its record
    if target.is_file() {
        let record = native(record, &source);
        write_record(&paths::store_record(home, store), &record)?;
        return Ok((record, None));
    }
    let not_placed = |why: String| {
        format!(
            "Store {store}: its 1.1.0 file {} was not placed ({why}); it stays in Docker \
             volume {volume}: copy global/store.sqlite from it to store/{store}/store.sqlite \
             on the Store's machine",
            source.display()
        )
    };
    match fs::metadata(&source) {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => return Ok((record, Some(not_placed("not a file".to_owned())))),
        Err(error) => return Ok((record, Some(not_placed(error.to_string())))),
    }
    let log = PathBuf::from(format!("{}-wal", source.display()));
    if fs::metadata(&log).is_ok_and(|meta| meta.len() > 0) {
        let why = "it has a write-ahead log: open it once with 1.1.0 to close it".to_owned();
        return Ok((record, Some(not_placed(why))));
    }
    private_dir(&paths::store_dir(home, store))?;
    if let Err(linked) = fs::hard_link(&source, &target) {
        if let Err(why) = copy_checked(&source, &target) {
            return Ok((record, Some(not_placed(format!("link: {linked}; {why}")))));
        }
    }
    let record = native(record, &source);
    write_record(&paths::store_record(home, store), &record)?;
    Ok((record, None))
}

/// Copies `source` to `target` through a private temporary, renamed in only
/// when its sha256 is the source's. The source is not changed.
fn copy_checked(source: &Path, target: &Path) -> Result<(), String> {
    let incoming = PathBuf::from(format!("{}.incoming", target.display()));
    let copied = (|| {
        fs::copy(source, &incoming).map_err(|e| format!("copy: {e}"))?;
        fs::set_permissions(&incoming, fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("copy: {e}"))?;
        let digest = |path: &Path| pieces::sha256_file(path, None).map_err(|e| e.to_string());
        if digest(source)? != digest(&incoming)? {
            return Err("the copy's sha256 differs from the file's".to_owned());
        }
        fs::rename(&incoming, target).map_err(|e| format!("copy: {e}"))
    })();
    if copied.is_err() {
        // only this call's own copy goes
        let _ = fs::remove_file(&incoming);
    }
    copied
}
