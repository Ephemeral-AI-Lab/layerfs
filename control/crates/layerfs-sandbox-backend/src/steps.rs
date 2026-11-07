//! The step times of one command a traced call runs (`LAYERFS_TRACE_STEP`,
//! the home's `trace` module): its caller [`begin`]s them on its thread before
//! the run and [`take`]s them after. A namespaces command marks its own
//! steps (`exec.*`), and its helper, given `--trace <file>`, writes the
//! wall-clock time each of its stages ended to that file; they become the
//! `helper.*` steps. Not begun, a mark is one thread-local check and the
//! helper is given no file.
use std::cell::RefCell;
use std::time::{Duration, Instant};

thread_local! {
    /// The last mark and the steps so far, while begun.
    static STEPS: RefCell<Option<Steps>> = const { RefCell::new(None) };
}

type Steps = (Instant, Vec<(String, Duration)>);

/// Starts timing this thread's command steps, from now.
pub fn begin() {
    STEPS.set(Some((Instant::now(), Vec::new())));
}

/// The steps timed since [`begin`], which stops timing them.
pub fn take() -> Vec<(String, Duration)> {
    STEPS.take().map(|(_, steps)| steps).unwrap_or_default()
}

#[cfg(target_os = "linux")]
pub(crate) use linux::{helper, helper_file, mark};

#[cfg(target_os = "linux")]
mod linux {
    use super::STEPS;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    /// Ends step `name`, the time since the last mark, and marks now.
    pub(crate) fn mark(name: &str) {
        STEPS.with_borrow_mut(|steps| {
            if let Some((last, list)) = steps {
                let now = Instant::now();
                list.push((name.to_owned(), now - *last));
                *last = now;
            }
        });
    }

    /// A new empty file, this process's own, for one helper's stage times;
    /// `None` when this thread's steps are not timed or it cannot be made.
    pub(crate) fn helper_file() -> Option<PathBuf> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        if STEPS.with_borrow(Option::is_none) {
            return None;
        }
        let name = format!(
            "layerfs-helper-steps-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        use std::os::unix::fs::OpenOptionsExt;
        let made = (std::fs::OpenOptions::new().write(true))
            .create_new(true)
            .mode(0o600)
            .open(&path);
        made.ok().map(|_| path)
    }

    /// Adds the helper's stages from its `file` (`<step> <microseconds since
    /// the epoch>` lines, in order) as steps, each the time since the one
    /// before (the first since `spawned`), then `helper.exit` until now, and
    /// removes the file. A file it did not write adds nothing.
    pub(crate) fn helper(file: &Path, spawned: SystemTime) {
        let text = std::fs::read_to_string(file).unwrap_or_default();
        let _ = std::fs::remove_file(file);
        let micros = |at: SystemTime| at.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_micros());
        let mut last = micros(spawned);
        let mut stages = Vec::new();
        for line in text.lines() {
            let Some((step, at)) = line.split_once(' ') else {
                return;
            };
            let Ok(at) = at.parse::<u128>() else {
                return;
            };
            stages.push((step.to_owned(), at.saturating_sub(last)));
            last = at;
        }
        if stages.is_empty() {
            return;
        }
        stages.push((
            "helper.exit".to_owned(),
            micros(SystemTime::now()).saturating_sub(last),
        ));
        STEPS.with_borrow_mut(|steps| {
            if let Some((_, list)) = steps {
                for (step, took) in stages {
                    let took = Duration::from_micros(u64::try_from(took).unwrap_or(u64::MAX));
                    list.push((step, took));
                }
            }
        });
    }
}
