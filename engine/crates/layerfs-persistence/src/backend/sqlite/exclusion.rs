//! Admission evidence that another process holds a SQLite Store file open.
//!
//! The seal runs on the last connection: its close removes `-wal`/`-shm`. A
//! client that still holds the file keeps them, and the seal then refuses
//! `Conflict` only after the whole mark. The sidecars alone cannot tell that
//! client from a crash remnant (which the next clean open removes), so this
//! reads open file descriptions instead. On Linux `/proc/<pid>/fd` names
//! them; the scan is bounded, a positive match is definite, and an unreadable
//! or hidden entry is never a refusal: the seal's own sidecar fence remains
//! the last guard.
use std::path::Path;

/// The most descriptor entries one admission reads. A scan that reaches the
/// bound answers `false` (not proven) instead of guessing; only a positive
/// match refuses.
#[cfg(target_os = "linux")]
const MAX_ENTRIES: usize = 100_000;

/// True when a process other than this one already holds the Store at `store`
/// open, through its main file or a sidecar. The compaction's own connections
/// (its session and, on a Normal profile, the checkpoint thread) are in
/// this process and are not evidence; a caller that keeps another in-process
/// connection open is still caught by the seal's sidecar fence.
#[cfg(target_os = "linux")]
pub(crate) fn held_by_other_process(store: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let identity = |path: &Path| {
        std::fs::metadata(path)
            .ok()
            .map(|meta| (meta.dev(), meta.ino()))
    };
    let Some(main) = identity(store) else {
        return false;
    };
    let mut watched = vec![main];
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = store.as_os_str().to_os_string();
        sidecar.push(suffix);
        if let Some(identity) = identity(Path::new(&sidecar)) {
            watched.push(identity);
        }
    }
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    let own = std::process::id();
    let mut seen = 0usize;
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == own {
            continue;
        }
        // An unreadable descriptor table (another user, a retired pid) hides
        // what it holds; it is skipped, never read as absence.
        let Ok(descriptors) = std::fs::read_dir(entry.path().join("fd")) else {
            continue;
        };
        for descriptor in descriptors.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                return false;
            }
            let Ok(meta) = std::fs::metadata(descriptor.path()) else {
                continue;
            };
            if watched.contains(&(meta.dev(), meta.ino())) {
                return true;
            }
        }
    }
    false
}

/// No portable descriptor table: the seal's sidecar fence stays the only
/// guard, exactly as before this admission probe.
#[cfg(not(target_os = "linux"))]
pub(crate) fn held_by_other_process(_store: &Path) -> bool {
    false
}
