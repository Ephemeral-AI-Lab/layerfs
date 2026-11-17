//! `forward` (#535 G4, §7.2): the host loopback address at which a caller
//! reaches a Sandbox daemon on `127.0.0.1:<guest_port>` in the machine, and
//! `forward_close`. `lima` starts a detached stand-in `ssh -L`
//! (a shell script that waits until it is ended); `native` answers the port
//! as it is; the others refuse before any tool runs. No host, VM or
//! container is reached.
//!
//! A stand-in `ssh` is run once before its forward ([`warm`]): macOS
//! assesses a new script at its first run (about 0.4 s here), which would
//! otherwise eat the forward's 0.6 s settle time.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{self as machine, Code, Forward, Platform, Spec};
use standin::Scratch;
use std::net::SocketAddr;
use std::process::Command;
use std::time::{Duration, Instant};

/// A stand-in `ssh -N -L` that runs, as a forward does, until it is ended;
/// `ssh -V` ends at once ([`warm`]).
const FORWARDING: &str =
    "[ \"$1\" = -V ] && exit 0\ntrap 'kill $! 2>/dev/null; exit 0' TERM\nsleep 60 &\nwait";

/// Runs the stand-in `ssh -V` once, so its first-run cost is paid before a
/// forward's settle time starts.
fn warm(scratch: &Scratch) {
    Command::new(scratch.bin().join("ssh"))
        .arg("-V")
        .output()
        .unwrap();
}

/// The stand-ins first, then the system's `sh` and `ps`, which a detached
/// forward is started and checked with.
fn with_system(scratch: &Scratch, mut spec: Spec) -> Spec {
    let mut path = scratch.bin().into_os_string();
    path.push(":/bin:/usr/bin");
    spec.tools.path = path;
    spec
}

