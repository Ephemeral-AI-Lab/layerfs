//! Init into a PostgreSQL Store (#533 step 10b): `store.connect` makes the
//! fixed schema in an empty database, and `project.init` adds a project
//! to it through the same live add as a sqlite Store, in this process.
//! The schema's URL is the call's ([`PgProjectAdd::postgresql`]) and is
//! never stored.
use crate::identity as stores;
use crate::identity::Identity;
use crate::open_args::PgSelection;
use crate::project::add::ProjectAdd;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::hex;
use layerfs_home::helpers::random;
use layerfs_home::outcome::{refused, Outcome};
use std::path::Path;
use std::sync::Arc;

#[cfg(not(feature = "postgresql"))]
const UNBUILT: &str =
    "this LayerFS build has no PostgreSQL engine; build it with --features postgresql";

/// Makes the schema `postgresql` names as an empty Store with
/// `identity`'s binding and cursor key. `Ok(false)` when the schema
/// exists already (nothing written).
pub fn create_schema(postgresql: &PgSelection, identity: &Identity) -> Result<bool, SdkRefusal> {
    #[cfg(feature = "postgresql")]
    {
        use crate::postgresql::open::selection;
        use layerfs_history::HistoryCatalogConfig;
        use layerfs_project::store::{CreateRequest, InitError, ProjectApi};
        use layerfs_storage::port::PersistenceError;
        let selection = selection(
            postgresql.url()?,
            &postgresql.schema()?,
            &postgresql.setting()?,
        );
        let made = ProjectApi::new().create_postgresql_store(CreateRequest {
            store: layerfs_persistence::StoreSelection::Postgres(selection),
            policy: layerfs_storage::StoragePolicy::frozen_default(),
            catalog: HistoryCatalogConfig {
                binding_key: identity.binding.clone(),
                cursor_key: identity.cursor_key,
                incarnation: 1,
            },
        });
        match made {
            Ok(()) => Ok(true),
            Err(InitError::Persistence(PersistenceError::Refused { status, .. }))
                if status == "schema exists" =>
            {
                Ok(false)
            }
            Err(InitError::Persistence(error)) => Err(crate::postgresql::open::no_session(&error)
                .unwrap_or_else(|| {
                    refused("Failed", format!("making the PostgreSQL Store: {error:?}"))
                })),
            Err(error) => Err(refused(
                "Failed",
                format!("making the PostgreSQL Store: {error:?}"),
            )),
        }
    }
    #[cfg(not(feature = "postgresql"))]
    {
        let _ = (postgresql, identity);
        Err(refused("Unsupported", UNBUILT))
    }
}

/// `project.init`'s add into a PostgreSQL Store: the add (with the Store's
/// record) and the server it dials. No `Debug`: the URL holds the password.
pub struct PgProjectAdd {
    /// The add.
    pub add: ProjectAdd,
    /// The Store's server.
    pub postgresql: PgSelection,
}

/// Inits the folder as the new project of the PostgreSQL Store. Answers
/// `{ layer, root, stack }`; the caller writes the project record.
pub fn add_project(request: &PgProjectAdd) -> Outcome {
    let add = &request.add;
    let (store, name) = (add.store.as_str(), add.name.as_str());
    let identity = stores::identity(&add.open())?;
    if !matches!(identity.kind, stores::Kind::Postgresql { .. }) {
        return Err(refused("InvalidInput", "the Store is not PostgreSQL"));
    }
    let deadline = crate::project::init::deadline(add.timeout_ms)?;
    let counters = Arc::new(layerfs_project::InitCounters::new());
    let postgresql = &request.postgresql;
    let source = add.source.as_path();
    let (added, stack) = run(source, postgresql, &identity, name, deadline, counters)?;
    let added = added
        .map_err(|failure| crate::project::add::add_failure(failure.error, None, name, store))?;
    Ok(SdkValue::map([
        (
            "layer",
            SdkValue::text(hex(added.branch.branch.base_layer.as_slice())),
        ),
        (
            "root",
            SdkValue::text(hex(added.branch.effective_root.as_bytes())),
        ),
        ("stack", SdkValue::text(hex(&stack))),
    ]))
}

type Added = Result<layerfs_project::store::AddedProject, Box<layerfs_project::store::AddFailure>>;

fn run(
    source: &Path,
    postgresql: &PgSelection,
    identity: &Identity,
    name: &str,
    deadline: std::time::Instant,
    counters: Arc<layerfs_project::InitCounters>,
) -> Result<(Added, Vec<u8>), SdkRefusal> {
    #[cfg(feature = "postgresql")]
    {
        use crate::postgresql::open::selection;
        use layerfs_history::{BranchId, HistoryName, LayerStackId};
        use layerfs_project::store::{AddRequest, ProjectApi};
        let history = |text: &str| HistoryName::new(text).map_err(layerfs_home::outcome::failed);
        let stack = LayerStackId::from_authority(crate::project::init::sixteen(random()?));
        let branch = BranchId::from_authority(crate::project::init::sixteen(random()?));
        let request = AddRequest {
            source: source.to_path_buf(),
            store: layerfs_persistence::StoreSelection::Postgres(selection(
                postgresql.url()?,
                &postgresql.schema()?,
                &postgresql.setting()?,
            )),
            binding: identity.binding.clone(),
            cursor_key: identity.cursor_key,
            stack,
            stack_name: history(name)?,
            branch,
            branch_name: history("main")?,
            scope_seed: random()?,
            deadline,
        };
        let (added, _) = layerfs_telemetry::timer::Timing::disabled("store.add_project", |timer| {
            ProjectApi::new().add_project(request, timer, counters)
        });
        Ok((added, stack.to_bytes().to_vec()))
    }
    #[cfg(not(feature = "postgresql"))]
    {
        let _ = (
            source, postgresql, identity, name, deadline, counters, random,
        );
        Err(refused("Unsupported", UNBUILT))
    }
}
