//! Observational sqlite Store engine entry points, without leases or
//! writable opens (PostgreSQL's are in `backend/postgresql/diagnostic.rs`).
use crate::admin::record::identity::Identity;
/// The project id a project audit takes.
pub use layerfs_history::LayerStackId;
use layerfs_home::outcome::refused;
pub use layerfs_persistence::diagnostic::{
    ConnectionStep, Observation, Options, Outcome, ProjectCoverage, PROJECT_CHECK,
};
/// How long a PostgreSQL connection may take to be made and logged in.
pub use layerfs_persistence::POSTGRESQL_HANDSHAKE;
use layerfs_protocol::sdk::hex;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
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

/// The shallow cut of a sqlite Store (call surface 2.0, C14): its file at
/// `path` opened read-only as Store `identity`, its id and schema checked
/// against this build's, and nothing else read. A host that cannot open a
/// sqlite Store is `Unsupported`.
pub fn sqlite_schema(path: &Path, identity: &Identity) -> Result<(), SdkRefusal> {
    layerfs_store_service::kernel_guard::check(path)
        .map_err(|reason| refused("Unsupported", reason.0))?;
    layerfs_persistence::SqliteHandles::open_diagnostic(
        path,
        &identity.binding,
        identity.cursor_key,
    )
    .map(drop)
    .map_err(open_refusal)
}

/// Whether the sqlite Store file at `path` records this Store's id, read
/// without the schema checks [`sqlite_schema`] adds.
pub fn sqlite_identity(path: &Path, identity: &Identity) -> Result<bool, SdkRefusal> {
    layerfs_persistence::SqliteHandles::diagnostic_identity(path, &identity.binding)
        .map_err(open_refusal)
}

/// A diagnostic open's refusal: `Busy` when another writer holds the file
/// (it says nothing of its schema or id), [`OLD_STORE`] with the version
/// refusal for a Store of an older schema version, else `Unavailable` with
/// its text.
///
/// [`OLD_STORE`]: crate::state::OLD_STORE
fn open_refusal(error: layerfs_storage::port::PersistenceError) -> SdkRefusal {
    use layerfs_storage::port::PersistenceError;
    let code = match &error {
        PersistenceError::Busy => "Busy",
        PersistenceError::Refused { status } => match layerfs_persistence::outdated_store(status) {
            Some(known) if known == status => {
                return refused(crate::admin::status::state::OLD_STORE, known)
            }
            _ => "Unavailable",
        },
        _ => "Unavailable",
    };
    refused(code, crate::words::persistence(&error))
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
                    ("owner_runtime", text(r.owner_runtime)),
                    ("owner_boot", text(r.owner_boot)),
                    ("machine_incarnation", text(r.machine_incarnation)),
                    ("sandbox_generation", text(r.sandbox_generation)),
                    (
                        "state",
                        SdkValue::text(if r.uncertain { "uncertain" } else { "live" }),
                    ),
                ])
            })
            .collect(),
    )
}

/// One bounded page of retained `done` operator-audit rows of the SQLite
/// Store at `path`, read on a diagnostic open. It changes no row and never
/// releases an admission; the live holder view filters these rows out.
pub fn done_audit(
    path: &Path,
    identity: &Identity,
    page: u32,
    limit: u32,
) -> Result<SdkValue, SdkRefusal> {
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
            format!("diagnostic read-only Store open failed: {error}"),
        )
    })?;
    let audit = handles
        .diagnostic_done_audit(page, limit)
        .map_err(|error| {
            refused(
                "Unavailable",
                format!("the retained done audit could not be read: {error}"),
            )
        })?;
    let text = |s: Option<String>| s.map_or(SdkValue::Null, SdkValue::text);
    Ok(SdkValue::map([
        (
            "items",
            SdkValue::List(
                audit
                    .items
                    .into_iter()
                    .map(|row| {
                        SdkValue::map([
                            ("op", SdkValue::text(hex(&row.operation))),
                            ("kind", SdkValue::text(&row.kind)),
                            ("scope_kind", SdkValue::text(&row.scope_kind)),
                            ("scope_id", SdkValue::text(hex(&row.scope))),
                            ("project", text(row.project)),
                            ("branch", text(row.branch)),
                            ("machine", text(row.machine)),
                            ("sandbox", text(row.sandbox)),
                            ("phase", text(row.phase)),
                            ("evidence", text(row.evidence)),
                            ("updated_ms", SdkValue::Int(row.updated_ms)),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("page", SdkValue::Int(i64::from(audit.page))),
        ("limit", SdkValue::Int(i64::from(audit.limit))),
        ("has_more", SdkValue::Bool(audit.has_more)),
    ]))
}

/// Lists the durable mount admissions of the SQLite Store at `path`, read on
/// a diagnostic open. Missing local records are reported by the doctor; this
/// function never releases an admission.
pub fn mount_holders(path: &Path, identity: &Identity) -> Result<SdkValue, SdkRefusal> {
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
            format!("diagnostic read-only Store open failed: {error}"),
        )
    })?;
    handles
        .diagnostic_mounts(None)
        .map(mount_rows)
        .map_err(|error| {
            refused(
                "Unavailable",
                format!("mount admissions could not be read: {error}"),
            )
        })
}
