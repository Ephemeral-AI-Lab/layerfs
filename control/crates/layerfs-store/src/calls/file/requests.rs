//! The requests of the `file` family's calls, one per call: its
//! positionals and options, snake_case, as the call table names them.
use super::{Change, Content};

/// `file.read(project, branch, path, { at, line, limit })`.
#[derive(Clone, Debug, Default)]
pub struct FileRead {
    /// The project, `<store>/<project>`.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The Commit read instead of the Branch's head: an id or a prefix.
    pub at: Option<String>,
    /// The file.
    pub path: String,
    /// The first line, 1-based.
    pub line: Option<i64>,
    /// The most lines, 1 to 2000.
    pub limit: Option<i64>,
}

/// `file.stat(project, branch, path, { at })`.
#[derive(Clone, Debug, Default)]
pub struct FileStat {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The Commit read instead of the Branch's head.
    pub at: Option<String>,
    /// The path.
    pub path: String,
}

/// `file.list(project, branch, path, { at, limit, cursor })`.
#[derive(Clone, Debug, Default)]
pub struct FileList {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The Commit read instead of the Branch's head.
    pub at: Option<String>,
    /// The folder.
    pub path: String,
    /// The most rows in one page.
    pub limit: Option<i64>,
    /// The `next_cursor` of the page before.
    pub cursor: Option<String>,
}

/// `file.glob(project, branch, pattern, path, { exclude, at, limit,
/// cursor })` and `file.grep(project, branch, pattern, path, { include,
/// exclude, ignore_case, at, limit, cursor })`.
#[derive(Clone, Debug, Default)]
pub struct FileSearch {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The glob, or `file.grep`'s literal text.
    pub pattern: String,
    /// The folder searched; `/` when absent.
    pub path: Option<String>,
    /// Only paths this glob matches (`file.grep`).
    pub include: Option<String>,
    /// No path this glob matches.
    pub exclude: Option<String>,
    /// Letters of either case match (`file.grep`).
    pub ignore_case: bool,
    /// The Commit read instead of the Branch's head.
    pub at: Option<String>,
    /// The most rows in one page.
    pub limit: Option<i64>,
    /// The `next_cursor` of the page before.
    pub cursor: Option<String>,
}

/// `file.glob`'s request.
pub type FileGlob = FileSearch;
/// `file.grep`'s request.
pub type FileGrep = FileSearch;

/// `file.history(project, branch, path, { at, traversal, limit, cursor })`.
#[derive(Clone, Debug, Default)]
pub struct FileHistory {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The path, which may be absent at the head.
    pub path: String,
    /// The Commit the history starts at instead of the head.
    pub at: Option<String>,
    /// `provenance` or `branch`.
    pub traversal: Option<String>,
    /// The most rows in one page.
    pub limit: Option<i64>,
    /// The `next_cursor` of the page before.
    pub cursor: Option<String>,
}

/// `file.blame(project, branch, path, { at, line, limit })`.
#[derive(Clone, Debug, Default)]
pub struct FileBlame {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The Commit read instead of the Branch's head.
    pub at: Option<String>,
    /// The file.
    pub path: String,
    /// The first line, 1-based.
    pub line: Option<i64>,
    /// The most lines, 1 to 2000.
    pub limit: Option<i64>,
}

/// What every write takes besides its own arguments.
#[derive(Clone, Debug, Default)]
pub struct Guard {
    /// The head the Branch must be at: an id or a prefix.
    pub expected: Option<String>,
    /// The Commit message.
    pub message: Option<String>,
}

/// `file.write(project, branch, path, content, { mode, expected, message })`.
#[derive(Clone, Debug)]
pub struct FileWrite {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The file.
    pub path: String,
    /// What it holds.
    pub content: Content,
    /// Its permission bits in octal text, `0644`.
    pub mode: Option<String>,
    /// The head guard and message.
    pub guard: Guard,
}

/// `file.edit(project, branch, path, { old_string, new_string,
/// replace_all, expected, message })`.
#[derive(Clone, Debug, Default)]
pub struct FileEdit {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The file.
    pub path: String,
    /// The text replaced.
    pub old_string: String,
    /// Its replacement.
    pub new_string: String,
    /// Every occurrence instead of exactly one.
    pub replace_all: bool,
    /// The head guard and message.
    pub guard: Guard,
}

/// `file.rm(project, branch, path, { recursive, expected, message })`.
#[derive(Clone, Debug, Default)]
pub struct FileRm {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The path.
    pub path: String,
    /// A folder with everything in it.
    pub recursive: bool,
    /// The head guard and message.
    pub guard: Guard,
}

/// `file.mkdir(project, branch, path, { mode, expected, message })`.
#[derive(Clone, Debug, Default)]
pub struct FileMkdir {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The folder.
    pub path: String,
    /// Its permission bits in octal text, `0755`.
    pub mode: Option<String>,
    /// The head guard and message.
    pub guard: Guard,
}

/// `file.mv(project, branch, from, to, { expected, message })`; `file.cp`
/// takes the same.
#[derive(Clone, Debug, Default)]
pub struct FileMv {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The path moved.
    pub from: String,
    /// Where to.
    pub to: String,
    /// The head guard and message.
    pub guard: Guard,
}

/// `file.cp`'s request.
pub type FileCp = FileMv;

/// `file.apply(project, branch, changes, { expected, message })`.
#[derive(Clone, Debug, Default)]
pub struct FileApply {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The changes, in order.
    pub changes: Vec<Change>,
    /// The head guard and message.
    pub guard: Guard,
}
