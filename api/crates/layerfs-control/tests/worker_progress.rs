//! The operation-progress wait (#550 B9, protocol 3): the worker claims a
//! request and reports progress from the operation path; a caller refreshes
//! its wait on that progress, not on a liveness beat, so an operation that
//! never returns is `Unknown` in bounded time and never replayed.
//!
//! A stand-in worker lets the tests run the caller's clock without a Sandbox:
//! it answers the secret and the request on one connection, then sends the
//! claim, progress, output or beats the test names.
use layerfs_bridge::control::SdkValue;
use layerfs_control::machine::worker::{
    self, read_frame, read_frame_within, write_frame, Endpoint, Hello, Windows, MAX_HELLO, PROTOCOL,
};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The bundle this build deploys in these tests: one folder whose
/// `bundle.json` names [`DEPLOYED`], set once for the whole binary.
const DEPLOYED: &str = "abcd0123";

fn deployed_bundle() {
    static SET: OnceLock<bool> = OnceLock::new();
    SET.get_or_init(|| {
        let dir =
            std::env::temp_dir().join(format!("layerfs-progress-bundle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("bundle.json"),
            format!("{{\"hash\":\"{DEPLOYED}\"}}"),
        )
        .unwrap();
        std::env::set_var("LAYERFS_BUNDLE", &dir);
        true
    });
}

/// A stand-in worker of this protocol and bundle that accepts every
/// connection, answers the secret and reads the request, then runs `handler`
/// on the one stream. It returns the endpoint and how many connections it
/// accepted: a call that replays its request opens a second one.
fn stand_in<F>(handler: F) -> (Endpoint, Arc<AtomicUsize>)
where
    F: Fn(&mut TcpStream) + Send + Sync + 'static,
{
    deployed_bundle();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let connections = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&connections);
    let handler = Arc::new(handler);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            count.fetch_add(1, Ordering::SeqCst);
            let handler = Arc::clone(&handler);
            std::thread::spawn(move || {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
                let Ok(Some(secret)) = read_frame_within(&mut stream, MAX_HELLO) else {
                    return;
                };
                if secret.get_text("secret").is_none() {
                    return;
                }
                let hello = Hello {
                    protocol: Some(PROTOCOL),
                    bundle: Some(DEPLOYED.to_owned()),
                };
                if write_frame(&mut stream, &hello.to_frame()).is_err() {
                    return;
                }
                let Ok(Some(_request)) = read_frame(&mut stream) else {
                    return;
                };
                handler(&mut stream);
            });
        }
    });
    (
        Endpoint {
            address,
            secret: "k".to_owned(),
        },
        connections,
    )
}

/// The operation was read and will run.
fn claimed(stream: &mut TcpStream) {
    let _ = write_frame(stream, &SdkValue::map([("claimed", SdkValue::Bool(true))]));
}

/// One operation-progress frame: `{ progress: { seq, phase } }`.
fn progress(stream: &mut TcpStream, seq: u64) {
    let _ = write_frame(
        stream,
        &SdkValue::map([(
            "progress",
            SdkValue::map([
                ("seq", SdkValue::Int(seq as i64)),
                ("phase", SdkValue::text("test")),
            ]),
        )]),
    );
}

/// A liveness beat: the worker is alive, not that the operation advanced.
fn beat(stream: &mut TcpStream) {
    let _ = write_frame(stream, &SdkValue::map([("beat", SdkValue::Bool(true))]));
}

fn answer(stream: &mut TcpStream, text: &str) {
    let _ = write_frame(stream, &SdkValue::map([("value", SdkValue::text(text))]));
}

fn windows(claim: u64, progress: u64) -> Windows {
    Windows {
        claim: Duration::from_millis(claim),
        progress: Duration::from_millis(progress),
    }
}

#[test]
fn a_wedged_operation_after_claim_is_unknown_in_bounded_time() {
    let (endpoint, connections) = stand_in(|stream| {
        claimed(stream);
        // the operation never returns and never reports progress
        std::thread::sleep(Duration::from_secs(2));
        answer(stream, "late");
    });
    let started = Instant::now();
    let refusal = worker::call_with(
        &endpoint,
        "file.read",
        SdkValue::Null,
        &mut |_| {},
        windows(500, 300),
    )
    .unwrap_err();
    let waited = started.elapsed();
    assert_eq!(refusal.code, "Unknown", "{refusal:?}");
    assert!(refusal.message.contains("no progress"), "{refusal:?}");
    assert!(refusal.detail.get_bool("claimed"), "{refusal:?}");
    assert!(waited >= Duration::from_millis(300), "{waited:?}");
    assert!(waited < Duration::from_millis(1500), "{waited:?}");
    // nothing was resent: the one connection is the one request
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(connections.load(Ordering::SeqCst), 1);
}

#[test]
fn a_worker_that_never_claims_is_unknown_in_bounded_time() {
    let (endpoint, _) = stand_in(|stream| {
        // the request was read, but the operation never starts
        std::thread::sleep(Duration::from_secs(1));
        answer(stream, "late");
    });
    let started = Instant::now();
    let refusal = worker::call_with(
        &endpoint,
        "file.read",
        SdkValue::Null,
        &mut |_| {},
        windows(300, 300),
    )
    .unwrap_err();
    let waited = started.elapsed();
    assert_eq!(refusal.code, "Unknown", "{refusal:?}");
    assert!(refusal.message.contains("did not claim"), "{refusal:?}");
    assert!(!refusal.detail.get_bool("claimed"), "{refusal:?}");
    assert!(waited >= Duration::from_millis(300), "{waited:?}");
    assert!(waited < Duration::from_secs(1), "{waited:?}");
}

