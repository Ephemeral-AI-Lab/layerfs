//! Both ends of a streamed host write: the CLI reads its input in bounded
//! pieces on a reader thread and sends them as frames; the worker hands
//! them to the Store call one change and one piece at a time.
//!
//! Memory on each side is independent of the input's size: the CLI holds
//! at most [`QUEUED`] read items plus the one it sends, the worker one
//! frame. The exception is one `file apply` change other than a write's
//! content (an edit's text): the engine needs it whole on every route. The
//! CLI's reader thread builds it from the input with no copy of its text,
//! converts it, and the sender prints it once; the worker builds it from
//! its frames the same way. Per connection that is the change's value and,
//! on the CLI, its printed text: about twice the element's size at most.
#![cfg(any(target_os = "macos", target_os = "linux"))]
use crate::host_write::{header, InputKind};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::json;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_machine::worker::{
    StreamFrame, StreamReader, StreamSender, STREAM_KEEPALIVE, STREAM_PIECE,
};
use layerfs_store_service::methods::Source;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::sync::mpsc::{sync_channel, RecvTimeoutError, SyncSender};

/// Read items waiting for the sender, beside the one being sent.
const QUEUED: usize = 2;

/// What the reader thread passes on.
enum Item {
    /// A piece of content.
    Piece(Vec<u8>),
    /// One `file apply` change, in the Store's form.
    Change(SdkValue),
    /// The input's end.
    End,
}

fn not_json(why: &str) -> SdkRefusal {
    refused("InvalidInput", format!("the input is not JSON: {why}"))
}

fn unreadable(error: std::io::Error) -> SdkRefusal {
    refused("InvalidInput", format!("cannot read the input: {error}"))
}

/// Sends the deferred `source` as `kind`. Nothing is resent; an error
/// stops the input before its end, so the worker commits nothing.
pub(crate) fn send(
    kind: InputKind,
    source: &mut Option<Box<dyn Read + Send>>,
    sender: &mut StreamSender<'_>,
) -> Result<(), SdkRefusal> {
    let source = source
        .take()
        .ok_or_else(|| refused("Failed", "the input was already read"))?;
    let (items, received) = sync_channel::<Result<Item, SdkRefusal>>(QUEUED);
    // The reader is detached: on an early stop it ends at its next send,
    // or with this process, without the sender waiting for its input. It
    // parses and converts changes too, so the sender's keepalive goes on
    // while a large change is read.
    std::thread::Builder::new()
        .name("layerfs-input".to_owned())
        .spawn(move || {
            let outcome = match kind {
                InputKind::Content => pieces(source, &items),
                InputKind::Changes => elements(source, &items),
            };
            if let Err(refusal) = outcome {
                let _ = items.send(Err(refusal));
            }
        })
        .map_err(|error| refused("Failed", format!("cannot start the input reader: {error}")))?;
    loop {
        let item = match received.recv_timeout(STREAM_KEEPALIVE) {
            Ok(item) => item?,
            Err(RecvTimeoutError::Timeout) => {
                sender.wait()?;
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(refused("Failed", "the input reader stopped"))
            }
        };
        match (kind, item) {
            (_, Item::Piece(piece)) => sender.content(&piece)?,
            (InputKind::Content, Item::End) => return sender.end_content(),
            (_, Item::Change(change)) => send_change(change, sender)?,
            (InputKind::Changes, Item::End) => return Ok(()),
        }
    }
}

/// One converted change, with a write's content streamed after it.
fn send_change(mut change: SdkValue, sender: &mut StreamSender<'_>) -> Result<(), SdkRefusal> {
    let SdkValue::Map(fields) = &mut change else {
        return Err(refused("Failed", "a change was not converted to a record"));
    };
    let content = match fields
        .iter()
        .position(|(name, _)| matches!(name.as_str(), "content" | "bytes"))
    {
        Some(at)
            if fields
                .iter()
                .any(|(n, v)| n == "op" && v == &SdkValue::text("write")) =>
        {
            Some(fields.remove(at).1)
        }
        _ => None,
    };
    let Some(content) = content else {
        return sender.change(change);
    };
    fields.push(("streamed".to_owned(), SdkValue::Bool(true)));
    sender.change(change)?;
    match &content {
        SdkValue::Text(text) => sender.content(text.as_bytes())?,
        SdkValue::Bytes(bytes) => sender.content(bytes)?,
        _ => {
            return Err(refused(
                "InvalidInput",
                "write content must be text or bytes",
            ))
        }
    }
    sender.end_content()
}

