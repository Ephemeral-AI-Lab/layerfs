//! The caller's side: one request per connection, sent once.
use super::frame::{
    frame_bytes, is_liveness_timeout, read_frame_within_wait, write_frame, MAX_FRAME, MAX_HELLO,
};
use super::hello::Hello;
use super::protocol::{CLAIM_WAIT, CONNECT, HELLO_WAIT, PING_WAIT, PROGRESS_WAIT, SEND_WAIT};
use crate::common::outcome::{refused, Outcome};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

/// How the caller reaches a machine's worker (from `machine/<name>/worker.json`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// The loopback address on the caller's host.
    pub address: SocketAddr,
    /// The worker's secret.
    pub secret: String,
}

/// What one call waits for: the claim after its request, and then each sign
/// of operation progress. [`Default`] is [`CLAIM_WAIT`] and
/// [`PROGRESS_WAIT`]; tests use smaller windows to keep their clocks short.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Windows {
    /// How long a sent request may go unclaimed.
    pub claim: Duration,
    /// How long a claimed operation may go without a progress or output
    /// frame.
    pub progress: Duration,
}

impl Default for Windows {
    fn default() -> Self {
        Self {
            claim: CLAIM_WAIT,
            progress: PROGRESS_WAIT,
        }
    }
}

/// Runs one host operation `op` in the worker. Events the worker sends
/// (operation progress, command output) go to `events`. A request too large
/// is `TooLarge` and a worker that cannot be reached or does not answer its
/// secret in [`HELLO_WAIT`] is `Unavailable`, and so is one of another
/// build ([`super::Hello::mismatch`]); a wrong secret is the worker's
/// refusal (`Denied`). In each, the request was not sent. Once the request
/// was sent the outcome is never resent: a connection lost is `Unknown`, and
/// so is a worker that does not claim the request in [`CLAIM_WAIT`] or
/// claims it and then reports no progress for [`PROGRESS_WAIT`]. An
/// operation that is `Unknown` this way may still run to its end; its
/// effects stay observable.
pub fn call(
    endpoint: &Endpoint,
    op: &str,
    args: SdkValue,
    events: &mut dyn FnMut(SdkValue),
) -> Outcome {
    call_with(endpoint, op, args, events, Windows::default())
}

/// [`call`] with explicit [`Windows`]: the same call, waiting the given
/// time for the claim and then for each sign of operation progress.
pub fn call_with(
    endpoint: &Endpoint,
    op: &str,
    args: SdkValue,
    events: &mut dyn FnMut(SdkValue),
    windows: Windows,
) -> Outcome {
    let request = request(op, args)?;
    send_with(endpoint, op, &request, events, windows)
        .unwrap_or_else(|Unreached(refusal)| Err(refusal))
}

/// A worker that was not reached: the connection, the secret or the
/// worker's answer to it failed, and the request was not sent. Only this
/// may start a machine before the request is sent once (#535 R1).
#[derive(Debug)]
pub struct Unreached(pub SdkRefusal);

/// Request `{ op, args }` framed once, with the caller's home id; `TooLarge`
/// past [`super::MAX_FRAME`].
pub fn request(op: &str, args: SdkValue) -> Result<Vec<u8>, SdkRefusal> {
    let mut request = SdkValue::map([("op", SdkValue::text(op)), ("args", args)]);
    // the caller's home id: the worker's root has none of its own (O33)
    if let Some(id) = crate::common::home::guest::current_id() {
        request = crate::common::home::with_field(request, "home_id", SdkValue::Text(id));
    }
    frame_bytes(&request)
}

/// Sends the framed `request` (for operation `op`) on one new connection:
/// the secret first, alone, then the request once. [`Unreached`] when
/// nothing was sent; otherwise the operation's outcome, `Unknown` when the
/// answer was lost after the request was sent. `answer_within` bounds the
/// whole answer when some (a ping); when none, the operation may run as long
/// as it keeps reporting progress ([`CLAIM_WAIT`], then [`PROGRESS_WAIT`]).
/// A frame is bounded as a whole (its length, each read and its body at a
/// minimum rate), so a peer that trickles one cannot stretch the wait.
pub fn send(
    endpoint: &Endpoint,
    op: &str,
    request: &[u8],
    events: &mut dyn FnMut(SdkValue),
    answer_within: Option<Duration>,
) -> Result<Outcome, Unreached> {
    let started = Instant::now();
    let sent = send_once(
        endpoint,
        op,
        request,
        events,
        answer_within,
        Windows::default(),
    );
    let worker = endpoint.address.to_string();
    crate::common::trace::connection(&worker, op, sent.is_ok(), started);
    sent
}

