//! S2.2 fairness and bounds through the real CLI (§11, review M3): streamed
//! writes run on write threads of their own, so short calls keep the
//! ordinary threads; an input that stops making progress is refused after
//! its waiting bound, which keepalives do not renew; drain and `machine
//! stop` end a stuck write's input in bounded time with nothing committed;
//! a write past the free space of the home's volume is a definite
//! `Capacity`.
#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;
#[path = "common/host_writes.rs"]
mod writes;

use layerfs_bridge::control::SdkValue;
use std::io::Write;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use writes::*;

/// A CLI write whose input trickles until `stop` is set, then ends.
fn trickle(path: &std::path::Path, target: &str, stop: &Arc<AtomicBool>) -> Child {
    let mut child = cli(path, &["file", "write", "shop/api", "work", target])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let stop = stop.clone();
    std::thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) && input.write_all(&[b'x'; 8 << 10]).is_ok() {
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    child
}

/// A CLI write that sent `bytes` and then stalls with its input open.
fn stalled(path: &std::path::Path, target: &str, bytes: usize) -> (Child, ChildStdin) {
    let mut child = cli(path, &["file", "write", "shop/api", "work", target])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input.write_all(&patterned(bytes)).unwrap();
    (child, input)
}

fn refusal(output: std::process::Output) -> SdkValue {
    assert!(!output.status.success(), "{output:?}");
    let text = if output.stdout.is_empty() {
        output.stderr
    } else {
        output.stdout
    };
    layerfs_api::json::parse(String::from_utf8_lossy(&text).trim()).unwrap()
}

#[test]
fn slow_writers_past_the_write_threads_leave_reads_free() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    let before = commits(path, "shop/api", "work");
    ok(
        path,
        &["file", "write", "shop/api", "work", "/warm"],
        b"w\n",
    );
    // six slow writers: four run on the write threads, two wait for one
    let stop = Arc::new(AtomicBool::new(false));
    let writers: Vec<Child> = (0..6)
        .map(|index| trickle(path, &format!("/slow-{index}"), &stop))
        .collect();
    let held = wait_for("four running and two waiting writes", || {
        let ping = home.ping();
        (ping.get_int("writes") == Some(4) && ping.get_int("writes_waiting") == Some(2))
            .then_some(ping)
    });
    assert_eq!(held.get_int("ordinary"), Some(0), "{held:?}");
    // short calls keep all eight ordinary threads meanwhile
    let began = Instant::now();
    let reads: Vec<Child> = (0..8)
        .map(|_| {
            cli(path, &["file", "read", "shop/api", "work", "/hello"])
                .spawn()
                .unwrap()
        })
        .collect();
    for read in reads {
        let output = finish(read);
        assert!(output.status.success(), "{output:?}");
    }
    assert!(
        began.elapsed() < Duration::from_secs(20),
        "reads waited behind writes: {:?}",
        began.elapsed()
    );
    let ping = home.ping();
    assert_eq!(ping.get_int("writes"), Some(4), "{ping:?}");
    stop.store(true, Ordering::SeqCst);
    for writer in writers {
        let output = finish(writer);
        assert!(output.status.success(), "{output:?}");
    }
    assert_eq!(commits(path, "shop/api", "work"), before + 7);
    drain(&home);
}

#[test]
fn a_stalled_input_is_refused_after_its_waiting_bound_despite_keepalives() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    let before = commits(path, "shop/api", "work");
    // a little input, then nothing: the CLI's keepalives (every 10 s) do
    // not renew the worker's 30 s minimum-progress bound
    let began = Instant::now();
    let (child, input) = stalled(path, "/stalled", 4 << 10);
    let output = finish(child);
    let waited = began.elapsed();
    drop(input);
    let answer = refusal(output);
    assert_eq!(answer.get_text("error"), Some("Unavailable"), "{answer:?}");
    let message = answer.get_text("message").unwrap();
    assert!(
        message.contains("less than") && message.contains("nothing was committed"),
        "{answer:?}"
    );
    assert!(
        waited >= Duration::from_secs(30) && waited < Duration::from_secs(50),
        "{waited:?}"
    );
    assert_eq!(commits(path, "shop/api", "work"), before);
    assert!(retained(path).is_empty());
    drain(&home);
}

/// Waits for the worker to exit; how long it took.
fn exited(home: &fixture::Fixture) -> Duration {
    let began = Instant::now();
    wait_for("the worker to exit", || {
        (!home.paths.socket.exists() && !home.paths.control.exists()).then_some(())
    });
    began.elapsed()
}

fn stopped_unsent(output: std::process::Output, waited: Duration) {
    // the caller learns at its next keepalive (10 s) at the latest
    assert!(waited < Duration::from_secs(15), "{waited:?}");
    let answer = refusal(output);
    assert_eq!(answer.get_text("error"), Some("Unavailable"), "{answer:?}");
    assert!(
        answer
            .get_text("message")
            .unwrap()
            .contains("nothing was committed"),
        "{answer:?}"
    );
}

