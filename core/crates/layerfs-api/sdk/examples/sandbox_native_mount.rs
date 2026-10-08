//! Positive-path full topology: host Init and install into a shared VM volume,
//! then control only. The actual daemon mounts the Workspace natively inside
//! its Sandbox; ordinary nonroot commands launched by the runtime, never
//! registered with the daemon, read the complete root through plain syscalls.
use layerfs_bridge::{
    control::{ControlCode, DaemonPhase, NativePhase, Reply, Request},
    daemon_setup::{DaemonLimits, DaemonSetup},
    native,
};
use layerfs_history::{BranchId, HistoryCatalogConfig, HistoryName, LayerStackId, WorkspaceId};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_sandbox::backend::docker::{Docker, SandboxRequest};
use layerfs_sdk::{
    InitRequest, ManagedSandbox, OperationCause, ProjectApi, SandboxApi, SandboxCreate,
    WorkspaceApi,
};
use layerfs_storage::StoragePolicy;
use layerfs_telemetry::timer::Timing;
use std::{
    error::Error,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const FILE_BYTES: usize = 200_003;
const FILES: [(&str, &[u8], u32); 5] = [
    (".gitignore", b"ignored.bin\nnode_modules/\n", 0o644),
    (".git/index", b"complete index\0\xff", 0o644),
    (
        "ignored.bin",
        b"\0\xff\x01ignored by Git, present in LayerFS",
        0o644,
    ),
    (
        "node_modules/pkg/index.js",
        b"module.exports = 42;\n",
        0o644,
    ),
    ("file", &[], 0o640),
];
fn body(name: &str, declared: &[u8]) -> Vec<u8> {
    if name == "file" {
        (0..FILE_BYTES).map(|n| (n % 251) as u8).collect()
    } else {
        declared.to_vec()
    }
}
fn source(root: &Path) -> Result<(), Box<dyn Error>> {
    for directory in ["", ".git", "node_modules", "node_modules/pkg"] {
        fs::create_dir_all(root.join(directory))?;
        fs::set_permissions(root.join(directory), fs::Permissions::from_mode(0o755))?;
    }
    for (name, declared, mode) in FILES {
        fs::write(root.join(name), body(name, declared))?;
        fs::set_permissions(root.join(name), fs::Permissions::from_mode(mode))?;
    }
    fs::hard_link(root.join("file"), root.join("alias"))?;
    std::os::unix::fs::symlink("node_modules/pkg", root.join("dependency"))?;
    Ok(())
}
/// What `find` must print inside the mount: every path, nothing else.
fn listing() -> String {
    let mut lines = vec![
        "d 755 501 20 ./.git".to_owned(),
        "d 755 501 20 ./node_modules".to_owned(),
        "d 755 501 20 ./node_modules/pkg".to_owned(),
        "f 640 501 20 ./alias".to_owned(),
        "l 777 501 20 ./dependency".to_owned(),
    ];
    lines.extend(
        FILES
            .iter()
            .map(|(name, _, mode)| format!("f {mode:o} 501 20 ./{name}")),
    );
    lines.sort();
    lines.join("\n") + "\n"
}
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
        },
        existing_store: None,
    }
}
type Running = (
    layerfs_sandbox::backend::docker::RuntimeOutput,
    layerfs_sandbox::backend::docker::ExecHandle,
);
/// One ordinary runtime command under the deployment's nonroot identity.
fn start(owner: &ManagedSandbox, script: &str, directory: &str) -> Result<Running, Box<dyn Error>> {
    let created = owner
        .exec(
            vec!["bash".into(), "-c".into(), script.into()],
            vec![],
            directory.into(),
            false,
        )
        .map_err(|f| format!("create {f:?}"))?;
    let attached = created.start().map_err(|f| format!("start {f:?}"))?;
    let (_input, output, handle) = attached.into_parts().map_err(|f| format!("split {f:?}"))?;
    Ok((output, handle))
}
fn finish((output, handle): Running) -> Result<Vec<u8>, Box<dyn Error>> {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    output
        .copy_to(&mut stdout, &mut stderr)
        .map_err(|f| format!("output {f:?}"))?;
    let inspected = handle.inspect().map_err(|f| format!("inspect {f:?}"))?;
    if inspected.known_root_exit() != Some(0) {
        return Err(format!(
            "command {inspected:?}: {} {}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        )
        .into());
    }
    Ok(stdout)
}
fn run(owner: &ManagedSandbox, script: &str, directory: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    finish(start(owner, script, directory)?)
}
fn main() -> Result<(), Box<dyn Error>> {
    let began = Instant::now();
    // Diagnostic wall-clock marks only: no performance or cache claim.
    let mark = |phase: &str| println!("ELAPSED_MS {phase}={}", began.elapsed().as_millis());
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(
        args.len(),
        6,
        "socket image volume executable fresh-host-directory"
    );
    let directory = PathBuf::from(&args[5]);
    fs::create_dir(&directory)?;
    source(&directory.join("source"))?;
    let branch = BranchId::from_authority([62; 16]);
    let request = InitRequest {
        source: directory.join("source"),
        store: PersistenceConfig::sqlite(directory.join("sealed.sqlite"))
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        locator: "/layerfs-store/global/store.sqlite".into(),
        policy: StoragePolicy::frozen_default(),
        catalog: HistoryCatalogConfig {
            binding_key: b"r2-native-mount".to_vec(),
            cursor_key: [64; 32],
            incarnation: 1,
        },
        stack: LayerStackId::from_authority([61; 16]),
        stack_name: HistoryName::new("project")?,
        branch,
        branch_name: HistoryName::new("main")?,
        scope_seed: [65; 32],
        deadline: Instant::now() + Duration::from_secs(5),
    };
    let project = Timing::disabled("init", |timer| ProjectApi::new().init(request, timer)).0?;
    // The native source is absent from here on: the host is control only.
    fs::remove_dir_all(directory.join("source"))?;
    let keys = native::generate_keypair()?;
    let daemon = native::generate_keypair()?;
    let api = SandboxApi::new(Docker::new(&args[1], Duration::from_secs(3))?);
    let mut owner = api
        .create(SandboxCreate {
            deployment: SandboxRequest {
                image: args[2].clone(),
                store_volume: args[3].clone(),
                port: 30421,
            },
            setup: setup(daemon.private, keys.public),
            executable: PathBuf::from(&args[4]),
            controller_private: keys.private,
            daemon_peer: daemon.public,
            startup_wait: Duration::from_secs(3),
        })
        .map_err(|f| format!("create {f:?}"))?;
    println!("OWNED_CONTAINER {}", owner.runtime.identity());
    mark("sandbox_started");
    assert_eq!(owner.startup.phase, DaemonPhase::InstallPending);
    let installed = owner.control.install_project(&project)?;
    assert_eq!(owner.status(true)?.phase, DaemonPhase::ControlReady);
    println!(
        "DIRECT_STORE host_sqlite={} daemon_sqlite={:?} profile=Disposable volume={}",
        installed.manifest.host_sqlite, installed.manifest.daemon_sqlite, args[3]
    );

    // Two acknowledgements: bound, then natively Ready.
    let workspace = WorkspaceId::from_authority([71; 32])?;
    let mounted = WorkspaceApi::new(&mut owner.control)
        .mount(workspace, branch)
        .map_err(|f| format!("mount {f:?}"))?;
    assert_eq!(
        mounted.bound.binding.effective_root,
        project.initialized.root
    );
    let root = mounted.ready.directory.clone();
    assert_eq!(
        root,
        format!("/workspaces/{}", mounted.bound.token.namespace)
    );
    assert_eq!(mounted.ready.receipt.loops, 2);
    println!("NATIVE_READY {:?}", mounted.ready);
    mark("ready");

    // Environment and protection facts observed by an ordinary command.
    let facts = run(
        &owner,
        &format!(
            "set -e; test \"$(id -u):$(id -g):$(id -G)\" = 501:20:20; \
             grep -q '^CapEff:[[:space:]]*0000000000000000$' /proc/self/status; \
             grep -q '^NoNewPrivs:[[:space:]]*1$' /proc/self/status; \
             for p in /layerfs-local/config/daemon.setup /layerfs-local/overlay/overlay.sqlite \
               /layerfs-store/global/store.sqlite /proc/1/root/layerfs-local/config/daemon.setup \
               /proc/1/root/layerfs-store/global/store.sqlite /proc/1/environ /proc/1/fd /dev/fuse; \
               do test ! -r \"$p\"; test ! -w \"$p\"; done; \
             ! (exec 3<>/dev/fuse) 2>/dev/null; test ! -w /sys/fs/fuse/connections; \
             ! touch {root}/created 2>/dev/null; ! (echo x >> {root}/file) 2>/dev/null; \
             ! mkdir {root}/made 2>/dev/null; ! rm {root}/ignored.bin 2>/dev/null; \
             printf 'KERNEL %s\\n' \"$(uname -r)\"; \
             printf 'DEVICE %s\\n' \"$(stat -c '%F %a %u:%g %t:%T' /dev/fuse)\"; \
             printf 'MOUNT %s\\n' \"$(grep ' {root} ' /proc/self/mountinfo)\"; \
             printf 'ORDINARY_ACCESS uid=501 gid=20 CapEff=0 NoNewPrivs=1 private_backing_denied proc_aliases_denied fuse_device_denied mutation_refused\\n'"
        ),
        "/",
    )?;
    let facts = String::from_utf8(facts)?;
    print!("{facts}");
    mark("protection_facts");
    assert!(facts.contains(" fuse layerfs ") && facts.contains("allow_other"));
    assert!(facts.contains("DEVICE character special file 600 0:0 a:e5"));

    // Independent complete-root oracle: names, types, modes, owners, link
    // identity, the raw link target and every byte of every regular file.
    let listed = run(
        &owner,
        "set -e; find . -mindepth 1 -printf '%y %m %U %G %p\\n' | LC_ALL=C sort",
        &root,
    )?;
    assert_eq!(String::from_utf8(listed)?, listing());
    let links = run(
        &owner,
        "set -e; readlink dependency; stat -c '%s %h' file alias; \
         test \"$(stat -c %i file)\" = \"$(stat -c %i alias)\"; \
         cmp dependency/index.js node_modules/pkg/index.js; test ! -e missing",
        &root,
    )?;
    assert_eq!(
        String::from_utf8(links)?,
        format!("node_modules/pkg\n{FILE_BYTES} 2\n{FILE_BYTES} 2\n")
    );
    let order = [0, 1, 2, 3, 4];
    let bytes = run(
        &owner,
        "set -e; cat .gitignore .git/index ignored.bin node_modules/pkg/index.js file alias",
        &root,
    )?;
    let mut expected: Vec<u8> = order
        .iter()
        .flat_map(|&n| body(FILES[n].0, FILES[n].1))
        .collect();
    expected.extend(body("file", &[]));
    assert!(bytes == expected, "regular file bytes differ");
    mark("complete_root");
    println!(
        "COMPLETE_ROOT paths=10 regular_bytes={} hardlink=true symlink=true ignored_and_dependencies=true",
        expected.len()
    );

    // Sustained work on the same mount by an ordinary long-running command.
    let rounds = run(
        &owner,
        "set -e; n=0; for i in $(seq 40); do cat file alias .git/index >/dev/null; \
         ls -laR . >/dev/null; dd if=file bs=4096 skip=31 count=2 iflag=direct status=none | wc -c >/dev/null; \
         n=$((n+1)); done; echo $n",
        &root,
    )?;
    assert_eq!(rounds, b"40\n");
    mark("sustained");

    // Reversible normal unmount: a running command's working directory keeps
    // the kernel busy. The filesystem neither kills nor waits for the command.
    let holder = start(&owner, "sleep 1.2", &format!("{root}/node_modules"))?;
    run(
        &owner,
        &format!(
            "until ls -l /proc/[0-9]*/cwd 2>/dev/null | grep -q '{root}/node_modules'; do sleep 0.02; done"
        ),
        "/",
    )?;
    let busy = WorkspaceApi::new(&mut owner.control)
        .unmount(mounted.bound.token)
        .unwrap_err();
    match &busy.cause {
        OperationCause::Remote(refusal) => {
            assert_eq!(
                (refusal.code, refusal.phase.as_str()),
                (ControlCode::Busy, "unmount:kernel")
            );
        }
        other => panic!("{other:?}"),
    }
    let status = WorkspaceApi::new(&mut owner.control).status(mounted.bound.token)?;
    let native = status.native.expect("native status block");
    assert_eq!(native.phase, NativePhase::Ready);
    assert_eq!(
        native.ready.as_ref(),
        Some(&mounted.ready),
        "same connection"
    );
    assert_eq!(run(&owner, "cat .git/index", &root)?, FILES[1].1);
    finish(holder)?;
    println!("UNMOUNT_BUSY phase=unmount:kernel workspace_ready=true command_untouched=true");
    mark("busy");

    // Terminal unmount: detach, loop joins, drain, revocation and Close.
    WorkspaceApi::new(&mut owner.control)
        .unmount(mounted.bound.token)
        .map_err(|f| format!("unmount {f:?}"))?;
    run(
        &owner,
        &format!("set -e; test ! -e {root}; ! grep -q ' {root} ' /proc/self/mountinfo"),
        "/",
    )?;
    let gone = WorkspaceApi::new(&mut owner.control)
        .locate(workspace)
        .unwrap_err();
    assert!(
        matches!(&gone.cause, OperationCause::Remote(refusal) if refusal.code == ControlCode::Missing)
    );
    println!("UNMOUNTED directory_removed=true mount_table_entry=absent routing=Missing");
    mark("unmounted");

    // Frequent fresh mounts of the same Branch: new namespace, new directory.
    for tag in 72..75 {
        let fresh = WorkspaceApi::new(&mut owner.control)
            .mount(WorkspaceId::from_authority([tag; 32])?, branch)
            .map_err(|f| format!("fresh mount {f:?}"))?;
        assert_ne!(fresh.ready.directory, root);
        assert_eq!(
            run(
                &owner,
                "wc -c < file; cat .git/index",
                &fresh.ready.directory
            )?,
            [format!("{FILE_BYTES}\n").as_bytes(), FILES[1].1].concat()
        );
        WorkspaceApi::new(&mut owner.control)
            .unmount(fresh.bound.token)
            .map_err(|f| format!("fresh unmount {f:?}"))?;
    }
    println!("FRESH_MOUNTS count=3 each_ready_read_unmounted=true");
    mark("fresh_mounts");
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
    mark("container_deleted");
    println!(
        "OWNED_CLEANUP container={} disposition={:?}",
        owner.runtime.identity(),
        owner.runtime.disposition()
    );
    println!(
        "NATIVE_FULL_TOPOLOGY host_init=true host_data_path=false named_volume={} root={:?} native_fuse=READ_PATH mutation=NOT_RUN commit=NOT_RUN force_unmount=NOT_RUN durable=NOT_RUN",
        args[3], project.initialized.root
    );
    fs::remove_dir_all(directory)?;
    Ok(())
}
