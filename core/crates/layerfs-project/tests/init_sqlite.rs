//! Canonical namespace oracle over both explicit embedded Store profiles.
//!
//! The embedded global Store is provided on macOS only. The oracle runs there;
//! every other platform proves the explicit refusal instead of running Init.
mod support;
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use support::*;
#[cfg(not(target_os = "macos"))]
#[test]
fn an_unsupported_platform_refuses_the_store_before_any_file_exists() {
    use layerfs_storage::port::PersistenceError;
    for selected in [
        SqlitePersistenceProfile::Durable,
        SqlitePersistenceProfile::Disposable,
    ] {
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
    for selected in [
        SqlitePersistenceProfile::Durable,
        SqlitePersistenceProfile::Disposable,
    ] {
        for count in [100, 1000] {
            let fixture = Fixture::new(count);
            let h = Handles::create(
                PersistenceConfig::sqlite(fixture.path.join("store.sqlite"))
                    .with_sqlite_profile(selected),
                StoragePolicy::frozen_default(),
                &HistoryCatalogConfig {
                    binding_key: b"init-sqlite".to_vec(),
                    cursor_key: [71; 32],
                    incarnation: 1,
                },
            )
            .unwrap();
            let storage = storage(h.storage.clone());
            let initialized = fixture.run(&storage, &h.history);
            fixture.verify_namespace(&storage, &h.history, &initialized);
            println!(
                "DIAGNOSTIC {selected:?}-init-{count} SQL={:?} namespace={:?}",
                h.diagnostics().unwrap(),
                initialized.namespace_work
            );
            assert!(!h.checkpoint().unwrap().busy);
        }
    }
}
