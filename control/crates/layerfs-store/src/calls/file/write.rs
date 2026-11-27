//! The `file` family's writes: each one `fs.*` Store call that makes one
//! Commit on a Branch (never main). `--expected` is the head guard: the
//! head the call's private Workspace bound to must be that Commit, else it
//! is refused `Conflict` naming the head as `commit`. Each answers the
//! Commit as its full id, `unchanged` when nothing changed and `merged`
//! when the Branch had moved and the change was merged.
use super::read::{date, run, take};
use super::requests::{FileApply, FileEdit, FileMkdir, FileMv, FileRm, FileWrite, Guard};
use super::{Change, Content};
use crate::calls::op::Args;
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::helpers::{map_of, shown};
use layerfs_home::outcome::{refused, with_fix, Outcome};
use std::path::Path;

/// The fields a change of `file.apply` may have.
const CHANGE_FIELDS: [&str; 10] = [
    "op",
    "path",
    "from",
    "to",
    "content",
    "old_string",
    "new_string",
    "replace_all",
    "mode",
    "recursive",
];

/// `text` as one shell word.
fn word(text: &str) -> String {
    let plain = !text.is_empty()
        && (text.bytes()).all(|c| c.is_ascii_alphanumeric() || b"/._-:@%+=,".contains(&c));
    match plain {
        true => text.to_owned(),
        false => format!("'{}'", text.replace('\'', "'\\''")),
    }
}

/// Permission bits given as octal text, `0644`: four digits, `0000` to
/// `7777`, as `file stat` answers them.
fn mode(text: Option<&str>) -> Result<Option<i64>, SdkRefusal> {
    let Some(text) = text else {
        return Ok(None);
    };
    let digits = text.len() == 4 && text.bytes().all(|c| (b'0'..=b'7').contains(&c));
    match digits.then(|| i64::from_str_radix(text, 8).ok()).flatten() {
        Some(bits) => Ok(Some(bits)),
        None => Err(not_a_mode(text)),
    }
}

fn not_a_mode(text: &str) -> SdkRefusal {
    refused(
        "InvalidInput",
        format!("`mode` is octal text of four digits, 0000 to 7777, such as 0644; not `{text}`"),
    )
}

/// A `mode` that is not text, named by what it was given as, never as
/// Rust `Debug` text.
fn not_text_mode(value: &SdkValue) -> SdkRefusal {
    let given = match value {
        SdkValue::Int(number) => format!("the number {number} (give it as text)"),
        SdkValue::Bool(value) => format!("{value}"),
        SdkValue::Bytes(_) => "bytes".to_owned(),
        SdkValue::List(_) => "a list".to_owned(),
        SdkValue::Map(_) => "a map".to_owned(),
        SdkValue::Text(_) | SdkValue::Null => "text".to_owned(),
    };
    refused(
        "InvalidInput",
        format!("`mode` is octal text of four digits, 0000 to 7777, such as 0644; not {given}"),
    )
}

/// The paths of a Commit's overwritten (`.lww`) decisions, the first 20,
/// and their count only when the list was cut.
fn overwritten(commit: &SdkValue) -> (SdkValue, SdkValue) {
    let paths = match commit.get("lww") {
        Some(SdkValue::List(rows)) => rows.iter().map(|row| take(row, "path")).collect(),
        _ => Vec::new(),
    };
    shown(paths, 0, commit.get_bool("lwwTruncated"))
}

