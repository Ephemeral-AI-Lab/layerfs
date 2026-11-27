//! Both ends of a streamed host write: the CLI reads its input in bounded
//! pieces on a reader thread and sends them as frames; the worker hands
//! them to the Store call one change and one piece at a time.
//!
//! Every input the CLI streams is a regular file: a pipe, FIFO or terminal
//! is first copied into an unnamed private file ([`spool`]), which takes
//! disk equal to the input on the home's filesystem until it is sent: the
//! file is closed once its last byte is read for sending (or, on the
//! direct route, read whole), never held to the answer. So the worker
//! reads at disk speed, and a `file apply` list is checked before
//! anything is sent: a list with a change past the worker's budget for one
//! change (4 MiB decoded, 128 fields per object, 32 levels, counted with
//! the `{"change": …}` frame around it) runs direct ([`fits_worker`]), a
//! route, never a refusal.
//!
//! Memory. `file write` content: the CLI holds at most [`QUEUED`] + 2
//! pieces of [`STREAM_PIECE`] bytes (about 1 MiB): the queued ones, the
//! one its input thread is reading and the one it sends; the worker one
//! frame.
//! `file apply`: each element is parsed, then converted by moving its
//! values, so the CLI holds one change being read and, through a
//! rendezvous channel, one change being sent, plus that change's printed
//! text (within the budget, at most about 1.4 MB). A write change's
//! content is held whole in its change, so the CLI's worst case is about
//! the two largest consecutive changes, contents included, plus 1.4 MB;
//! [`fits_worker`] holds one element at a time and measures it without
//! printing it. A write's content then travels as data frames; any other
//! change is printed once, and the worker builds it from its frames under
//! the same budget, holding one frame and that value.
#![cfg(any(target_os = "macos", target_os = "linux"))]
use crate::host_write::{header, InputKind};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::json;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_machine::worker::{
    StreamFrame, StreamReader, StreamSender, FRAME_LIMITS, STREAM_KEEPALIVE, STREAM_PIECE,
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
    // pieces are bounded; a change is not, so changes pass hand to hand
    let queued = match kind {
        InputKind::Content => QUEUED,
        InputKind::Changes => 0,
    };
    let (items, received) = sync_channel::<Result<Item, SdkRefusal>>(queued);
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

/// A converted change and, for a write, its content taken out of it: the
/// content travels as data frames after the change, never in its JSON.
fn split_content(mut change: SdkValue) -> Result<(SdkValue, Option<SdkValue>), SdkRefusal> {
    let SdkValue::Map(fields) = &mut change else {
        return Err(refused("Failed", "a change was not converted to a record"));
    };
    let write = fields
        .iter()
        .any(|(n, v)| n == "op" && v == &SdkValue::text("write"));
    let content = match fields
        .iter()
        .position(|(name, _)| matches!(name.as_str(), "content" | "bytes"))
    {
        Some(at) if write => {
            let content = fields.remove(at).1;
            fields.push(("streamed".to_owned(), SdkValue::Bool(true)));
            Some(content)
        }
        _ => None,
    };
    Ok((change, content))
}

/// One converted change, with a write's content streamed after it.
fn send_change(change: SdkValue, sender: &mut StreamSender<'_>) -> Result<(), SdkRefusal> {
    let (change, content) = split_content(change)?;
    let Some(content) = content else {
        return sender.change(change);
    };
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

/// Whether every change of the `file apply` list in `file`, from its
/// current offset, fits the worker's budget for one change (a write's
/// content excepted: it streams). Read before anything is sent, so a list
/// with a larger change is routed direct; the offset is restored. One
/// element is held at a time and measured without printing it. A list
/// that does not parse is left to the streamed call, which refuses it. A
/// failed read or seek routes direct too, never to a refusal of its own:
/// the direct route reads the input again and meets any lasting failure
/// itself. (Were the offset not restored, the direct route would read the
/// list from inside it, which is not a list: it is refused, never
/// committed in part.)
pub(crate) fn fits_worker(file: &mut std::fs::File) -> bool {
    use std::io::{Seek, SeekFrom};
    let Ok(start) = file.stream_position() else {
        return false;
    };
    let fits = {
        let mut input = BufReader::with_capacity(
            64 << 10,
            Watched {
                file: &mut *file,
                failed: false,
            },
        );
        let mut fits = true;
        let _ = each_element(&mut input, &mut |change| {
            let (change, content) = split_content(change)?;
            drop(content);
            fits = layerfs_machine::worker::value_within_budget(&change);
            Ok(fits)
        });
        fits && !input.get_ref().failed
    };
    file.seek(SeekFrom::Start(start)).is_ok() && fits
}

/// A file read that notes whether any read failed.
struct Watched<'a> {
    file: &'a mut std::fs::File,
    failed: bool,
}

impl Read for Watched<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.file.read(buffer);
        self.failed |= read.is_err();
        read
    }
}

