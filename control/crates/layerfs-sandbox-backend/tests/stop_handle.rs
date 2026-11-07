//! A command's stop handle on `host_process` (layerfs-project#113,
//! compute-plane 2.0 §5.3): a cancel ends the command's process group,
//! background children included, with SIGTERM within the grace and SIGKILL
//! after it, and a group with nothing left is `Ended` and not signalled.
//! A command whose cancel came before its handle was told (the slot refuses
//! it as interrupted) is ended as it starts and reports SIGKILL's exit.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_bridge::process::LocalProcess;
use layerfs_sandbox_backend::local::Local;
use layerfs_sandbox_backend::stop::{cancel, component, Cancelled, ExecSlot, Stop};
use layerfs_sandbox_backend::{CommandIdentity, ExecRequest};
use nix::sys::signal::Signal;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};
use std::time::Duration;

/// `script` in a process group of its own, and its stop handle.
fn group(script: &str) -> (Child, Stop) {
    let child = Command::new("/bin/sh")
        .args(["-c", script])
        .process_group(0)
        .spawn()
        .unwrap();
    let leader = LocalProcess::observe(child.id()).unwrap();
    let stop = Stop::Group {
        pgid: child.id(),
        leader,
    };
    (child, stop)
}

/// Reaps `child` in the background, as an exec's owner waits for it, so
/// the leader is not left a zombie the group check would still count.
fn reaped(mut child: Child) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let _ = child.wait();
    })
}

#[test]
fn a_group_and_its_background_children_end_with_sigterm() {
    let (child, stop) = group("sleep 600 & sleep 600 & wait");
    let waiter = reaped(child);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(stop.form(), "group");
    let outcome = cancel(&stop, Duration::from_secs(5)).unwrap();
    assert_eq!(outcome, Cancelled::Signalled(Signal::SIGTERM));
    waiter.join().unwrap();
    // nothing of it is left: a second cancel finds it ended
    assert_eq!(
        cancel(&stop, Duration::from_secs(5)).unwrap(),
        Cancelled::Ended
    );
}

#[test]
fn a_group_that_ignores_sigterm_ends_with_sigkill_after_the_grace() {
    let (child, stop) = group("trap '' TERM; sleep 600 & wait; sleep 600");
    let waiter = reaped(child);
    std::thread::sleep(Duration::from_millis(200));
    let outcome = cancel(&stop, Duration::from_millis(300)).unwrap();
    assert_eq!(outcome, Cancelled::Signalled(Signal::SIGKILL));
    waiter.join().unwrap();
}

#[test]
fn an_ended_group_is_not_signalled() {
    let (mut child, stop) = group("true");
    child.wait().unwrap();
    assert_eq!(
        cancel(&stop, Duration::from_secs(1)).unwrap(),
        Cancelled::Ended
    );
}

#[test]
fn a_leader_id_that_names_another_process_is_ended() {
    let (mut child, stop) = group("sleep 600");
    let Stop::Group { pgid, leader } = stop else {
        unreachable!()
    };
    // the same id with another start time: not this group
    let other = Stop::Group {
        pgid,
        leader: LocalProcess {
            start: leader.start + 1,
            ..leader
        },
    };
    assert_eq!(
        cancel(&other, Duration::from_secs(1)).unwrap(),
        Cancelled::Ended
    );
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn stop_handle_names_are_one_plain_path_component() {
    for good in ["exec-0123abcd", "a", "build_1.2", &"f".repeat(64)] {
        assert!(component(good), "{good}");
    }
    for bad in [
        "",
        ".",
        "..",
        ".hidden",
        "-x",
        "a/b",
        "a b",
        "ä",
        &"f".repeat(129),
    ] {
        assert!(!component(bad), "{bad}");
    }
}

#[test]
fn a_command_cancelled_before_its_handle_was_told_ends_as_it_starts() {
    let base = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("layerfs-stop-early-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("root")).unwrap();
    for dir in [base.clone(), base.join("root")] {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let daemon = base.join("daemon.sh");
    std::fs::write(
        &daemon,
        "#!/bin/sh\n[ \"$1\" = --config ] && [ -f \"$2\" ] || exit 3\necho \"LAYERFS_DAEMON_LISTEN 127.0.0.1:40404\"\nexec /bin/sleep 60\n",
    )
    .unwrap();
    std::fs::set_permissions(&daemon, std::fs::Permissions::from_mode(0o755)).unwrap();
    let wait = Duration::from_secs(10);
    let mut sandbox = Local::native(base.join("root"))
        .unwrap()
        .naming("early")
        .unwrap()
        .create(&daemon, b"setup bytes", wait, Vec::new())
        .unwrap();
    let request = ExecRequest {
        // a command is never root (a Linux build container runs as root)
        identity: match nix::unistd::Uid::effective().as_raw() {
            0 => CommandIdentity {
                uid: 65534,
                gid: 65534,
            },
            uid => CommandIdentity {
                uid,
                gid: nix::unistd::Gid::effective().as_raw(),
            },
        },
        arguments: vec![
            "/bin/sh".into(),
            "-c".into(),
            "sleep 600 & sleep 600".into(),
        ],
        environment: Vec::new(),
        directory: sandbox.directory().display().to_string(),
        stdin: false,
    };
    let (sent, ran) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            // the command runs when its handle is told; a cancel came first
            let mut told = |stop: &Stop| {
                let Stop::Group { pgid, leader } = stop else {
                    panic!("{stop:?}")
                };
                assert_eq!(*pgid, leader.pid);
                assert!(leader.alive(), "the handle is told while it runs");
                Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "cancelled as it started",
                ))
            };
            let slot = ExecSlot {
                workspace: "ws1",
                exec: "e1",
                spawned: &mut told,
            };
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let exit = sandbox.run_in(&request, Some(slot), &mut out, &mut err);
            let _ = sent.send(exit.map(|exit| exit.code).map_err(|f| f.cause));
        });
        // it ends at once, as SIGKILL ended it, not after its 600 s
        let exit = ran.recv_timeout(Duration::from_secs(20));
        assert_eq!(format!("{exit:?}"), "Ok(Ok(137))");
    });
    sandbox.stop(1).unwrap();
    sandbox.delete().unwrap();
    std::fs::remove_dir_all(&base).unwrap();
}
