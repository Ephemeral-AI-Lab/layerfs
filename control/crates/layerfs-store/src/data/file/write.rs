//! `file.write`, and what the `file` family's writes share: each one `fs.*` Store call that makes one
//! Commit on a Branch (never main). `--expected` is the head guard: the
//! head the call's private Workspace bound to must be that Commit, else it
//! is refused `Conflict` naming the head as `commit`. Each answers the
//! Commit as its full id, `unchanged` when nothing changed and `merged`
//! when the Branch had moved and the change was merged.
use super::read::{date, run, take};
use super::requests::{FileWrite, Guard};
use super::Content;
use crate::admin::route::methods::Args;
use layerfs_home::reply::events::Events;
use layerfs_home::reply::outcome::{refused, with_fix, Outcome};
use layerfs_home::reply::values::{map_of, shown};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

/// `text` as one shell word.
pub(super) fn word(text: &str) -> String {
    let plain = !text.is_empty()
        && (text.bytes()).all(|c| c.is_ascii_alphanumeric() || b"/._-:@%+=,".contains(&c));
    match plain {
        true => text.to_owned(),
        false => format!("'{}'", text.replace('\'', "'\\''")),
    }
}

/// Permission bits given as octal text, `0644`: four digits, `0000` to
/// `7777`, as `file stat` answers them.
pub(super) fn mode(text: Option<&str>) -> Result<Option<i64>, SdkRefusal> {
    let Some(text) = text else {
        return Ok(None);
    };
    let digits = text.len() == 4 && text.bytes().all(|c| (b'0'..=b'7').contains(&c));
    match digits.then(|| i64::from_str_radix(text, 8).ok()).flatten() {
        Some(bits) => Ok(Some(bits)),
        None => Err(not_a_mode(text)),
    }
}

pub(super) fn not_a_mode(text: &str) -> SdkRefusal {
    refused(
        "InvalidInput",
        format!("`mode` is octal text of four digits, 0000 to 7777, such as 0644; not `{text}`"),
    )
}

/// The paths of a Commit's overwritten (`.lww`) decisions, the first 20,
/// and their count only when the list was cut.
pub(super) fn overwritten(commit: &SdkValue) -> (SdkValue, SdkValue) {
    let paths = match commit.get("lww") {
        Some(SdkValue::List(rows)) => rows.iter().map(|row| take(row, "path")).collect(),
        _ => Vec::new(),
    };
    shown(paths, 0, commit.get_bool("lwwTruncated"))
}

/// A write's answer from the Store's: `replaced` only for `file.edit`.
pub(super) fn answer(value: &SdkValue, replaced: Option<SdkValue>) -> SdkValue {
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
pub(super) fn base(branch: &str, guard: &Guard) -> Args {
    let mut args = Args::default();
    args.text("branch", branch)
        .opt("message", guard.message.as_deref())
        .opt("expected", guard.expected.as_deref());
    args
}

/// A write refused for its Branch being main, with the fork that makes a
/// Branch to write to.
pub(super) fn on_main(refusal: SdkRefusal, project: &str) -> SdkRefusal {
    match refusal.code == "ReadOnly" {
        true => with_fix(
            refusal,
            format!("layerfs branch fork {} work", word(project)),
        ),
        false => refusal,
    }
}

/// Runs one write and answers it.
pub(super) fn commit(
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
