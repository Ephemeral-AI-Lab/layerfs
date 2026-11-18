//! The Store methods the three families run and the one translation of each
//! to its engine method and arguments: snake_case to the engine's camelCase,
//! an absent option not sent. Both routes send these (in this process, or in
//! a `store.call` frame to the Store's worker).
use super::branch::{BranchFork, BranchMerge, BranchPublish, BranchRemove};
use super::commit::{CommitGet, CommitPage, DiffPage};
use super::file::{
    FileApply, FileBlame, FileCp, FileDiff, FileEdit, FileGlob, FileGrep, FileList, FileMkdir,
    FileMv, FileRead, FileRevision, FileRm, FileStat, FileWrite, Side,
};
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

/// File content: text or bytes.
#[derive(Clone, Debug)]
pub enum Content {
    /// Text.
    Text(String),
    /// Bytes.
    Bytes(Vec<u8>),
}

/// One change of `file.apply`: its fields as sent, in order (snake_case;
/// any value).
#[derive(Clone, Debug, Default)]
pub struct Change {
    /// Its fields.
    pub fields: Vec<(String, SdkValue)>,
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
    /// `fs.revision`: saved changes to one path.
    Revision(&'a FileRevision),
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
    /// `fs.read`.
    Read(&'a FileRead),
    /// `fs.stat`.
    Stat(&'a FileStat),
    /// `fs.list`.
    List(&'a FileList),
    /// `fs.glob`.
    Glob(&'a FileGlob),
    /// `fs.grep`.
    Grep(&'a FileGrep),
    /// `fs.blame`.
    Blame(&'a FileBlame),
    /// `fs.diff_files`.
    DiffFiles(&'a FileDiff),
    /// `fs.write`.
    Write(&'a FileWrite),
    /// `fs.edit`.
    Edit(&'a FileEdit),
    /// `fs.rm`.
    Rm(&'a FileRm),
    /// `fs.mkdir`.
    Mkdir(&'a FileMkdir),
    /// `fs.mv`.
    Mv(&'a FileMv),
    /// `fs.cp`.
    Cp(&'a FileCp),
    /// `fs.apply`.
    Apply(&'a FileApply),
}

/// The engine's arguments, built in order; an absent value is not sent.
#[derive(Default)]
struct Args(Vec<(String, SdkValue)>);

impl Args {
    fn put(&mut self, name: &str, value: SdkValue) -> &mut Self {
        self.0.push((name.to_owned(), value));
        self
    }

    fn text(&mut self, name: &str, value: &str) -> &mut Self {
        self.put(name, SdkValue::text(value))
    }

    fn opt(&mut self, name: &str, value: Option<&str>) -> &mut Self {
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

    fn int(&mut self, name: &str, value: Option<i64>) -> &mut Self {
        match value {
            Some(value) => self.put(name, SdkValue::Int(value)),
            None => self,
        }
    }

    fn boolean(&mut self, name: &str, value: Option<bool>) -> &mut Self {
        match value {
            Some(value) => self.put(name, SdkValue::Bool(value)),
            None => self,
        }
    }

    fn page(&mut self, page: Page) -> &mut Self {
        self.int("pageSize", page.size)
            .int("pagePos", page.pos)
            .int("page", page.number)
    }

    /// The snapshot a read names: its Branch, or the Commit when given.
    fn at(&mut self, branch: &str, commit: Option<&str>) -> &mut Self {
        self.text("branch", branch).opt("commit", commit)
    }

    fn content(&mut self, content: &Content) -> &mut Self {
        match content {
            Content::Bytes(bytes) => self.put("bytes", SdkValue::Bytes(bytes.clone())),
            Content::Text(text) => self.put("content", SdkValue::text(text)),
        }
    }
}

/// One side of `fs.diff_files`: the Commit, else the Branch.
fn side(side: &Side) -> SdkValue {
    let text = |value: Option<&String>| value.map(SdkValue::text).unwrap_or_default();
    SdkValue::map([
        (
            "commit",
            text(side.commit.as_ref().or(side.branch.as_ref())),
        ),
        ("path", text(side.path.as_ref())),
    ])
}

fn renamed(name: &str) -> &str {
    match name {
        "old" => "oldText",
        "new" => "newText",
        "from_path" => "from",
        "to_path" => "to",
        other => other,
    }
}

/// `file.apply`'s changes, from their snake_case fields to the Store's.
fn changes(changes: &[Change]) -> SdkValue {
    let change = |change: &Change| {
        let fields = change.fields.iter().map(|(name, value)| match value {
            // write content may be bytes: the Store takes those as `bytes`
            SdkValue::Bytes(_) if name == "content" => ("bytes".to_owned(), value.clone()),
            _ => (renamed(name).to_owned(), value.clone()),
        });
        SdkValue::Map(fields.collect())
    };
    SdkValue::List(changes.iter().map(change).collect())
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
            Self::Revision(_) => "fs.revision",
            Self::Fork(_) => "branch.fork",
            Self::Merge(_) => "branch.merge",
            Self::Publish(_) => "branch.publish",
            Self::BranchRemove(_) => "branch.remove",
            Self::Diff(_) => "commit.diff",
            Self::Read(_) => "fs.read",
            Self::Stat(_) => "fs.stat",
            Self::List(_) => "fs.list",
            Self::Glob(_) => "fs.glob",
            Self::Grep(_) => "fs.grep",
            Self::Blame(_) => "fs.blame",
            Self::DiffFiles(_) => "fs.diff_files",
            Self::Write(_) => "fs.write",
            Self::Edit(_) => "fs.edit",
            Self::Rm(_) => "fs.rm",
            Self::Mkdir(_) => "fs.mkdir",
            Self::Mv(_) => "fs.mv",
            Self::Cp(_) => "fs.cp",
            Self::Apply(_) => "fs.apply",
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
            Self::Revision(r) => args
                .at(&r.branch, r.at_commit.as_deref())
                .text("path", &r.path)
                .opt("traversal", r.traversal.as_deref())
                .opt("continuation", r.continuation.as_deref())
                .page(r.page),
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
            Self::Read(r) => args
                .at(&r.branch, r.commit.as_deref())
                .text("path", &r.path)
                .int("startLine", r.start_line)
                .int("endLine", r.end_line),
            Self::Stat(r) => args
                .at(&r.branch, r.commit.as_deref())
                .text("path", &r.path),
            Self::List(r) => args
                .at(&r.branch, r.commit.as_deref())
                .text("path", &r.path)
                .page(r.page),
            Self::Glob(r) | Self::Grep(r) => args
                .at(&r.branch, r.commit.as_deref())
                .text("path", &r.path)
                .text("pattern", &r.pattern)
                .page(r.page),
            Self::Blame(r) => args
                .at(&r.branch, r.commit.as_deref())
                .text("path", &r.path)
                .int("fromLine", r.from_line)
                .int("lineCount", r.line_count),
            Self::DiffFiles(r) => args
                .put("left", side(&r.left))
                .put("right", side(&r.right))
                .boolean("includeAttribution", r.include_attribution)
                .boolean("allowMissing", r.allow_missing)
                .page(r.page),
            Self::Write(r) => args
                .text("branch", &r.branch)
                .text("path", &r.path)
                .opt("message", r.message.as_deref())
                .int("mode", r.mode)
                .content(&r.content),
            Self::Edit(r) => args
                .text("branch", &r.branch)
                .text("path", &r.path)
                .opt("oldText", r.old.as_deref())
                .opt("newText", r.new.as_deref())
                .opt("message", r.message.as_deref()),
            Self::Rm(r) => args
                .text("branch", &r.branch)
                .text("path", &r.path)
                .opt("message", r.message.as_deref()),
            Self::Mkdir(r) => args
                .text("branch", &r.branch)
                .text("path", &r.path)
                .int("mode", r.mode)
                .opt("message", r.message.as_deref()),
            Self::Mv(r) | Self::Cp(r) => args
                .text("branch", &r.branch)
                .text("from", &r.from_path)
                .text("to", &r.to_path)
                .opt("message", r.message.as_deref()),
            Self::Apply(r) => args
                .text("branch", &r.branch)
                .opt("message", r.message.as_deref())
                .put("changes", changes(&r.changes)),
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
        ("Conflict", "conflicts", "conflicts"),
    ];
    for (code, held, next) in carried {
        if let (true, Some(value)) = (refusal.code == code, detail.get(held)) {
            refusal = with_next(refusal, next, value.clone());
        }
    }
    refusal
}
