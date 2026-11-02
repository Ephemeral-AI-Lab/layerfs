//! `layerfs worker` (H §7.2, #533 step 15): a machine's worker, started by
//! the host inside the machine with its settings in its environment, which
//! LayerFS itself sets: `LAYERFS_WORKER_LISTEN` (address), and
//! `LAYERFS_WORKER_SECRET` (a 0600 file holding the secret). It serves its
//! root, the machine's data disk `/layerfs/data` (internal
//! `LAYERFS_WORKER_ROOT` names another, for tests), until it has been idle
//! for [`IDLE`] (O2), then exits; with `LAYERFS_WORKER_POWEROFF=1` (a VM
//! guest) it then powers the guest off, and with `reboot` (a Firecracker
//! microVM, whose VMM exits on a reset and ignores an ACPI power-off) it
//! syncs and resets the guest.
use layerfs_api::worker::{serve_until_idle, IDLE};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

/// The machine's data disk, where every guest keeps its Stores.
const ROOT: &str = "/layerfs/data";
/// How often the worker looks whether it is idle.
const LOOK: Duration = Duration::from_secs(5);

fn setting(name: &str) -> Result<String, String> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("{name} is not set; the host starts the worker with it"))
}

fn serve() -> Result<(), String> {
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
    eprintln!("layerfs worker: listening on {listen}, root {root}");
    serve_until_idle(listener, secret, PathBuf::from(root), IDLE, LOOK)
        .map_err(|why| format!("the worker stopped: {why}"))?;
    eprintln!("layerfs worker: idle for {} s; stopping", IDLE.as_secs());
    Ok(())
}

/// Serves, then powers the guest off when asked to.
pub fn main() -> ExitCode {
    if let Err(why) = serve() {
        eprintln!("layerfs worker: {why}");
        return ExitCode::from(crate::run::REFUSED);
    }
    let argv: &[&str] = match std::env::var("LAYERFS_WORKER_POWEROFF").as_deref() {
        Ok("1") => &["poweroff"],
        Ok("reboot") => &["sh", "-c", "sync; reboot -f"],
        _ => &[],
    };
    if let Some((program, args)) = argv.split_first() {
        match std::process::Command::new(program).args(args).status() {
            Ok(status) if status.success() => {}
            Ok(status) => eprintln!("layerfs worker: {argv:?} exited {status}"),
            Err(why) => eprintln!("layerfs worker: {argv:?} failed: {why}"),
        }
    }
    ExitCode::SUCCESS
}
