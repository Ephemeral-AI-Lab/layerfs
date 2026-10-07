//! Public owner behavior when Docker refuses its published-port observation.
#![cfg(unix)]

use layerfs_api_core::SandboxStatus;
use layerfs_bridge::contract::{Code, Operation};
use layerfs_sandbox::{ControlRoute, OwnerConfig, RouteError, SandboxOwner};
use layerfs_telemetry::runtime::Runtime;
use std::{env, fs, os::unix::fs::PermissionsExt};

#[test]
fn docker_allocates_port_and_pending_record_cannot_route() {
    let dir = env::temp_dir().join(format!("layerfs-port-pending-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let log = dir.join("docker.log");
    let docker = dir.join("docker");
    fs::write(
        &docker,
        r#"#!/bin/sh
case "$1" in
  image)
    printf 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n'
    ;;
  run)
    printf 'run\n' >> "$LAYERFS_TEST_DOCKER_LOG"
    for arg in "$@"; do
      if [ "$arg" = '127.0.0.1::23456' ]; then
        printf 'ephemeral-loopback\n' >> "$LAYERFS_TEST_DOCKER_LOG"
      fi
    done
    ;;
  port)
    printf 'port\n' >> "$LAYERFS_TEST_DOCKER_LOG"
    printf '0.0.0.0:49152\n'
    ;;
  *)
    printf 'No such object\n' >&2
    exit 1
    ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o700)).unwrap();
    let old_path = env::var_os("PATH");
    env::set_var("PATH", &dir);
    env::set_var("LAYERFS_TEST_DOCKER_LOG", &log);

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
    let error = owner
        .create(
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "pending-port",
        )
        .unwrap_err();
    assert_eq!(error.cause.code, Code::Denied);
    assert!(error.retained);
    let id = error.sandbox.expect("prelaunch Sandbox custody");
    assert!(matches!(
        owner.lookup_route(id),
        Err(RouteError::Failure(failure)) if failure.code == Code::Busy
    ));
    assert!(matches!(
        owner.checked_lookup(id),
        Err(RouteError::Failure(failure)) if failure.code == Code::Busy
    ));
    assert!(matches!(
        owner.control_call(
            ControlRoute {
                sandbox: id,
                instance: [9; 32],
                workspace: None,
            },
            1000,
            || Operation::SandboxHello,
        ),
        Err(RouteError::Failure(failure)) if failure.code == Code::Denied
    ));
    assert_eq!(owner.list().unwrap()[0].status, SandboxStatus::Stopped);
    owner.delete(id).unwrap();
    assert!(owner.list().unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        "run\nephemeral-loopback\nport\n"
    );

    match old_path {
        Some(path) => env::set_var("PATH", path),
        None => env::remove_var("PATH"),
    }
    env::remove_var("LAYERFS_TEST_DOCKER_LOG");
    fs::remove_dir_all(dir).unwrap();
}
