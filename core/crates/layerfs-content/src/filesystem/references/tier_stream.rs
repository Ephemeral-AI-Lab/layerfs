//! Fixed immutable tier heads, shared by borrowed and owning ordered consumers.
use super::merge::{MergeWork, Run};
use super::record::{Row, ROW_BYTES};
use super::runs::MAXIMUM_LEVELS;
use crate::{ContentError, ContentResult};

#[derive(Clone, Copy, Default)]
struct TierHead {
    offset: u64,
    remaining: u64,
    previous: Option<u64>,
    row: Option<Row>,
}

pub(crate) struct TierHeads {
    heads: [TierHead; MAXIMUM_LEVELS],
    failure: Option<ContentError>,
}
impl TierHeads {
    pub(crate) fn new(runs: &[Option<Run>], work: &mut MergeWork) -> ContentResult<Self> {
        if runs.len() > MAXIMUM_LEVELS {
            return Err(ContentError::InvalidOrderingRecord("stream tier count"));
        }
        // Validate every selected owner before any input read begins.
        for run in runs.iter().flatten() {
            run.check()?;
        }
        let mut cursor = Self {
            heads: [TierHead::default(); MAXIMUM_LEVELS],
            failure: None,
        };
        for (index, run) in runs.iter().enumerate() {
            if let Some(run) = run {
                cursor.heads[index].remaining = run.count;
                cursor.advance(index, run, work)?;
            }
        }
        Ok(cursor)
    }

    pub(crate) fn exhausted(&self, index: usize) -> bool {
        self.heads[index].remaining == 0 && self.heads[index].row.is_none()
    }

    pub(crate) fn next(
        &mut self,
        runs: &[Option<Run>],
        work: &mut MergeWork,
    ) -> ContentResult<Option<Row>> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = self.next_selected(runs, work);
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }

    fn next_selected(
        &mut self,
        runs: &[Option<Run>],
        work: &mut MergeWork,
    ) -> ContentResult<Option<Row>> {
        let mut selected: Option<Row> = None;
        // Lower tiers are newer. Keeping the first equal key establishes
        // precedence without sorting or retaining another population.
        for head in &self.heads {
            if let Some(row) = head.row {
                if selected.is_none_or(|previous| row.serial() < previous.serial()) {
                    selected = Some(row);
                }
            }
        }
        let Some(row) = selected else {
            return Ok(None);
        };
        // Consume every duplicate, including hidden older rows. Their next
        // record must validate before the selected row can be returned.
        for (index, run) in runs.iter().enumerate() {
            if self.heads[index]
                .row
                .is_some_and(|head| head.serial() == row.serial())
            {
                let run = run
                    .as_ref()
                    .ok_or(ContentError::InvalidOrderingRecord("stream run owner"))?;
                self.advance(index, run, work)?;
            }
        }
        Ok(Some(row))
    }

    fn advance(&mut self, index: usize, run: &Run, work: &mut MergeWork) -> ContentResult<()> {
        let head = &mut self.heads[index];
        run.check()?;
        if head.remaining == 0 {
            head.row = None;
            return Ok(());
        }
        let next = head
            .offset
            .checked_add(ROW_BYTES as u64)
            .ok_or(ContentError::LengthOverflow)?;
        if next > run.handle.len() {
            return Err(ContentError::InvalidOrderingRecord("stream run offset"));
        }
        let mut bytes = [0_u8; ROW_BYTES];
        run.handle.read_at(head.offset, &mut bytes)?;
        let row = Row::decode(&bytes)?;
        let serial = row.serial();
        if serial < run.first
            || serial > run.last
            || (head.offset == 0 && serial != run.first)
            || (head.remaining == 1 && serial != run.last)
            || head.previous.is_some_and(|previous| previous >= serial)
        {
            return Err(ContentError::InvalidOrderingRecord(
                "stream run order/bounds",
            ));
        }
        head.offset = next;
        head.remaining -= 1;
        head.previous = Some(serial);
        head.row = Some(row);
        work.rows_read = work.rows_read.saturating_add(1);
        Ok(())
    }
}
