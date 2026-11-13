//! Direct Store access is refused `Unsupported` where the Store's locks do
//! not reach every opener (#352 F-020): off Linux and macOS, and on Linux for
//! a filesystem outside the allowlist (here tmpfs). The refusal comes before
//! any file is opened: the Store file's bytes are unchanged and no overlay
//! template is created. On macOS a local apfs or hfs directory is allowed.
use layerfs_store_service::{OpenRefusal, StoreService};
use std::path::{Path, PathBuf};

#[cfg(not(target_os = "macos"))]
fn refused(directory: &Path) -> (String, Vec<u8>, Vec<u8>) {
    let store = directory.join("store.sqlite");
    std::fs::write(&store, b"not opened").unwrap();
    let template = directory.join("overlay.sqlite");
    let before = std::fs::read(&store).unwrap();
    let outcome = StoreService::open_direct(&store, b"binding", [7; 32], &template);
    let reason = match outcome {
        Err(OpenRefusal::Unsupported(reason)) => reason.0,
        Err(other) => panic!("expected Unsupported, got {other:?}"),
        Ok(_) => panic!("expected Unsupported, the Store opened"),
    };
    assert!(
        !template.exists(),
        "no overlay template before the guard passes"
    );
    (reason, before, std::fs::read(&store).unwrap())
}