/// A write's answer from the Store's: `replaced` only for `file.edit`.
fn answer(value: &SdkValue, replaced: Option<SdkValue>) -> SdkValue {
    let commit = take(value, "commit");
    let committed = value.get_text("kind") == Some("Committed");
    let merged = committed && take(&commit, "mergedFrom") != SdkValue::Null;
    let (paths, count) = overwritten(&commit);
    let outcome = match (committed, merged) {
        (false, _) => "unchanged",
        (true, true) => "merged",
        (true, false) => "committed",
    };
    let parent = match take(&commit, "parent") {
        SdkValue::Null => take(&commit, "base"),
        parent => parent,
    };
    let root = match committed {
        true => take(&commit, "root"),
        false => take(value, "root"),
    };
    let mut fields = vec![
        ("commit", take(value, "head")),
        ("unchanged", SdkValue::Bool(!committed)),
    ];
    fields.extend(replaced.map(|replaced| ("replaced", replaced)));
    fields.extend([
        ("merged", SdkValue::Bool(merged)),
        ("overwritten_count", count),
        ("overwritten", paths),
        ("outcome", SdkValue::text(outcome)),
        ("branch", take(value, "branch")),
        ("parent", parent),
        ("root", root),
        ("message", take(&commit, "message")),
        ("created_at", date(commit.get("time"))),
    ]);
    map_of(fields)
}

/// The engine arguments every write starts with.
fn base(branch: &str, guard: &Guard) -> Args {
    let mut args = Args::default();
    args.text("branch", branch)
        .opt("message", guard.message.as_deref())
        .opt("expected", guard.expected.as_deref());
    args
}

/// A write refused for its Branch being main, with the fork that makes a
/// Branch to write to.
fn on_main(refusal: SdkRefusal, project: &str) -> SdkRefusal {
    match refusal.code == "ReadOnly" {
        true => with_fix(
            refusal,
            format!("layerfs branch fork {} work", word(project)),
        ),
        false => refusal,
    }
}

/// Runs one write and answers it.
fn commit(
    home: &Path,
    events: &mut Events<'_>,
    project: &str,
    method: &'static str,
    args: Args,
) -> Outcome {
    let value = run(home, events, project, method, args).map_err(|e| on_main(e, project))?;
    Ok(answer(&value, None))
}

/// `file.write`: a file written, as one Commit.
pub fn write(home: &Path, events: &mut Events<'_>, request: &FileWrite) -> Outcome {
    let mut args = base(&request.branch, &request.guard);
    args.text("path", &request.path)
        .int("mode", mode(request.mode.as_deref())?);
    match &request.content {
        Content::Bytes(bytes) => args.put("bytes", SdkValue::Bytes(bytes.clone())),
        Content::Text(text) => args.put("content", SdkValue::text(text)),
    };
    commit(home, events, &request.project, "fs.write", args)
}

/// `file.edit`: one text, or with `replace_all` every occurrence of it,
/// replaced in a file, as one Commit. More than one occurrence without
/// `replace_all` is refused with a `fix` that adds it.
pub fn edit(home: &Path, events: &mut Events<'_>, request: &FileEdit) -> Outcome {
    let mut args = base(&request.branch, &request.guard);
    args.text("path", &request.path)
        .text("oldText", &request.old_string)
        .text("newText", &request.new_string)
        .boolean("replaceAll", Some(request.replace_all));
    let value = run(home, events, &request.project, "fs.edit", args).map_err(|refusal| {
        match refusal.code == "AmbiguousMatch" {
            true => with_fix(
                refusal,
                format!(
                    "layerfs file edit {} {} {} --old-string {} --new-string {} --replace-all",
                    word(&request.project),
                    word(&request.branch),
                    word(&request.path),
                    word(&request.old_string),
                    word(&request.new_string),
                ),
            ),
            false => on_main(refusal, &request.project),
        }
    })?;
    // without --replace-all exactly one occurrence was replaced
    let replaced = match take(&value, "replaced") {
        SdkValue::Null => SdkValue::Int(1),
        counted => counted,
    };
    Ok(answer(&value, Some(replaced)))
}

