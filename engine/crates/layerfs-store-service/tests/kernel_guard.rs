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
    use std::os::unix::fs::DirBuilderExt;
    let directory = parent.join(format!("layerfs-guard-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)
        .unwrap();
    directory
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn missing_store_directory_is_refused_without_creating_it_and_non_private_directory_is_refused() {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let parent = scratch(&std::env::temp_dir(), "private-mode");
    let missing = parent.join("global");
    assert!(!missing.exists());
    let refused_missing = layerfs_store_service::kernel_guard::check(&missing.join("store.sqlite"))
        .expect_err("missing directory must be refused without creating it");
    assert!(
        refused_missing.0.contains(&missing.display().to_string()),
        "{refused_missing}"
    );
    assert!(
        !missing.exists(),
        "kernel_guard::check must not create a missing directory"
    );
    let template = parent.join("overlay.sqlite");
    let open_missing = StoreService::open_direct(
        &missing.join("store.sqlite"),
        b"binding",
        [7; 32],
        &template,
    );
    match open_missing {
        Err(OpenRefusal::Unsupported(_)) => {}
        Err(other) => panic!("expected OpenRefusal::Unsupported, got {other:?}"),
        Ok(_) => panic!("expected OpenRefusal::Unsupported, Store opened"),
    }
    assert!(
        !missing.exists(),
        "open_direct on a missing Store directory must not leave an empty directory"
    );

    // An existing 0700 directory passes, even for a non-.sqlite filename.
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&missing)
        .unwrap();
    let checked = layerfs_store_service::kernel_guard::check(&missing.join("db"));
    assert!(checked.is_ok(), "{checked:?}");

    // An existing directory with group/other access (e.g. 0755 from default umask)
    // is refused up front with its path, current mode, expected 0700 and chmod command,
    // without modifying the user's directory permissions.
    std::fs::set_permissions(&missing, std::fs::Permissions::from_mode(0o755)).unwrap();
    let refused = layerfs_store_service::kernel_guard::check(&missing.join("store.sqlite"))
        .expect_err("0755 directory must be refused");
    assert!(
        refused.0.contains(&missing.display().to_string()),
        "{refused}"
    );
    assert!(refused.0.contains("0755"), "{refused}");
    assert!(refused.0.contains("0700"), "{refused}");
    assert!(
        refused
            .0
            .contains(&format!("chmod 700 {}", missing.display())),
        "{refused}"
    );
    let unchanged = std::fs::metadata(&missing).unwrap().permissions().mode() & 0o7777;
    assert_eq!(unchanged, 0o755);
    std::fs::remove_dir_all(parent).unwrap();
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

    let corrupt = fixture.directory.join("corrupt.sqlite");
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

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn three_processes_open_direct_and_commit_concurrently() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let fixture = installed::Fixture::new(2, "concurrent-open");
    let sock_path = fixture.directory.join("barrier.sock");
    let listener = UnixListener::bind(&sock_path).unwrap();
    listener.set_nonblocking(true).unwrap();

    let exe = std::env::current_exe().unwrap();
    let mut children = Vec::new();
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
        children.push(child);
    }
    let mut sockets = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while sockets.len() < 3 {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                let mut ready = [0u8; 1];
                stream.read_exact(&mut ready).unwrap();
                assert_eq!(&ready, b"R");
                sockets.push(stream);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                std::thread::yield_now();
            }
            Err(e) => panic!("barrier accept: {e}"),
        }
    }
    for mut stream in sockets {
        stream.write_all(b"G").unwrap();
    }
    let mut failures = Vec::new();
    for (id, child) in children.into_iter().enumerate() {
        let output = child.wait_with_output().unwrap();
        if !output.status.success() {
            failures.push(format!(
                "agent-{id}: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "concurrent open_direct + 20 commits failed:\n{}",
        failures.join("\n")
    );
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
