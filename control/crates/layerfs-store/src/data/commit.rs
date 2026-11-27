//! The `commit` family: the one place history is read. Each call is
//! translated to the Store method that does it, its answer the call's full
//! map (the keys of its long form; the dispatcher shapes it). A paged call's
//! cursor carries its filters and the Commits it pinned.
use crate::admin::route::methods::{next_fields, on_project, StoreOp};
use layerfs_home::reply::cursor;
use layerfs_home::reply::events::Events;
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

/// `commit.list(project, branch, { at, limit, cursor })`.
#[derive(Clone, Debug, Default)]
pub struct CommitList {
    /// The project, `<store>/<project>`.
    pub project: String,
    /// The Branch, or main.
    pub branch: String,
    /// The Commit the list starts at instead of the head: an id or prefix.
    pub at: Option<String>,
    /// The most rows in one page.
    pub limit: Option<i64>,
    /// The `next_cursor` of the page before.
    pub cursor: Option<String>,
}

/// `commit.get(project, commit, { branch })`.
#[derive(Clone, Debug, Default)]
pub struct CommitGet {
    /// The project.
    pub project: String,
    /// The Commit's id, or a prefix of 8 or more of its characters.
    pub commit: String,
    /// A Branch whose history must hold it.
    pub branch: Option<String>,
}

/// `commit.diff(project, ref, { from, path, with patch, limit, cursor })`.
#[derive(Clone, Debug, Default)]
pub struct CommitDiff {
    /// The project.
    pub project: String,
    /// The side compared to: a Branch name, else a Commit id or prefix.
    pub reference: String,
    /// The side compared from, when it is not the default.
    pub from: Option<String>,
    /// Only this file, or this folder and what is under it.
    pub path: Option<String>,
    /// Whether each changed text file's lines are answered.
    pub patch: bool,
    /// The most rows in one page.
    pub limit: Option<i64>,
    /// The `next_cursor` of the page before.
    pub cursor: Option<String>,
}

/// One page of `branch.list_commits`, as the Store takes it.
#[derive(Clone, Debug, Default)]
pub struct CommitPage {
    /// The Branch, or main.
    pub branch: String,
    /// Where the first page starts, or the Commit it started at.
    pub at: Option<String>,
    /// The last Commit of the page before.
    pub after: Option<String>,
    /// The most rows.
    pub limit: i64,
}

/// One page of `commit.diff`, as the Store takes it.
#[derive(Clone, Debug, Default)]
pub struct DiffPage {
    /// The side compared to, on a first page.
    pub reference: Option<String>,
    /// The side compared from, on a first page.
    pub from: Option<String>,
    /// The path the page is narrowed to.
    pub path: Option<String>,
    /// Whether lines are answered.
    pub hunks: bool,
    /// The last path of the page before.
    pub after: Option<String>,
    /// The most rows.
    pub limit: i64,
    /// A later page's two sides, as the first page compared them: the
    /// Commit compared to and the one compared from (none: the empty tree).
    pub pins: Option<(String, Option<String>)>,
}

/// The calls the cursors belong to.
const LIST: &str = "commit.list";
const DIFF: &str = "commit.diff";

fn text(value: &Option<String>) -> SdkValue {
    value.clone().map_or(SdkValue::Null, SdkValue::Text)
}

/// A Commit row as the call answers it: its time as a date.
fn row(value: &SdkValue) -> SdkValue {
    let SdkValue::Map(fields) = value else {
        return value.clone();
    };
    let field = |(key, value): &(String, SdkValue)| match (key.as_str(), value) {
        ("createdMs", SdkValue::Int(ms)) => (
            "created_at".to_owned(),
            SdkValue::text(layerfs_home::reply::time::iso(*ms)),
        ),
        ("createdMs", _) => ("created_at".to_owned(), SdkValue::Null),
        _ => (key.clone(), value.clone()),
    };
    SdkValue::Map(fields.iter().map(field).collect())
}

fn items(value: &SdkValue, each: impl Fn(&SdkValue) -> SdkValue) -> SdkValue {
    match value.get("items") {
        Some(SdkValue::List(items)) => SdkValue::List(items.iter().map(each).collect()),
        _ => SdkValue::List(Vec::new()),
    }
}

