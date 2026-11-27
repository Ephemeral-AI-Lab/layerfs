//! S2.2 writes under load and slow producers (§11, review rounds 5 and 6):
//! calls wait. A piped `file write` is spooled before the CLI dials, so a
//! producer slower than the worker's progress window still commits once;
//! twenty writes at once all commit, or past the worker's accept limit an
//! extra one is refused by its hello bound before it sends anything.
//! Streamed writes have their own endpoint: with every write place held,
//! an ordinary read still answers at once, a further writer meets only the
//! hello bound naming the load, a single-change write runs on an ordinary
//! thread, and a held writer that is killed is dropped and its place given
//! to the next.
#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;
#[path = "common/host_writes.rs"]
mod writes;

use layerfs_api::worker::{read_host_frame, HOST_HELD_WRITES, HOST_MAX_REPLY, MAX_HELLO};
use layerfs_protocol::sdk::SdkValue;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::process::Stdio;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};
use writes::*;

/// How long a parked write waits for its `go`: past the 30 s hello bound
/// the test waits out while every place is held.
const PARKED: Duration = Duration::from_secs(95);

/// A raw streamed write on `exchange` (greeted on the write endpoint),
/// held: it sends its request, says `index` on `went` when the worker asks
/// for its input, then sends 64 KiB every 5 s (more than the worker's
/// minimum progress, so a running write keeps its thread) until told to
/// end it; its answer.
fn parked(
    exchange: fixture::Exchange,
    index: usize,
    went: Sender<usize>,
    told: Receiver<()>,
) -> std::thread::JoinHandle<SdkValue> {
    std::thread::spawn(move || {
        let id = format!("{index:032x}");
        let mut socket: &UnixStream = exchange.socket();
        let target = format!("/p-{index:02}");
        socket
            .write_all(&content_request(&exchange, &id, &target))
            .unwrap();
        let go = read_host_frame(socket, HOST_MAX_REPLY, Some(Instant::now() + PARKED))
            .unwrap()
            .expect("go");
        assert!(go.get_bool("go"), "{go:?}");
        went.send(index).unwrap();
        let mut sent = 0;
        while let Err(RecvTimeoutError::Timeout) = told.recv_timeout(Duration::from_secs(5)) {
            socket
                .write_all(&frame(&patterned(64 << 10), true))
                .unwrap();
            sent += 64 << 10;
        }
        let end = format!(r#"{{"end":{{"changes":0,"bytes":{sent}}}}}"#);
        socket.write_all(&frame(&[], true)).unwrap();
        socket.write_all(&frame(end.as_bytes(), false)).unwrap();
        raw_answer(socket)
    })
}

/// A CLI `file write` of a regular file to `target`, unwaited.
fn cli_write(path: &std::path::Path, target: &str) -> std::process::Child {
    let input = path.join(format!("{}.in", target.trim_start_matches('/')));
    std::fs::write(&input, b"cli\n").unwrap();
    cli(path, &["file", "write", "shop/api", "work", target])
        .stdin(std::fs::File::open(&input).unwrap())
        .spawn()
        .unwrap()
}

#[test]
fn with_every_write_place_held_reads_answer_and_a_killed_writer_frees_its_place() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    ok(
        path,
        &["file", "write", "shop/api", "work", "/warm"],
        b"w\n",
    );
    let before = commits(path, "shop/api", "work");
    let (went, gone) = channel();
    let mut senders = Vec::new();
    let mut writers = Vec::new();
    let mut park = |index: usize| {
        let (send, told) = channel();
        senders.push(send);
        writers.push(parked(home.dial_writes(), index, went.clone(), told));
    };
    let held = |running, waiting| {
        wait_for("the held writes", || {
            let ping = home.ping();
            (ping.get_int("writes") == Some(running)
                && ping.get_int("writes_waiting") == Some(waiting))
            .then_some(ping)
        })
    };
    // four running on the write threads, then a CLI writer waiting
    for index in 0..4 {
        park(index);
    }
    held(4, 0);
    let mut running: Vec<usize> = (0..4).map(|_| gone.recv_timeout(WAIT).unwrap()).collect();
    running.sort();
    assert_eq!(running, [0, 1, 2, 3]);
    let mut killed = cli_write(path, "/killed");
    held(4, 1);
    // the rest of the places, all held at `go`: 33 writers with the next
    let rest = HOST_HELD_WRITES - 5;
    for index in 4..4 + rest {
        park(index);
    }
    let full = held(4, rest as i64 + 1);
    assert_eq!(full.get_int("ordinary"), Some(0), "{full:?}");
    // an ordinary read answers at once
    let began = Instant::now();
    let read = finish(
        cli(path, &["file", "read", "shop/api", "work", "/warm"])
            .spawn()
            .unwrap(),
    );
    assert!(read.status.success(), "{read:?}");
    let took = began.elapsed();
    eprintln!("RECEIPT read with {HOST_HELD_WRITES} writes held: {took:?}");
    assert!(took < Duration::from_secs(5), "{took:?}");
    // so does a single-change write: it runs on an ordinary thread
    let routed_before = routed(path);
    let began = Instant::now();
    ok(path, &["file", "mkdir", "shop/api", "work", "/made"], b"");
    let took = began.elapsed();
    eprintln!("RECEIPT mkdir with {HOST_HELD_WRITES} writes held: {took:?}");
    assert!(took < Duration::from_secs(5), "{took:?}");
    assert_eq!(
        routed(path),
        routed_before + 1,
        "it went through the worker"
    );
    // a further writer waits in the write endpoint's listen queue and gets
    // only the hello bound's refusal naming the load, nothing sent
    let began = Instant::now();
    let mut refused = cli_write(path, "/refused");
    let end = began + Duration::from_secs(45);
    while refused.try_wait().unwrap().is_none() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = refused.kill();
    let refused = refused.wait_with_output().unwrap();
    let waited = began.elapsed();
    let text = String::from_utf8_lossy(&refused.stderr);
    assert!(!refused.status.success(), "{refused:?}");
    assert!(
        text.contains("Unavailable")
            && text.contains("no write place was free")
            && !text.contains("Unknown"),
        "{text}"
    );
    assert!(waited >= Duration::from_secs(29), "{waited:?}");
    // a writer queued now gets its hello only once the killed writer's
    // place is freed
    let queued = UnixStream::connect(&home.paths.writes).unwrap();
    let (hello, greeted) = channel();
    let waiting = std::thread::spawn({
        let queued = queued.try_clone().unwrap();
        move || {
            let ready = read_host_frame(&queued, MAX_HELLO, Some(Instant::now() + PARKED));
            hello.send(Instant::now()).unwrap();
            ready
        }
    });
    std::thread::sleep(Duration::from_secs(1));
    assert!(
        greeted.try_recv().is_err(),
        "greeted while every place was held"
    );
    killed.kill().unwrap();
    let killed_at = Instant::now();
    killed.wait().unwrap();
    let freed = greeted
        .recv_timeout(Duration::from_secs(20))
        .expect("hello");
    let purged = freed.saturating_duration_since(killed_at);
    eprintln!("RECEIPT place freed after a held writer was killed: {purged:?}");
    assert!(purged < Duration::from_secs(12), "{purged:?}");
    waiting.join().unwrap().unwrap().expect("hello frame");
    drop(queued);
    // the next writer takes the freed place and is held with the rest
    let last = 4 + rest;
    park(last);
    held(4, rest as i64 + 1);
    for sender in &senders[..4] {
        sender.send(()).unwrap();
    }
    for _ in 0..=rest {
        let index = gone
            .recv_timeout(Duration::from_secs(20))
            .unwrap_or_else(|_| panic!("a waiting write's go: {:?}", home.ping()));
        senders[index].send(()).unwrap();
    }
    for writer in writers {
        let answer = writer.join().unwrap();
        let ok = answer.get("ok").unwrap_or_else(|| panic!("{answer:?}"));
        assert_eq!(ok.get_text("kind"), Some("Committed"), "{answer:?}");
    }
    assert_eq!(commits(path, "shop/api", "work"), before + last + 2);
    let (found, _) = run(path, &["file", "stat", "shop/api", "work", "/killed"], b"");
    assert!(!found, "the killed writer's write ran");
    assert!(retained(path).is_empty());
    drain(&home);
}

