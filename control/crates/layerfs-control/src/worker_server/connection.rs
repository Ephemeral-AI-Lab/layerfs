//! One connection: the hello, the request, its operation, and the answer.
//!
//! What one peer can hold is bounded (layerfs-project#177). The whole hello
//! is read by one deadline, not one wait per read. Every send waits at most
//! the write wait without progress, so a caller that still reads is never
//! cut and one that stopped is. After the first send that fails, [`Wire`]
//! writes nothing more: later events fail at once, no answer is written and
//! the connection is closed, while the operation ends its own step. A panic
//! in an operation ends its connection alone (layerfs-project#179 D7): it
//! is logged, and the connection closes with no answer, so the caller
//! reports `Unknown`; a read-only operation that wrote nothing yet answers
//! `Failed`.
use super::{ops, Server};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::refused;
use layerfs_machine::worker::{read_frame, read_frame_within, write_frame, MAX_HELLO};
use std::io::Read;
use std::net::{Shutdown, TcpStream};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

/// The longest the worker waits for the next byte of a request, for a send
/// to make progress, and for its caller to close after the answer.
pub const REQUEST_WAIT: Duration = Duration::from_secs(30);

/// The most of a panic's message, or of an operation's name, one log line
/// holds.
const LOGGED: usize = 300;

/// The connection's stream from the request on: events, then one answer.
struct Wire {
    stream: TcpStream,
    /// A frame was started: some of its bytes may have reached the caller.
    wrote: bool,
    /// A send failed or made no progress in time: nothing more is written.
    closed: bool,
}

impl Wire {
    /// Writes `frame` unless an earlier send failed. One that fails now
    /// closes the connection for both sides, so the caller learns at once
    /// that its answer is lost, and later sends do not wait again.
    fn send(&mut self, frame: &SdkValue) -> std::io::Result<()> {
        if self.closed {
            return Err(std::io::ErrorKind::NotConnected.into());
        }
        self.wrote = true;
        let sent = write_frame(&mut self.stream, frame);
        if sent.is_err() {
            self.closed = true;
            let _ = self.stream.shutdown(Shutdown::Both);
        }
        sent
    }

    fn event(&mut self, event: SdkValue) -> Result<(), SdkRefusal> {
        self.send(&SdkValue::map([("event", event)]))
            .map_err(|e| refused("Failed", format!("worker event stream closed: {e}")))
    }

    /// Writes the answer `frame`, then waits for the caller to close first.
    /// The caller sends nothing after its request and closes when it has
    /// read the answer. Apple Container's published port drops what it
    /// still holds for the host when the guest side closes first, which cut
    /// the tail off large answers (an export's last parts). Reads only that
    /// close, at most `close`; a caller that never closes then loses
    /// nothing worse than before.
    fn answer(&mut self, frame: &SdkValue, close: Duration) {
        if self.send(frame).is_ok() && self.stream.set_read_timeout(Some(close)).is_ok() {
            let _ = self.stream.read(&mut [0u8; 1]);
        }
    }
}

/// A stream read until `end`, however many reads that takes.
struct Until<'a> {
    stream: &'a TcpStream,
    end: Instant,
}

impl Read for Until<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let left = self.end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        self.stream.set_read_timeout(Some(left))?;
        let mut stream = self.stream;
        stream.read(buffer)
    }
}

/// Serves `stream`: one request, on this thread. A panic ends here.
pub(super) fn serve(server: &Server, stream: TcpStream) {
    #[cfg(feature = "worker-fault")]
    let _served = server.faults.serving();
    let mut wire = Wire {
        stream,
        wrote: false,
        closed: false,
    };
    // the operation, once its request is read
    let mut op = None;
    let panic = match catch_unwind(AssertUnwindSafe(|| exchange(server, &mut wire, &mut op))) {
        Ok(()) => return,
        Err(panic) => panic,
    };
    let message = (panic.downcast_ref::<&str>().copied())
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("no message");
    let op = op.as_deref().unwrap_or("a connection before its request");
    let (op, message) = (one_line(op), one_line(message));
    server.log(&format!("{op} panicked: {message}"));
    if ops::read_only(&op) && !wire.wrote {
        let failed = refused(
            "Failed",
            format!(
                "{op} panicked in the worker and changed nothing; worker.log holds its message"
            ),
        );
        wire.answer(&refusal_frame(failed), server.waits.close);
    }
}

/// `text` as part of one log line: no control characters, at most
/// [`LOGGED`] characters.
fn one_line(text: &str) -> String {
    (text.chars().take(LOGGED))
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// Equal secrets, compared in time independent of where they differ.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// The hello, the request and its answer on `wire`. `served` is set to the
/// request's operation once it is read.
fn exchange(server: &Server, wire: &mut Wire, served: &mut Option<String>) {
    let waits = &server.waits;
    let _ = wire.stream.set_nodelay(true);
    if wire.stream.set_write_timeout(Some(waits.write)).is_err() {
        return;
    }
    // the secret first, from a small frame: nothing larger is read, and
    // nothing is waited for past one deadline, before it matches
    let mut first = Until {
        stream: &wire.stream,
        end: Instant::now() + waits.hello,
    };
    let hello = match read_frame_within(&mut first, MAX_HELLO) {
        Ok(Some(hello)) => hello,
        _ => return,
    };
    if !same(hello.get_text("secret").unwrap_or(""), &server.secret) {
        let denied = refused("Denied", "the worker's secret does not match");
        let _ = write_frame(&mut wire.stream, &refusal_frame(denied));
        return;
    }
    if write_frame(&mut wire.stream, &server.ready).is_err()
        || wire.stream.set_read_timeout(Some(waits.request)).is_err()
    {
        return;
    }
    let request = match read_frame(&mut wire.stream) {
        Ok(Some(request)) => request,
        _ => return,
    };
    let _ = wire.stream.set_read_timeout(None);
    let stepped = request.get("trace_step") == Some(&SdkValue::Bool(true));
    if stepped {
        layerfs_home::trace::steps_begin();
    }
    let _home = layerfs_home::home::guest::sent(request.get_text("home_id"));
    let (op, args) = op_and_args(request);
    let op = served.insert(op).as_str();
    #[cfg(feature = "worker-fault")]
    server.faults.before(op, &mut |event| wire.event(event));
    let outcome = ops::run(op, args, server, &mut |event| wire.event(event));
    let mut frame = match outcome {
        Ok(value) => SdkValue::map([("value", value)]),
        Err(refusal) => refusal_frame(refusal),
    };
    if stepped {
        layerfs_home::trace::step("result");
        let steps = layerfs_home::trace::steps_value(layerfs_home::trace::steps_take());
        frame = layerfs_home::home::with_field(frame, "steps", steps);
    }
    wire.answer(&frame, waits.close);
}

/// A request's `op` and its `args` (moved out, not copied: they may hold a
/// large part of a file).
fn op_and_args(request: SdkValue) -> (String, SdkValue) {
    let (mut op, mut args) = (String::new(), SdkValue::Null);
    if let SdkValue::Map(fields) = request {
        for (name, value) in fields {
            match (name.as_str(), value) {
                ("op", SdkValue::Text(text)) => op = text,
                ("args", value) => args = value,
                _ => {}
            }
        }
    }
    (op, args)
}

fn refusal_frame(refusal: SdkRefusal) -> SdkValue {
    SdkValue::map([(
        "refused",
        SdkValue::map([
            ("code", SdkValue::Text(refusal.code)),
            ("message", SdkValue::Text(refusal.message)),
            ("detail", refusal.detail),
        ]),
    )])
}