/// Content in pieces of [`STREAM_PIECE`] bytes, the last one shorter.
fn pieces(
    mut source: Box<dyn Read + Send>,
    items: &SyncSender<Result<Item, SdkRefusal>>,
) -> Result<(), SdkRefusal> {
    loop {
        let mut piece = vec![0; STREAM_PIECE];
        let mut filled = 0;
        let ended = loop {
            match source.read(&mut piece[filled..]) {
                Ok(0) => break true,
                Ok(read) => {
                    filled += read;
                    if filled == piece.len() {
                        break false;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(unreadable(error)),
            }
        };
        piece.truncate(filled);
        if filled > 0 && items.send(Ok(Item::Piece(piece))).is_err() {
            return Ok(());
        }
        if ended {
            let _ = items.send(Ok(Item::End));
            return Ok(());
        }
    }
}

/// The elements of a JSON list, each built from the input as it is read
/// (no copy of its text) and converted to the Store's form.
fn elements(
    source: Box<dyn Read + Send>,
    items: &SyncSender<Result<Item, SdkRefusal>>,
) -> Result<(), SdkRefusal> {
    let mut input = BufReader::with_capacity(64 << 10, source);
    let next = |input: &mut BufReader<_>| -> Result<Option<u8>, SdkRefusal> {
        loop {
            let byte = input.fill_buf().map_err(unreadable)?.first().copied();
            match byte {
                Some(b' ' | b'\t' | b'\n' | b'\r') => input.consume(1),
                other => return Ok(other),
            }
        }
    };
    if next(&mut input)? != Some(b'[') {
        return Err(refused(
            "InvalidInput",
            "`changes` must be a list of changes",
        ));
    }
    input.consume(1);
    if next(&mut input)? == Some(b']') {
        input.consume(1);
    } else {
        loop {
            let element = json::read_value(&mut input).map_err(|error| not_json(&error.0))?;
            let change = crate::host_write::store_change(&element)?;
            drop(element);
            if items.send(Ok(Item::Change(change))).is_err() {
                return Ok(());
            }
            match next(&mut input)? {
                Some(b',') => input.consume(1),
                Some(b']') => {
                    input.consume(1);
                    break;
                }
                None => return Err(not_json("it ends inside the list")),
                Some(_) => return Err(not_json("expected , or ] after an element")),
            }
        }
    }
    if next(&mut input)?.is_some() {
        return Err(not_json("text after the list"));
    }
    let _ = items.send(Ok(Item::End));
    Ok(())
}

/// One spilled change's data frames as a byte stream: the change is built
/// from them as they arrive, holding one frame, never its whole text.
struct Spilled<'r, 'a> {
    reader: &'r mut StreamReader<'a>,
    at: usize,
    ended: bool,
    /// Why the frames stopped, kept for the refusal.
    failed: Option<SdkRefusal>,
}

impl<'r, 'a> Spilled<'r, 'a> {
    fn read(reader: &'r mut StreamReader<'a>) -> Result<SdkValue, SdkRefusal> {
        // the reader's buffer still holds an earlier frame: start past it
        let at = reader.piece.len();
        let mut input = Self {
            reader,
            at,
            ended: false,
            failed: None,
        };
        let change = json::read_value(&mut input);
        // only blanks may follow the change, then its content's end
        let rest = loop {
            match input.fill_buf() {
                Ok([]) => break Ok(()),
                Ok(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => {
                    let blank = bytes.len();
                    input.consume(blank);
                }
                Ok(_) => break Err(()),
                Err(_) => break Err(()),
            }
        };
        if let Some(refusal) = input.failed {
            return Err(refusal);
        }
        match (change, rest) {
            (Ok(change), Ok(())) => Ok(change),
            _ => Err(out_of_order()),
        }
    }
}

impl Read for Spilled<'_, '_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let available = self.fill_buf()?;
        let n = available.len().min(out.len());
        out[..n].copy_from_slice(&available[..n]);
        self.consume(n);
        Ok(n)
    }
}

impl BufRead for Spilled<'_, '_> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        while !self.ended && self.at == self.reader.piece.len() {
            match self.reader.next_frame() {
                Ok(StreamFrame::Piece) => self.at = 0,
                Ok(StreamFrame::EndContent) => self.ended = true,
                Ok(StreamFrame::Wait) => {}
                Ok(_) => {
                    self.failed = Some(out_of_order());
                    return Err(std::io::ErrorKind::InvalidData.into());
                }
                Err(refusal) => {
                    self.failed = Some(refusal);
                    return Err(std::io::ErrorKind::UnexpectedEof.into());
                }
            }
        }
        Ok(match self.ended {
            true => &[],
            false => &self.reader.piece[self.at..],
        })
    }

    fn consume(&mut self, n: usize) {
        self.at += n;
    }
}

