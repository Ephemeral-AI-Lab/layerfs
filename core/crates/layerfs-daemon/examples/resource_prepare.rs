//! Separate host-only preparation for the registered growth cases.
use layerfs_history::{BranchId, HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_sdk::{initialize, InitRequest};
use layerfs_storage::StoragePolicy;
use layerfs_telemetry::timer::Timing;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    assert_eq!(args.len(), 3, "resource_prepare FRESH_DIRECTORY FILES");
    let folder = PathBuf::from(&args[1]);
    let count: usize = args[2].parse().unwrap();
    assert!(matches!(count, 1000 | 10000));
    fs::create_dir(&folder).unwrap();
    let source = folder.join("source");
    fs::create_dir(&source).unwrap();
    for n in 0..count {
        fs::write(
            source.join(format!("entry-{n:06}")),
            format!("complete-native-entry:{n:06}\0"),
        )
        .unwrap();
        let file = fs::File::open(source.join(format!("entry-{n:06}"))).unwrap();
        file.set_permissions(fs::Permissions::from_mode(0o640))
            .unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::new(1_700_000_003, 456_789_123))
            .unwrap();
    }
    let file = fs::metadata(source.join("entry-000000")).unwrap();
    let directory = fs::metadata(&source).unwrap();
    let project = Timing::disabled("growth.prepare", |scope| {
        initialize(
            InitRequest {
                source: source.clone(),
                store: PersistenceConfig::sqlite(folder.join("prepared.sqlite"))
                    .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
                locator: folder.join("installed.sqlite").to_str().unwrap().into(),
                policy: StoragePolicy::frozen_default(),
                catalog: HistoryCatalogConfig {
                    binding_key: b"q1-installed".to_vec(),
                    cursor_key: [37; 32],
                    incarnation: 1,
                },
                stack: LayerStackId::from_authority([38; 16]),
                stack_name: HistoryName::new("complete").unwrap(),
                branch: BranchId::from_authority([39; 16]),
                branch_name: HistoryName::new("main").unwrap(),
                scope_seed: [40; 32],
                deadline: Instant::now() + Duration::from_secs(90),
            },
            scope,
        )
    })
    .0
    .unwrap();
    assert_eq!(project.initialized.entries, count as u64 + 1);
    fs::write(
        folder.join("prepared.manifest"),
        project.manifest.encode().unwrap(),
    )
    .unwrap();
    fs::write(
        folder.join("expected-metadata.txt"),
        format!(
            "{} {} {} {} {} {}\n",
            file.mode() & 0o7777,
            file.mtime(),
            file.mtime_nsec(),
            directory.mode() & 0o7777,
            directory.mtime(),
            directory.mtime_nsec()
        ),
    )
    .unwrap();
    fs::remove_dir_all(source).unwrap();
    println!("GROWTH_PREPARED files={count} entries={} bytes={} sqlite={} source_removed=true single_sealed_file=true",project.initialized.entries,project.store.bytes,project.store.sqlite_version);
}
