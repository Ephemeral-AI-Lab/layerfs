//! `file.history`: one page of the Commits that changed a path.
use super::read::{date, items, octal, own_text, pin, run, take, text, Page};
use super::requests::FileHistory;
use crate::admin::route::methods::Args;
use layerfs_home::reply::cursor;
use layerfs_home::reply::events::Events;
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

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
    let value = run(home, events, &request.project, "file.history", args)?;
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
