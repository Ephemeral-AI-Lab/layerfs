//! Named external macOS host for the uncontrolled E04 Linux consumer.
#[path = "e2_writes/control.rs"]
#[allow(dead_code)] // Shared host and consumer lifecycle sides.
mod control;
#[path = "../../../benchmark/fs-bench-pro-storage-content/src/workload/digest.rs"]
#[allow(dead_code, clippy::needless_range_loop)]
mod digest;
#[path = "e2_writes/fixture.rs"]
#[allow(dead_code)]
mod fixture;
#[cfg(target_os = "macos")]
#[path = "e2_writes/host.rs"]
mod host;
#[path = "e2_startup/json.rs"]
#[allow(dead_code)]
mod json;

fn main() {
    #[cfg(target_os = "macos")]
    host::run();
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("E04_HOST_FAILED supported global provider requires macOS");
        std::process::exit(2);
    }
}
