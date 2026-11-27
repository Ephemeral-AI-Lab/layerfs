//! Whether a machine's calls run in this process (G8; moved from the Store
//! placement, layerfs-project#105), and the other facts behaviour asks a
//! machine's platform rather than its name. This computer (the host) is
//! the control process, never a machine: `native` is one built-in machine
//! whose platform runs Sandboxes on the host's own OS, and what is special
//! about it comes from that platform on this host.
use layerfs_protocol::sdk::SdkRefusal;
use std::path::Path;

/// Whether machine `machine`'s calls run in this process: its platform's
/// executor on this host (G8).
#[cfg(feature = "machine")]
pub fn runs_here(home: &Path, machine: &str) -> Result<bool, SdkRefusal> {
    use crate::platform::{ExecutorKind, HostOs, Platform};
    let platform = match machine {
        layerfs_home::names::HOST_MACHINE if in_process().is_some() => Platform::Host,
        _ => crate::record::MachineRecord::of(home, machine)?.platform()?,
    };
    Ok(platform.executor(HostOs::current()) == ExecutorKind::Local)
}

/// Without the machine adapters (the guest worker's build): `native` is
/// this process; any other machine's worker is the one its `worker.json`
/// records.
#[cfg(not(feature = "machine"))]
pub fn runs_here(_: &Path, machine: &str) -> Result<bool, SdkRefusal> {
    Ok(in_process() == Some(machine))
}

/// The machine whose calls run in this process on this host: the built-in
/// machine whose platform's executor is local here (`native` on macOS and
/// Linux); none on a host where every machine has a worker (Windows). A
/// created machine always has a worker. A built-in machine's platform has
/// its name.
#[cfg(feature = "machine")]
pub fn in_process() -> Option<&'static str> {
    layerfs_home::names::BUILT_IN
        .into_iter()
        .find(|name| *name == layerfs_home::names::HOST_MACHINE)
}

/// Without the machine adapters (the guest worker's build): the guest's
/// own `native`, as [`runs_here`] says.
#[cfg(not(feature = "machine"))]
pub fn in_process() -> Option<&'static str> {
    Some(layerfs_home::names::HOST_MACHINE)
}

/// Refuses `what`, which needs a machine whose calls run in this process,
/// on a host with none ([`in_process`]): `Unsupported`; no other machine
/// is chosen for it.
pub fn none_in_process(what: &str) -> SdkRefusal {
    layerfs_home::outcome::refused(
        "Unsupported",
        format!(
            "{what} needs a machine whose calls run in this process, and on this host ({}) \
             every machine's calls run in its worker",
            std::env::consts::OS
        ),
    )
}

/// Whether a provider holds a machine of platform `platform` (a VM or
/// worker container named by its record's `vm_name`); a platform with none
/// runs Sandboxes on the host's own OS, and its machine is never stopped,
/// never missing and has no provider name. A platform LayerFS does not
/// know (one removed in 2.0) had one.
#[cfg(feature = "machine")]
pub fn provided(platform: &str) -> bool {
    crate::platform::Platform::parse(platform).is_none_or(|p| p.provided())
}

/// Without the machine adapters (the guest worker's build): only the
/// guest's own `native` has no provider.
#[cfg(not(feature = "machine"))]
pub fn provided(platform: &str) -> bool {
    platform != "host"
}