fn scratch(parent: &Path, label: &str) -> PathBuf {
    let directory = parent.join(format!("layerfs-guard-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

#[cfg(target_os = "macos")]
#[test]
fn direct_access_is_allowed_on_a_local_macos_filesystem() {
    let directory = scratch(&std::env::temp_dir(), "apfs");
    let checked = layerfs_store_service::kernel_guard::check(&directory.join("store.sqlite"));
    assert!(matches!(checked, Ok("apfs") | Ok("hfs")), "{checked:?}");
    std::fs::remove_dir_all(directory).unwrap();
}

#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;

/// An open that finds the Store held by another writer's unwritten transaction
/// is a definite pre-effect `Busy` the caller may repeat; any other open
/// failure stays `Failed`.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn open_direct_reports_busy_when_locked_and_failed_when_corrupt() {
    let fixture = installed::Fixture::new(1, "open-refusal");
    let sql = rusqlite::Connection::open(&fixture.config.path).unwrap();
    sql.busy_timeout(std::time::Duration::ZERO).unwrap();
    sql.execute_batch("BEGIN IMMEDIATE").unwrap();
    let template = fixture.directory.join("overlay.sqlite");
    let busy = StoreService::open_direct(
        &fixture.config.path,
        installed::BINDING,
        installed::CURSOR,
        &template,
    );
    match busy {
        Err(OpenRefusal::Busy(reason)) => {
            assert_eq!(reason, "Store open: Store busy before effect");
        }
        Err(other) => panic!("expected OpenRefusal::Busy, got {other:?}"),
        Ok(_) => panic!("expected OpenRefusal::Busy, Store opened"),
    }
    sql.execute_batch("ROLLBACK").unwrap();
    drop(sql);

    let corrupt = fixture.store_directory().join("corrupt.sqlite");
    std::fs::write(&corrupt, b"not a valid sqlite store").unwrap();
    let failed =
        StoreService::open_direct(&corrupt, installed::BINDING, installed::CURSOR, &template);
    match failed {
        Err(OpenRefusal::Failed(reason)) => assert!(reason.starts_with("Store open:"), "{reason}"),
        Err(other) => panic!("expected OpenRefusal::Failed, got {other:?}"),
        Ok(_) => panic!("expected OpenRefusal::Failed, Store opened"),
    }
    fixture.cleanup();
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn open_concurrent_child() {
    let Some(store) = std::env::var_os("LAYERFS_OPEN_CONCURRENT_STORE") else {
        return;
    };
    use layerfs_bridge::control::{SdkCall, SdkValue};
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    let id = std::env::var("LAYERFS_OPEN_CONCURRENT_ID").unwrap();
    let template =
        std::path::PathBuf::from(std::env::var_os("LAYERFS_OPEN_CONCURRENT_TPL").unwrap());
    let mut sock =
        UnixStream::connect(std::env::var_os("LAYERFS_OPEN_CONCURRENT_SOCK").unwrap()).unwrap();
    sock.set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .unwrap();
    sock.set_write_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    sock.write_all(b"R").unwrap();
    let mut go = [0u8; 1];
    sock.read_exact(&mut go).unwrap();
    let service = StoreService::open_direct(
        Path::new(&store),
        installed::BINDING,
        installed::CURSOR,
        &template,
    )
    .unwrap_or_else(|e| panic!("agent-{id} open_direct: {e:?}"));
    let call = |method: &str, fields: Vec<(&str, SdkValue)>| {
        let mut map: Vec<(String, SdkValue)> =
            fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
        map.push(("project".to_owned(), SdkValue::text("installed")));
        layerfs_store_service::methods::call(
            &service,
            &SdkCall {
                method: method.to_owned(),
                args: SdkValue::Map(map),
            },
        )
        .unwrap_or_else(|e| panic!("agent-{id} {method}: {e:?}"))
    };
    let branch = format!("agent-{id}");
    call("branch.fork", vec![("name", SdkValue::text(&branch))]);
    for step in 0..20 {
        call(
            "fs.write",
            vec![
                ("branch", SdkValue::text(&branch)),
                ("path", SdkValue::text(format!("/agent-{id}-{step}.txt"))),
                (
                    "bytes",
                    SdkValue::Bytes(format!("step {step}\n").into_bytes()),
                ),
                (
                    "message",
                    SdkValue::text(format!("agent-{id} commit {step}")),
                ),
            ],
        );
    }
    service.close().unwrap();
}

/// The test children of one concurrency case, owned as one group: the wait
/// carries a single deadline for the whole set, a child that outlives it or
/// exits unsuccessfully fails the case, and dropping the group while any child
/// still runs stops it. A lane deadlock therefore fails the case instead of
/// hanging it or leaving a live child that holds the Store, and the failure
/// names the child that hung together with what the group wrote to stderr.
#[cfg(any(target_os = "linux", target_os = "macos"))]
struct Children(Vec<(String, std::process::Child)>);

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Children {
    fn new() -> Self {
        Self(Vec::new())
    }
    fn push(&mut self, what: String, child: std::process::Child) {
        self.0.push((what, child));
    }
    /// Waits for every child, `seconds` for the group as a whole; the 60 s
    /// ceiling stays under the per-invocation test limit. Any child failure or
    /// timeout stops the entire group (`kill` + `wait`) and attaches every
    /// child's stderr to the failure panic.
    fn wait_all(mut self, seconds: u64) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
        let mut early_failure = false;
        let mut late = Vec::new();
        loop {
            let mut any_running = false;
            for (what, child) in self.0.iter_mut() {
                match child.try_wait().unwrap() {
                    Some(status) if !status.success() => {
                        early_failure = true;
                    }
                    Some(_) => {}
                    None => {
                        any_running = true;
                        if std::time::Instant::now() >= deadline {
                            late.push(what.clone());
                        }
                    }
                }
            }
            if early_failure || !late.is_empty() || !any_running {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if early_failure || !late.is_empty() {
            self.stop();
        }
        let details = self.collect_details();
        assert!(
            late.is_empty(),
            "the group's deadline passed with {late:?} still running\n{}",
            details
                .iter()
                .map(|(line, _)| line.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert!(
            !early_failure && details.iter().all(|(_, ok)| *ok),
            "concurrent open_direct + 20 commits failed:\n{}",
            details
                .into_iter()
                .map(|(line, _)| line)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    fn collect_details(&mut self) -> Vec<(String, bool)> {
        let mut details = Vec::new();
        for (what, child) in std::mem::take(&mut self.0) {
            let output = child.wait_with_output().unwrap();
            let stderr = String::from_utf8_lossy(&output.stderr);
            let line = format!("{what}: {}\n{}", output.status, stderr.trim_end());
            details.push((line, output.status.success()));
        }
        details
    }
    /// Stops every child and panics with `reason` plus each child's stderr.
    fn abort(mut self, reason: impl std::fmt::Display) -> ! {
        self.stop();
        let details = self
            .collect_details()
            .into_iter()
            .map(|(line, _)| line)
            .collect::<Vec<_>>();
        panic!("{reason}\n{}", details.join("\n"));
    }
    /// Stops every child of the group, exited or not.
    fn stop(&mut self) {
        for (_, child) in self.0.iter_mut() {
            let _ = child.kill();
        }
        for (_, child) in self.0.iter_mut() {
            let _ = child.wait();
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Drop for Children {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Three processes open the same Store directly and each commits twenty times
/// at once: opening waits on the directory lane instead of failing, and no
/// Commit is lost or doubled.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn three_processes_open_direct_and_commit_concurrently() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    // A short fixture label and socket name: `sockaddr_un.sun_path` is 104 bytes on
    // Darwin (103 of path), and this directory sits under the host's TMPDIR.
    let fixture = installed::Fixture::new(2, "co");
    let sock_path = fixture.directory.join("s");
    let listener = UnixListener::bind(&sock_path).unwrap();
    listener.set_nonblocking(true).unwrap();

    let exe = std::env::current_exe().unwrap();
    let mut children = Children::new();
    for id in 0..3 {
        let overlay_dir = fixture.directory.join(format!("overlay-{id}"));
        std::fs::create_dir(&overlay_dir).unwrap();
        let tpl = overlay_dir.join("overlay.sqlite");
        let child = Command::new(&exe)
            .args(["--exact", "open_concurrent_child", "--nocapture"])
            .env("LAYERFS_OPEN_CONCURRENT_STORE", &fixture.config.path)
            .env("LAYERFS_OPEN_CONCURRENT_ID", id.to_string())
            .env("LAYERFS_OPEN_CONCURRENT_TPL", &tpl)
            .env("LAYERFS_OPEN_CONCURRENT_SOCK", &sock_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        children.push(format!("agent-{id}"), child);
    }
    let mut sockets = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while sockets.len() < 3 {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                // The handshake itself is bounded too: a child that connects
                // and never writes `R` must fail the case, not wait forever.
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut ready = [0u8; 1];
                if let Err(e) = stream.read_exact(&mut ready) {
                    children.abort(format!("barrier read: {e}"));
                }
                if &ready != b"R" {
                    children.abort(format!("unexpected barrier byte: {ready:?}"));
                }
                sockets.push(stream);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(e) => children.abort(format!("barrier accept: {e}")),
        }
    }
    for mut stream in sockets {
        if let Err(e) = stream.write_all(b"G") {
            children.abort(format!("barrier write: {e}"));
        }
    }
    children.wait_all(60);
    let verify_dir = fixture.directory.join("overlay-verify");
    std::fs::create_dir(&verify_dir).unwrap();
    let service = StoreService::open_direct(
        &fixture.config.path,
        installed::BINDING,
        installed::CURSOR,
        &verify_dir.join("overlay.sqlite"),
    )
    .unwrap();
    for id in 0..3 {
        let branch = format!("agent-{id}");
        let list = layerfs_store_service::methods::call(
            &service,
            &layerfs_bridge::control::SdkCall {
                method: "branch.list_commits".to_owned(),
                args: layerfs_bridge::control::SdkValue::map([
                    (
                        "project",
                        layerfs_bridge::control::SdkValue::text("installed"),
                    ),
                    ("branch", layerfs_bridge::control::SdkValue::text(&branch)),
                    ("pageSize", layerfs_bridge::control::SdkValue::Int(50)),
                ]),
            },
        )
        .unwrap();
        let Some(layerfs_bridge::control::SdkValue::List(items)) = list.get("items") else {
            panic!("missing items: {list:?}");
        };
        assert_eq!(items.len(), 20, "branch {branch} commit count");
    }
    service.close().unwrap();
    fixture.cleanup();
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[test]
fn direct_access_is_unsupported_off_linux() {
    let directory = scratch(&std::env::temp_dir(), "host");
    let (reason, before, after) = refused(&directory);
    assert!(reason.contains("Linux"), "{reason}");
    assert_eq!(before, after);
    std::fs::remove_dir_all(directory).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn direct_access_is_unsupported_on_a_filesystem_outside_the_allowlist() {
    let directory = scratch(Path::new("/dev/shm"), "tmpfs");
    let (reason, before, after) = refused(&directory);
    assert!(reason.contains("ext4, xfs or btrfs"), "{reason}");
    assert_eq!(before, after);
    std::fs::remove_dir_all(directory).unwrap();
}
