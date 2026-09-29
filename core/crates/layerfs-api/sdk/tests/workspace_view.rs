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

/// One external authenticated host-Service relay gates the Commit ciphertext
/// only after a read-only check finds the canonical branch head published.
/// The test process, not the daemon, applies/restores the Linux file-size fault.
fn c5_control(endpoint: &str, command: &str) -> String {
    use std::io::{Read, Write};
    let mut socket = std::net::TcpStream::connect(endpoint).unwrap();
    socket.write_all(format!("{command}\n").as_bytes()).unwrap();
    socket.shutdown(std::net::Shutdown::Write).unwrap();
    let mut result = String::new();
    socket.read_to_string(&mut result).unwrap();
    result
}

struct C5Limit(String);
impl C5Limit {
    fn apply(name: &str, helper: &str) -> Self {
        let docker = |args: &[&str]| {
            let result = std::process::Command::new("docker")
                .args(args)
                .output()
                .unwrap();
            assert!(result.status.success(), "docker {args:?}: {result:?}");
            String::from_utf8(result.stdout).unwrap()
        };
        let names = docker(&[
            "ps",
            "--filter",
            "label=io.layerfs.owner=agent-sdk",
            "--filter",
            &format!("label=io.layerfs.sandbox-name={name}"),
            "--format",
            "{{.ID}}",
        ]);
        assert_eq!(names.lines().count(), 1, "only the owned daemon is faulted");
        let container = names.trim().to_owned();
        // Docker cp refuses read-only rootfs; stream the owned helper into
        // the mounted volume instead (the tmpfs is noexec in this profile).
        use std::io::Write;
        let mut copy = std::process::Command::new("docker")
            .args([
                "exec",
                "-i",
                "--privileged",
                &container,
                "/bin/sh",
                "-c",
                "cat > /layerfs/.c5-limit-test && chmod 500 /layerfs/.c5-limit-test",
            ])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        copy.stdin
            .take()
            .unwrap()
            .write_all(&std::fs::read(helper).unwrap())
            .unwrap();
        assert!(
            copy.wait().unwrap().success(),
            "install owned Linux fault helper"
        );
        let profile = docker(&[
            "exec",
            "--privileged",
            &container,
            "/layerfs/.c5-limit-test",
            "1",
            "2048",
        ]);
        assert!(profile.contains("new=2048"), "{profile}");
        Self(container)
    }
}
impl Drop for C5Limit {
    fn drop(&mut self) {
        let result = std::process::Command::new("docker")
            .args([
                "exec",
                "--privileged",
                &self.0,
                "/layerfs/.c5-limit-test",
                "1",
                "max",
            ])
            .output()
            .unwrap();
        assert!(result.status.success(), "restore file limit: {result:?}");
        let removed = std::process::Command::new("docker")
            .args(["exec", &self.0, "/bin/rm", "-f", "/layerfs/.c5-limit-test"])
            .output()
            .unwrap();
        assert!(
            removed.status.success(),
            "remove external helper: {removed:?}"
        );
    }
}

