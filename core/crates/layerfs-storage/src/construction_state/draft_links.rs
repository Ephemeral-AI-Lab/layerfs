//! Exact combined child-count transitions and monotone zero jobs.
use super::{
    draft_index as index,
    draft_state::{Effect, Ledger},
    draft_write as write,
    session::Resource,
};
use crate::error::{StorageError, StorageResult};
use layerfs_content::{file::edit::DraftJob, ObjectId};
use rusqlite::{Connection, OptionalExtension};

pub(crate) fn links(
    connection: &Connection,
    effects: &mut Vec<Effect>,
    after: &mut Ledger,
    ids: &[ObjectId],
    add: bool,
) -> StorageResult<()> {
    // At most32/24 input links; combine repeated targets before one CAS each.
    let mut changes: Vec<(ObjectId, u64)> = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some((_, count)) = changes.iter_mut().find(|(key, _)| key == id) {
            *count = count
                .checked_add(1)
                .ok_or(StorageError::Integrity("draft link multiplicity"))?;
        } else {
            changes.push((*id, 1));
        }
    }
    for (id, multiplicity) in changes {
        let Some(before) = index::count(connection, id)? else {
            continue;
        };
        let proposed = if add {
            before.checked_add(multiplicity)
        } else {
            before.checked_sub(multiplicity)
        }
        .ok_or(StorageError::Integrity(
            "draft exact link count overflow/underflow",
        ))?;
        effects.push(Effect::Count {
            id,
            before: Some(before),
            after: Some(proposed),
        });
        if !add && proposed == 0 {
            let header = index::header(connection, id)?
                .ok_or(StorageError::Integrity("draft linked header missing"))?;
            if header.queued.is_none() {
                let sequence = after.next_job;
                after.next_job = sequence
                    .checked_add(1)
                    .ok_or(StorageError::Integrity("draft job sequence overflow"))?;
                let mut proposed = header.clone();
                proposed.queued = Some(sequence);
                effects.push(Effect::HeaderChange {
                    before: header,
                    after: proposed,
                });
                effects.push(Effect::JobInsert { sequence, id });
            }
        }
    }
    Ok(())
}
pub(crate) fn select(
    resource: &mut Resource,
    before: Option<ObjectId>,
    selected: Option<ObjectId>,
) -> StorageResult<()> {
    let prior = resource.draft.as_ref().unwrap().ledger;
    if prior.selected != before {
        return Err(StorageError::Integrity("draft selected root expected"));
    }
    if before == selected {
        return Ok(());
    }
    let connection = resource.connection.as_ref().unwrap();
    let mut effects = write::effects();
    let mut proposed = prior;
    if let Some(id) = selected {
        links(connection, &mut effects, &mut proposed, &[id], true)?;
    }
    // If old and new are different, their exact rows are independent targets.
    if let Some(id) = before {
        links(connection, &mut effects, &mut proposed, &[id], false)?;
    }
    let next = proposed.next_job;
    proposed = write::ledger(prior, &effects)?;
    proposed.next_job = next;
    proposed.selected = selected;
    write::transition(resource, "SelectRoot", effects, proposed)
}
pub(crate) fn next(connection: &Connection) -> StorageResult<Option<DraftJob>> {
    let row: Option<(i64, Vec<u8>)> = connection
        .query_row(
            "SELECT sequence,id FROM draft_jobs ORDER BY sequence LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    row.map(|(sequence, id)| {
        if sequence <= 0 {
            return Err(StorageError::Integrity("draft job sequence"));
        }
        let id = ObjectId::from_bytes(&id)?;
        let header = index::header(connection, id)?
            .ok_or(StorageError::Integrity("draft job header missing"))?;
        if header.queued != Some(sequence as u64) {
            return Err(StorageError::Integrity("draft exact queued membership"));
        }
        let links = index::count(connection, id)?
            .ok_or(StorageError::Integrity("draft job count missing"))?;
        Ok(DraftJob {
            sequence: sequence as u64,
            id,
            links,
        })
    })
    .transpose()
}
