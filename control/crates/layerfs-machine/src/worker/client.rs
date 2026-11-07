//! The caller's side: one request per connection, sent once.
use super::frame::{frame_bytes, read_frame, read_frame_within, write_frame, MAX_HELLO};
use super::hello::Hello;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{refused, Outcome};
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

/// How the caller reaches a machine's worker (from `machine/<name>/worker.json`).
#[derive(Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// The loopback address on the caller's host.
    pub address: SocketAddr,
    /// The worker's secret.
    pub secret: String,
}

/// The secret is never shown.
impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint")
            .field("address", &self.address)
            .field("secret", &"<redacted>")
            .finish()
    }
}

const CONNECT: Duration = Duration::from_secs(10);

/// How long the caller waits for the worker to answer its secret.
pub const HELLO_WAIT: Duration = Duration::from_secs(10);

/// How long a ping waits for its answer. Other operations wait for theirs
/// as long as they run (a command, a move): the worker has answered the
/// secret by then, so it is serving.
pub const PING_WAIT: Duration = Duration::from_secs(10);

/// Runs one host operation `op` in the worker. Events the worker sends
/// (progress, command output) go to `events`. A request too large is
/// `TooLarge` and a worker that cannot be reached or does not answer its
/// secret in [`HELLO_WAIT`] is `Unavailable`, and so is one of another
/// build ([`super::Hello::mismatch`]); a wrong secret is the worker's
/// refusal (`Denied`). In each, the request was not sent. A
/// connection lost after the request was sent is `Unknown`: the operation
/// may have run, and nothing is resent.
pub fn call(
    endpoint: &Endpoint,
    op: &str,
    args: SdkValue,
    events: &mut dyn FnMut(SdkValue),
) -> Outcome {
    let request = request(op, args)?;
    send(endpoint, op, &request, events, None).unwrap_or_else(|Unreached(refusal)| Err(refusal))
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
    if let Some(id) = layerfs_home::home::guest::current_id() {
        request = layerfs_home::home::with_field(request, "home_id", SdkValue::Text(id));
    }
    frame_bytes(&request)
}

/// Sends the framed `request` (for operation `op`) on one new connection:
/// the secret first, alone, then the request once. [`Unreached`] when
/// nothing was sent; otherwise the operation's outcome, `Unknown` when the
/// answer was lost after the request was sent. The answer is waited for at
/// most `answer_within`, or as long as the operation runs.
pub fn send(
    endpoint: &Endpoint,
    op: &str,
    request: &[u8],
    events: &mut dyn FnMut(SdkValue),
    answer_within: Option<Duration>,
) -> Result<Outcome, Unreached> {
    let started = Instant::now();
    let sent = send_once(endpoint, op, request, events, answer_within);
    let worker = endpoint.address.to_string();
    layerfs_home::trace::connection(&worker, op, sent.is_ok(), started);
    sent
}

fn send_once(
    endpoint: &Endpoint,
    op: &str,
    request: &[u8],
    events: &mut dyn FnMut(SdkValue),
    answer_within: Option<Duration>,
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
        .set_read_timeout(answer_within)
        .map_err(|error| unavailable(&error))?;
    // from here the request may have reached the worker: nothing is resent
    Ok(answer(stream, op, request, events))
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
    // the secret first, alone: the worker answers it before the request
    stream
        .set_read_timeout(Some(HELLO_WAIT))
        .map_err(|error| unanswered(&error))?;
    let secret = SdkValue::map([("secret", SdkValue::text(&endpoint.secret))]);
    write_frame(&mut stream, &secret).map_err(|error| unanswered(&error))?;
    match read_frame_within(&mut stream, MAX_HELLO) {
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
    layerfs_home::trace::connection(&endpoint.address.to_string(), "hello", false, started);
    match greeted {
        Ok((_, hello)) => Ok(hello),
        Err(Greeting::Refused(refusal)) => Err(refusal),
        Err(Greeting::Unanswered(why)) => Err(refused(
            "Unavailable",
            format!("the worker at {} does not answer: {why}", endpoint.address),
        )),
    }
}

/// Writes the request and reads the events and the answer.
fn answer(
    mut stream: TcpStream,
    op: &str,
    request: &[u8],
    events: &mut dyn FnMut(SdkValue),
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
    loop {
        let frame = match read_frame(&mut stream) {
            Ok(Some(frame)) => frame,
            Ok(None) => return Err(lost(op, "the worker closed the connection")),
            Err(refusal) => return Err(lost(op, &refusal.message)),
        };
        if let Some(event) = frame.get("event") {
            events(event.clone());
        } else if let Some(value) = frame.get("value") {
            return Ok(value.clone());
        } else if let Some(refusal) = frame.get("refused") {
            return Err(refusal_of(refusal));
        } else {
            return Err(refused(
                "Failed",
                "the worker sent a frame of no known kind",
            ));
        }
    }
}

fn refusal_of(refusal: &SdkValue) -> SdkRefusal {
    SdkRefusal {
        code: refusal.get_text("code").unwrap_or("Failed").to_owned(),
        message: refusal.get_text("message").unwrap_or("").to_owned(),
        detail: refusal.get("detail").cloned().unwrap_or_default(),
    }
}

fn lost(op: &str, why: &str) -> SdkRefusal {
    refused(
        "Unknown",
        format!("{op}: the worker's answer was lost ({why}); it may have run, and was not resent"),
    )
}

/// Whether the worker answers its secret: `{ kind: "worker", version, pid }`.
/// Its answer is waited for at most [`PING_WAIT`].
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
