//! The `commit` family: a Branch's Commits, each call translated to the
//! Store method that does it.
use super::op::{on_project, page_of, DiffOptions, Page, StoreOp};
use crate::call::store_call;
use crate::resolve::target;
use layerfs_home::events::Events;
use layerfs_home::outcome::{refused, Outcome};
use std::path::Path;

/// `commit.list(project, branch, { page_size, page_pos })`.
#[derive(Clone, Debug, Default)]
pub struct CommitList {
    /// The project, `<store>/<project>`.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The page.
    pub page: Page,
}

/// `commit.get(project, branch, commit)`.
#[derive(Clone, Debug, Default)]
pub struct CommitGet {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The Commit's id, or a prefix of 8 or more of its characters.
    pub commit: String,
}

/// `commit.diff(project, from_commit, to_commit, { paths_only, page_size,
/// page_pos, cursor })`.
#[derive(Clone, Debug, Default)]
pub struct CommitDiff {
    /// The project.
    pub project: String,
    /// One Commit.
    pub from_commit: String,
    /// The other.
    pub to_commit: String,
    /// Its options.
    pub options: DiffOptions,
}

/// `commit.list`: one page of a Branch's Commits.
pub fn list(home: &Path, events: &mut Events<'_>, request: &CommitList) -> Outcome {
    let op = StoreOp::ListCommits {
        branch: &request.branch,
        page: request.page,
    };
    on_project(home, events, &request.project, op)
}

/// `commit.diff`: the changes between two Commits.
pub fn diff(home: &Path, events: &mut Events<'_>, request: &CommitDiff) -> Outcome {
    on_project(
        home,
        events,
        &request.project,
        StoreOp::DiffCommits(request),
    )
}

/// `commit.get`: the Commit, found by paging the Branch's Commits newest
/// first, each page from where the last one ended, to the end.
pub fn get(home: &Path, events: &mut Events<'_>, request: &CommitGet) -> Outcome {
    let target = target(home, &request.project)?;
    let wanted = request.commit.as_str();
    let mut pos = 0i64;
    loop {
        let op = StoreOp::ListCommits {
            branch: &request.branch,
            page: Page {
                number: None,
                size: Some(1000),
                pos: Some(pos),
            },
        };
        let page = store_call(home, events, &target, &op)?;
        let (items, next) = page_of(&page, pos)?;
        let found = items.into_iter().find(|item| {
            item.get_text("id")
                .is_some_and(|id| id == wanted || (wanted.len() >= 8 && id.starts_with(wanted)))
        });
        if let Some(found) = found {
            return Ok(found);
        }
        match next {
            Some(next) => pos = next,
            None => {
                return Err(refused(
                    "NoSuchCommit",
                    format!("no Commit {wanted} on Branch {}", request.branch),
                ))
            }
        }
    }
}
