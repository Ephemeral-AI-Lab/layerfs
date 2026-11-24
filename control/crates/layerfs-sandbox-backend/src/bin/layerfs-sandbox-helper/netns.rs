//! The outer init stage's half of the Sandbox's own network: it is the
//! namespace's holder. Before the inner stage exists (whose `pivot_root`
//! would move this process's root too), it takes the machine's network lock,
//! plans the lowest free slot of the spec's pool, enters a new network
//! namespace, makes the veth pair with the guest end inside it and the host
//! end's address on the machine's side, releases the lock, configures the
//! inside and writes `<sandbox>/network`. It returns the handles of both
//! namespaces, so the caller can start the inner stage (and the daemon) on
//! the machine's network and then hold the Sandbox's. Nothing is retried; a
//! failure ends the stage, and the namespace and the pair end with it; a
//! pool or link the machine no longer gives is said on an `UNAVAILABLE` line
//! first.
use layerfs_sandbox_backend::network::pool::UNAVAILABLE;
use layerfs_sandbox_backend::network::{
    configure, connect, plan_slot, NetworkLock, Routes, Wiring,
};
use layerfs_sandbox_backend::{HelperSpec, RuntimeError};
use nix::sched::{unshare, CloneFlags};
use std::{
    fs::{self, File},
    io::Write,
    os::fd::AsFd,
    os::unix::fs::OpenOptionsExt,
    time::Duration,
};

/// The longest wait for another Sandbox start's slot choice.
const LOCK_WAIT: Duration = Duration::from_secs(10);
/// This process's own network namespace handle.
const OWN: &str = "/proc/self/ns/net";
/// The file in the Sandbox directory the launcher reads the network from.
pub(super) const RECORD: &str = "network";

/// The machine's and the Sandbox's network namespace handles, this process
/// left in the Sandbox's.
pub(super) fn wire(spec: &HelperSpec) -> Result<(File, File), String> {
    let network = spec.network.as_ref().ok_or(
        "the Sandbox's spec names no network (an earlier build wrote it), and no Sandbox's \
         commands run on the machine's network",
    )?;
    let machine_handle = File::open(OWN).map_err(|error| format!("{OWN}: {error}"))?;
    let mut machine =
        Routes::open().map_err(|error| format!("opening the machine's routing socket: {error}"))?;
    let lock = NetworkLock::take(LOCK_WAIT)
        .map_err(|error| format!("taking the machine's network lock: {error}"))?;
    let wiring = match plan_slot(&mut machine, network.pool, &network.link) {
        Ok(wiring) => wiring,
        // the machine changed since the launcher checked it: its log line
        // makes the start `Unavailable`, as the launcher's own check does
        Err(RuntimeError::Unavailable(why)) => {
            eprintln!("{UNAVAILABLE}{}", why.replace('\n', " "));
            return Err(format!("choosing the Sandbox's address: {why}"));
        }
        Err(error) => return Err(format!("choosing the Sandbox's address: {error}")),
    };
    unshare(CloneFlags::CLONE_NEWNET)
        .map_err(|error| format!("creating the Sandbox's network namespace: {error}"))?;
    let sandbox_handle = File::open(OWN).map_err(|error| format!("{OWN}: {error}"))?;
    connect(&mut machine, &wiring, sandbox_handle.as_fd())
        .map_err(|error| format!("making the Sandbox's link {}: {error}", wiring.link))?;
    drop(lock);
    configure(&wiring)
        .map_err(|error| format!("configuring the Sandbox's network inside: {error}"))?;
    record(spec, &wiring)?;
    Ok((machine_handle, sandbox_handle))
}

/// `<sandbox>/network`, 0644: the host end and the guest's address.
fn record(spec: &HelperSpec, wiring: &Wiring) -> Result<(), String> {
    let path = spec.sandbox.join(RECORD);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(&path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    file.write_all(format!("{}\n", wiring.line()).as_bytes())
        .map_err(|error| format!("{}: {error}", path.display()))
}
