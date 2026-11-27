#[allow(dead_code)]
#[path = "../../src/home/bucket.rs"]
mod bucket;
#[path = "../support/doctor_own.rs"]
mod doctor_own;
#[path = "../../../layerfs-store/tests/support/doctor_s3.rs"]
mod peer;
/// `crate::doctor::budget`, as `src/home/bucket.rs` (included by the
/// bucket tests) names it.
mod doctor {
    pub mod budget {
        pub use layerfs_inspect::doctor::budget::{Budget, Checks};
    }
}
mod backup_target;
mod fixes_sandbox_backup;
mod fixes_store;
mod frame;
mod home_layout;
mod host;
mod host_legacy_home;
mod live_runtime;
mod records;
mod report;
