//! The `file` family's reads: each one `fs.*` Store call at a Branch's
//! head or at the Commit `--at` names (an id or a prefix of 8 or more of
//! its characters, which the Store resolves). Answers carry the Commit
//! read as its full id. A paged read's cursor carries its filters and that
//! Commit, so a later page reads the same tree.
use super::requests::{FileBlame, FileHistory, FileList, FileRead, FileSearch, FileStat};
use super::FileOp;
use crate::calls::op::{next_fields, on_project, Args, StoreOp};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::cursor::{self, Carried};
use layerfs_home::events::Events;
use layerfs_home::outcome::{refused, Outcome};
use std::path::Path;

/// The most lines one window of `file.read` or `file.blame` may ask for.
pub const MAX_LINES: i64 = 2000;
/// The largest line number the Store takes: 2^53 - 1.
const MAX_SAFE: i64 = 9_007_199_254_740_991;

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
fn pin(value: &SdkValue) -> SdkValue {
    value.get("at").map_or(SdkValue::Null, |at| take(at, "id"))
}

/// A time in milliseconds as a date; null when there is none.
pub(super) fn date(ms: Option<&SdkValue>) -> SdkValue {
    match ms {
        Some(SdkValue::Int(ms)) => SdkValue::text(layerfs_home::time::iso(*ms)),
        _ => SdkValue::Null,
    }
}

fn text(value: &Option<String>) -> SdkValue {
    value.clone().map_or(SdkValue::Null, SdkValue::Text)
}

