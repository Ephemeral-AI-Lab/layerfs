//! Functional finite-arrival proof, using the same external workflow as the observer.
#[path = "../examples/engine_finite/driver.rs"]
mod driver;
#[allow(dead_code)]
#[path = "../examples/e2_startup/json.rs"]
mod json;
#[path = "../examples/engine_finite/receipt.rs"]
mod receipt;
#[allow(dead_code)]
#[path = "../examples/e2_startup/records.rs"]
mod records;
#[test]
fn every_class_and_workspace_completes_its_finite_arrivals() {
    let directory =
        std::env::temp_dir().join(format!("layerfs-finite-engine-{}", std::process::id()));
    let receipts = std::env::var_os("LAYERFS_ENGINE_RECEIPTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| directory.with_extension("jobs.jsonl"));
    driver::run(&directory, &receipts, |_| {});
    println!(
        "ENGINE_RETAINED receipts={} database={}",
        receipts.display(),
        directory.display()
    );
}
