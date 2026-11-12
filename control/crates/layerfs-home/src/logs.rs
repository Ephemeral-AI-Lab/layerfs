//! The call log (#659): `trace/calls.jsonl`, one JSON line per public
//! call, written by the calling process when the call ends. A line is
//! appended with one `write` on a file opened with `O_APPEND` and closed
//! again, so a long-lived process never writes into a rotated or reset
//! file. Past 16 MiB the file rotates once to `calls.1.jsonl`, under a
//! non-blocking `flock` on `trace/calls.lock`; reset takes the same lock,
//! waiting, truncates `calls.jsonl` in place and removes `calls.1.jsonl`.
//! A line that cannot be written is dropped: it changes no outcome and is
//! not retried. `LAYERFS_LOGS=0` turns the log off for the process.
use crate::home::{flock, paths};
use crate::json;
use crate::outcome::{failed, refused, Outcome};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, FileExt, MetadataExt, OpenOptionsExt};
use std::path::Path;

/// The size past which `calls.jsonl` rotates to `calls.1.jsonl`.
pub const ROTATE_AT: u64 = 16 << 20;
/// The page `logs.list` answers when no limit is given.
pub const DEFAULT_LIMIT: i64 = 100;
/// The largest page `logs.list` answers.
pub const MAX_LIMIT: i64 = 1000;
/// The longest tag a line carries, in bytes.
pub const MAX_TAG: usize = 128;

/// How much of a file the reader takes at a time, from its end.
const CHUNK: u64 = 64 << 10;

/// Whether this process writes the call log: on unless `LAYERFS_LOGS=0`.
pub fn enabled() -> bool {
    std::env::var_os("LAYERFS_LOGS").is_none_or(|value| value != "0")
}

/// `tag` when a line may carry it: at most [`MAX_TAG`] bytes, not empty,
/// and no control characters; else none.
pub fn tag(tag: &str) -> Option<String> {
    (!tag.is_empty() && tag.len() <= MAX_TAG && !tag.chars().any(char::is_control))
        .then(|| tag.to_owned())
}

/// Appends `line` to the call log of `home`, best effort. Nothing is
/// written in a folder that is not yet a LayerFS home (no
/// `layerfs.json`), and the home itself is never created here.
pub fn append(home: &Path, line: &SdkValue) {
    if !home.join("layerfs.json").is_file() {
        return;
    }
    let dir = paths::trace_dir(home);
    match fs::DirBuilder::new().mode(0o700).create(&dir) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(_) => return,
    }
    let mut text = json::print(line);
    text.push('\n');
    let file = OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(paths::calls_file(home));
    let Ok(mut file) = file else {
        return;
    };
    // one write: a line is whole or absent, never interleaved
    match file.write(text.as_bytes()) {
        Ok(written) if written == text.len() => {}
        _ => return,
    }
    let size = file.metadata().map_or(0, |meta| meta.len());
    drop(file);
    if size > ROTATE_AT {
        rotate(home);
    }
}

/// Renames `calls.jsonl` to `calls.1.jsonl` when this process gets the
/// lock without waiting; a writer that does not get it skips rotation.
fn rotate(home: &Path) {
    let Ok(lock) = open_lock(home) else {
        return;
    };
    if !matches!(flock::try_lock(&lock, true), Ok(true)) {
        return;
    }
    // another holder may have rotated it since this writer's append
    let current = paths::calls_file(home);
    if fs::metadata(&current).is_ok_and(|meta| meta.len() > ROTATE_AT) {
        let _ = fs::rename(&current, paths::calls_rotated(home));
    }
    flock::unlock(&lock);
}

fn open_lock(home: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(paths::calls_lock(home))
}

/// Clears the call log: truncates `calls.jsonl` in place and removes
/// `calls.1.jsonl` under the lock, and answers how many lines both files
/// held. A line written meanwhile lands before or after the reset.
pub fn reset(home: &Path) -> Result<u64, SdkRefusal> {
    if !paths::trace_dir(home).is_dir() {
        return Ok(0);
    }
    let lock = open_lock(home).map_err(failed)?;
    flock::lock(&lock).map_err(failed)?;
    let cleared = clear(home);
    flock::unlock(&lock);
    cleared
}

