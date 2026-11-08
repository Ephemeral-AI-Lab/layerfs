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
use layerfs_sandbox_backend::platform_policy::disabled_reason;
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
    for kind in [
        Platform::Docker,
        Platform::AppleContainer,
        Platform::Firecracker,
    ] {
        let mut spec = scratch.spec(kind);
        if kind == Platform::Firecracker {
            let mut host = scratch.spec(Platform::Lima);
            host.nested = true;
            spec.host = Some(Box::new(host));
        }
        let error = machine::forward(&spec, 40006).unwrap_err();
        if let Some(reason) = disabled_reason(kind.name()) {
            assert_eq!(error.code, Code::Unsupported);
            assert_eq!(error.message, reason);
            assert!(!error.vm_exists);
            continue;
        }
        let (code, why) = match kind {
            Platform::Docker => (Code::Unsupported, "Docker publishes a container's ports only when it starts, so a Sandbox daemon's port in the worker container cannot be published later; its Workspace calls go through the worker"),
            Platform::AppleContainer => (Code::Unsupported, "the container CLI publishes a port only when the machine's container is created, so a Sandbox daemon's port cannot be published later; its Workspace calls go through the worker"),
            _ => (Code::Unsupported, "its Sandbox daemons listen on the microVM's loopback, which a forward through the Lima host cannot reach (it reaches only the microVM's tap address); its Workspace calls go through the worker"),
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

#[test]
fn port_zero_is_refused_before_any_tool_runs() {
    let scratch = Scratch::new("forward-zero");
    scratch.tool("ssh", FORWARDING);
    for kind in [Platform::Native, Platform::Lima, Platform::Firecracker] {
        let error = machine::forward(&scratch.spec(kind), 0).unwrap_err();
        assert_eq!(
            error.code,
            if disabled_reason(kind.name()).is_some() {
                Code::Unsupported
            } else {
                Code::InvalidInput
            },
            "{kind:?}"
        );
    }
    assert!(scratch.log().is_empty(), "{}", scratch.log());
}
