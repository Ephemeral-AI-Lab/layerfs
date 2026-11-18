//! The `file` family: a project's files at a Branch or Commit, each call
//! translated to the `fs.*` Store method that does it, its answer the
//! call's full map (the keys of its long form; the dispatcher shapes it).
//! Each change is one Commit. A paged read's cursor carries its filters
//! and the Commit its first page read.
mod read;
mod requests;
mod write;

pub use read::{blame, glob, grep, history, list, read, stat};
pub use requests::{
    FileApply, FileBlame, FileCp, FileEdit, FileGlob, FileGrep, FileHistory, FileList, FileMkdir,
    FileMv, FileRead, FileRm, FileSearch, FileStat, FileWrite, Guard,
};
pub use write::{apply, cp, edit, mkdir, mv, rm, write};

use layerfs_bridge::control::SdkValue;

/// One `fs.*` Store method with its engine arguments (camelCase; an absent
/// value is not sent), as a file call built them.
#[derive(Clone, Debug)]
pub struct FileOp {
    /// The engine method.
    pub method: &'static str,
    /// Its arguments, without the project.
    pub args: Vec<(String, SdkValue)>,
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
