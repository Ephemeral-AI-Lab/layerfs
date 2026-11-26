//! Streamed input of one host write, in bounded frames after its request.
//!
//! After an ordinary request whose arguments name a streamed `body`, the
//! caller sends frames until one `end`. A JSON frame (header top bit clear,
//! at most [`HOST_MAX_REQUEST`] bytes) is `{"change": {...}}`,
//! `{"spilled": true}` (the next content is one change's JSON text, for a
//! change too large for one frame), `{"wait": true}` (the input is still
//! coming) or `{"end": {"changes": n, "bytes": b}}`. A data frame (top bit
//! set) carries at most [`STREAM_PIECE`] bytes of the current content; an
//! empty data frame ends that content.
//!
//! The worker reads a frame only when its Store call asks for the next
//! change or piece, so a faster caller waits in the kernel socket buffer:
//! backpressure with one frame held on each side. The worker commits only
//! after it has read `end`, so a failure before the caller has written all
//! of `end` leaves nothing committed; after that, a lost answer is `Unknown`.
use super::super::frame::ATTACHED;
use super::super::host_frame::{self, HOST_MAX_REPLY, HOST_MAX_REQUEST};
use super::{decode_answer, unavailable, unknown, HostPaths};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::{
    home::host_socket,
    json,
    outcome::{refused, Outcome},
    trace,
};
use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

/// Largest data frame of streamed content: four engine records
/// (`MAX_PLAINTEXT_BYTES`), so full pieces are written as whole records.
pub const STREAM_PIECE: usize = 4 * layerfs_bridge::duplex::MAX_PLAINTEXT_BYTES;
/// The worker's minimum-progress rule (§11, review M3): while it waits
/// for its caller, a streamed input must deliver [`STREAM_PROGRESS`] bytes
/// within this much waiting, or end. Only the time the worker spends
/// blocked on the input counts, never its own Store work, and a `wait`
/// frame is no progress, so neither a stalled nor a trickling input holds
/// a write thread past it: it is refused, nothing committed.
pub const STREAM_WAIT: Duration = Duration::from_secs(30);
/// Input bytes, frame headers included, that renew [`STREAM_WAIT`].
pub const STREAM_PROGRESS: usize = STREAM_PIECE;
/// A caller whose input produced nothing for this long sends `wait`, which
/// only shows the caller the worker is still there.
pub const STREAM_KEEPALIVE: Duration = Duration::from_secs(10);
/// The longest one caller write may go without progress.
const SEND_WAIT: Duration = Duration::from_secs(60);
/// How long a caller whose send failed reads for an early refusal.
const EARLY_ANSWER: Duration = Duration::from_secs(2);

/// One frame of a streamed input, as the worker reads it.
pub enum StreamFrame {
    /// The next change.
    Change(SdkValue),
    /// The next content is one change's JSON text.
    Spilled,
    /// The caller's input is still coming.
    Wait,
    /// The whole input was sent: its change count and content bytes.
    End {
        /// Changes, spilled ones included.
        changes: u64,
        /// Content bytes, spilled JSON excluded.
        bytes: u64,
    },
    /// A piece of the current content, in [`StreamReader::piece`].
    Piece,
    /// The current content ended.
    EndContent,
}

/// The worker's side of one streamed input: one frame held at a time.
pub struct StreamReader<'a> {
    stream: &'a UnixStream,
    /// The last data frame's bytes.
    pub piece: Vec<u8>,
    /// Time blocked on the input since the last renewal.
    waited: Duration,
    /// Input bytes since the last renewal.
    progressed: usize,
}

fn lost(error: io::Error) -> SdkRefusal {
    if error.kind() == io::ErrorKind::TimedOut {
        return refused(
            "Unavailable",
            format!(
                "the streamed input delivered less than {} bytes (four engine records) in {} s of waiting; nothing was committed",
                STREAM_PROGRESS,
                STREAM_WAIT.as_secs()
            ),
        );
    }
    refused(
        "Unavailable",
        format!("the streamed input stopped before its end: {error}; nothing was committed"),
    )
}

