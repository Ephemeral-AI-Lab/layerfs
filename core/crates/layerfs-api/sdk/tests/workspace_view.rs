//! Optional live Docker proof of the public read-only view lease: set
//! `LAYERFS_TEST_IMAGE` to an immutable image with `/layerfs-daemon` and
//! `/bin/sh`. Every product operation goes through the public SDK.
//!
//! The falsifiers pinned here: the pinned root, generation/revision and base
//! are observable and stable across a later Commit; old G1 bytes read through
//! the lease and live G2 bytes read through `exec` are both exact in one
//! attached Workspace; a released lease, a cross-lease entry and a forged
//! token are each refused; 32 leases are admitted and the 33rd is refused
//! without pinning; and after every lease released, the unmount completes,
//! which the retained pins of a failed release would have blocked.
use layerfs_api_core::{WorkspaceError, WorkspaceViewKind};
use layerfs_bridge::contract::Code;
use layerfs_sdk::{HistoryMode, ProjectApi, SandboxApi, Server, ServerConfig, WorkspaceApi};
use std::path::PathBuf;

const BRANCH: [u8; 16] = [37; 16];

struct Cleanup<'a> {
    root: PathBuf,
    owner: &'a layerfs_sandbox::SandboxOwner,
    sandboxes: Vec<layerfs_api_core::SandboxId>,
}
impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let api = SandboxApi::new(self.owner);
        for id in std::mem::take(&mut self.sandboxes) {
            let mut captured = Vec::new();
            let (_, logs) = api.delete_with_logs(id, &mut captured);
            eprintln!("---- sandbox {id} daemon logs: {logs:?} ----");
            eprintln!("{}", String::from_utf8_lossy(&captured));
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn view_lease_pins_g1_reads_old_bytes_across_commit_and_refuses_stale_use() {
    let Ok(image) = std::env::var("LAYERFS_TEST_IMAGE") else {
        return;
    };
    let root = std::env::temp_dir().join(format!("layerfs-view-lease-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("source");
    std::fs::create_dir_all(source.join("dir/sub")).unwrap();
    std::fs::write(source.join("note"), b"base").unwrap();
    std::fs::write(source.join("dir/sub/leaf"), b"leaf-base").unwrap();
    let server = Server::create(ServerConfig {
        store_path: root.join("store.sqlite"),
        history_path: root.join("history.sqlite"),
        binding_key: b"view-lease".to_vec(),
        incarnation: 1,
        cursor_key: [38; 32],
        history: HistoryMode::Create,
        service_host: "host.docker.internal".into(),
        runtime: layerfs_telemetry::runtime::Runtime::disabled(),
        telemetry_run: None,
    })
    .unwrap();
    server.listen().unwrap();
    let owner = server.owner().unwrap();
    let mut cleanup = Cleanup {
        root: root.clone(),
        owner: &owner,
        sandboxes: Vec::new(),
    };
    let projects = ProjectApi::new(&server);
    let sandboxes = SandboxApi::new(&owner);
    let workspaces = WorkspaceApi::new(&owner);
    let project = projects.init("view-lease", &source).unwrap();
    let branch = projects.fork(&project, BRANCH, "main").unwrap();
    let sandbox = sandboxes.create(&image, "view-lease-one").unwrap();
    cleanup.sandboxes.push(sandbox);
    let mount = workspaces
        .mount(sandbox, &project, branch.id, None)
        .unwrap();

    // G1: one uncommitted edit generation with distinct bytes and a directory.
    workspaces
        .exec(
            &mount.id,
            "printf g1-note > note; printf g1-leaf > dir/sub/leaf; mkdir dir/added",
        )
        .unwrap();

    // Pin the current selected view before the commit.
    let lease = workspaces.pin_view(&mount.id).unwrap();
    assert_eq!(lease.root().kind, WorkspaceViewKind::Directory);
    assert!(lease.root().serial > 0);
    let pinned = (lease.generation(), lease.revision());
    let lease_root_serial = lease.root().serial;

    // The pinned view resolves names and reads bytes from the pinned G1.
    let note = workspaces
        .view_lookup(&lease, lease.root(), b"note")
        .unwrap();
    assert_eq!(note.kind, WorkspaceViewKind::File);
    assert_eq!(note.size, b"g1-note".len() as u64);
    let dir = workspaces
        .view_lookup(&lease, lease.root(), b"dir")
        .unwrap();
    assert_eq!(dir.kind, WorkspaceViewKind::Directory);
    let read = workspaces.view_read(&lease, &note, 0, 64).unwrap();
    assert_eq!(read.bytes, b"g1-note");
    assert!(read.eof);
    assert_eq!(read.size, b"g1-note".len() as u64);
    let sub = workspaces.view_lookup(&lease, &dir, b"sub").unwrap();
    let leaf = workspaces.view_lookup(&lease, &sub, b"leaf").unwrap();
    assert_eq!(
        workspaces.view_read(&lease, &leaf, 0, 64).unwrap().bytes,
        b"g1-leaf"
    );
    // The pinned listing sees the G1 directory contents.
    let page = workspaces.view_list(&lease, &dir, None, 128).unwrap();
    let names = page
        .entries
        .iter()
        .map(|(name, _)| name.as_slice())
        .collect::<Vec<_>>();
    assert!(names.contains(&b"sub".as_slice()));
    assert!(names.contains(&b"added".as_slice()));

    // Commit G1, then advance G2 with different bytes and namespace.
    workspaces.commit(&mount.id).unwrap();
    workspaces
        .exec(
            &mount.id,
            "printf g2-note > note; rmdir dir/added; printf g2-leaf > dir/sub/leaf",
        )
        .unwrap();

    // The lease still reports the pinned selection; nothing was re-pinned.
    let status = workspaces.view_status(&lease).unwrap();
    assert_eq!((status.generation, status.revision), pinned);
    assert!(status.held_leases >= 1);
    // Root, note, dir, sub and leaf: every entry this lease issued.
    assert_eq!(status.entries, 5);

    // Old G1 bytes through the lease and live G2 bytes through exec, one
    // attached Workspace, same process, independently exact.
    assert_eq!(
        workspaces.view_read(&lease, &note, 0, 64).unwrap().bytes,
        b"g1-note"
    );
    assert_eq!(
        workspaces.view_read(&lease, &leaf, 0, 64).unwrap().bytes,
        b"g1-leaf"
    );
    let live = workspaces
        .exec(&mount.id, "cat note; cat dir/sub/leaf")
        .unwrap();
    assert_eq!(live.stdout, b"g2-noteg2-leaf");
    // The pinned listing keeps the G1 name; live G2 removed it.
    let page = workspaces.view_list(&lease, &dir, None, 128).unwrap();
    assert!(page
        .entries
        .iter()
        .any(|(name, _)| name.as_slice() == b"added"));
    let live_dir = workspaces.exec(&mount.id, "ls dir").unwrap();
    assert!(!live_dir.stdout.windows(5).any(|w| w == b"added"));

    // A second lease pins the G2 selection: its entries cannot serve the
    // first lease's reads, and neither lease accepts the other's entry.
    let second = workspaces.pin_view(&mount.id).unwrap();
    let second_note = workspaces
        .view_lookup(&second, second.root(), b"note")
        .unwrap();
    assert_eq!(
        workspaces
            .view_read(&second, &second_note, 0, 64)
            .unwrap()
            .bytes,
        b"g2-note"
    );
    assert_eq!(
        workspaces.view_read(&lease, &second_note, 0, 64),
        Err(WorkspaceError::Stale)
    );
    assert_eq!(
        workspaces.view_read(&second, &note, 0, 64),
        Err(WorkspaceError::Stale)
    );

    // A malformed-tag token is refused by wire validation, and a well-formed
    // token nobody registered is refused by the lease registry; neither
    // reaches any resolution.
    let mut malformed = lease.clone();
    malformed.token = [9; 33];
    assert!(matches!(
        workspaces.view_status(&malformed),
        Err(WorkspaceError::Failure(failure)) if failure.code == Code::InvalidInput
    ));
    let mut unregistered = lease.clone();
    unregistered.token[1] ^= 1;
    // Print the typed outcome, never the token. A previous live assertion
    // failed without capturing the actual code; that attempt remains FAIL.
    let refusal = workspaces.view_status(&unregistered);
    eprintln!("VIEW_LEASE_TOKEN_DIAGNOSTIC case=unregistered expected=Denied actual={refusal:?}");
    assert!(
        matches!(&refusal, Err(WorkspaceError::Failure(failure)) if failure.code == Code::Denied),
        "unregistered token must be Denied, observed {refusal:?}"
    );

    // The checked release: Completed, and the lease token is dead afterwards.
    assert_eq!(
        workspaces.release_view(&lease),
        Ok(layerfs_api_core::WorkspaceViewRelease::Completed)
    );
    assert!(matches!(
        workspaces.view_status(&lease),
        Err(WorkspaceError::Failure(failure)) if failure.code == Code::Denied
    ));
    workspaces.release_view(&second).unwrap();

    // 32 leases are admitted; the 33rd refuses without pinning.
    let mut leases = Vec::new();
    for _ in 0..32 {
        leases.push(workspaces.pin_view(&mount.id).unwrap());
    }
    assert!(matches!(
        workspaces.pin_view(&mount.id),
        Err(WorkspaceError::Failure(failure)) if failure.code == Code::Capacity
    ));
    for lease in &leases {
        workspaces.release_view(lease).unwrap();
    }

    // Every lease released: the unmount completes. A retained pin (a failed
    // release's custody) would have failed this closed.
    workspaces.unmount(&mount.id).unwrap();
    sandboxes.delete(sandbox).unwrap();
    cleanup.sandboxes.clear();
    drop(cleanup);
    println!(
        "VIEW_LEASE pinned_root=serial:{} generation={} revision={} g1_bytes=exact g2_bytes=exact stale=refused forged=refused bound=32 release=Completed unmount=Completed",
        lease_root_serial, pinned.0, pinned.1
    );
}

/// Test-only external fault: pause exactly the sandbox this test created,
/// discovered by its owner and unique name labels. Always resume it before
/// SDK cleanup, including when the asserted deadline path panics.
struct PausedSandbox(String);
impl PausedSandbox {
    fn new(name: &str) -> Self {
        let output = std::process::Command::new("docker")
            .args([
                "ps",
                "--filter",
                "label=io.layerfs.owner=agent-sdk",
                "--filter",
                &format!("label=io.layerfs.sandbox-name={name}"),
                "--format",
                "{{.ID}}",
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        let ids = String::from_utf8(output.stdout).unwrap();
        let id = ids.lines().next().unwrap();
        assert_eq!(ids.lines().count(), 1, "pause only our unique sandbox");
        let paused = std::process::Command::new("docker")
            .args(["pause", id])
            .output()
            .unwrap();
        assert!(paused.status.success(), "pause: {paused:?}");
        Self(id.to_owned())
    }
}
impl Drop for PausedSandbox {
    fn drop(&mut self) {
        let resumed = std::process::Command::new("docker")
            .args(["unpause", &self.0])
            .output()
            .unwrap();
        assert!(resumed.status.success(), "unpause: {resumed:?}");
    }
}

#[test]
fn view_lease_deadline_keeps_old_bytes_and_allows_checked_release() {
    let Ok(image) = std::env::var("LAYERFS_TEST_IMAGE") else {
        return;
    };
    let root = std::env::temp_dir().join(format!("layerfs-view-deadline-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("note"), b"base").unwrap();
    let server = Server::create(ServerConfig {
        store_path: root.join("store.sqlite"),
        history_path: root.join("history.sqlite"),
        binding_key: b"view-deadline".to_vec(),
        incarnation: 1,
        cursor_key: [39; 32],
        history: HistoryMode::Create,
        service_host: "host.docker.internal".into(),
        runtime: layerfs_telemetry::runtime::Runtime::disabled(),
        telemetry_run: None,
    })
    .unwrap();
    server.listen().unwrap();
    let owner = server.owner().unwrap();
    let mut cleanup = Cleanup {
        root: root.clone(),
        owner: &owner,
        sandboxes: Vec::new(),
    };
    let projects = ProjectApi::new(&server);
    let sandboxes = SandboxApi::new(&owner);
    let workspaces = WorkspaceApi::new(&owner);
    let project = projects.init("view-deadline", &source).unwrap();
    let branch = projects.fork(&project, BRANCH, "deadline").unwrap();
    let name = format!("view-deadline-{}", std::process::id());
    let sandbox = sandboxes.create(&image, &name).unwrap();
    cleanup.sandboxes.push(sandbox);
    let mount = workspaces
        .mount(sandbox, &project, branch.id, None)
        .unwrap();
    workspaces.exec(&mount.id, "printf g1-note > note").unwrap();
    let lease = workspaces.pin_view(&mount.id).unwrap();
    let note = workspaces
        .view_lookup(&lease, lease.root(), b"note")
        .unwrap();
    assert_eq!(
        workspaces.view_read(&lease, &note, 0, 64).unwrap().bytes,
        b"g1-note"
    );
    // No sleep, shortened deadline, fake clock, test hook or request retry:
    // an external paused daemon cannot reply to the SDK's real 5s deadline.
    let paused = PausedSandbox::new(&name);
    let timed_out = workspaces.view_read(&lease, &note, 0, 64);
    drop(paused);
    assert!(
        matches!(timed_out, Err(WorkspaceError::Failure(ref f)) if f.code == Code::Deadline),
        "expected the original Deadline, got {timed_out:?}"
    );
    assert_eq!(workspaces.view_status(&lease).unwrap().held_leases, 1);
    assert_eq!(
        workspaces.view_read(&lease, &note, 0, 64).unwrap().bytes,
        b"g1-note"
    );
    workspaces.commit(&mount.id).unwrap();
    workspaces.exec(&mount.id, "printf g2-note > note").unwrap();
    assert_eq!(
        workspaces.view_read(&lease, &note, 0, 64).unwrap().bytes,
        b"g1-note"
    );
    assert_eq!(
        workspaces.exec(&mount.id, "cat note").unwrap().stdout,
        b"g2-note"
    );
    assert_eq!(
        workspaces.release_view(&lease),
        Ok(layerfs_api_core::WorkspaceViewRelease::Completed)
    );
    workspaces.unmount(&mount.id).unwrap();
    sandboxes.delete(sandbox).unwrap();
    cleanup.sandboxes.clear();
    drop(cleanup);
    println!("VIEW_LEASE_DEADLINE read=Deadline g1=exact g2=exact held=1 release=Completed unmount=Completed");
}

/// External route `workspace_view_release_route.py` supplies a byte-transparent
/// control relay. It removes only the daemon's terminal checked-release reply,
/// after it has arrived at the relay; it cannot fabricate a daemon outcome.
#[test]
fn view_lease_uncertain_release_does_not_retry_or_claim_completion() {
    let (Ok(image), Ok(control)) = (
        std::env::var("LAYERFS_TEST_IMAGE"),
        std::env::var("LAYERFS_RELEASE_RELAY_CONTROL"),
    ) else {
        return;
    };
    let root = std::env::temp_dir().join(format!("layerfs-release-loss-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("note"), b"base").unwrap();
    let server = Server::create(ServerConfig {
        store_path: root.join("store.sqlite"),
        history_path: root.join("history.sqlite"),
        binding_key: b"release-loss".to_vec(),
        incarnation: 1,
        cursor_key: [40; 32],
        history: HistoryMode::Create,
        service_host: "host.docker.internal".into(),
        runtime: layerfs_telemetry::runtime::Runtime::disabled(),
        telemetry_run: None,
    })
    .unwrap();
    server.listen().unwrap();
    let owner = server.owner().unwrap();
    let mut cleanup = Cleanup {
        root: root.clone(),
        owner: &owner,
        sandboxes: Vec::new(),
    };
    let projects = ProjectApi::new(&server);
    let sandboxes = SandboxApi::new(&owner);
    let workspaces = WorkspaceApi::new(&owner);
    let project = projects.init("release-loss", &source).unwrap();
    let branch = projects.fork(&project, BRANCH, "release-loss").unwrap();
    let sandbox = sandboxes
        .create(&image, &format!("view-release-loss-{}", std::process::id()))
        .unwrap();
    cleanup.sandboxes.push(sandbox);
    let mount = workspaces
        .mount(sandbox, &project, branch.id, None)
        .unwrap();
    workspaces.exec(&mount.id, "printf g1-note > note").unwrap();
    let lease = workspaces.pin_view(&mount.id).unwrap();
    let note = workspaces
        .view_lookup(&lease, lease.root(), b"note")
        .unwrap();
    assert_eq!(
        workspaces.view_read(&lease, &note, 0, 64).unwrap().bytes,
        b"g1-note"
    );
    workspaces.commit(&mount.id).unwrap();
    workspaces.exec(&mount.id, "printf g2-note > note").unwrap();
    assert_eq!(
        workspaces.view_read(&lease, &note, 0, 64).unwrap().bytes,
        b"g1-note"
    );
    assert_eq!(workspaces.view_status(&lease).unwrap().held_leases, 1);
    // ARM is acknowledged only after the relay has selected the *current*
    // authenticated SDK session; no timing sleep or changed product deadline.
    use std::io::{Read, Write};
    let mut command = std::net::TcpStream::connect(control).unwrap();
    command.write_all(b"ARM\n").unwrap();
    command.shutdown(std::net::Shutdown::Write).unwrap();
    let mut acknowledged = String::new();
    command.read_to_string(&mut acknowledged).unwrap();
    assert_eq!(acknowledged, "ARMED\n");
    let result = workspaces.release_view(&lease);
    assert!(
        matches!(&result, Err(WorkspaceError::Failure(f)) if f.code == Code::Unknown && f.unknown),
        "a lost checked-release reply is never Completed: {result:?}"
    );
    // The relay proves it received and withheld a terminal encrypted result.
    // A separate *read-only* query determines what the remote authority did;
    // it is not a second attempt to release an uncertain token.
    let observed = workspaces.view_status(&lease);
    assert!(
        matches!(&observed, Err(WorkspaceError::Failure(f)) if f.code == Code::Denied),
        "remote token after withheld completed release: {observed:?}"
    );
    assert_eq!(
        workspaces.exec(&mount.id, "cat note").unwrap().stdout,
        b"g2-note"
    );
    workspaces.unmount(&mount.id).unwrap();
    sandboxes.delete(sandbox).unwrap();
    cleanup.sandboxes.clear();
    drop(cleanup);
    println!("VIEW_LEASE_RELEASE_LOSS checked_release=Unknown remote_token=Denied g1=exact g2=exact no_retry=true unmount=Completed");
}
