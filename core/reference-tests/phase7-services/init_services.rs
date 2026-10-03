mod support;
use layerfs_history::HistoryCatalogConfig;
use layerfs_metadata::{PgHistory, PgMetadata};
use layerfs_s3::{S3Config, S3Objects};
use std::sync::Arc;
use support::*;
#[test]
fn init_100_and_1000_over_owned_services_pass_the_full_namespace_oracle() {
    for count in [100, 1000] {
        let fixture = Fixture::new(count);
        let config = fixture.service_config();
        let metadata = Arc::new(
            PgMetadata::create(
                config.clone(),
                layerfs_storage::StoragePolicy::frozen_default(),
            )
            .unwrap(),
        );
        let history = PgHistory::create(
            config,
            &HistoryCatalogConfig {
                binding_key: b"project-init".to_vec(),
                incarnation: 1,
                cursor_key: [57; 32],
            },
        )
        .unwrap();
        let mut config = S3Config::from_env().unwrap();
        config.prefix = format!(
            "{}/{}",
            config.prefix,
            fixture.path.file_name().unwrap().to_string_lossy()
        );
        let objects = Arc::new(S3Objects::connect_parallel(config).unwrap());
        assert_eq!(objects.diagnostics().unwrap().connections, 4);
        let storage = storage(metadata.clone(), objects.clone());
        let initialized = fixture.run(&storage, &history);
        println!(
            "DIAGNOSTIC init-before-verifier-{count} pg={:?} s3={:?}",
            metadata.diagnostics().unwrap(),
            objects.diagnostics().unwrap()
        );
        fixture.verify_namespace(&storage, &history, &initialized);
        println!(
            "DIAGNOSTIC init-services-{count} storage={:?} pg={:?} s3={:?}",
            initialized.diagnostics,
            metadata.diagnostics().unwrap(),
            objects.diagnostics().unwrap()
        );
    }
}
