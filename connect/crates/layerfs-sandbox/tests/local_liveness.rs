//! A native (macOS) command's own process is a liveness source: a quiet
//! command keeps reporting while it lives, and the reports stop once it has
//! exited even when a descendant still holds its pipes. A small shell script
//! stands in for the daemon, as in `local_search_path`.
#![cfg(target_os = "macos")]
use layerfs_sandbox::{
    docker::ExecRequest, host_process::Sandbox, local::Local, CommandIdentity, Liveness,
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(10);
const EVERY: Duration = Duration::from_millis(100);

/// A fresh 0755 runtime root and a stand-in daemon that prints its listener
/// line, then waits.
fn fixture() -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("layerfs-liveness-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(base.join("root")).unwrap();
    fs::set_permissions(&base, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(base.join("root"), fs::Permissions::from_mode(0o755)).unwrap();
    let daemon = base.join("daemon.sh");
    fs::write(
        &daemon,
        "#!/bin/sh\n[ \"$1\" = --config ] && [ -f \"$2\" ] || exit 3\necho \"LAYERFS_DAEMON_LISTEN 127.0.0.1:40424\"\nexec /bin/sleep 60\n",
    )
    .unwrap();
    fs::set_permissions(&daemon, fs::Permissions::from_mode(0o755)).unwrap();
    (base.join("root"), daemon)
}

fn started(root: &Path, daemon: &Path, name: &str) -> Sandbox {
    Local::native(root)
        .unwrap()
        .naming(name)
        .unwrap()
        .create(daemon, b"setup bytes", WAIT, Vec::new())
        .unwrap()
}

fn request(sandbox: &Sandbox, script: &str) -> ExecRequest {
    ExecRequest {
        identity: CommandIdentity {
            uid: nix::unistd::Uid::effective().as_raw(),
            gid: nix::unistd::Gid::effective().as_raw(),
        },
        arguments: vec!["/bin/sh".into(), "-c".into(), script.into()],
        environment: Vec::new(),
        directory: sandbox.directory().display().to_string(),
        stdin: false,
    }
}

#[test]
fn a_quiet_command_reports_its_live_process_then_completes() {
    let (root, daemon) = fixture();
    let mut sandbox = started(&root, &daemon, "layerfs-live-1");
    let reports = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&reports);
    let mut report = move |pid: Option<u32>| {
        assert!(pid.is_some(), "a native command knows its process id");
        counted.fetch_add(1, Ordering::SeqCst);
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let started_at = Instant::now();
    let exit = sandbox
        .run_reporting(
            &request(&sandbox, "/bin/sleep 1"),
            &mut out,
            &mut err,
            Some(Liveness {
                every: EVERY,
                report: &mut report,
            }),
        )
        .unwrap();
    assert_eq!(exit.code, 0, "{}", String::from_utf8_lossy(&err));
    assert!(
        started_at.elapsed() >= Duration::from_millis(900),
        "{:?}",
        started_at.elapsed()
    );
    // no output, no exit: only the live process kept the caller informed
    assert!(
        reports.load(Ordering::SeqCst) >= 5,
        "only {} reports",
        reports.load(Ordering::SeqCst)
    );
    sandbox.stop(1).unwrap();
    sandbox.delete().unwrap();
    let _ = fs::remove_dir_all(root.parent().unwrap());
}

#[test]
fn reports_stop_when_the_command_exits_even_if_a_descendant_holds_the_pipes() {
    let (root, daemon) = fixture();
    let mut sandbox = started(&root, &daemon, "layerfs-live-2");
    let reports = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(AtomicBool::new(false));
    let counted = Arc::clone(&reports);
    let done = Arc::clone(&finished);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let mut report = |_pid: Option<u32>| {
                counted.fetch_add(1, Ordering::SeqCst);
            };
            let mut out = Vec::new();
            let mut err = Vec::new();
            // the shell exits at once; its background `sleep` keeps the pipes
            // open, so the operation stays blocked long after its child is gone
            let exit = sandbox
                .run_reporting(
                    &request(&sandbox, "/bin/sleep 2 & exit 0"),
                    &mut out,
                    &mut err,
                    Some(Liveness {
                        every: EVERY,
                        report: &mut report,
                    }),
                )
                .unwrap();
            assert_eq!(exit.code, 0, "{}", String::from_utf8_lossy(&err));
            done.store(true, Ordering::SeqCst);
        });
        // longer than several report intervals: a live child would have
        // reported by now, and this one has exited, so the bound starts here
        std::thread::sleep(Duration::from_millis(700));
        assert_eq!(
            reports.load(Ordering::SeqCst),
            0,
            "a child that exited (a zombie) is not a live process"
        );
        assert!(
            !finished.load(Ordering::SeqCst),
            "the operation is still blocked on the descendant's pipes"
        );
    });
    assert!(finished.load(Ordering::SeqCst));
    sandbox.stop(1).unwrap();
    sandbox.delete().unwrap();
    let _ = fs::remove_dir_all(root.parent().unwrap());
}
