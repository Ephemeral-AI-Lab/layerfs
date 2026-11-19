//! A log and its one rotated file, read by cursor (layerfs-project#178):
//! the pair a rotation makes (`calls.jsonl` and `calls.1.jsonl`,
//! `worker.log` and `worker.1.log`), which file a cursor was made in
//! ([`Identity`]), and the two readers over it. [`Backward`] gives whole
//! lines from the end, however long, for `home.log`. [`read`] follows a
//! log a process is still writing, for `machine.log` and `sandbox.log`:
//! one window of at most [`CHUNK`] bytes per call, oldest line first,
//! forward from a cursor or backward from one, with a line longer than
//! the window answered in pieces.
//!
//! Nothing here locks, writes or waits: a reader opens the file, reads
//! one window and closes it.
use crate::outcome::{failed, refused, Outcome};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::os::unix::fs::{FileExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// How much of a file a reader takes at a time.
pub const CHUNK: u64 = 64 << 10;
/// The lines [`read`] answers when no limit is given.
pub const DEFAULT_LINES: i64 = 200;
/// The most lines [`read`] answers.
pub const MAX_LINES: i64 = 1000;
/// How much of a file's start names it.
const HEAD: usize = 4096;

/// A log and the file its rotation renames it to.
#[derive(Clone, Debug)]
pub struct Pair {
    /// The file being written.
    pub current: PathBuf,
    /// Its one rotated file.
    pub rotated: PathBuf,
}

impl Pair {
    /// `current` with the file every log rotates to
    /// (`layerfs_bridge::log_file::rotated`).
    pub fn of(current: PathBuf) -> Self {
        let rotated = layerfs_bridge::log_file::rotated(&current);
        Self { current, rotated }
    }

    /// Its files, newest first: a cursor's `file` indexes them.
    pub fn files(&self) -> [&Path; 2] {
        [&self.current, &self.rotated]
    }
}

/// Which file a cursor was made in: its inode and a hash of its first
/// line, which a rotation (a new file) or a reset (new first line) changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    inode: u64,
    first: u64,
}

impl Identity {
    /// The identity of `file`: its first line is whatever its first 4096
    /// bytes hold before a newline.
    pub fn of(file: &File) -> std::io::Result<Self> {
        let head = head(file)?;
        let first = head.split(|byte| *byte == b'\n').next().unwrap_or(&[]);
        Ok(Self {
            inode: file.metadata()?.ino(),
            first: fnv(first),
        })
    }

    /// The identity of a file a process is still writing: its first line
    /// names it only once it is whole (it ends in a newline, or fills the
    /// first 4096 bytes), since a line still being written hashes
    /// differently each time. Until then `first` is 0 and only the inode
    /// names the file.
    fn settled(file: &File) -> std::io::Result<Self> {
        let head = head(file)?;
        let first = match head.iter().position(|byte| *byte == b'\n') {
            Some(end) => fnv(&head[..end]).max(1),
            None if head.len() == HEAD => fnv(&head).max(1),
            None => 0,
        };
        Ok(Self {
            inode: file.metadata()?.ino(),
            first,
        })
    }

    /// Whether a cursor made with identity `then` still names this file:
    /// the same inode, and the same first line when the cursor knew it.
    fn still(&self, then: &Identity) -> bool {
        self.inode == then.inode && (then.first == 0 || then.first == self.first)
    }
}

