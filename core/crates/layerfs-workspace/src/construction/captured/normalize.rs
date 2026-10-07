//! Coalesced final changes, never chronological mutation replay or an edit Vec.
use super::{
    context,
    owner::Facts,
    scan::Scan,
    state::{change, encode, key, Shared, CONTEXT, EDITS},
};
use crate::{OverlayCapturedRuns, OverlayScratch};
use layerfs_content::{ContentError, ContentResult, Edit, EditRecordChange, EditRecordExpected};

const BATCH: usize = 64;
struct Writer {
    changes: Vec<EditRecordChange>,
    count: usize,
}
impl Writer {
    fn flush<P: OverlayScratch + ?Sized>(&mut self, shared: &Shared<'_, P>) -> ContentResult<()> {
        if !self.changes.is_empty() {
            let changes = std::mem::replace(&mut self.changes, Vec::with_capacity(BATCH));
            shared.borrow_mut().apply(changes)?;
        }
        Ok(())
    }
    fn push<P: OverlayScratch + ?Sized>(
        &mut self,
        shared: &Shared<'_, P>,
        edit: Edit,
    ) -> ContentResult<()> {
        let number = u64::try_from(self.count).map_err(|_| ContentError::LengthOverflow)?;
        self.changes.push(change(EDITS, number, encode(edit)));
        self.count = self
            .count
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
        if self.changes.len() == BATCH {
            self.flush(shared)?;
        }
        Ok(())
    }
}
pub(super) fn prepare<P: OverlayCapturedRuns + OverlayScratch + ?Sized>(
    provider: &P,
    shared: &Shared<'_, P>,
    facts: Facts,
) -> ContentResult<(usize, Vec<u8>)> {
    let initial = context::encode(facts, 0, false)?;
    shared
        .borrow_mut()
        .apply(vec![change(CONTEXT, 0, initial.clone())])?;
    let mut scan = Scan::new(
        facts.reader,
        facts.serial,
        facts.final_size,
        facts.base_size,
    )?;
    let mut writer = Writer {
        changes: Vec::with_capacity(BATCH),
        count: 0,
    };
    let mut at = 0;
    let surviving = facts.base_size.min(facts.final_size);
    let mut changed = None;
    while at < facts.final_size {
        let span = scan.span(provider, shared, at, true)?;
        let end = if at < surviving {
            span.end.min(surviving)
        } else {
            span.end
        };
        if end <= at {
            return Err(ContentError::InvalidRecord(
                "captured normalization progress",
            ));
        }
        if at < surviving {
            if span.changed {
                let (start, _) = changed.unwrap_or((span.start, at));
                changed = Some((start, end));
            } else if let Some((start, end)) = changed.take() {
                writer.push(shared, Edit::overwrite(start, end))?;
            }
        } else if !span.changed {
            return Err(ContentError::InvalidRecord("inherited captured extension"));
        }
        at = end;
        if at == surviving {
            if let Some((start, end)) = changed.take() {
                writer.push(shared, Edit::overwrite(start, end))?;
            }
        }
    }
    if let Some((start, end)) = changed.take() {
        writer.push(shared, Edit::overwrite(start, end))?;
    }
    match facts.final_size.cmp(&facts.base_size) {
        std::cmp::Ordering::Less => {
            writer.push(shared, Edit::delete(facts.final_size, facts.base_size))?;
        }
        std::cmp::Ordering::Greater => writer.push(
            shared,
            Edit::insert(facts.base_size, facts.final_size - facts.base_size),
        )?,
        std::cmp::Ordering::Equal => {}
    }
    writer.flush(shared)?;
    let sealed = context::encode(facts, writer.count, true)?;
    shared.borrow_mut().apply(vec![EditRecordChange {
        key: key(CONTEXT, 0),
        expected: EditRecordExpected::ExactBytes(initial),
        value: Some(sealed.clone()),
    }])?;
    shared.borrow_mut().work.normalized_edits =
        u64::try_from(writer.count).map_err(|_| ContentError::LengthOverflow)?;
    Ok((writer.count, sealed))
}
