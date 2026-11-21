//! Observational Store engine entry points, without leases or writable opens.
use crate::{identity::Identity, record::StoreRecord};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::hex;
/// The project id a project audit takes.
pub use layerfs_history::LayerStackId;
use layerfs_home::outcome::refused;
pub use layerfs_persistence::diagnostic::{
    Observation, Options, Outcome, ProjectCoverage, PROJECT_CHECK,
};
/// How long a PostgreSQL connection may take to be made and logged in.
pub use layerfs_persistence::POSTGRESQL_HANDSHAKE;
use std::path::Path;

/// Diagnoses the authoritative same-kernel SQLite file without modifying it.
pub fn sqlite(
    path: &Path,
    identity: &Identity,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), SdkRefusal> {
    layerfs_store_service::kernel_guard::check(path)
        .map_err(|reason| refused("Unsupported", reason.0))?;
    let handles = layerfs_persistence::SqliteHandles::open_diagnostic(
        path,
        &identity.binding,
        identity.cursor_key,
    )
    .map_err(|error| {
        // Schema/authority validation may encounter raw corruption before the
        // ordinary audit begins. Probe the original SQLite file independently;
        // only exact engine evidence can produce a structural failure.
        let _ = layerfs_persistence::diagnostic::sqlite_structure(path, options, observer);
        refused(
            "Unavailable",
            format!(
                "diagnostic read-only Store open failed: {}",
                crate::words::persistence(&error)
            ),
        )
    })?;
    handles.diagnose(options, observer).map_err(|error| {
        let _ = layerfs_persistence::diagnostic::sqlite_structure(path, options, observer);
        refused(
            "Unavailable",
            format!(
                "Store audit did not complete: {}",
                crate::words::persistence(&error)
            ),
        )
    })
}

/// A bounded diagnostic result suitable for forwarding from a worker.
fn mount_rows(rows: Vec<layerfs_persistence::diagnostic::MountHolder>) -> SdkValue {
    let text = |s: Option<String>| s.map_or(SdkValue::Null, SdkValue::text);
    SdkValue::List(
        rows.into_iter()
            .map(|r| {
                SdkValue::map([
                    ("operation", SdkValue::text(hex(&r.operation))),
                    ("scope_id", SdkValue::text(hex(&r.scope))),
                    ("workspace", text(r.workspace)),
                    ("sandbox", text(r.sandbox)),
                    ("machine", text(r.machine)),
                    ("project", text(r.project)),
                    ("branch", text(r.branch)),
                ])
            })
            .collect(),
    )
}

/// Reads mount rows on the kernel that owns the SQLite file.
pub fn sqlite_mounts(
    path: &Path,
    identity: &Identity,
    stack: Option<LayerStackId>,
) -> Result<SdkValue, SdkRefusal> {
    layerfs_store_service::kernel_guard::check(path)
        .map_err(|reason| refused("Unsupported", reason.0))?;
    layerfs_persistence::SqliteHandles::open_diagnostic(
        path,
        &identity.binding,
        identity.cursor_key,
    )
    .and_then(|handles| handles.diagnostic_mounts(stack))
    .map(mount_rows)
    .map_err(open_refusal)
}

/// Reads the selected PostgreSQL schema through an admitted read-only session.
pub fn postgresql_mounts(
    record: &StoreRecord,
    identity: &Identity,
    stack: Option<LayerStackId>,
    options: Options,
) -> Result<SdkValue, SdkRefusal> {
    let selected = crate::postgresql::connect::selection(record)?;
    let selection = crate::postgresql::open::selection(
        selected.url()?,
        &selected.schema()?,
        &selected.setting()?,
    );
    open_admitted(&selection, identity, options, &mut |_| {})?
        .diagnostic_mounts(stack)
        .map(mount_rows)
        .map_err(|_| {
            refused(
                "Unavailable",
                "the Store's mount admissions could not be read",
            )
        })
}

/// Whether the sqlite Store file at `path` records this Store's id, read
/// without opening a diagnostic session.
pub fn sqlite_identity(path: &Path, identity: &Identity) -> Result<bool, SdkRefusal> {
    layerfs_persistence::SqliteHandles::diagnostic_identity(path, &identity.binding)
        .map_err(open_refusal)
}

/// A diagnostic open's refusal: `Busy` when another writer holds the file
/// (it says nothing of its schema or id), else `Unavailable` with its text.
fn open_refusal(error: layerfs_storage::port::PersistenceError) -> SdkRefusal {
    let code = match error {
        layerfs_storage::port::PersistenceError::Busy => "Busy",
        _ => "Unavailable",
    };
    refused(code, crate::words::persistence(&error))
}

