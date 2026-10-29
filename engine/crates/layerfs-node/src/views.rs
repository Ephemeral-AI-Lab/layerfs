//! Live, read-only views of Store files on a remote (#510).
//!
//! `runtime.remote.open_view` stats the remote file once and opens its Store
//! in place, read-only, through ranged requests pinned to that version
//! (`layerfs_store_service::remote::open_view`). The view is addressed by
//! its id and by its store path, `remote:<connection>/<file>#<view>`, which
//! a client passes as `storePath` like any Store's. A view reads a Store's
//! tree, sizes and history, never file contents: the folder listings,
//! stats and globs, Branches and Commits, each file's revisions and the
//! storage figures work on it; a call that reads file bytes (a file's
//! text, grep, blame, and diffs, which compare bytes) is refused
//! `Unsupported`, and every other call `ReadOnly`, before it runs. A view is
//! never registered as a Store of the home and never linked. A read that
//! failed on the remote is answered with the failure's kind: `Changed`
//! (the file was replaced since the view opened), `Unavailable`, `Denied`,
//! `Unknown` (a request's outcome is unknown) or `Failed`. Nothing is
//! retried. A failed read quarantines the Store session that made it, as
//! on any Store, so a view that met a remote failure answers that failure
//! on every later call that fails, and is opened again to read on. A view closes with `runtime.remote.close_view`, or once it has
//! been unused for a minute (checked at every view call).
use crate::job::{failed, refused, Outcome};
use crate::registry::{lock, random, text};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::hex;
use layerfs_persistence::remote::{RemoteFailureKind, RemoteFile, DEFAULT_CACHE, MAX_REQUEST};
use layerfs_store_service::remote::storage::PinnedFile;
use layerfs_store_service::remote::{open_view as open_remote, OpenedView, ViewFailure};
use layerfs_store_service::StoreService;
use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

/// A view unused this long is closed.
const IDLE: Duration = Duration::from_secs(60);
/// The store path prefix of a view.
pub(crate) const PREFIX: &str = "remote:";
/// The calls a view answers: the tree, sizes and history, which read no
/// file bytes.
const READS: &[&str] = &[
    "project.list_branches",
    "branch.list_commits",
    "fs.list",
    "fs.stat",
    "fs.glob",
    "backend.store.project.get_history",
    "backend.store.project.get_storage",
    "backend.store.project.list_overwritten",
    "backend.store.branch.list_commit_storage",
    "backend.store.fs.get_storage",
    "backend.store.fs.list_revisions",
];

/// One open view.
pub(crate) struct View {
    pub(crate) service: Arc<StoreService>,
    file: Arc<RemoteFile>,
    pinned: Arc<PinnedFile>,
    used: Mutex<Instant>,
}

static VIEWS: LazyLock<Mutex<BTreeMap<String, Arc<View>>>> = LazyLock::new(Default::default);

/// Whether `path` is a view's store path.
pub(crate) fn is_view(path: &str) -> bool {
    path.starts_with(PREFIX)
}

/// Closes every view unused for [`IDLE`]; a call still running on one
/// keeps its service until it returns.
fn sweep() -> Result<(), SdkRefusal> {
    let now = Instant::now();
    lock(&VIEWS)?.retain(|_, view| {
        let used = *view.used.lock().unwrap_or_else(|p| p.into_inner());
        now.duration_since(used) < IDLE
    });
    Ok(())
}

/// The open view whose store path or id the call names, marked used.
fn named(id: &str) -> Result<Arc<View>, SdkRefusal> {
    sweep()?;
    let id = id.rsplit('#').next().unwrap_or(id);
    let view = lock(&VIEWS)?.get(id).cloned().ok_or_else(|| {
        refused(
            "NoSuchView",
            "no such view: it was closed or unused for a minute; open it again",
        )
    })?;
    *view.used.lock().unwrap_or_else(|p| p.into_inner()) = Instant::now();
    Ok(view)
}

/// The service of the view at store path `path`, for `storeOpen`.
pub(crate) fn service(path: &str) -> Result<Arc<StoreService>, SdkRefusal> {
    Ok(named(path)?.service.clone())
}

/// The reads a view refuses because they read file bytes.
const CONTENTS: &[&str] = &[
    "fs.read",
    "fs.grep",
    "fs.blame",
    "fs.diff_files",
    "branch.diff_commits",
    "branch.diff_branches",
];

/// Refuses every call but a tree, size or history read on a view's store
/// path, before it runs.
pub(crate) fn guard(op: &str, args: &SdkValue) -> Result<(), SdkRefusal> {
    match args.get_text("storePath") {
        Some(path) if is_view(path) && CONTENTS.contains(&op) => Err(refused(
            "Unsupported",
            format!("{op}: a view of a remote file reads its tree, sizes and history, not file contents; load the file to read them"),
        )),
        Some(path) if is_view(path) && !READS.contains(&op) => Err(refused(
            "ReadOnly",
            format!("{op}: a view of a remote file is read-only; load the file to change it"),
        )),
        _ => Ok(()),
    }
}

/// Runs `call` on the view at store path `path`; a failure a remote read
/// caused, during this call or before it (its session is then
/// quarantined), is answered with that read's kind and message.
pub(crate) fn answered(path: &str, call: impl FnOnce() -> Outcome) -> Outcome {
    let view = named(path)?;
    let outcome = call();
    let Err(refusal) = outcome else {
        return outcome;
    };
    match view.file.failure_since(0) {
        Some(failure) => Err(refused(code(failure.kind), failure.message)),
        None => Err(refusal),
    }
}