/// [`send`] with explicit [`Windows`] for an answer that is not bounded as
/// a whole: a test can shorten the claim and progress waits.
pub fn send_with(
    endpoint: &Endpoint,
    op: &str,
    request: &[u8],
    events: &mut dyn FnMut(SdkValue),
    windows: Windows,
) -> Result<Outcome, Unreached> {
    let started = Instant::now();
    let sent = send_once(endpoint, op, request, events, None, windows);
    let worker = endpoint.address.to_string();
    crate::common::trace::connection(&worker, op, sent.is_ok(), started);
    sent
}

fn send_once(
    endpoint: &Endpoint,
    op: &str,
    request: &[u8],
    events: &mut dyn FnMut(SdkValue),
    answer_within: Option<Duration>,
    windows: Windows,
) -> Result<Outcome, Unreached> {
    let unavailable = |why: &dyn std::fmt::Display| {
        Unreached(refused(
            "Unavailable",
            format!(
                "the worker at {} does not answer: {why}; {op} was not sent",
                endpoint.address
            ),
        ))
    };
    let (stream, hello) = greet(endpoint).map_err(|refusal| match refusal {
        Greeting::Refused(refusal) => Unreached(refusal),
        Greeting::Unanswered(why) => unavailable(&why),
    })?;
    // a worker of another build is sent nothing
    if let Some(refusal) = hello.mismatch(&endpoint.address.to_string()) {
        return Err(Unreached(refusal));
    }
    stream
        .set_write_timeout(Some(SEND_WAIT))
        .map_err(|error| unavailable(&error))?;
    // from here the request may have reached the worker: nothing is resent
    Ok(answer(
        stream,
        endpoint,
        op,
        request,
        events,
        answer_within,
        windows,
    ))
}

/// Why a worker did not answer its secret.
enum Greeting {
    /// It refused (a wrong secret: `Denied`) or answered no known frame.
    Refused(SdkRefusal),
    /// It could not be reached or did not answer in [`HELLO_WAIT`].
    Unanswered(String),
}

/// One new connection to the worker, its secret sent alone and answered:
/// the open stream and what the worker said of itself. Nothing else is sent.
fn greet(endpoint: &Endpoint) -> Result<(TcpStream, Hello), Greeting> {
    let unanswered = |why: &dyn std::fmt::Display| Greeting::Unanswered(why.to_string());
    let mut stream = TcpStream::connect_timeout(&endpoint.address, CONNECT)
        .map_err(|error| unanswered(&error))?;
    let _ = stream.set_nodelay(true);
    stream
        .set_write_timeout(Some(SEND_WAIT))
        .map_err(|error| unanswered(&error))?;
    // the secret first, alone: the worker answers it before the request
    let secret = SdkValue::map([("secret", SdkValue::text(&endpoint.secret))]);
    write_frame(&mut stream, &secret).map_err(|error| unanswered(&error))?;
    match read_frame_within_wait(&mut stream, MAX_HELLO, HELLO_WAIT) {
        Ok(Some(frame)) if frame.get("ready").is_some() => {
            let hello = Hello::of(&frame);
            Ok((stream, hello))
        }
        Ok(Some(frame)) => Err(Greeting::Refused(match frame.get("refused") {
            Some(refusal) => refusal_of(refusal),
            None => refused(
                "Failed",
                "the worker answered its secret with no known frame",
            ),
        })),
        Ok(None) => Err(unanswered(&"it closed the connection")),
        Err(refusal) => Err(unanswered(&refusal.message)),
    }
}