/// The first [`HEAD`] bytes of `file`, or all of a shorter one.
fn head(file: &File) -> std::io::Result<Vec<u8>> {
    let mut head = vec![0u8; HEAD];
    let mut filled = 0;
    while filled < head.len() {
        let read = file.read_at(&mut head[filled..], filled as u64)?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    head.truncate(filled);
    Ok(head)
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A place in a pair: byte `offset` of file `file` (0 the current file,
/// 1 the rotated one), as that file was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    /// Which file of the pair.
    pub file: usize,
    /// The byte it names.
    pub offset: u64,
    /// The file it was made in.
    pub identity: Identity,
}

impl Cursor {
    /// The cursor as a call answers it.
    pub fn text(&self) -> String {
        format!(
            "c1.{}.{:x}.{:x}.{:x}",
            self.file, self.identity.inode, self.identity.first, self.offset
        )
    }

    /// The cursor `text` names, if it is one.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('.');
        if parts.next()? != "c1" {
            return None;
        }
        let file = match parts.next()? {
            "0" => 0,
            "1" => 1,
            _ => return None,
        };
        let hex = |part: Option<&str>| part.and_then(|p| u64::from_str_radix(p, 16).ok());
        let inode = hex(parts.next())?;
        let first = hex(parts.next())?;
        let offset = hex(parts.next())?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            file,
            offset,
            identity: Identity { inode, first },
        })
    }

    /// The start of a current file that does not exist yet: whatever file
    /// appears there is read from its first byte.
    const UNBORN: Self = Self {
        file: 0,
        offset: 0,
        identity: Identity { inode: 0, first: 0 },
    };
}

/// The lines of a file before `end`, last first, read in chunks from the
/// end; blank lines are passed over.
pub struct Backward<'f> {
    file: &'f File,
    /// Where the buffered bytes start in the file.
    at: u64,
    /// Bytes from `at` not yet answered.
    buffer: Vec<u8>,
}

impl<'f> Backward<'f> {
    /// The lines of `file` before byte `end`.
    pub fn new(file: &'f File, end: u64) -> Self {
        Self {
            file,
            at: end,
            buffer: Vec::new(),
        }
    }

    /// The next line back and the offset it starts at.
    pub fn next_line(&mut self) -> std::io::Result<Option<(u64, Vec<u8>)>> {
        loop {
            while self.buffer.last() == Some(&b'\n') {
                self.buffer.pop();
            }
            if let Some(newline) = self.buffer.iter().rposition(|byte| *byte == b'\n') {
                let line = self.buffer.split_off(newline + 1);
                return Ok(Some((self.at + newline as u64 + 1, line)));
            }
            if self.at == 0 {
                if self.buffer.is_empty() {
                    return Ok(None);
                }
                return Ok(Some((0, std::mem::take(&mut self.buffer))));
            }
            let take = CHUNK.min(self.at);
            self.at -= take;
            let mut chunk = vec![0u8; take as usize];
            self.file.read_exact_at(&mut chunk, self.at)?;
            chunk.extend_from_slice(&self.buffer);
            self.buffer = chunk;
        }
    }
}

/// One file of a pair, open.
struct Open {
    /// 0 the current file, 1 the rotated one.
    index: usize,
    file: File,
    size: u64,
    identity: Identity,
}

impl Open {
    /// File `index` of `pair`, or `None` when it is not there.
    fn of(pair: &Pair, index: usize) -> Result<Option<Self>, SdkRefusal> {
        // never through a link, and never waiting on a FIFO put in its place
        let opened = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(pair.files()[index]);
        let file = match opened {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(failed(error)),
        };
        let found = file.metadata().map_err(failed)?;
        if !found.is_file() {
            return Err(refused(
                "Failed",
                format!("{} is not a file", pair.files()[index].display()),
            ));
        }
        let size = found.len();
        let identity = Identity::settled(&file).map_err(failed)?;
        Ok(Some(Self {
            index,
            file,
            size,
            identity,
        }))
    }

    /// Bytes `start..end`, which the file holds.
    fn window(&self, start: u64, end: u64) -> Result<Vec<u8>, SdkRefusal> {
        let mut bytes = vec![0u8; (end - start) as usize];
        self.file.read_exact_at(&mut bytes, start).map_err(failed)?;
        Ok(bytes)
    }

