//! The call log (#659): `trace/calls.jsonl`, one JSON line per public
//! call, written by the calling process when the call ends. A line is
//! appended with one `write` on a file opened with `O_APPEND` and closed
//! again, so a long-lived process never writes into a rotated or reset
//! file. Past 16 MiB the file rotates once to `calls.1.jsonl`, under a
//! non-blocking `flock` on `trace/calls.lock`; reset takes the same lock,
//! waiting, truncates `calls.jsonl` in place and removes `calls.1.jsonl`.
//! A line that cannot be written is dropped: it changes no outcome and is
//! not retried. `LAYERFS_LOGS=0` turns the log off for the process.
//!
//! `home.log` reads the two files by cursor with the pair reader of
//! [`crate::tail`], which the log calls of a machine and a Sandbox share,
//! and answers each stored line as a row of the call surface (2.0): the
//! stored format, `{ v, at, ms, client, argv, outcome, kind?, commit?,
//! layer?, tag? }`, does not change.
//!
//! The lines of `log/*.jsonl` rotate the same way ([`rotate`], under
//! `log/rotate.lock`; layerfs-project#180), all but `log/uncertain.jsonl`.
use crate::home::{flock, paths};
use crate::json;
use crate::outcome::{failed, public_code, refused, Outcome};
use crate::tail::{Backward, Cursor, Identity, Pair};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::log_file;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;

/// The size past which `calls.jsonl` rotates to `calls.1.jsonl`, as every
/// log does.
pub const ROTATE_AT: u64 = log_file::ROTATE_AT;
/// The page `home.log` answers when no limit is given.
pub const DEFAULT_LIMIT: i64 = 200;
/// The largest page `home.log` answers.
pub const MAX_LIMIT: i64 = 1000;
/// The size past which a page ends early with a cursor, in bytes of its
/// rows as JSON text.
pub const PAGE_BYTES: usize = 64 << 10;
/// The longest tag a line carries, in bytes.
pub const MAX_TAG: usize = 128;

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
        rotate(&paths::calls_lock(home), &paths::calls_file(home));
    }
}

/// Renames the log `file` to its `.1` (`calls.jsonl` to `calls.1.jsonl`)
/// when it is past [`ROTATE_AT`] and this process gets the lock file
/// `lock` without waiting; a writer that does not get it skips rotation.
/// For logs whose writers open, append one line and close.
pub(crate) fn rotate(lock: &Path, file: &Path) {
    let Ok(lock) = open_lock(lock) else {
        return;
    };
    if !matches!(flock::try_lock(&lock, true), Ok(true)) {
        return;
    }
    // another holder may have rotated it since this writer's append
    let _ = log_file::rotate(file, ROTATE_AT);
    flock::unlock(&lock);
}

fn open_lock(lock: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(lock)
}

/// Clears the call log: truncates `calls.jsonl` in place and removes
/// `calls.1.jsonl` under the lock, and answers how many lines both files
/// held. A line written meanwhile lands before or after the reset.
pub fn reset(home: &Path) -> Result<u64, SdkRefusal> {
    if !paths::trace_dir(home).is_dir() {
        return Ok(0);
    }
    let lock = open_lock(&paths::calls_lock(home)).map_err(failed)?;
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

/// Which lines `home.log` answers; every field given must match.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    /// `family` or `family.call`.
    pub call: Option<String>,
    /// A project the line's `argv` names.
    pub project: Option<String>,
    /// `cli`, `node`, `python` or `rust`.
    pub client: Option<String>,
    /// An exact tag, or a prefix ending in `:`.
    pub tag: Option<String>,
    /// An outcome word (`ok`, `committed`, `refused`) or a refusal code.
    pub outcome: Option<String>,
    /// Lines whose `at` is at or after this time (ms).
    pub since: Option<i64>,
}

/// The call a cursor of this reader belongs to; one given to another call,
/// or another call's given here, is refused.
const CURSOR_FOR: &str = "home.log";

/// The filter's text fields with their names, in one order for the cursor.
const TEXT_FILTERS: [&str; 5] = ["call", "project", "client", "tag", "outcome"];

impl Filter {
    fn text(&mut self, name: &str) -> &mut Option<String> {
        match name {
            "call" => &mut self.call,
            "project" => &mut self.project,
            "client" => &mut self.client,
            "tag" => &mut self.tag,
            _ => &mut self.outcome,
        }
    }

