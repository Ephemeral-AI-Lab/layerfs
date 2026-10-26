//! Bounded text scanning at immutable roots: a file's text arrives in whole
//! blocks, checked as before, and a buffer-wide line scanner splits it.
use crate::history::lookup as args;
use layerfs_bridge::control::SdkRefusal;
use layerfs_content::{
    compare::EntryFacts, file::FileView, object::inode_leaf::InodeKind, AuthenticatedObjects,
};

pub use super::walk::{walk, Walker};

pub const BYTES: usize = 48 * 1024;
pub const LINES: usize = 2000;

/// The window a NUL or invalid UTF-8 is found in: the scan stops at the same
/// window boundary as the window-at-a-time scan it replaced (#352 F-035).
const WINDOW: u64 = 16 * 1024;
/// Bytes of text handed on at once from a chunked file's read pass.
const BLOCK: u64 = 16 * WINDOW;

/// One logical LF-delimited line. A final LF belongs to this line, not a new one.
#[derive(Clone)]
pub struct Line {
    pub text: String,
    pub bytes: u64,
    pub truncated: bool,
    pub matched: bool,
}

/// The line being scanned: its number, shown prefix and length so far.
pub(super) struct Open {
    pub number: u64,
    head: Vec<u8>,
    pub length: usize,
}
impl Open {
    pub fn new() -> Self {
        Self {
            number: 1,
            head: Vec::new(),
            length: 0,
        }
    }
    /// Appends a part of the line, keeping its first `prefix` bytes.
    pub fn take(&mut self, part: &[u8], prefix: usize) {
        let room = prefix.saturating_sub(self.head.len()).min(part.len());
        self.head.extend_from_slice(&part[..room]);
        self.length += part.len();
    }
    /// Forgets the line's parts: it ended unreported.
    pub fn clear(&mut self) {
        self.head.clear();
        self.length = 0;
    }
    /// The finished line; the next one starts empty.
    pub fn finish(&mut self, matched: bool) -> Line {
        let line = line(&mut self.head, self.length, matched);
        self.length = 0;
        line
    }
}

/// LF bytes in `bytes`, counted over the whole slice at once.
pub(super) fn newlines(bytes: &[u8]) -> u64 {
    bytes.iter().filter(|&&byte| byte == b'\n').count() as u64
}

/// The position just after the last LF in `bytes`, if any.
pub(super) fn after_last_newline(bytes: &[u8]) -> Option<usize> {
    bytes.iter().rposition(|&byte| byte == b'\n').map(|i| i + 1)
}

/// The position after the `count`th LF in `bytes`, or `None` with the LFs it
/// holds when it has fewer.
fn skip(bytes: &[u8], count: u64) -> Result<usize, u64> {
    let (mut seen, mut done) = (0, 0);
    for part in bytes.chunks(4096) {
        let here = newlines(part);
        if seen + here < count {
            seen += here;
            done += part.len();
            continue;
        }
        for (i, &byte) in part.iter().enumerate() {
            if byte == b'\n' {
                seen += 1;
                if seen == count {
                    return Ok(done + i + 1);
                }
            }
        }
    }
    Err(seen)
}

/// Calls `visit` with the file's text, in order, as whole valid UTF-8 parts.
/// A NUL or invalid UTF-8 ends the text at the start of the window it is
/// found in, as the window-at-a-time scan did, and the scan then fails
/// `Binary` unless `visit` stopped it first. A part may end inside a line.
///
/// A chunked file is read in one pass, so each chunk is decoded once and
/// the mapping walked once; its text is handed on in parts of about
/// [`BLOCK`] bytes as the pass delivers it, and stopping ends the pass.
///
/// `view` is the file's root when the caller read it already, as a batch.
pub(super) fn texts(
    reader: &dyn AuthenticatedObjects,
    entry: &EntryFacts,
    view: Option<FileView>,
    mut visit: impl FnMut(&str) -> Result<bool, SdkRefusal>,
) -> Result<(), SdkRefusal> {
    if entry.value.kind != InodeKind::RegularFile {
        return Err(args::refuse(
            if entry.value.kind == InodeKind::Directory {
                "IsDirectory"
            } else {
                "NotFile"
            },
            "expected a regular file",
            layerfs_bridge::control::SdkValue::Null,
        ));
    }
    // The root is read and classified once.
    let view = match view {
        Some(view) => view,
        None => crate::telemetry::timed("sdk.lines", |timing| {
            FileView::open(
                reader,
                entry.value.content_root,
                timing.child("content.read"),
            )
        })
        .map_err(args::content)?,
    };
    let size = view.logical_len();
    // A whole-file payload is already in memory: one part, nothing copied.
    if let Some(payload) = view.whole_file_bytes().map_err(args::content)? {
        let (text, bad) = checked(payload, 0, size, true);
        return if !visit(text)? || !bad {
            Ok(())
        } else {
            Err(binary())
        };
    }
    let mut feed = Feed {
        pending: Vec::new(),
        from: 0,
        size,
        visit: &mut visit,
        end: None,
    };
    // The pass's time includes the scan of what it delivers.
    let read = crate::telemetry::timed("sdk.lines", |timing| {
        timing
            .child("content.read")
            .run(|scope| view.read_range(reader, 0..size, &mut feed, scope))
    });
    if let Some(end) = feed.end.take() {
        return end;
    }
    read.map_err(args::content)?;
    feed.deliver(true);
    feed.end.unwrap_or(Ok(()))
}