/// `commit.list`: one page of a Branch's Commits, newest first; on main,
/// its publications. The cursor carries `--at` as given and the Commit the
/// first page started at, so a later page lists the same history.
pub fn list(home: &Path, events: &mut Events<'_>, request: &CommitList) -> Outcome {
    let limit = cursor::limit(LIST, request.limit, cursor::DEFAULT_LIMIT)?;
    // `at` is the cursor's own place: `--at` travels as `given_at`
    let filters = [
        ("branch", SdkValue::text(&request.branch)),
        ("given_at", text(&request.at)),
    ];
    let carried = (request.cursor.as_deref())
        .map(|made| cursor::resume(LIST, made, &filters))
        .transpose()?;
    let page = match &carried {
        Some(carried) => CommitPage {
            branch: request.branch.clone(),
            at: carried.text("pin").map(str::to_owned),
            after: carried.text("at").map(str::to_owned),
            limit,
        },
        None => CommitPage {
            branch: request.branch.clone(),
            at: request.at.clone(),
            after: None,
            limit,
        },
    };
    let value = on_project(home, events, &request.project, StoreOp::ListCommits(&page))
        .map_err(next_fields)?;
    let next = match (value.get("next"), &carried) {
        (Some(place @ SdkValue::Text(_)), _) => {
            let (at, pin) = match &carried {
                Some(carried) => (carried.get("given_at"), carried.get("pin")),
                None => (
                    text(&request.at),
                    value.get("at").cloned().unwrap_or_default(),
                ),
            };
            let fields = [
                ("branch", SdkValue::text(&request.branch)),
                ("given_at", at),
                ("pin", pin),
            ];
            SdkValue::Text(cursor::make(LIST, place.clone(), &fields))
        }
        _ => SdkValue::Null,
    };
    Ok(SdkValue::map([
        ("items", items(&value, row)),
        ("next_cursor", next),
    ]))
}

/// `commit.get`: one Commit of the project, by its id or a prefix, resolved
/// by the Store in one call.
pub fn get(home: &Path, events: &mut Events<'_>, request: &CommitGet) -> Outcome {
    let value = on_project(home, events, &request.project, StoreOp::GetCommit(request))
        .map_err(next_fields)?;
    Ok(cut_overwritten(row(&value)))
}

/// A Commit's `overwritten` paths as an answer shows them: the first 20,
/// and `overwrittenCount` (every decision the Commit made) only when the
/// list was cut, by the 20 or by the Store's own byte window.
fn cut_overwritten(value: SdkValue) -> SdkValue {
    let paths = match value.get("overwritten") {
        Some(SdkValue::List(paths)) => paths.clone(),
        _ => return value,
    };
    let total = (value.get_int("overwrittenCount"))
        .map_or(0, |count| usize::try_from(count).unwrap_or(usize::MAX));
    let SdkValue::Map(mut fields) = value else {
        return value;
    };
    let (paths, count) = layerfs_home::reply::values::shown(paths, total, false);
    for (key, value) in fields.iter_mut() {
        match key.as_str() {
            "overwritten" => *value = paths.clone(),
            "overwrittenCount" => *value = count.clone(),
            _ => {}
        }
    }
    SdkValue::Map(fields)
}

/// `commit.diff`: one page of the paths that differ between `from` and
/// `ref`. The cursor carries both Commits the first page compared and the
/// filters.
pub fn diff(home: &Path, events: &mut Events<'_>, request: &CommitDiff) -> Outcome {
    let limit = cursor::limit(DIFF, request.limit, cursor::DEFAULT_LIMIT)?;
    let filters = [
        ("ref", SdkValue::text(&request.reference)),
        ("from", text(&request.from)),
        ("path", text(&request.path)),
    ];
    let carried = (request.cursor.as_deref())
        .map(|made| cursor::resume(DIFF, made, &filters))
        .transpose()?;
    let page = match &carried {
        Some(carried) => DiffPage {
            reference: None,
            from: None,
            path: carried.text("path").map(str::to_owned),
            hunks: request.patch,
            after: carried.text("at").map(str::to_owned),
            limit,
            pins: Some((
                carried.text("to").unwrap_or_default().to_owned(),
                carried.text("from_commit").map(str::to_owned),
            )),
        },
        None => DiffPage {
            reference: Some(request.reference.clone()),
            from: request.from.clone(),
            path: request.path.clone(),
            hunks: request.patch,
            after: None,
            limit,
            pins: None,
        },
    };
    let value =
        on_project(home, events, &request.project, StoreOp::Diff(&page)).map_err(next_fields)?;
    let take = |key: &str| value.get(key).cloned().unwrap_or_default();
    let next = match value.get("next") {
        Some(place @ SdkValue::Text(_)) => {
            let filters: [(&str, SdkValue); 5] = match &carried {
                Some(carried) => [
                    ("ref", carried.get("ref")),
                    ("from", carried.get("from")),
                    ("path", carried.get("path")),
                    ("to", carried.get("to")),
                    ("from_commit", carried.get("from_commit")),
                ],
                None => [
                    ("ref", SdkValue::text(&request.reference)),
                    ("from", text(&request.from)),
                    ("path", text(&request.path)),
                    ("to", take("to")),
                    ("from_commit", take("from")),
                ],
            };
            SdkValue::Text(cursor::make(DIFF, place.clone(), &filters))
        }
        _ => SdkValue::Null,
    };
    Ok(SdkValue::map([
        ("items", take("items")),
        ("next_cursor", next),
        ("from", take("from")),
        ("to", take("to")),
        ("from_root", take("fromRoot")),
        ("to_root", take("toRoot")),
    ]))
}