/// An input that is not a regular file, copied in bounded pieces into an
/// unnamed private file: the source to send from and that file. The file
/// is made 0600 with an exclusive create in the host machine's `spool`
/// folder (0700, owned by this user, else not used) and unlinked at once,
/// so no exit, a signal's included, leaves it behind; it takes disk equal
/// to the input until it is sent. When it cannot be made or written,
/// the input read so far and the rest are returned with no file: the call
/// then runs direct, as without a worker.
pub(crate) fn spool(
    home: &Path,
    mut source: Box<dyn Read + Send>,
) -> (Box<dyn Read + Send>, Option<std::fs::File>) {
    use std::io::{Seek, SeekFrom, Write};
    let Some(mut file) = spool_file(home) else {
        return (source, None);
    };
    let mut kept = 0u64;
    let mut buffer = vec![0; 64 << 10];
    loop {
        let read = match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            // the direct route reads the same failure
            Err(error) => {
                return (
                    Box::new(kept_part(file, kept).chain(Failed(Some(error)))),
                    None,
                )
            }
        };
        if file.write_all(&buffer[..read]).is_err() {
            buffer.truncate(read);
            let rest = std::io::Cursor::new(buffer).chain(source);
            return (Box::new(kept_part(file, kept).chain(rest)), None);
        }
        kept += read as u64;
    }
    match file.seek(SeekFrom::Start(0)).and_then(|_| file.try_clone()) {
        Ok(shared) => (Box::new(file), Some(shared)),
        Err(error) => (Box::new(Failed(Some(error))), None),
    }
}

/// The first `kept` bytes of the spool file, from its start.
fn kept_part(mut file: std::fs::File, kept: u64) -> Box<dyn Read + Send> {
    use std::io::{Seek, SeekFrom};
    match file.seek(SeekFrom::Start(0)) {
        Ok(_) => Box::new(file.take(kept)),
        Err(error) => Box::new(Failed(Some(error))),
    }
}

/// A new unnamed spool file, or None.
fn spool_file(home: &Path) -> Option<std::fs::File> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = layerfs_home::home::paths::machine_dir(home, layerfs_home::names::HOST_MACHINE)
        .join("spool");
    layerfs_home::home::private_dir(&dir).ok()?;
    // an existing folder is used only when it is this user's and private
    let folder = std::fs::symlink_metadata(&dir).ok()?;
    if !folder.is_dir()
        || folder.uid() != layerfs_home::home::flock::euid()
        || folder.mode() & 0o077 != 0
    {
        return None;
    }
    remove_stale(&dir);
    // `<pid>-<nanoseconds>-<n>`: never a name a killed process left
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let path = dir.join(format!(
        "{}-{nanos}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .ok()?;
    std::fs::remove_file(&path).ok()?;
    Some(file)
}

/// Removes spool files a killed process left between creating and
/// unlinking one: those whose process is gone.
fn remove_stale(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let pid = name
            .to_str()
            .and_then(|name| name.split('-').next())
            .and_then(|pid| pid.parse::<u32>().ok());
        if pid.is_some_and(|pid| !layerfs_home::process::alive(pid)) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// A read that fails once with the input's own error.
struct Failed(Option<std::io::Error>);

impl Read for Failed {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        Err(self
            .0
            .take()
            .unwrap_or_else(|| std::io::Error::other("the input failed")))
    }
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
    let ended = each_element(&mut input, &mut |change| {
        Ok(items.send(Ok(Item::Change(change))).is_ok())
    })?;
    if ended {
        let _ = items.send(Ok(Item::End));
    }
    Ok(())
}

/// Calls `each` with every element of the JSON list in `input`, converted
/// to the Store's form, while it answers true; whether the list ended.
fn each_element(
    input: &mut dyn BufRead,
    each: &mut dyn FnMut(SdkValue) -> Result<bool, SdkRefusal>,
) -> Result<bool, SdkRefusal> {
    let next = |input: &mut dyn BufRead| -> Result<Option<u8>, SdkRefusal> {
        loop {
            let byte = input.fill_buf().map_err(unreadable)?.first().copied();
            match byte {
                Some(b' ' | b'\t' | b'\n' | b'\r') => input.consume(1),
                other => return Ok(other),
            }
        }
    };
    if next(input)? != Some(b'[') {
        return Err(refused(
            "InvalidInput",
            "`changes` must be a list of changes",
        ));
    }
    input.consume(1);
    if next(input)? == Some(b']') {
        input.consume(1);
    } else {
        loop {
            let element = json::read_value(input).map_err(|error| not_json(&error.0))?;
            let change = crate::host_write::store_change(element)?;
            if !each(change)? {
                return Ok(false);
            }
            match next(input)? {
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
    if next(input)?.is_some() {
        return Err(not_json("text after the list"));
    }
    Ok(true)
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
        // held to a frame's decoded budget (§11), refused before effect
        let change = json::read_value_within(&mut input, Some(FRAME_LIMITS));
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
            (Err(error), _) => Err(refused(
                "InvalidInput",
                format!(
                    "streamed input: a spilled change is not valid within the host worker's budget for one change: {}; nothing was committed",
                    error.0
                ),
            )),
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
