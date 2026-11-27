//! Retired Sandbox tombstones: `machine/<machine>/sandbox-retired/<name>.json`.
//!
//! A removal of a Sandbox's home record writes the tombstone **before** it
//! deletes that record, best effort, only when the Sandbox's runtime is
//! confirmed stopped, confirmed gone or never started: a close or force
//! close on `native`, a reclaim of a runtime that ended by itself, and
//! `sandbox.remove` after its side answered `Closed` or `NoSuchSandbox`,
//! on a gone VM or stopped machine, or of a create that ended claiming the
//! name (it writes the home's tombstone for a worker's Sandbox). A removal
//! that did not stop the runtime (a record this build does not read or
//! reopen, a retired docker record, a worker that keeps its record) writes
//! none: a tombstone proves the mounts of its generation dead, and no
//! evidence is the safe outcome. A crash leaves the record or both, never
//! neither; a write that fails is reported in `left` and the removal goes on,
//! since a missing tombstone is only "no proof". The path is outside
//! `sandbox/`, so a Sandbox listing never mistakes a tombstone for a Sandbox.
//!
//! A tombstone is a home record: the place view reads the caller's home, so a
//! worker's root (which no view reads) gets none, and its caller writes the
//! home's. It names the generation the removal retired; a Sandbox recreated
//! under the same name mints a new, greater generation, so the old tombstone
//! never retires it, and once that record is written the old tombstone goes
//! ([`drop_retired`]): the newer record proves the older generation's mounts
//! dead by itself, so one name keeps at most one tombstone and none while it
//! is in use. `machine.remove` deletes `machine/<machine>/` with its
//! tombstones: it is refused while a Store or Sandbox uses the machine, and a
//! mount intent placed there then has no Sandbox evidence (no proof, never
//! death) until a machine of that name starts again (its newer incarnation
//! proves the old placements dead) or the operator repairs it. A missing
//! tombstone and a missing record are both "no proof", never death.
use super::sandbox::SandboxRecord;
use layerfs_home::disk::{self, paths, read_record, write_entity};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// One retired Sandbox record, as written before the Sandbox record was
/// removed. The generation is the ordered value its create minted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxTombstone {
    /// The retired Sandbox's name.
    pub name: String,
    /// The machine its record named.
    pub machine: String,
    /// The generation the close retired.
    pub generation: String,
    /// When it was retired, in Unix milliseconds.
    pub retired_ms: i64,
}

impl SandboxTombstone {
    /// The tombstone of Sandbox `name` on `machine` in `home`, if it has one.
    pub fn read(home: &Path, machine: &str, name: &str) -> Result<Option<Self>> {
        let Some(value) = read_record(&paths::sandbox_tombstone(home, machine, name))? else {
            return Ok(None);
        };
        Ok(Some(Self::from_record(value)))
    }

    /// Writes the tombstone under `home`, replacing an earlier one for the
    /// same name (the newest retirement is the one admission compares).
    pub fn write(&self, home: &Path) -> Result<()> {
        let value = SdkValue::map([
            ("name", SdkValue::text(&self.name)),
            ("machine", SdkValue::text(&self.machine)),
            ("generation", SdkValue::text(&self.generation)),
            ("retired_ms", SdkValue::Int(self.retired_ms)),
        ]);
        write_entity(
            &paths::sandbox_tombstone(home, &self.machine, &self.name),
            &value,
        )
    }

    /// The record as read; a tombstone missing a field is kept as read.
    fn from_record(value: SdkValue) -> Self {
        Self {
            name: value.get_text("name").unwrap_or("").to_owned(),
            machine: value.get_text("machine").unwrap_or("").to_owned(),
            generation: value.get_text("generation").unwrap_or("").to_owned(),
            retired_ms: value.get_int("retired_ms").unwrap_or(0),
        }
    }
}

/// Writes `record`'s tombstone under `home` before its record is deleted.
/// A record without a generation or machine has nothing a tombstone could
/// name, and a worker's root is no home (no place view reads it), so there
/// is nothing to write.
pub fn retire(home: &Path, record: &SandboxRecord) -> Result<()> {
    let Some(generation) = record.generation() else {
        return Ok(());
    };
    if layerfs_home::disk::guest::is_root(home) {
        return Ok(());
    }
    let machine = record.machine();
    if generation.is_empty() || machine.is_empty() {
        return Ok(());
    }
    SandboxTombstone {
        name: record.name().to_owned(),
        machine: machine.to_owned(),
        generation: generation.to_owned(),
        retired_ms: disk::now_ms(),
    }
    .write(home)
}

/// [`retire`] for a record still in `directory`, named `name`: reads the
/// record there and writes its tombstone under `home`.
pub fn retire_in(home: &Path, directory: &Path, name: &str) -> Result<()> {
    match SandboxRecord::read_in(directory, name)? {
        Some(record) => retire(home, &record),
        None => Ok(()),
    }
}

/// [`retire`], best effort: the sentence a removal reports in `left` when the
/// tombstone was not written, else `None`. The removal goes on either way.
pub fn retire_reported(home: &Path, record: &SandboxRecord) -> Option<String> {
    let refusal = retire(home, record).err()?;
    Some(unretired(record.name(), &refusal.message))
}

/// The `left` sentence of a removal whose retired tombstone of Sandbox `name`
/// was not written for reason `why`. Nothing is unsafe about it: without a
/// tombstone, admission has no proof the Sandbox is gone, so a mount intent
/// it held and that was not ended stays live until its owner settles it.
pub fn unretired(name: &str, why: &str) -> String {
    format!(
        "Sandbox {name}'s retired tombstone was not written ({why}): a mount intent it held \
         that was not ended is not proven dead"
    )
}

/// Deletes the tombstone of `record`'s name once `record` (a Sandbox
/// recreated under it) is written with a newer generation of the same home:
/// that record proves the retired generation's mounts dead by itself
/// (`RecordedOlder`), so the tombstone adds nothing. A tombstone that is not
/// older (a restore, another home's value, an unreadable file) is kept. Best
/// effort: a tombstone left behind is only one more file for the name.
pub fn drop_retired(home: &Path, record: &SandboxRecord) {
    let Some(generation) = record.generation() else {
        return;
    };
    let (machine, name) = (record.machine(), record.name());
    let Ok(Some(tombstone)) = SandboxTombstone::read(home, machine, name) else {
        return;
    };
    let order = layerfs_history::compare_place(&tombstone.generation, generation);
    if order == layerfs_history::PlaceOrder::RecordedOlder {
        let _ = std::fs::remove_file(paths::sandbox_tombstone(home, machine, name));
    }
}
