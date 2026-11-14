//! A sqlite Store's file on the machine that holds it (H §7.3): the steps a
//! `store push`, a `store pull` and a `store remove` take on each side. They run in the
//! caller's process for `native` (under the home) and in a machine's worker
//! for every other machine (under the worker's root). Paths come from the
//! root and the Store's name only, never from the caller. The steps that
//! read, replace or delete the file close this process's open copy first
//! (a worker keeps a Store open after its calls); a call running on it is
//! `InUse`.
use super::step::{StepFrame, StoreStep};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bucket::backup::heads::heads;
use layerfs_bucket::bucket::pieces;
use layerfs_home::home::paths;
use layerfs_home::outcome::{failed, refused, Outcome};
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

fn length(path: &Path) -> Result<u64, SdkRefusal> {
    match std::fs::metadata(path) {
        Ok(meta) => Ok(meta.len()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(failed(e)),
    }
}

/// The bytes free to this process on the filesystem that holds `path`,
/// or would: its nearest existing ancestor.
fn free(path: &Path) -> Result<u64, SdkRefusal> {
    let at = path.ancestors().find(|at| at.exists()).unwrap_or(path);
    let filesystem = nix::sys::statvfs::statvfs(at).map_err(failed)?;
    let free = u128::from(filesystem.blocks_available()) * u128::from(filesystem.fragment_size());
    Ok(u64::try_from(free).unwrap_or(u64::MAX))
}

fn hex(bytes: &[u8]) -> SdkValue {
    SdkValue::text(layerfs_bridge::sdk_values::hex(bytes))
}

/// `{ bytes, sha256, heads }` of the closed Store file at `file`.
fn digest(file: &Path, progress: Option<&mut dyn FnMut(u64, u64)>) -> Outcome {
    let (bytes, sha256) = match progress {
        Some(emit) => pieces::sha256_file_observed(file, emit),
        None => pieces::sha256_file(file, None),
    }
    .map_err(|why| refused("Failed", why.to_string()))?;
    let heads = heads(file).map_err(|why| refused("Failed", why))?;
    Ok(SdkValue::map([
        ("bytes", SdkValue::Int(bytes as i64)),
        ("sha256", hex(&sha256)),
        ("heads", hex(&heads)),
    ]))
}

/// Checkpoints and closes a leftover `-wal` beside `file` (a crashed opener's,
/// or one whose close could not wait for the writer lane),
/// through the same `seal` a graceful close takes (#86): a Store whose last
/// opener was killed keeps its committed frames in the log, which blocks a
/// snapshot until one opener folds them in. Nothing runs while no live
/// process holds the Store (the caller holds the exclusive intent and
/// lease), so this opens it once here and seals it.
fn checkpoint_leftover_wal(file: &Path, frame: &StepFrame) -> Result<(), SdkRefusal> {
    if length(&with_suffix(file, "-wal"))? == 0 {
        return Ok(());
    }
    let Some(record) = &frame.store_record else {
        return Ok(());
    };
    let identity = crate::identity::identity_of(record)?;
    if let Ok(handles) = layerfs_persistence::SqliteHandles::open_writable(
        layerfs_persistence::PersistenceConfig::sqlite(file)
            .with_sqlite_profile(layerfs_persistence::SqlitePersistenceProfile::Disposable),
        &identity.binding,
        identity.cursor_key,
    ) {
        // a failure leaves the log for the caller's own message: no retry
        let _ = handles.seal();
    }
    Ok(())
}

/// Runs `frame`'s step on its Store under `root` (the home, or a worker's
/// root).
pub fn run(root: &Path, frame: &StepFrame) -> Outcome {
    run_observed(root, frame, &mut |_, _| {})
}

/// [`run`], reporting `(done, total)` bytes while hashing a `snapshot` or a
/// `verify` (#536 §10b).
pub fn run_observed(root: &Path, frame: &StepFrame, progress: &mut dyn FnMut(u64, u64)) -> Outcome {
    let store = frame.store.as_str();
    layerfs_home::names::StoreName::parse(store)?;
    let file = paths::store_file(root, store);
    let incoming = with_suffix(&file, ".incoming");
    if matches!(
        frame.step,
        StoreStep::Snapshot | StoreStep::Install | StoreStep::Retire | StoreStep::Remove
    ) {
        crate::open::close(&file)?;
    }
    match &frame.step {
        // whether the file is here, its size with its write-ahead log and
        // any partial incoming copy, and the free bytes where it is (or
        // would be)
        StoreStep::Stat => Ok(SdkValue::map([
            ("exists", SdkValue::Bool(file.is_file())),
            (
                "free",
                SdkValue::Int(free(&file)?.min(i64::MAX as u64) as i64),
            ),
            ("bytes", SdkValue::Int(length(&file)? as i64)),
            (
                "wal_bytes",
                SdkValue::Int(length(&with_suffix(&file, "-wal"))? as i64),
            ),
            ("incoming_bytes", SdkValue::Int(length(&incoming)? as i64)),
        ])),
        // snapshot: the idle Store's bytes, sha256 and history heads. A Store
        // nobody holds has no write-ahead log left unless a process crashed;
        // if a crashed opener left one, checkpoint and close it first.
        StoreStep::Snapshot => {
            if !file.is_file() {
                return Err(refused(
                    "NotFound",
                    format!("no Store file {}", file.display()),
                ));
            }
            checkpoint_leftover_wal(&file, frame)?;
            if length(&with_suffix(&file, "-wal"))? > 0 {
                return Err(refused(
                    "Failed",
                    format!(
                        "{} has an unfinished write-ahead log; open the Store once on its machine",
                        file.display()
                    ),
                ));
            }
            digest(&file, Some(progress))
        }
        // a consistent snapshot of the live Store for a backup, staged
        // beside it; `{ bytes, sha256, heads, sqlite }`
        StoreStep::Stage => {
            let staged = with_suffix(&file, ".backup");
            let _ = std::fs::remove_file(&staged);
            let open = crate::open_args::OpenArgs {
                home: Some(root.to_path_buf()),
                store: Some(store.to_owned()),
                store_path: Some(file.display().to_string()),
                store_record: frame.store_record.clone(),
                ..Default::default()
            };
            let sealed = crate::service::with_service(&open, |service| {
                service
                    .opened
                    .file()
                    .ok_or_else(|| {
                        refused(
                            "Failed",
                            format!(
                                "snapshot: {}",
                                layerfs_storage::port::PersistenceError::BackendUnavailable
                            ),
                        )
                    })?
                    .snapshot(&staged, &mut |_, _| {})
                    .map_err(|e| refused("Failed", format!("snapshot: {e}")))
            })?;
            let digest = digest(&staged, None)?;
            Ok(layerfs_home::home::with_field(
                digest,
                "sqlite",
                SdkValue::text(sealed.sqlite_version),
            ))
        }
        StoreStep::Unstage => {
            match std::fs::remove_file(with_suffix(&file, ".backup")) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(failed(e)),
                _ => {}
            }
            Ok(SdkValue::map([("unstaged", SdkValue::Bool(true))]))
        }
        StoreStep::Read {
            offset,
            len,
            staged,
        } => {
            let (offset, len) = (*offset, (*len).min(CHUNK));
            let source = if *staged {
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
        // send: the transfer's own staging file, written in order; offset 0
        // starts it again
        StoreStep::Write { offset, bytes } => {
            let offset = *offset;
            if file.exists() {
                return Err(refused(
                    "Exists",
                    format!("a Store file is already at {}", file.display()),
                ));
            }
            layerfs_home::home::private_dir(&paths::store_dir(root, store))?;
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
        StoreStep::Verify {
            bytes,
            sha256,
            heads,
        } => {
            let got = digest(&incoming, Some(progress))?;
            let text = |t: &Option<String>| t.as_deref().map(SdkValue::text);
            let expected = [
                ("bytes", bytes.map(SdkValue::Int)),
                ("sha256", text(sha256)),
                ("heads", text(heads)),
            ];
            for (field, want) in expected {
                if got.get(field) != Some(&want.unwrap_or_default()) {
                    return Err(refused(
                        "Failed",
                        format!("the copied Store's {field} is not the snapshot's; the copy is kept at {}", incoming.display()),
                    ));
                }
            }
            Ok(got)
        }
        // install: rename the verified copy into place, never over a Store
        StoreStep::Install => {
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
        // retire: a copy the record does not name (the source's, after the
        // record names the target, or what an ended transfer left), with
        // any partial copy; the Store's folder and records stay
        StoreStep::Retire => {
            for path in [
                file.clone(),
                with_suffix(&file, "-wal"),
                with_suffix(&file, "-shm"),
                incoming,
            ] {
                match std::fs::remove_file(&path) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(failed(e)),
                    _ => {}
                }
            }
            Ok(SdkValue::map([("retired", SdkValue::Bool(true))]))
        }
        // remove: the Store's whole directory on this machine (a worker's
        // root: `store.remove`, and the retire of a push's or pull's
        // source there). A symlink in it, or the directory itself if it is
        // one, is removed, never followed.
        StoreStep::Remove => {
            let dir = paths::store_dir(root, store);
            let existed = std::fs::symlink_metadata(&dir).is_ok();
            if existed {
                std::fs::remove_dir_all(&dir).map_err(failed)?;
            }
            Ok(SdkValue::map([("removed", SdkValue::Bool(existed))]))
        }
    }
}