/// `file.rm`: a path removed, as one Commit; a folder that is not empty
/// only with `recursive`, else it is refused with a `fix` that adds it.
pub fn rm(home: &Path, events: &mut Events<'_>, request: &FileRm) -> Outcome {
    let mut args = base(&request.branch, &request.guard);
    args.text("path", &request.path)
        .boolean("recursive", Some(request.recursive));
    let value =
        run(home, events, &request.project, "fs.rm", args).map_err(|refusal| {
            match refusal.code == "NotEmpty" {
                true => with_fix(
                    refusal,
                    format!(
                        "layerfs file rm {} {} {} --recursive",
                        word(&request.project),
                        word(&request.branch),
                        word(&request.path),
                    ),
                ),
                false => on_main(refusal, &request.project),
            }
        })?;
    Ok(answer(&value, None))
}

/// `file.mkdir`: a folder and its missing parents made, as one Commit.
pub fn mkdir(home: &Path, events: &mut Events<'_>, request: &FileMkdir) -> Outcome {
    let mut args = base(&request.branch, &request.guard);
    args.text("path", &request.path)
        .int("mode", mode(request.mode.as_deref())?);
    commit(home, events, &request.project, "fs.mkdir", args)
}

/// `file.mv`: a path moved, as one Commit.
pub fn mv(home: &Path, events: &mut Events<'_>, request: &FileMv) -> Outcome {
    let mut args = base(&request.branch, &request.guard);
    args.text("from", &request.from).text("to", &request.to);
    commit(home, events, &request.project, "fs.mv", args)
}

/// `file.cp`: a path copied, a folder with everything in it, as one Commit.
pub fn cp(home: &Path, events: &mut Events<'_>, request: &FileMv) -> Outcome {
    let mut args = base(&request.branch, &request.guard);
    args.text("from", &request.from).text("to", &request.to);
    commit(home, events, &request.project, "fs.cp", args)
}

/// One change of `file.apply` as the Store takes it: its fields under the
/// Store's names, its mode a number, and a removal that is not recursive
/// unless it says so.
pub(crate) fn change(change: &Change) -> Result<SdkValue, SdkRefusal> {
    owned_change(change.fields.clone())
}

/// [`change`] of fields the caller gives up: each value is moved, never
/// copied (a streamed `file apply` holds one change at a time).
pub(crate) fn owned_change(input: Vec<(String, SdkValue)>) -> Result<SdkValue, SdkRefusal> {
    let mut fields = Vec::with_capacity(input.len() + 1);
    for (name, value) in input {
        if !CHANGE_FIELDS.contains(&name.as_str()) {
            return Err(refused(
                "InvalidInput",
                format!(
                    "`{name}` is not a change field; a change uses {}",
                    CHANGE_FIELDS.join(", ")
                ),
            ));
        }
        let field = match (name.as_str(), value) {
            ("content", value @ SdkValue::Bytes(_)) => ("bytes", value),
            ("old_string", value) => ("oldText", value),
            ("new_string", value) => ("newText", value),
            ("replace_all", value) => ("replaceAll", value),
            ("mode", SdkValue::Text(text)) => match mode(Some(&text))? {
                Some(bits) => ("mode", SdkValue::Int(bits)),
                None => ("mode", SdkValue::Null),
            },
            ("mode", SdkValue::Null) => ("mode", SdkValue::Null),
            ("mode", other) => return Err(not_text_mode(&other)),
            (other, value) => (other, value),
        };
        fields.push((field.0.to_owned(), field.1));
    }
    let removal =
        (fields.iter()).any(|(name, value)| name == "op" && *value == SdkValue::text("rm"));
    if removal && !fields.iter().any(|(name, _)| name == "recursive") {
        fields.push(("recursive".to_owned(), SdkValue::Bool(false)));
    }
    Ok(SdkValue::Map(fields))
}

/// `file.apply`: several changes as one Commit, all or none.
pub fn apply(home: &Path, events: &mut Events<'_>, request: &FileApply) -> Outcome {
    let changes = (request.changes.iter())
        .map(change)
        .collect::<Result<Vec<_>, _>>()?;
    let mut args = base(&request.branch, &request.guard);
    args.put("changes", SdkValue::List(changes));
    commit(home, events, &request.project, "fs.apply", args)
}
