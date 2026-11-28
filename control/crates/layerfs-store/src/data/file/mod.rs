//! The `file` family: a project's files at a Branch or Commit, each call
//! translated to the `file.*` Store method that does it, its answer the
//! call's full map (the keys of its long form; the dispatcher shapes it).
//! Each change is one Commit. A paged read's cursor carries its filters
//! and the Commit its first page read. One file per call; `read.rs` and
//! `write.rs` also hold what the reads and the writes share.
mod apply;
mod blame;
mod cp;
mod edit;
mod glob;
mod grep;
mod history;
mod list;
mod mkdir;
mod mv;
mod read;
mod requests;
mod rm;
mod stat;
mod write;

pub use apply::apply;
pub(crate) use apply::owned_change;
pub use blame::blame;
pub use cp::cp;
pub use edit::edit;
pub use glob::glob;
pub use grep::grep;
pub use history::history;
pub use list::list;
pub use mkdir::mkdir;
pub use mv::mv;
pub use read::read;
pub use requests::{Change, Content};
pub use requests::{
    FileApply, FileBlame, FileCp, FileEdit, FileGlob, FileGrep, FileHistory, FileList, FileMkdir,
    FileMv, FileOp, FileRead, FileRm, FileSearch, FileStat, FileWrite, Guard,
};
pub use rm::rm;
pub use stat::stat;
pub use write::write;