fn clear(home: &Path) -> Result<u64, SdkRefusal> {
    let current = paths::calls_file(home);
    let rotated = paths::calls_rotated(home);
    let removed = count(&current)? + count(&rotated)?;
    match OpenOptions::new().write(true).open(&current) {
        Ok(file) => file.set_len(0).map_err(failed)?,
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(failed(error)),
    }
    match fs::remove_file(&rotated) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(failed(error)),
    }
    Ok(removed)
}

/// The lines `path` holds, torn ones included; none when it is absent.
fn count(path: &Path) -> Result<u64, SdkRefusal> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(failed(error)),
    };
    let mut lines = 0;
    for line in BufReader::new(file).split(b'\n') {
        if !line.map_err(failed)?.is_empty() {
            lines += 1;
        }
    }
    Ok(lines)
}

/// Which lines `logs.list` answers; every field given must match.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    /// `family` or `family.call`.
    pub op: Option<String>,
    /// A project the line's `argv` names.
    pub project: Option<String>,
    /// `cli`, `node`, `python` or `rust`.
    pub client: Option<String>,
    /// An exact tag, or a prefix ending in `:`.
    pub tag: Option<String>,
    /// `ok` or a refusal code.
    pub outcome: Option<String>,
    /// Lines whose `at` is at or after this time (ms).
    pub since: Option<i64>,
}

impl Filter {
    fn matches(&self, line: &SdkValue) -> bool {
        let argv: Vec<&str> = match line.get("argv") {
            Some(SdkValue::List(words)) => words
                .iter()
                .map(|word| match word {
                    SdkValue::Text(text) => text.as_str(),
                    _ => "",
                })
                .collect(),
            _ => Vec::new(),
        };
        let same_call = |a: &str, b: &str| a.replace('-', "_") == b.replace('-', "_");
        if let Some(op) = &self.op {
            let (family, call) = match op.split_once('.') {
                Some((family, call)) => (family, Some(call)),
                None => (op.as_str(), None),
            };
            if argv.first() != Some(&family) {
                return false;
            }
            if let Some(call) = call {
                if !argv.get(1).is_some_and(|word| same_call(word, call)) {
                    return false;
                }
            }
        }
        if let Some(project) = &self.project {
            if !argv.iter().skip(2).any(|word| word == project) {
                return false;
            }
        }
        let field = |name: &str, want: &Option<String>| match want {
            Some(want) => line.get_text(name) == Some(want.as_str()),
            None => true,
        };
        if !field("client", &self.client) || !field("outcome", &self.outcome) {
            return false;
        }
        if let Some(want) = &self.tag {
            let Some(tag) = line.get_text("tag") else {
                return false;
            };
            let found = if want.ends_with(':') {
                tag.starts_with(want.as_str())
            } else {
                tag == want
            };
            if !found {
                return false;
            }
        }
        match self.since {
            Some(since) => line.get_int("at").is_some_and(|at| at >= since),
            None => true,
        }
    }
}

