//! The `file` family: a project's files at a Branch or Commit, each call
//! translated to the `fs.*` Store method that does it, its answer the
//! call's full map (the keys of its long form; the dispatcher shapes it).
//! Each change is one Commit. A paged read's cursor carries its filters
//! and the Commit its first page read.
mod read;
mod requests;
mod write;

pub use read::{blame, glob, grep, history, list, read, stat};
pub use requests::{Change, Content};
pub use requests::{
    FileApply, FileBlame, FileCp, FileEdit, FileGlob, FileGrep, FileHistory, FileList, FileMkdir,
    FileMv, FileOp, FileRead, FileRm, FileSearch, FileStat, FileWrite, Guard,
};
pub(crate) use write::change as store_change;
pub use write::{apply, cp, edit, mkdir, mv, rm, write};
