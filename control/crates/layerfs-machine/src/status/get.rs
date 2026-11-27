//! `machine.get`: one machine from the home's records alone (call surface
//! 2.0), as `machine.list` shows it. Nothing is probed unless the caller
//! asks for `live` (`live.rs`).
use super::list::summary;
use super::live;
use crate::record::machine::MachineRecord;
use crate::Related;
use layerfs_home::names::MachineName;
use layerfs_home::reply::outcome::{with_fix, Outcome};
use std::path::Path;

/// `machine.get(machine)`: its record and state, with no probe. With
/// `live`, one probe of its provider and one hello to its worker, and the
/// `live` section: `{ disk_free_bytes, tool_version, worker }`. A read:
/// answered while an `op` holds the machine too.
pub fn get(home: &Path, name: &MachineName, live: bool, related: Related) -> Outcome {
    // a record that cannot be read: refused, with its doctor as the fix
    let rows = [
        MachineRecord::of(home, name).map_err(|refusal| match refusal.code.as_str() {
            "Failed" => with_fix(refusal, crate::status::state::unreadable_fix(name.as_str())),
            _ => refusal,
        })?,
    ];
    if !live {
        return summary(home, &rows[0], None, related);
    }
    let seen = live::observe(home, &rows, related)?.pop().flatten();
    let shown = summary(home, &rows[0], seen.as_ref(), related)?;
    let section = match &seen {
        Some(seen) => seen.section(),
        None => live::unprobed(home, &rows[0]),
    };
    Ok(layerfs_home::disk::with_field(shown, "live", section))
}
