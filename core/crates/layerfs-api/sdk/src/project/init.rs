//! One create → native import → first Branch → consuming seal.
use crate::{InitError, InitFailure, InitRequest, SealedProject};
use layerfs_bridge::provision::{check_destination, ProviderKind, StoreManifest, StoreProfile};
use layerfs_history::{ForkRequest, ForkSource, HistoryCatalog};
use layerfs_persistence::{Handles, SqliteAcquisitionSchema, SqlitePersistenceProfile};
use layerfs_storage::Storage;
use layerfs_telemetry::timer::{Active, TimingScope};

/// Builds and seals one complete Store. It never opens that Store again and
/// returns no data-serving runtime. On failure, output and original known
/// outcomes remain owned by the caller; no file deletion or replay occurs.
pub fn initialize(
    request: InitRequest,
    timer: &TimingScope<'_, Active>,
) -> Result<SealedProject, Box<InitFailure>> {
    let mut initialized = None;
    let mut branch = None;
    let result = (|| {
        if request.store.sqlite_profile != SqlitePersistenceProfile::Disposable {
            return Err(InitError::Manifest(
                "explicit Disposable Store profile required",
            ));
        }
        check_destination(
            &request.locator,
            &request.catalog.binding_key,
            request.catalog.cursor_key,
        )
        .map_err(InitError::Manifest)?;
        let config = request
            .store
            .clone()
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables);
        let handles = Handles::create(config, request.policy, &request.catalog)
            .map_err(InitError::Persistence)?;
        let storage = Storage::new(handles.storage.clone()).map_err(InitError::Storage)?;
        let imported = layerfs_project::init(
            &storage,
            &handles.history,
            layerfs_project::InitRequest {
                source: &request.source,
                acquisition: &handles.acquisition,
                stack: request.stack,
                name: request.stack_name.clone(),
                scope_seed: request.scope_seed,
                deadline: request.deadline,
            },
            timer,
        )
        .map_err(InitError::Project)?;
        let source = ForkSource::Layer(imported.stack.head_layer);
        initialized = Some(imported);
        branch = Some(
            handles
                .history
                .fork(&ForkRequest {
                    stack: request.stack,
                    branch: request.branch,
                    name: request.branch_name.clone(),
                    source,
                })
                .map_err(InitError::History)?,
        );
        drop(storage);
        let sealed = handles.seal().map_err(InitError::Persistence)?;
        Ok(sealed)
    })();
    match result {
        Ok(store) => {
            let manifest = StoreManifest {
                provider: ProviderKind::Sqlite,
                locator: request.locator.clone(),
                profile: match request.store.sqlite_profile {
                    SqlitePersistenceProfile::Durable => StoreProfile::Durable,
                    SqlitePersistenceProfile::Disposable => StoreProfile::Disposable,
                },
                binding: request.catalog.binding_key.clone(),
                cursor_key: request.catalog.cursor_key,
                stack: request.stack.to_bytes(),
                branch: request.branch.to_bytes(),
                root: initialized
                    .as_ref()
                    .expect("acknowledged Init")
                    .root
                    .to_bytes(),
                bytes: store.bytes,
                host_sqlite: store.sqlite_version.clone(),
                daemon_sqlite: None,
            };
            Ok(SealedProject {
                store,
                manifest,
                initialized: initialized.expect("acknowledged Init"),
                branch: branch.expect("acknowledged Branch"),
            })
        }
        Err(error) => Err(Box::new(InitFailure {
            request,
            error,
            initialized,
            branch,
        })),
    }
}
