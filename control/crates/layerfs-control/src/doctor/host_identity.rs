//! `home.layout`'s cross-check of the id `layerfs.json` holds against the
//! provider names the home's own records keep (layerfs-project#752, c4).
//! A record copied from another home, or a `layerfs.json` whose id was
//! replaced, leaves the two disagreeing; this reads only what the records
//! scan already read and touches nothing outside the home.
use super::scan::Scanned;
use layerfs_home::home::paths;
use layerfs_home::names::{matches_provider_name, provider_prefix};
use std::path::{Path, PathBuf};

/// A record whose provider name this home's id does not give.
pub(super) struct Mismatch {
    /// The record file.
    pub(super) path: PathBuf,
    /// The name the record holds.
    pub(super) recorded: String,
    /// The name this home's id gives the same machine or Sandbox.
    pub(super) expected: String,
}

/// The first record of `scanned` whose provider name does not match
/// `home_id`, if any. A record with no provider name is not compared: a
/// `native` machine and a machine made by an earlier build keep none, and a
/// Sandbox that never started keeps no `daemon`. Nothing is read here
/// beyond what the scan holds.
pub(super) fn mismatch(home: &Path, home_id: &str, scanned: &Scanned) -> Option<Mismatch> {
    for machine in &scanned.machines {
        let Some(recorded) = machine.text("vm_name") else {
            continue;
        };
        if !matches_provider_name(home_id, recorded) {
            return Some(Mismatch {
                path: paths::machine_record(home, machine.name()),
                recorded: recorded.to_owned(),
                expected: layerfs_machine::record::vm_name(home_id, machine.name()),
            });
        }
    }
    for sandbox in &scanned.sandboxes {
        let Some(recorded) = sandbox.daemon().and_then(|daemon| daemon.name()) else {
            continue;
        };
        if !matches_provider_name(home_id, recorded) {
            return Some(Mismatch {
                path: paths::sandbox_record(home, sandbox.name()),
                recorded: recorded.to_owned(),
                expected: format!("{}{}", provider_prefix(home_id), sandbox.name()),
            });
        }
    }
    None
}
