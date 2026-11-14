//! The local process identity (`layerfs_bridge::process`, moved from the
//! Sandbox driver by layerfs-project#112) on this kernel: this process is
//! observed with its start time and boot, another start time or boot is
//! another process, a record without a boot is matched by id and start and
//! never signalled, and the process table lists this process in its group.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_bridge::process::{self, LocalProcess};
use nix::sys::signal::Signal;

#[test]
fn this_process_is_named_by_id_start_and_boot() {
    let me = LocalProcess::observe(std::process::id()).unwrap();
    assert_eq!(me.pid, std::process::id());
    assert_eq!(me.boot, Some(process::boot().unwrap()));
    assert_eq!(me.start, process::stat(me.pid).unwrap().start);
    assert!(me.alive());
    assert_eq!(LocalProcess::observe(me.pid).unwrap(), me);
    assert!(!LocalProcess {
        start: me.start + 1,
        ..me
    }
    .alive());
    let other_boot = LocalProcess {
        boot: me.boot.map(|boot| boot ^ 1),
        ..me
    };
    assert!(!other_boot.alive());
    // a record from before boot ids: matched by id and start, never signalled
    let legacy = LocalProcess { boot: None, ..me };
    assert!(legacy.alive());
    let refused = legacy.signal(Signal::SIGTERM).unwrap_err();
    assert_eq!(refused.kind(), std::io::ErrorKind::InvalidData);
    // a process that is gone is not alive, and signalling it does nothing
    assert!(other_boot.signal(Signal::SIGTERM).is_ok());
}

#[test]
fn the_process_table_lists_this_process_in_its_group() {
    let me = std::process::id();
    let stat = process::stat(me).unwrap();
    assert!(!stat.ended());
    let table = process::processes().unwrap();
    let (_, listed) = table.iter().find(|(pid, _)| *pid == me).unwrap();
    assert_eq!(listed.start, stat.start);
    assert_eq!(listed.group, stat.group);
    assert!(process::pid(u32::MAX).is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn sync_volume_wait_flushes_existing_directory_and_rejects_missing_path() {
    let dir = std::env::temp_dir().join(format!("layerfs-bridge-sync-vol-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert!(process::sync_volume_wait(&dir).is_ok());
    std::fs::remove_dir_all(&dir).unwrap();

    let missing = dir.join("missing-subdir");
    let err = process::sync_volume_wait(&missing).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}
