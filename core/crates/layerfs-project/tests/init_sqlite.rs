//! Canonical namespace oracle over both explicit embedded Store profiles.
mod support;
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use support::*;
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
