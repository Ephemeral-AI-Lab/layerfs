//! `file.blame`: the Commit that last changed each line of a window.
use super::read::{date, pin, run, take, window};
use super::requests::FileBlame;
use crate::admin::route::methods::Args;
use layerfs_home::events::Events;
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

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
