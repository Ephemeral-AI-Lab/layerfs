//! Independent fixed-row point search; no sequential cursor or heap ownership.

use crate::error::{ContentError, ContentResult};

use super::merge::{MergeWork, Run};
use super::record::{Row, ROW_BYTES};

pub(crate) fn find_run(run: &Run, serial: u64, work: &mut MergeWork) -> ContentResult<Option<Row>> {
    run.check()?;
    if serial < run.first || serial > run.last {
        return Ok(None);
    }
    let mut lower = 0_u64;
    let mut upper = run.count;
    let mut lower_serial = None;
    let mut upper_serial = None;
    let mut bytes = [0_u8; ROW_BYTES];
    while lower < upper {
        let index = lower
            .checked_add((upper - lower) / 2)
            .ok_or(ContentError::LengthOverflow)?;
        let offset = index
            .checked_mul(ROW_BYTES as u64)
            .ok_or(ContentError::LengthOverflow)?;
        run.handle.read_at(offset, &mut bytes)?;
        let row = Row::decode(&bytes)?;
        work.rows_read = work.rows_read.saturating_add(1);
        let key = row.serial();
        if key < run.first
            || key > run.last
            || (index == 0 && key != run.first)
            || (index == run.count - 1 && key != run.last)
        {
            return Err(ContentError::InvalidOrderingRecord("run point bounds"));
        }
        if lower_serial.is_some_and(|previous| previous >= key)
            || upper_serial.is_some_and(|next| next <= key)
        {
            return Err(ContentError::InvalidOrderingRecord("run point order"));
        }
        match key.cmp(&serial) {
            std::cmp::Ordering::Less => {
                lower = index.checked_add(1).ok_or(ContentError::LengthOverflow)?;
                lower_serial = Some(key);
            }
            std::cmp::Ordering::Greater => {
                upper = index;
                upper_serial = Some(key);
            }
            std::cmp::Ordering::Equal => return Ok(Some(row)),
        }
    }
    Ok(None)
}