/// The worker's streamed input as the Store call pulls it.
struct Body<'r, 'a> {
    reader: &'r mut StreamReader<'a>,
    /// `file apply`: changes come from the stream.
    changes: bool,
    /// A content is open: only its pieces may follow.
    content: bool,
    counted: (u64, u64),
    ended: bool,
}

impl Body<'_, '_> {
    fn frame(&mut self) -> Result<StreamFrame, SdkRefusal> {
        loop {
            match self.reader.next_frame()? {
                StreamFrame::Wait => {}
                frame => return Ok(frame),
            }
        }
    }

    fn opened(&mut self, change: SdkValue) -> SdkValue {
        self.counted.0 += 1;
        self.content = change.get("streamed") == Some(&SdkValue::Bool(true));
        change
    }
}

fn out_of_order() -> SdkRefusal {
    refused(
        "InvalidInput",
        "streamed input: a frame out of order; nothing was committed",
    )
}

impl Source for Body<'_, '_> {
    fn next(&mut self) -> Result<Option<SdkValue>, SdkRefusal> {
        if self.content || self.ended {
            return Err(out_of_order());
        }
        match self.frame()? {
            StreamFrame::Change(change) if self.changes => Ok(Some(self.opened(change))),
            StreamFrame::Spilled if self.changes => {
                let change = Spilled::read(self.reader)?;
                Ok(Some(self.opened(change)))
            }
            StreamFrame::End { changes, bytes } if (changes, bytes) == self.counted => {
                self.ended = true;
                Ok(None)
            }
            StreamFrame::End { .. } => Err(refused(
                "InvalidInput",
                "streamed input: its end does not match what was sent; nothing was committed",
            )),
            _ => Err(out_of_order()),
        }
    }

    fn piece(&mut self) -> Result<Option<&[u8]>, SdkRefusal> {
        if !self.content {
            return Err(out_of_order());
        }
        match self.frame()? {
            StreamFrame::Piece => {
                self.counted.1 += self.reader.piece.len() as u64;
                Ok(Some(&self.reader.piece))
            }
            StreamFrame::EndContent => {
                self.content = false;
                Ok(None)
            }
            _ => Err(out_of_order()),
        }
    }
}

/// Runs one authenticated write request on the worker, its streamed input
/// read from `reader`. Commits only after the input's end was read.
pub fn serve(home: &Path, request: &SdkValue, reader: &mut StreamReader<'_>) -> Outcome {
    let (method, store, project, body, fields) = header(request)?;
    let store = layerfs_home::names::StoreName::parse(store)?;
    let project = layerfs_home::names::ProjectName::parse(project)?;
    let resolved = crate::resolve::target(home, &format!("{store}/{project}"))?;
    let fields = fields.clone();
    if body == "none" {
        return crate::open::run_host(home, &resolved.store, &resolved.project, method, fields);
    }
    let mut source = Body {
        reader,
        changes: body == "changes",
        content: body == "content",
        counted: (0, 0),
        ended: false,
    };
    crate::open::run_host_streamed(
        home,
        &resolved.store,
        &resolved.project,
        method,
        fields,
        &mut source,
    )
}
