//! The docker machine's Engine socket: chosen automatically only until its
//! first start, which records it (so a later `DOCKER_HOST` or context does
//! not move it); `machine update docker --docker ""` makes it automatic
//! again; a machine that ran before sockets were recorded keeps Docker's
//! default socket. One test: it owns this process's environment.
#![cfg(all(feature = "machine", target_os = "macos"))]
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home;
use layerfs_machine::worker::{read_frame_within, write_frame, MAX_HELLO};
use layerfs_machine::{
    docker_socket, ensure_built_ins, record::MachineRecord, running, update, MachineUpdate, Related,
};
use std::fs;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

fn none(_: &Path, _: &str) -> Result<Vec<String>, SdkRefusal> {
    Ok(Vec::new())
}

fn free(_: &Path, _: &str) -> Result<(), SdkRefusal> {
    Ok(())
}

const RELATED: Related = Related {
    stores_on: none,
    postgresql_on: none,
    sandboxes_on: none,
    can_remove: free,
    can_stop: free,
    can_change: free,
};

fn set_docker(home: &Path, value: &str) -> Result<SdkValue, SdkRefusal> {
    let request = MachineUpdate {
        name: "docker".into(),
        cpus: None,
        memory: None,
        image: None,
        docker: Some(value.into()),
        wsl: None,
        daemon: None,
    };
    update(home, &request, RELATED)
}

fn chosen(home: &Path) -> (PathBuf, String) {
    docker_socket(home).unwrap()
}

/// A worker of another build on a loopback port: the start reaches it and
/// stops there, as `start_compatibility.rs`'s does.
fn worker(stop: Arc<AtomicBool>) -> (u16, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let thread = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(8);
        while !stop.load(Ordering::Relaxed) && Instant::now() < until {
            let (mut stream, _) = match listener.accept() {
                Ok(stream) => stream,
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
            };
            stream.set_nonblocking(false).unwrap();
            let second = Some(Duration::from_secs(1));
            stream.set_read_timeout(second).unwrap();
            stream.set_write_timeout(second).unwrap();
            if read_frame_within(&mut stream, MAX_HELLO)
                .ok()
                .flatten()
                .is_none()
            {
                continue;
            }
            let hello = SdkValue::map([
                ("ready", SdkValue::Bool(true)),
                ("protocol", SdkValue::Int(2)),
                ("bundle", SdkValue::text("abcd")),
            ]);
            let _ = write_frame(&mut stream, &hello);
        }
    });
    (port, thread)
}

#[test]
fn the_first_start_records_the_socket_and_an_empty_docker_setting_clears_it() {
    let root = PathBuf::from(format!("/tmp/lfs-dsc-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::create_dir_all(root.join("user")).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let (port, server) = worker(Arc::clone(&stop));
    let script = format!(
        r#"#!/bin/sh
echo "$*" >> '{root}/calls'
case "$3" in
  inspect) echo 'No such object' >&2; exit 1 ;;
  run) [ -n "$LFS_FAIL_RUN" ] && exit 1; echo fake-owned-container ;;
  port) echo '127.0.0.1:{port}' ;;
  exec) case "$*" in
    *'/proc/net/tcp'*) printf 'alive\n0: 00000000:1B9E 00000000:0000 0A\n' ;;
    *) cat >/dev/null ;;
  esac ;;
