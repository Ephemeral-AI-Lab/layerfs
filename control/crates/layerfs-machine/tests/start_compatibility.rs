//! A started or already-running incompatible worker is never declared ready.
#![cfg(all(feature = "machine", target_os = "macos"))]
use layerfs_bridge::control::SdkValue;
use layerfs_home::home;
use layerfs_machine::worker::{read_frame_within, write_frame, MAX_HELLO};
use layerfs_machine::{ensure_built_ins, record::MachineRecord, running};
use std::fs;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

#[test]
fn incompatible_start_retains_custody_and_second_start_never_replays_provider_work() {
    let root = std::env::temp_dir().join(format!("lfs-start-compat-{}", std::process::id()));
    fs::create_dir_all(root.join("bin")).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&stopped);
    let server = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(8);
        while !stop.load(Ordering::Relaxed) && Instant::now() < until {
            let (mut stream, _) = match listener.accept() {
                Ok(stream) => stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(e) => panic!("accept: {e}"),
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            if read_frame_within(&mut stream, MAX_HELLO)
                .ok()
                .flatten()
                .is_none()
            {
                continue;
            }
            write_frame(
                &mut stream,
                &SdkValue::map([
                    ("ready", SdkValue::Bool(true)),
                    ("protocol", SdkValue::Int(2)),
                    ("bundle", SdkValue::text("abcd")),
                ]),
            )
            .unwrap();
            assert!(
                read_frame_within(&mut stream, MAX_HELLO).unwrap().is_none(),
                "no request reaches an incompatible worker"
            );
        }
    });
    let script = format!(
        r#"#!/bin/sh
echo "$3" >> '{root}/calls'
case "$3" in
  inspect) echo 'No such object' >&2; exit 1 ;;
  run) echo fake-owned-container ;;
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
    std::env::set_var("LAYERFS_BUNDLE", &bundle);
    let path = std::env::var_os("PATH").unwrap();
    std::env::set_var(
        "PATH",
        format!("{}:{}", root.join("bin").display(), path.to_string_lossy()),
    );
    let h = root.join("home");
    let id = home::ensure(&h).unwrap().id;
    ensure_built_ins(&h, &id).unwrap();
    MachineRecord::of(&h, "docker")
        .unwrap()
        .with("image", SdkValue::text("test-base"))
        .write(&h)
        .unwrap();
    let first = running(&h, "docker").unwrap_err();
    assert_eq!(first.code, "Unavailable");
    assert_eq!(
        MachineRecord::of(&h, "docker").unwrap().recorded_state(),
        Some("failed")
    );
    assert!(
        home::paths::worker_record(&h, "docker").is_file(),
        "endpoint remains in custody for explicit stop/remove"
    );
    let calls = fs::read_to_string(root.join("calls")).unwrap();
    let second = running(&h, "docker").unwrap_err();
    assert_eq!(second.code, "Unavailable");
    assert_eq!(
        fs::read_to_string(root.join("calls")).unwrap(),
        calls,
        "no second provider attempt"
    );
    stopped.store(true, Ordering::Relaxed);
    server.join().unwrap();
    std::env::set_var("PATH", path);
    std::env::remove_var("LAYERFS_BUNDLE");
    fs::remove_dir_all(root).unwrap();
}
