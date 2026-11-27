//! Observational PostgreSQL Store engine entry points, without leases or
//! writable opens; credentials are resolved only here.
use crate::admin::record::{identity::Identity, store::StoreRecord};
use layerfs_history::LayerStackId;
use layerfs_home::reply::outcome::refused;
use layerfs_persistence::diagnostic::{Observation, Options, Outcome, ProjectCoverage};
use layerfs_protocol::sdk::SdkRefusal;

/// Diagnoses a registered PostgreSQL Store, resolving credentials only here.
pub fn postgresql(
    record: &StoreRecord,
    identity: &Identity,
    options: Options,
    connection_only: bool,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), SdkRefusal> {
    let selected = crate::backend::postgresql::connect::selection(record)?;
    let selection = crate::backend::postgresql::open::selection(
        selected.url()?,
        &selected.schema()?,
        &selected.setting()?,
    );
    if connection_only {
        return layerfs_persistence::diagnostic::postgresql_connection(
            &selection,
            &identity.binding,
            options,
            &mut layerfs_os::network::resolve_all_within,
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
        &mut layerfs_os::network::resolve_all_within,
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
            &mut layerfs_os::network::resolve_all_within,
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
    let selected = crate::backend::postgresql::connect::selection(record)?;
    let selection = crate::backend::postgresql::open::selection(
        selected.url()?,
        &selected.schema()?,
        &selected.setting()?,
    );
    // the admission's own steps are the Store's checks, not the project's
    open_admitted(&selection, identity, options, &mut |_| {})?
        .diagnose_project(stack, options, observer, progress)
        .map_err(|_| refused("Unavailable", "the project's data was not read to the end; credentials and driver error text omitted"))
}
