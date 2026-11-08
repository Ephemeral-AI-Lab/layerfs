//! Observational Store engine entry points, without leases or writable opens.
use crate::{identity::Identity, record::StoreRecord};
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::outcome::refused;
pub use layerfs_persistence::diagnostic::{Observation, Options, Outcome};
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
        refused(
            "Unavailable",
            format!("diagnostic read-only Store open failed: {error}"),
        )
    })?;
    handles.diagnose(options, observer).map_err(|error| {
        refused(
            "Unavailable",
            format!("Store audit did not complete: {error}"),
        )
    })
}

/// Diagnoses a registered PostgreSQL Store, resolving credentials only here.
pub fn postgresql(
    record: &StoreRecord,
    identity: &Identity,
    options: Options,
    connection_only: bool,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), SdkRefusal> {
    #[cfg(feature = "postgresql")]
    {
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
        let handles = layerfs_persistence::PostgresHandles::open_diagnostic(
            &selection,
            &identity.binding,
            identity.cursor_key,
            options.deadline,
            &mut layerfs_bridge::network::resolve_all_within,
        )
        .map_err(|_| {
            refused(
                "Unavailable",
                "PostgreSQL read-only Store open failed; credentials and driver error text omitted",
            )
        })?;
        handles.diagnose(options,observer).map_err(|_|refused("Unavailable","PostgreSQL Store integrity diagnosis did not complete; credentials and driver error text omitted"))
    }
    #[cfg(not(feature = "postgresql"))]
    {
        let _ = (record, identity, options, connection_only, observer);
        Err(refused(
            "Unsupported",
            "this build has no PostgreSQL engine",
        ))
    }
}
