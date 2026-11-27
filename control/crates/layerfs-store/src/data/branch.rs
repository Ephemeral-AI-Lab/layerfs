//! The `branch` family: a project's Branches, each call translated to the
//! Store method that does it, its answer the call's full map (the keys of
//! its long form; the dispatcher shapes it).
use super::super::admin::route::methods::{next_fields, on_project, Names, StoreOp};
use layerfs_home::cursor;
use layerfs_home::events::Events;
use layerfs_home::helpers::shown;
use layerfs_home::outcome::{refused, with_fix, Outcome};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

/// `branch.list(project, { prefix, exclude_prefix, limit, cursor })`.
#[derive(Clone, Debug, Default)]
pub struct BranchList {
    /// The project, `<store>/<project>`.
    pub project: String,
    /// The Branch namespace of a scoped call (only a scope sets it): the
    /// names listed are `<namespace>.<name>`, answered as `<name>`.
    pub namespace: Option<String>,
    /// Only Branches whose name starts with this.
    pub prefix: Option<String>,
    /// No Branch whose name starts with this.
    pub exclude_prefix: Option<String>,
    /// The most rows in one page.
    pub limit: Option<i64>,
    /// The `next_cursor` of the page before.
    pub cursor: Option<String>,
}

/// `branch.get(project, branch)`.
#[derive(Clone, Debug, Default)]
pub struct BranchGet {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
}

/// `branch.fork(project, name, [from], { at })`.
#[derive(Clone, Debug, Default)]
pub struct BranchFork {
    /// The project.
    pub project: String,
    /// The new Branch's name.
    pub name: String,
    /// The Branch it forks from (main when absent): a Branch name first,
    /// else a Commit id or prefix.
    pub from: Option<String>,
    /// The Commit of `from`'s history it forks at, instead of its head.
    pub at: Option<String>,
}

/// `branch.merge(project, branch, from, { into, units, expected, message })`.
#[derive(Clone, Debug, Default)]
pub struct BranchMerge {
    /// The project.
    pub project: String,
    /// The Branch merged into; its rules apply.
    pub branch: String,
    /// The Branch, or main, merged from.
    pub from: String,
    /// A new Branch the result goes to, leaving `branch` as it is.
    pub into: Option<String>,
    /// How each unit merges (JSON).
    pub units: Option<SdkValue>,
    /// The head `branch` must be at: a Commit id or prefix.
    pub expected: Option<String>,
    /// The merge Commit's message.
    pub message: Option<String>,
}

/// `branch.publish(project, branch, { at, expected, force })`.
#[derive(Clone, Debug, Default)]
pub struct BranchPublish {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The Commit published instead of the Branch's head.
    pub at: Option<String>,
    /// The head main must be at: a Commit id or prefix.
    pub expected: Option<String>,
    /// Publish over what main gained since the Branch forked.
    pub force: Option<bool>,
}

/// `branch.remove(project, branch)`.
#[derive(Clone, Debug, Default)]
pub struct BranchRemove {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
}

/// The call a `branch.list` cursor belongs to.
const LIST: &str = "branch.list";

/// A Branch row as the call answers it: what the Store listed, less the
/// head Commit it answers for other callers.
fn branch_row(row: &SdkValue, namespace: Option<&str>) -> SdkValue {
    let SdkValue::Map(fields) = row else {
        return row.clone();
    };
    let prefix = namespace.map(|namespace| format!("{namespace}."));
    let fields = fields
        .iter()
        .filter(|(key, _)| key != "head")
        .map(|(key, value)| {
            let value = match (key.as_str(), value, &prefix) {
                ("branch", SdkValue::Text(name), Some(prefix)) => name
                    .strip_prefix(prefix.as_str())
                    .map_or(value.clone(), SdkValue::text),
                _ => value.clone(),
            };
            (key.clone(), value)
        });
    SdkValue::Map(fields.collect())
}

fn items(value: &SdkValue) -> Vec<SdkValue> {
    match value.get("items") {
        Some(SdkValue::List(items)) => items.clone(),
        _ => Vec::new(),
    }
}

