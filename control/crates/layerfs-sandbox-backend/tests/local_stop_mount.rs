//! Opt-in host proof against one caller-owned local Sandbox fixture.
//! Supply LAYERFS_TEST_STOP_DIRECTORY, _PID and _PORT. The fixture must be
//! disposable: this test stops its daemon and force-detaches its mounts.
#![cfg(target_os = "macos")]
use layerfs_bridge::process::LocalProcess;
use layerfs_sandbox_backend::{host_process::Sandbox, ContainerId};
use std::{path::PathBuf, process::Command};

#[test]
fn force_reopen_refuses_unconfirmed_process_identity_before_stop() {
    let process = LocalProcess::observe(std::process::id()).unwrap();
    for unconfirmed in [
        LocalProcess {
            boot: None,
            ..process
        },
        LocalProcess {
            start: process.start + 1,
            ..process
        },
    ] {
        let result = Sandbox::reopen_for_stop(
            std::env::temp_dir().join("layerfs-force-unconfirmed-owner"),
            ContainerId::parse(&"1".repeat(64)).unwrap(),
            unconfirmed,
            12345,
            Vec::new(),
        );
        let error = result.unwrap_err();
        assert!(format!("{error:?}").contains("mounts retained"));
        assert!(process.alive());
    }
}

#[test]
#[ignore = "requires an explicitly supplied disposable mounted Sandbox"]
fn stop_detaches_mounts_before_returning_without_delete() {
    let directory = PathBuf::from(std::env::var("LAYERFS_TEST_STOP_DIRECTORY").unwrap());
    let pid = std::env::var("LAYERFS_TEST_STOP_PID")
        .unwrap()
        .parse()
        .unwrap();
    let port = std::env::var("LAYERFS_TEST_STOP_PORT")
        .unwrap()
        .parse()
        .unwrap();
    assert!(directory.is_absolute());
    let mounts = || {
        let out = Command::new("/sbin/mount").output().unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .filter(|line| line.contains(&format!(" on {}/", directory.display())))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert!(!mounts().is_empty(), "fixture must start with a real mount");
    let process = LocalProcess::observe(pid).unwrap();
    let mut sandbox = Sandbox::reopen(
        directory.clone(),
        ContainerId::parse(&"1".repeat(64)).unwrap(),
        process,
        port,
        Vec::new(),
    )
    .unwrap();
    sandbox.stop(1).unwrap();
    assert!(!process.alive(), "fixture daemon survived Stop");
    assert!(directory.exists(), "Stop must not delete its directory");
    assert!(mounts().is_empty(), "Stop left mounts until Delete");
}
