#[allow(dead_code)]
#[path = "../support/installed_store.rs"]
mod installed;
#[allow(dead_code)]
#[path = "../support/rigs/namespace_model.rs"]
mod model;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/mounted.rs"]
mod mounted;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/mounted_commit.rs"]
mod rig;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../support/install/host.rs"]
mod support;
/// The Store service's in-process SDK fixture over its own installed Store;
/// `sdk.rs` reaches its sibling `installed`, so the pair keeps one parent.
#[path = "../../../layerfs-store-service/tests/support"]
mod sdk {
    #[allow(dead_code)]
    #[path = "installed_store.rs"]
    pub mod installed;
    #[allow(dead_code)]
    #[path = "sdk.rs"]
    pub mod support;
}
mod commit;
mod concurrent_commits;
mod dirty_mapping;
mod file_digests;
mod lane_merge;
mod merge_report;
mod postgres_unreachable;
mod producer_profile;
mod reader_lifecycle;
mod shared_cache_pressure;
