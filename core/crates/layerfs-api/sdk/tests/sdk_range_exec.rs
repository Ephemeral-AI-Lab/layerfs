//! Public SDK Exec→Commit gate for the frozen #241 mounted splice tool.
//! Set LAYERFS_TEST_IMAGE to an immutable image containing the release daemon
//! and `/layerfs-bench/bin/layerfs-edit-tool` to run the live Docker selection.
#[path = "support/range_exec_gate.rs"]
mod range_exec_gate;

use layerfs_api_core::{SandboxId, WorkspaceId};
use layerfs_bridge::contract::CommitOutcomeWire;
use layerfs_sandbox::SandboxOwner;
use layerfs_sdk::{HistoryMode, ProjectApi, SandboxApi, Server, ServerConfig, WorkspaceApi};
use layerfs_telemetry::runtime::Runtime;
use std::{fs, path::PathBuf};

struct Cleanup<'a> {
    owner: &'a SandboxOwner,
    mount: Option<WorkspaceId>,
    sandbox: Option<SandboxId>,
    root: PathBuf,
}
impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let mut clean = true;
        if let Some(mount) = self.mount.take() {
            if let Err(error) = WorkspaceApi::new(self.owner).unmount(&mount) {
                eprintln!("SDK_RANGE_CLEANUP unmount {mount:?}: {error:?}");
                clean = false;
            }
        }
        if let Some(sandbox) = self.sandbox.take() {
            if let Err(error) = SandboxApi::new(self.owner).delete(sandbox) {
                eprintln!("SDK_RANGE_CLEANUP sandbox {sandbox:?}: {error:?}");
                clean = false;
            }
        }
        if clean {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

#[test]
fn only_confirmed_exit_is_commit_eligible() {
    assert!(range_exec_gate::confirmed_exit(Some(0)));
    for status in [Some(75), Some(1), None] {
        assert!(!range_exec_gate::confirmed_exit(status));
    }
}

#[test]
fn mounted_sdk_exec_confirms_before_explicit_commit() {
    let Ok(image) = std::env::var("LAYERFS_TEST_IMAGE") else {
        return;
    };
    assert!(image.starts_with("sha256:"), "use an immutable image ID");
    let root = std::env::temp_dir().join(format!("layerfs-241-sdk-gate-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let source = root.join("source");
    fs::create_dir(&source).unwrap();
    fs::write(
        source.join("data.bin"),
        (0..8192).map(|i| (i % 251) as u8).collect::<Vec<_>>(),
    )
    .unwrap();
    fs::write(
        source.join("payload.bin"),
        (0..4096).map(|i| (i % 239) as u8).collect::<Vec<_>>(),
    )
    .unwrap();
    let server = Server::create(ServerConfig {
        store_path: root.join("store.sqlite"),
        history_path: root.join("history.sqlite"),
        binding_key: b"issue241-sdk-gate".to_vec(),
        incarnation: 1,
        cursor_key: [35; 32],
        history: HistoryMode::Create,
        service_host: "host.docker.internal".into(),
        runtime: Runtime::disabled(),
        telemetry_run: None,
    })
    .unwrap();
    server.listen().unwrap();
    let owner = server.owner().unwrap();
    let mut cleanup = Cleanup {
        owner: &owner,
        mount: None,
        sandbox: None,
        root,
    };
    let project = ProjectApi::new(&server)
        .init("issue241-sdk-gate", &source)
        .unwrap();
    let branch = ProjectApi::new(&server)
        .fork(&project, [45; 16], "main")
        .unwrap();
    let sandbox = SandboxApi::new(&owner)
        .create(&image, "issue241-gate")
        .unwrap();
    cleanup.sandbox = Some(sandbox);
    let api = WorkspaceApi::new(&owner);
    let mount = api.mount(sandbox, &project, branch.id, None).unwrap();
    cleanup.mount = Some(mount.id.clone());

    let command = "/layerfs-bench/bin/layerfs-edit-tool splice --file data.bin --expect-size 8192 --offset 4093 --delete-length 0 --length 4096 --payload payload.bin";
    let (exec, commit) =
        range_exec_gate::exec_and_commit_on_success(&api, &mount.id, command).unwrap();
    assert_eq!(
        exec.exit_status,
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&exec.stderr)
    );
    assert!(!exec.stdout_truncated && !exec.stderr_truncated);
    let output = String::from_utf8(exec.stdout).unwrap();
    assert!(
        output.contains("\"status\":\"PASS\",\"operation\":\"splice\""),
        "{output}"
    );
    assert!(
        output.contains("\"final_bytes\":12288,\"shifted_bytes\":0"),
        "{output}"
    );
    let commit = commit.expect("confirmed Exec must precede explicit Commit");
    assert!(matches!(commit.outcome, CommitOutcomeWire::Committed(_)));
    println!("SDK_RANGE_TOOL {}", output.trim());
    println!("SDK_RANGE_COMMIT {:?}", commit.outcome);
    let status = api.status(&mount.id).unwrap();
    assert_eq!(status.projection_count("range_state"), Some(2));
    assert_eq!(status.projection_count("range_edit"), Some(1));
    assert_eq!(status.range_accepted_payload_bytes, 4096);
    assert_eq!(status.range_shifted_suffix_bytes, 0);
    println!(
        "SDK_RANGE_STATUS mounted={} state={} edit={} accepted={} shifted={}",
        status.mounted,
        status.projection_count("range_state").unwrap(),
        status.projection_count("range_edit").unwrap(),
        status.range_accepted_payload_bytes,
        status.range_shifted_suffix_bytes
    );

    let (unknown, omitted) =
        range_exec_gate::exec_and_commit_on_success(&api, &mount.id, "exit 75").unwrap();
    assert_eq!(unknown.exit_status, Some(75));
    assert!(omitted.is_none(), "UNKNOWN exit must not call Commit");
    println!(
        "SDK_RANGE_UNKNOWN exit={:?} commit=omitted",
        unknown.exit_status
    );
    println!("SDK_RANGE_GATE success-then-Commit PASS; exit-75 Commit omitted PASS");

    api.unmount(&mount.id).unwrap();
    cleanup.mount = None;
    SandboxApi::new(&owner).delete(sandbox).unwrap();
    cleanup.sandbox = None;
    server.shutdown();
}
