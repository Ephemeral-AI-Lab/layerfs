//! `layerfs worker` (H §7.2, #533 step 15): a machine's worker, started by
//! the host inside the machine with its settings in its environment, which
//! LayerFS itself sets: `LAYERFS_WORKER_LISTEN` (address),
//! `LAYERFS_WORKER_SECRET` (a 0600 file holding the secret) and
//! `LAYERFS_WORKER_INCARNATION` (the ordered value the host persisted before
//! launching it; absent only for a worker an older host started). It serves its
//! root, the machine's data disk `/layerfs/data` (internal
//! `LAYERFS_WORKER_ROOT` names another, for tests), until `machine.stop`
//! ends it. As it starts it prepares the machine for its Sandboxes' own
//! networks (`prepare_network`): its port closed to their commands when it
//! listens on a non-loopback address, and the firewall table written. It never stops by itself (#535 D11: lifecycle is manual).
use layerfs_api::worker::{prepare_network, serve_with_incarnation};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::ExitCode;

/// The machine's data disk, where every guest keeps its Stores.
const ROOT: &str = "/layerfs/data";

fn setting(name: &str) -> Result<String, String> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("{name} is not set; the host starts the worker with it"))
}

fn run() -> Result<(), String> {
    let listen = setting("LAYERFS_WORKER_LISTEN")?;
    let secret_file = setting("LAYERFS_WORKER_SECRET")?;
    let secret = std::fs::read_to_string(&secret_file)
        .map_err(|why| format!("cannot read the secret file {secret_file}: {why}"))?
        .trim()
        .to_owned();
    if secret.is_empty() {
        return Err(format!("the secret file {secret_file} is empty"));
    }
    let listener =
        TcpListener::bind(&listen).map_err(|why| format!("cannot listen on {listen}: {why}"))?;
    let root = setting("LAYERFS_WORKER_ROOT").unwrap_or_else(|_| ROOT.to_owned());
    // an empty setting is an old host: the worker mints its own value
    let incarnation = std::env::var("LAYERFS_WORKER_INCARNATION")
        .ok()
        .filter(|value| !value.is_empty());
    crate::output::diagnostic(&format!(
        "layerfs worker: listening on {listen}, root {root}"
    ));
    // a listener on a non-loopback address is closed to Sandboxes' commands
    let address = listener
        .local_addr()
        .map_err(|why| format!("cannot read the address of {listen}: {why}"))?;
    crate::output::diagnostic(&format!("layerfs worker: {}", prepare_network(address)));
    serve_with_incarnation(listener, secret, PathBuf::from(root), incarnation)
        .map_err(|why| format!("the worker stopped: {why}"))?;
    Ok(())
}

/// Serves until the process is ended or its listener fails.
pub fn main() -> ExitCode {
    if let Err(why) = run() {
        crate::output::diagnostic(&format!("layerfs worker: {why}"));
        return ExitCode::from(crate::run::REFUSED);
    }
    ExitCode::SUCCESS
}
