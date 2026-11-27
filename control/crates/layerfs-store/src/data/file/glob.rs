//! `file.glob`: one page of the paths a pattern matches.
use super::list::positional;
use super::read::{items, pin, run, take, text, Page};
use super::requests::FileSearch;
use crate::admin::route::methods::Args;
use layerfs_home::reply::events::Events;
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

/// The filters a search was given.
pub(super) fn search_filters(request: &FileSearch, grep: bool) -> Vec<(&'static str, SdkValue)> {
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
pub(super) fn search_args(page: &Page) -> Args {
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