    /// Byte `offset` of this file.
    fn at(&self, offset: u64) -> Cursor {
        Cursor {
            file: self.index,
            offset,
            identity: self.identity,
        }
    }

    /// Whether `cursor` names a byte of this file as it is now.
    fn holds(&self, cursor: &Cursor) -> bool {
        self.identity.still(&cursor.identity) && cursor.offset <= self.size
    }
}

/// One line of an answer, or one piece of a line longer than a window.
struct Piece {
    /// Where it starts in its file.
    start: u64,
    bytes: Vec<u8>,
    /// The line goes on in the next piece.
    continued: bool,
}

/// `bytes` (which start at byte `start` of a file) cut at each newline. A
/// last piece with no newline is `continued` when the file goes on past
/// the window (`more`), is the file's last line when the file is finished
/// (`closed`), and is otherwise left out: a line still being written.
/// Answers the pieces and where they end.
fn pieces(bytes: &[u8], start: u64, more: bool, closed: bool) -> (Vec<Piece>, u64) {
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at..].iter().position(|byte| *byte == b'\n') {
            Some(newline) => {
                found.push(Piece {
                    start: start + at as u64,
                    bytes: bytes[at..at + newline].to_vec(),
                    continued: false,
                });
                at += newline + 1;
            }
            None if more || closed => {
                found.push(Piece {
                    start: start + at as u64,
                    bytes: bytes[at..].to_vec(),
                    continued: more,
                });
                at = bytes.len();
            }
            None => break,
        }
    }
    (found, start + at as u64)
}

/// How many of `bytes` come before a UTF-8 character their end cuts: a
/// window never ends inside a character, so the pieces of a line put
/// together are the line.
pub fn whole_characters(bytes: &[u8]) -> usize {
    let len = bytes.len();
    for back in 1..=len.min(3) {
        let byte = bytes[len - back];
        if byte & 0xc0 == 0x80 {
            continue;
        }
        let needs = match byte {
            0xf0.. => 4,
            0xe0.. => 3,
            0xc0.. => 2,
            _ => 1,
        };
        return if needs > back { len - back } else { len };
    }
    len
}

/// `bytes` as one line of text: what is not UTF-8 is U+FFFD, and every
/// control character is written as its escape (`\t`, `\r`, `\u{1b}`).
fn text(bytes: &[u8]) -> String {
    escaped(bytes, &[])
}

/// `bytes` as text a terminal or a page can show as it is: what is not
/// UTF-8 is U+FFFD, and every control character but those of `kept` (a
/// file's line breaks, which a log's lines do not hold) is written as its
/// escape (`\r`, `\u{1b}`).
pub fn escaped(bytes: &[u8], kept: &[char]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for c in String::from_utf8_lossy(bytes).chars() {
        if c.is_control() && !kept.contains(&c) {
            out.extend(c.escape_default());
        } else {
            out.push(c);
        }
    }
    out
}

fn stale(call: &str) -> SdkRefusal {
    refused(
        "InvalidInput",
        format!(
            "{call}: the cursor is stale (the log was rotated or replaced); read again without a cursor"
        ),
    )
}

/// The lines a window of `call` answers at most, or its refusal before
/// any read: `cursor` and `before` both given, or a `limit` outside 1 to
/// [`MAX_LINES`].
pub fn check(
    call: &str,
    cursor: bool,
    before: bool,
    limit: Option<i64>,
) -> Result<usize, SdkRefusal> {
    if cursor && before {
        return Err(refused(
            "InvalidInput",
            format!("{call}: give `cursor` or `before`, not both"),
        ));
    }
    let limit = limit.unwrap_or(DEFAULT_LINES);
    if !(1..=MAX_LINES).contains(&limit) {
        return Err(refused(
            "InvalidInput",
            format!("{call}: `limit` is 1 to {MAX_LINES}"),
        ));
    }
    Ok(limit as usize)
}

