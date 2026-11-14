//! Real control Service with process helpers, no kernel NFS mount required.
#[allow(dead_code)]
#[path = "support/rig.rs"]
mod rig;
use layerfs_bridge::control::{
    CleanupObservation, DetachDisposition, ForcedCleanup, MountPhase, Reply, Request,
    TeardownStage, UnmountControl,
};
use layerfs_daemon::control::{
    ExportServing, ExportServingConfig, Failure, Helper, Mounter, HELPER_WAIT,
};
use layerfs_history::WorkspaceId;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

fn base(label: &str) -> PathBuf {
    let root = std::env::var_os("H07_TEST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    root.join(format!("helper-{}-{label}", std::process::id()))
}
fn run_case(label: &str, helper: Helper, before: impl FnOnce()) -> (String, MountPhase) {
    let base = base(label);
    fs::create_dir_all(base.join("src")).unwrap();
    fs::create_dir_all(base.join("mounts")).unwrap();
    fs::write(base.join("src/file"), b"fixture").unwrap();
    let rig = rig::Rig::new(
        &base.join("src"),
        &base.join("work"),
        Duration::from_secs(5),
    );
    let service = rig.service(Arc::new(ExportServing::start(ExportServingConfig::new(
        base.join("mounts"),
        501,
        20,
        Mounter {
            mount: Helper::words("true").unwrap(),
            unmount: helper,
            force: Helper::words("true").unwrap(),
            flush: None,
        },
    ))));
    let token = match service
        .execute_control(&Request::Mount {
            workspace: WorkspaceId::from_authority([33; 32]).unwrap(),
            branch: rig.branch,
        })
        .unwrap()
        .reply
    {
        Reply::Bound { token, .. } => token,
        other => panic!("{other:?}"),
    };
    service.execute_control(&Request::Attach(token)).unwrap();
    before();
    let reply = format!(
        "{:?}",
        service.execute_control(&Request::Unmount(UnmountControl {
            token,
            discard: true
        }))
    );
    let phase = match service.execute_control(&Request::Status(token)) {
        Ok(done) => match done.reply {
            Reply::Status(status) => status.kernel.unwrap().phase,
            other => panic!("{other:?}"),
        },
        Err(_) => MountPhase::Unattached,
    };
    if phase == MountPhase::Retained {
        let repeated = format!(
            "{:?}",
            service.execute_control(&Request::Unmount(UnmountControl {
                token,
                discard: true
            }))
        );
        assert_eq!(
            reply, repeated,
            "retained request must not launch another helper"
        );
    }
    // No actual kernel mount exists in this fixture. Service owns retained
    // export custody until drop.
    drop(service);
    drop(rig);
    fs::remove_dir_all(base).unwrap();
    (reply, phase)
}
#[test]
fn exited_helper_does_not_wait_for_a_descendants_pipe_eof() {
    let root = base("pipe-holder");
    fs::create_dir_all(&root).unwrap();
    let pidfile = root.join("descendant.pid");
    let script = format!("sleep 20 & echo $! > {}; exit 0", pidfile.display());
    let (send, receive) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let result = run_case(
            "pipes",
            Helper(vec!["sh".into(), "-c".into(), script]),
            || {},
        );
        let _ = send.send(result);
    });
    let result = receive.recv_timeout(Duration::from_secs(3));
    // Always release the descendant, including on failure, before asserting.
    if let Ok(pid) = fs::read_to_string(&pidfile) {
        let _ = Command::new("kill").args(["-KILL", pid.trim()]).status();
    }
    worker.join().unwrap();
    fs::remove_dir_all(root).unwrap();
    let (reply, _) = result.expect("exited helper blocked on descendant's open pipe");
    assert!(reply.contains("Unmounted"), "{reply}");
}
#[test]
fn timed_out_helper_returns_retained_and_does_not_replay() {
    // STOP makes the helper ignore TERM; KILL must reap it without wait().
    // A normal process cannot ignore SIGKILL: this tests the actual deadline,
    // not the kernel's uninterruptible U state.
    let (reply, phase) = run_case(
        "timeout",
        Helper(vec!["sh".into(), "-c".into(), "kill -STOP $$".into()]),
        || {},
    );
    assert!(
        reply.contains("Retained") && reply.contains("60s"),
        "{reply}"
    );
    assert_eq!(phase, MountPhase::Retained);
    assert_eq!(HELPER_WAIT, Duration::from_secs(60));
}

