//! S2.2 fairness and bounds (§11, review M3, round 5): streamed writes run
//! on write threads of their own, so short calls keep the ordinary
//! threads, and a write queued behind them waits for its `go` with nothing
//! sent; an input that stops making progress once the worker reads it is
//! refused after its waiting bound, which keepalives do not renew; drain
//! and `machine stop` never stop an admitted write: it finishes, and the
//! stop names it; a write past the free space of the home's volume is a
//! definite `Capacity`. The CLI spools any piped input before it dials,
//! so slow and stalled inputs here are raw callers on the socket.
#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;
#[path = "common/host_writes.rs"]
mod writes;

use layerfs_home::json;
use layerfs_protocol::sdk::SdkValue;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};
use writes::*;

/// A raw streamed write of `target` held at its `go`: it sends its
/// request, says on `went` when the worker asks for its input, then sends
/// `content` once told on `send`; its answer.
fn held_write(
    home: &fixture::Fixture,
    id: String,
    target: String,
    content: Vec<u8>,
    went: std::sync::mpsc::Sender<String>,
    send: Receiver<()>,
) -> std::thread::JoinHandle<SdkValue> {
    let exchange = home.dial_writes();
    std::thread::spawn(move || {
        let mut socket: &UnixStream = exchange.socket();
        socket
            .write_all(&content_request(&exchange, &id, &target))
            .unwrap();
        read_go(exchange.socket(), &id);
        went.send(id).unwrap();
        send.recv().unwrap();
        socket.write_all(&content_tail(&content)).unwrap();
        raw_answer(exchange.socket())
    })
}

fn committed(answer: &SdkValue) {
    let ok = answer.get("ok").unwrap_or_else(|| panic!("{answer:?}"));
    assert_eq!(ok.get_text("kind"), Some("Committed"), "{answer:?}");
}

#[test]
fn writes_queued_past_the_write_threads_wait_for_go_and_leave_reads_free() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    ok(
        path,
        &["file", "write", "shop/api", "work", "/warm"],
        b"w\n",
    );
    let before = commits(path, "shop/api", "work");
    // six held writes: four get `go` on the write threads, two wait
    let (went, gone) = channel();
    let mut senders = Vec::new();
    let writers: Vec<_> = (0..6)
        .map(|index| {
            let (send, told) = channel();
            senders.push(send);
            let id = format!("{index:032x}");
            held_write(
                &home,
                id,
                format!("/slow-{index}"),
                vec![b'x'; 8 << 10],
                went.clone(),
                told,
            )
        })
        .collect();
    let held = wait_for("four running and two waiting writes", || {
        let ping = home.ping();
        (ping.get_int("writes") == Some(4) && ping.get_int("writes_waiting") == Some(2))
            .then_some(ping)
    });
    assert_eq!(held.get_int("ordinary"), Some(0), "{held:?}");
    let running: Vec<String> = (0..4).map(|_| gone.recv().unwrap()).collect();
    assert!(gone.try_recv().is_err(), "a waiting write got go");
    // short calls keep all eight ordinary threads meanwhile
    let began = Instant::now();
    let reads: Vec<_> = (0..8)
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
    // the running writes send and commit; each frees a thread whose `go`
    // reaches a waiting one, which then sends
    let index = |id: &str| usize::from_str_radix(id, 16).unwrap();
    for id in &running {
        senders[index(id)].send(()).unwrap();
    }
    for _ in 0..2 {
        let id = gone.recv_timeout(WAIT).expect("a waiting write's go");
        senders[index(&id)].send(()).unwrap();
    }
    for writer in writers {
        committed(&writer.join().unwrap());
    }
    assert_eq!(commits(path, "shop/api", "work"), before + 6);
    drain(&home);
}