/// One window of the log `pair`, oldest line first:
/// `{ lines: [{ text, continued? }], cursor, before?, has_more }`.
///
/// `cursor` is where the answer ends and `before` where it starts; either
/// may be given to a later call. With neither, the answer is the last
/// `limit` lines of the current file (of the rotated one while the current
/// file is empty or absent). With `cursor`, it is the lines after that
/// place, and `has_more` says more are there already. With `before`, it is
/// the lines before that place, back through the current file and then the
/// rotated one. A read with no `cursor` answers `before` while earlier
/// lines remain, and its `has_more` says the same.
///
/// A call reads at most [`CHUNK`] bytes, so it may answer fewer than
/// `limit` lines. A line longer than the window comes in pieces, each but
/// its last with `continued: true`; a read with no `cursor` then starts
/// in the middle of that line, and the pages before it hold its start. The last line of the current file is
/// answered once its newline is written. A cursor whose file was rotated
/// or replaced (another inode or first line, or a size below its offset)
/// is refused `InvalidInput`, named as `call`'s.
pub fn read(
    pair: &Pair,
    call: &str,
    cursor: Option<&str>,
    before: Option<&str>,
    limit: Option<i64>,
) -> Outcome {
    let limit = check(call, cursor.is_some(), before.is_some(), limit)?;
    let parsed = |text: &str| Cursor::parse(text).ok_or_else(|| stale(call));
    match (cursor, before) {
        (Some(text), _) => forward(pair, call, parsed(text)?, limit),
        (None, Some(text)) => backward(pair, call, Some(parsed(text)?), limit),
        (None, None) => backward(pair, call, None, limit),
    }
}

/// The lines after `from`.
fn forward(pair: &Pair, call: &str, from: Cursor, limit: usize) -> Outcome {
    let current = Open::of(pair, 0)?;
    // the rotated file first, when the cursor is in it; at its end, the
    // current file from its start
    let rotated = match from.file {
        1 => match Open::of(pair, 1)? {
            Some(open) if open.holds(&from) => Some(open),
            _ => return Err(stale(call)),
        },
        _ => None,
    };
    let (open, start) = match (&rotated, &current) {
        (Some(open), _) if from.offset < open.size => (open, from.offset),
        (Some(_), Some(open)) => (open, 0),
        (Some(_), None) => return Ok(answer(Vec::new(), &Cursor::UNBORN, None, false)),
        (None, None) if from == Cursor::UNBORN => {
            return Ok(answer(Vec::new(), &from, None, false))
        }
        (None, Some(open)) if from == Cursor::UNBORN => (open, 0),
        (None, Some(open)) if open.holds(&from) => (open, from.offset),
        (None, _) => return Err(stale(call)),
    };
    let mut end = open.size.min(start + CHUNK);
    let mut bytes = open.window(start, end)?;
    if end < open.size {
        bytes.truncate(whole_characters(&bytes));
        end = start + bytes.len() as u64;
    }
    let (mut found, mut next) = pieces(&bytes, start, end < open.size, open.index == 1);
    let cut = found.len() > limit;
    if cut {
        next = found[limit].start;
        found.truncate(limit);
    }
    // the end of the rotated file is the start of the current one
    let follows = current.as_ref().map_or(Cursor::UNBORN, |open| open.at(0));
    let ends = match open.index == 1 && next == open.size {
        true => follows,
        false => open.at(next),
    };
    // more is there already, unless all that is left is an unfinished line
    let beyond = open.index == 1 && current.as_ref().is_some_and(|open| open.size > 0);
    Ok(answer(found, &ends, None, cut || end < open.size || beyond))
}