impl<'a> StreamReader<'a> {
    /// Reads frames following a request on `stream`.
    pub fn new(stream: &'a UnixStream) -> Self {
        Self {
            stream,
            piece: Vec::new(),
            waited: Duration::ZERO,
            progressed: 0,
        }
    }

    fn fill(&mut self, output: &mut [u8], end: Instant) -> Result<(), SdkRefusal> {
        let mut at = 0;
        while at < output.len() {
            match host_socket::read_before(self.stream, &mut output[at..], Some(end)) {
                Ok(0) => return Err(lost(io::ErrorKind::UnexpectedEof.into())),
                Ok(read) => at += read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(lost(error)),
            }
        }
        Ok(())
    }

    /// The next frame, under the minimum-progress rule of [`STREAM_WAIT`].
    /// Sizes are checked from the header before any body is read.
    pub fn next_frame(&mut self) -> Result<StreamFrame, SdkRefusal> {
        let began = Instant::now();
        let end = began + STREAM_WAIT.saturating_sub(self.waited);
        let frame = self.frame(end);
        self.waited += began.elapsed();
        frame
    }

    fn counted(&mut self, bytes: usize) {
        self.progressed += bytes;
        if self.progressed >= STREAM_PROGRESS {
            self.progressed = 0;
            self.waited = Duration::ZERO;
        }
    }

    fn frame(&mut self, end: Instant) -> Result<StreamFrame, SdkRefusal> {
        let mut header = [0u8; 4];
        self.fill(&mut header, end)?;
        let header = u32::from_be_bytes(header);
        let len = (header & !ATTACHED) as usize;
        if header & ATTACHED != 0 {
            if len > STREAM_PIECE {
                return Err(invalid("a data frame exceeds its byte ceiling"));
            }
            self.piece.resize(len, 0);
            let mut piece = std::mem::take(&mut self.piece);
            let filled = self.fill(&mut piece, end);
            self.piece = piece;
            filled?;
            self.counted(4 + len);
            return Ok(match len {
                0 => StreamFrame::EndContent,
                _ => StreamFrame::Piece,
            });
        }
        if len > HOST_MAX_REQUEST {
            return Err(invalid("a JSON frame exceeds its byte ceiling"));
        }
        let mut body = vec![0; len];
        self.fill(&mut body, end)?;
        let frame = host_frame::decode(&body)?;
        let count = |field: &str| {
            frame
                .get("end")
                .and_then(|end| end.get_int(field))
                .and_then(|n| u64::try_from(n).ok())
        };
        match frame {
            SdkValue::Map(ref fields) if fields.len() == 1 => {}
            _ => return Err(invalid("a JSON frame holds one field")),
        }
        // a `wait` frame renews nothing; every other frame is progress
        if !frame.get_bool("wait") {
            self.counted(4 + len);
        }
        Ok(if let Some(change) = frame.get("change") {
            StreamFrame::Change(change.clone())
        } else if frame.get_bool("spilled") {
            StreamFrame::Spilled
        } else if frame.get_bool("wait") {
            StreamFrame::Wait
        } else if let (Some(changes), Some(bytes)) = (count("changes"), count("bytes")) {
            StreamFrame::End { changes, bytes }
        } else {
            return Err(invalid("an unknown JSON frame"));
        })
    }
}

fn invalid(message: &str) -> SdkRefusal {
    refused(
        "InvalidInput",
        format!("streamed input: {message}; nothing was committed"),
    )
}

/// The caller's side of one streamed input.
pub struct StreamSender<'a> {
    stream: &'a UnixStream,
    changes: u64,
    bytes: u64,
    /// Some byte of this exchange was written.
    started: bool,
    /// A write failed: the exchange is lost.
    lost: Option<String>,
    /// That write failed at its send bound: the worker read nothing for
    /// [`SEND_WAIT`].
    stalled: bool,
}

