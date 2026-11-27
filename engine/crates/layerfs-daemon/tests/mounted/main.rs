//! Linux FUSE mount tests of the daemon's `mounted` area, one module per topic.
#![cfg(target_os = "linux")]
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/complete/bytes.rs"]
mod bytes;
mod drain;
mod execute;
mod failure_scope;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/complete/fixture.rs"]
mod fixture;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/kernel/fusectl.rs"]
mod fusectl;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/gate/held_writer.rs"]
mod held_writer;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/installed_store.rs"]
mod installed;
mod links;
mod listing_reads;
mod low_water;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/rigs/namespace_model.rs"]
mod model;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/mounted.rs"]
mod mounted;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/mutating.rs"]
mod mutating;
mod patterns;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/mounted_commit.rs"]
mod rig;
mod sequential_read;
mod serial_admission;
mod sqlite_full;
mod storage_reads;
mod store_read;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/install/host.rs"]
mod support;
