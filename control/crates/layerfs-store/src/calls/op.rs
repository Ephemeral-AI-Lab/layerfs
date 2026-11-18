//! The Store methods the three families run and the one translation of each
//! to its engine method and arguments: snake_case to the engine's camelCase,
//! an absent option not sent. Both routes send these (in this process, or in
//! a `store.call` frame to the Store's worker).
use super::branch::{BranchFork, BranchMerge, BranchPublish, BranchRemove};
use super::commit::{CommitGet, CommitPage, DiffPage};
use super::file::FileOp;
use crate::call::store_call;
use crate::resolve::target;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::outcome::{with_next, Outcome};
use std::path::Path;

/// One page of a list: its size and position, each when sent.
#[derive(Clone, Copy, Debug, Default)]
pub struct Page {
    /// One-based `page`, mutually exclusive with `page_pos`.
    pub number: Option<i64>,
    /// `page_size`.
    pub size: Option<i64>,
    /// `page_pos`.
    pub pos: Option<i64>,
}

/// Which Branches one page of `branch.list` reads, by name.
#[derive(Clone, Copy, Debug, Default)]
pub struct Names<'a> {
    /// Only names that start with this.
    pub prefix: Option<&'a str>,
    /// No name that starts with this.
    pub exclude_prefix: Option<&'a str>,
    /// Only names after this one: where the page before ended.
    pub after: Option<&'a str>,
}

