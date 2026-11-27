//! The Sandbox side's records (typed by layerfs-project#108): a Sandbox's
//! `record.json` (`sandbox`), what reopens it (`daemon`, its record's
//! `daemon` field), each Workspace's `workspace/<id>.json` (`workspace`)
//! and the machine-level retired tombstone
//! `machine/<machine>/sandbox-retired/<name>.json` (`tombstone`), written
//! before a close or remove deletes the Sandbox record. Each is typed field
//! by field; its codec, on `layerfs_home::record::fields`, is the only place it
//! meets a value, and keeps the JSON as read.
mod daemon;
#[doc(hidden)]
pub mod discovery;
pub mod exec;
#[doc(hidden)]
pub mod reopen;
mod sandbox;
/// What interrupted Sandbox starts and removals left in a run root, removed
/// where nothing can own it (spec v20).
pub mod sweep;
mod tombstone;
#[doc(hidden)]
pub mod unmount_guard;
mod workspace;

pub use daemon::{Carrier, DaemonRecord, Forward, Local};
pub use sandbox::{lock, lock_in, mount_place, sandbox, sandboxes, SandboxRecord};
pub use tombstone::{
    drop_retired, retire, retire_in, retire_reported, unretired, SandboxTombstone,
};
pub use workspace::WorkspaceRecord;