#[test]
fn view_lease_known_c1_local_c5_failure() {
    use layerfs_bridge::contract::{
        CommitOutcomeWire, WorkspaceCommitFailureDisposition, WorkspaceCommitPhase,
    };
    let (Ok(image), Ok(control), Ok(host_ip), Ok(helper)) = (
        std::env::var("LAYERFS_TEST_IMAGE"),
        std::env::var("LAYERFS_C5_CONTROL"),
        std::env::var("LAYERFS_C5_HOST_IP"),
        std::env::var("LAYERFS_C5_HELPER"),
    ) else {
        return;
    };
    let root = std::env::temp_dir().join(format!("layerfs-view-c5-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("note"), b"base").unwrap();
    let server = Server::create(ServerConfig {
        store_path: root.join("store.sqlite"),
        history_path: root.join("history.sqlite"),
        binding_key: b"view-c5".to_vec(),
        incarnation: 1,
        cursor_key: [41; 32],
        history: HistoryMode::Create,
        service_host: host_ip,
        runtime: layerfs_telemetry::runtime::Runtime::disabled(),
        telemetry_run: None,
    })
    .unwrap();
    let endpoint = server.listen().unwrap();
    assert_eq!(
        c5_control(&control, &format!("BIND {}", endpoint.port())),
        "BOUND\n"
    );
    let owner = server.owner().unwrap();
    let mut cleanup = Cleanup {
        root: root.clone(),
        owner: &owner,
        sandboxes: Vec::new(),
    };
    let projects = ProjectApi::new(&server);
    let sandboxes = SandboxApi::new(&owner);
    let workspaces = WorkspaceApi::new(&owner);
    let project = projects.init("view-c5", &source).unwrap();
    let branch = projects.fork(&project, BRANCH, "view-c5").unwrap();
    let name = format!("view-c5-{}", std::process::id());
    let sandbox = sandboxes.create(&image, &name).unwrap();
    cleanup.sandboxes.push(sandbox);
    let mount = workspaces
        .mount(sandbox, &project, branch.id, None)
        .unwrap();
    workspaces.exec(&mount.id, "printf g1-note > note").unwrap();
    let lease = workspaces.pin_view(&mount.id).unwrap();
    let entry = workspaces
        .view_lookup(&lease, lease.root(), b"note")
        .unwrap();
    assert_eq!(
        workspaces.view_read(&lease, &entry, 0, 64).unwrap().bytes,
        b"g1-note"
    );
    let pinned = (lease.generation(), lease.revision());
    assert_eq!(
        c5_control(
            &control,
            &format!("ARM {}", root.join("history.sqlite").display())
        ),
        "ARMED\n"
    );
    std::thread::scope(|scope| {
        let committing = scope.spawn(|| workspaces.commit(&mount.id));
        let gated = c5_control(&control, "WAIT");
        let head = gated.strip_prefix("GATED ").unwrap().trim_end().to_owned();
        let limit = C5Limit::apply(&name, &helper);
        assert_eq!(c5_control(&control, "GO"), "RELEASED\n");
        let result = committing.join().unwrap();
        drop(limit);
        let Err(WorkspaceError::Commit(failure)) = result else {
            panic!("expected known canonical/local physical failure: {result:?}")
        };
        assert_eq!(
            failure.disposition,
            WorkspaceCommitFailureDisposition::KnownCommitLocalFailure
        );
        assert_eq!(failure.phase, WorkspaceCommitPhase::Reconcile);
        assert!(failure.installed_revision.is_none());
        assert_eq!(failure.cause.code, Code::Io);
        assert!(!failure.cause.unknown);
        let Some(CommitOutcomeWire::Committed(committed)) = &failure.known_outcome else {
            panic!("must preserve a known canonical Commit: {failure:?}")
        };
        use std::fmt::Write as _;
        let mut actual_head = String::new();
        for byte in committed.commit {
            write!(&mut actual_head, "{byte:02x}").unwrap();
        }
        assert_eq!(
            actual_head, head,
            "relay gate must match SDK's known C1 Commit"
        );
        assert_eq!(failure.observed_outcome, failure.known_outcome);
        eprintln!("VIEW_LEASE_C5_COMMIT {failure:?}");
    });
    let status = workspaces.view_status(&lease).unwrap();
    assert_eq!((status.generation, status.revision), pinned);
    assert_eq!(status.held_leases, 1);
    assert_eq!(
        workspaces.view_read(&lease, &entry, 0, 64).unwrap().bytes,
        b"g1-note"
    );
    // A failed local installation must never be disguised as a successful
    // clean close. Cleanup is owner-scoped even when a stopped C5 refuses it.
    let release = workspaces.release_view(&lease);
    eprintln!("VIEW_LEASE_C5_RELEASE {release:?}");
    let unmount = workspaces.unmount(&mount.id);
    eprintln!("VIEW_LEASE_C5_UNMOUNT {unmount:?}");
    let mut logs = Vec::new();
    let (removed, capture) = sandboxes.delete_with_logs(sandbox, &mut logs);
    assert!(removed.is_ok(), "remove owned stopped daemon: {removed:?}");
    assert!(
        capture.error.is_none() && !capture.truncated,
        "daemon stderr capture: {capture:?}"
    );
    assert!(
        String::from_utf8_lossy(&logs).contains("sandbox shutdown retained: Busy"),
        "C5 physical fault must not be mislabeled clean close: {}",
        String::from_utf8_lossy(&logs)
    );
    cleanup.sandboxes.clear();
    drop(cleanup);
    println!("VIEW_LEASE_C5 canonical=Committed phase=Reconcile disposition=KnownCommitLocalFailure pinned_g1=exact held=1");
}

/// Standalone POSIX workload installed in the owned daemon's volume (not
/// the Workspace FUSE mount); removed before deletion. No product hook.
struct BudgetWriter(String);
impl BudgetWriter {
    fn install(name: &str, binary: &str) -> Self {
        use std::io::Write;
        let find = std::process::Command::new("docker")
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
        assert!(find.status.success());
        let ids = String::from_utf8(find.stdout).unwrap();
        assert_eq!(ids.lines().count(), 1);
        let container = ids.trim().to_owned();
        let mut copy = std::process::Command::new("docker")
            .args([
                "exec",
                "-i",
                &container,
                "/bin/sh",
                "-c",
                "cat > /layerfs/.budget-writer-test && chmod 500 /layerfs/.budget-writer-test",
            ])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        copy.stdin
            .take()
            .unwrap()
            .write_all(&std::fs::read(binary).unwrap())
            .unwrap();
        assert!(copy.wait().unwrap().success());
        Self(container)
    }
}
impl Drop for BudgetWriter {
    fn drop(&mut self) {
        let removed = std::process::Command::new("docker")
            .args([
                "exec",
                &self.0,
                "/bin/rm",
                "-f",
                "/layerfs/.budget-writer-test",
            ])
            .output()
            .unwrap();
        assert!(
            removed.status.success(),
            "remove external workload: {removed:?}"
        );
    }
}

/// Actual public SDK response-budget pressure on the unchanged 16 MiB sandbox.
/// Accepted POSIX writes, rather than a test-only Budget handle or a changed
/// container limit, must occupy the Workspace before the large pinned read.
#[test]
fn view_lease_read_refuses_exhausted_response_budget_without_partial_entry() {
    let (Ok(image), Ok(binary)) = (
        std::env::var("LAYERFS_TEST_IMAGE"),
        std::env::var("LAYERFS_BUDGET_WRITER"),
    ) else {
        return;
    };
    const BUDGET: u64 = 16 * 1024 * 1024;
    const READ: usize = 128 * 1024;
    let root = std::env::temp_dir().join(format!("layerfs-view-budget-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("large"), vec![b'Q'; READ]).unwrap();
    // Prepared sparse 64 MiB base (the mounted writes still pay for all edits).
    std::fs::File::create(source.join("work"))
        .unwrap()
        .set_len(64 * 1024 * 1024)
        .unwrap();
    let server = Server::create(ServerConfig {
        store_path: root.join("store.sqlite"),
        history_path: root.join("history.sqlite"),
        binding_key: b"view-budget".to_vec(),
        incarnation: 1,
        cursor_key: [42; 32],
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
    let project = projects.init("view-budget", &source).unwrap();
    let branch = projects.fork(&project, BRANCH, "view-budget").unwrap();
    let name = format!("view-budget-proof-{}", std::process::id());
    let sandbox = sandboxes.create(&image, &name).unwrap();
    cleanup.sandboxes.push(sandbox);
    let mount = workspaces
        .mount(sandbox, &project, branch.id, None)
        .unwrap();
    let lease = workspaces.pin_view(&mount.id).unwrap();
    let large = workspaces
        .view_lookup(&lease, lease.root(), b"large")
        .unwrap();
    assert_eq!(
        workspaces.view_read(&lease, &large, 0, 16).unwrap().bytes,
        vec![b'Q'; 16]
    );
    let initial = workspaces
        .status(&mount.id)
        .unwrap()
        .consumer_accounted_bytes;
    eprintln!("VIEW_BUDGET start={initial} limit={BUDGET} response={READ}");
    let writer = BudgetWriter::install(&name, &binary);
    // Each Exec performs real pwrite calls through the mounted FUSE path.
    // No direct backing access, changed limit or reserved test-only Budget.
    let mut observed = initial;
    let mut accepted = 0usize;
    // Stop with ~1 MiB of Budget. One public pin of the resulting view
    // exposes already-created entries; their charged registrations consume
    // the remaining Budget without attempting a speculative further WRITE.
    for batch in 0..20 {
        if observed >= BUDGET - 1_100_000 {
            break;
        }
        let spare = (BUDGET - 1_100_000).saturating_sub(observed);
        let count = ((spare / 6000) as usize).clamp(1, 1024);
        let command = format!("/layerfs/.budget-writer-test create {accepted} {count}");
        let result = workspaces.exec(&mount.id, &command).unwrap();
        let next = workspaces
            .status(&mount.id)
            .unwrap()
            .consumer_accounted_bytes;
        eprintln!("VIEW_BUDGET batch={batch} files={count} accepted_before={accepted} exit={:?} stderr={:?} used={next} remaining={}", result.exit_status, result.stderr, BUDGET.saturating_sub(next));
        assert_eq!(
            result.exit_status,
            Some(0),
            "ordinary names refused before response Budget"
        );
        accepted += count;
        observed = next;
    }
    assert!(
        accepted > 1500 && observed >= BUDGET - 1_100_000,
        "public namespace must reach near-full Budget before pinning"
    );
    let second = workspaces.pin_view(&mount.id).unwrap();
    let second_pinned = (second.generation(), second.revision());
    for index in 0..accepted {
        let byte = b'a' + (index % 25) as u8;
        let name = format!("f{index:06}-{}", char::from(byte).to_string().repeat(180));
        let issued_before = workspaces.view_status(&second).unwrap().entries;
        let resolved = workspaces.view_lookup(&second, second.root(), name.as_bytes());
        if matches!(&resolved, Err(WorkspaceError::Failure(f)) if f.code == Code::Capacity && !f.unknown)
        {
            assert_eq!(
                workspaces.view_status(&second).unwrap().entries,
                issued_before,
                "failed entry registration must be atomic"
            );
            observed = workspaces
                .status(&mount.id)
                .unwrap()
                .consumer_accounted_bytes;
            eprintln!(
                "VIEW_BUDGET_LOOKUP_CAPACITY index={index} used={observed} remaining={}",
                BUDGET.saturating_sub(observed)
            );
            break;
        }
        assert!(
            resolved.is_ok(),
            "lookup failed before Budget refusal: {resolved:?}"
        );
        if index % 64 == 63 || index + 1 == accepted {
            observed = workspaces
                .status(&mount.id)
                .unwrap()
                .consumer_accounted_bytes;
            eprintln!(
                "VIEW_BUDGET entries={} used={} remaining={}",
                index + 1,
                observed,
                BUDGET.saturating_sub(observed)
            );
            if observed > BUDGET - READ as u64 {
                break;
            }
        }
    }
    assert!(
        observed > BUDGET - 2 * READ as u64,
        "real issued entries did not exhaust the fixed response-and-call headroom"
    );
    assert_eq!(
        workspaces.view_status(&second).unwrap().generation,
        second_pinned.0
    );
    let entries_before = workspaces.view_status(&lease).unwrap().entries;
    let response = workspaces.view_read(&lease, &large, 0, READ);
    eprintln!("VIEW_BUDGET_RESPONSE used={observed} outcome={response:?}");
    assert!(
        matches!(&response, Err(WorkspaceError::Failure(f)) if f.code == Code::Capacity && !f.unknown)
    );
    assert_eq!(
        workspaces.view_status(&lease).unwrap().entries,
        entries_before
    );
    assert_eq!(
        workspaces.view_read(&lease, &large, 0, 1).unwrap().bytes,
        b"Q"
    );
    workspaces.release_view(&second).unwrap();
    workspaces.release_view(&lease).unwrap();
    workspaces.unmount(&mount.id).unwrap();
    drop(writer);
    sandboxes.delete(sandbox).unwrap();
    cleanup.sandboxes.clear();
    drop(cleanup);
    println!("VIEW_BUDGET fixed=16777216 response=131072 capacity_before_bytes=true no_partial_entry=true");
}
