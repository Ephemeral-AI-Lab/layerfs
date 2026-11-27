//! The built-in machines' records: the host machine and `docker` exist
//! from a home's first call and are never removed (#535 R6).
use super::machine::MachineRecord;
use layerfs_protocol::sdk::SdkRefusal;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The homes, by path and id, whose built-in machines this process has
/// found or written (#535 R6).
static ENSURED: Mutex<BTreeSet<(PathBuf, String)>> = Mutex::new(BTreeSet::new());

/// The host machine's folder (`machine/macos-host/` or
/// `machine/linux-host/`) and `machine/docker/` exist from the first call
/// and are never removed. Checked once per process for each home, by its path
/// and its id `id` (a home made again at the same path is checked again),
/// not on every call (#535 R6). Read first; the home lock is taken only to
/// write one.
pub fn ensure_built_ins(home: &Path, id: &str) -> Result<()> {
    let key = (home.to_owned(), id.to_owned());
    if ENSURED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&key)
    {
        return Ok(());
    }
    write_built_ins(home, id)?;
    ENSURED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key);
    Ok(())
}

/// Forgets that this process found the built-in machines of the home at
/// `home`: the next call writes a built-in record that is not there again
/// (`machine remove docker` of a record with no `vm_name`).
pub fn forget_built_ins(home: &Path) {
    let mut ensured = ENSURED.lock().unwrap_or_else(|e| e.into_inner());
    ensured.retain(|(path, _)| path != home);
}

/// Writes each built-in machine's record that is not there. A record that
/// is there but cannot be read is left as it is, for its doctor to name
/// (`machine.record`), and fails no call: every other machine, Store and
/// Sandbox stays usable (RF re-check N-D).
fn write_built_ins(home: &Path, id: &str) -> Result<()> {
    let missing = |name: &str| matches!(MachineRecord::read(home, name), Ok(None));
    if !layerfs_home::names::BUILT_IN
        .iter()
        .any(|name| missing(name))
    {
        return Ok(());
    }
    let _lock = layerfs_home::lock::home::take(home)?;
    for name in layerfs_home::names::BUILT_IN {
        if missing(name) {
            MachineRecord::built_in(name, id).write(home)?;
        }
    }
    Ok(())
}
