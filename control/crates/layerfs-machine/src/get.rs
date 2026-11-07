//! `machine.get`.
use super::list::{live, summary};
use super::record::MachineRecord;
use crate::Related;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;
use std::path::Path;

/// `machine.get(name)`: its record with its live state, Stores and
/// Sandboxes. A read: answered while an `op` holds the machine too.
pub fn get(home: &Path, name: &MachineName, related: Related) -> Outcome {
    let rows = [MachineRecord::of(home, name)?];
    let state = live(home, &rows)?.pop().flatten();
    summary(home, &rows[0], state, related)
}
