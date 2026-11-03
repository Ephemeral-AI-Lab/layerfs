//! What host Init shares with `project.init` (`store::identity::add_project`,
//! `project::postgres_init`): the installed Store's locator, Init's deadline
//! and its live progress. The 1.1 sealed `project.init` into a project
//! volume is gone (#536 step 2).
use crate::common::outcome::{refused, Outcome};
use crate::sandbox::registry::{lock, text, INITS};
use layerfs_bridge::control::SdkValue;
use layerfs_project::InitStep;
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

/// Runs an add on its own thread while this one reports its counts to
/// `report` (`step`, `entries`, `files`, `bytes`) as they change.
pub(crate) fn watched(
    report: &mut dyn FnMut(SdkValue),
    watch: &SdkValue,
    add: impl FnOnce() -> Outcome + Send,
) -> Outcome {
    let outcome = std::thread::scope(|scope| {
        let worker = scope.spawn(add);
        let mut last = SdkValue::Null;
        while !worker.is_finished() {
            if let Ok(counts) = progress(watch) {
                if counts != SdkValue::Null && counts != last {
                    report(counts.clone());
                    last = counts;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        worker.join()
    });
    outcome.map_err(|_| refused("Failed", "the Init thread panicked"))?
}

/// `project.init` in a worker (H11), `{ home (its root), store, storePath,
/// name, path, timeoutMs, store_record }`: the add under the worker's own
/// lease on the Store file under its root, its counts sent as `{ progress }`
/// events. The caller writes the home's project record.
pub(crate) fn in_worker(args: &SdkValue, events: &mut dyn FnMut(SdkValue)) -> Outcome {
    let root = crate::sandbox::files::home(args)?;
    let store = text(args, "store")?;
    crate::common::names::StoreName::parse(store)?;
    let name = text(args, "name")?;
    let _lease = crate::common::home::lease::hold_locked(&root, store)?;
    let watch = SdkValue::map([
        ("store", SdkValue::text(store)),
        ("project", SdkValue::text(name)),
    ]);
    let mut report = |counts: SdkValue| events(SdkValue::map([("progress", counts)]));
    watched(&mut report, &watch, || {
        crate::store::identity::add_project(args)
    })
}

/// How far the project's running Init has come
/// (`step`, `entries`, `files`, `bytes`), or null when none runs.
pub(crate) fn progress(args: &SdkValue) -> Outcome {
    let name = crate::store::identity::init_key(args, text(args, "project")?);
    let Some(counters) = lock(&INITS)?.get(&name).cloned() else {
        return Ok(SdkValue::Null);
    };
    let counts = counters.read();
    let step = match counts.step {
        InitStep::Scan => "scan",
        InitStep::Files => "files",
        InitStep::Tree => "tree",
        InitStep::Publish => "publish",
        InitStep::Seal => "seal",
    };
    let number = |n: u64| SdkValue::Int(i64::try_from(n).unwrap_or(i64::MAX));
    Ok(SdkValue::map([
        ("step", SdkValue::text(step)),
        ("entries", number(counts.entries)),
        ("files", number(counts.files)),
        ("bytes", number(counts.bytes)),
    ]))
}
