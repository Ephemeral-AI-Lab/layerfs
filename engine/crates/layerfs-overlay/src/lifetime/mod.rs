//! Responsibility-scoped implementation modules and reexports.
pub(crate) mod captured_reader;
pub(crate) mod close;
pub(crate) mod composition;
pub(crate) mod file_owners;
pub(crate) mod frontier;
pub(crate) mod generation;
pub(crate) mod orphan;
pub(crate) mod source;
pub(crate) mod tickets;
pub(crate) mod workspace;

mod indexed_operation_record;
mod operation;

mod kernel;
mod kernel_cookie;
mod kernel_directory;
mod lookup;
pub(crate) use kernel_directory::INLINE_PAGES;
mod kernel_directory_read;
mod kernel_file;
mod kernel_mutation;
mod kernel_visit;