#[test]
fn verbose_helper_preserves_busy_refusal_without_pipe_deadlock() {
    let (reply, phase) = run_case(
        "verbose",
        Helper(vec!["sh".into(), "-c".into(), "echo busy >&2; i=0; while [ $i -lt 5000 ]; do echo verbose-helper-output; i=$((i+1)); done; exit 32".into()]),
        || {},
    );
    assert!(reply.to_ascii_lowercase().contains("busy"), "{reply}");
    assert_eq!(phase, MountPhase::Ready);
}

fn wait_reaped(serving: &ExportServing, pid: u32, limit: Duration) -> String {
    let deadline = Instant::now() + limit;
    loop {
        if let Some((_, status)) = serving
            .reaped_helpers()
            .into_iter()
            .find(|(p, _)| *p == pid)
        {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "timed-out helper {pid} was not reaped within {limit:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn assert_not_zombie(pid: u32) {
    let out = Command::new("ps")
        .args(["-o", "state=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    let state = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    assert!(
        !state.starts_with('Z'),
        "helper pid {pid} remained a zombie: state={state:?}"
    );
}

#[test]
fn timed_out_helper_when_mount_remains_stays_retained_reaps_zombie_and_forces_cleanly() {
    let (send, receive) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let base = base("mount-remains");
        drop(fs::remove_dir_all(&base));
        fs::create_dir_all(base.join("src")).unwrap();
        fs::create_dir_all(base.join("mounts")).unwrap();
        fs::write(base.join("src/file"), b"fixture").unwrap();
        let table = base.join("mount.table");
        let pidfile = base.join("unmount.pid");
        let rig = rig::Rig::new(
            &base.join("src"),
            &base.join("work"),
            Duration::from_secs(5),
        );
        let mut config = ExportServingConfig::new(
            base.join("mounts"),
            501,
            20,
            Mounter {
                mount: Helper(vec![
                    "sh".into(),
                    "-c".into(),
                    format!("echo '{{dir}}' > {}", table.display()),
                ]),
                unmount: Helper(vec![
                    "sh".into(),
                    "-c".into(),
                    format!("echo $$ > {}; sleep 5", pidfile.display()),
                ]),
                force: Helper(vec![
                    "sh".into(),
                    "-c".into(),
                    format!("rm -f {}", table.display()),
                ]),
                flush: None,
            },
        );
        config.helper_wait = Duration::from_millis(100);
        config.reap_wait = Duration::ZERO;
        config.mount_table = Some(table.clone());
        let export = Arc::new(ExportServing::start(config));
        let service = rig.service(export.clone());
        let token = match service
            .execute_control(&Request::Mount {
                workspace: WorkspaceId::from_authority([34; 32]).unwrap(),
                branch: rig.branch,
            })
            .unwrap()
            .reply
        {
            Reply::Bound { token, .. } => token,
            other => panic!("{other:?}"),
        };
        let ready = match service
            .execute_control(&Request::Attach(token))
            .unwrap()
            .reply
        {
            Reply::Ready(ready) => *ready,
            other => panic!("{other:?}"),
        };

        // Mount remains in table when unmount helper times out: keep Retained(Detach).
        let err = service
            .execute_control(&Request::Unmount(UnmountControl {
                token,
                discard: true,
            }))
            .unwrap_err();
        match &err {
            Failure::Retained(custody) => {
                assert_eq!(custody.stage, TeardownStage::Detach);
                assert!(!custody.detached);
                let work = custody.work.expect("mount work");
                assert_eq!(work.loops_entered, 1);
                assert_eq!(work.loops_exited, 0);
                assert_eq!(work.loops_joined, 0);
            }
            other => panic!("expected Retained, got {other:?}"),
        }

        // With no new Helpers::run call, background reaper reaps the child and records exit.
        let pid: u32 = fs::read_to_string(&pidfile)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let status = wait_reaped(&export, pid, Duration::from_secs(2));
        assert!(
            !status.is_empty(),
            "recorded exit status should not be empty"
        );
        assert_not_zombie(pid);

        // Explicit ForceUnmount on ExportKept(Detach) finishes teardown and closes Workspace.
        match service
            .execute_control(&Request::ForceUnmount {
                token,
                relinquish_unknown: false,
            })
            .unwrap()
            .reply
        {
            Reply::ForceUnmounted(done) => {
                assert_eq!(done.outcome.facts.detach, DetachDisposition::Detached);
                assert_eq!(done.outcome.work.loops_entered, 1);
                assert_eq!(done.outcome.work.loops_exited, 1);
                assert_eq!(done.outcome.work.loops_joined, 1);
                assert!(matches!(
                    done.outcome.cleanup,
                    ForcedCleanup::Queued | ForcedCleanup::Gone
                ));
            }
            other => panic!("expected ForceUnmounted, got {other:?}"),
        }
        let released = service.export_released(token).expect("export released");
        assert_eq!(released.retained, 0);
        assert!(!Path::new(&ready.directory).exists());
        assert!(service.execute_control(&Request::Status(token)).is_err());
        match service
            .execute_control(&Request::Cleanup(token))
            .unwrap()
            .reply
        {
            Reply::Cleanup { state, .. } => assert_eq!(state, CleanupObservation::Gone),
            other => panic!("expected Cleanup, got {other:?}"),
        }

        drop(service);
        drop(rig);
        fs::remove_dir_all(base).unwrap();
        let _ = send.send(());
    });
    receive
        .recv_timeout(Duration::from_secs(10))
        .expect("bounded test timed out");
    worker.join().unwrap();
}

#[test]
fn timed_out_helper_when_mount_already_gone_completes_teardown() {
    let (send, receive) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let base = base("mount-gone");
        drop(fs::remove_dir_all(&base));
        fs::create_dir_all(base.join("src")).unwrap();
        fs::create_dir_all(base.join("mounts")).unwrap();
        fs::write(base.join("src/file"), b"fixture").unwrap();
        let table = base.join("mount.table");
        let pidfile = base.join("unmount.pid");
        let rig = rig::Rig::new(
            &base.join("src"),
            &base.join("work"),
            Duration::from_secs(5),
        );
        let mut config = ExportServingConfig::new(
            base.join("mounts"),
            501,
            20,
            Mounter {
                mount: Helper(vec![
                    "sh".into(),
                    "-c".into(),
                    format!("echo '{{dir}}' > {}", table.display()),
                ]),
                unmount: Helper(vec![
                    "sh".into(),
                    "-c".into(),
                    format!(
                        "rm -f {}; echo $$ > {}; sleep 5",
                        table.display(),
                        pidfile.display()
                    ),
                ]),
                force: Helper::words("true").unwrap(),
                flush: None,
            },
        );
        config.helper_wait = Duration::from_millis(100);
        config.reap_wait = Duration::ZERO;
        config.mount_table = Some(table);
        let export = Arc::new(ExportServing::start(config));
        let service = rig.service(export.clone());
        let token = match service
            .execute_control(&Request::Mount {
                workspace: WorkspaceId::from_authority([35; 32]).unwrap(),
                branch: rig.branch,
            })
            .unwrap()
            .reply
        {
            Reply::Bound { token, .. } => token,
            other => panic!("{other:?}"),
        };
        let ready = match service
            .execute_control(&Request::Attach(token))
            .unwrap()
            .reply
        {
            Reply::Ready(ready) => *ready,
            other => panic!("{other:?}"),
        };

        // Helper timed out, but mount table confirms the mount is gone: teardown completes.
        let reply = service
            .execute_control(&Request::Unmount(UnmountControl {
                token,
                discard: true,
            }))
            .expect("unmount should complete teardown when mount is gone")
            .reply;
        assert_eq!(reply, Reply::Unmounted(token));
        let released = service.export_released(token).expect("export released");
        assert_eq!(released.retained, 0);
        assert!(!Path::new(&ready.directory).exists());
        assert!(service.execute_control(&Request::Status(token)).is_err());
        match service
            .execute_control(&Request::Cleanup(token))
            .unwrap()
            .reply
        {
            Reply::Cleanup { state, .. } => assert_eq!(state, CleanupObservation::Gone),
            other => panic!("expected Cleanup, got {other:?}"),
        }

        // The timed-out helper is still reaped in the background without becoming a zombie.
        let pid: u32 = fs::read_to_string(&pidfile)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let status = wait_reaped(&export, pid, Duration::from_secs(2));
        assert!(!status.is_empty());
        assert_not_zombie(pid);

        drop(service);
        drop(rig);
        fs::remove_dir_all(base).unwrap();
        let _ = send.send(());
    });
    receive
        .recv_timeout(Duration::from_secs(10))
        .expect("bounded test timed out");
    worker.join().unwrap();
}

