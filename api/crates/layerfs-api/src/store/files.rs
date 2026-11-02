//! A sqlite Store's file on the machine that holds it (H §7.3): the steps a
//! `store move` and a `store remove` take on each side. They run in the
//! caller's process for `native` (under the home) and in a machine's worker
//! for every other machine (under the worker's root). Paths come from the
//! root and the Store's name only, never from the caller. The steps that
//! read, replace or delete the file close this process's open copy first
//! (a worker keeps a Store open after its calls); a call running on it is
//! `InUse`.
use crate::home::paths;
use crate::host::outcome::{failed, refused, Outcome};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_store_backup::backup::heads::heads;
use layerfs_store_backup::bucket::pieces;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// The largest piece one `read` returns.
pub const CHUNK: u64 = 8 << 20;

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn int(args: &SdkValue, name: &str) -> Result<u64, SdkRefusal> {
    match args.get(name) {
        Some(SdkValue::Int(n)) if *n >= 0 => Ok(*n as u64),
        _ => Err(refused(
            "InvalidInput",
            format!("`{name}` must be a whole number"),
        )),
    }
}

fn length(path: &Path) -> Result<u64, SdkRefusal> {
    match std::fs::metadata(path) {
        Ok(meta) => Ok(meta.len()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(failed(e)),
    }
}

fn hex(bytes: &[u8]) -> SdkValue {
    SdkValue::text(layerfs_bridge::sdk_values::hex(bytes))
}

/// `{ bytes, sha256, heads }` of the closed Store file at `file`.
fn digest(file: &Path) -> Outcome {
    let (bytes, sha256) =
        pieces::sha256_file(file, None).map_err(|why| refused("Failed", why.to_string()))?;
    let heads = heads(file).map_err(|why| refused("Failed", why))?;
    Ok(SdkValue::map([
        ("bytes", SdkValue::Int(bytes as i64)),
        ("sha256", hex(&sha256)),
        ("heads", hex(&heads)),
    ]))
}

/// Runs step `op` on Store `store` under `root` (the home, or a worker's root).
pub(crate) fn run(root: &Path, op: &str, args: &SdkValue) -> Outcome {
    let store = args
        .get_text("store")
        .ok_or_else(|| refused("InvalidInput", "a Store step names its Store"))?;
    crate::names::name("store", store).map_err(|why| refused("InvalidInput", why))?;
    let file = paths::store_file(root, store);
    let incoming = with_suffix(&file, ".incoming");
    if matches!(op, "snapshot" | "install" | "retire" | "remove") {
        crate::store::open::close(&file)?;
    }
    match op {
        // whether the file is here, and its size with its write-ahead log
        "stat" => Ok(SdkValue::map([
            ("exists", SdkValue::Bool(file.is_file())),
            ("bytes", SdkValue::Int(length(&file)? as i64)),
            (
                "wal_bytes",
                SdkValue::Int(length(&with_suffix(&file, "-wal"))? as i64),
            ),
        ])),
        // snapshot: the idle Store's bytes, sha256 and history heads. A Store
        // nobody holds has no write-ahead log left unless a process crashed.
        "snapshot" => {
            if !file.is_file() {
                return Err(refused(
                    "NotFound",
                    format!("no Store file {}", file.display()),
                ));
            }
            if length(&with_suffix(&file, "-wal"))? > 0 {
                return Err(refused(
                    "Failed",
                    format!(
                        "{} has an unfinished write-ahead log; open the Store once on its machine",
                        file.display()
                    ),
                ));
            }
            digest(&file)
        }
        // a consistent snapshot of the live Store for a backup, staged
        // beside it; `{ bytes, sha256, heads, sqlite }`
        "stage" => {
            let staged = with_suffix(&file, ".backup");
            let _ = std::fs::remove_file(&staged);
            let mut fields = match args.clone() {
                SdkValue::Map(fields) => fields,
                _ => Vec::new(),
            };
            fields.push((
                "home".to_owned(),
                SdkValue::text(root.display().to_string()),
            ));
            fields.push((
                "storePath".to_owned(),
                SdkValue::text(file.display().to_string()),
            ));
            let sealed = crate::host::direct::with_service(&SdkValue::Map(fields), |service| {
                service
                    .opened
                    .snapshot_observed(&staged, &mut |_, _| {})
                    .map_err(|e| refused("Failed", format!("snapshot: {e}")))
            })?;
            let digest = digest(&staged)?;
            Ok(crate::home::with_field(
                digest,
                "sqlite",
                SdkValue::text(sealed.sqlite_version),
            ))
        }
        "unstage" => {
            match std::fs::remove_file(with_suffix(&file, ".backup")) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(failed(e)),
                _ => {}
            }
            Ok(SdkValue::map([("unstaged", SdkValue::Bool(true))]))
        }
        "read" => {
            let offset = int(args, "offset")?;
            let len = int(args, "len")?.min(CHUNK);
            let source = if matches!(args.get("staged"), Some(SdkValue::Bool(true))) {
                with_suffix(&file, ".backup")
            } else {
                file.clone()
            };
            let mut handle = std::fs::File::open(&source).map_err(failed)?;
            handle.seek(SeekFrom::Start(offset)).map_err(failed)?;
            let mut bytes = Vec::with_capacity(len as usize);
            handle.take(len).read_to_end(&mut bytes).map_err(failed)?;
            Ok(SdkValue::map([("bytes", SdkValue::Bytes(bytes))]))
        }
        // send: the mover's own staging file, written in order; offset 0
        // starts it again
        "write" => {
            let offset = int(args, "offset")?;
            let Some(SdkValue::Bytes(bytes)) = args.get("bytes") else {
                return Err(refused("InvalidInput", "`bytes` must be bytes"));
            };
            if file.exists() {
                return Err(refused(
                    "Exists",
                    format!("a Store file is already at {}", file.display()),
                ));
            }
            crate::home::private_dir(&paths::store_dir(root, store))?;
            let mut handle = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(offset == 0)
                .mode(0o600)
                .open(&incoming)
                .map_err(failed)?;
            if handle.metadata().map_err(failed)?.len() != offset {
                return Err(refused(
                    "Failed",
                    "the staged copy is not the length this piece follows",
                ));
            }
            handle.seek(SeekFrom::Start(offset)).map_err(failed)?;
            handle.write_all(bytes).map_err(failed)?;
            Ok(SdkValue::map([(
                "bytes",
                SdkValue::Int((offset + bytes.len() as u64) as i64),
            )]))
        }
        // verify: the staged copy's sha256 and heads are the snapshot's
        "verify" => {
            let got = digest(&incoming)?;
            for field in ["bytes", "sha256", "heads"] {
                if got.get(field) != args.get(field) {
                    return Err(refused(
                        "Failed",
                        format!("the copied Store's {field} is not the snapshot's; the copy is kept at {}", incoming.display()),
                    ));
                }
            }
            Ok(got)
        }
        // install: rename the verified copy into place, never over a Store
        "install" => {
            if file.exists() {
                return Err(refused(
                    "Exists",
                    format!("a Store file is already at {}", file.display()),
                ));
            }
            std::fs::set_permissions(&incoming, std::fs::Permissions::from_mode(0o600))
                .map_err(failed)?;
            std::fs::rename(&incoming, &file).map_err(failed)?;
            Ok(SdkValue::map([(
                "installed",
                SdkValue::text(file.display().to_string()),
            )]))
        }
        // retire: the source's copy, after the record names the target
        "retire" => {
            for path in [
                file.clone(),
                with_suffix(&file, "-wal"),
                with_suffix(&file, "-shm"),
            ] {
                match std::fs::remove_file(&path) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(failed(e)),
                    _ => {}
                }
            }
            Ok(SdkValue::map([("retired", SdkValue::Bool(true))]))
        }
        // remove: the Store's whole directory on this machine
        "remove" => {
            let dir = paths::store_dir(root, store);
            let existed = dir.exists();
            if existed {
                std::fs::remove_dir_all(&dir).map_err(failed)?;
            }
            Ok(SdkValue::map([("removed", SdkValue::Bool(existed))]))
        }
        _ => Err(refused("InvalidInput", format!("no Store step {op}"))),
    }
}