/// One Store method with its arguments.
pub enum StoreOp<'a> {
    /// Store-wide statistics (no project required).
    Statistics,
    /// Entry counts at one project pin.
    ProjectStats(&'a super::project::ProjectStats),
    /// `project.remove_file`: hide one path from every node at or below its
    /// marker frontier.
    RemoveFile(&'a super::project::ProjectRemoveFile),
    /// `project.list_branches`: the Branch `name`, or, without one, the
    /// first page of `page.size` Branches.
    ListBranches {
        /// The one Branch it lists.
        name: Option<&'a str>,
        /// The page.
        page: Page,
    },
    /// `project.list_branches`: one page of Branches by name.
    Branches {
        /// Which names.
        names: Names<'a>,
        /// The most rows.
        limit: i64,
    },
    /// `branch.list_commits`: one page of a Branch's Commits.
    ListCommits(&'a CommitPage),
    /// `branch.get_commit`: one Commit of the project.
    GetCommit(&'a CommitGet),
    /// `branch.fork`.
    Fork(&'a BranchFork),
    /// `branch.merge`.
    Merge(&'a BranchMerge),
    /// `branch.publish`.
    Publish(&'a BranchPublish),
    /// `branch.remove`.
    BranchRemove(&'a BranchRemove),
    /// `commit.diff`: one page of the paths that differ.
    Diff(&'a DiffPage),
    /// One `fs.*` method of the `file` family.
    File(&'a FileOp),
}

/// The engine's arguments, built in order; an absent value is not sent.
#[derive(Default)]
pub(super) struct Args(pub(super) Vec<(String, SdkValue)>);

impl Args {
    pub(super) fn put(&mut self, name: &str, value: SdkValue) -> &mut Self {
        self.0.push((name.to_owned(), value));
        self
    }

    pub(super) fn text(&mut self, name: &str, value: &str) -> &mut Self {
        self.put(name, SdkValue::text(value))
    }

    pub(super) fn opt(&mut self, name: &str, value: Option<&str>) -> &mut Self {
        match value {
            Some(value) => self.text(name, value),
            None => self,
        }
    }

    /// A JSON value; null is not sent.
    fn json(&mut self, name: &str, value: Option<&SdkValue>) -> &mut Self {
        match value {
            Some(SdkValue::Null) | None => self,
            Some(value) => self.put(name, value.clone()),
        }
    }

    pub(super) fn int(&mut self, name: &str, value: Option<i64>) -> &mut Self {
        match value {
            Some(value) => self.put(name, SdkValue::Int(value)),
            None => self,
        }
    }

    pub(super) fn boolean(&mut self, name: &str, value: Option<bool>) -> &mut Self {
        match value {
            Some(value) => self.put(name, SdkValue::Bool(value)),
            None => self,
        }
    }
    /// The snapshot a read names: its Branch, or the Commit when given.
    pub(super) fn at(&mut self, branch: &str, commit: Option<&str>) -> &mut Self {
        self.text("branch", branch).opt("commit", commit)
    }
}

impl StoreOp<'_> {
    /// The engine method.
    pub fn method(&self) -> &'static str {
        match self {
            Self::Statistics => "store.stats",
            Self::ProjectStats(_) => "project.stats",
            Self::RemoveFile(_) => "project.remove_file",
            Self::ListBranches { .. } | Self::Branches { .. } => "project.list_branches",
            Self::ListCommits(_) => "branch.list_commits",
            Self::GetCommit(_) => "branch.get_commit",
            Self::Fork(_) => "branch.fork",
            Self::Merge(_) => "branch.merge",
            Self::Publish(_) => "branch.publish",
            Self::BranchRemove(_) => "branch.remove",
            Self::Diff(_) => "commit.diff",
            Self::File(op) => op.method,
        }
    }

    /// The engine's arguments, without the project (the route adds it).
    pub fn engine_args(&self) -> Vec<(String, SdkValue)> {
        let mut args = Args::default();
        match self {
            Self::Statistics => &mut args,
            Self::ProjectStats(r) => args.at(&r.branch, r.commit.as_deref()),
            Self::RemoveFile(r) => args.text("path", &r.path),
            Self::ListBranches { name, page } => args.opt("name", *name).int("limit", page.size),
            Self::Branches { names, limit } => args
                .opt("prefix", names.prefix)
                .opt("excludePrefix", names.exclude_prefix)
                .opt("after", names.after)
                .int("limit", Some(*limit)),
            Self::ListCommits(r) => args
                .text("branch", &r.branch)
                .opt("at", r.at.as_deref())
                .opt("after", r.after.as_deref())
                .int("limit", Some(r.limit)),
            Self::GetCommit(r) => args
                .text("commit", &r.commit)
                .opt("branch", r.branch.as_deref()),
            Self::Fork(r) => args
                .text("name", &r.name)
                .opt("from", r.from.as_deref())
                .opt("at", r.at.as_deref()),
            Self::Merge(r) => args
                .text("branch", &r.branch)
                .text("with", &r.from)
                .opt("name", r.into.as_deref())
                .opt("expected", r.expected.as_deref())
                .opt("message", r.message.as_deref())
                .json("units", r.units.as_ref()),
            Self::Publish(r) => args
                .text("branch", &r.branch)
                .opt("at", r.at.as_deref())
                .opt("expected", r.expected.as_deref())
                .boolean("force", r.force),
            Self::BranchRemove(r) => args.text("branch", &r.branch),
            Self::Diff(r) => {
                args.opt("ref", r.reference.as_deref())
                    .opt("from", r.from.as_deref())
                    .opt("path", r.path.as_deref())
                    .boolean("hunks", Some(r.hunks))
                    .opt("after", r.after.as_deref())
                    .int("limit", Some(r.limit));
                match &r.pins {
                    Some((to, from)) => args
                        .boolean("pinned", Some(true))
                        .text("pinTo", to)
                        .opt("pinFrom", from.as_deref()),
                    None => &mut args,
                }
            }
            Self::File(op) => return op.args.clone(),
        };
        args.0
    }
}

/// Runs `op` on the Store of `project`.
pub(super) fn on_project(
    home: &Path,
    events: &mut Events<'_>,
    project: &str,
    op: StoreOp<'_>,
) -> Outcome {
    let target = target(home, project)?;
    store_call(home, events, &target, &op)
}

/// `refusal` with the fields its caller's next call needs, flat beside it
/// (call surface 2.0): the head a write expected, as `commit`, on a moved
/// head; how many Commits a prefix matches, as `matches`; the conflicting
/// paths of a merge, as `conflicts`.
pub(super) fn next_fields(refusal: SdkRefusal) -> SdkRefusal {
    let mut refusal = refusal;
    let detail = refusal.detail.clone();
    let carried = [
        ("HeadMoved", "head", "commit"),
        ("InvalidInput", "matches", "matches"),
        ("AmbiguousMatch", "matches", "matches"),
        ("Conflict", "conflicts", "conflicts"),
    ];
    for (code, held, next) in carried {
        if let (true, Some(value)) = (refusal.code == code, detail.get(held)) {
            refusal = with_next(refusal, next, value.clone());
        }
    }
    refusal
}
