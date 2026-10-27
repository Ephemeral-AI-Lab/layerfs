//! The local process runtime (`backend::local`): one daemon process per
//! Sandbox directory, observed by process id and start time, its commands run
//! as their own identity, and its stop and delete each attempted once. A
//! small shell script stands in for the daemon; nothing here needs root.
//! The runtime is Linux only, and so are these tests.
#![cfg(target_os = "linux")]
use layerfs_sandbox::{
    backend::{
        docker::ExecRequest,
        local::{Local, LocalPhase, LocalProcess, Sandbox},
    },
    CommandIdentity, RuntimeError,
};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const WAIT: Duration = Duration::from_secs(10);

/// A fresh 0755 runtime root with a stand-in daemon `daemon.sh` beside it.
fn fixture(script: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!(
        "layerfs-local-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(base.join("root")).unwrap();
    fs::set_permissions(&base, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(base.join("root"), fs::Permissions::from_mode(0o755)).unwrap();
    let daemon = base.join("daemon.sh");
    fs::write(&daemon, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(&daemon, fs::Permissions::from_mode(0o755)).unwrap();
    (base.join("root"), daemon)
}

/// Prints the listener line after checking its arguments, then waits.
const LISTENS: &str = r#"[ "$1" = --config ] && [ -f "$2" ] || exit 3
env >&2
echo "starting"
echo "LAYERFS_DAEMON_LISTEN 127.0.0.1:40404"
exec sleep 60"#;

/// The command identity: this user, or nobody when the tests run as root.
fn me() -> CommandIdentity {
    let meta = fs::metadata("/proc/self").unwrap();
    if meta.uid() == 0 {
        CommandIdentity {
            uid: 65534,
            gid: 65534,
        }
    } else {
        CommandIdentity {
            uid: meta.uid(),
            gid: meta.gid(),
        }
    }
}

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().mode() & 0o7777
}

fn started(root: &Path, daemon: &Path, name: &str) -> Sandbox {
    Local::new(root)
        .unwrap()
        .naming(name)
        .unwrap()
        .create(
            daemon,
            b"setup bytes",
            WAIT,
            vec![("layerfs.role".into(), "sandbox".into())],
        )
        .unwrap()
}

fn run(sandbox: &Sandbox, arguments: &[&str], environment: &[&str]) -> (i32, String, String) {
    let request = ExecRequest {
        identity: me(),
        arguments: arguments.iter().map(|a| a.to_string()).collect(),
        environment: environment.iter().map(|a| a.to_string()).collect(),
        directory: sandbox.directory().display().to_string(),
        stdin: false,
    };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let exit = sandbox.run(&request, &mut out, &mut err).unwrap();
    assert_eq!(exit.stdout, out.len() as u64);
    assert_eq!(exit.stderr, err.len() as u64);
    (
        exit.code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

#[test]
fn a_root_must_be_absolute_owned_private_to_write_and_traversable() {
    let (root, _) = fixture("exit 0");
    assert!(matches!(
        Local::new("relative/root"),
        Err(RuntimeError::Protocol(_))
    ));
    assert!(matches!(
        Local::new(root.join("..").join("root")),
        Err(RuntimeError::Protocol(_))
    ));
    assert!(matches!(
        Local::new(root.join("missing")),
        Err(RuntimeError::Io(_))
    ));
    fs::set_permissions(&root, fs::Permissions::from_mode(0o775)).unwrap();
    assert!(matches!(Local::new(&root), Err(RuntimeError::Protocol(_))));
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(Local::new(&root), Err(RuntimeError::Protocol(_))));
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    let local = Local::new(&root).unwrap();
    for bad in ["", ".hidden", "-x", "a/b", "..", "a b", &"x".repeat(129)] {
        assert!(local.clone().naming(bad).is_err(), "{bad:?} admitted");
    }
    assert!(local.paths().is_err(), "paths before a name");
    let paths = local.naming("layerfs-shop-1").unwrap().paths().unwrap();
    assert_eq!(paths.directory, root.join("layerfs-shop-1"));
    assert_eq!(paths.config, paths.directory.join("config/daemon.setup"));
    assert_eq!(
        paths.overlay,
        paths.directory.join("overlay/overlay.sqlite")
    );
    assert_eq!(paths.mounts, paths.directory.join("workspaces"));
    assert_eq!(paths.log, paths.directory.join("daemon.log"));
    assert_eq!(paths.tmp, paths.directory.join("tmp"));
}

#[test]
fn a_start_writes_a_private_setup_spawns_once_and_reads_the_listener() {
    let (root, daemon) = fixture(LISTENS);
    let mut sandbox = started(&root, &daemon, "layerfs-shop-1");
    let directory = root.join("layerfs-shop-1");
    assert_eq!(sandbox.directory(), directory);
    assert_eq!(sandbox.port(), 40404);
    assert_eq!(
        sandbox.endpoint().unwrap(),
        "127.0.0.1:40404".parse().unwrap()
    );
    assert_eq!(
        sandbox.labels(),
        [("layerfs.role".to_owned(), "sandbox".to_owned())]
    );
    let process = sandbox.process().unwrap();
    assert!(process.alive());
    assert_eq!(LocalProcess::observe(process.pid).unwrap(), process);
    assert!(!LocalProcess {
        start: process.start + 1,
        ..process
    }
    .alive());
    // private setup, private overlay home, traversable Sandbox directory
    assert_eq!(mode(&directory), 0o755);
    assert_eq!(mode(&directory.join("config")), 0o700);
    assert_eq!(mode(&directory.join("config/daemon.setup")), 0o600);
    assert_eq!(
        fs::read(directory.join("config/daemon.setup")).unwrap(),
        b"setup bytes"
    );
    assert_eq!(mode(&directory.join("overlay")), 0o700);
    assert_eq!(mode(&directory.join("daemon.log")), 0o600);
    assert_eq!(mode(&directory.join("tmp")), 0o1777);
    assert_eq!(sandbox.tmp(), directory.join("tmp"));
    // the daemon inherits nothing of this process's environment but PATH
    let log = String::from_utf8(sandbox.log_tail(100, 4096).unwrap()).unwrap();
    assert!(log.contains("PATH=/usr/local/sbin:"), "{log}");
    assert!(!log.contains("CARGO"), "{log}");
    assert_eq!(
        String::from_utf8(sandbox.log_tail(1, 4096).unwrap()).unwrap(),
        "LAYERFS_DAEMON_LISTEN 127.0.0.1:40404\n"
    );
    assert_eq!(sandbox.log_tail(1, 5).unwrap(), b"0404\n");
    // the same name again is refused before anything is made
    let again = Local::new(&root)
        .unwrap()
        .naming("layerfs-shop-1")
        .unwrap()
        .create(&daemon, b"x", WAIT, Vec::new())
        .unwrap_err();
    assert_eq!(again.phase, LocalPhase::Validate);
    assert!(again.sandbox.is_none());
    // delete needs a known stop; stop ends the process; each is attempted once
    assert!(sandbox.delete().is_err());
    let mut sandbox = Sandbox::reopen(
        directory.clone(),
        sandbox.identity(),
        process,
        40404,
        Vec::new(),
    )
    .unwrap();
    sandbox.stop(1).unwrap();
    assert!(!process.alive());
    assert!(sandbox.stop(1).is_err());
    assert!(sandbox.endpoint().is_err());
    sandbox.delete().unwrap();
    assert!(!directory.exists());
    assert!(sandbox.delete().is_err());
}

#[test]
fn a_command_runs_as_its_identity_in_its_directory_with_only_its_environment() {
    let (root, daemon) = fixture(LISTENS);
    let mut sandbox = started(&root, &daemon, "layerfs-shop-2");
    let identity = me();
    let (code, out, err) = run(
        &sandbox,
        &[
            "bash",
            "-c",
            "id -u; id -g; pwd; echo \"$HOME|${CARGO:-none}\"; id -G >&2; echo oops >&2; exit 7",
        ],
        &["HOME=/tmp"],
    );
    assert_eq!(code, 7);
    assert_eq!(
        out,
        format!(
            "{}\n{}\n{}\n/tmp|none\n",
            identity.uid,
            identity.gid,
            sandbox.directory().display()
        )
    );
    let (groups, err) = err.split_once('\n').unwrap();
    assert_eq!(err, "oops\n");
    // as root, the command keeps no supplementary group of this process
    if fs::metadata("/proc/self").unwrap().uid() == 0 {
        assert_eq!(groups, identity.gid.to_string());
    }
    // a command a signal ended reports 128 + the signal, as the Engine does
    let (code, _, _) = run(&sandbox, &["bash", "-c", "kill -9 $$"], &[]);
    assert_eq!(code, 137);
    // more output than one pipe holds, on both streams at once
    let (code, out, err) = run(
        &sandbox,
        &[
            "bash",
            "-c",
            "head -c 300000 /dev/zero | tr '\\0' a; head -c 200000 /dev/zero | tr '\\0' b >&2",
        ],
        &[],
    );
    assert_eq!((code, out.len(), err.len()), (0, 300000, 200000));
    // refused before any effect: root, stdin, a relative directory, a bad variable
    let mut request = ExecRequest {
        identity,
        arguments: vec!["true".into()],
        environment: Vec::new(),
        directory: "/".into(),
        stdin: false,
    };
    for change in [
        (|r: &mut ExecRequest| r.identity = CommandIdentity { uid: 0, gid: 0 })
            as fn(&mut ExecRequest),
        |r| r.stdin = true,
        |r| r.directory = "relative".into(),
        |r| r.environment = vec!["NOEQUALS".into()],
        |r| r.arguments = Vec::new(),
    ] {
        let mut changed = request.clone();
        change(&mut changed);
        let failure = sandbox
            .run(&changed, &mut Vec::new(), &mut Vec::new())
            .unwrap_err();
        assert!(!failure.attempted, "{failure:?}");
    }
    sandbox.stop(1).unwrap();
    // no command once the daemon is stopped
    request.directory = "/".into();
    assert!(sandbox
        .run(&request, &mut Vec::new(), &mut Vec::new())
        .is_err());
    sandbox.delete().unwrap();
}

#[test]
fn the_daemon_and_its_commands_start_with_a_containers_search_path() {
    // on Linux, whatever this process's PATH is (macOS: tests/local_search_path.rs)
    const CONTAINER: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
    let (root, daemon) = fixture(LISTENS);
    let mut sandbox = started(&root, &daemon, "layerfs-shop-path");
    let log = String::from_utf8(sandbox.log_tail(100, 4096).unwrap()).unwrap();
    assert!(
        log.lines().any(|line| line == format!("PATH={CONTAINER}")),
        "{log}"
    );
    let (code, out, _) = run(&sandbox, &["/bin/sh", "-c", "printf %s \"$PATH\""], &[]);
    assert_eq!((code, out.as_str()), (0, CONTAINER));
    // a command's own PATH still wins
    let (code, out, _) = run(
        &sandbox,
        &["/bin/sh", "-c", "printf %s \"$PATH\""],
        &["PATH=/opt/tools/bin:/usr/bin:/bin"],
    );
    assert_eq!((code, out.as_str()), (0, "/opt/tools/bin:/usr/bin:/bin"));
    sandbox.stop(1).unwrap();
    sandbox.delete().unwrap();
}

#[test]
fn a_daemon_that_ends_is_the_stopped_observation_and_only_its_delete_remains() {
    let (root, daemon) = fixture(LISTENS);
    let sandbox = started(&root, &daemon, "layerfs-shop-3");
    let process = sandbox.process().unwrap();
    let directory = sandbox.directory().to_owned();
    // killed from outside, as a crash or the OOM killer would
    assert!(Command::new("kill")
        .args(["-9", &process.pid.to_string()])
        .status()
        .unwrap()
        .success());
    let deadline = Instant::now() + WAIT;
    let failure = loop {
        match sandbox.endpoint() {
            Err(failure) => break failure,
            Ok(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(_) => panic!("killed daemon still observed running"),
        }
    };
    assert!(failure.container_stopped(), "{failure:?}");
    assert!(!failure.container_gone());
    // this process reaped its child: no zombie remains
    let stat = fs::read_to_string(format!("/proc/{}/stat", process.pid));
    assert!(stat.is_err() || !stat.unwrap().contains(") Z "));
    // a reopen from the record observes the same
    let reopened = Sandbox::reopen(
        directory.clone(),
        sandbox.identity(),
        process,
        40404,
        Vec::new(),
    )
    .unwrap_err();
    assert!(reopened.container_stopped());
    let mut stopped = Sandbox::stopped(
        directory.clone(),
        sandbox.identity(),
        process,
        40404,
        Vec::new(),
    );
    assert!(stopped.endpoint().is_err());
    stopped.delete().unwrap();
    assert!(!directory.exists());
}

#[test]
fn a_failed_start_returns_what_it_made() {
    // exits before its listener line
    let (root, daemon) = fixture("echo broken >&2; exit 9");
    let failure = Local::new(&root)
        .unwrap()
        .naming("layerfs-shop-4")
        .unwrap()
        .create(&daemon, b"x", WAIT, Vec::new())
        .unwrap_err();
    assert_eq!(failure.phase, LocalPhase::Listener);
    let mut sandbox = failure.sandbox.unwrap();
    assert_eq!(sandbox.log_tail(5, 100).unwrap(), b"broken\n");
    sandbox.stop(1).unwrap();
    sandbox.delete().unwrap();
    assert!(!root.join("layerfs-shop-4").exists());
    // never prints it: the wait ends
    let (root, daemon) = fixture("exec sleep 60");
    let failure = Local::new(&root)
        .unwrap()
        .naming("layerfs-shop-5")
        .unwrap()
        .create(&daemon, b"x", Duration::from_millis(300), Vec::new())
        .unwrap_err();
    assert_eq!(failure.phase, LocalPhase::Listener);
    assert!(
        matches!(&failure.cause, RuntimeError::Io(e) if e.kind() == std::io::ErrorKind::TimedOut)
    );
    let mut sandbox = failure.sandbox.unwrap();
    let process = sandbox.process().unwrap();
    assert!(process.alive());
    sandbox.stop(1).unwrap();
    assert!(!process.alive());
    sandbox.delete().unwrap();
    // a listener that is not loopback is refused
    let (root, daemon) = fixture("echo 'LAYERFS_DAEMON_LISTEN 0.0.0.0:5'; exec sleep 60");
    let failure = Local::new(&root)
        .unwrap()
        .naming("layerfs-shop-6")
        .unwrap()
        .create(&daemon, b"x", WAIT, Vec::new())
        .unwrap_err();
    assert_eq!(failure.phase, LocalPhase::Listener);
    let mut sandbox = failure.sandbox.unwrap();
    sandbox.stop(1).unwrap();
    sandbox.delete().unwrap();
    // a missing executable or an empty setup makes nothing
    let failure = Local::new(&root)
        .unwrap()
        .naming("layerfs-shop-7")
        .unwrap()
        .create(&root.join("missing"), b"x", WAIT, Vec::new())
        .unwrap_err();
    assert_eq!(
        (failure.phase, failure.sandbox.is_none()),
        (LocalPhase::Validate, true)
    );
    let failure = Local::new(&root)
        .unwrap()
        .naming("layerfs-shop-7")
        .unwrap()
        .create(&daemon, b"", WAIT, Vec::new())
        .unwrap_err();
    assert_eq!(
        (failure.phase, failure.sandbox.is_none()),
        (LocalPhase::Validate, true)
    );
    assert!(!root.join("layerfs-shop-7").exists());
}

#[test]
fn a_zombie_is_not_alive() {
    let mut child = Command::new("true").spawn().unwrap();
    let process = LocalProcess::observe(child.id()).unwrap();
    let deadline = Instant::now() + WAIT;
    while process.alive() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!process.alive(), "an exited, unreaped child counted alive");
    child.wait().unwrap();
}

#[test]
fn every_directory_above_a_root_must_be_traversable_by_others() {
    let (root, _) = fixture("exit 0");
    let base = root.parent().unwrap().to_owned();
    // a root inside a directory others cannot pass, as `/root` (0700) is
    let closed = base.join("closed");
    fs::create_dir(&closed).unwrap();
    fs::create_dir(closed.join("root")).unwrap();
    fs::set_permissions(closed.join("root"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&closed, fs::Permissions::from_mode(0o700)).unwrap();
    let refused = |path: &Path| match Local::new(path) {
        Err(RuntimeError::Io(error)) => {
            assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
            error.to_string()
        }
        other => panic!("{} admitted: {other:?}", path.display()),
    };
    let message = refused(&closed.join("root"));
    assert!(
        message.contains(&format!("{} is not traversable", closed.display())),
        "{message}"
    );
    // the same through a symlinked parent: the directory it resolves to counts
    std::os::unix::fs::symlink(&closed, base.join("link")).unwrap();
    let message = refused(&base.join("link").join("root"));
    assert!(message.contains("is not traversable"), "{message}");
    // traversable without being listable is enough
    fs::set_permissions(&closed, fs::Permissions::from_mode(0o711)).unwrap();
    Local::new(closed.join("root")).unwrap();
    Local::new(base.join("link").join("root")).unwrap();
}

/// The process ids a command printed, one per line.
fn pids(out: &str) -> Vec<u32> {
    out.lines()
        .map(|line| line.trim().parse().unwrap())
        .collect()
}

/// True once none of `pids` runs any more (a zombie has ended).
fn gone(pids: &[u32]) -> bool {
    pids.iter().all(
        |pid| match fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => matches!(
                stat.rsplit_once(") ").map(|(_, rest)| &rest[..1]),
                Some("Z" | "X")
            ),
            Err(_) => true,
        },
    )
}

/// Leaves behind what an exec can: a `nohup` server writing into the
/// Sandbox's scratch directory, and a `setsid` one named with
/// [`Sandbox::track`] as the SDK names its background processes.
fn leave_running(sandbox: &Sandbox) -> Vec<u32> {
    let home = format!("HOME={}", sandbox.tmp().display());
    let (code, out, err) = run(
        sandbox,
        &[
            "bash",
            "-c",
            "cd \"$HOME\"; nohup sh -c 'while :; do echo x >> \"$HOME/server.out\"; sleep 0.01; done' >/dev/null 2>&1 & echo $!",
        ],
        &[&home],
    );
    assert_eq!(code, 0, "{err}");
    let mut left = pids(&out);
    let (code, out, err) = run(
        sandbox,
        &[
            "bash",
            "-c",
            "setsid sleep 300 >/dev/null 2>&1 </dev/null & echo $!",
        ],
        &[],
    );
    assert_eq!(code, 0, "{err}");
    let detached = pids(&out)[0];
    let start = LocalProcess::observe(detached).unwrap().start;
    sandbox.track(detached, start).unwrap();
    left.push(detached);
    // a command that leaves nothing running leaves no group named
    let groups = || {
        fs::read_dir(sandbox.directory().join("groups"))
            .unwrap()
            .count()
    };
    let named = groups();
    assert!(named >= 2, "{named}");
    let (code, _, _) = run(sandbox, &["true"], &[]);
    assert_eq!(code, 0);
    assert_eq!(groups(), named);
    let deadline = Instant::now() + WAIT;
    while !sandbox.tmp().join("server.out").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!gone(&left));
    left
}

#[test]
fn what_commands_leave_running_ends_with_the_sandbox() {
    // stopped and deleted by its owner
    let (root, daemon) = fixture(LISTENS);
    let mut sandbox = started(&root, &daemon, "layerfs-shop-8");
    let left = leave_running(&sandbox);
    sandbox.stop(1).unwrap();
    assert!(gone(&left), "{left:?} outlived the stop");
    sandbox.delete().unwrap();
    assert!(!root.join("layerfs-shop-8").exists());
    // its daemon killed from outside, then found stopped from its record
    // and deleted, as a reclaim does
    let sandbox = started(&root, &daemon, "layerfs-shop-9");
    let left = leave_running(&sandbox);
    let process = sandbox.process().unwrap();
    assert!(Command::new("kill")
        .args(["-9", &process.pid.to_string()])
        .status()
        .unwrap()
        .success());
    let deadline = Instant::now() + WAIT;
    while sandbox.endpoint().is_ok() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!gone(&left), "a command ended with its daemon");
    let directory = sandbox.directory().to_owned();
    let mut stopped = Sandbox::stopped(
        directory.clone(),
        sandbox.identity(),
        process,
        40404,
        Vec::new(),
    );
    stopped.delete().unwrap();
    assert!(gone(&left), "{left:?} outlived the delete");
    assert!(!directory.exists());
}

#[test]
fn a_process_of_another_boot_is_not_alive_and_one_without_a_boot_is_never_signalled() {
    let (root, daemon) = fixture(LISTENS);
    let sandbox = started(&root, &daemon, "layerfs-shop-10");
    let process = sandbox.process().unwrap();
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap();
    let boot = u128::from_str_radix(&boot.trim().replace('-', ""), 16).unwrap();
    assert_eq!(process.boot, Some(boot));
    // the same id and start time in another boot (a reboot or a restored
    // snapshot) is another process
    let other = LocalProcess {
        boot: Some(boot ^ 1),
        ..process
    };
    assert!(!other.alive());
    assert!(Sandbox::reopen(
        root.join("layerfs-shop-10"),
        sandbox.identity(),
        other,
        40404,
        Vec::new()
    )
    .unwrap_err()
    .container_stopped());
    // a record from before boot ids is matched by id and start as then, and
    // a stop through it signals nothing
    let legacy = LocalProcess {
        boot: None,
        ..process
    };
    assert!(legacy.alive());
    let mut reopened = Sandbox::reopen(
        root.join("layerfs-shop-10"),
        sandbox.identity(),
        legacy,
        40404,
        Vec::new(),
    )
    .unwrap();
    assert!(reopened.stop(1).is_err());
    assert!(process.alive());
    let mut sandbox = sandbox;
    sandbox.stop(1).unwrap();
    assert!(!process.alive());
    sandbox.delete().unwrap();
}

/// A daemon whose main thread ends while another thread still runs: its
/// id is then a zombie that cannot be reaped yet, so it is not alive, but
/// its exit has not happened. A stop ends and reaps it before returning,
/// so it never stays a zombie of this process (the stand-in needs python3).
#[test]
fn a_stop_reaps_a_daemon_whose_main_thread_ended_first() {
    let python = Command::new("python3").args(["-c", ""]).status();
    if !python.is_ok_and(|status| status.success()) {
        eprintln!("skipped: the stand-in daemon needs python3");
        return;
    }
    let (root, daemon) = fixture(
        r#"echo "LAYERFS_DAEMON_LISTEN 127.0.0.1:40404"
exec python3 -c 'import ctypes, threading, time; threading.Thread(target=time.sleep, args=(60,)).start(); ctypes.CDLL(None).pthread_exit(None)'"#,
    );
    let mut sandbox = started(&root, &daemon, "layerfs-shop-11");
    let process = sandbox.process().unwrap();
    let deadline = Instant::now() + WAIT;
    while process.alive() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!process.alive(), "its main thread did not end");
    let tasks = fs::read_dir(format!("/proc/{}/task", process.pid))
        .unwrap()
        .count();
    assert!(tasks >= 2, "its other thread ended already ({tasks})");
    sandbox.stop(1).unwrap();
    assert!(
        fs::metadata(format!("/proc/{}", process.pid)).is_err(),
        "the stopped daemon is still a process of this kernel (unreaped)"
    );
    sandbox.delete().unwrap();
    assert!(!root.join("layerfs-shop-11").exists());
}