impl<'a> StreamSender<'a> {
    fn new(stream: &'a UnixStream) -> Self {
        Self {
            stream,
            changes: 0,
            bytes: 0,
            started: false,
            lost: None,
            stalled: false,
        }
    }

    fn raw(&mut self, mut bytes: &[u8]) -> Result<(), SdkRefusal> {
        if let Some(lost) = &self.lost {
            return Err(refused("Unavailable", lost.clone()));
        }
        let mut stream = self.stream;
        while !bytes.is_empty() {
            let written = stream
                .set_write_timeout(Some(SEND_WAIT))
                .and_then(|()| stream.write(bytes));
            match written {
                Ok(0) => return Err(self.fail(io::ErrorKind::WriteZero.into())),
                Ok(written) => {
                    self.started = true;
                    bytes = &bytes[written..];
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(self.fail(error)),
            }
        }
        Ok(())
    }

    fn fail(&mut self, error: io::Error) -> SdkRefusal {
        self.stalled = matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        );
        let message = format!("host worker connection: {error}");
        self.lost = Some(message.clone());
        refused("Unavailable", message)
    }

    fn json(&mut self, frame: &SdkValue) -> Result<(), SdkRefusal> {
        let body = json::print(frame);
        if body.len() > HOST_MAX_REQUEST {
            return Err(refused("TooLarge", "a JSON frame exceeds its byte ceiling"));
        }
        self.raw(&(body.len() as u32).to_be_bytes())?;
        self.raw(body.as_bytes())
    }

    fn data(&mut self, bytes: &[u8]) -> Result<(), SdkRefusal> {
        self.raw(&(bytes.len() as u32 | ATTACHED).to_be_bytes())?;
        self.raw(bytes)
    }

    /// Sends one change in the Store's form, printed once. A change whose
    /// frame would exceed the JSON ceiling is sent spilled, its text as
    /// content; the value is dropped before that text is sent.
    pub fn change(&mut self, change: SdkValue) -> Result<(), SdkRefusal> {
        const WRAP: &str = "{\"change\":";
        self.changes += 1;
        let text = json::print(&change);
        drop(change);
        if !host_frame::within_budget(text.as_bytes()) {
            // a file input is routed direct before sending (`fits_worker`);
            // a piped one cannot be, once it started
            return Err(refused(
                "TooLarge",
                format!(
                    "change {} exceeds the host worker's budget for one change (4 MiB decoded, 128 fields per object, 32 levels); nothing was committed. A change this large runs directly when the changes come from a file (--from)",
                    self.changes
                ),
            ));
        }
        if WRAP.len() + text.len() < HOST_MAX_REQUEST {
            let frame = format!("{WRAP}{text}}}");
            self.raw(&(frame.len() as u32).to_be_bytes())?;
            return self.raw(frame.as_bytes());
        }
        self.json(&SdkValue::map([("spilled", SdkValue::Bool(true))]))?;
        for piece in text.as_bytes().chunks(STREAM_PIECE) {
            self.data(piece)?;
        }
        self.data(&[])
    }

    /// Sends content bytes in data frames of at most [`STREAM_PIECE`].
    pub fn content(&mut self, bytes: &[u8]) -> Result<(), SdkRefusal> {
        for piece in bytes.chunks(STREAM_PIECE) {
            self.data(piece)?;
            self.bytes += piece.len() as u64;
        }
        Ok(())
    }

    /// Ends the current content.
    pub fn end_content(&mut self) -> Result<(), SdkRefusal> {
        self.data(&[])
    }

    /// Tells the worker the input is still coming.
    pub fn wait(&mut self) -> Result<(), SdkRefusal> {
        self.json(&SdkValue::map([("wait", SdkValue::Bool(true))]))
    }

