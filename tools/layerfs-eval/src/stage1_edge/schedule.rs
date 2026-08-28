use super::limits::{INITIAL_BYTES, MAXIMUM_BYTES, MAX_USER_FILE_BYTES, REPLACEMENT_BACKING_BYTES};
use super::schedule_bursts::build_bursts;
use super::schedule_logical::append_logical;
use super::schedule_model::{FrozenSchedule, PieceTable};
use super::schedule_physical::append_physical;
use super::schedule_rows::build_rows;
use crate::stage1_fixture::EvalResult;
pub(crate) fn frozen_schedule() -> EvalResult<FrozenSchedule> {
    let mut edits = Vec::new();
    let mut replacement_backing = Vec::with_capacity(REPLACEMENT_BACKING_BYTES);
    append_physical(&mut edits, &mut replacement_backing)?;
    append_logical(&mut edits, &mut replacement_backing)?;
    let bursts = build_bursts(&mut edits, &mut replacement_backing)?;
    let rows = build_rows(&edits)?;
    let schedule = FrozenSchedule {
        edits,
        bursts,
        rows,
        replacement_backing,
    };
    validate_schedule(&schedule)?;
    Ok(schedule)
}
pub(crate) fn validate_schedule(schedule: &FrozenSchedule) -> EvalResult<()> {
    if schedule.rows.len() != 47
        || schedule.edits.len() != 51
        || schedule.bursts.len() != 4
        || schedule.replacement_backing.len() != REPLACEMENT_BACKING_BYTES
    {
        return Err("frozen 47/51/4 population mismatch".to_owned());
    }
    let mut row_ids = schedule
        .rows
        .iter()
        .map(|row| row.row_id.as_str())
        .collect::<Vec<_>>();
    row_ids.sort_unstable();
    if row_ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("frozen row IDs are not unique".to_owned());
    }
    if schedule
        .rows
        .iter()
        .enumerate()
        .any(|(index, row)| row.row_index != index)
    {
        return Err("frozen row indices are not ordered".to_owned());
    }
    let transitions = schedule
        .rows
        .iter()
        .filter(|row| row.transition_root.is_some())
        .count();
    if transitions != 34 {
        return Err(format!("frozen transition count is {transitions}, not 34"));
    }
    let mut table = PieceTable::initial();
    let mut snapshots = vec![table.clone()];
    let mut maximum = table.logical_length;
    for edit in &schedule.edits[..30] {
        table.splice(edit)?;
        maximum = maximum.max(table.logical_length);
        snapshots.push(table.clone());
    }
    for burst in &schedule.bursts {
        for edit in &burst.edits {
            table.splice(edit)?;
            maximum = maximum.max(table.logical_length);
        }
        snapshots.push(table.clone());
    }
    let descriptor_total = snapshots.iter().try_fold(0_usize, |total, snapshot| {
        total
            .checked_add(snapshot.pieces.len())
            .ok_or_else(|| "snapshot descriptor count overflow".to_owned())
    })?;
    if snapshots.len() != 35
        || maximum != MAXIMUM_BYTES
        || maximum > MAX_USER_FILE_BYTES
        || table.logical_length != INITIAL_BYTES
        || table.pieces.len() > 103
        || descriptor_total > 1_315
    {
        return Err(format!(
            "oracle bounds mismatch: snapshots={} max={} terminal={} live_descriptors={} snapshot_descriptors={descriptor_total}",
            snapshots.len(), maximum, table.logical_length, table.pieces.len()
        ));
    }
    Ok(())
}
pub(crate) fn oracle_snapshots(schedule: &FrozenSchedule) -> EvalResult<Vec<PieceTable>> {
    let mut table = PieceTable::initial();
    let mut snapshots = vec![table.clone()];
    for edit in &schedule.edits[..30] {
        table.splice(edit)?;
        snapshots.push(table.clone());
    }
    for burst in &schedule.bursts {
        for edit in &burst.edits {
            table.splice(edit)?;
        }
        snapshots.push(table.clone());
    }
    Ok(snapshots)
}
