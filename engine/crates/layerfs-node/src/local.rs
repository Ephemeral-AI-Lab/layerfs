//! What this process can tell of its home's local SQLite Stores without
//! opening them for Store calls (#506 E1, E2): each Store file's size and
//! its WAL's (stat), whether the Store is open now (its `-shm` exists), and
//! its history heads digest.
//!
//! The digest is read only when the file or WAL changed (size or mtime)
//! since this process last read it; otherwise the remembered value is
//! answered. A read opens a connection of its own and closes it before it
//! returns, never through the Store clients this process keeps, so it
//! never keeps a Store open for a pull to find. Reads of one Store are
//! serialized with its pull and clone in this process: while either runs,
//! the remembered value is answered and nothing is opened; a pull waits for
//! a read in flight, which is one short query.
use crate::job::{refused, Outcome};
use crate::registry::{lock, text};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::hex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::UNIX_EPOCH;

/// `(file size, file mtime, WAL size, WAL mtime)`, mtimes in nanoseconds.
type Key = (u64, u128, u64, u128);
/// The last digest read of each Store file, under the key it was read at.
type Remembered = BTreeMap<PathBuf, (Key, [u8; 32])>;

static HEADS: LazyLock<Mutex<Remembered>> = LazyLock::new(Default::default);
static GATES: LazyLock<Mutex<BTreeMap<PathBuf, Arc<Mutex<()>>>>> = LazyLock::new(Default::default);

/// The gate of the Store file `path`: its pull and clone hold it, and a
/// heads read takes it only when it is free.
pub(crate) fn gate(path: &Path) -> Result<Arc<Mutex<()>>, SdkRefusal> {
    Ok(lock(&GATES)?
        .entry(path.to_owned())
        .or_insert_with(Default::default)
        .clone())
}

/// Runs `work` holding the gate of the Store file `path` (pull and clone).
pub(crate) fn exclusive<T>(
    path: &Path,
    work: impl FnOnce() -> Result<T, SdkRefusal>,
) -> Result<T, SdkRefusal> {
    let gate = gate(path)?;
    let _held = gate.lock().unwrap_or_else(|poison| poison.into_inner());
    work()
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// A file's size and mtime; `(0, 0)` when it is absent.
fn stat(path: &Path) -> std::io::Result<(u64, u128)> {
    match std::fs::metadata(path) {
        Ok(found) => {
            let modified = found
                .modified()?
                .duration_since(UNIX_EPOCH)
                .map_or(0, |since| since.as_nanos());
            Ok((found.len(), modified))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok((0, 0)),
        Err(error) => Err(error),
    }
}

/// The heads digest of `path` under `key`: remembered, or read now when the
/// gate is free; `None` when it cannot be read.
fn heads(path: &Path, key: Key) -> Result<Option<[u8; 32]>, SdkRefusal> {
    let remembered = lock(&HEADS)?.get(path).copied();
    if let Some((seen, digest)) = remembered {
        if seen == key {
            return Ok(Some(digest));
        }
    }
    let gate = gate(path)?;
    let Ok(_held) = gate.try_lock() else {
        // a pull or clone of this Store runs: answer what was read before
        return Ok(remembered.map(|(_, digest)| digest));
    };
    match layerfs_store_service::remote::heads(path) {
        Ok(digest) => {
            lock(&HEADS)?.insert(path.to_owned(), (key, digest));
            Ok(Some(digest))
        }
        Err(_) => Ok(None),
    }
}

/// One Store's entry: `{ store, bytes: { file, wal } | null, open, heads }`.
fn one(store: &str, path: &Path) -> Result<SdkValue, SdkRefusal> {
    let file = std::fs::metadata(path);
    let wal = stat(&sidecar(path, "-wal"));
    let open = std::fs::symlink_metadata(sidecar(path, "-shm"));
    let (Ok(file), Ok(wal)) = (file, wal) else {
        return Ok(SdkValue::map([
            ("store", SdkValue::text(store)),
            ("bytes", SdkValue::Null),
            ("open", SdkValue::Null),
            ("heads", SdkValue::Null),
        ]));
    };
    let modified = file
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_nanos());
    let open = match open {
        Ok(_) => SdkValue::Bool(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => SdkValue::Bool(false),
        Err(_) => SdkValue::Null,
    };
    let digest = heads(path, (file.len(), modified, wal.0, wal.1))?;
    Ok(SdkValue::map([
        ("store", SdkValue::text(store)),
        (
            "bytes",
            SdkValue::map([
                ("file", SdkValue::Int(file.len() as i64)),
                ("wal", SdkValue::Int(wal.0 as i64)),
            ]),
        ),
        ("open", open),
        (
            "heads",
            digest.map_or(SdkValue::Null, |d| SdkValue::text(hex(&d))),
        ),
    ]))
}

/// `runtime.store.stat { home, stores: [{ store, storePath }] }`: each local
/// SQLite Store's `{ store, bytes: { file, wal }, open, heads }`; a Store
/// whose file this process cannot stat has nulls.
pub(crate) fn stat_stores(args: &SdkValue) -> Outcome {
    let Some(SdkValue::List(stores)) = args.get("stores") else {
        return Err(refused(
            "InvalidInput",
            "stores is a list of { store, storePath }",
        ));
    };
    stores
        .iter()
        .map(|entry| one(text(entry, "store")?, Path::new(text(entry, "storePath")?)))
        .collect::<Result<Vec<_>, _>>()
        .map(SdkValue::List)
}