#[test]
fn a_piped_write_whose_producer_sleeps_past_the_progress_window_commits_once() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    ok(
        path,
        &["file", "write", "shop/api", "work", "/warm"],
        b"w\n",
    );
    let before = commits(path, "shop/api", "work");
    let routed_before = routed(path);
    let began = Instant::now();
    let mut child = cli(path, &["file", "write", "shop/api", "work", "/late"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    // longer than the worker's 30 s minimum-progress window
    let producer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(35));
        input.write_all(b"late\n").unwrap();
    });
    let end = began + Duration::from_secs(80);
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < end, "the piped write did not finish");
        std::thread::sleep(Duration::from_millis(50));
    }
    producer.join().unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(began.elapsed() >= Duration::from_secs(35));
    assert_eq!(
        routed(path),
        routed_before + 1,
        "it went through the worker"
    );
    assert_eq!(commits(path, "shop/api", "work"), before + 1);
    assert_eq!(read(path, "work", "/late"), "late\n");
    drain(&home);
}

#[test]
fn twenty_writes_at_once_all_commit_or_are_refused_before_sending() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    ok(
        path,
        &["file", "write", "shop/api", "work", "/warm"],
        b"w\n",
    );
    let before = commits(path, "shop/api", "work");
    let writers: Vec<_> = (0..20)
        .map(|index| {
            let target = format!("/c-{index:02}");
            let mut child = cli(path, &["file", "write", "shop/api", "work", &target])
                .stdin(Stdio::piped())
                .spawn()
                .unwrap();
            let mut input = child.stdin.take().unwrap();
            input.write_all(&patterned(256 << 10)).unwrap();
            drop(input);
            child
        })
        .collect();
    let mut committed = 0;
    for writer in writers {
        let output = finish(writer);
        if output.status.success() {
            committed += 1;
            continue;
        }
        // only the pre-send refusal of a full worker: nothing was sent
        let text = String::from_utf8_lossy(&output.stderr);
        let answer = layerfs_home::json::parse(text.trim()).unwrap();
        assert_eq!(answer.get_text("error"), Some("Unavailable"), "{answer:?}");
        assert!(
            !text.contains("request ") && !text.contains("Unknown"),
            "{answer:?}"
        );
    }
    eprintln!("RECEIPT twenty concurrent writes: {committed} committed");
    assert_eq!(commits(path, "shop/api", "work"), before + committed);
    for index in 0..20 {
        let target = format!("/c-{index:02}");
        let (found, _) = run(path, &["file", "stat", "shop/api", "work", &target], b"");
        assert!(found || committed < 20, "{target} is missing");
    }
    assert!(retained(path).is_empty());
    drain(&home);
}
