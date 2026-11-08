//! The Store methods the three families run and the one translation of each
//! to its engine method and arguments: snake_case to the engine's camelCase,
//! an absent option not sent. Both routes send these (in this process, or in
//! a `store.call` frame to the Store's worker).
use super::branch::{
    BranchCommits, BranchDiff, BranchFork, BranchGetCommit, BranchMerge, BranchPublish,
};
use super::commit::CommitDiff;
use super::file::{
    FileApply, FileBlame, FileCp, FileDiff, FileEdit, FileGlob, FileGrep, FileList, FileMkdir,
    FileMv, FileRead, FileRevision, FileRm, FileStat, FileWrite, Side,
};
use crate::call::store_call;
use crate::resolve::target;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::outcome::{refused, Outcome};
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

/// A diff's options.
#[derive(Clone, Debug, Default)]
pub struct DiffOptions {
    /// Its page.
    pub page: Page,
    /// `cursor`.
    pub cursor: Option<String>,
    /// `paths_only`: whether it leaves out hunks.
    pub paths_only: Option<bool>,
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
    /// `project.list_branches`: the Branch `name`, or one page.
    ListBranches {
        /// The one Branch it lists.
        name: Option<&'a str>,
        /// The page.
        page: Page,
    },
    /// `branch.list_commits`: one page of a Branch's Commits.
    ListCommits {
        /// The Branch.
        branch: &'a str,
        /// The page.
        page: Page,
    },
    /// `branch.commits`: pinned saved history, including main Layers.
    BranchCommits(&'a BranchCommits),
    /// `branch.get_commit`: one saved record in its branch context.
    BranchGetCommit(&'a BranchGetCommit),
    /// `fs.revision`: saved changes to one path.
    Revision(&'a FileRevision),
    /// `branch.fork`.
    Fork(&'a BranchFork),
    /// `branch.merge`.
    Merge(&'a BranchMerge),
    /// `branch.publish`.
    Publish(&'a BranchPublish),
    /// `branch.diff_branches`.
    DiffBranches(&'a BranchDiff),
    /// `branch.diff_commits`.
    DiffCommits(&'a CommitDiff),
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

    /// A diff's page and whether it carries hunks (`paths_only` is its
    /// opposite).
    fn diff(&mut self, options: &DiffOptions) -> &mut Self {
        self.page(options.page)
            .opt("cursor", options.cursor.as_deref());
        match options.paths_only {
            Some(paths_only) => self.put("hunks", SdkValue::Bool(!paths_only)),
            None => self,
        }
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
            Self::ListBranches { .. } => "project.list_branches",
            Self::ListCommits { .. } => "branch.list_commits",
            Self::BranchCommits(_) => "branch.commits",
            Self::BranchGetCommit(_) => "branch.get_commit",
            Self::Revision(_) => "fs.revision",
            Self::Fork(_) => "branch.fork",
            Self::Merge(_) => "branch.merge",
            Self::Publish(_) => "branch.publish",
            Self::DiffBranches(_) => "branch.diff_branches",
            Self::DiffCommits(_) => "branch.diff_commits",
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
            Self::ListBranches { name, page } => args.opt("name", *name).page(*page),
            Self::ListCommits { branch, page } => args.text("branch", branch).page(*page),
            Self::BranchCommits(r) => args
                .text("branch", &r.branch)
                .opt("atCommit", r.at_commit.as_deref())
                .int("page", r.page)
                .int("pageSize", r.page_size),
            Self::BranchGetCommit(r) => args.text("branch", &r.branch).text("commit", &r.commit),
            Self::Revision(r) => args
                .at(&r.branch, r.at_commit.as_deref())
                .text("path", &r.path)
                .opt("traversal", r.traversal.as_deref())
                .page(r.page),
            Self::Fork(r) => {
                let from = r.from_commit.as_deref().unwrap_or(&r.from_branch);
                args.text("name", &r.name).text("from", from)
            }
            Self::Merge(r) => args
                .text("branch", &r.a)
                .text("with", &r.b)
                .text("name", &r.name)
                .opt("message", r.message.as_deref())
                .json("units", r.units.as_ref()),
            Self::Publish(r) => args
                .text("branch", &r.branch)
                .opt("commit", r.commit.as_deref()),
            Self::DiffBranches(r) => args
                .text("from", &r.from_branch)
                .text("to", &r.to_branch)
                .opt("fromCommit", r.from_commit.as_deref())
                .opt("toCommit", r.to_commit.as_deref())
                .opt("pathPrefix", r.path_prefix.as_deref())
                .boolean("includeAttribution", r.include_attribution)
                .diff(&r.options),
            Self::DiffCommits(r) => args
                .text("from", &r.from_commit)
                .text("to", &r.to_commit)
                .diff(&r.options),
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

/// The items of one list page and the position of the next page, `None`
/// after the last. A next position that does not move on is `Failed`:
/// the list cannot go on, and it is not asked again.
pub(super) fn page_of(
    page: &SdkValue,
    pos: i64,
) -> Result<(Vec<SdkValue>, Option<i64>), SdkRefusal> {
    let items = match page.get("items") {
        Some(SdkValue::List(items)) => items.clone(),
        _ => Vec::new(),
    };
    match page.get("nextPagePos") {
        Some(SdkValue::Int(next)) if *next > pos => Ok((items, Some(*next))),
        Some(SdkValue::Int(_)) => Err(refused(
            "Failed",
            format!("a list made no progress at page position {pos}"),
        )),
        _ => Ok((items, None)),
    }
}
