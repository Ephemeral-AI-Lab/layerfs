//! `file.list`: one page of a folder's entries.
use super::read::{items, pin, run, take, text, Page};
use super::requests::FileList;
use crate::admin::route::methods::Args;
use layerfs_home::events::Events;
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

/// The engine arguments of one page of a positional listing.
pub(super) fn positional(page: &Page, args: &mut Args) {
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
