//! Import installs the sealed Store in its own process (#373): no Sandbox
//! daemon is needed. The installed file opens as every Store opener opens
//! it, a second install into the same place is refused before anything is
//! written, and an already installed manifest is refused.
use layerfs_api::{InitRequest, ProjectApi, SealedProject};
use layerfs_history::{BranchId, HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use layerfs_store_service::install::install_local;
use layerfs_telemetry::timer::Timing;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A scratch directory on the target's filesystem (a local Linux volume in
/// the build container), which carries the Store's locks.
fn scratch(label: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "layerfs-local-install-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join("source")).unwrap();
    std::fs::create_dir_all(directory.join("volume")).unwrap();
    std::fs::write(directory.join("source/file"), b"imported").unwrap();
    directory
}

fn sealed(directory: &Path) -> SealedProject {
    let request = InitRequest {
        source: directory.join("source"),
        store: PersistenceConfig::sqlite(directory.join("sealed.sqlite"))
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        locator: "/layerfs-store/global/store.sqlite".into(),
        policy: StoragePolicy::frozen_default(),
        catalog: HistoryCatalogConfig {
            binding_key: b"local-install".to_vec(),
            cursor_key: [31; 32],
            incarnation: 1,
        },
        stack: LayerStackId::from_authority([41; 16]),
        stack_name: HistoryName::new("project").unwrap(),
        branch: BranchId::from_authority([42; 16]),
        branch_name: HistoryName::new("main").unwrap(),
        scope_seed: [43; 32],
        deadline: Instant::now() + Duration::from_secs(10),
    };
    Timing::disabled("init", |timer| ProjectApi::new().init(request, timer))
        .0
        .unwrap()
}

#[cfg(target_os = "linux")]
fn root() -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self").is_ok_and(|m| m.uid() == 0)
}

#[cfg(target_os = "linux")]
#[test]
fn import_installs_the_store_in_process() {
    use std::os::unix::fs::PermissionsExt;
    let directory = scratch("ok");
    let project = sealed(&directory);
    let destination = directory.join("volume/global/store.sqlite");
    let sealed_bytes = std::fs::read(&project.store.path).unwrap();
    let outcome = install_local(&project.store.path, &project.manifest, &destination);
    if !root() {
        let failure = outcome.unwrap_err();
        assert!(failure.cause.contains("must run as user 0"), "{failure}");
        assert!(failure.temporary.is_none() && !destination.exists());
        return;
    }
    let installed = outcome.unwrap();
    assert!(installed.daemon_sqlite.is_some());
    let mut expected = project.manifest.clone();
    expected.daemon_sqlite = installed.daemon_sqlite.clone();
    assert_eq!(installed, expected);
    // owner-only, as a daemon requires of the Store it opens
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&directory.join("volume/global")), 0o700);
    assert_eq!(mode(&destination), 0o600);
    assert!(!directory
        .join("volume/global/store.sqlite.installing")
        .exists());
    assert_eq!(std::fs::read(&project.store.path).unwrap(), sealed_bytes);

    // every Store opener opens it as it opens a daemon-installed one
    let template = directory.join("overlay.sqlite");
    let service = layerfs_store_service::StoreService::open_direct(
        &destination,
        &project.manifest.binding,
        project.manifest.cursor_key,
        &template,
    )
    .unwrap();
    service.close().unwrap();

    // a second install into the same place is refused before it writes
    let before = std::fs::read(&destination).unwrap();
    let again = install_local(&project.store.path, &project.manifest, &destination).unwrap_err();
    assert!(again.cause.contains("exists"), "{again}");
    assert!(again.temporary.is_none() && !again.published);
    assert_eq!(std::fs::read(&destination).unwrap(), before);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn an_installed_manifest_is_refused_before_anything_is_written() {
    let directory = scratch("installed");
    let project = sealed(&directory);
    let mut manifest = project.manifest.clone();
    manifest.daemon_sqlite = Some(project.manifest.host_sqlite.clone());
    let destination = directory.join("volume/global/store.sqlite");
    let failure = install_local(&project.store.path, &manifest, &destination).unwrap_err();
    assert!(failure.cause.contains("not yet installed"), "{failure}");
    assert!(failure.temporary.is_none() && !directory.join("volume/global").exists());
    std::fs::remove_dir_all(directory).unwrap();
}

#[cfg(not(target_os = "linux"))]
#[test]
fn local_install_is_unsupported_off_linux() {
    let directory = scratch("host");
    let project = sealed(&directory);
    let destination = directory.join("volume/global/store.sqlite");
    let failure = install_local(&project.store.path, &project.manifest, &destination).unwrap_err();
    assert!(failure.cause.contains("Linux"), "{failure}");
    assert!(failure.temporary.is_none() && !directory.join("volume/global").exists());
    std::fs::remove_dir_all(directory).unwrap();
}
