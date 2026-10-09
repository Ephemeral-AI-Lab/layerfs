//! Responsibility-scoped implementation modules and reexports.
pub(crate) mod captured_reader;
pub(crate) mod close;
pub(crate) mod composition;
pub(crate) mod file_owners;
pub(crate) mod frontier;
pub(crate) mod generation;
pub(crate) mod operation_record;
pub(crate) mod orphan;
pub(crate) mod source;
pub(crate) mod tickets;
pub(crate) mod workspace;

mod indexed_operation_record;
mod operation;

mod lookup;
mod native;
mod native_cookie;
mod native_directory;
mod native_directory_read;
mod native_file;
mod native_mutation;
mod native_observation;
mod native_visit;
