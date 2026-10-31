//! Positive-path host Init -> protected owned Sandbox -> direct shared Store control proof.
//! Typed failure custody is covered separately by public adapter tests; this example reports failures in stderr.
use layerfs_api::{InitRequest, ProjectApi, ProviderCreate, ProviderSandbox, SandboxApi};
use layerfs_bridge::{
    control::{DaemonPhase, Reply, Request},
    daemon_setup::{DaemonLimits, DaemonSetup},
    duplex as transport,
};
use layerfs_history::{BranchId, HistoryCatalogConfig, HistoryName, LayerStackId, WorkspaceId};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_sandbox::backend::docker::{Docker, DockerProvider};
use layerfs_sandbox::provider::{LaunchRequest, ProviderLaunchOptions};
use layerfs_storage::StoragePolicy;
use layerfs_telemetry::timer::Timing;
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
fn setup(private: [u8; 32], peer: [u8; 32]) -> DaemonSetup {
    DaemonSetup {
        listen: "0.0.0.0:30421".into(),
        private_key: private,
        control_peer: peer,
        store: "/layerfs-store/global/store.sqlite".into(),
        overlay: "/layerfs-local/overlay/overlay.sqlite".into(),
        mounts: "/workspaces".into(),
        command_uid: 501,
        command_gid: 20,
        limits: DaemonLimits {
            connections: 4,
            handshake_ms: 2000,
            cache_bytes: 65536,
            namespaces: 8,
            pager_kib: 1024,
            serial_low_water: 256,
            idle_exit_s: 0,
        },
        existing_store: None,
        postgresql: None,
    }
}
fn ordinary(owner: &ProviderSandbox) -> Result<(), Box<dyn std::error::Error>> {
    let command = "set -e; test \"$(id -u)\" = 501; test \"$(id -g)\" = 20; test \"$(id -G)\" = 20; grep -q '^CapEff:[[:space:]]*0000000000000000$' /proc/self/status; grep -q '^CapAmb:[[:space:]]*0000000000000000$' /proc/self/status; grep -q '^NoNewPrivs:[[:space:]]*1$' /proc/self/status; for p in /layerfs-local/config/daemon.setup /layerfs-local/overlay/overlay.sqlite /layerfs-store/global/store.sqlite /proc/1/root/layerfs-local/config/daemon.setup /proc/1/root/layerfs-store/global/store.sqlite /proc/1/environ /proc/1/fd; do test ! -r \"$p\"; done; test ! -w /dev/fuse; test ! -w /sys/fs/fuse/connections; printf 'ORDINARY_ACCESS uid=501 gid=20 groups=20 CapEff=0 CapAmb=0 NoNewPrivs=1 private_backing_denied proc_aliases_denied fuse_control_denied\\n'";
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    let exit = owner
        .runtime
        .run(
            &layerfs_sandbox::ExecRequest {
                identity: layerfs_sandbox::CommandIdentity { uid: 501, gid: 20 },
                arguments: vec!["bash".into(), "-c".into(), command.into()],
                environment: vec![],
                directory: "/".into(),
                stdin: false,
            },
            &mut stdout,
            &mut stderr,
        )
        .map_err(|f| format!("run {f:?}"))?;
    println!("ORDINARY_RESULT {:?}", exit);
    assert_eq!(exit.code, 0);
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(
        args.len(),
        6,
        "socket image volume executable fresh-host-directory"
    );
    let directory = PathBuf::from(&args[5]);
    fs::create_dir(&directory)?;
    fs::create_dir(directory.join("source"))?;
    fs::create_dir(directory.join("source/.git"))?;
    fs::write(directory.join("source/.git/index"), b"complete index")?;
    fs::write(directory.join("source/file"), vec![0x7a; 65536])?;
    let branch = BranchId::from_authority([32; 16]);
    let request = InitRequest {
        source: directory.join("source"),
        store: PersistenceConfig::sqlite(directory.join("sealed.sqlite"))
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        locator: "/layerfs-store/global/store.sqlite".into(),
        policy: StoragePolicy::frozen_default(),
        catalog: HistoryCatalogConfig {
            binding_key: b"r1d-kernel-host".to_vec(),
            cursor_key: [34; 32],
            incarnation: 1,
        },
        stack: LayerStackId::from_authority([31; 16]),
        stack_name: HistoryName::new("project")?,
        branch,
        branch_name: HistoryName::new("main")?,
        scope_seed: [35; 32],
        deadline: Instant::now() + Duration::from_secs(5),
    };
    let project = Timing::disabled("init", |timer| ProjectApi::new().init(request, timer)).0?;
    let keys = transport::generate_keypair()?;
    let daemon = transport::generate_keypair()?;
    let docker = Docker::new(&args[1], Duration::from_secs(3))?;
    let provider = Arc::new(layerfs_sandbox::Runtime::Docker(DockerProvider::new(
        docker,
    )));
    let api = SandboxApi::with_runtime(provider);
    let make = |name: &str, setup: DaemonSetup| ProviderCreate {
        launch: LaunchRequest {
            name: name.into(),
            daemon_executable: PathBuf::from(&args[4]),
            daemon_config: vec![],
            labels: vec![],
            startup_timeout: Duration::from_secs(3),
            options: ProviderLaunchOptions::Docker {
                image: args[2].clone(),
                store_volume: Some(args[3].clone()),
                port: 30421,
                connect_host: None,
            },
        },
        setup,
        controller_private: keys.private,
        daemon_peer: daemon.public,
    };
    let mut first = api
        .create(make("first", setup(daemon.private, keys.public)), &project)
        .map_err(|f| format!("first {f:?}"))?;
    println!(
        "OWNED_FIRST_CONTAINER {} phase={:?}",
        first.runtime.identity(),
        first.startup.phase
    );
    std::io::stdout().flush()?;
    assert_eq!(first.startup.phase, DaemonPhase::ControlReady);
    let installed = first.installed.take().expect("installed");
    let ready = layerfs_api::internal::sandbox_status(&mut first, true)?;
    assert_eq!(ready.phase, DaemonPhase::ControlReady);
    println!(
        "DIRECT_STORE host_sqlite={} daemon_sqlite={:?} incarnation={:?} profile=Disposable",
        installed.manifest.host_sqlite, installed.manifest.daemon_sqlite, ready.instance
    );
    let mut second_setup = setup(daemon.private, keys.public);
    second_setup.existing_store = Some(installed.manifest.clone());
    let second = api
        .create(make("second", second_setup), &project)
        .map_err(|f| format!("second {f:?}"))?;
    println!(
        "OWNED_SECOND_CONTAINER {} phase={:?}",
        second.runtime.identity(),
        second.startup.phase
    );
    std::io::stdout().flush()?;
    assert_eq!(second.startup.phase, DaemonPhase::ControlReady);
    assert_ne!(first.startup.instance, second.startup.instance);
    for (n, mut owner) in [(1, first), (2, second)] {
        ordinary(&owner)?;
        let mut workspace =
            layerfs_api::internal::workspace(layerfs_api::internal::control(&mut owner));
        let bound = workspace.bind(WorkspaceId::from_authority([40 + n; 32])?, branch)?;
        assert_eq!(bound.binding.effective_root, project.initialized.root);
        workspace.unmount(bound.token, false)?;
        assert!(matches!(
            layerfs_api::internal::control(&mut owner)
                .call(Request::EndSession)
                .map_err(|f| format!("end {f:?}"))?,
            Reply::SessionEnded
        ));
        let container = owner.runtime.identity().to_string();
        api.close(owner)?;
        println!("OWNED_CLEANUP container={container} disposition=Deleted_acknowledged_by_close");
    }
    let mut reopened_setup = setup(daemon.private, keys.public);
    reopened_setup.existing_store = Some(installed.manifest.clone());
    let mut reopened = api
        .create(make("reopened", reopened_setup), &project)
        .map_err(|f| format!("reopen {f:?}"))?;
    println!(
        "OWNED_REOPEN_CONTAINER {} phase={:?}",
        reopened.runtime.identity(),
        reopened.startup.phase
    );
    let mut workspace =
        layerfs_api::internal::workspace(layerfs_api::internal::control(&mut reopened));
    let bound = workspace.bind(WorkspaceId::from_authority([43; 32])?, branch)?;
    assert_eq!(bound.binding.effective_root, project.initialized.root);
    workspace.unmount(bound.token, false)?;
    assert!(matches!(
        layerfs_api::internal::control(&mut reopened)
            .call(Request::EndSession)
            .map_err(|f| format!("end {f:?}"))?,
        Reply::SessionEnded
    ));
    let container = reopened.runtime.identity().to_string();
    api.close(reopened)?;
    println!("SHARED_STORE_REOPEN_AFTER_BOTH_DELETIONS root={:?} container={container} cleanup=Deleted_acknowledged_by_close", project.initialized.root);
    println!("TWO_DAEMONS_SHARED_STORE separate_overlay=true root={:?} kernel_fuse=NOT_RUN graceful_daemon_drain=NOT_RUN",project.initialized.root);
    fs::remove_dir_all(directory)?;
    Ok(())
}
