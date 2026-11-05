//! Per-entity locks (#536 §8): `machine/<m>/start.lock`, `store/<s>/lock`
//! and `sandbox/<name>/lock`, each an exclusive `flock` on a 0600 file in
//! the entity's folder. The order is home, then start, then store, then
//! sandbox, never the reverse. None is held across a worker call, a VM boot
//! or a byte transfer, except the start lock across the start it guards:
//! the `op` intent keeps other operations out meanwhile. A wait for a
//! holder is bounded and is a readiness wait, not a retry.
use crate::common::outcome::{failed, refused};
use layerfs_bridge::control::SdkRefusal;
use std::fs::{File, OpenOptions};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::time::{Duration, Instant};

/// How long a Store or Sandbox lock waits for its holder.
pub const WAIT: Duration = Duration::from_secs(120);
const PAUSE: Duration = Duration::from_millis(5);

/// What an entity lock file says without taking it (#551 B8-156): whether
/// an exclusive holder has it now, and when it was last written. A probe
/// opens the file read-only and never creates or truncates it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeaseProbe {
    /// Whether another process holds the lock now.
    pub held: bool,
    /// The file's modification time, milliseconds since the epoch.
    pub since: Option<i64>,
}

/// Probes the entity lock file `path`: `None` when it does not exist, else
/// whether it is held. A shared `flock` that an exclusive holder refuses is
/// the probe; it is released at once, so nothing is waited for.
pub fn probe(path: &Path) -> Option<LeaseProbe> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let since = metadata
        .mtime()
        .checked_mul(1000)
        .and_then(|ms| ms.checked_add(metadata.mtime_nsec() / 1_000_000));
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .ok()?;
    let held = match crate::common::home::flock::try_lock(&file, false) {
        Ok(true) => {
            crate::common::home::flock::unlock(&file);
            false
        }
        Ok(false) => true,
        Err(_) => true,
    };
    Some(LeaseProbe { held, since })
}

/// A held entity lock; dropping it releases it.
#[derive(Debug)]
pub struct EntityLock(File);

impl Drop for EntityLock {
    fn drop(&mut self) {
        crate::common::home::flock::unlock(&self.0);
    }
}

/// Takes the lock file `path` (its folder must exist), waiting up to
/// `wait`; past it the call is refused `Unavailable` with `held`, and
/// nothing was changed.
pub fn take(path: &Path, wait: Duration, held: &str) -> Result<EntityLock, SdkRefusal> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(failed)?;
    let deadline = Instant::now() + wait;
    while !crate::common::home::flock::try_lock(&file, true).map_err(failed)? {
        if Instant::now() >= deadline {
            return Err(refused(
                "Unavailable",
                format!("{held} for {} s; nothing was changed", wait.as_secs()),
            ));
        }
        std::thread::sleep(PAUSE);
    }
    Ok(EntityLock(file))
}
