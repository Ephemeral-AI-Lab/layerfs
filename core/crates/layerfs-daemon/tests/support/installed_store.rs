//! Real sealed Store fixture; the copy is an external install fixture, not F5.
use layerfs_history::{
    BranchId, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryName,
    LayerStackId,
};
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePersistenceProfile,
};
use layerfs_project::{init, InitRequest};
use layerfs_storage::{Storage, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub const BINDING: &[u8] = b"daemon-installed-store";
pub const CURSOR: [u8; 32] = [43; 32];
pub struct Fixture {
    pub directory: PathBuf,
    pub config: PersistenceConfig,
    pub branch: BranchId,
    pub count: usize,
}
impl Fixture {
    pub fn new(count: usize, label: &str) -> Self {
        let directory =
            std::env::temp_dir().join(format!("layerfs-installed-{}-{label}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let source = directory.join("source");
        std::fs::create_dir(&source).unwrap();
        for n in 0..count {
            std::fs::write(source.join(format!("file-{n:06}")), bytes(n)).unwrap();
        }
        let sealed = directory.join("sealed.sqlite");
        let config = PersistenceConfig::sqlite(&sealed)
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables);
        let handles = Handles::create(
            config.clone(),
            StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: BINDING.to_vec(),
                cursor_key: CURSOR,
                incarnation: 1,
            },
        )
        .unwrap();
        let storage = Storage::new(handles.storage.clone()).unwrap();
        let initialized = Timing::disabled("installed.init", |timing| {
            init(
                &storage,
                &handles.history,
                InitRequest {
                    source: &source,
                    acquisition: &handles.acquisition,
                    stack: LayerStackId::from_authority([23; 16]),
                    name: HistoryName::new("installed").unwrap(),
                    scope_seed: [31; 32],
                    deadline: Instant::now() + Duration::from_secs(10),
                },
                timing,
            )
        })
        .0
        .unwrap();
        assert_eq!(initialized.entries, count as u64 + 1);
        let branch = BranchId::from_authority([24; 16]);
        handles
            .history
            .fork(&ForkRequest {
                stack: initialized.stack.id,
                branch,
                name: HistoryName::new("main").unwrap(),
                source: ForkSource::Layer(initialized.stack.head_layer),
            })
            .unwrap();
        drop(storage);
        let sealed = handles.seal().unwrap();
        let installed = directory.join("store.sqlite");
        std::fs::copy(sealed.path, &installed).unwrap();
        // No source or sealed original remains available to the consumer.
        std::fs::remove_dir_all(source).unwrap();
        std::fs::remove_file(&config.path).unwrap();
        Self {
            directory,
            config: PersistenceConfig {
                path: installed,
                ..config
            },
            branch,
            count,
        }
    }
    pub fn cleanup(self) {
        std::fs::remove_dir_all(self.directory).unwrap();
    }
}
pub fn bytes(n: usize) -> Vec<u8> {
    format!("complete installed file {n:06}\n").into_bytes()
}