#[test]
fn a_lazily_detached_force_helper_never_completes_teardown() {
    // `umount -f -l` detaches lazily: the mount leaves the host's table while
    // the kernel still holds its superblock. A vanished entry is therefore no
    // proof that the client's mount ended, and Force must keep the custody.
    let (send, receive) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let base = base("lazy-force");
        drop(fs::remove_dir_all(&base));
        fs::create_dir_all(base.join("src")).unwrap();
        fs::create_dir_all(base.join("mounts")).unwrap();
        fs::write(base.join("src/file"), b"fixture").unwrap();
        let table = base.join("mount.table");
        let rig = rig::Rig::new(
            &base.join("src"),
            &base.join("work"),
            Duration::from_secs(5),
        );
        let mut config = ExportServingConfig::new(
            base.join("mounts"),
            501,
            20,
            Mounter {
                mount: Helper(vec![
                    "sh".into(),
                    "-c".into(),
                    format!("echo '{{dir}}' > {}", table.display()),
                ]),
                unmount: Helper(vec!["sh".into(), "-c".into(), "exit 1".into()]),
                // `-l` is what the Linux force helper (`umount -f -l`) passes:
                // the helper fails, yet the entry is gone from the table.
                force: Helper(vec![
                    "sh".into(),
                    "-c".into(),
                    format!("rm -f {}; exit 1", table.display()),
                    "-l".into(),
                ]),
                flush: None,
            },
        );
        config.mount_table = Some(table.clone());
        let export = Arc::new(ExportServing::start(config));
        let service = rig.service(export);
        let token = match service
            .execute_control(&Request::Mount {
                workspace: WorkspaceId::from_authority([38; 32]).unwrap(),
                branch: rig.branch,
            })
            .unwrap()
            .reply
        {
            Reply::Bound { token, .. } => token,
            other => panic!("{other:?}"),
        };
        let ready = match service
            .execute_control(&Request::Attach(token))
            .unwrap()
            .reply
        {
            Reply::Ready(ready) => *ready,
            other => panic!("{other:?}"),
        };

        // The plain helper fails while the table still lists the mount.
        let err = service
            .execute_control(&Request::Unmount(UnmountControl {
                token,
                discard: true,
            }))
            .unwrap_err();
        match &err {
            Failure::Retained(custody) => {
                assert_eq!(custody.stage, TeardownStage::Detach);
                assert!(!custody.detached);
            }
            other => panic!("expected Retained, got {other:?}"),
        }

        // The forced helper fails too, and the entry is gone from the table:
        // a lazy detach, so the mount is not known gone.
        let err = service
            .execute_control(&Request::ForceUnmount {
                token,
                relinquish_unknown: false,
            })
            .unwrap_err();
        match &err {
            Failure::Retained(custody) => {
                assert_eq!(custody.stage, TeardownStage::Detach);
                assert!(!custody.detached);
                assert_eq!(
                    custody.forced.as_ref().map(|facts| facts.detach),
                    Some(DetachDisposition::Failed)
                );
            }
            other => panic!("expected Retained, got {other:?}"),
        }
        assert!(Path::new(&ready.directory).exists());
        match service.execute_control(&Request::Status(token)) {
            Ok(done) => match done.reply {
                Reply::Status(status) => assert_eq!(
                    status.kernel.map(|kernel| kernel.phase),
                    Some(MountPhase::Retained)
                ),
                other => panic!("{other:?}"),
            },
            Err(error) => panic!("{error:?}"),
        }
        assert!(
            service.retained_export(token).unwrap().is_some(),
            "the export stays in custody"
        );
        assert!(service.export_released(token).is_none());
        assert!(!table.exists(), "the lazy helper removed the table entry");

        drop(service);
        drop(rig);
        fs::remove_dir_all(base).unwrap();
        let _ = send.send(());
    });
    receive
        .recv_timeout(Duration::from_secs(10))
        .expect("bounded test timed out");
    worker.join().unwrap();
}

