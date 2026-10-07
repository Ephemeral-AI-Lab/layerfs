//! Daemon service ownership. SQL stays in the independently usable overlay crate.
//!
//! Initial service runs bounded typed jobs fairly and retains queue/reply credits.
//! Provisioned authenticated upstreams compose the existing consumer/Workspace
//! ports. Native FUSE and process/control remain separate integration slices.
#![forbid(unsafe_code)]

pub mod bootstrap;
mod overlay;
mod service;
pub mod store;
/// Authenticated provisioned host binding and fresh operation consumers.
pub mod upstream;
pub(crate) use overlay::commands;
pub(crate) use overlay::credits;
pub(crate) use overlay::owner;
pub(crate) use overlay::queue;

pub use commands::{Command, Response, ServiceClass};
pub use overlay::indexed_operation_record::{
    IndexedOperationRecordJob, IndexedOperationRecordReply,
};
pub use owner::{Owner, OwnerClient, OwnerConfig, OwnerError};
pub use queue::OwnerWork;
pub use service::completion::{Completion, Pending};
pub use service::job_sql::JobSql;
pub use service::observations::JobWork;
pub use service::startup::OwnerStart;
