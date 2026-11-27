//! Checks and in-place maintenance of a Store: `store.compact` and its
//! report, `store.upgrade` and `store.repair_mount_holders`.
pub mod compact;
pub mod compact_report;
pub mod repair_mount_holders;
pub mod upgrade;