/// The lines before `from`, or the last ones when there is none.
fn backward(pair: &Pair, call: &str, from: Option<Cursor>, limit: usize) -> Outcome {
    let current = Open::of(pair, 0)?;
    let follows = current.as_ref().map_or(Cursor::UNBORN, |open| open.at(0));
    if from == Some(Cursor::UNBORN) {
        return Ok(answer(Vec::new(), &Cursor::UNBORN, None, false));
    }
    // which file the window is in: the rotated one for a cursor in it, for
    // one at the current file's first byte, and for a tail while the
    // current file holds nothing
    let in_rotated = match (&from, &current) {
        (Some(cursor), _) if cursor.file == 1 => true,
        (Some(cursor), Some(open)) if open.holds(cursor) => cursor.offset == 0,
        (Some(_), _) => return Err(stale(call)),
        (None, open) => open.as_ref().is_none_or(|open| open.size == 0),
    };
    let rotated = match in_rotated {
        true => Open::of(pair, 1)?,
        false => None,
    };
    let open = match in_rotated {
        true => rotated.as_ref(),
        false => current.as_ref(),
    };
    // where the window ends: the cursor's byte, else the file's last
    let within = from
        .as_ref()
        .filter(|cursor| cursor.file == usize::from(in_rotated));
    let echo = from.unwrap_or(follows);
    let Some(open) = open else {
        return match within {
            Some(_) => Err(stale(call)),
            None => Ok(answer(Vec::new(), &echo, None, false)),
        };
    };
    let end = match within {
        Some(cursor) if open.holds(cursor) => cursor.offset,
        Some(_) => return Err(stale(call)),
        None => open.size,
    };
    let mut start = end.saturating_sub(CHUNK);
    let mut bytes = open.window(start, end)?;
    // whether the window's first piece starts a line: its first byte is
    // read only to know, and the window proper starts after it
    let mut whole = true;
    if start > 0 {
        whole = bytes[0] == b'\n';
        // and it starts on a character, as the window before it ends
        let skip = match whole {
            true => 1,
            false => (1..bytes.len().min(5))
                .find(|at| bytes[*at] & 0xc0 != 0x80)
                .unwrap_or(1),
        };
        bytes.drain(..skip);
        start += skip as u64;
    }
    // the rotated file is finished: its last line is whole as it stands
    let (mut found, next) = pieces(&bytes, start, end < open.size, in_rotated);
    if !whole && found.len() > 1 {
        // a line the window's start cuts is left for the page before it;
        // only a line longer than the window is answered from its middle
        found.remove(0);
    }
    if found.len() > limit {
        found.drain(..found.len() - limit);
    }
    let first = found.first().map_or(start, |piece| piece.start);
    // earlier lines: before the first one here, or in the rotated file
    let older = || std::fs::metadata(&pair.rotated).is_ok_and(|rotated| rotated.len() > 0);
    let earlier = first > 0 || (!in_rotated && older());
    let before = earlier.then(|| open.at(first));
    // a tail ends where the current file's unfinished line starts; a read
    // back from a cursor ends at that cursor
    let ends = match (from, in_rotated) {
        (Some(cursor), _) => cursor,
        (None, true) => follows,
        (None, false) => open.at(next),
    };
    Ok(answer(found, &ends, before.as_ref(), earlier))
}

/// The answer of one window.
fn answer(found: Vec<Piece>, cursor: &Cursor, before: Option<&Cursor>, has_more: bool) -> SdkValue {
    let lines = found
        .into_iter()
        .map(|piece| {
            let mut line = vec![("text".to_owned(), SdkValue::Text(text(&piece.bytes)))];
            if piece.continued {
                line.push(("continued".to_owned(), SdkValue::Bool(true)));
            }
            SdkValue::Map(line)
        })
        .collect();
    let mut answer = vec![
        ("lines".to_owned(), SdkValue::List(lines)),
        ("cursor".to_owned(), SdkValue::Text(cursor.text())),
    ];
    if let Some(before) = before {
        answer.push(("before".to_owned(), SdkValue::Text(before.text())));
    }
    answer.push(("has_more".to_owned(), SdkValue::Bool(has_more)));
    SdkValue::Map(answer)
}