    /// Whether the stored `line`, answered as `row`, is one of these.
    fn matches(&self, line: &SdkValue, row: &SdkValue) -> bool {
        let argv = words(line);
        let same_call = |a: &str, b: &str| a.replace('-', "_") == b.replace('-', "_");
        if let Some(named) = &self.call {
            let (family, call) = match named.split_once('.') {
                Some((family, call)) => (family, Some(call)),
                None => (named.as_str(), None),
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
        if let Some(client) = &self.client {
            if line.get_text("client") != Some(client.as_str()) {
                return false;
            }
        }
        if let Some(outcome) = &self.outcome {
            let want = Some(outcome.as_str());
            if row.get_text("outcome") != want && row.get_text("error") != want {
                return false;
            }
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

    /// The filters as a cursor carries them, in one order.
    fn fields(&self) -> Vec<(&'static str, SdkValue)> {
        let mut this = self.clone();
        let mut fields: Vec<(&'static str, SdkValue)> = (TEXT_FILTERS.iter())
            .map(|name| {
                (
                    *name,
                    this.text(name)
                        .take()
                        .map_or(SdkValue::Null, SdkValue::Text),
                )
            })
            .collect();
        fields.push(("since", self.since.map_or(SdkValue::Null, SdkValue::Int)));
        fields
    }

    /// The `next_cursor` of a page that ended at `last`: the place and
    /// these filters, as one opaque word ([`crate::cursor`]).
    fn cursor(&self, last: &Cursor) -> String {
        crate::cursor::make(CURSOR_FOR, SdkValue::Text(last.text()), &self.fields())
    }

    /// The place `cursor` names and the filters of the call that made it,
    /// which this call continues: a filter given again must be the same.
    fn resume(&self, cursor: &str) -> Result<(Cursor, Filter), SdkRefusal> {
        let made = crate::cursor::resume(CURSOR_FOR, cursor, &self.fields())?;
        let place = match made.place() {
            SdkValue::Text(text) => Cursor::parse(text),
            _ => None,
        }
        .ok_or_else(stale)?;
        let since = match made.get("since") {
            SdkValue::Int(since) => Some(since),
            _ => None,
        };
        let mut carried = Filter {
            since,
            ..Filter::default()
        };
        for name in TEXT_FILTERS {
            *carried.text(name) = made.text(name).map(str::to_owned);
        }
        Ok((place, carried))
    }
}

/// A stored line's `argv` words.
fn words(line: &SdkValue) -> Vec<&str> {
    match line.get("argv") {
        Some(SdkValue::List(words)) => words
            .iter()
            .map(|word| match word {
                SdkValue::Text(text) => text.as_str(),
                _ => "",
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// `UpToDate` as `up_to_date`: a stored result word as an outcome word.
fn outcome_word(kind: &str) -> String {
    let mut word = String::with_capacity(kind.len() + 4);
    for (at, c) in kind.chars().enumerate() {
        if c.is_ascii_uppercase() && at > 0 {
            word.push('_');
        }
        word.push(c.to_ascii_lowercase());
    }
    word
}

/// One stored line as `home.log` answers it: `{ call, outcome, logged_at,
/// project, error, duration_ms, client, argv, commit, tag }`. `call` and
/// `project` come from the line's `argv`; a refused call's `outcome` is
/// `refused` and its code, folded to a public one, is `error`; any other
/// call's `outcome` is its result's word, `ok` when it had none.
fn row(line: &SdkValue) -> SdkValue {
    let argv = words(line);
    let call = match (argv.first(), argv.get(1)) {
        (Some(family), Some(call)) => {
            SdkValue::Text(format!("{family}.{}", call.replace('-', "_")))
        }
        (Some(family), None) => SdkValue::text(*family),
        _ => SdkValue::Null,
    };
    let project = argv
        .iter()
        .skip(2)
        .find(|word| !word.starts_with('-') && crate::names::project(word).is_ok());
    let (outcome, error) = match line.get_text("outcome") {
        Some("ok") | None => (
            line.get_text("kind").map_or("ok".to_owned(), outcome_word),
            SdkValue::Null,
        ),
        Some(code) => ("refused".to_owned(), SdkValue::text(public_code(code))),
    };
    let copied = |name: &str| line.get(name).cloned().unwrap_or_default();
    SdkValue::map([
        ("call", call),
        ("outcome", SdkValue::Text(outcome)),
        (
            "logged_at",
            line.get_int("at")
                .map_or(SdkValue::Null, |at| SdkValue::Text(crate::time::iso(at))),
        ),
        ("project", SdkValue::opt_text(project.copied())),
        ("error", error),
        ("duration_ms", copied("ms")),
        ("client", copied("client")),
        ("argv", copied("argv")),
        ("commit", copied("commit")),
        ("tag", copied("tag")),
    ])
}

/// One page of the call log, newest first across `calls.jsonl` then
/// `calls.1.jsonl`: `{ items, next_cursor, skipped }`, `next_cursor` null at
/// the end. `cursor` is a previous page's `next_cursor`: it carries that
/// call's filters, and after a rotation or reset it is stale and refused
/// `InvalidInput`. A page ends early past [`PAGE_BYTES`]. A line that does
/// not parse is counted in `skipped`, not answered.
pub fn page(home: &Path, filter: &Filter, cursor: Option<&str>, limit: Option<i64>) -> Outcome {
    let limit = limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(refused(
            "InvalidInput",
            format!("home.log: `limit` is 1 to {MAX_LIMIT}"),
        ));
    }
    let limit = limit as usize;
    let pair = Pair {
        current: paths::calls_file(home),
        rotated: paths::calls_rotated(home),
    };
    let (start, filter) = match cursor {
        Some(text) => {
            let (place, carried) = filter.resume(text)?;
            (Some(place), carried)
        }
        None => (None, filter.clone()),
    };
    let mut items = Vec::new();
    let mut bytes = 0usize;
    let mut skipped = 0i64;
    let mut last: Option<Cursor> = None;
    let mut more = false;
    let first = start.as_ref().map_or(0, |cursor| cursor.file);
    'files: for (index, path) in pair.files().into_iter().enumerate().skip(first) {
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
        while let Some((offset, raw)) = reader.next_line().map_err(failed)? {
            let Some(line) = parse(&raw) else {
                skipped += 1;
                continue;
            };
            let row = row(&line);
            if !filter.matches(&line, &row) {
                continue;
            }
            if items.len() == limit || bytes > PAGE_BYTES {
                more = true;
                break 'files;
            }
            bytes += raw.len();
            items.push(row);
            last = Some(Cursor {
                file: index,
                offset,
                identity,
            });
        }
    }
    let next = match (more, last) {
        (true, Some(last)) => SdkValue::Text(filter.cursor(&last)),
        _ => SdkValue::Null,
    };
    Ok(SdkValue::map([
        ("items", SdkValue::List(items)),
        ("next_cursor", next),
        ("skipped", SdkValue::Int(skipped)),
    ]))
}

fn stale() -> SdkRefusal {
    refused(
        "InvalidInput",
        "home.log: the cursor is stale (the call log was rotated or cleared); read from the start",
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
