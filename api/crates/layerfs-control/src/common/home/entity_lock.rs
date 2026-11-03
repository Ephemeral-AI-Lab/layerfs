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
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

/// How long a Store or Sandbox lock waits for its holder.
pub const WAIT: Duration = Duration::from_secs(120);
const PAUSE: Duration = Duration::from_millis(5);

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
