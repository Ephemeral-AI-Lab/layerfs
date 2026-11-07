//! What host Init shares with `project.init` (`store::project::add::add_project`,
//! `project::postgres_init`): the installed Store's locator, Init's deadline
//! and the worker's half of an Init on another machine. The 1.1 sealed
//! `project.init` into a project volume is gone (#536 step 2).
use super::add::ProjectAdd;
use super::frame::InitFrame;
use layerfs_home::helpers::required;
use layerfs_home::outcome::{refused, Outcome};
use std::path::Path;
use std::time::{Duration, Instant};

/// Where the daemon opens the installed Store inside its Sandbox.
pub const LOCATOR: &str = "/layerfs-store/global/store.sqlite";
pub fn sixteen(bytes: [u8; 32]) -> [u8; 16] {
    let mut out = [0; 16];
    out.copy_from_slice(&bytes[..16]);
    out
}

/// Init's deadline, counted from now: the caller's required `timeoutMs`, a
/// positive whole number of milliseconds. There is no default here.
pub fn deadline(timeout_ms: Option<i64>) -> Result<Instant, layerfs_bridge::control::SdkRefusal> {
    let Some(ms @ 1..) = timeout_ms else {
        return Err(refused(
            "InvalidInput",
            "timeoutMs is required: a positive whole number of milliseconds",
        ));
    };
    Instant::now()
        .checked_add(Duration::from_millis(ms as u64))
        .ok_or_else(|| refused("InvalidInput", "timeoutMs is too large"))
}

/// `project.init` in a worker (H11, H11a) under its `root` (checked by
/// the caller): the add of the folder copy `frame.staged` the caller sent
/// ([`super::staged`]) under the worker's own lease on the Store file under
/// its root. The caller reports the progress (#536 §10b). The copy is
/// removed when the add ends, whatever its outcome. The caller writes the
/// home's project record.
pub fn in_worker(root: &Path, frame: &InitFrame) -> Outcome {
    let staged = super::staged::folder(root, &frame.staged);
    if !staged.is_dir() {
        return Err(refused(
            "NotFound",
            format!("no folder copy at {}", staged.display()),
        ));
    }
    let add = || {
        let _lease = crate::lease::hold_locked(root, &frame.store)?;
        crate::project::add::add_project(&ProjectAdd {
            home: root.to_path_buf(),
            store: frame.store.clone(),
            name: required(frame.name.as_deref(), "name")?.to_owned(),
            source: staged.clone(),
            timeout_ms: frame.timeout_ms,
            store_record: frame.store_record.clone(),
        })
    };
    let added = add();
    if let Err(why) = super::staged::remove(&staged) {
        eprintln!("layerfs worker: left {}: {}", staged.display(), why.message);
    }
    added
}