/// `branch.list`: one page of the project's Branches, main among them, in
/// name order. The cursor carries the filters only.
pub fn list(home: &Path, events: &mut Events<'_>, request: &BranchList) -> Outcome {
    let limit = cursor::limit(LIST, request.limit, cursor::DEFAULT_LIMIT)?;
    let text = |value: &Option<String>| value.clone().map_or(SdkValue::Null, SdkValue::Text);
    let filters = [
        ("prefix", text(&request.prefix)),
        ("exclude_prefix", text(&request.exclude_prefix)),
    ];
    let carried = (request.cursor.as_deref())
        .map(|made| cursor::resume(LIST, made, &filters))
        .transpose()?;
    let filter = |name: &str, given: &Option<String>| match &carried {
        Some(carried) => carried.text(name).map(str::to_owned),
        None => given.clone(),
    };
    let (prefix, exclude) = (
        filter("prefix", &request.prefix),
        filter("exclude_prefix", &request.exclude_prefix),
    );
    // a scope lists its namespace's Branches, by their names in it
    let scoped = |name: Option<String>| match &request.namespace {
        Some(namespace) => Some(format!("{namespace}.{}", name.unwrap_or_default())),
        None => name,
    };
    let (prefix, exclude) = (scoped(prefix), exclude.and_then(|e| scoped(Some(e))));
    let after = carried
        .as_ref()
        .and_then(|c| c.text("at"))
        .map(str::to_owned);
    let op = StoreOp::Branches {
        names: Names {
            prefix: prefix.as_deref(),
            exclude_prefix: exclude.as_deref(),
            after: after.as_deref(),
        },
        limit,
    };
    let page = on_project(home, events, &request.project, op).map_err(next_fields)?;
    let next = match page.get("next") {
        Some(place @ SdkValue::Text(_)) => {
            let filters = match &carried {
                Some(carried) => [
                    ("prefix", carried.get("prefix")),
                    ("exclude_prefix", carried.get("exclude_prefix")),
                ],
                None => filters,
            };
            SdkValue::Text(cursor::make(LIST, place.clone(), &filters))
        }
        _ => SdkValue::Null,
    };
    let namespace = request.namespace.as_deref();
    let rows = items(&page)
        .iter()
        .map(|row| branch_row(row, namespace))
        .collect();
    Ok(SdkValue::map([
        ("items", SdkValue::List(rows)),
        ("next_cursor", next),
    ]))
}

/// `branch.get`: one Branch, or main, read through the name index.
pub fn get(home: &Path, events: &mut Events<'_>, request: &BranchGet) -> Outcome {
    let op = StoreOp::ListBranches {
        name: Some(&request.branch),
        page: Default::default(),
    };
    let page = on_project(home, events, &request.project, op).map_err(next_fields)?;
    match items(&page).first() {
        Some(row) => Ok(branch_row(row, None)),
        None => Err(refused(
            "NoSuchBranch",
            format!("no Branch {}", request.branch),
        )),
    }
}

/// `branch.fork`: a new Branch from a Branch's head or one of its Commits.
pub fn fork(home: &Path, events: &mut Events<'_>, request: &BranchFork) -> Outcome {
    let (from, at) = (request.from.as_deref(), request.at.as_deref());
    let again = format!(
        "layerfs branch fork {} {}-2{}{}",
        request.project,
        request.name,
        from.map(|from| format!(" {from}")).unwrap_or_default(),
        at.map(|at| format!(" --at {at}")).unwrap_or_default(),
    );
    let value = on_project(home, events, &request.project, StoreOp::Fork(request))
        .map_err(|refusal| taken(next_fields(refusal), again))?;
    let take = |key: &str| value.get(key).cloned().unwrap_or_default();
    Ok(SdkValue::map([
        ("commit", take("commit")),
        ("outcome", SdkValue::text("forked")),
        ("branch", take("branch")),
        ("base", take("base")),
        ("root", take("root")),
    ]))
}

/// `refusal`, and when it is a taken name the call again under another
/// name as its `fix`.
fn taken(refusal: SdkRefusal, again: String) -> SdkRefusal {
    match refusal.code == "NameTaken" {
        true => with_fix(refusal, again),
        false => refusal,
    }
}

/// The rows of a list, or none.
fn rows(value: Option<&SdkValue>) -> Vec<SdkValue> {
    match value {
        Some(SdkValue::List(rows)) => rows.clone(),
        _ => Vec::new(),
    }
}

