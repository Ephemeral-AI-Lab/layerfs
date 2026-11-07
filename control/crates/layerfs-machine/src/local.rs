//! Whether a machine's calls run in this process (G8; moved from the Store
//! placement, layerfs-project#105).
use layerfs_bridge::control::SdkRefusal;
use std::path::Path;

/// Whether machine `machine`'s calls run in this process: its platform's
/// executor on this host (G8).
#[cfg(feature = "machine")]
pub fn runs_here(home: &Path, machine: &str) -> Result<bool, SdkRefusal> {
    use layerfs_platform::{ExecutorKind, HostOs, Platform};
    let platform = match machine {
        "native" => Platform::Native,
        _ => crate::record::MachineRecord::of(home, machine)?.platform()?,
    };
    Ok(platform.executor(HostOs::current()) == ExecutorKind::Local)
}

/// Without the machine adapters (the guest worker's build): `native` is
/// this process; any other machine's worker is the one its `worker.json`
/// records.
#[cfg(not(feature = "machine"))]
pub fn runs_here(_: &Path, machine: &str) -> Result<bool, SdkRefusal> {
    Ok(machine == "native")
}
