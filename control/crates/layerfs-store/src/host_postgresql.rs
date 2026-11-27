//! The CLI's PostgreSQL file reads, and the worker's side of its
//! PostgreSQL file writes ([`serve_write`]), through the host worker
//! (#819 P-B).
//!
//! The CLI chooses this route after its argument and scope checks, before
//! taking a lease, for `file.read`, `file.stat` and `file.list` on a
//! PostgreSQL Store whose record holds a login revision, while the host
//! machine is not stopped and the request fits the host envelope; any
//! other call stays on the direct route (P-A), chosen before dialing. The
//! CLI resolves the Store's saved password and sends it, with the record's
//! non-secret login revision, only in the authenticated request. The worker
//! reads no secret itself, from its environment or the home, and writes
//! none: it puts the password into the record's own address for the
//! Store's sessions and drops it when the Store's last session closes
//! ([`crate::postgresql::pool`]).
//!
//! Under the Store's lock the worker re-reads the record: a Store that is
//! gone, blocked by an operation, or at another login revision is refused
//! there with nothing run. It takes this call's lease there, as the direct
//! route does, and holds it for the call alone: no lease is held between
//! calls, so `store disconnect`, compaction and upgrade see a warm worker
//! as idle. The call then runs once on a held session
//! ([`crate::postgresql::pool`]). Only [`READS`] are served by [`serve`],
//! never a method the SQLite host route may add.
//!
//! A write (`store.write` with the same `login_revision` and `password`,
//! routed by [`crate::host_write`]) is admitted the same way and runs once
//! on a held write session: a writable Store service, never a read
//! session.
use crate::host_read::Method;
use crate::lease;
use crate::open_args::OpenArgs;
use crate::record::{self, StoreRecord, StoreRef};
use crate::resolve::{self, Target};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::names::{ProjectName, StoreName};
use layerfs_home::outcome::{refused, Outcome};
use std::path::Path;

/// The host worker's PostgreSQL read operation.
pub const OP: &str = "store.postgresql_read";
/// The host worker's management operation that retires one login revision
/// of a Store's held PostgreSQL sessions.
pub const RETIRE: &str = "store.postgresql_retire";

/// The methods this route serves: its own list, so that a method the
/// SQLite host route adds (a write) never reaches a PostgreSQL read-only
/// session.
const READS: [&str; 3] = ["fs.read", "fs.stat", "fs.list"];

/// Whether read request `args` passes [`serve`]'s checks of the request
/// alone: six fields, a method this route serves, its accepted `fields`
/// and non-empty Store, project, login revision and password texts. The
/// host worker checks it before it hands a read to its PostgreSQL lane,
/// so a held read is small: every field text but `fields`, whose at most
/// eight fields are scalar. [`serve`] refuses whatever fails it, before
/// any session is taken.
pub fn checks(args: &SdkValue) -> bool {
    let text = |name| args.get_text(name).is_some_and(|text| !text.is_empty());
    matches!(args, SdkValue::Map(envelope) if envelope.len() == 6)
        && (args.get_text("method").and_then(read_method))
            .zip(args.get("fields"))
            .is_some_and(|(method, fields)| method.accepts(fields))
        && ["store", "project", "login_revision", "password"]
            .into_iter()
            .all(text)
}

