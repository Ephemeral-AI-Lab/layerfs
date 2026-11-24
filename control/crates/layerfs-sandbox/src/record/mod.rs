//! The Sandbox side's records (typed by layerfs-project#108): a Sandbox's
//! `record.json` (`sandbox`), what reopens it (`daemon`, its record's
//! `daemon` field), each Workspace's `workspace/<id>.json` (`workspace`)
//! and the machine-level retired tombstone
//! `machine/<machine>/sandbox-retired/<name>.json` (`tombstone`), written
//! before a close or remove deletes the Sandbox record. Each is typed field
//! by field; its codec, on `layerfs_home::home::fields`, is the only place it
//! meets a value, and keeps the JSON as read.
mod daemon;
mod sandbox;
mod tombstone;
mod workspace;

pub use daemon::{Carrier, DaemonRecord, Forward, Local};
pub use sandbox::{lock, lock_in, mount_place, sandbox, sandboxes, SandboxRecord};
pub use tombstone::{
    drop_retired, retire, retire_in, retire_reported, unretired, SandboxTombstone,
};
pub use workspace::WorkspaceRecord;