/// One page of the call log, newest first across `calls.jsonl` then
/// `calls.1.jsonl`: `{ lines, next?, has_more, skipped }`. `cursor` is a
/// previous page's `next`; after a rotation or reset it is stale and
/// refused `InvalidInput`. A line that does not parse is counted in
/// `skipped`, not answered.
pub fn read(home: &Path, filter: &Filter, cursor: Option<&str>, limit: Option<i64>) -> Outcome {
    let limit = limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(refused(
            "InvalidInput",
            format!("logs.list: `limit` is 1 to {MAX_LIMIT}"),
        ));
    }
    let limit = limit as usize;
    let files = [paths::calls_file(home), paths::calls_rotated(home)];
    let start = match cursor {
        Some(text) => Some(Cursor::parse(text).ok_or_else(stale)?),
        None => None,
    };
    let mut lines = Vec::new();
    let mut skipped = 0i64;
    let mut last: Option<Cursor> = None;
    let mut has_more = false;
    let first = start.as_ref().map_or(0, |cursor| cursor.file);
    'files: for (index, path) in files.iter().enumerate().skip(first) {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if start.as_ref().is_some_and(|c| c.file == index) {
                    return Err(stale());
                }
                continue;
            }
            Err(error) => return Err(failed(error)),
        };
        let identity = Identity::of(&file).map_err(failed)?;
        let size = file.metadata().map_err(failed)?.len();
        let end = match &start {
            Some(cursor) if cursor.file == index => {
                if cursor.identity != identity || cursor.offset > size {
                    return Err(stale());
                }
                cursor.offset
            }
            _ => size,
        };
        let mut reader = Backward::new(&file, end);
        while let Some((offset, bytes)) = reader.next_line().map_err(failed)? {
            let Some(line) = parse(&bytes) else {
                skipped += 1;
                continue;
            };
            if !filter.matches(&line) {
                continue;
            }
            if lines.len() == limit {
                has_more = true;
                break 'files;
            }
            lines.push(line);
            last = Some(Cursor {
                file: index,
                offset,
                identity,
            });
        }
    }
    let mut answer = vec![("lines".to_owned(), SdkValue::List(lines))];
    if let (true, Some(last)) = (has_more, last) {
        answer.push(("next".to_owned(), SdkValue::Text(last.text())));
    }
    answer.push(("has_more".to_owned(), SdkValue::Bool(has_more)));
    answer.push(("skipped".to_owned(), SdkValue::Int(skipped)));
    Ok(SdkValue::Map(answer))
}

fn stale() -> SdkRefusal {
    refused(
        "InvalidInput",
        "logs.list: the cursor is stale (the call log was rotated or reset); list from the start",
    )
}

/// One whole line as its JSON map; anything else is a torn or foreign line.
fn parse(bytes: &[u8]) -> Option<SdkValue> {
    let text = std::str::from_utf8(bytes).ok()?;
    match json::parse(text).ok()? {
        line @ SdkValue::Map(_) => Some(line),
        _ => None,
    }
}

/// Which file a cursor was made in: its inode and a hash of its first
/// line, which a rotation (a new file) or a reset (new first line) changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity {
    inode: u64,
    first: u64,
}

impl Identity {
    fn of(file: &File) -> std::io::Result<Self> {
        let mut head = vec![0u8; 4096];
        let mut filled = 0;
        while filled < head.len() {
            let read = file.read_at(&mut head[filled..], filled as u64)?;
            if read == 0 {
                break;
            }
            filled += read;
        }
        let first = head[..filled]
            .split(|byte| *byte == b'\n')
            .next()
            .unwrap_or(&[]);
        Ok(Self {
            inode: file.metadata()?.ino(),
            first: fnv(first),
        })
    }
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Where the next page ends: before the line at `offset` of file `file`
/// (0 `calls.jsonl`, 1 `calls.1.jsonl`), as that file was.
struct Cursor {
    file: usize,
    offset: u64,
    identity: Identity,
}

impl Cursor {
    fn text(&self) -> String {
        format!(
            "c1.{}.{:x}.{:x}.{:x}",
            self.file, self.identity.inode, self.identity.first, self.offset
        )
    }

    fn parse(text: &str) -> Option<Self> {
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
}

/// The lines of a file before `end`, last first, read in chunks from the
/// end; blank lines are passed over.
struct Backward<'f> {
    file: &'f File,
    /// Where the buffered bytes start in the file.
    at: u64,
    /// Bytes from `at` not yet answered.
    buffer: Vec<u8>,
}

impl<'f> Backward<'f> {
    fn new(file: &'f File, end: u64) -> Self {
        Self {
            file,
            at: end,
            buffer: Vec::new(),
        }
    }

    /// The next line back and the offset it starts at.
    fn next_line(&mut self) -> std::io::Result<Option<(u64, Vec<u8>)>> {
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
