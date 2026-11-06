//! `layerfs worker` (H §7.2, #533 step 15): a machine's worker, started by
//! the host inside the machine with its settings in its environment, which
//! LayerFS itself sets: `LAYERFS_WORKER_LISTEN` (address), and
//! `LAYERFS_WORKER_SECRET` (a 0600 file holding the secret). It serves its
//! root, the machine's data disk `/layerfs/data` (internal
//! `LAYERFS_WORKER_ROOT` names another, for tests), until `machine.stop`
//! ends it. It never stops by itself (#535 D11: lifecycle is manual).
use layerfs_api::worker::serve;
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
    crate::output::diagnostic(&format!(
        "layerfs worker: listening on {listen}, root {root}"
    ));
    serve(listener, secret, PathBuf::from(root))
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
