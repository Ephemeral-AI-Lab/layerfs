//! What host Init shares with `project.init` (`store::identity::add_project`,
//! `project::postgres_init`): the installed Store's locator, Init's deadline
//! and the worker's half of an Init on another machine. The 1.1 sealed
//! `project.init` into a project volume is gone (#536 step 2).
use layerfs_bridge::control::SdkValue;
use layerfs_home::helpers::text;
use layerfs_home::outcome::{refused, Outcome};
use std::time::{Duration, Instant};

/// Where the daemon opens the installed Store inside its Sandbox.
pub(crate) const LOCATOR: &str = "/layerfs-store/global/store.sqlite";
pub(crate) fn sixteen(bytes: [u8; 32]) -> [u8; 16] {
    let mut out = [0; 16];
    out.copy_from_slice(&bytes[..16]);
    out
}

/// Init's deadline, counted from now: the caller's required `timeoutMs`, a
/// positive whole number of milliseconds. There is no default here.
pub(crate) fn deadline(args: &SdkValue) -> Result<Instant, layerfs_bridge::control::SdkRefusal> {
    let Some(SdkValue::Int(ms @ 1..)) = args.get("timeoutMs") else {
        return Err(refused(
            "InvalidInput",
            "timeoutMs is required: a positive whole number of milliseconds",
        ));
    };
    Instant::now()
        .checked_add(Duration::from_millis(*ms as u64))
        .ok_or_else(|| refused("InvalidInput", "timeoutMs is too large"))
}

/// `project.init` in a worker (H11, H11a), `{ home (its root), store,
/// storePath, name, staged, timeoutMs, store_record }`: the add of the
/// folder copy `staged` the caller sent ([`super::staged`]) under the
/// worker's own lease on the Store file under its root. The caller reports
/// the progress (#536 §10b). The copy is removed when the add ends,
/// whatever its outcome. The caller writes the home's project record.
pub(crate) fn in_worker(args: &SdkValue) -> Outcome {
    let root = crate::sandbox::files::home(args)?;
    let store = text(args, "store")?;
    layerfs_home::names::StoreName::parse(store)?;
    let staged = super::staged::folder(&root, super::staged::id(args)?);
    if !staged.is_dir() {
        return Err(refused(
            "NotFound",
            format!("no folder copy at {}", staged.display()),
        ));
    }
    let args = layerfs_home::home::with_field(
        args.clone(),
        "path",
        SdkValue::text(staged.display().to_string()),
    );
    let add = || {
        let _lease = crate::common::lease::hold_locked(&root, store)?;
        crate::store::identity::add_project(&args)
    };
    let added = add();
    if let Err(why) = super::staged::remove(&staged) {
        eprintln!("layerfs worker: left {}: {}", staged.display(), why.message);
    }
    added
}