/// `method`'s argument and answer checks when this route serves it.
fn read_method(method: &str) -> Option<Method> {
    READS
        .contains(&method)
        .then(|| Method::of(method))
        .flatten()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn routed(
    home: &Path,
    target: &Target,
    method: &str,
    fields: &[(String, SdkValue)],
) -> Option<Outcome> {
    use layerfs_machine::worker::HostPaths;

    if !crate::host_read::cli() || layerfs_machine::in_process().is_none() {
        return None;
    }
    let StoreRef::Postgres { .. } = &target.store_ref else {
        return None;
    };
    let checked = read_method(method)?;
    let fields = SdkValue::Map(fields.to_vec());
    // a record connected before login revisions stays direct
    let revision = target.record.login_revision()?;
    if !checked.accepts(&fields)
        || layerfs_machine::record::MachineRecord::admit_host(
            home,
            layerfs_home::names::HOST_MACHINE,
        )
        .is_err()
    {
        // a stopped host machine keeps PostgreSQL calls direct (§8.1)
        return None;
    }
    let saved = match crate::postgresql::login::saved(&target.record) {
        Ok(saved) => saved,
        Err(refusal) => return Some(Err(refusal)),
    };
    let args = SdkValue::map([
        ("method", SdkValue::text(method)),
        ("store", SdkValue::text(&target.store)),
        ("project", SdkValue::text(&target.project)),
        ("fields", fields),
        ("login_revision", SdkValue::text(revision)),
        ("password", SdkValue::text(saved.password())),
    ]);
    let paths = match HostPaths::new(home) {
        Ok(paths) => paths,
        Err(error) => {
            layerfs_home::wipe::value(args);
            return Some(Err(error));
        }
    };
    if !paths.request_fits(OP, &args) {
        // it goes direct: the copy of the password it carries goes now
        layerfs_home::wipe::value(args);
        return None;
    }
    Some(paths.call_validated(OP, args, |value| checked.valid_answer(value)))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn routed(
    _home: &Path,
    _target: &Target,
    _method: &str,
    _fields: &[(String, SdkValue)],
) -> Option<Outcome> {
    None
}

/// Runs one authenticated PostgreSQL read request on the host worker.
pub fn serve(home: &Path, request: &SdkValue) -> Outcome {
    harden();
    let invalid = || refused("InvalidInput", "invalid host PostgreSQL read request");
    let SdkValue::Map(envelope) = request else {
        return Err(invalid());
    };
    if envelope.len() != 6 {
        return Err(invalid());
    }
    let method = request.get_text("method").ok_or_else(invalid)?;
    let allowed = read_method(method).ok_or_else(|| {
        refused(
            "Unsupported",
            "the host worker only serves file read, stat and list",
        )
    })?;
    let Some(fields @ SdkValue::Map(_)) = request.get("fields").filter(|f| allowed.accepts(f))
    else {
        return Err(refused(
            "InvalidInput",
            "invalid or unbounded PostgreSQL read arguments",
        ));
    };
    let (store, revision, mut open, fields) = prepared(home, request, fields, &invalid)?;
    let outcome = crate::postgresql::pool::run(home, &store, revision, &open, |service| {
        // the lease only once a session is in hand, the record checked again
        let lease = admit(home, &store, revision)?;
        let outcome = crate::service::dispatch(service, method, fields);
        drop(lease);
        outcome
    });
    wipe_dial(&mut open);
    outcome
}

/// Runs one authenticated PostgreSQL write request (#819 P-B writes) on
/// the host worker, its header already checked
/// ([`crate::host_write::header`]): `method` with `fields`, its content or
/// changes read from `source` when the request streams them. It is
/// admitted as a read is, on a held write session, and runs once.
pub(crate) fn serve_write(
    home: &Path,
    request: &SdkValue,
    (method, fields): (&str, &SdkValue),
    source: Option<&mut dyn layerfs_store_service::methods::Source>,
) -> Outcome {
    harden();
    let invalid = || refused("InvalidInput", "invalid host PostgreSQL write request");
    let (store, revision, mut open, fields) = prepared(home, request, fields, &invalid)?;
    let outcome = crate::postgresql::pool::run_write(home, &store, revision, &open, |service| {
        // the lease only once a session is in hand, the record checked again
        let lease = admit(home, &store, revision)?;
        let outcome = match source {
            Some(source) => crate::service::dispatch_streamed(service, method, fields, source),
            None => crate::service::dispatch(service, method, fields),
        };
        drop(lease);
        outcome
    });
    wipe_dial(&mut open);
    outcome
}

/// What a PostgreSQL request names, checked: its Store, its login
/// revision, the open arguments with the address dialled with the login it
/// sent, and `fields` with its project, once the Store's record admitted
/// it ([`admitted`]).
fn prepared<'a>(
    home: &Path,
    request: &'a SdkValue,
    fields: &SdkValue,
    invalid: &dyn Fn() -> SdkRefusal,
) -> Result<(StoreName, &'a str, OpenArgs, SdkValue), SdkRefusal> {
    let text = |name: &str| request.get_text(name).filter(|text| !text.is_empty());
    let store = text("store")
        .ok_or_else(invalid)
        .and_then(StoreName::parse)?;
    let project = text("project")
        .ok_or_else(invalid)
        .and_then(ProjectName::parse)?;
    let revision = text("login_revision").ok_or_else(invalid)?;
    let password = text("password").ok_or_else(invalid)?;
    let SdkValue::Map(fields) = fields else {
        return Err(invalid());
    };
    resolve::target(home, &format!("{store}/{project}"))?;
    let record = admitted(home, &store, revision)?;
    let dial = crate::postgresql::login::dial_sent(&record, password)?;
    let selection = crate::postgresql::connect::selection_at(&record, dial);
    let open = OpenArgs::dialled(home, &store, &record, selection).with_project(&project);
    let mut fields = fields.clone();
    fields.retain(|(name, _)| name != "project");
    fields.push(("project".to_owned(), SdkValue::text(project.as_str())));
    Ok((store, revision, open, SdkValue::Map(fields)))
}

/// Overwrites the address in `open`, which holds the password.
fn wipe_dial(open: &mut OpenArgs) {
    if let Some(url) = open
        .postgresql
        .as_mut()
        .and_then(|selection| selection.url.take())
    {
        layerfs_home::wipe::text(url);
    }
}

/// The record of Store `store`, read under the Store's lock, when it admits
/// a read at login revision `revision`: present, unblocked, PostgreSQL, at
/// that revision, and the host machine admitted.
fn admitted(
    home: &Path,
    store: &str,
    revision: &str,
) -> Result<StoreRecord, layerfs_bridge::control::SdkRefusal> {
    let _lock = record::lock(home, store)?;
    checked(home, store, revision)
}

fn checked(
    home: &Path,
    store: &str,
    revision: &str,
) -> Result<StoreRecord, layerfs_bridge::control::SdkRefusal> {
    let record = StoreRecord::of(home, store)?;
    record.unblocked()?;
    let StoreRef::Postgres { .. } = record.store_ref()? else {
        return Err(refused(
            "Unsupported",
            format!("Store {store} is not a PostgreSQL Store"),
        ));
    };
    if record.login_revision() != Some(revision) {
        return Err(refused(
            "Conflict",
            format!("Store {store}'s login changed after this call read it; nothing was run"),
        ));
    }
    layerfs_machine::record::MachineRecord::admit_host(home, layerfs_home::names::HOST_MACHINE)?;
    Ok(record)
}

/// This call's lease, taken under the Store's lock with the record checked
/// there again, once its session is in hand: a read waiting for a session
/// or for the server's login holds none, so maintenance is not blocked by
/// it.
fn admit(
    home: &Path,
    store: &str,
    revision: &str,
) -> Result<lease::Lease, layerfs_bridge::control::SdkRefusal> {
    let _lock = record::lock(home, store)?;
    checked(home, store, revision)?;
    lease::claim_unblocked_locked(home, store)
}

/// Overwrites a PostgreSQL read `request`, which carries a password, once
/// its answer is written or it is dropped unrun.
pub fn wipe_request(request: SdkValue) {
    layerfs_home::wipe::value(request);
}

/// Answers the management request that retires login revision `revision`
/// of Store `store`'s held sessions, unless it is the record's current one:
/// `{ closed }`, how many idle sessions closed.
pub fn retire_request(home: &Path, request: &SdkValue) -> Outcome {
    let invalid = || refused("InvalidInput", "a retire names its Store and revision");
    let store = request
        .get_text("store")
        .ok_or_else(invalid)
        .and_then(StoreName::parse)?;
    let revision = (request.get_text("revision"))
        .filter(|revision| !revision.is_empty())
        .ok_or_else(invalid)?;
    let closed = crate::postgresql::pool::retire(home, &store, revision)?;
    Ok(SdkValue::map([("closed", SdkValue::Int(closed as i64))]))
}

/// After a `store connect`, `store upgrade` or `disconnect` replaced or removed login
/// revision `revision` of Store `store`, and with the Store's locks
/// dropped: asks a running host worker of `home` to retire it. It never
/// starts a worker, and its failure changes nothing: a call at the old
/// revision is refused when it is admitted or when it meets the sessions.
pub fn retire(home: &Path, store: &str, revision: Option<&str>) {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if let (Some(revision), Some(_)) = (revision, layerfs_machine::in_process()) {
        if let Ok(paths) = layerfs_machine::worker::HostPaths::new(home) {
            let args = SdkValue::map([
                ("store", SdkValue::text(store)),
                ("revision", SdkValue::text(revision)),
            ]);
            let _ = paths.call_existing(true, RETIRE, args);
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = (home, store, revision);
}

/// No core file from this process: the host worker sets it before it reads
/// its first request, since any request may carry a PostgreSQL password.
pub fn limit_core_dumps() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        #[cfg(unix)]
        if let Err(error) =
            nix::sys::resource::setrlimit(nix::sys::resource::Resource::RLIMIT_CORE, 0, 0)
        {
            let _ = std::io::Write::write_all(
                &mut std::io::stderr(),
                format!("layerfs host worker: core dumps not limited: {error}\n").as_bytes(),
            );
        }
    });
}

/// Keeps a held password out of core dumps and, on Linux, out of reach of
/// the user's other processes: no core file, and a process no other process
/// of its user may read or trace (`PR_SET_DUMPABLE` 0), set once, as soon as
/// a PostgreSQL request is decoded and before any session holds its
/// password. A worker that served no PostgreSQL call stays observable by
/// its user (`/proc/<pid>/environ`), which proves its cleared environment.
/// macOS has no non-dumpable flag: there only the core file limit is set.
pub fn harden() {
    limit_core_dumps();
    #[cfg(target_os = "linux")]
    {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            if let Err(error) = nix::sys::prctl::set_dumpable(false) {
                let _ = std::io::Write::write_all(
                    &mut std::io::stderr(),
                    format!("layerfs host worker: process left dumpable: {error}\n").as_bytes(),
                );
            }
        });
    }
}