#[test]
fn canonical_mount_paths_and_unreadable_table_preserve_detach_safety() {
    let (send, receive) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let base = base("canon-table");
        drop(fs::remove_dir_all(&base));
        fs::create_dir_all(base.join("src")).unwrap();
        fs::create_dir_all(base.join("real_mounts")).unwrap();
        let link_mounts = base.join("link_mounts");
        std::os::unix::fs::symlink(base.join("real_mounts"), &link_mounts).unwrap();
        fs::write(base.join("src/file"), b"fixture").unwrap();
        let table = base.join("mount.table");
        let rig = rig::Rig::new(
            &base.join("src"),
            &base.join("work"),
            Duration::from_secs(5),
        );
        let mut config = ExportServingConfig::new(
            link_mounts,
            501,
            20,
            Mounter {
                // Record the canonicalized path (like macOS /sbin/mount printing /private/...).
                mount: Helper(vec![
                    "sh".into(),
                    "-c".into(),
                    format!("cd '{{dir}}' && pwd -P > {}", table.display()),
                ]),
                unmount: Helper(vec!["sh".into(), "-c".into(), "exit 1".into()]),
                force: Helper(vec!["sh".into(), "-c".into(), "exit 1".into()]),
                flush: None,
            },
        );
        config.mount_table = Some(table.clone());
        let export = Arc::new(ExportServing::start(config));
        let service = rig.service(export);
        let token = match service
            .execute_control(&Request::Mount {
                workspace: WorkspaceId::from_authority([36; 32]).unwrap(),
                branch: rig.branch,
            })
            .unwrap()
            .reply
        {
            Reply::Bound { token, .. } => token,
            other => panic!("{other:?}"),
        };
        let ready = match service
            .execute_control(&Request::Attach(token))
            .unwrap()
            .reply
        {
            Reply::Ready(ready) => *ready,
            other => panic!("{other:?}"),
        };

        // M1: Replace mount.table with a directory so reading fails with a non-NotFound error.
        // Unmount must NOT treat the read error as an empty mount table; it must keep Retained.
        fs::remove_file(&table).unwrap();
        fs::create_dir(&table).unwrap();
        let err = service
            .execute_control(&Request::Unmount(UnmountControl {
                token,
                discard: true,
            }))
            .unwrap_err();
        assert!(
            matches!(err, Failure::Retained(_)),
            "expected Retained, got {err:?}"
        );
        assert!(Path::new(&ready.directory).exists());

        // M2: Once mount.table is removed (NotFound -> mount gone), even if force helper exits 1
        // ("not currently mounted"), canonicalized tracked path recognizes the mount is gone.
        fs::remove_dir(&table).unwrap();
        match service
            .execute_control(&Request::ForceUnmount {
                token,
                relinquish_unknown: false,
            })
            .unwrap()
            .reply
        {
            Reply::ForceUnmounted(done) => {
                assert_eq!(done.outcome.facts.detach, DetachDisposition::Detached);
                assert_eq!(done.outcome.work.loops_joined, 1);
            }
            other => panic!("expected ForceUnmounted, got {other:?}"),
        }
        assert!(!Path::new(&ready.directory).exists());

        drop(service);
        drop(rig);
        fs::remove_dir_all(base).unwrap();
        let _ = send.send(());
    });
    receive
        .recv_timeout(Duration::from_secs(10))
        .expect("bounded test timed out");
    worker.join().unwrap();
}

