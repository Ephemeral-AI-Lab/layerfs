//! Invalid native topology must refuse before spawning or touching another instance.
#![cfg(target_os = "macos")]
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct ProjectFixture {
    directory: PathBuf,
    project: layerfs_api::SealedProject,
}
impl ProjectFixture {
    fn new() -> Self {
        use layerfs_history::{BranchId, HistoryCatalogConfig, HistoryName, LayerStackId};
        use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
        let directory = std::env::temp_dir().join(format!(
            "sdk-existing-project-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let source = directory.join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("payload"), b"original complete project").unwrap();
        let request = layerfs_api::InitRequest {
            source,
            store: PersistenceConfig::sqlite(directory.join("store.sqlite"))
                .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
            locator: "/layerfs-store/global/store.sqlite".into(),
            policy: layerfs_storage::StoragePolicy::frozen_default(),
            catalog: HistoryCatalogConfig {
                binding_key: b"sdk-existing-project".to_vec(),
                incarnation: 1,
                cursor_key: [7; 32],
            },
            stack: LayerStackId::from_authority([3; 16]),
            stack_name: HistoryName::new("project").unwrap(),
            branch: BranchId::from_authority([4; 16]),
            branch_name: HistoryName::new("main").unwrap(),
            scope_seed: [5; 32],
            deadline: Instant::now() + Duration::from_secs(10),
        };
        let project = layerfs_telemetry::timer::Timing::disabled("sdk.existing.project", |timer| {
            layerfs_api::ProjectApi::new().init(request, timer)
        })
        .0
        .unwrap();
        Self { directory, project }
    }
}
impl Drop for ProjectFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn native_resource_paths_are_validated_before_launch() {
    use layerfs_api::{sandbox::ProviderCreate, SandboxApi, SandboxPhase};
    use layerfs_bridge::{
        daemon_setup::{DaemonLimits, DaemonSetup},
        duplex,
    };
    use layerfs_sandbox::{
        backend::native::NativeMacosProvider,
        provider::{LaunchRequest, ProviderLaunchOptions},
        CommandIdentity, Runtime,
    };
    use std::{
        os::unix::fs::{MetadataExt, PermissionsExt},
        sync::Arc,
    };
    let fixture = ProjectFixture::new();
    let root = fixture.directory.join("runtime");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    let provider = NativeMacosProvider::new(&root).unwrap();
    let paths = provider.paths("one").unwrap();
    let metadata = fs::metadata(&root).unwrap();
    let identity = CommandIdentity {
        uid: metadata.uid(),
        gid: metadata.gid(),
    };
    let setup = DaemonSetup {
        listen: "127.0.0.1:0".into(),
        private_key: [32; 32],
        control_peer: duplex::public_key(&[31; 32]).unwrap(),
        store: fixture.project.store.path.display().to_string(),
        overlay: paths.overlay.display().to_string(),
        mounts: paths.mounts.display().to_string(),
        command_uid: identity.uid,
        command_gid: identity.gid,
        existing_store: None,
        limits: DaemonLimits {
            connections: 2,
            handshake_ms: 1000,
            cache_bytes: 65536,
            namespaces: 2,
            pager_kib: 1024,
            serial_low_water: 0,
            idle_exit_s: 0,
        },
    };
    let request = ProviderCreate {
        setup,
        controller_private: [31; 32],
        daemon_peer: duplex::public_key(&[32; 32]).unwrap(),
        launch: LaunchRequest {
            name: "one".into(),
            daemon_executable: "/bin/false".into(),
            daemon_config: vec![],
            labels: vec![],
            startup_timeout: Duration::from_secs(1),
            options: ProviderLaunchOptions::Local {
                command_identity: identity,
                environment: vec![],
            },
        },
    };
    let api = SandboxApi::with_runtime(Arc::new(Runtime::MacosNative(provider)));
    for field in ["overlay", "mounts", "store", "listen"] {
        let mut invalid = request.clone();
        match field {
            "overlay" => {
                invalid.setup.overlay = root.join("other/overlay.sqlite").display().to_string()
            }
            "mounts" => invalid.setup.mounts = root.join("other/workspaces").display().to_string(),
            "store" => {
                invalid.setup.store = paths.directory.join("store.sqlite").display().to_string()
            }
            _ => invalid.setup.listen = "0.0.0.0:30421".into(),
        }
        let failure = api
            .create_provider_file(invalid, &fixture.project.store, &fixture.project.manifest)
            .err()
            .unwrap();
        assert_eq!(failure.phase, SandboxPhase::Validate);
        assert!(failure.runtime.is_none() && failure.control.is_none());
        assert!(!paths.directory.exists());
    }
}