/// Diagnoses a registered PostgreSQL Store, resolving credentials only here.
pub fn postgresql(
    record: &StoreRecord,
    identity: &Identity,
    options: Options,
    connection_only: bool,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), SdkRefusal> {
    let selected = crate::postgresql::connect::selection(record)?;
    let selection = crate::postgresql::open::selection(
        selected.url()?,
        &selected.schema()?,
        &selected.setting()?,
    );
    if connection_only {
        return layerfs_persistence::diagnostic::postgresql_connection(
            &selection,
            &identity.binding,
            options,
            &mut layerfs_bridge::network::resolve_all_within,
            observer,
        )
        .map_err(|_| {
            refused(
                "Unavailable",
                "PostgreSQL connection diagnosis did not complete",
            )
        });
    }
    open_admitted(&selection, identity, options, observer)?
        .diagnose(options, observer)
        .map_err(|_| refused("Unavailable", "PostgreSQL Store integrity diagnosis did not complete; credentials and driver error text omitted"))
}

/// The read-only, admitted open of the PostgreSQL Store `selection` names
/// as Store `identity`: one connection and authority preflight, then the
/// pinned open the audits read through.
fn open_admitted(
    selection: &layerfs_persistence::PostgresSelection,
    identity: &Identity,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<layerfs_persistence::PostgresHandles, SdkRefusal> {
    let admission = options
        .deadline
        .unwrap_or_else(|| std::time::Instant::now() + std::time::Duration::from_secs(10));
    let mut identity_confirmed = false;
    let mut admitted = true;
    observer(Observation{id:"store.admission.interval",outcome:Outcome::Pass,message:"independent read-only connection/authority preflight; logical audit uses a subsequent independently validated pinned snapshot".to_owned()});
    layerfs_persistence::diagnostic::postgresql_connection(
        selection,
        &identity.binding,
        Options {
            deadline: Some(admission),
            ..options
        },
        &mut layerfs_bridge::network::resolve_all_within,
        &mut |observation| {
            if observation.id == "store.connection.identity" && observation.outcome == Outcome::Pass
            {
                identity_confirmed = true;
            }
            if observation.id != "store.connection.tls" && observation.outcome != Outcome::Pass {
                admitted = false;
            }
            observer(observation);
        },
    )
    .map_err(|_| {
        refused(
            "Unavailable",
            "PostgreSQL read-only admission did not complete",
        )
    })?;
    if !admitted || !identity_confirmed {
        return Err(refused("Unavailable","PostgreSQL authority/access admission did not pass; logical integrity checks were not started"));
    }
    layerfs_persistence::PostgresHandles::open_diagnostic_admitted(
            selection,
            &identity.binding,
            identity.cursor_key,
            options.deadline,
            admission,
            &mut layerfs_bridge::network::resolve_all_within,
            options.cancelled,
        )
        .map_err(|_| {
            if options.cancelled.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)) {
                observer(Observation { id: "store.admission.cancelled", outcome: Outcome::Unknown, message: "pinned-open connection or schema admission cancelled; logical integrity audit did not start".to_owned() });
            }
            refused(
                "Unavailable",
                "PostgreSQL read-only Store open failed; credentials and driver error text omitted",
            )
        })
}

/// Audits one project's data (`project doctor --full`) in the same-kernel
/// SQLite file at `path`, opened read-only as Store `identity`: problems
/// stream to `observer`, `(done, total)` trees to `progress`.
pub fn sqlite_project(
    path: &Path,
    identity: &Identity,
    stack: LayerStackId,
    options: Options,
    observer: &mut dyn FnMut(Observation),
    progress: &mut dyn FnMut(u64, u64),
) -> Result<ProjectCoverage, SdkRefusal> {
    layerfs_store_service::kernel_guard::check(path)
        .map_err(|reason| refused("Unsupported", reason.0))?;
    let handles = layerfs_persistence::SqliteHandles::open_diagnostic(
        path,
        &identity.binding,
        identity.cursor_key,
    )
    .map_err(|error| {
        refused(
            "Unavailable",
            format!(
                "read-only Store open failed: {}",
                crate::words::persistence(&error)
            ),
        )
    })?;
    handles
        .diagnose_project(stack, options, observer, progress)
        .map_err(|error| {
            refused(
                "Unavailable",
                format!(
                    "the project's data was not read to the end: {}",
                    crate::words::persistence(&error)
                ),
            )
        })
}

/// Audits one project's data (`project doctor --full`) in the PostgreSQL
/// Store `record` names, resolving credentials only here, through the
/// same admitted read-only open as the Store's own audit.
pub fn postgresql_project(
    record: &StoreRecord,
    identity: &Identity,
    stack: LayerStackId,
    options: Options,
    observer: &mut dyn FnMut(Observation),
    progress: &mut dyn FnMut(u64, u64),
) -> Result<ProjectCoverage, SdkRefusal> {
    let selected = crate::postgresql::connect::selection(record)?;
    let selection = crate::postgresql::open::selection(
        selected.url()?,
        &selected.schema()?,
        &selected.setting()?,
    );
    // the admission's own steps are the Store's checks, not the project's
    open_admitted(&selection, identity, options, &mut |_| {})?
        .diagnose_project(stack, options, observer, progress)
        .map_err(|_| refused("Unavailable", "the project's data was not read to the end; credentials and driver error text omitted"))
}
