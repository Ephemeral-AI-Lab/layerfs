//! The idle checkpoint (#86): when the last call on a SQLite Store
//! returns, or a Sandbox on it closes, with no other call running here, its
//! write-ahead log is checkpointed and truncated once. The Store stays open.
use super::cache::*;
use crate::admin::open::args::OpenArgs;
use crate::admin::open::service as direct;
use layerfs_home::outcome::failed;
use layerfs_protocol::sdk::SdkRefusal;
use std::path::Path;

/// Whether this process holds the Store at `path` open now.
pub(crate) fn holds(path: &Path) -> bool {
    cache().contains_key(path)
}

/// Starts an idle checkpoint on `entry` when it is the last user here: no
/// call and no other checkpoint running. Its open arguments.
pub(super) fn settle(entry: &mut Entry) -> Option<OpenArgs> {
    if entry.calls > 0 || entry.settling > 0 {
        return None;
    }
    entry.settling += 1;
    Some(entry.args.clone())
}

/// Ends an idle checkpoint started by [`settle`] on the Store at `path`.
pub(super) fn settled(path: &Path, result: Result<Option<bool>, SdkRefusal>) {
    let mut open = cache();
    if let Some(entry) = open.get_mut(path).and_then(Cached::ready) {
        entry.settling -= 1;
        entry.checkpoint = result;
    }
    CHANGED.notify_all();
}

/// Whether the Store file at `path` has a non-empty write-ahead log. A
/// PostgreSQL Store's key names no file: never.
pub(super) fn has_log(path: &Path) -> bool {
    wal_remaining(path) == Some(true)
}

pub(super) fn wal_remaining(path: &Path) -> Option<bool> {
    let mut log = path.as_os_str().to_os_string();
    log.push("-wal");
    match std::fs::symlink_metadata(log) {
        Ok(metadata) => Some(metadata.len() > 0),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(false),
        Err(_) => None,
    }
}

/// One `wal_checkpoint(TRUNCATE)` on the open Store `args` name, through
/// its open service. `Some(true)`: its log is empty; `Some(false)`: left for
/// the idle close; `None`: no writer available or the checkpoint failed.
/// An explicit Sandbox release can start a writer to complete its checkpoint.
pub(super) fn checkpoint(args: &OpenArgs, explicit: bool) -> Result<Option<bool>, SdkRefusal> {
    direct::with_service(args, |service| {
        service.opened.checkpoint(explicit).map_err(failed)
    })
}

/// A Sandbox on the Store at `path` has closed: with no call running on it
/// here, its log is checkpointed and truncated once now, before the close
/// is answered (#86). The Store stays open. What [`checkpoint`] returns, or
/// `None` when nothing ran.
pub fn released(path: &Path) -> Option<bool> {
    if !has_log(path) {
        return None;
    }
    let args = {
        let mut open = cache();
        let cached = open.get_mut(path)?;
        if cached.host.is_some() {
            return None;
        }
        settle(cached.ready()?)?
    };
    direct::transition(
        || checkpoint(&args, true),
        |result| {
            let done = result.as_ref().ok().copied().flatten();
            settled(path, result);
            done
        },
    )
}
