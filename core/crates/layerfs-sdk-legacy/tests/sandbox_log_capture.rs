//! SDK diagnostic deletion uses the owned Docker route without a live daemon.
#![cfg(unix)]

use layerfs_bridge::contract::Code;
use layerfs_sandbox::{OwnerConfig, SandboxOwner};
use layerfs_sdk::SandboxApi;
use layerfs_telemetry::runtime::Runtime;
use std::{env, fs, os::unix::fs::PermissionsExt};

const IMAGE: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn diagnostic_logs_are_bounded_and_capture_failure_does_not_skip_cleanup() {
    let dir = env::temp_dir().join(format!("layerfs-log-capture-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let docker = dir.join("docker");
    fs::write(
        &docker,
        r#"#!/bin/sh
case "$1" in
  image) printf 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n' ;;
  run)
    previous=
    for arg in "$@"; do
      if [ "$previous" = '--name' ]; then container="$arg"; fi
      previous="$arg"
    done
    touch "$LAYERFS_TEST_DIR/$container" "$LAYERFS_TEST_DIR/$container-root"
    ;;
  port) printf '0.0.0.0:49152\n' ;;
  inspect)
    if [ "$3" = '{{.State.Running}}' ]; then
      printf 'false\n'
    else
      for arg in "$@"; do object="$arg"; done
      [ -e "$LAYERFS_TEST_DIR/$object" ] || { printf 'No such object\n' >&2; exit 1; }
    fi
    ;;
  logs)
    printf 'logs\n' >> "$LAYERFS_TEST_DIR/events"
    printf 'LFT1 {"kind":"run-summary"}\n' >&2
    if [ "$LAYERFS_TEST_LOG_MODE" = big ]; then
      head -c 8388609 /dev/zero >&2
    else
      exit 2
    fi
    ;;
  rm)
    printf 'rm\n' >> "$LAYERFS_TEST_DIR/events"
    rm "$LAYERFS_TEST_DIR/$2"
    ;;
  volume)
    if [ "$2" = inspect ]; then
      [ -e "$LAYERFS_TEST_DIR/$3" ] || { printf 'No such volume\n' >&2; exit 1; }
    else
      printf 'volume-rm\n' >> "$LAYERFS_TEST_DIR/events"
      rm "$LAYERFS_TEST_DIR/$3"
    fi
    ;;
  *) exit 1 ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o700)).unwrap();
    let old_path = env::var_os("PATH");
    env::set_var(
        "PATH",
        format!(
            "{}:{}",
            dir.display(),
            old_path.as_deref().unwrap_or_default().to_string_lossy()
        ),
    );
    env::set_var("LAYERFS_TEST_DIR", &dir);
    let owner = SandboxOwner::new(OwnerConfig {
        service_endpoint: "127.0.0.1:1".into(),
        service_selector: 1,
        service_private: [1; 32],
        service_public: [2; 32],
        control_private: [3; 32],
        store: 1,
        telemetry_run: None,
        telemetry: Runtime::disabled(),
    })
    .unwrap();
    let api = SandboxApi::new(&owner);

    env::set_var("LAYERFS_TEST_LOG_MODE", "big");
    let id = api.create(IMAGE, "big-logs").unwrap_err().sandbox.unwrap();
    let mut raw = Vec::new();
    let (cleanup, capture) = api.delete_with_logs(id, &mut raw);
    cleanup.unwrap();
    assert!(capture.attempted);
    assert_eq!(capture.error.unwrap().code, Code::Capacity);
    assert!(capture.truncated);
    assert_eq!(capture.bytes, 8 * 1024 * 1024);
    assert_eq!(raw.len(), capture.bytes as usize);
    assert!(raw.starts_with(b"LFT1 {\"kind\":\"run-summary\"}\n"));
    assert!(api.list().unwrap().is_empty());

    env::set_var("LAYERFS_TEST_LOG_MODE", "fail");
    let id = api
        .create(IMAGE, "failed-logs")
        .unwrap_err()
        .sandbox
        .unwrap();
    let mut raw = Vec::new();
    let (cleanup, capture) = api.delete_with_logs(id, &mut raw);
    cleanup.unwrap();
    assert!(capture.attempted);
    assert_eq!(capture.error.unwrap().code, Code::Io);
    assert!(!capture.truncated);
    assert!(raw.starts_with(b"LFT1 {\"kind\":\"run-summary\"}\n"));
    assert!(api.list().unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(dir.join("events")).unwrap(),
        "logs\nrm\nvolume-rm\nlogs\nrm\nvolume-rm\n"
    );

    match old_path {
        Some(path) => env::set_var("PATH", path),
        None => env::remove_var("PATH"),
    }
    env::remove_var("LAYERFS_TEST_DIR");
    env::remove_var("LAYERFS_TEST_LOG_MODE");
    fs::remove_dir_all(dir).unwrap();
}
