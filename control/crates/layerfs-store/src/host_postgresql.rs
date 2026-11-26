//! The CLI's PostgreSQL file reads through the host worker (#819 P-B).
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
//! session it opens and keeps it in that session's memory alone.
//!
//! Under the Store's lock the worker re-reads the record: a Store that is
//! gone, blocked by an operation, or at another login revision is refused
//! there with nothing run. It takes this call's lease there, as the direct
//! route does, and holds it for the call alone: no lease is held between
//! calls, so `store disconnect`, compaction and upgrade see a warm worker
//! as idle. The call then runs once on a held session
//! ([`crate::postgresql::pool`]).
use crate::host_read::Method;
use crate::lease;
use crate::open_args::OpenArgs;
use crate::record::{self, StoreRecord, StoreRef};
use crate::resolve::{self, Target};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::{ProjectName, StoreName};
use layerfs_home::outcome::{refused, Outcome};
use std::path::Path;

/// The host worker's PostgreSQL read operation.
pub const OP: &str = "store.postgresql_read";
/// The host worker's management operation that retires a Store's held
/// PostgreSQL sessions.
pub const RETIRE: &str = "store.postgresql_retire";

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
    let checked = Method::of(method)?;
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
        Err(error) => return Some(Err(error)),
    };
    paths
        .request_fits(OP, &args)
        .then(|| paths.call_validated(OP, args, |value| checked.valid_answer(value)))
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
    let allowed = Method::of(method).ok_or_else(|| {
        refused(
            "Unsupported",
            "the host worker only serves file read, stat and list",
        )
    })?;
    let text = |name: &str| request.get_text(name).filter(|text| !text.is_empty());
    let store = text("store")
        .ok_or_else(invalid)
        .and_then(StoreName::parse)?;
    let project = text("project")
        .ok_or_else(invalid)
        .and_then(ProjectName::parse)?;
    let revision = text("login_revision").ok_or_else(invalid)?;
    let password = text("password").ok_or_else(invalid)?;
    let Some(SdkValue::Map(fields)) = request.get("fields").filter(|f| allowed.accepts(f)) else {
        return Err(refused(
            "InvalidInput",
            "invalid or unbounded PostgreSQL read arguments",
        ));
    };
    resolve::target(home, &format!("{store}/{project}"))?;
    let (lease, record) = admit(home, &store, revision)?;
    let dial = crate::postgresql::login::dial_sent(&record, password)?;
    let selection = crate::postgresql::connect::selection_at(&record, dial);
    let open = OpenArgs::dialled(home, &store, &record, selection).with_project(&project);
    let mut fields = fields.clone();
    fields.retain(|(name, _)| name != "project");
    fields.push(("project".to_owned(), SdkValue::text(project.as_str())));
    let outcome = crate::postgresql::pool::run(revision, &open, |service| {
        crate::service::dispatch(service, method, SdkValue::Map(fields))
    });
    drop(lease);
    outcome
}

/// This call's lease, taken under the Store's lock with the record read
/// there at login revision `revision`.
fn admit(
    home: &Path,
    store: &str,
    revision: &str,
) -> Result<(lease::Lease, StoreRecord), layerfs_bridge::control::SdkRefusal> {
    let _lock = record::lock(home, store)?;
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
    Ok((lease::claim_unblocked_locked(home, store)?, record))
}

/// Answers the management request that retires Store `store`'s held
/// sessions: `{ closed }`, how many idle ones closed.
pub fn retire_request(home: &Path, request: &SdkValue) -> Outcome {
    let store = request
        .get_text("store")
        .ok_or_else(|| refused("InvalidInput", "a retire names its Store"))
        .and_then(StoreName::parse)?;
    let closed = crate::postgresql::pool::retire_store(home, &store);
    Ok(SdkValue::map([("closed", SdkValue::Int(closed as i64))]))
}

/// After a `store connect`, `disconnect` or upgrade of Store `store`: asks
/// a running host worker of `home` to retire the Store's held sessions. It
/// never starts a worker, and its failure changes nothing: the worker's
/// admission refuses the old login revision regardless.
pub fn retire(home: &Path, store: &str) {
    crate::postgresql::pool::retire_store(home, store);
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if layerfs_machine::in_process().is_some() {
        if let Ok(paths) = layerfs_machine::worker::HostPaths::new(home) {
            let args = SdkValue::map([("store", SdkValue::text(store))]);
            let _ = paths.call_existing(true, RETIRE, args);
        }
    }
}

/// Keeps a held password out of core dumps: no core file, and on Linux a
/// process no other process of its user may read or trace. Once, before
/// the first PostgreSQL request is served.
pub fn harden() {
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
        #[cfg(target_os = "linux")]
        if let Err(error) = nix::sys::prctl::set_dumpable(false) {
            let _ = std::io::Write::write_all(
                &mut std::io::stderr(),
                format!("layerfs host worker: process left dumpable: {error}\n").as_bytes(),
            );
        }
    });
}