esac
"#,
        root = root.display()
    );
    let docker = root.join("bin/docker");
    fs::write(&docker, script).unwrap();
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).unwrap();
    let bundle = root.join("bundle");
    fs::create_dir(&bundle).unwrap();
    for name in ["layerfs", "layerfs-daemon", "layerfs-sandbox-helper"] {
        fs::write(bundle.join(name), b"").unwrap();
    }
    fs::write(
        bundle.join("bundle.json"),
        r#"{"hash":"abcd","protocol":3}"#,
    )
    .unwrap();
    let path = std::env::var_os("PATH").unwrap();
    let saved: Vec<(&str, Option<std::ffi::OsString>)> = [
        "HOME",
        "DOCKER_HOST",
        "DOCKER_CONTEXT",
        "DOCKER_CONFIG",
        "LAYERFS_BUNDLE",
        "LAYERFS_TOOL_FALLBACKS",
    ]
    .into_iter()
    .map(|name| (name, std::env::var_os(name)))
    .collect();
    std::env::set_var("LAYERFS_BUNDLE", &bundle);
    std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");
    std::env::set_var(
        "PATH",
        format!("{}:{}", root.join("bin").display(), path.to_string_lossy()),
    );
    std::env::set_var("HOME", root.join("user"));
    std::env::remove_var("DOCKER_CONTEXT");
    std::env::remove_var("DOCKER_CONFIG");
    let first = root.join("first.sock");
    let second = root.join("second.sock");
    std::env::set_var("DOCKER_HOST", format!("unix://{}", first.display()));

    // no home yet: the host's choice
    let h = root.join("home");
    assert_eq!(chosen(&h), (first.clone(), "env:DOCKER_HOST".to_owned()));
    let id = home::ensure(&h).unwrap().id;
    ensure_built_ins(&h, &id).unwrap();
    MachineRecord::of(&h, "docker")
        .unwrap()
        .with("image", SdkValue::text("test-base"))
        .write(&h)
        .unwrap();
    assert_eq!(chosen(&h), (first.clone(), "env:DOCKER_HOST".to_owned()));

    // the first start runs on that socket and records it, whatever its outcome
    let started = running(&h, "docker").unwrap_err();
    assert_eq!(started.code, "Unavailable", "{}", started.message);
    let calls = fs::read_to_string(root.join("calls")).unwrap();
    let on_first = format!("-H unix://{} ", first.display());
    assert!(calls.contains(&on_first), "{calls}");
    assert_eq!(
        calls.matches("-H unix://").count(),
        calls.matches(&on_first).count(),
        "{calls}"
    );
    let record = MachineRecord::of(&h, "docker").unwrap();
    assert_eq!(record.docker(), Some(first.to_str().unwrap()));
    // a later DOCKER_HOST does not move it
    std::env::set_var("DOCKER_HOST", format!("unix://{}", second.display()));
    assert_eq!(chosen(&h), (first.clone(), "recorded".to_owned()));

    // its worker stays recorded (custody): the socket cannot move under it
    let held = set_docker(&h, "").unwrap_err();
    assert_eq!(held.code, "InUse", "{}", held.message);
    fs::remove_file(home::paths::worker_record(&h, "docker")).unwrap();

    // `--docker ""`: automatic again, though the machine ran; get shows none
    let summary = set_docker(&h, "").unwrap();
    assert_eq!(summary.get("docker"), None, "{summary:?}");
    let record = MachineRecord::of(&h, "docker").unwrap();
    assert_eq!(record.field("docker"), Some(&SdkValue::Null));
    assert_eq!(record.docker(), None);
    assert_eq!(chosen(&h), (second.clone(), "env:DOCKER_HOST".to_owned()));

    // a socket is an absolute path, `unix://` and one accepted
    let summary = set_docker(&h, "unix:///tmp/lfs-dsc-engine.sock").unwrap();
    assert_eq!(summary.get_text("docker"), Some("/tmp/lfs-dsc-engine.sock"));
    assert_eq!(
        chosen(&h),
        (
            PathBuf::from("/tmp/lfs-dsc-engine.sock"),
            "recorded".to_owned()
        )
    );
    let refused = set_docker(&h, "engine.sock").unwrap_err();
    assert_eq!(refused.code, "InvalidInput");
    assert!(
        refused.message.contains("absolute path"),
        "{}",
        refused.message
    );

    // a machine that ran before sockets were recorded keeps the default one,
    // which its next start records: one with lasting state, or one whose
    // record predates `vm_name`
    let base = MachineRecord::of(&h, "docker")
        .unwrap()
        .without("docker")
        .without("state")
        .without("lasting");
    let legacy = (PathBuf::from("/var/run/docker.sock"), "legacy".to_owned());
    base.clone()
        .with("state", SdkValue::text("stopped"))
        .with("lasting", SdkValue::map([]))
        .write(&h)
        .unwrap();
    assert_eq!(chosen(&h), legacy);
    base.clone().without("vm_name").write(&h).unwrap();
    assert_eq!(chosen(&h), legacy);
    // a stop before any start records `stopped` alone: still never started
    base.with("state", SdkValue::text("stopped"))
        .write(&h)
        .unwrap();
    assert_eq!(chosen(&h), (second.clone(), "env:DOCKER_HOST".to_owned()));

    // an Engine that answers is pinned before anything is made in it, so a
    // start that then fails still keeps it
    let answering = root.join("e.sock");
    let engine = std::os::unix::net::UnixListener::bind(&answering).unwrap();
    engine.set_nonblocking(true).unwrap();
    let pinging = std::thread::spawn(move || {
        use std::io::{Read, Write};
        let until = Instant::now() + Duration::from_secs(5);
        while Instant::now() < until {
            match engine.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    let mut request = [0u8; 128];
                    let _ = stream.read(&mut request);
                    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\nOK");
                    return;
                }
                Err(_) => std::thread::sleep(Duration::from_millis(2)),
            }
        }
    });
    std::env::set_var("DOCKER_HOST", format!("unix://{}", answering.display()));
    std::env::set_var("LFS_FAIL_RUN", "1");
    let h2 = root.join("home2");
    let id2 = home::ensure(&h2).unwrap().id;
    ensure_built_ins(&h2, &id2).unwrap();
    MachineRecord::of(&h2, "docker")
        .unwrap()
        .with("image", SdkValue::text("test-base"))
        .write(&h2)
        .unwrap();
    let failed = running(&h2, "docker").unwrap_err();
    pinging.join().unwrap();
    let record = MachineRecord::of(&h2, "docker").unwrap();
    assert_eq!(
        record.docker(),
        Some(answering.to_str().unwrap()),
        "{}: {}",
        failed.code,
        failed.message
    );
    assert!(!record.ever_started(), "{record:?}");
    std::env::remove_var("LFS_FAIL_RUN");

    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();
    std::env::set_var("PATH", path);
    for (name, value) in saved {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
    fs::remove_dir_all(root).unwrap();
}
