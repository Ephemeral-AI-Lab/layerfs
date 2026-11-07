//! The `file` family: a project's files at a Branch or Commit, each call
//! translated to the Store method that does it; each change is one Commit.
use super::op::{on_project, Change, Content, Page, StoreOp};
use layerfs_home::events::Events;
use layerfs_home::outcome::Outcome;
use std::path::Path;

/// `file.read(project, branch, path, commit, { start_line, end_line })`.
#[derive(Clone, Debug, Default)]
pub struct FileRead {
    /// The project, `<store>/<project>`.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The Commit, read instead of the Branch's head.
    pub commit: Option<String>,
    /// The file.
    pub path: String,
    /// The first line.
    pub start_line: Option<i64>,
    /// The last line.
    pub end_line: Option<i64>,
}

/// `file.stat(project, branch, path, commit)`.
#[derive(Clone, Debug, Default)]
pub struct FileStat {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The Commit, read instead of the Branch's head.
    pub commit: Option<String>,
    /// The path.
    pub path: String,
}

/// `file.list(project, branch, path, commit, { page_size, page_pos })`.
#[derive(Clone, Debug, Default)]
pub struct FileList {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The Commit, read instead of the Branch's head.
    pub commit: Option<String>,
    /// The folder.
    pub path: String,
    /// The page.
    pub page: Page,
}

/// `file.glob(project, branch, path, pattern, commit, { page_size,
/// page_pos })`; `file.grep` takes the same.
#[derive(Clone, Debug, Default)]
pub struct FileGlob {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The Commit, read instead of the Branch's head.
    pub commit: Option<String>,
    /// The folder searched.
    pub path: String,
    /// The glob, or `file.grep`'s regular expression.
    pub pattern: String,
    /// The page.
    pub page: Page,
}

/// `file.grep(project, branch, path, pattern, commit, { page_size,
/// page_pos })`.
pub type FileGrep = FileGlob;

/// `file.blame(project, branch, path, commit, { from_line })`.
#[derive(Clone, Debug, Default)]
pub struct FileBlame {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The Commit, read instead of the Branch's head.
    pub commit: Option<String>,
    /// The file.
    pub path: String,
    /// The first line.
    pub from_line: Option<i64>,
}

/// One side of `file.diff`: a path at a Commit, else at a Branch's head.
#[derive(Clone, Debug, Default)]
pub struct Side {
    /// The Branch.
    pub branch: Option<String>,
    /// The Commit, which takes precedence.
    pub commit: Option<String>,
    /// The path.
    pub path: Option<String>,
}

/// `file.diff(project, left_branch, left_path, right_branch, right_path, {
/// left_commit, right_commit })`.
#[derive(Clone, Debug, Default)]
pub struct FileDiff {
    /// The project.
    pub project: String,
    /// The left side.
    pub left: Side,
    /// The right side.
    pub right: Side,
}

/// `file.write(project, branch, path, content, { message, mode })`.
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
    /// The Commit message.
    pub message: Option<String>,
    /// Its mode.
    pub mode: Option<i64>,
}

/// `file.edit(project, branch, path, { old, new, message })`.
#[derive(Clone, Debug, Default)]
pub struct FileEdit {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The file.
    pub path: String,
    /// The text replaced.
    pub old: Option<String>,
    /// Its replacement.
    pub new: Option<String>,
    /// The Commit message.
    pub message: Option<String>,
}

/// `file.rm(project, branch, path, { message })`.
#[derive(Clone, Debug, Default)]
pub struct FileRm {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The path.
    pub path: String,
    /// The Commit message.
    pub message: Option<String>,
}

/// `file.mkdir(project, branch, path, { message, mode })`.
#[derive(Clone, Debug, Default)]
pub struct FileMkdir {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The folder.
    pub path: String,
    /// The Commit message.
    pub message: Option<String>,
    /// Its mode.
    pub mode: Option<i64>,
}

