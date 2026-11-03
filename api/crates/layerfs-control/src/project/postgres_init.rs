//! Init into a PostgreSQL Store (#533 step 10b): `store.connect` makes the
//! fixed schema in an empty database, and `project.init` adds a project
//! to it through the same live add as a sqlite Store, in this process.
//! The schema's URL is the call's (`postgresql: { url, schema,
//! synchronousCommit }`) and is never stored.
use crate::common::outcome::{refused, Outcome};
use crate::sandbox::registry::{lock, random, text, INITS};
use crate::store::identity::{self as stores, Identity};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::hex;
use std::sync::Arc;

#[cfg(not(feature = "postgresql"))]
const UNBUILT: &str =
    "this LayerFS build has no PostgreSQL engine; build it with --features postgresql";

/// Makes the schema `args.postgresql` names as an empty Store with
/// `identity`'s binding and cursor key. `Ok(false)` when the schema
/// exists already (nothing written).
pub(crate) fn create_schema(args: &SdkValue, identity: &Identity) -> Result<bool, SdkRefusal> {
    #[cfg(feature = "postgresql")]
    {
        use crate::store::postgres::{schema, selection, setting, url};
        use layerfs_history::HistoryCatalogConfig;
        use layerfs_project::store::{CreateRequest, InitError, ProjectApi};
        use layerfs_storage::port::PersistenceError;
        let selection = selection(url(args)?, &schema(args)?, &setting(args)?);
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
            Err(InitError::Persistence(error)) => Err(crate::store::postgres::no_session(&error)
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
        let _ = (args, identity);
        Err(refused("Unsupported", UNBUILT))
    }
}

/// `{ store, store_record, name, path, timeoutMs, postgresql }`: Inits the
/// folder `path` as the new project `name` of the PostgreSQL Store.
/// Answers `{ layer, root, stack }`; the caller writes the project record.
pub(crate) fn add_project(args: &SdkValue) -> Outcome {
    let store = text(args, "store")?;
    let name = text(args, "name")?;
    let identity = stores::identity(args)?;
    if !matches!(identity.kind, stores::Kind::Postgresql { .. }) {
        return Err(refused("InvalidInput", "the Store is not PostgreSQL"));
    }
    let deadline = crate::project::init::deadline(args)?;
    let counters = Arc::new(layerfs_project::InitCounters::new());
    let key = stores::init_key(args, name);
    lock(&INITS)?.insert(key.clone(), Arc::clone(&counters));
    let added = run(args, &identity, name, deadline, counters);
    if let Ok(mut inits) = INITS.lock() {
        inits.remove(&key);
    }
    let (added, stack) = added?;
    let added = added.map_err(|failure| stores::add_failure(failure.error, None, name, store))?;
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
    args: &SdkValue,
    identity: &Identity,
    name: &str,
    deadline: std::time::Instant,
    counters: Arc<layerfs_project::InitCounters>,
) -> Result<(Added, Vec<u8>), SdkRefusal> {
    #[cfg(feature = "postgresql")]
    {
        use crate::store::postgres::{schema, selection, setting, url};
        use layerfs_history::{BranchId, HistoryName, LayerStackId};
        use layerfs_project::store::{AddRequest, ProjectApi};
        let history = |text: &str| HistoryName::new(text).map_err(crate::common::outcome::failed);
        let stack = LayerStackId::from_authority(crate::project::init::sixteen(random()?));
        let branch = BranchId::from_authority(crate::project::init::sixteen(random()?));
        let request = AddRequest {
            source: std::path::PathBuf::from(text(args, "path")?),
            store: layerfs_persistence::StoreSelection::Postgres(selection(
                url(args)?,
                &schema(args)?,
                &setting(args)?,
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
        let _ = (args, identity, name, deadline, counters, random);
        Err(refused("Unsupported", UNBUILT))
    }
}
