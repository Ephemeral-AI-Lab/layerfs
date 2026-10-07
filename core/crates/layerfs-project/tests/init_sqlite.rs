//! Canonical namespace oracle over both explicit embedded Store profiles.
//!
//! The embedded global Store is provided on macOS only. The oracle runs there;
//! every other platform proves the explicit refusal instead of running Init.
mod support;
use layerfs_history::HistoryCatalogConfig;
#[cfg(target_os = "macos")]
use layerfs_persistence::SqliteAcquisitionSchema;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use support::*;
#[cfg(not(target_os = "macos"))]
#[test]
fn an_unsupported_platform_refuses_the_store_before_any_file_exists() {
    use layerfs_storage::port::PersistenceError;
    for selected in DEVELOPMENT_PROFILES {
        let fixture = Fixture::new(0);
        let store = fixture.path.join("store.sqlite");
        let refused = Handles::create(
            PersistenceConfig::sqlite(&store).with_sqlite_profile(selected),
            StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: b"init-sqlite".to_vec(),
                cursor_key: [71; 32],
                incarnation: 1,
            },
        );
        assert!(
            matches!(refused, Err(PersistenceError::BackendUnavailable)),
            "{selected:?} must be refused on this platform"
        );
        assert!(!store.exists(), "a refused Store creates no file");
    }
}
#[cfg(target_os = "macos")]
#[test]
fn selected_profiles_init_100_and_1000_pass_the_full_namespace_oracle() {
    for selected in DEVELOPMENT_PROFILES {
        for count in [100, 1000] {
            let fixture = Fixture::new(count);
            let h = Handles::create(
                PersistenceConfig::sqlite(fixture.path.join("store.sqlite"))
                    .with_sqlite_profile(selected)
                    .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
                StoragePolicy::frozen_default(),
                &HistoryCatalogConfig {
                    binding_key: b"init-sqlite".to_vec(),
                    cursor_key: [71; 32],
                    incarnation: 1,
                },
            )
            .unwrap();
            let storage = storage(h.storage.clone());
            let initialized = fixture.run_with(&storage, &h.history, &h.acquisition);
            fixture.verify_namespace(&storage, &h.history, &initialized);
            // The provider-backed root is the one the memory ports derive.
            let memory = support::storage(std::sync::Arc::new(
                memory_metadata::MemoryMetadata::default(),
            ));
            let reference = fixture.run(&memory, &memory_history::MemoryHistory::default());
            assert_eq!(initialized.root, reference.root);
            assert_eq!(initialized.entries, reference.entries);
            // Nothing of the operation is left: no record, no working row.
            use layerfs_storage::port::acquisition::Acquisition as _;
            assert!(h.acquisition.abandoned(None, 8).unwrap().is_empty());
            let work = initialized.namespace_work;
            assert_eq!(work.backing_rows, 2 * count as u64 + 11);
            println!(
                "DIAGNOSTIC {selected:?}-init-{count} SQL={:?} namespace={:?}",
                h.diagnostics().unwrap(),
                initialized.namespace_work
            );
            assert!(!h.checkpoint().unwrap().busy);
        }
    }
}

#[cfg(target_os = "macos")]
fn attempt(
    fixture: &Fixture,
    handles: &Handles,
) -> Result<layerfs_project::Initialized, layerfs_project::ProjectError> {
    use layerfs_history::{HistoryName, LayerStackId};
    use layerfs_project::{init, InitRequest};
    use std::time::{Duration, Instant};
    let storage = storage(handles.storage.clone());
    layerfs_telemetry::timer::Timing::disabled("project.init", |scope| {
        init(
            &storage,
            &handles.history,
            InitRequest {
                source: &fixture.source,
                acquisition: &handles.acquisition,
                stack: LayerStackId::from_authority([19; 16]),
                name: HistoryName::new("main").unwrap(),
                scope_seed: [29; 32],
                deadline: Instant::now() + Duration::from_secs(60),
            },
            scope,
        )
    })
    .0
}

#[cfg(target_os = "macos")]
fn create(store: &std::path::Path, acquisition: SqliteAcquisitionSchema) -> Handles {
    Handles::create(
        PersistenceConfig::sqlite(store).with_sqlite_acquisition(acquisition),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"init-sqlite".to_vec(),
            cursor_key: [71; 32],
            incarnation: 1,
        },
    )
    .unwrap()
}

/// A Store created without the acquisition tables has no working state for
/// Init, and there is no second acquisition algorithm to fall back to.
#[cfg(target_os = "macos")]
#[test]
fn a_store_without_acquisition_tables_refuses_init() {
    use layerfs_history::{HistoryCatalog, LayerStackId};
    use layerfs_project::ProjectError;
    use layerfs_storage::port::{acquisition::AcquisitionError, PersistenceError};
    let fixture = Fixture::new(20);
    let handles = create(
        &fixture.path.join("store.sqlite"),
        SqliteAcquisitionSchema::Absent,
    );
    let error = attempt(&fixture, &handles).unwrap_err();
    assert!(
        matches!(
            error,
            ProjectError::Acquisition(AcquisitionError::Persistence(
                PersistenceError::BackendUnavailable
            ))
        ),
        "{error:?}"
    );
    let stack = LayerStackId::from_authority([19; 16]);
    assert!(handles.history.layer_stack(stack).unwrap().is_none());
}

/// The Store's own directory is acquisition backing: a Store inside the source
/// would be read while it is written and acquired as part of the root.
#[cfg(target_os = "macos")]
#[test]
fn a_store_inside_the_source_is_refused_before_an_operation_begins() {
    use layerfs_project::ProjectError;
    use layerfs_storage::port::acquisition::Acquisition as _;
    let fixture = Fixture::new(5);
    let inside = fixture.source.join("d0/state");
    std::fs::create_dir(&inside).unwrap();
    let handles = create(
        &inside.join("store.sqlite"),
        SqliteAcquisitionSchema::Tables,
    );
    let error = attempt(&fixture, &handles).unwrap_err();
    assert!(
        matches!(error, ProjectError::BackingInsideSource),
        "{error:?}"
    );
    // No operation was begun, so a later session finds nothing abandoned and
    // this session's first operation is the Store's first.
    let owner = handles
        .acquisition
        .begin(&layerfs_storage::port::acquisition::Begin {
            source_device: 1,
            source_inode: 1,
            stack: [0; 16],
            scope: layerfs_content::ObjectId::for_bytes(b"scope"),
        })
        .unwrap();
    assert_eq!((owner.operation, owner.epoch), (1, 1));
}

/// Owner direction 2026-10-07: Disposable is the sole active development
/// verification profile. Durable execution of these bodies is NOT_RUN —
/// deferred by owner for Disposable-only development. Durable support and its
/// retained historical receipts are unchanged.
const DEVELOPMENT_PROFILES: [SqlitePersistenceProfile; 1] = [SqlitePersistenceProfile::Disposable];