/// The mount home is a symbolic link (macOS `/var`, `/tmp` are
/// `/private/...`), so the host's table prints the resolved point. A still
/// mounted Workspace must stay `Retained(Detach)` even when the mount
/// directory itself cannot be resolved: that lookup would go through the
/// mount, which is exactly what a wedged client makes fail.
#[test]
fn a_listed_canonical_mount_stays_retained_when_the_mount_point_does_not_resolve() {
    let (send, receive) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let base = base("canon-listed");
        drop(fs::remove_dir_all(&base));
        fs::create_dir_all(base.join("src")).unwrap();
        fs::create_dir_all(base.join("real_mounts")).unwrap();
        let link_mounts = base.join("link_mounts");
        std::os::unix::fs::symlink(base.join("real_mounts"), &link_mounts).unwrap();
        fs::write(base.join("src/file"), b"fixture").unwrap();
        let table = base.join("mount.table");
        let rig = rig::Rig::new(
            &base.join("src"),
            &base.join("work"),
            Duration::from_secs(5),
        );
        let mut config = ExportServingConfig::new(
            link_mounts,
            501,
            20,
            Mounter {
                // The host prints the resolved path, as `mount` does on macOS.
                mount: Helper(vec![
                    "sh".into(),
                    "-c".into(),
                    format!("cd '{{dir}}' && pwd -P > {}", table.display()),
                ]),
                unmount: Helper(vec!["sh".into(), "-c".into(), "exit 1".into()]),
                force: Helper::words("true").unwrap(),
                flush: None,
            },
        );
        config.mount_table = Some(table.clone());
        let export = Arc::new(ExportServing::start(config));
        let service = rig.service(export);
        let token = match service
            .execute_control(&Request::Mount {
                workspace: WorkspaceId::from_authority([37; 32]).unwrap(),
                branch: rig.branch,
            })
            .unwrap()
            .reply
        {
            Reply::Bound { token, .. } => token,
            other => panic!("{other:?}"),
        };
        let ready = match service
            .execute_control(&Request::Attach(token))
            .unwrap()
            .reply
        {
            Reply::Ready(ready) => *ready,
            other => panic!("{other:?}"),
        };
        let name = ready.directory.rsplit('/').next().unwrap().to_owned();
        let canonical = base.canonicalize().unwrap().join("real_mounts").join(&name);
        let listed = || fs::read_to_string(&table).unwrap().trim().to_owned();
        assert_eq!(
            listed(),
            canonical.display().to_string(),
            "the table lists the resolved point, never the configured home"
        );

        // The mount point stops resolving (a wedged mount answers EIO here).
        fs::remove_dir(&ready.directory).unwrap();
        std::os::unix::fs::symlink(base.join("gone"), &ready.directory).unwrap();

        // The table still lists it, so the mount is not gone: keep custody.
        let err = service
            .execute_control(&Request::Unmount(UnmountControl {
                token,
                discard: true,
            }))
            .unwrap_err();
        match &err {
            Failure::Retained(custody) => {
                assert_eq!(custody.stage, TeardownStage::Detach);
                assert!(!custody.detached);
            }
            other => panic!("expected Retained, got {other:?}"),
        }
        assert_eq!(
            listed(),
            canonical.display().to_string(),
            "an unresolved mount point must not be read as a removed mount"
        );

        drop(service);
        drop(rig);
        fs::remove_dir_all(base).unwrap();
        let _ = send.send(());
    });
    receive
        .recv_timeout(Duration::from_secs(10))
        .expect("bounded test timed out");
    worker.join().unwrap();
}
