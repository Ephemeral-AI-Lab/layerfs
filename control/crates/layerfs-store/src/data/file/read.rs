//! `file.read`, and what the `file` family's reads share: each one `fs.*` Store call at a Branch's
//! head or at the Commit `--at` names (an id or a prefix of 8 or more of
//! its characters, which the Store resolves). Answers carry the Commit
//! read as its full id. A paged read's cursor carries its filters and that
//! Commit, so a later page reads the same tree.
use super::requests::FileRead;
use super::FileOp;
use crate::admin::route::methods::{next_fields, on_project, Args, StoreOp};
use layerfs_home::reply::cursor::{self, Carried};
use layerfs_home::reply::events::Events;
use layerfs_home::reply::outcome::{refused, Outcome};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

/// The most lines one window of `file.read` or `file.blame` may ask for.
pub const MAX_LINES: i64 = 2000;
/// The largest line number the Store takes: 2^53 - 1.
pub(super) const MAX_SAFE: i64 = 9_007_199_254_740_991;

/// Runs the Store method `method` with `args` on `project`.
pub(super) fn run(
    home: &Path,
    events: &mut Events<'_>,
    project: &str,
    method: &'static str,
    args: Args,
) -> Outcome {
    let op = FileOp {
        method,
        args: args.0,
    };
    on_project(home, events, project, StoreOp::File(&op)).map_err(next_fields)
}

/// The value under `key`, null when absent.
pub(super) fn take(value: &SdkValue, key: &str) -> SdkValue {
    value.get(key).cloned().unwrap_or_default()
}

/// The full id of the Commit a read answered at.
pub(super) fn pin(value: &SdkValue) -> SdkValue {
    value.get("at").map_or(SdkValue::Null, |at| take(at, "id"))
}

/// A time in milliseconds as a date; null when there is none.
pub(super) fn date(ms: Option<&SdkValue>) -> SdkValue {
    match ms {
        Some(SdkValue::Int(ms)) => SdkValue::text(layerfs_home::reply::time::iso(*ms)),
        _ => SdkValue::Null,
    }
}

pub(super) fn text(value: &Option<String>) -> SdkValue {
    value.clone().map_or(SdkValue::Null, SdkValue::Text)
}

/// The first and last line of a window of `limit` lines from `line`.
pub(super) fn window(
    call: &str,
    line: Option<i64>,
    limit: Option<i64>,
) -> Result<(i64, i64), SdkRefusal> {
    let (line, limit) = (line.unwrap_or(1), limit.unwrap_or(MAX_LINES));
    if line < 1 {
        return Err(refused(
            "InvalidInput",
            format!("{call}: `line` is 1-based"),
        ));
    }
    if !(1..=MAX_LINES).contains(&limit) {
        return Err(refused(
            "InvalidInput",
            format!("{call}: `limit` is 1 to {MAX_LINES}"),
        ));
    }
    // the last line stays a safe integer: a window that reaches past it
    // ends where the file does anyway
    Ok((line, line.saturating_add(limit - 1).min(MAX_SAFE).max(line)))
}

/// The items of an answer, each made by `each`.
pub(super) fn items(value: &SdkValue, each: impl Fn(&SdkValue) -> SdkValue) -> SdkValue {
    match value.get("items") {
        Some(SdkValue::List(items)) => SdkValue::List(items.iter().map(each).collect()),
        _ => SdkValue::List(Vec::new()),
    }
}

/// One page of a paged read: its filters as given on the first page (a
/// later page's are the cursor's), the Commit the first page read, where
/// this page starts and its size.
pub(super) struct Page {
    pub(super) call: &'static str,
    pub(super) filters: Vec<(&'static str, SdkValue)>,
    pub(super) carried: Option<Carried>,
    pub(super) limit: i64,
}

impl Page {
    /// The page `cursor` asks for, or the first one; a cursor given with
    /// other filters, or of another call, is refused `InvalidInput`.
    pub(super) fn of(
        call: &'static str,
        given: Vec<(&'static str, SdkValue)>,
        cursor: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Page, SdkRefusal> {
        let limit = cursor::limit(call, limit, cursor::DEFAULT_LIMIT)?;
        let carried = cursor
            .map(|made| cursor::resume(call, made, &given))
            .transpose()?;
        let filters = match &carried {
            Some(carried) => (given.iter())
                .map(|(name, _)| (*name, carried.get(name)))
                .collect(),
            None => given,
        };
        Ok(Page {
            call,
            filters,
            carried,
            limit,
        })
    }

    /// The filter `name` this page reads with.
    pub(super) fn text(&self, name: &str) -> Option<String> {
        (self.filters.iter())
            .find(|(held, _)| *held == name)
            .and_then(|(_, value)| own_text(value).map(str::to_owned))
    }

    /// The Commit this page reads: the first page's, else `--at` as given.
    pub(super) fn pin(&self) -> Option<String> {
        match &self.carried {
            Some(carried) => carried.text("pin").map(str::to_owned),
            None => self.text("given_at"),
        }
    }

    /// Where this page starts, as the cursor carries it.
    pub(super) fn place(&self) -> SdkValue {
        self.carried
            .as_ref()
            .map_or(SdkValue::Null, |carried| carried.place().clone())
    }

    /// The cursor of the next page, which starts at `place`; null when
    /// there is no next page. `read` is the Commit this page read.
    pub(super) fn next(&self, place: SdkValue, read: SdkValue) -> SdkValue {
        if place == SdkValue::Null {
            return SdkValue::Null;
        }
        let pin = match &self.carried {
            Some(carried) => carried.get("pin"),
            None => read,
        };
        let mut fields = self.filters.clone();
        fields.push(("pin", pin));
        SdkValue::Text(cursor::make(self.call, place, &fields))
    }
}

/// A text value's own text.
pub(super) fn own_text(value: &SdkValue) -> Option<&str> {
    match value {
        SdkValue::Text(text) => Some(text),
        _ => None,
    }
}

/// `file.read`: a window of a text file's lines; a file that is not text
/// answers `binary` and its `size`.
pub fn read(home: &Path, events: &mut Events<'_>, request: &FileRead) -> Outcome {
    let (start, end) = window("file.read", request.line, request.limit)?;
    let mut args = Args::default();
    args.at(&request.branch, request.at.as_deref())
        .text("path", &request.path)
        .int("startLine", Some(start))
        .int("endLine", Some(end))
        .boolean("binary", Some(true));
    let value = run(home, events, &request.project, "fs.read", args)?;
    let binary = value.get_bool("binary");
    let unless_binary = |key: &str| match binary {
        true => SdkValue::Null,
        false => take(&value, key),
    };
    Ok(SdkValue::map([
        ("text", unless_binary("text")),
        ("next_line", take(&value, "nextLine")),
        (
            "line_truncated",
            SdkValue::Bool(value.get_bool("lineTruncated")),
        ),
        ("binary", SdkValue::Bool(binary)),
        ("size", take(&value, "size")),
        ("commit", pin(&value)),
        ("root", take(&value, "root")),
        ("path", take(&value, "path")),
        ("start_line", unless_binary("startLine")),
        ("end_line", take(&value, "endLine")),
    ]))
}

/// A permission mode as octal text, `0644`.
pub(super) fn octal(mode: &SdkValue) -> SdkValue {
    match mode {
        SdkValue::Int(mode) => SdkValue::text(format!("{:04o}", mode & 0o7777)),
        _ => SdkValue::Null,
    }
}