/// The sink of a file's single read pass: bytes not yet handed on, from file
/// offset `from`, and how the scan ended when it ended early.
struct Feed<'v> {
    pending: Vec<u8>,
    from: u64,
    size: u64,
    visit: &'v mut dyn FnMut(&str) -> Result<bool, SdkRefusal>,
    end: Option<Result<(), SdkRefusal>>,
}

impl Feed<'_> {
    /// Hands on the checked text; false, with `end` set, when the scan ends.
    fn deliver(&mut self, last: bool) -> bool {
        let (text, bad) = checked(&self.pending, self.from, self.size, last);
        let length = text.len();
        match (self.visit)(text) {
            Err(refusal) => self.end = Some(Err(refusal)),
            Ok(false) => self.end = Some(Ok(())),
            Ok(true) if bad => self.end = Some(Err(binary())),
            Ok(true) => {
                self.pending.drain(..length);
                self.from += length as u64;
                return true;
            }
        }
        false
    }
}

impl std::io::Write for Feed<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.pending.extend_from_slice(bytes);
        if self.pending.len() as u64 >= BLOCK && !self.deliver(false) {
            // Ends the read pass; `end` says why.
            return Err(std::io::Error::other("scan ended"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The text of `bytes`, which start at file offset `from` on a character
/// boundary, that may be handed on, and whether a NUL or invalid UTF-8 ends
/// the scan after it. `last` when `bytes` reach the file's end.
///
/// The window-at-a-time scan failed in the first window holding a NUL, or
/// whose text so far was decisively invalid UTF-8, or, for a character left
/// incomplete, the file's last window; the text ends where that window
/// starts. Otherwise text is handed on only through the last whole window,
/// since a NUL later in a window fails it.
fn checked(bytes: &[u8], from: u64, size: u64, last: bool) -> (&str, bool) {
    let window = |at: usize| (from + at as u64) / WINDOW;
    let start = |w: u64| (w * WINDOW).saturating_sub(from) as usize;
    // `contains` searches a whole slice at once; most text has no NUL.
    let mut bad = bytes
        .contains(&0)
        .then(|| bytes.iter().position(|&byte| byte == 0))
        .flatten()
        .map(window);
    let (text, valid) = match std::str::from_utf8(bytes) {
        Ok(text) => (Some(text), bytes.len()),
        Err(error) => {
            let at = error.valid_up_to();
            let found = match error.error_len() {
                None if !last => None,
                None => Some((size - 1) / WINDOW),
                // Decided in the window holding the sequence's start, or else
                // by its next byte, the first of the next window.
                Some(_) => {
                    let w = window(at);
                    let end = start(w + 1).min(bytes.len());
                    match std::str::from_utf8(&bytes[at..end]) {
                        Err(error) if error.error_len().is_some() => Some(w),
                        _ => Some(w + 1),
                    }
                }
            };
            if let Some(found) = found {
                bad = Some(bad.map_or(found, |nul| nul.min(found)));
            }
            (None, at)
        }
    };
    let end = match bad {
        Some(w) => start(w),
        None if last => bytes.len(),
        None => start(window(bytes.len())),
    }
    .min(valid);
    // The text stops at a character boundary; what lies past it up to the
    // failing window holds no LF, so no reported line is lost.
    let text = match text {
        Some(text) => {
            let mut end = end;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            &text[..end]
        }
        None => match std::str::from_utf8(&bytes[..end]) {
            Ok(text) => text,
            Err(error) => std::str::from_utf8(&bytes[..error.valid_up_to()]).unwrap_or_default(),
        },
    };
    (text, bad.is_some())
}

/// Scans with bounded buffers, including huge lines: every line from line
/// `from` on, in order, until `visit` stops. Earlier lines are only counted.
pub fn lines(
    reader: &dyn AuthenticatedObjects,
    entry: &EntryFacts,
    prefix: usize,
    from: u64,
    mut visit: impl FnMut(u64, Line) -> Result<bool, SdkRefusal>,
) -> Result<(), SdkRefusal> {
    let (mut open, mut stopped) = (Open::new(), false);
    texts(reader, entry, None, |text| {
        let bytes = text.as_bytes();
        let mut at = 0;
        while at < bytes.len() {
            if open.number < from {
                match skip(&bytes[at..], from - open.number) {
                    Ok(next) => {
                        open.number = from;
                        at += next;
                    }
                    Err(seen) => {
                        open.number += seen;
                        at = bytes.len();
                    }
                }
                continue;
            }
            match text[at..].find('\n') {
                Some(i) => {
                    open.take(&bytes[at..at + i + 1], prefix);
                    if !visit(open.number, open.finish(true))? {
                        stopped = true;
                        return Ok(false);
                    }
                    open.number += 1;
                    at += i + 1;
                }
                None => {
                    open.take(&bytes[at..], prefix);
                    at = bytes.len();
                }
            }
        }
        Ok(true)
    })?;
    if !stopped && open.length > 0 {
        visit(open.number, open.finish(true))?;
    }
    Ok(())
}

pub(super) fn binary() -> SdkRefusal {
    args::refuse(
        "Binary",
        "text contains NUL or invalid UTF-8",
        layerfs_bridge::control::SdkValue::Null,
    )
}

fn line(bytes: &mut Vec<u8>, length: usize, matched: bool) -> Line {
    let mut shown = std::mem::take(bytes);
    let end = match std::str::from_utf8(&shown) {
        Ok(_) => shown.len(),
        Err(e) => e.valid_up_to(),
    };
    shown.truncate(end);
    Line {
        bytes: length as u64,
        text: String::from_utf8(shown).expect("validated prefix"),
        truncated: end < length,
        matched,
    }
}
