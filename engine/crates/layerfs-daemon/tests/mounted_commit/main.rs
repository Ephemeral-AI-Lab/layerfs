//! Linux FUSE mount tests of the daemon's `mounted_commit` area, one module per topic.
#![cfg(target_os = "linux")]
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/complete/bytes.rs"]
mod bytes;
mod changed_activity;
mod commit;
mod failures;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/complete/fixture.rs"]
mod fixture;
mod install;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/installed_store.rs"]
mod installed;
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
mod orphan;
mod parallel;
mod recognizer;
mod rewind;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/mounted_commit.rs"]
mod rig;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/install/host.rs"]
mod support;
