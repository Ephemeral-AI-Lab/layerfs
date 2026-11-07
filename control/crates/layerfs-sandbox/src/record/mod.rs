//! The Sandbox side's records (typed by layerfs-project#108): a Sandbox's
//! `record.json` (`sandbox`), what reopens it (`daemon`, its record's
//! `daemon` field) and each Workspace's `workspace/<id>.json`
//! (`workspace`). Each is typed field by field; its codec, on
//! `layerfs_home::home::fields`, is the only place it meets a value, and
//! keeps the JSON as read.
mod daemon;
mod sandbox;
mod workspace;

pub use daemon::{Carrier, DaemonRecord, Forward, Local};
pub use sandbox::{lock, lock_in, sandbox, sandboxes, SandboxRecord};
pub use workspace::WorkspaceRecord;
