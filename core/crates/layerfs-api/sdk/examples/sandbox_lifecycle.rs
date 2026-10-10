//! Positive-path host Init -> protected owned Sandbox -> direct shared Store control proof.
//! Typed failure custody is covered separately by public adapter tests; this example reports failures in stderr.
use layerfs_bridge::{
    control::{DaemonPhase, Reply, Request},
    daemon_setup::{DaemonLimits, DaemonSetup},
    native,
};
use layerfs_history::{BranchId, HistoryCatalogConfig, HistoryName, LayerStackId, WorkspaceId};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_sandbox::backend::docker::{Docker, SandboxRequest};
use layerfs_sdk::{
    InitRequest, ManagedSandbox, ProjectApi, SandboxApi, SandboxCreate, WorkspaceApi,
};
use layerfs_storage::StoragePolicy;
use layerfs_telemetry::timer::Timing;
use std::{
    fs,
    io::Write,
    path::PathBuf,
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
            read_handles: 2,
            cache_bytes: 65536,
            owner_bytes: 16 * 1024 * 1024,
            lifecycle_reserve: 2 * 1024 * 1024,
            namespaces: 8,
            ordinary_jobs: 8,
            lifecycle_jobs: 4,
            pager_kib: 1024,
            serial_low_water: 0,
        },
        existing_store: None,
    }
}
fn ordinary(owner: &ManagedSandbox) -> Result<(), Box<dyn std::error::Error>> {
    let command="set -e; test \"$(id -u)\" = 501; test \"$(id -g)\" = 20; test \"$(id -G)\" = 20; grep -q '^CapEff:[[:space:]]*0000000000000000$' /proc/self/status; grep -q '^CapAmb:[[:space:]]*0000000000000000$' /proc/self/status; grep -q '^NoNewPrivs:[[:space:]]*1$' /proc/self/status; for p in /layerfs-local/config/daemon.setup /layerfs-local/overlay/overlay.sqlite /layerfs-store/global/store.sqlite /proc/1/root/layerfs-local/config/daemon.setup /proc/1/root/layerfs-store/global/store.sqlite /proc/1/environ /proc/1/fd; do test ! -r \"$p\"; done; test ! -w /dev/fuse; test ! -w /sys/fs/fuse/connections; printf 'ORDINARY_ACCESS uid=501 gid=20 groups=20 CapEff=0 CapAmb=0 NoNewPrivs=1 private_backing_denied proc_aliases_denied fuse_control_denied\\n'";
    let created = owner
        .exec(
            vec!["bash".into(), "-c".into(), command.into()],
            vec![],
            "/".into(),
            false,
        )
        .map_err(|f| format!("create {f:?}"))?;
    let attached = created.start().map_err(|f| format!("start {f:?}"))?;
    let (_input, output, handle) = attached.into_parts().map_err(|f| format!("split {f:?}"))?;
    let progress = output
        .copy_to(&mut std::io::stdout(), &mut std::io::stderr())
        .map_err(|f| format!("output {f:?}"))?;
    let inspected = handle.inspect().map_err(|f| format!("inspect {f:?}"))?;
    println!("ORDINARY_RESULT {:?} {:?}", progress, inspected);
    assert_eq!(inspected.known_root_exit(), Some(0));
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
            binding_key: b"r1d-native-host".to_vec(),
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
    let keys = native::generate_keypair()?;
    let daemon = native::generate_keypair()?;
    let docker = Docker::new(&args[1], Duration::from_secs(3))?;
    let api = SandboxApi::new(docker);
    let make = |setup: DaemonSetup| SandboxCreate {
        deployment: SandboxRequest {
            image: args[2].clone(),
            store_volume: args[3].clone(),
            port: 30421,
        },
        setup,
        executable: PathBuf::from(&args[4]),
        controller_private: keys.private,
        daemon_peer: daemon.public,
        startup_wait: Duration::from_secs(3),
    };
    let mut first = api
        .create(make(setup(daemon.private, keys.public)))
        .map_err(|f| format!("first {f:?}"))?;
    println!(
        "OWNED_FIRST_CONTAINER {} phase={:?}",
        first.runtime.identity(),
        first.startup.phase
    );
    std::io::stdout().flush()?;
    assert_eq!(first.startup.phase, DaemonPhase::InstallPending);
    let installed = first.control.install_project(&project)?;
    let ready = first.status(true)?;
    assert_eq!(ready.phase, DaemonPhase::ControlReady);
    println!(
        "DIRECT_STORE host_sqlite={} daemon_sqlite={:?} incarnation={:?} profile=Disposable",
        installed.manifest.host_sqlite, installed.manifest.daemon_sqlite, ready.instance
    );
    let mut second_setup = setup(daemon.private, keys.public);
    second_setup.existing_store = Some(installed.manifest.clone());
    let mut second = api
        .create(make(second_setup))
        .map_err(|f| format!("second {f:?}"))?;
    println!(
        "OWNED_SECOND_CONTAINER {} phase={:?}",
        second.runtime.identity(),
        second.startup.phase
    );
    std::io::stdout().flush()?;
    assert_eq!(second.startup.phase, DaemonPhase::ControlReady);
    assert_ne!(first.startup.instance, second.startup.instance);
    for (n, owner) in [(1, &mut first), (2, &mut second)] {
        ordinary(owner)?;
        let mut workspace = WorkspaceApi::new(&mut owner.control);
        let bound = workspace.bind(WorkspaceId::from_authority([40 + n; 32])?, branch)?;
        assert_eq!(bound.binding.effective_root, project.initialized.root);
        workspace.unmount(bound.token)?;
        assert!(matches!(
            owner
                .control
                .call(Request::EndSession)
                .map_err(|f| format!("end {f:?}"))?,
            Reply::SessionEnded
        ));
        owner.runtime.stop(1).map_err(|f| format!("stop {f:?}"))?;
        owner
            .runtime
            .delete()
            .map_err(|f| format!("delete {f:?}"))?;
        println!(
            "OWNED_CLEANUP container={} disposition={:?}",
            owner.runtime.identity(),
            owner.runtime.disposition()
        );
    }
    let mut reopened_setup = setup(daemon.private, keys.public);
    reopened_setup.existing_store = Some(installed.manifest.clone());
    let mut reopened = api
        .create(make(reopened_setup))
        .map_err(|f| format!("reopen {f:?}"))?;
    println!(
        "OWNED_REOPEN_CONTAINER {} phase={:?}",
        reopened.runtime.identity(),
        reopened.startup.phase
    );
    let mut workspace = WorkspaceApi::new(&mut reopened.control);
    let bound = workspace.bind(WorkspaceId::from_authority([43; 32])?, branch)?;
    assert_eq!(bound.binding.effective_root, project.initialized.root);
    workspace.unmount(bound.token)?;
    assert!(matches!(
        reopened
            .control
            .call(Request::EndSession)
            .map_err(|f| format!("end {f:?}"))?,
        Reply::SessionEnded
    ));
    reopened
        .runtime
        .stop(1)
        .map_err(|f| format!("stop {f:?}"))?;
    reopened
        .runtime
        .delete()
        .map_err(|f| format!("delete {f:?}"))?;
    println!(
        "SHARED_STORE_REOPEN_AFTER_BOTH_DELETIONS root={:?} container={} cleanup={:?}",
        project.initialized.root,
        reopened.runtime.identity(),
        reopened.runtime.disposition()
    );
    println!("TWO_DAEMONS_SHARED_STORE separate_overlay=true root={:?} native_fuse=NOT_RUN graceful_daemon_drain=NOT_RUN",project.initialized.root);
    fs::remove_dir_all(directory)?;
    Ok(())
}
