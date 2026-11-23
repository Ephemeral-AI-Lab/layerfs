//! A provider call's own deadline (no probe deadline around it): the whole
//! process group is killed then, and a descendant that holds the call's
//! output cannot keep it waiting; `limactl create` gets the create bound.
//! Slow: each case waits out a real 10 s or 30 s bound.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{self as machine, probe, Code, HostOs, Platform};
use standin::Scratch;
use std::time::{Duration, Instant};

/// The version query's own bound (`probe.rs` `ANSWER`).
const ANSWER: Duration = Duration::from_secs(10);

/// Whether the process whose pid the stand-in wrote to `file` is gone,
/// asked for up to 2 s (it is reparented and reaped after the kill).
fn gone(scratch: &Scratch, file: &str) -> bool {
    let pid = standin::read(&scratch.path(file));
    let pid = pid.trim();
    assert!(!pid.is_empty(), "the stand-in wrote no pid");
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        let alive = std::process::Command::new("/bin/kill")
            .args(["-0", pid])
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if !alive {
            return true;
        }
        if Instant::now() >= until {
            // leave nothing behind for the next test
            let _ = std::process::Command::new("/bin/kill")
                .args(["-9", pid])
                .status();
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// `limactl --version` (the probe's one call, bounded by [`ANSWER`]) that
/// leaves a 60 s `sleep` holding its stdout, then itself either sleeps past
/// the bound (`hang`) or exits at once.
fn held(label: &str, hang: bool) -> Scratch {
    let scratch = Scratch::new(label);
    let end = if hang { "sleep 60" } else { "exit 0" };
    scratch.tool(
        "limactl",
        &format!("echo 'limactl version 9.9.9'; sleep 60 & echo $! > \"$SCRATCH/held\"; {end}"),
    );
    scratch
}

#[test]
fn a_timed_out_call_kills_the_descendant_holding_its_output() {
    let scratch = held("deadline-hang", true);
    let started = Instant::now();
    let found = probe(Platform::Lima, &scratch.tools());
    let took = started.elapsed();
    // before, only the direct child was killed and the readers waited for
    // the 60 s `sleep`
    assert!(took >= ANSWER, "{took:?}");
    assert!(took < ANSWER + Duration::from_secs(5), "{took:?}");
    assert_eq!(found.version, None, "a timeout is Unknown, not an answer");
    assert!(
        gone(&scratch, "held"),
        "the output-holding descendant runs on"
    );
}

#[test]
fn an_ended_call_whose_output_stays_held_ends_by_its_deadline() {
    let scratch = held("deadline-held", false);
    let started = Instant::now();
    let found = probe(Platform::Lima, &scratch.tools());
    let took = started.elapsed();
    assert!(took < ANSWER + Duration::from_secs(5), "{took:?}");
    // its output never ended, so what it said is not known
    assert_eq!(found.version, None);
    assert!(
        gone(&scratch, "held"),
        "the output-holding descendant runs on"
    );
}

#[test]
fn lima_create_gets_the_create_bound_not_the_call_bound() {
    if HostOs::current() != HostOs::Macos {
        return;
    }
    let scratch = Scratch::new("deadline-lima-create");
    // `create` outlasts the 30 s call bound, then fails with nothing made
    scratch.tool(
        "limactl",
        r#"case "$1" in
  create) cat > /dev/null; sleep 31; echo "no image" >&2; exit 1 ;;
esac
exit 0"#,
    );
    let started = Instant::now();
    let error = machine::create(&scratch.spec(Platform::Lima), &scratch.bundle()).unwrap_err();
    assert!(started.elapsed() >= Duration::from_secs(31));
    // at 30 s this was `Unknown` ("did not end within 30 s")
    assert_eq!(error.code, Code::Failed, "{error}");
    assert!(error.message.contains("no image"), "{error}");
    assert!(!error.vm_exists);
}