/// What the worker at `endpoint` says of itself when it answers its secret
/// (its protocol and bundle), whatever build it is; the connection then
/// closes with no request. `Unavailable` when it does not answer, its
/// refusal (`Denied`) when it refuses the secret.
pub fn hello(endpoint: &Endpoint) -> Result<Hello, SdkRefusal> {
    let started = Instant::now();
    let greeted = greet(endpoint);
    crate::common::trace::connection(&endpoint.address.to_string(), "hello", false, started);
    match greeted {
        Ok((_, hello)) => Ok(hello),
        Err(Greeting::Refused(refusal)) => Err(refusal),
        Err(Greeting::Unanswered(why)) => Err(refused(
            "Unavailable",
            format!("the worker at {} does not answer: {why}", endpoint.address),
        )),
    }
}

/// Writes the request and reads the frames: the claim, the operation's
/// progress and output, the liveness beats, then the value or the refusal.
///
/// With `answer_within` set, the whole answer is waited for that long (a
/// ping). Without it, after the request is sent the worker has
/// [`CLAIM_WAIT`] to claim it and then each sign of operation progress has
/// [`PROGRESS_WAIT`]: a `progress` or `event` frame refreshes that wait, a
/// liveness `beat` does not. A command's live child is such a `progress`
/// frame (`phase: "running"`), so a quiet `sleep` or build keeps the wait
/// while its process lives; an operation whose child is gone reports
/// nothing more and is `Unknown` within [`PROGRESS_WAIT`]. A wait that runs
/// out is `Unknown`, because the request was sent and may have run; it is
/// never resent. Each frame is
/// bounded as a whole: its length and every read within the wait, and its
/// body within the wait or its length at a minimum rate, so a peer that
/// trickles a frame cannot stretch a wait. A wait that expires writes one
/// `layerfs worker: liveness timeout:` line to stderr.
fn answer(
    mut stream: TcpStream,
    endpoint: &Endpoint,
    op: &str,
    request: &[u8],
    events: &mut dyn FnMut(SdkValue),
    answer_within: Option<Duration>,
    windows: Windows,
) -> Outcome {
    stream
        .write_all(request)
        .and_then(|()| stream.flush())
        .map_err(|error| {
            refused(
                "Unknown",
                format!("sending {op} to the worker failed part way ({error}); it was not resent"),
            )
        })?;
    // the claim ends the first wait; progress or output then refreshes the
    // next one, and a beat never does
    let started = Instant::now();
    let mut claimed = false;
    let mut signals = 0u64;
    let mut last = started;
    loop {
        let wait = match answer_within {
            Some(total) => total
                .checked_sub(started.elapsed())
                .filter(|left| !left.is_zero()),
            None if !claimed => windows
                .claim
                .checked_sub(started.elapsed())
                .filter(|left| !left.is_zero()),
            None => windows
                .progress
                .checked_sub(last.elapsed())
                .filter(|left| !left.is_zero()),
        };
        let Some(wait) = wait else {
            if answer_within.is_some() {
                return Err(lost(op, "the answer did not arrive within its wait"));
            }
            return Err(if claimed {
                no_progress(op, signals, windows.progress, started)
            } else {
                no_claim(op, windows.claim, started)
            });
        };
        let stage = if answer_within.is_some() {
            "answer"
        } else if claimed {
            "progress"
        } else {
            "claim"
        };
        let frame = match read_frame_within_wait(&mut stream, MAX_FRAME, wait) {
            Ok(Some(frame)) => frame,
            Ok(None) => return Err(lost(op, "the worker closed the connection")),
            Err(refusal) => {
                let expired = is_liveness_timeout(&refusal);
                if expired {
                    liveness_timeout(endpoint, op, stage, wait);
                }
                // a liveness wait that expires on an operation is the
                // operation not advancing, not just a dead connection
                return Err(if expired && answer_within.is_none() {
                    if claimed {
                        no_progress(op, signals, windows.progress, started)
                    } else {
                        no_claim(op, windows.claim, started)
                    }
                } else {
                    lost(op, &refusal.message)
                });
            }
        };
        if frame.get("claimed").is_some() {
            claimed = true;
            last = Instant::now();
            continue;
        }
        if let Some(value) = frame.get("progress") {
            signals += 1;
            last = Instant::now();
            // the worker's own start/done frame bounds the wait; only the
            // operation's own progress is a caller event, as it is locally
            if value.get_bool("worker") {
                continue;
            }
            events(SdkValue::map([("progress", value.clone())]));
            continue;
        }
        if frame.get("beat").is_some() {
            // a liveness beat says the worker is alive, not that its
            // operation is progressing: it does not refresh the wait
            continue;
        }
        if let Some(event) = frame.get("event") {
            signals += 1;
            last = Instant::now();
            events(event.clone());
            continue;
        }
        if let Some(value) = frame.get("value") {
            return Ok(value.clone());
        }
        if let Some(refusal) = frame.get("refused") {
            return Err(refusal_of(refusal));
        }
        return Err(refused(
            "Failed",
            "the worker sent a frame of no known kind",
        ));
    }
}