/// The first and last line of a window of `limit` lines from `line`.
fn window(call: &str, line: Option<i64>, limit: Option<i64>) -> Result<(i64, i64), SdkRefusal> {
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
fn items(value: &SdkValue, each: impl Fn(&SdkValue) -> SdkValue) -> SdkValue {
    match value.get("items") {
        Some(SdkValue::List(items)) => SdkValue::List(items.iter().map(each).collect()),
        _ => SdkValue::List(Vec::new()),
    }
}

/// One page of a paged read: its filters as given on the first page (a
/// later page's are the cursor's), the Commit the first page read, where
/// this page starts and its size.
struct Page {
    call: &'static str,
    filters: Vec<(&'static str, SdkValue)>,
    carried: Option<Carried>,
    limit: i64,
}

impl Page {
    /// The page `cursor` asks for, or the first one; a cursor given with
    /// other filters, or of another call, is refused `InvalidInput`.
    fn of(
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
    fn text(&self, name: &str) -> Option<String> {
        (self.filters.iter())
            .find(|(held, _)| *held == name)
            .and_then(|(_, value)| own_text(value).map(str::to_owned))
    }

    /// The Commit this page reads: the first page's, else `--at` as given.
    fn pin(&self) -> Option<String> {
        match &self.carried {
            Some(carried) => carried.text("pin").map(str::to_owned),
            None => self.text("given_at"),
        }
    }

    /// Where this page starts, as the cursor carries it.
    fn place(&self) -> SdkValue {
        self.carried
            .as_ref()
            .map_or(SdkValue::Null, |carried| carried.place().clone())
    }

    /// The cursor of the next page, which starts at `place`; null when
    /// there is no next page. `read` is the Commit this page read.
    fn next(&self, place: SdkValue, read: SdkValue) -> SdkValue {
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
fn own_text(value: &SdkValue) -> Option<&str> {
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

/// `file.stat`: one path's type, size, mode and modification time.
pub fn stat(home: &Path, events: &mut Events<'_>, request: &FileStat) -> Outcome {
    let mut args = Args::default();
    args.at(&request.branch, request.at.as_deref())
        .text("path", &request.path);
    let value = run(home, events, &request.project, "fs.stat", args)?;
    Ok(SdkValue::map([
        ("type", take(&value, "type")),
        ("size", take(&value, "size")),
        ("mode", octal(&take(&value, "mode"))),
        ("commit", pin(&value)),
        ("root", take(&value, "root")),
        ("path", take(&value, "path")),
        ("inode", take(&value, "inode")),
        ("modified_at", date(value.get("modifiedMs"))),
    ]))
}

/// The engine arguments of one page of a positional listing.
fn positional(page: &Page, args: &mut Args) {
    let pos = match page.place() {
        SdkValue::Int(pos) => Some(pos),
        _ => None,
    };
    args.int("pageSize", Some(page.limit)).int("pagePos", pos);
}

/// `file.list`: one page of a folder's own entries.
pub fn list(home: &Path, events: &mut Events<'_>, request: &FileList) -> Outcome {
    let given = vec![
        ("branch", SdkValue::text(&request.branch)),
        ("path", SdkValue::text(&request.path)),
        ("given_at", text(&request.at)),
    ];
    let page = Page::of("file.list", given, request.cursor.as_deref(), request.limit)?;
    let (branch, path) = (page.text("branch"), page.text("path"));
    let mut args = Args::default();
    args.at(&branch.unwrap_or_default(), page.pin().as_deref())
        .text("path", &path.unwrap_or_default());
    positional(&page, &mut args);
    let value = run(home, events, &request.project, "fs.list", args)?;
    let row = |row: &SdkValue| {
        SdkValue::map([
            ("path", take(row, "path")),
            ("type", take(row, "type")),
            ("inode", take(row, "inode")),
        ])
    };
    Ok(SdkValue::map([
        ("items", items(&value, row)),
        (
            "next_cursor",
            page.next(take(&value, "nextPagePos"), pin(&value)),
        ),
        ("commit", pin(&value)),
        ("root", take(&value, "root")),
        ("path", take(&value, "path")),
        ("skipped_hidden", take(&value, "removed_count")),
    ]))
}

/// The filters a search was given.
fn search_filters(request: &FileSearch, grep: bool) -> Vec<(&'static str, SdkValue)> {
    let mut given = vec![
        ("branch", SdkValue::text(&request.branch)),
        ("pattern", SdkValue::text(&request.pattern)),
        (
            "path",
            SdkValue::text(request.path.as_deref().unwrap_or("/")),
        ),
        ("exclude", text(&request.exclude)),
        ("given_at", text(&request.at)),
    ];
    if grep {
        // a flag left off is no filter: the cursor's holds
        let fold = request.ignore_case.then_some(SdkValue::Bool(true));
        given.push(("include", text(&request.include)));
        given.push(("ignore_case", fold.unwrap_or_default()));
    }
    given
}

/// The engine arguments of a search page.
fn search_args(page: &Page) -> Args {
    let mut args = Args::default();
    args.at(
        &page.text("branch").unwrap_or_default(),
        page.pin().as_deref(),
    )
    .text("path", &page.text("path").unwrap_or_default())
    .text("pattern", &page.text("pattern").unwrap_or_default())
    .opt("exclude", page.text("exclude").as_deref())
    .opt("include", page.text("include").as_deref());
    let fold = (page.filters.iter())
        .any(|(name, value)| *name == "ignore_case" && *value == SdkValue::Bool(true));
    args.boolean("ignoreCase", fold.then_some(true));
    positional(page, &mut args);
    args
}

/// `file.glob`: one page of the paths under a folder that match a glob.
pub fn glob(home: &Path, events: &mut Events<'_>, request: &FileSearch) -> Outcome {
    let given = search_filters(request, false);
    let page = Page::of("file.glob", given, request.cursor.as_deref(), request.limit)?;
    let value = run(
        home,
        events,
        &request.project,
        "fs.glob",
        search_args(&page),
    )?;
    Ok(SdkValue::map([
        ("items", items(&value, Clone::clone)),
        (
            "next_cursor",
            page.next(take(&value, "nextPagePos"), pin(&value)),
        ),
        ("commit", pin(&value)),
        ("root", take(&value, "root")),
        ("path", take(&value, "path")),
        ("skipped_hidden", take(&value, "removed_count")),
    ]))
}

/// `file.grep`: one page of the lines under a folder that hold a literal
/// text; files that are not text are skipped and counted.
pub fn grep(home: &Path, events: &mut Events<'_>, request: &FileSearch) -> Outcome {
    let given = search_filters(request, true);
    let page = Page::of("file.grep", given, request.cursor.as_deref(), request.limit)?;
    let value = run(
        home,
        events,
        &request.project,
        "fs.grep",
        search_args(&page),
    )?;
    let row = |row: &SdkValue| {
        SdkValue::map([
            ("path", take(row, "path")),
            ("line", take(row, "line")),
            ("text", take(row, "text")),
            (
                "line_truncated",
                SdkValue::Bool(row.get_bool("lineTruncated")),
            ),
        ])
    };
    Ok(SdkValue::map([
        ("items", items(&value, row)),
        (
            "next_cursor",
            page.next(take(&value, "nextPagePos"), pin(&value)),
        ),
        ("commit", pin(&value)),
        ("root", take(&value, "root")),
        ("path", take(&value, "path")),
        ("skipped_binary", take(&value, "skipped")),
    ]))
}

/// One Commit that changed the path, as `file.history` lists it: the
/// path's type, size and mode after it, and which of its facts changed.
fn change(row: &SdkValue) -> SdkValue {
    let after = take(row, "after");
    let changes = take(row, "changes");
    let id = |key: &str| row.get(key).map_or(SdkValue::Null, |at| take(at, "id"));
    let change = row.get_text("kind").map(str::to_ascii_lowercase);
    SdkValue::map([
        ("commit", id("ref")),
        ("change", SdkValue::opt_text(change)),
        ("message", take(row, "message")),
        ("parent", id("comparedTo")),
        ("created_at", date(row.get("recordedAt"))),
        ("type", take(&after, "type")),
        ("size", take(&after, "size")),
        ("mode", octal(&take(&after, "mode"))),
        ("content", SdkValue::Bool(changes.get_bool("content"))),
        ("metadata", SdkValue::Bool(changes.get_bool("metadata"))),
        ("identity", SdkValue::Bool(changes.get_bool("identity"))),
    ])
}

/// `file.history`: one page of the Commits that changed one path, newest
/// first. The cursor carries the Store's own continuation, the page size
/// it was made for, the filters and the Commit the first page started at.
pub fn history(home: &Path, events: &mut Events<'_>, request: &FileHistory) -> Outcome {
    let traversal = request.traversal.as_deref().unwrap_or("provenance");
    let given = vec![
        ("branch", SdkValue::text(&request.branch)),
        ("path", SdkValue::text(&request.path)),
        ("traversal", SdkValue::text(traversal)),
        ("given_at", text(&request.at)),
    ];
    let page = Page::of(
        "file.history",
        given,
        request.cursor.as_deref(),
        request.limit,
    )?;
    // the Store's continuation is bound to the page size it was made with
    let size = match &page.carried {
        Some(carried) => carried.get("size"),
        None => SdkValue::Int(page.limit),
    };
    let mut args = Args::default();
    args.at(
        &page.text("branch").unwrap_or_default(),
        page.pin().as_deref(),
    )
    .text("path", &page.text("path").unwrap_or_default())
    .opt("traversal", page.text("traversal").as_deref())
    .opt("continuation", own_text(&page.place()))
    .put("pageSize", size.clone());
    let value = run(home, events, &request.project, "fs.revision", args)?;
    let next = match take(&value, "continuation") {
        SdkValue::Null => SdkValue::Null,
        place => {
            let mut fields = page.filters.clone();
            fields.push(("size", size));
            let pin = match &page.carried {
                Some(carried) => carried.get("pin"),
                None => pin(&value),
            };
            fields.push(("pin", pin));
            SdkValue::Text(cursor::make(page.call, place, &fields))
        }
    };
    Ok(SdkValue::map([
        ("items", items(&value, change)),
        ("next_cursor", next),
        ("commit", pin(&value)),
        (
            "root",
            value
                .get("at")
                .map_or(SdkValue::Null, |at| take(at, "root")),
        ),
        ("path", take(&value, "path")),
    ]))
}

/// `file.blame`'s runs of lines with one owner, and each owner once.
fn owners(lines: &[SdkValue]) -> (Vec<SdkValue>, Vec<SdkValue>) {
    let (mut ranges, mut commits): (Vec<(i64, i64, SdkValue)>, Vec<SdkValue>) =
        (Vec::new(), Vec::new());
    for line in lines {
        let n = line.get_int("n").unwrap_or_default();
        let owner = take(line, "owner");
        let commit = owner.get("ref").map_or(SdkValue::Null, |at| take(at, "id"));
        match ranges.last_mut() {
            Some((_, end, held)) if *held == commit && *end + 1 == n => *end = n,
            _ => ranges.push((n, n, commit.clone())),
        }
        let listed = commits.iter().any(|row| row.get("commit") == Some(&commit));
        if commit != SdkValue::Null && !listed {
            commits.push(SdkValue::map([
                ("commit", commit),
                ("message", take(&owner, "message")),
                ("created_at", date(owner.get("time"))),
            ]));
        }
    }
    let range = |(start, end, commit): (i64, i64, SdkValue)| {
        SdkValue::map([
            ("start", SdkValue::Int(start)),
            ("end", SdkValue::Int(end)),
            ("commit", commit),
        ])
    };
    (ranges.into_iter().map(range).collect(), commits)
}

/// `file.blame`: the Commit that last changed each line of a window of a
/// text file, as runs of lines, each Commit listed once.
pub fn blame(home: &Path, events: &mut Events<'_>, request: &FileBlame) -> Outcome {
    let (start, end) = window("file.blame", request.line, request.limit)?;
    let mut args = Args::default();
    args.at(&request.branch, request.at.as_deref())
        .text("path", &request.path)
        .int("fromLine", Some(start))
        .int("lineCount", Some(end - start + 1));
    let value = run(home, events, &request.project, "fs.blame", args)?;
    let lines = match value.get("lines") {
        Some(SdkValue::List(lines)) => lines.as_slice(),
        _ => &[],
    };
    let (ranges, commits) = owners(lines);
    let partial = value
        .get("coverage")
        .and_then(|coverage| coverage.get_text("status"))
        == Some("partial");
    Ok(SdkValue::map([
        ("ranges", SdkValue::List(ranges)),
        ("commits", SdkValue::List(commits)),
        ("next_line", take(&value, "nextLine")),
        ("partial", SdkValue::Bool(partial)),
        ("commit", pin(&value)),
        ("root", take(&value, "root")),
        ("path", take(&value, "path")),
    ]))
}
