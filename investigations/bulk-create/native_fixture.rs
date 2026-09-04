// Diagnostic preparation only; the measured Linux workload is the sealed helper.
#[allow(dead_code)]
#[path = "../../benchmark/fs-bench-pro/workload.rs"]
mod workload;
fn main() -> workload::Result<()> {
    let root = std::env::args().nth(1).ok_or("fixture directory")?;
    let case = workload::workspace_registry::resolve("tiny-bulk-create-500")?;
    let entries = workload::workspace_registry::fixture(&case, 1)?;
    workload::workspace_common::create_fixture(std::path::Path::new(&root), &entries)?;
    print!("{}", workload::workspace_common::manifest(&entries)?);
    Ok(())
}