fn refusal_of(refusal: &SdkValue) -> SdkRefusal {
    SdkRefusal {
        code: refusal.get_text("code").unwrap_or("Failed").to_owned(),
        message: refusal.get_text("message").unwrap_or("").to_owned(),
        detail: refusal.get("detail").cloned().unwrap_or_default(),
    }
}

/// One stable line on stderr when a call loses its worker to a wait that
/// expired. It names the operation, the worker and the wait it was in, so an
/// operator can tell a dead worker from a refused call without a metrics
/// framework. A lost connection (closed or reset) is not a liveness timeout
/// and logs nothing.
fn liveness_timeout(endpoint: &Endpoint, op: &str, stage: &str, wait: Duration) {
    eprintln!(
        "layerfs worker: liveness timeout: {op} at {}: no {stage} within {} s; the answer is Unknown and was not resent",
        endpoint.address,
        wait.as_secs()
    );
}

fn lost(op: &str, why: &str) -> SdkRefusal {
    refused(
        "Unknown",
        format!("{op}: the worker's answer was lost ({why}); it may have run, and was not resent"),
    )
}

/// The request was sent but the worker never claimed it.
fn no_claim(op: &str, wait: Duration, started: Instant) -> SdkRefusal {
    let mut refusal = refused(
        "Unknown",
        format!(
            "{op}: the worker did not claim the request within {}s; it was sent and may have run, and was not resent",
            wait.as_secs_f64()
        ),
    );
    refusal.detail = SdkValue::map([
        ("op", SdkValue::text(op)),
        ("claimed", SdkValue::Bool(false)),
        ("progress", SdkValue::Int(0)),
        ("waited_ms", SdkValue::Int(ms(started.elapsed()))),
        ("window_ms", SdkValue::Int(ms(wait))),
    ]);
    refusal
}

/// The worker claimed the operation but stopped reporting progress: the
/// operation may be wedged, so the caller stops waiting. Its eventual result
/// is not replayed; if it does finish, its effects stay observable.
fn no_progress(op: &str, signals: u64, wait: Duration, started: Instant) -> SdkRefusal {
    let mut refusal = refused(
        "Unknown",
        format!(
            "{op}: the worker claimed the request but reported no progress for {}s ({signals} progress frames); it may still run, and was not resent",
            wait.as_secs_f64()
        ),
    );
    refusal.detail = SdkValue::map([
        ("op", SdkValue::text(op)),
        ("claimed", SdkValue::Bool(true)),
        (
            "progress",
            SdkValue::Int(i64::try_from(signals).unwrap_or(i64::MAX)),
        ),
        ("waited_ms", SdkValue::Int(ms(started.elapsed()))),
        ("window_ms", SdkValue::Int(ms(wait))),
    ]);
    refusal
}

fn ms(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

/// Whether the worker answers its secret: `{ kind: "worker", version, pid }`.
/// Every frame of its answer is waited for at most [`PING_WAIT`].
pub fn ping(endpoint: &Endpoint) -> Outcome {
    let request = request("worker.ping", SdkValue::Null)?;
    send(
        endpoint,
        "worker.ping",
        &request,
        &mut |_| {},
        Some(PING_WAIT),
    )
    .unwrap_or_else(|Unreached(refusal)| Err(refusal))
}
