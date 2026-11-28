//! `file.grep`: one page of the lines a pattern matches.
use super::glob::{search_args, search_filters};
use super::read::{items, pin, run, take, Page};
use super::requests::FileSearch;
use layerfs_home::reply::events::Events;
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

/// `file.grep`: one page of the lines under a folder that hold a literal
/// text; files that are not text are skipped and counted.
pub fn grep(home: &Path, events: &mut Events<'_>, request: &FileSearch) -> Outcome {
    let given = search_filters(request, true);
    let page = Page::of("file.grep", given, request.cursor.as_deref(), request.limit)?;
    let value = run(
        home,
        events,
        &request.project,
        "file.grep",
        search_args(&page),
    )?;
    let row = |row: &SdkValue| {
        SdkValue::map([
            ("path", take(row, "path")),
            ("line", take(row, "line")),
            // the line without its end, as the row's examples show it
            (
                "text",
                match row.get_text("text") {
                    Some(text) => SdkValue::text(text.trim_end_matches(['\n', '\r'])),
                    None => take(row, "text"),
                },
            ),
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
