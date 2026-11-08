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
        // Schema/authority validation may encounter raw corruption before the
        // ordinary audit begins. Probe the original SQLite file independently;
        // only exact engine evidence can produce a structural failure.
        let _ = layerfs_persistence::diagnostic::sqlite_structure(path, options, observer);
        refused(
            "Unavailable",
            format!("diagnostic read-only Store open failed: {error}"),
        )
    })?;
    handles.diagnose(options, observer).map_err(|error| {
        let _ = layerfs_persistence::diagnostic::sqlite_structure(path, options, observer);
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
        let admission = options
            .deadline
            .unwrap_or_else(|| std::time::Instant::now() + std::time::Duration::from_secs(10));
        let mut identity_confirmed = false;
        let mut admitted = true;
        observer(Observation{id:"store.admission.interval",outcome:Outcome::Pass,message:"independent read-only connection/authority preflight; logical audit uses a subsequent independently validated pinned snapshot".to_owned()});
        layerfs_persistence::diagnostic::postgresql_connection(
            &selection,
            &identity.binding,
            Options {
                deadline: Some(admission),
                ..options
            },
            &mut layerfs_bridge::network::resolve_all_within,
            &mut |observation| {
                if observation.id == "store.connection.identity"
                    && observation.outcome == Outcome::Pass
                {
                    identity_confirmed = true;
                }
                if observation.id != "store.connection.tls" && observation.outcome != Outcome::Pass
                {
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
        let handles = layerfs_persistence::PostgresHandles::open_diagnostic_admitted(
            &selection,
            &identity.binding,
            identity.cursor_key,
            options.deadline,
            admission,
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