    fn end(&mut self) -> Result<(), SdkRefusal> {
        let counts = SdkValue::map([
            ("changes", SdkValue::Int(self.changes as i64)),
            ("bytes", SdkValue::Int(self.bytes as i64)),
        ]);
        self.json(&SdkValue::map([("end", counts)]))
    }
}

fn not_committed(op: &str, id: &str, why: &str) -> SdkRefusal {
    let mut refusal = refused(
        "Unavailable",
        format!(
            "{op}: request {id} lost its host worker before the input's end was sent ({why}); nothing was committed"
        ),
    );
    refusal.detail = SdkValue::map([("request_id", SdkValue::text(id))]);
    refusal
}

/// A write whose input the worker did not read for [`SEND_WAIT`]: queued
/// behind busy write threads, or a worker that stopped reading. Named for
/// the load, as the hello refusal is.
fn stalled(op: &str, id: &str) -> SdkRefusal {
    let mut refusal = refused(
        "Unavailable",
        format!(
            "{op}: request {id}: the host worker read none of this write's input for {} s: earlier writes hold its write threads, or it is not responding; nothing was committed",
            SEND_WAIT.as_secs()
        ),
    );
    refusal.detail = SdkValue::map([
        ("request_id", SdkValue::text(id)),
        ("state", SdkValue::text("writes-busy")),
    ]);
    refusal
}

impl HostPaths {
    /// Makes one ordinary call whose input `body` streams after its request,
    /// launching a worker if the home has none. A failure before the end
    /// frame is written is a definite refusal (the worker's own, when it
    /// refused early); after it, a missing or invalid answer is `Unknown`.
    pub fn call_streamed(
        &self,
        op: &str,
        args: SdkValue,
        body: &mut dyn FnMut(&mut StreamSender<'_>) -> Result<(), SdkRefusal>,
        valid: impl FnOnce(&SdkValue) -> bool,
    ) -> Outcome {
        let path = &self.socket;
        let Some(greeted) = self.greet(false, true, op, args, None)? else {
            return Err(unavailable("start", "no accepting host endpoint"));
        };
        let (stream, id, began) = (greeted.stream, greeted.id, greeted.began);
        // a write carries no login; its request is dropped once encoded
        let frame = host_frame::encode(greeted.request.get(), HOST_MAX_REQUEST)?;
        drop(greeted.request);
        let mut sender = StreamSender::new(&stream);
        let sent = (sender.raw(&frame))
            .and_then(|()| body(&mut sender))
            .and_then(|()| sender.end());
        if let Err(refusal) = sent {
            trace::connection(&path.to_string_lossy(), op, sender.started, began);
            return Err(match sender.lost {
                // the caller's own input stopped it: its refusal, as sent
                None => refusal,
                Some(why) => early(&stream, op, &id).unwrap_or_else(|| match sender.stalled {
                    true => stalled(op, &id),
                    false => not_committed(op, &id, &why),
                }),
            });
        }
        trace::step("request");
        let answer = host_frame::read_host_frame(&stream, HOST_MAX_REPLY, None)
            .map_err(|refusal| unknown(op, &id, &refusal.message))
            .and_then(|frame| frame.ok_or_else(|| unknown(op, &id, "endpoint closed")));
        trace::connection(&path.to_string_lossy(), op, true, began);
        let answer = decode_answer(op, &id, answer?)?;
        trace::step("answer");
        if !valid(&answer) {
            return Err(unknown(op, &id, "invalid success payload"));
        }
        Ok(answer)
    }
}

/// A refusal the worker answered before the input's end: it cannot have
/// committed, so only a refusal echoing this request is taken.
fn early(stream: &UnixStream, op: &str, id: &str) -> Option<SdkRefusal> {
    let frame =
        host_frame::read_host_frame(stream, HOST_MAX_REPLY, Some(Instant::now() + EARLY_ANSWER))
            .ok()??;
    match decode_answer(op, id, frame) {
        Err(refusal) if refusal.code != "Unknown" => Some(refusal),
        _ => None,
    }
}