#[test]
fn a_stalled_input_is_refused_after_its_waiting_bound_despite_keepalives() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    ok(
        path,
        &["file", "write", "shop/api", "work", "/warm"],
        b"w\n",
    );
    let before = commits(path, "shop/api", "work");
    // after `go`, a little input, then only keepalives: they do not renew
    // the worker's 30 s minimum-progress bound
    let exchange = home.dial_writes();
    let id = "000000000000000000000000057a11ed";
    let mut socket: &UnixStream = exchange.socket();
    socket
        .write_all(&content_request(&exchange, id, "/stalled"))
        .unwrap();
    read_go(exchange.socket(), id);
    let began = Instant::now();
    socket.write_all(&frame(&patterned(4 << 10), true)).unwrap();
    let keepalive = exchange.socket().try_clone().unwrap();
    std::thread::spawn(move || {
        let mut socket = &keepalive;
        while socket.write_all(&frame(br#"{"wait":true}"#, false)).is_ok() {
            std::thread::sleep(Duration::from_secs(10));
        }
    });
    let answer = raw_answer(exchange.socket());
    let waited = began.elapsed();
    let error = answer.get("error").unwrap_or_else(|| panic!("{answer:?}"));
    assert_eq!(error.get_text("code"), Some("Unavailable"), "{answer:?}");
    let message = error.get_text("message").unwrap();
    assert!(
        message.contains("less than") && message.contains("nothing was committed"),
        "{answer:?}"
    );
    assert!(
        waited >= Duration::from_secs(29) && waited < Duration::from_secs(50),
        "{waited:?}"
    );
    drop(exchange);
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

#[test]
fn drain_and_machine_stop_let_an_admitted_write_finish() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    ok(
        path,
        &["file", "write", "shop/api", "work", "/warm"],
        b"w\n",
    );
    let before = commits(path, "shop/api", "work");
    let content = patterned(1 << 20);
    // drain: the running write is counted, not stopped, and commits
    let (went, gone) = channel();
    let (send, told) = channel();
    let id = "0000000000000000000000000d7a1700".to_owned();
    let writer = held_write(&home, id, "/kept".to_owned(), content.clone(), went, told);
    gone.recv_timeout(WAIT).expect("go");
    let drained = home.dial(true).request("drain");
    let drained = drained.get("ok").unwrap_or_else(|| panic!("{drained:?}"));
    assert_eq!(drained.get_int("writes_running"), Some(1), "{drained:?}");
    send.send(()).unwrap();
    committed(&writer.join().unwrap());
    let exit = exited(&home);
    assert!(exit < Duration::from_secs(10), "{exit:?}");
    assert_eq!(commits(path, "shop/api", "work"), before + 1);
    assert_eq!(size(path, "work", "/kept"), 1 << 20);

    // machine stop: it does not wait on the write, names it, and the write
    // still commits
    ok(
        path,
        &["file", "write", "shop/api", "work", "/warm-2"],
        b"w\n",
    );
    let (went, gone) = channel();
    let (send, told) = channel();
    let id = "0000000000000000000000000d7a1701".to_owned();
    let writer = held_write(&home, id, "/kept-2".to_owned(), content, went, told);
    gone.recv_timeout(WAIT).expect("go");
    let began = Instant::now();
    let (success, answer) = run(path, &["machine", "stop", host()], b"");
    assert!(
        began.elapsed() < Duration::from_secs(10),
        "{:?}",
        began.elapsed()
    );
    assert!(
        success || answer.get_text("error") == Some("InUse"),
        "{answer:?}"
    );
    let printed = json::print(&answer);
    assert!(
        printed.contains("running_writes") && printed.contains("1 write still running"),
        "{answer:?}"
    );
    send.send(()).unwrap();
    committed(&writer.join().unwrap());
    let exit = exited(&home);
    assert!(exit < Duration::from_secs(10), "{exit:?}");
    ok(path, &["machine", "start", host()], b"");
    assert_eq!(commits(path, "shop/api", "work"), before + 3);
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
    // patterned, so neither compression nor deduplication makes it fit; a
    // file off the small volume, so it streams to the worker unspooled
    let input = small.mount.with_extension("input");
    std::fs::write(&input, patterned(192 << 20)).unwrap();
    let (success, answer) = run(
        path,
        &[
            "file",
            "write",
            "shop/api",
            "work",
            "/big",
            "--from",
            input.to_str().unwrap(),
        ],
        b"",
    );
    let _ = std::fs::remove_file(&input);
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
