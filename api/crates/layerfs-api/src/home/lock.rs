//! The home lock, `layerfs.lock` (H §7.8): it guards record writes and
//! Sandbox-name uniqueness. Only host processes take it. Taking it waits,
//! bounded: a readiness wait, never a `Busy` refusal. Within one process,
//! calls take turns before the file lock, since `flock` is held per open
//! file description. A thread that holds it takes it again at once, so a
//! record writer can take it whether or not its caller already holds it.
use crate::host::outcome::{failed, refused};
use layerfs_bridge::control::SdkRefusal;
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

/// How long a call waits for another holder of the home lock.
pub const WAIT: Duration = Duration::from_secs(120);
const PAUSE: Duration = Duration::from_millis(5);

/// Each held home: the thread holding it and how many times it took it.
static HELD: Mutex<Vec<(PathBuf, ThreadId, usize)>> = Mutex::new(Vec::new());
static FREED: Condvar = Condvar::new();

/// The held home lock; dropping it releases it.
pub struct HomeLock {
    /// The locked file; only the outermost take of a thread holds it.
    file: Option<File>,
    home: PathBuf,
}

impl Drop for HomeLock {
    fn drop(&mut self) {
        if let Some(file) = &self.file {
            crate::host::flock::unlock(file);
        }
        release(&self.home);
    }
}

/// One take of `home` by this thread is over; the last frees it.
fn release(home: &Path) {
    let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(at) = held.iter().position(|(h, _, _)| h == home) {
        held[at].2 -= 1;
        if held[at].2 == 0 {
            held.remove(at);
            FREED.notify_all();
        }
    }
}

/// Takes the lock of `home`, waiting up to [`WAIT`] for its holder.
pub fn take(home: &Path) -> Result<HomeLock, SdkRefusal> {
    let deadline = Instant::now() + WAIT;
    let home = home.to_owned();
    let me = std::thread::current().id();
    {
        let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            match held.iter_mut().find(|(h, _, _)| h == &home) {
                None => break,
                Some((_, thread, depth)) if *thread == me => {
                    *depth += 1;
                    return Ok(HomeLock { file: None, home });
                }
                Some(_) => {}
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(expired(&home));
            }
            held = FREED
                .wait_timeout(held, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        held.push((home.clone(), me, 1));
    }
    let opened = super::records::private_dir(&home).and_then(|()| {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(home.join("layerfs.lock"))
            .map_err(failed)
    });
    let file = match opened {
        Ok(file) => file,
        Err(refusal) => {
            release(&home);
            return Err(refusal);
        }
    };
    loop {
        match crate::host::flock::try_lock(&file, true) {
            Ok(true) => {
                return Ok(HomeLock {
                    file: Some(file),
                    home,
                })
            }
            Ok(false) if Instant::now() < deadline => std::thread::sleep(PAUSE),
            Ok(false) => {
                release(&home);
                return Err(expired(&home));
            }
            Err(error) => {
                release(&home);
                return Err(failed(error));
            }
        }
    }
}

fn expired(home: &Path) -> SdkRefusal {
    refused(
        "Unavailable",
        format!(
            "another process held {} for {} s; nothing was changed",
            home.join("layerfs.lock").display(),
            WAIT.as_secs()
        ),
    )
}
