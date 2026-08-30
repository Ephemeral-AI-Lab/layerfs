use layerfs_sdk::{
    BranchStore, Client, ConnectionContext, ContainerId, CreateWorkspaceSession, EndWorkspaceMode,
    EntityName, LayerStackEndpoint, LayerStackInitialization, LayerStackStore, LocalForkSource,
    NonEmpty, RemotePlacement, WorkspaceCommitResult, WorkspacePlacement, WorkspaceProjection,
};
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::Arc;

fn root() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "layerfs-v2-live-docker-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn one_prepared_container_runs_two_mounts_and_both_placement_benchmarks() {
    let Some(container) = std::env::var_os("LAYERFS_V2_CONTAINER") else {
        return;
    };
    let evidence = std::env::var_os("LAYERFS_V2_EVIDENCE_DIR")
        .map(std::path::PathBuf::from)
        .expect("LAYERFS_V2_EVIDENCE_DIR");
    let (base, control) = match std::env::var("LAYERFS_V2_BENCH_BASE").as_deref() {
        Ok("/var/tmp") => ("/var/tmp", "overlay"),
        Ok("/tmp") | Err(std::env::VarError::NotPresent) => ("/tmp", "tmpfs"),
        _ => panic!("LAYERFS_V2_BENCH_BASE must be /tmp or /var/tmp"),
    };
    let container = container.to_string_lossy().into_owned();
    let root = root();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&evidence).unwrap();
    let authority = Arc::new(LayerStackStore::create(root.join("authority.sqlite")).unwrap());
    let branches = BranchStore::create(root.join("branch.sqlite"), authority.store_id()).unwrap();
    let client = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(authority.clone()),
        branches,
    })
    .unwrap();
    let mut branch_ids = Vec::new();
    for (name, placement) in [
        ("reference", RemotePlacement::Reference),
        ("replica", RemotePlacement::Replica),
    ] {
        let source = root.join(format!("source-{name}"));
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("base"), name).unwrap();
        let layer = client
            .initialize_layerstack(
                EntityName::new(name).unwrap(),
                LayerStackInitialization::Directory(source),
            )
            .unwrap()
            .genesis_layer_id;
        client.pull_layer(layer, placement).unwrap();
        branch_ids.push((
            name,
            client
                .fork_branch(
                    EntityName::new(format!("{name}-main")).unwrap(),
                    LocalForkSource::Layer { layer_id: layer },
                )
                .unwrap(),
        ));
    }

    let valid_helper = std::env::var_os("LAYERFS_FUSE_HELPER").expect("LAYERFS_FUSE_HELPER");
    let invalid_helper = root.join("invalid-layerfs-fuse");
    std::fs::write(&invalid_helper, []).unwrap();
    std::env::set_var("LAYERFS_FUSE_HELPER", &invalid_helper);
    let failed_attach = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch_ids[0].1,
        placement: WorkspacePlacement::Container {
            container_id: ContainerId(container.clone()),
            root: "/workspace/failed-attach".into(),
        },
        projection: Some(WorkspaceProjection::Fuse),
    });
    std::env::set_var("LAYERFS_FUSE_HELPER", valid_helper);
    assert!(failed_attach.is_err());
    let failed_cleanup = docker(
        &container,
        &[
            "exec",
            &container,
            "/bin/sh",
            "-c",
            "test ! -e /workspace/failed-attach && ! find /var/tmp/layerfs-owned -mindepth 1 -print -quit | grep .",
        ],
    );
    assert!(
        failed_cleanup.status.success(),
        "{}",
        String::from_utf8_lossy(&failed_cleanup.stderr)
    );

    let mut mounted = Vec::new();
    for (name, branch_id) in &branch_ids {
        let started = std::time::Instant::now();
        let workspace = client
            .create_workspace_session(CreateWorkspaceSession {
                branch_id: *branch_id,
                placement: WorkspacePlacement::Container {
                    container_id: ContainerId(container.clone()),
                    root: format!("/workspace/{name}").into(),
                },
                projection: Some(WorkspaceProjection::Fuse),
            })
            .unwrap();
        println!("DOCKER_MOUNT {name} {}", started.elapsed().as_nanos());
        mounted.push((*name, workspace));
    }
    let stopped = client
        .exec_workspace_session(
            mounted[0].1,
            NonEmpty::new(vec![
                OsString::from("/bin/sh"),
                OsString::from("-c"),
                OsString::from("sleep 30 & wait"),
            ])
            .unwrap(),
        )
        .unwrap();
    client.stop_workspace_execution(stopped.id).unwrap();
    let output = client.workspace_output(stopped.id).unwrap();
    let mut after = 0;
    let stopped_receipt = loop {
        let page = output.read(after, true).unwrap();
        after = page.next_sequence;
        if page.exited {
            break page.receipt.unwrap();
        }
    };
    assert!(stopped_receipt.stopped);
    assert!(stopped_receipt.timing_balanced());
    assert!(stopped_receipt.direct_engine);
    let baseline_umask = docker(&container, &["exec", &container, "/bin/sh", "-c", "umask"]);
    assert!(baseline_umask.status.success());
    let baseline_umask =
        u32::from_str_radix(String::from_utf8_lossy(&baseline_umask.stdout).trim(), 8).unwrap();
    let mode_path = "/workspace/reference/exact-argv-mode";
    let exact = client
        .exec_workspace_session(
            mounted[0].1,
            NonEmpty::new(vec![
                OsString::from("/usr/bin/touch"),
                OsString::from(mode_path),
            ])
            .unwrap(),
        )
        .unwrap();
    let output = client.workspace_output(exact.id).unwrap();
    let mut after = 0;
    loop {
        let page = output.read(after, true).unwrap();
        after = page.next_sequence;
        if page.exited {
            let receipt = page.receipt.unwrap();
            assert_eq!(receipt.exit_code, Some(0));
            assert!(receipt.timing_balanced());
            assert!(receipt.direct_engine);
            break;
        }
    }
    let mode = docker(
        &container,
        &["exec", &container, "stat", "-c", "%a", mode_path],
    );
    assert!(mode.status.success());
    let mode = u32::from_str_radix(String::from_utf8_lossy(&mode.stdout).trim(), 8).unwrap();
    assert_eq!(
        mode,
        0o666 & !baseline_umask,
        "public Exec changed the container umask"
    );
    assert!(docker(&container, &["exec", &container, "rm", mode_path])
        .status
        .success());
    let functional = docker(
        &container,
        &[
            "exec",
            &container,
            "/bin/sh",
            "-c",
            "set -eu; test \"$(cat /workspace/reference/base)\" = reference; test \"$(cat /workspace/replica/base)\" = replica; printf reference-fuse > /workspace/reference/created; printf replica-fuse > /workspace/replica/created; ln /workspace/reference/created /workspace/reference/hard-link; ln -s created /workspace/replica/symlink; test \"$(cat /workspace/reference/hard-link)\" = reference-fuse; test \"$(cat /workspace/replica/symlink)\" = replica-fuse",
        ],
    );
    retain(&evidence.join("dual-functional.stdout"), &functional.stdout);
    retain(&evidence.join("dual-functional.stderr"), &functional.stderr);
    assert!(functional.status.success());
    let mounts = docker(
        &container,
        &[
            "exec",
            &container,
            "/bin/sh",
            "-c",
            "grep 'fuse.layerfs' /proc/self/mountinfo; find /workspace /var/lib/layerfs /var/tmp/layerfs-owned -name '*.sqlite*' -print",
        ],
    );
    retain(&evidence.join("dual-mountinfo.txt"), &mounts.stdout);
    retain(&evidence.join("dual-mountinfo.stderr"), &mounts.stderr);
    assert!(mounts.status.success());
    assert_eq!(
        String::from_utf8_lossy(&mounts.stdout)
            .lines()
            .filter(|line| line.contains(" - fuse layerfs "))
            .count(),
        2
    );
    assert!(!String::from_utf8_lossy(&mounts.stdout).contains("sqlite"));

    for (name, workspace) in mounted {
        let mount = format!("/workspace/{name}");
        let payload = format!("{mount}/created");
        let before_mount = mountinfo_for(&container, &mount);
        let ready = format!("/tmp/layerfs-rebase-{name}.ready");
        let release = format!("/tmp/layerfs-rebase-{name}.release");
        assert!(docker(
            &container,
            &["exec", &container, "rm", "-f", &ready, &release],
        )
        .status
        .success());
        let expected = format!("{name}-fuse");
        let script = format!(
            "import pathlib,time\np={payload:?}\nr={ready:?}\ng={release:?}\nwith open(p,'rb') as f:\n assert f.read()=={expected:?}.encode()\n pathlib.Path(r).touch()\n while not pathlib.Path(g).exists(): time.sleep(.01)\n f.seek(0)\n print(f.read().decode(),end='')",
        );
        let held = Command::new("docker")
            .args(["exec", &container, "python3", "-c", &script])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        for _ in 0..100 {
            if docker(&container, &["exec", &container, "test", "-e", &ready])
                .status
                .success()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            docker(&container, &["exec", &container, "test", "-e", &ready])
                .status
                .success()
        );
        assert!(matches!(
            client.commit_workspace_session(workspace).unwrap(),
            WorkspaceCommitResult::Created { .. }
        ));
        assert_eq!(mountinfo_for(&container, &mount), before_mount);
        assert_eq!(
            docker(&container, &["exec", &container, "cat", &payload]).stdout,
            format!("{name}-fuse").into_bytes()
        );
        assert!(docker(&container, &["exec", &container, "touch", &release])
            .status
            .success());
        let held = held.wait_with_output().unwrap();
        assert!(
            held.status.success(),
            "{}",
            String::from_utf8_lossy(&held.stderr)
        );
        assert_eq!(held.stdout, format!("{name}-fuse").into_bytes());
        let next = format!("{name}-second");
        assert!(docker(
            &container,
            &[
                "exec",
                &container,
                "/bin/sh",
                "-c",
                "printf '%s' \"$1\" > \"$2\"",
                "layerfs-next",
                &next,
                &payload,
            ],
        )
        .status
        .success());
        assert!(matches!(
            client.commit_workspace_session(workspace).unwrap(),
            WorkspaceCommitResult::Created { .. }
        ));
        client
            .push_branch(
                branch_ids
                    .iter()
                    .find(|(candidate, _)| *candidate == name)
                    .unwrap()
                    .1,
            )
            .unwrap();
        client
            .end_workspace_session(workspace, EndWorkspaceMode::Clean)
            .unwrap();
    }

    for (name, branch_id) in branch_ids {
        let workspace = client
            .create_workspace_session(CreateWorkspaceSession {
                branch_id,
                placement: WorkspacePlacement::Container {
                    container_id: ContainerId(container.clone()),
                    root: "/workspace".into(),
                },
                projection: Some(WorkspaceProjection::Fuse),
            })
            .unwrap();
        let cleanup = format!("rm -f /tmp/{name}.json /tmp/{name}.stdout /tmp/{name}.stderr /tmp/{name}.verification.json /tmp/{name}.verifier.stdout /tmp/{name}.verifier.stderr");
        assert!(
            docker(&container, &["exec", &container, "/bin/sh", "-c", &cleanup],)
                .status
                .success()
        );
        let command = format!(
            "SCENARIOS='create 1000 files,stat 1000 files,rm 1000 files,mkdir tree (10x10x10),find tree,write 64 MiB,copy 64 MiB,read 64 MiB,pure read 64 MiB,pure copy 64 MiB,overwrite 64 MiB,git init + commit 100 files' MOUNT=/workspace BASE={base} REPS=3 WARMUP=1 RANDOMIZE_TARGETS=1 OUTPUT_JSON=/tmp/{name}.json /usr/local/bin/fs-bench.sh > /tmp/{name}.stdout 2> /tmp/{name}.stderr"
        );
        let benchmark = docker(&container, &["exec", &container, "/bin/sh", "-c", &command]);
        assert!(benchmark.status.success());
        let verify = format!(
            "python3 /usr/local/bin/verify_fs_bench.py /tmp/{name}.json /tmp/{name}.stdout {control} /tmp/{name}.verification.json > /tmp/{name}.verifier.stdout 2> /tmp/{name}.verifier.stderr"
        );
        let verified = docker(&container, &["exec", &container, "/bin/sh", "-c", &verify]);
        for suffix in [
            "json",
            "stdout",
            "stderr",
            "verification.json",
            "verifier.stdout",
            "verifier.stderr",
        ] {
            copy_from(
                &container,
                &format!("/tmp/{name}.{suffix}"),
                &evidence.join(format!("{name}.{suffix}")),
            );
        }
        assert!(verified.status.success(), "{name} benchmark verifier");
        client
            .end_workspace_session(workspace, EndWorkspaceMode::Discard)
            .unwrap();
    }

    drop(client);
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

fn docker(_container: &str, arguments: &[&str]) -> Output {
    Command::new("docker").args(arguments).output().unwrap()
}

fn mountinfo_for(container: &str, mount: &str) -> Vec<u8> {
    let output = docker(
        container,
        &[
            "exec",
            container,
            "/bin/sh",
            "-c",
            "awk -v mount=\"$1\" '$5 == mount && $0 ~ / - fuse / { print; exit }' /proc/self/mountinfo",
            "layerfs-mountinfo",
            mount,
        ],
    );
    assert!(output.status.success());
    assert!(!output.stdout.is_empty());
    output.stdout
}

fn copy_from(container: &str, source: &str, destination: &Path) {
    let output = Command::new("docker")
        .args(["exec", container, "cat", source])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    retain(destination, &output.stdout);
}

fn retain(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
}