#[test]
fn progress_refreshes_the_wait() {
    let (endpoint, _) = stand_in(|stream| {
        claimed(stream);
        // longer than the progress window, but each frame arrives inside it
        for i in 0..10 {
            std::thread::sleep(Duration::from_millis(100));
            progress(stream, i);
        }
        answer(stream, "done");
    });
    let started = Instant::now();
    let mut seen = Vec::new();
    let outcome = worker::call_with(
        &endpoint,
        "file.read",
        SdkValue::Null,
        &mut |event| seen.push(event),
        windows(500, 300),
    );
    assert_eq!(outcome.unwrap(), SdkValue::text("done"));
    assert!(
        started.elapsed() >= Duration::from_millis(1000),
        "{:?}",
        started.elapsed()
    );
    assert!(
        seen.iter().any(|event| event.get("progress").is_some()),
        "{seen:?}"
    );
}

#[test]
fn a_liveness_beat_does_not_refresh_the_progress_wait() {
    let (endpoint, _) = stand_in(|stream| {
        claimed(stream);
        // the worker keeps beating while its operation never progresses
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(50));
            beat(stream);
        }
        answer(stream, "late");
    });
    let started = Instant::now();
    let refusal = worker::call_with(
        &endpoint,
        "file.read",
        SdkValue::Null,
        &mut |_| {},
        windows(500, 300),
    )
    .unwrap_err();
    let waited = started.elapsed();
    assert_eq!(refusal.code, "Unknown", "{refusal:?}");
    assert!(refusal.message.contains("no progress"), "{refusal:?}");
    assert!(waited < Duration::from_millis(1500), "{waited:?}");
}

#[test]
fn a_wedged_operation_is_not_replayed_and_its_result_is_kept() {
    let late = Arc::new(Mutex::new(None::<String>));
    let recorded = Arc::clone(&late);
    let (endpoint, connections) = stand_in(move |stream| {
        claimed(stream);
        // the caller has already stopped waiting; the operation runs to its
        // end and records its result, and nothing resends the request
        std::thread::sleep(Duration::from_millis(1000));
        *recorded.lock().unwrap() = Some("late".to_owned());
        answer(stream, "late");
    });
    let refusal = worker::call_with(
        &endpoint,
        "file.read",
        SdkValue::Null,
        &mut |_| {},
        windows(500, 300),
    )
    .unwrap_err();
    assert_eq!(refusal.code, "Unknown", "{refusal:?}");
    assert!(refusal.detail.get_bool("claimed"));
    // the operation kept running: its eventual result is observable
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(late.lock().unwrap().as_deref(), Some("late"));
    assert_eq!(connections.load(Ordering::SeqCst), 1);
}

#[test]
fn a_long_operation_that_reports_progress_completes() {
    // With the real windows, a 60 s operation that progresses completes:
    // there is no operation-duration deadline.
    let (endpoint, connections) = stand_in(|stream| {
        claimed(stream);
        for i in 0..60 {
            progress(stream, i);
            std::thread::sleep(Duration::from_secs(1));
        }
        answer(stream, "sixty");
    });
    let started = Instant::now();
    let mut seen = Vec::new();
    let outcome = worker::call(&endpoint, "file.read", SdkValue::Null, &mut |event| {
        seen.push(event)
    });
    let waited = started.elapsed();
    assert_eq!(outcome.unwrap(), SdkValue::text("sixty"));
    assert!(waited >= Duration::from_secs(60), "{waited:?}");
    assert!(seen.iter().any(|event| event.get("progress").is_some()));
    assert_eq!(connections.load(Ordering::SeqCst), 1);
}

#[test]
fn the_worker_claims_and_reports_progress_before_its_answer() {
    // The real worker's operation path: the claim and the start/done
    // progress frames surround the operation's value, in order.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let root = std::env::temp_dir().join(format!("layerfs-progress-root-{}", std::process::id()));
    let served = root.clone();
    std::thread::spawn(move || {
        let _ = worker::serve(listener, "k".to_owned(), served);
    });
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write_frame(
        &mut stream,
        &SdkValue::map([("secret", SdkValue::text("k"))]),
    )
    .unwrap();
    let hello = read_frame_within(&mut stream, MAX_HELLO).unwrap().unwrap();
    assert_eq!(Hello::of(&hello).protocol, Some(PROTOCOL));
    write_frame(
        &mut stream,
        &SdkValue::map([
            ("op", SdkValue::text("worker.ping")),
            ("args", SdkValue::Null),
        ]),
    )
    .unwrap();
    let mut frames = Vec::new();
    loop {
        let frame = read_frame(&mut stream).unwrap().unwrap();
        let finished = frame.get("value").is_some() || frame.get("refused").is_some();
        frames.push(frame);
        if finished {
            break;
        }
    }
    assert!(frames[0].get("claimed").is_some(), "{frames:?}");
    let phases: Vec<&str> = frames
        .iter()
        .filter_map(|frame| frame.get("progress"))
        .filter_map(|progress| progress.get_text("phase"))
        .collect();
    assert!(phases.contains(&"start"), "{phases:?}");
    assert!(phases.contains(&"done"), "{phases:?}");
    assert!(frames.last().unwrap().get("value").is_some(), "{frames:?}");
    let seqs: Vec<i64> = frames
        .iter()
        .filter_map(|frame| frame.get("progress"))
        .filter_map(|progress| progress.get_int("seq"))
        .collect();
    assert!(seqs.windows(2).all(|pair| pair[0] < pair[1]), "{seqs:?}");
    let _ = std::fs::remove_dir_all(root);
}
