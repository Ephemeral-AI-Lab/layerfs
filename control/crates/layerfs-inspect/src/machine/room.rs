//! The one room rule of the two disk checks, `home.capacity` (the disk
//! that holds the home) and `machine.capacity` (a machine's data disk): a
//! disk with less than [`ROOM`] free is nearly full, a warning (it works,
//! and the next Store, image or Commit may not fit); anything more passes.
//! The free bytes are the evidence, as `disk_free_bytes` (review fix 16).
use super::super::doctor::budget::Checks;
use layerfs_protocol::sdk::SdkValue;

/// Free bytes under which a disk is nearly full: 2 GiB.
pub(super) const ROOM: u64 = 2 << 30;

/// `bytes` as a person reads them: GiB with one decimal, else MiB.
fn shown(bytes: u64) -> String {
    const GIB: f64 = (1u64 << 30) as f64;
    const MIB: f64 = (1u64 << 20) as f64;
    let value = bytes as f64;
    match bytes >= 1 << 30 {
        true => format!("{:.1} GiB", value / GIB),
        false => format!("{:.0} MiB", value / MIB),
    }
}

/// Judges check `id` on `disk` (`its data disk`, `the disk that holds the
/// home`) with `free` bytes free: a warning with `fix` under [`ROOM`], else
/// a pass; `evidence` is extended with `disk_free_bytes`.
pub(crate) fn room(
    checks: &mut Checks,
    id: &str,
    disk: &str,
    free: u64,
    fix: Option<String>,
    evidence: Vec<(&'static str, SdkValue)>,
) {
    if free < ROOM {
        let limit = shown(ROOM);
        checks.warning(
            id,
            format!(
                "{disk} has {} free; under {limit} it is nearly full",
                shown(free)
            ),
        );
        if let Some(fix) = fix {
            checks.fix(id, fix);
        }
    } else {
        checks.pass(id, format!("{disk} has {} free", shown(free)));
    }
    let bytes = i64::try_from(free).map_or(SdkValue::Null, SdkValue::Int);
    let mut fields = evidence;
    fields.push(("disk_free_bytes", bytes));
    let fields = fields
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value));
    checks.evidence(id, SdkValue::Map(fields.collect()));
}
