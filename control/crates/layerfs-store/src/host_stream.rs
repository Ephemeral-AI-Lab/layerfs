//! Both ends of a streamed host write: the CLI reads its input in bounded
//! pieces on a reader thread and sends them as frames; the worker hands
//! them to the Store call one change and one piece at a time.
//!
//! Memory on each side is independent of the input's size: the CLI holds
//! at most [`QUEUED`] read pieces plus the one it sends, the worker one
//! frame. The exception is one `file apply` change, which the CLI parses
//! whole (and the worker too, when it is too large for one frame); its
//! write content still streams.
#![cfg(any(target_os = "macos", target_os = "linux"))]
use crate::host_write::{header, InputKind};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::json;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_machine::worker::{
    StreamFrame, StreamReader, StreamSender, STREAM_KEEPALIVE, STREAM_PIECE,
};
use layerfs_store_service::methods::Source;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::mpsc::{sync_channel, RecvTimeoutError, SyncSender};

/// Read pieces waiting for the sender, beside the one being sent.
const QUEUED: usize = 2;

/// What the reader thread passes on: a piece of content or one change's
/// text, the input's end, or why it stopped.
type Item = Result<Option<Vec<u8>>, SdkRefusal>;

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
    let (items, received) = sync_channel::<Item>(QUEUED);
    // The reader is detached: on an early stop it ends at its next send,
    // or with this process, without the sender waiting for its input.
    std::thread::Builder::new()
        .name("layerfs-input".to_owned())
        .spawn(move || match kind {
            InputKind::Content => pieces(source, &items),
            InputKind::Changes => elements(source, &items),
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
            (InputKind::Content, Some(piece)) => sender.content(&piece)?,
            (InputKind::Content, None) => return sender.end_content(),
            (InputKind::Changes, Some(text)) => change(&text, sender)?,
            (InputKind::Changes, None) => return Ok(()),
        }
    }
}

/// One `file apply` element: checked and converted as the direct route
/// does, then sent with any write content after it.
fn change(text: &[u8], sender: &mut StreamSender<'_>) -> Result<(), SdkRefusal> {
    let text = std::str::from_utf8(text).map_err(|_| not_json("it is not UTF-8"))?;
    let element = json::parse(text).map_err(|error| not_json(&error.0))?;
    let SdkValue::Map(mut fields) = crate::host_write::store_change(&element)? else {
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
        return sender.change(&SdkValue::Map(fields));
    };
    fields.push(("streamed".to_owned(), SdkValue::Bool(true)));
    sender.change(&SdkValue::Map(fields))?;
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

/// Content in pieces of at most [`STREAM_PIECE`] bytes.
fn pieces(mut source: Box<dyn Read + Send>, items: &SyncSender<Item>) {
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
                Err(error) => {
                    let _ = items.send(Err(unreadable(error)));
                    return;
                }
            }
        };
        piece.truncate(filled);
        if filled > 0 && items.send(Ok(Some(piece))).is_err() {
            return;
        }
        if ended {
            let _ = items.send(Ok(None));
            return;
        }
    }
}

/// The elements of a JSON list, each one element's text, by a scan that
/// tracks only strings and nesting. Each element is parsed by itself.
fn elements(source: Box<dyn Read + Send>, items: &SyncSender<Item>) {
    let outcome = (|| {
        let mut bytes = BufReader::with_capacity(64 << 10, source).bytes();
        let mut next =
            || -> Result<Option<u8>, SdkRefusal> { bytes.next().transpose().map_err(unreadable) };
        let blank = |byte: u8| matches!(byte, b' ' | b'\t' | b'\n' | b'\r');
        let first = loop {
            match next()? {
                Some(byte) if blank(byte) => {}
                other => break other,
            }
        };
        if first != Some(b'[') {
            return Err(refused(
                "InvalidInput",
                "`changes` must be a list of changes",
            ));
        }
        let (mut element, mut depth, mut quoted, mut escaped) = (Vec::new(), 0usize, false, false);
        let mut count = 0usize;
        loop {
            let byte = next()?.ok_or_else(|| not_json("it ends inside the list"))?;
            if quoted {
                element.push(byte);
                match (escaped, byte) {
                    (true, _) => escaped = false,
                    (false, b'\\') => escaped = true,
                    (false, b'"') => quoted = false,
                    _ => {}
                }
                continue;
            }
            let end = match byte {
                b'"' => {
                    quoted = true;
                    false
                }
                b'[' | b'{' => {
                    depth += 1;
                    false
                }
                b']' | b'}' if depth > 0 => {
                    depth -= 1;
                    false
                }
                b',' | b']' if depth == 0 => true,
                _ => false,
            };
            if !end {
                element.push(byte);
                continue;
            }
            let empty = element.iter().all(|&byte| blank(byte));
            match (empty, byte) {
                (true, b']') if count == 0 => break,
                (true, _) => return Err(not_json("an empty list element")),
                (false, _) => {
                    count += 1;
                    if items.send(Ok(Some(std::mem::take(&mut element)))).is_err() {
                        return Ok(());
                    }
                }
            }
            if byte == b']' {
                break;
            }
        }
        while let Some(byte) = next()? {
            if !blank(byte) {
                return Err(not_json("text after the list"));
            }
        }
        let _ = items.send(Ok(None));
        Ok(())
    })();
    if let Err(refusal) = outcome {
        let _ = items.send(Err(refusal));
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
                let mut text = Vec::new();
                loop {
                    match self.frame()? {
                        StreamFrame::Piece => text.extend_from_slice(&self.reader.piece),
                        StreamFrame::EndContent => break,
                        _ => return Err(out_of_order()),
                    }
                }
                let text = String::from_utf8(text).map_err(|_| out_of_order())?;
                let change = json::parse(&text).map_err(|_| out_of_order())?;
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