#[test]
fn drain_and_machine_stop_end_a_stuck_write_in_bounded_time() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    let before = commits(path, "shop/api", "work");
    // drain: the stuck write's input is ended, its answer a refusal
    let (child, input) = stalled(path, "/stuck", 64 << 10);
    wait_for("the stuck write to run", || {
        (home.ping().get_int("writes") == Some(1)).then_some(())
    });
    let began = Instant::now();
    home.drain();
    let exit = exited(&home);
    assert!(exit < Duration::from_secs(10), "{exit:?}");
    let output = finish(child);
    stopped_unsent(output, began.elapsed());
    drop(input);

    // machine stop: the same through the CLI; it may still see the
    // exiting worker's Store hold (`InUse`), never wait on the write
    let (child, input) = stalled(path, "/stuck", 64 << 10);
    wait_for("the second stuck write to run", || {
        (home.ping().get_int("writes") == Some(1)).then_some(())
    });
    let began = Instant::now();
    let (success, answer) = run(path, &["machine", "stop", host()], b"");
    assert!(
        success || answer.get_text("error") == Some("InUse"),
        "{answer:?}"
    );
    assert!(
        began.elapsed() < Duration::from_secs(10),
        "{:?}",
        began.elapsed()
    );
    let exit = exited(&home);
    assert!(exit < Duration::from_secs(10), "{exit:?}");
    let output = finish(child);
    stopped_unsent(output, began.elapsed());
    drop(input);
    ok(path, &["machine", "start", host()], b"");
    assert_eq!(commits(path, "shop/api", "work"), before);
    assert!(retained(path).is_empty());
    drain(&home);
}

/// A small volume for the home: an APFS disk image on macOS; a loop ext4
/// on Linux where this process may mount one. None when it cannot.
struct Small {
    mount: std::path::PathBuf,
    image: std::path::PathBuf,
}

impl Small {
    fn make() -> Option<Self> {
        let mount = std::path::PathBuf::from(format!("/tmp/lfs7n-{}", std::process::id()));
        let image = mount.with_extension("img");
        std::fs::create_dir(&mount).ok()?;
        let run = |words: &[&str]| {
            std::process::Command::new(words[0])
                .args(&words[1..])
                .output()
                .is_ok_and(|output| output.status.success())
        };
        let (m, i) = (mount.to_str()?, image.to_str()?);
        let made = if cfg!(target_os = "macos") {
            let dmg = format!("{i}.dmg");
            run(&[
                "hdiutil", "create", "-size", "96m", "-fs", "APFS", "-volname", "lfs7n", &dmg,
            ]) && run(&["hdiutil", "attach", "-nobrowse", "-mountpoint", m, &dmg])
        } else {
            run(&["truncate", "-s", "96M", i])
                && run(&["mkfs.ext4", "-q", i])
                && run(&["mount", "-o", "loop", i, m])
        };
        let small = Self { mount, image };
        made.then_some(small)
    }
}

impl Drop for Small {
    fn drop(&mut self) {
        let m = self.mount.to_str().unwrap_or_default();
        let _ = if cfg!(target_os = "macos") {
            std::process::Command::new("hdiutil")
                .args(["detach", m, "-force"])
                .output()
        } else {
            std::process::Command::new("umount").arg(m).output()
        };
        let _ = std::fs::remove_dir(&self.mount);
        let _ = std::fs::remove_file(&self.image);
        let _ = std::fs::remove_file(self.image.with_extension("img.dmg"));
    }
}

#[test]
fn a_write_past_the_free_space_is_a_definite_capacity_refusal() {
    let Some(small) = Small::make() else {
        eprintln!("NOT_RUN: no small volume can be mounted here");
        return;
    };
    let home = prepare_in(fixture::Fixture::at(small.mount.join("h")), &["work"]);
    let path = &home.paths.home;
    ok(
        path,
        &["file", "write", "shop/api", "work", "/warm"],
        b"w\n",
    );
    let before = commits(path, "shop/api", "work");
    // patterned, so neither compression nor deduplication makes it fit
    let (success, answer) = run(
        path,
        &["file", "write", "shop/api", "work", "/big"],
        &patterned(192 << 20),
    );
    assert!(!success, "{answer:?}");
    assert_eq!(answer.get_text("error"), Some("Capacity"), "{answer:?}");
    assert_eq!(commits(path, "shop/api", "work"), before);
    let (found, _) = run(path, &["file", "stat", "shop/api", "work", "/big"], b"");
    assert!(!found);
    assert_eq!(home.ping().get_text("state"), Some("ready"));
    assert!(retained(path).is_empty());
    assert_eq!(retained_check(path).get_text("outcome"), Some("pass"));
    ok(
        path,
        &["file", "write", "shop/api", "work", "/after"],
        b"a\n",
    );
    assert_eq!(commits(path, "shop/api", "work"), before + 1);
    drain(&home);
    drop(home);
    drop(small);
}