fn code(kind: RemoteFailureKind) -> &'static str {
    match kind {
        RemoteFailureKind::Unavailable => "Unavailable",
        RemoteFailureKind::Denied => "Denied",
        RemoteFailureKind::Changed => "Changed",
        RemoteFailureKind::Unknown => "Unknown",
        RemoteFailureKind::Failed => "Failed",
    }
}

/// The cache bound: `LAYERFS_REMOTE_VIEW_CACHE` bytes, at least one
/// request's worth; 64 MiB when unset.
fn cache_bytes() -> Result<u64, SdkRefusal> {
    match std::env::var("LAYERFS_REMOTE_VIEW_CACHE") {
        Err(_) => Ok(DEFAULT_CACHE),
        Ok(text) => text
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|bytes| *bytes >= MAX_REQUEST)
            .ok_or_else(|| {
                refused(
                    "InvalidInput",
                    format!("LAYERFS_REMOTE_VIEW_CACHE is a byte count of at least {MAX_REQUEST}"),
                )
            }),
    }
}

fn opt(value: Option<&str>) -> SdkValue {
    value.map_or(SdkValue::Null, SdkValue::text)
}

/// `runtime.remote.open_view { home, remote, file, credentials? }`: `{ view,
/// storePath, remote, file, bytes, etag, sha256, heads, modified,
/// projects }`. One stat of the file, then the reads that open its Store.
pub(crate) fn open(args: &SdkValue) -> Outcome {
    sweep()?;
    let file = text(args, "file")?;
    let object = layerfs_store_service::remote::store_object(file)
        .map_err(|why| refused("InvalidInput", why))?;
    let cache = cache_bytes()?;
    let (remote, driver) = crate::sync::driver(args)?;
    let opened =
        open_remote(driver, &object, cache, random()?).map_err(|failure| match failure {
            ViewFailure::Remote(error) => match error {
                layerfs_store_service::remote::storage::RemoteError::Missing => refused(
                    "NoSuchFile",
                    format!("no Store file {file} on remote {remote}"),
                ),
                error => crate::copies::remote_refusal(&error),
            },
            ViewFailure::Read(read) => refused(code(read.kind), read.message),
            ViewFailure::Invalid(why) => refused("InvalidInput", why),
        })?;
    let OpenedView {
        service,
        file: remote_file,
        pinned,
        ..
    } = opened;
    let service = Arc::new(service);
    let projects = service
        .store
        .published(layerfs_project::store::projects)
        .map_err(|error| match remote_file.failure_since(0) {
            Some(failure) => refused(code(failure.kind), failure.message),
            None => failed(error),
        })?;
    let id = hex(&random()?[..12]);
    let path = format!("{PREFIX}{remote}/{file}#{id}");
    let stat = pinned.stat().clone();
    let answer = SdkValue::map([
        ("view", SdkValue::text(&id)),
        ("storePath", SdkValue::text(&path)),
        ("remote", SdkValue::text(&remote)),
        ("file", SdkValue::text(file)),
        ("bytes", SdkValue::Int(pinned.bytes() as i64)),
        ("etag", opt(stat.etag.as_deref())),
        (
            "sha256",
            opt(stat
                .metadata
                .get(layerfs_store_service::remote::storage::pieces::SHA256)
                .map(String::as_str)),
        ),
        (
            "heads",
            opt(stat
                .metadata
                .get(layerfs_store_service::remote::HEADS)
                .map(String::as_str)),
        ),
        (
            "modified",
            stat.modified
                .map_or(SdkValue::Null, |ms| SdkValue::Int(ms as i64)),
        ),
        (
            "projects",
            SdkValue::List(
                projects
                    .iter()
                    .map(|stack| SdkValue::text(stack.name.as_str()))
                    .collect(),
            ),
        ),
    ]);
    lock(&VIEWS)?.insert(
        id,
        Arc::new(View {
            service,
            file: remote_file,
            pinned,
            used: Mutex::new(Instant::now()),
        }),
    );
    Ok(answer)
}

/// `runtime.remote.close_view { view }`: `{ closed }`, false when it was
/// already closed. Its cache goes with its last connection.
pub(crate) fn close(args: &SdkValue) -> Outcome {
    let id = text(args, "view")?;
    let closed = lock(&VIEWS)?.remove(id).is_some();
    sweep()?;
    Ok(SdkValue::map([("closed", SdkValue::Bool(closed))]))
}

/// `runtime.remote.view_stats { view }`: what the view sent and cached.
pub(crate) fn stats(args: &SdkValue) -> Outcome {
    let view = named(text(args, "view")?)?;
    let cache = view.file.stats();
    let int = |n: u64| SdkValue::Int(n as i64);
    Ok(SdkValue::map([
        ("requests", int(view.pinned.requests())),
        ("bytes", int(view.pinned.fetched())),
        ("fileBytes", int(view.pinned.bytes())),
        ("hits", int(cache.hits)),
        ("misses", int(cache.misses)),
        ("fetches", int(cache.fetches)),
        ("fetched", int(cache.fetched)),
        ("resident", int(cache.resident)),
        ("capacity", int(cache.capacity)),
    ]))
}