/// `file.mv(project, branch, from_path, to_path, { message })`; `file.cp`
/// takes the same.
#[derive(Clone, Debug, Default)]
pub struct FileMv {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The path moved.
    pub from_path: String,
    /// Where to.
    pub to_path: String,
    /// The Commit message.
    pub message: Option<String>,
}

/// `file.cp(project, branch, from_path, to_path, { message })`.
pub type FileCp = FileMv;

/// `file.apply(project, branch, changes, { message })`.
#[derive(Clone, Debug, Default)]
pub struct FileApply {
    /// The project.
    pub project: String,
    /// The Branch.
    pub branch: String,
    /// The changes, in order.
    pub changes: Vec<Change>,
    /// The Commit message.
    pub message: Option<String>,
}

/// `file.read`: a file's text or bytes.
pub fn read(home: &Path, events: &mut Events<'_>, request: &FileRead) -> Outcome {
    on_project(home, events, &request.project, StoreOp::Read(request))
}

/// `file.stat`: one path's metadata.
pub fn stat(home: &Path, events: &mut Events<'_>, request: &FileStat) -> Outcome {
    on_project(home, events, &request.project, StoreOp::Stat(request))
}

/// `file.list`: one page of a folder's entries.
pub fn list(home: &Path, events: &mut Events<'_>, request: &FileList) -> Outcome {
    on_project(home, events, &request.project, StoreOp::List(request))
}

/// `file.glob`: the paths a pattern matches.
pub fn glob(home: &Path, events: &mut Events<'_>, request: &FileGlob) -> Outcome {
    on_project(home, events, &request.project, StoreOp::Glob(request))
}

/// `file.grep`: the lines a pattern matches.
pub fn grep(home: &Path, events: &mut Events<'_>, request: &FileGrep) -> Outcome {
    on_project(home, events, &request.project, StoreOp::Grep(request))
}

/// `file.blame`: the Commit of each line of a file.
pub fn blame(home: &Path, events: &mut Events<'_>, request: &FileBlame) -> Outcome {
    on_project(home, events, &request.project, StoreOp::Blame(request))
}

/// `file.diff`: the changes between two files.
pub fn diff(home: &Path, events: &mut Events<'_>, request: &FileDiff) -> Outcome {
    on_project(home, events, &request.project, StoreOp::DiffFiles(request))
}

/// `file.write`: a file written, as one Commit.
pub fn write(home: &Path, events: &mut Events<'_>, request: &FileWrite) -> Outcome {
    on_project(home, events, &request.project, StoreOp::Write(request))
}

/// `file.edit`: one text replaced in a file, as one Commit.
pub fn edit(home: &Path, events: &mut Events<'_>, request: &FileEdit) -> Outcome {
    on_project(home, events, &request.project, StoreOp::Edit(request))
}

/// `file.rm`: a file or folder removed, as one Commit.
pub fn rm(home: &Path, events: &mut Events<'_>, request: &FileRm) -> Outcome {
    on_project(home, events, &request.project, StoreOp::Rm(request))
}

/// `file.mkdir`: a folder made, as one Commit.
pub fn mkdir(home: &Path, events: &mut Events<'_>, request: &FileMkdir) -> Outcome {
    on_project(home, events, &request.project, StoreOp::Mkdir(request))
}

/// `file.mv`: a file moved, as one Commit.
pub fn mv(home: &Path, events: &mut Events<'_>, request: &FileMv) -> Outcome {
    on_project(home, events, &request.project, StoreOp::Mv(request))
}

/// `file.cp`: a file copied, as one Commit.
pub fn cp(home: &Path, events: &mut Events<'_>, request: &FileCp) -> Outcome {
    on_project(home, events, &request.project, StoreOp::Cp(request))
}

/// `file.apply`: several changes as one Commit.
pub fn apply(home: &Path, events: &mut Events<'_>, request: &FileApply) -> Outcome {
    on_project(home, events, &request.project, StoreOp::Apply(request))
}