/// `branch.merge`: `from` merged into `branch`, or into a new Branch.
pub fn merge(home: &Path, events: &mut Events<'_>, request: &BranchMerge) -> Outcome {
    let again = format!(
        "layerfs branch merge {} {} {} --into {}-2",
        request.project,
        request.branch,
        request.from,
        request.into.as_deref().unwrap_or_default(),
    );
    // main moves only by publish: publishing `from` is the merge into it
    let publish = format!(
        "layerfs branch publish {} {}",
        request.project, request.from
    );
    let into_main = request.branch == "main" && request.into.is_none();
    let value =
        on_project(home, events, &request.project, StoreOp::Merge(request)).map_err(|refusal| {
            match into_main && refusal.code == "InvalidInput" {
                true => with_fix(next_fields(refusal), publish),
                false => taken(next_fields(refusal), again),
            }
        })?;
    let take = |key: &str| value.get(key).cloned().unwrap_or_default();
    let source = |at: usize| match value.get("sources") {
        Some(SdkValue::List(sources)) => sources
            .get(at)
            .and_then(|side| side.get("node").cloned())
            .unwrap_or_default(),
        _ => SdkValue::Null,
    };
    let (unchanged, fast_forward) = (value.get_bool("unchanged"), value.get_bool("fastForward"));
    let outcome = match (unchanged, fast_forward) {
        (true, _) => "unchanged",
        (false, true) => "fast_forward",
        (false, false) => "merged",
    };
    let path = |row: SdkValue| row.get("path").cloned().unwrap_or_default();
    let paths = rows(value.get("lww")).into_iter().map(path).collect();
    // each list keeps its first 20; its count only when it was cut
    let (overwritten, overwritten_count) = shown(paths, 0, value.get_bool("lwwTruncated"));
    let (units, units_count) = shown(
        rows(value.get("units")),
        0,
        value.get_bool("unitsTruncated"),
    );
    Ok(SdkValue::map([
        ("commit", take("commit")),
        ("unchanged", SdkValue::Bool(unchanged)),
        ("fast_forward", SdkValue::Bool(fast_forward)),
        ("overwritten_count", overwritten_count),
        ("overwritten", overwritten),
        ("outcome", SdkValue::text(outcome)),
        ("branch", take("branch")),
        ("parent", source(0)),
        ("second_parent", source(1)),
        ("merge_base", take("mergeBase")),
        ("root", take("root")),
        ("units_count", units_count),
        ("units", units),
    ]))
}

/// `branch.publish`: a Branch's head, or one of its Commits, as main's next
/// head.
pub fn publish(home: &Path, events: &mut Events<'_>, request: &BranchPublish) -> Outcome {
    let value = on_project(home, events, &request.project, StoreOp::Publish(request))
        .map_err(next_fields)?;
    let take = |key: &str| value.get(key).cloned().unwrap_or_default();
    let published = |key: &str| {
        (value.get("publishedLayer"))
            .and_then(|layer| layer.get(key).cloned())
            .unwrap_or_default()
    };
    let (commit, unchanged) = match value.get_text("kind") {
        Some("Added") => (published("id"), false),
        Some("UpToDate") => (take("layer"), true),
        _ => (take("headLayer"), true),
    };
    let source = |key: &str| {
        (value.get("source"))
            .and_then(|source| source.get(key).cloned())
            .unwrap_or_default()
    };
    Ok(SdkValue::map([
        ("commit", commit),
        ("unchanged", SdkValue::Bool(unchanged)),
        ("previous", take("previous")),
        (
            "outcome",
            SdkValue::text(if unchanged { "unchanged" } else { "published" }),
        ),
        ("branch", SdkValue::text(&request.branch)),
        ("source_commit", source("commit")),
        ("base", take("basedOn")),
        ("root", published("root")),
        ("size", published("logical")),
    ]))
}

/// `branch.remove`: one Branch hidden by its Store marker; its name stays
/// taken until compaction.
pub fn remove(home: &Path, events: &mut Events<'_>, request: &BranchRemove) -> Outcome {
    let op = StoreOp::BranchRemove(request);
    let value = match on_project(home, events, &request.project, op) {
        Ok(value) => value,
        Err(refusal) => {
            return Err(crate::data::unknown::named_unknown(
                refusal,
                "branch",
                &request.branch,
                None,
            ))
        }
    };
    let removed_at = value.get_int("removed_ms").map_or(SdkValue::Null, |ms| {
        SdkValue::text(layerfs_home::time::iso(ms))
    });
    Ok(SdkValue::map([
        ("ok", SdkValue::Bool(true)),
        ("outcome", SdkValue::text("removed")),
        ("branch", value.get("branch").cloned().unwrap_or_default()),
        ("removed_at", removed_at),
    ]))
}
