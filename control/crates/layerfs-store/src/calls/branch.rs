//! The `branch` family: a project's Branches, each call translated to the
//! Store method that does it.
use super::op::{on_project, page_of, DiffOptions, Page, StoreOp};
use crate::call::store_call;
use crate::resolve::target;
use layerfs_bridge::control::SdkValue;
use layerfs_home::events::Events;
use layerfs_home::outcome::{refused, Outcome};
use std::path::Path;

/// `branch.list(project, { page_size, page_pos })`.
#[derive(Clone, Debug, Default)]
pub struct BranchList {
    /// The project, `<store>/<project>`.
    pub project: String,
    /// The Branch namespace of a scoped call (only a scope sets it).
    pub namespace: Option<String>,
    /// The page.
    pub page: Page,
}

/// `branch.get(project, branch)`.
#[derive(Clone, Debug, Default)]
pub struct BranchGet {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
}

/// `branch.fork(project, name, from_branch, from_commit)`.
#[derive(Clone, Debug, Default)]
pub struct BranchFork {
    /// The project.
    pub project: String,
    /// The new Branch's name.
    pub name: String,
    /// The Branch it forks from.
    pub from_branch: String,
    /// The Commit it forks from, which takes precedence.
    pub from_commit: Option<String>,
}

/// `branch.merge(project, name, a, b, { message, units })`.
#[derive(Clone, Debug, Default)]
pub struct BranchMerge {
    /// The project.
    pub project: String,
    /// The new Branch's name.
    pub name: String,
    /// One Branch.
    pub a: String,
    /// The other.
    pub b: String,
    /// The Commit message.
    pub message: Option<String>,
    /// How each unit merges (JSON).
    pub units: Option<SdkValue>,
}

/// `branch.publish(project, branch, commit)`.
#[derive(Clone, Debug, Default)]
pub struct BranchPublish {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The Commit, else its head.
    pub commit: Option<String>,
}

/// `branch.diff(project, from_branch, to_branch, { paths_only, page_size,
/// page_pos, cursor })`.
#[derive(Clone, Debug, Default)]
pub struct BranchDiff {
    /// The project.
    pub project: String,
    /// One Branch.
    pub from_branch: String,
    /// The other.
    pub to_branch: String,
    /// Its options.
    pub options: DiffOptions,
}

/// `branch.list`: one page of the project's Branches.
pub fn list(home: &Path, events: &mut Events<'_>, request: &BranchList) -> Outcome {
    match &request.namespace {
        Some(namespace) => branches_in(home, events, request, namespace),
        None => {
            let op = StoreOp::ListBranches {
                name: None,
                page: request.page,
            };
            on_project(home, events, &request.project, op)
        }
    }
}

/// `branch.get`: one Branch.
pub fn get(home: &Path, events: &mut Events<'_>, request: &BranchGet) -> Outcome {
    let op = StoreOp::ListBranches {
        name: Some(&request.branch),
        page: Page::default(),
    };
    let page = on_project(home, events, &request.project, op)?;
    first(page, "NoSuchBranch", "Branch", &request.branch)
}

/// `branch.fork`: a new Branch from a Branch or a Commit.
pub fn fork(home: &Path, events: &mut Events<'_>, request: &BranchFork) -> Outcome {
    on_project(home, events, &request.project, StoreOp::Fork(request))
}

/// `branch.merge`: two Branches merged into a new one.
pub fn merge(home: &Path, events: &mut Events<'_>, request: &BranchMerge) -> Outcome {
    on_project(home, events, &request.project, StoreOp::Merge(request))
}

/// `branch.publish`: a Branch moved to a Commit.
pub fn publish(home: &Path, events: &mut Events<'_>, request: &BranchPublish) -> Outcome {
    on_project(home, events, &request.project, StoreOp::Publish(request))
}

/// `branch.diff`: the changes between two Branches.
pub fn diff(home: &Path, events: &mut Events<'_>, request: &BranchDiff) -> Outcome {
    on_project(
        home,
        events,
        &request.project,
        StoreOp::DiffBranches(request),
    )
}

/// The one item of a list page whose `name` or `id` is `wanted`; else the
/// refusal `code` (S §2.9 keeps 1.1.0's `NoSuch*` codes).
fn first(page: SdkValue, code: &str, what: &str, wanted: &str) -> Outcome {
    let items = match page.get("items") {
        Some(SdkValue::List(items)) => items.clone(),
        _ => Vec::new(),
    };
    items
        .into_iter()
        .find(|item| item.get_text("name") == Some(wanted) || item.get_text("id") == Some(wanted))
        .ok_or_else(|| refused(code, format!("no {what} {wanted}")))
}

/// A scoped `branch.list`: the Branches named `<namespace>.<name>`, named
/// relative to the namespace, one page (`page_size`, `page_pos`) of them.
/// The project's Branches are read page by page to the end, or until the
/// page is full; only the requested page is kept.
fn branches_in(
    home: &Path,
    events: &mut Events<'_>,
    request: &BranchList,
    namespace: &str,
) -> Outcome {
    let size = request.page.size.unwrap_or(50);
    let skip = request.page.pos.unwrap_or(0);
    if !(1..=1000).contains(&size) || skip < 0 {
        return Err(refused(
            "InvalidInput",
            "page_size is in 1..=1000 and page_pos is not negative",
        ));
    }
    let target = target(home, &request.project)?;
    let prefix = format!("{namespace}.");
    let (mut pos, mut seen, mut kept, mut more) = (0i64, 0i64, Vec::new(), false);
    'pages: loop {
        let op = StoreOp::ListBranches {
            name: None,
            page: Page {
                size: Some(1000),
                pos: Some(pos),
            },
        };
        let page = store_call(home, events, &target, &op)?;
        let (items, next) = page_of(&page, pos)?;
        for item in items {
            let Some(name) = item.get_text("name").and_then(|n| n.strip_prefix(&prefix)) else {
                continue;
            };
            let name = SdkValue::text(name);
            if seen < skip {
                seen += 1;
            } else if kept.len() as i64 == size {
                more = true;
                break 'pages;
            } else {
                kept.push(layerfs_home::home::with_field(item, "name", name));
            }
        }
        match next {
            Some(next) => pos = next,
            None => break,
        }
    }
    let next = if more {
        SdkValue::Int(skip + kept.len() as i64)
    } else {
        SdkValue::Null
    };
    Ok(SdkValue::map([
        ("items", SdkValue::List(kept)),
        ("hasMore", SdkValue::Bool(more)),
        ("nextPagePos", next),
    ]))
}
