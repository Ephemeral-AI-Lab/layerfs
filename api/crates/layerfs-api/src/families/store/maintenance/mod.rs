//! The Store calls that keep a Store in order: compact, upgrade and the
//! repair of its mount holders.
pub(super) mod compact;
pub(super) mod repair_mount_holders;
pub(super) mod upgrade;