/// A lima spec whose stand-in `limactl list` answers `status` and the
/// instance folder `$SCRATCH/lima`.
fn lima_spec(scratch: &Scratch, status: &str) -> Spec {
    scratch.tool(
        "limactl",
        &format!(r#"[ "$1" = list ] && printf '{status}\t%s\n' "$SCRATCH/lima"; exit 0"#),
    );
    with_system(scratch, scratch.spec(Platform::Lima))
}

fn runs(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .unwrap()
        .success()
}

/// The forwards the stand-in `ssh` logged.
fn ssh_calls(scratch: &Scratch) -> Vec<String> {
    scratch
        .log()
        .lines()
        .filter(|l| l.starts_with("ssh ") && l.contains(" -L "))
        .map(str::to_owned)
        .collect()
}

/// The forwards logged once `count` are: a detached stand-in logs on its
/// own time (a bounded readiness wait, at most 5 s).
fn logged(scratch: &Scratch, count: usize) -> Vec<String> {
    let until = Instant::now() + Duration::from_secs(5);
    while ssh_calls(scratch).len() < count && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    ssh_calls(scratch)
}

/// The forward's host loopback address and its pid; the address's port is free
/// and not the daemon's.
fn opened(forward: &Forward, guest_port: u16) -> (u16, u32) {
    assert!(forward.address.ip().is_loopback(), "{forward:?}");
    assert_ne!(forward.address.port(), guest_port);
    let pid = forward.pid.expect("a forward with a pid");
    assert!(runs(pid), "{forward:?}");
    (forward.address.port(), pid)
}

#[test]
fn native_answers_the_daemons_port_and_has_nothing_to_close() {
    let scratch = Scratch::new("forward-native");
    let spec = scratch.spec(Platform::Native);
    let forward = machine::forward(&spec, 43210).unwrap();
    let expected: SocketAddr = "127.0.0.1:43210".parse().unwrap();
    assert_eq!(
        forward,
        Forward {
            address: expected,
            pid: None
        }
    );
    machine::forward_close(&spec, &forward);
    assert!(scratch.log().is_empty(), "{}", scratch.log());
}

#[test]
fn a_forward_that_ends_at_once_fails_once_and_leaves_nothing() {
    let scratch = Scratch::new("forward-lima-busy");
    scratch.tool(
        "ssh",
        "[ \"$1\" = -V ] && exit 0\necho 'bind [127.0.0.1]: Address already in use' >&2; exit 255",
    );
    warm(&scratch);
    let spec = lima_spec(&scratch, "Running");
    let error = machine::forward(&spec, 40003).unwrap_err();
    assert_eq!(error.code, Code::Failed, "{error}");
    assert!(error.message.contains("ended at once"), "{error}");
    assert!(error.message.contains("Address already in use"), "{error}");
    assert!(!error.vm_exists);
    assert_eq!(ssh_calls(&scratch).len(), 1, "one attempt, no retry");
}

#[test]
fn lima_forwards_through_its_own_ssh_config() {
    let scratch = Scratch::new("forward-lima");
    scratch.tool("ssh", FORWARDING);
    warm(&scratch);
    let spec = lima_spec(&scratch, "Running");
    let forward = machine::forward(&spec, 40004).unwrap();
    let (port, pid) = opened(&forward, 40004);
    assert_eq!(
        logged(&scratch, 1),
        [format!(
            "ssh -N -o ExitOnForwardFailure=yes -F {}/ssh.config -o ControlMaster=no \
             -o ControlPath=none -L 127.0.0.1:{port}:127.0.0.1:40004 lima-lfs-dev",
            scratch.path("lima").display()
        )]
    );
    machine::forward_close(&spec, &forward);
    assert!(!runs(pid));
}

#[test]
fn a_stopped_lima_machine_forwards_nothing() {
    let scratch = Scratch::new("forward-lima-stopped");
    scratch.tool("ssh", FORWARDING);
    let spec = lima_spec(&scratch, "Stopped");
    let error = machine::forward(&spec, 40005).unwrap_err();
    assert_eq!(error.code, Code::Unavailable, "{error}");
    assert!(ssh_calls(&scratch).is_empty());
}

#[test]
fn the_other_platforms_refuse_before_any_tool_runs() {
    let scratch = Scratch::new("forward-refused");
    for kind in [Platform::Docker, Platform::AppleContainer] {
        let spec = scratch.spec(kind);
        let error = machine::forward(&spec, 40006).unwrap_err();
        let (code, why) = match kind {
            Platform::Docker => (Code::Unsupported, "Docker publishes a container's ports only when it starts, so a Sandbox daemon's port in the worker container cannot be published later; its Workspace calls go through the worker"),
            _ => (Code::Unsupported, "the container CLI publishes a port only when the machine's container is created, so a Sandbox daemon's port cannot be published later; its Workspace calls go through the worker"),
        };
        assert_eq!(error.code, code, "{kind:?}");
        assert_eq!(
            error.message,
            format!(
                "the {} machine dev forwards no Sandbox daemon port: {why}",
                kind.name()
            )
        );
        assert!(!error.vm_exists);
    }
    assert!(scratch.log().is_empty(), "{}", scratch.log());
}

/// A stand-in forward that says so on its stderr (the machine's
/// `forward.log`) when it starts and again at each `USR1`.
const TALKING: &str = r#"[ "$1" = -V ] && exit 0
echo "forward up" >&2
trap 'echo "forward still here" >&2' USR1
trap 'kill $! 2>/dev/null; exit 0' TERM
sleep 60 &
while kill -0 $! 2>/dev/null; do wait; done"#;

/// The text of `log` once it holds `part` (a bounded wait, at most 5 s:
/// a detached stand-in writes on its own time).
fn holding(log: &std::path::Path, part: &str) -> String {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let text = String::from_utf8_lossy(&std::fs::read(log).unwrap()).into_owned();
        if text.contains(part) || Instant::now() >= until {
            return text;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// layerfs-project#180: a machine's forwards share `forward.log`. A
/// forward that starts past 16 MiB rotates it once to `forward.1.log`,
/// and a forward already running keeps writing into `forward.log` itself:
/// the log is emptied in place, never renamed under it.
#[test]
fn a_forward_start_rotates_the_shared_log_in_place_under_a_running_forward() {
    use std::os::unix::fs::MetadataExt;
    const SIZE: u64 = 16 << 20;
    let scratch = Scratch::new("forward-log");
    scratch.tool("ssh", TALKING);
    warm(&scratch);
    let spec = lima_spec(&scratch, "Running");
    let log = spec.work_dir.join("forward.log");
    let rotated = spec.work_dir.join("forward.1.log");
    let sized = |size: u64| {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&log)
            .unwrap();
        file.set_len(size).unwrap();
    };
    // a log at the size is appended to, not rotated
    sized(SIZE);
    let first = machine::forward(&spec, 40007).unwrap();
    let (_, first_pid) = opened(&first, 40007);
    assert!(holding(&log, "forward up").ends_with("forward up\n"));
    assert!(!rotated.exists());
    let held = std::fs::metadata(&log).unwrap().ino();
    let past = std::fs::metadata(&log).unwrap().len();
    assert!(past > SIZE);
    // the second forward starts past the size, while the first holds the log
    std::fs::write(&rotated, "the oldest\n").unwrap();
    let second = machine::forward(&spec, 40008).unwrap();
    opened(&second, 40008);
    assert_eq!(std::fs::metadata(&rotated).unwrap().len(), past);
    assert_eq!(holding(&log, "forward up"), "forward up\n");
    assert_eq!(
        std::fs::metadata(&log).unwrap().ino(),
        held,
        "emptied in place, never renamed"
    );
    // the running forward's next line lands in the log, not the rotated file
    Command::new("kill")
        .args(["-USR1", &first_pid.to_string()])
        .status()
        .unwrap();
    assert_eq!(
        holding(&log, "still here"),
        "forward up\nforward still here\n"
    );
    assert_eq!(std::fs::metadata(&rotated).unwrap().len(), past);
    // once: a third start below the size leaves the rotated file
    let third = machine::forward(&spec, 40009).unwrap();
    assert_eq!(
        holding(&log, "here\nforward up"),
        "forward up\nforward still here\nforward up\n"
    );
    assert_eq!(std::fs::metadata(&rotated).unwrap().len(), past);
    for forward in [first, second, third] {
        machine::forward_close(&spec, &forward);
    }
}

#[test]
fn port_zero_is_refused_before_any_tool_runs() {
    let scratch = Scratch::new("forward-zero");
    scratch.tool("ssh", FORWARDING);
    for kind in [Platform::Native, Platform::Lima] {
        let error = machine::forward(&scratch.spec(kind), 0).unwrap_err();
        assert_eq!(error.code, Code::InvalidInput, "{kind:?}");
    }
    assert!(scratch.log().is_empty(), "{}", scratch.log());
}
