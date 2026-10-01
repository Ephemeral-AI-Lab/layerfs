//! Closed exact expected/proposed row effects and transaction acknowledgement.
use super::draft_state::{Attempt, Effect, Ledger};
use super::session::Resource;
use crate::error::{StorageError, StorageResult};
use rusqlite::{params, Connection};

pub(crate) fn ledger(before: Ledger, effects: &[Effect]) -> StorageResult<Ledger> {
    let mut after = before;
    for effect in effects {
        let (records, bytes): (i64, i64) = match effect {
            Effect::HeaderInsert(_) => (1, 151),
            Effect::HeaderDelete(_) => (-1, -151),
            Effect::BodyInsert { body, .. } => (1, (body.len() + 55) as i64),
            Effect::BodyDelete { body, .. } => (-1, -((body.len() + 55) as i64)),
            Effect::ReferenceInsert { .. } => (1, 89),
            Effect::ReferenceDelete { .. } => (-1, -89),
            Effect::PredecessorInsert { .. } => (1, 90),
            Effect::PredecessorDelete { .. } => (-1, -90),
            Effect::Count {
                before: None,
                after: Some(_),
                ..
            } => (1, 63),
            Effect::Count {
                before: Some(_),
                after: None,
                ..
            } => (-1, -63),
            Effect::JobInsert { .. } => (1, 71),
            Effect::JobDelete { .. } => (-1, -71),
            Effect::ResolveInsert { .. } => (1, 87),
            Effect::ResolveDelete { .. } => (-1, -87),
            Effect::EmissionInsert { .. } => (1, 95),
            Effect::EmissionDelete { .. } => (-1, -95),
            _ => (0, 0),
        };
        after.records = after
            .records
            .checked_add_signed(records)
            .ok_or(StorageError::Integrity("draft record arithmetic"))?;
        after.bytes = after
            .bytes
            .checked_add_signed(bytes as isize)
            .ok_or(StorageError::Integrity("draft byte arithmetic"))?;
    }
    Ok(after)
}
pub(crate) fn transition(
    resource: &mut Resource,
    kind: &'static str,
    effects: Vec<Effect>,
    after: Ledger,
) -> StorageResult<()> {
    let drafts = resource
        .draft
        .as_ref()
        .ok_or(StorageError::Integrity("draft unavailable"))?;
    if drafts.failed.get() || drafts.ledger.ended || drafts.attempt.is_some() {
        return Err(StorageError::Integrity("draft transition unavailable"));
    }
    let encoded: usize = effects.iter().map(Effect::width).sum();
    if effects.len() > 128
        || effects.capacity() > 128
        || encoded > 65_536
        || after.records > drafts.scope.capacity().records()
        || after.bytes as u64 > drafts.scope.capacity().encoded_bytes()
        || after.next_job > i64::MAX as u64
    {
        return Err(StorageError::CapacityExceeded {
            what: "draft transition window",
            limit: 65_536,
            actual: encoded as u64,
        });
    }
    resource.verify()?;
    resource.native.reserve()?;
    let before = drafts.ledger;
    resource.draft.as_mut().unwrap().attempt = Some(Box::new(Attempt {
        kind,
        before,
        after,
        effects,
    }));
    let connection = resource.connection.as_ref().unwrap();
    resource.check_engine()?;
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        let drafts = resource.draft.as_mut().unwrap();
        let attempt = drafts.attempt.as_ref().unwrap();
        apply_effects(connection, &attempt.effects, &mut drafts.stats)?;
        if connection.execute("UPDATE draft_owner SET records=?1,bytes=?2,next_job=?3 WHERE id=1 AND records=?4 AND bytes=?5 AND next_job=?6",params![after.records as i64,after.bytes as i64,after.next_job as i64,before.records as i64,before.bytes as i64,before.next_job as i64])?!=1
            || connection.execute("UPDATE draft_owner SET selected=?1,stage=?2 WHERE id=1 AND selected IS ?3 AND stage=?4",params![after.selected.as_ref().map(|id|id.as_bytes().as_slice()),i64::from(after.ended),before.selected.as_ref().map(|id|id.as_bytes().as_slice()),i64::from(before.ended)])?!=1 {return Err(StorageError::Integrity("draft owner CAS"));}
        Ok(())
    })();
    super::profile::finish_write_guarded(connection, result, resource.engine)?;
    resource
        .native
        .observe_allocation()
        .map_err(|original| StorageError::UnknownOutcome {
            original: Box::new(original),
        })?;
    let drafts = resource.draft.as_mut().unwrap();
    drafts.ledger = after;
    drafts.stats.bytes = after.bytes;
    drafts.stats.peak_bytes = drafts.stats.peak_bytes.max(after.bytes);
    drafts.attempt = None;
    Ok(())
}
// One statement lives only through a contiguous run in this owned transaction.
// Preserve the original effect order, complete expected fields and per-row acknowledgement.
fn apply_effects(
    connection: &Connection,
    mut effects: &[Effect],
    stats: &mut layerfs_content::file::edit::DraftStats,
) -> StorageResult<()> {
    while let Some(first) = effects.first() {
        let insert = match first {
            Effect::ReferenceInsert { .. } => Some(true),
            Effect::ReferenceDelete { .. } => Some(false),
            _ => None,
        };
        if let Some(insert) = insert {
            let count = effects
                .iter()
                .take_while(|effect| {
                    matches!(
                        (insert, effect),
                        (true, Effect::ReferenceInsert { .. })
                            | (false, Effect::ReferenceDelete { .. })
                    )
                })
                .count();
            let (run, remaining) = effects.split_at(count);
            let sql = if insert {
                "INSERT INTO draft_references VALUES(?1,?2,?3,?4)"
            } else {
                "DELETE FROM draft_references WHERE id=?1 AND ordinal=?2 AND value=?3 AND linked=?4"
            };
            let calls = if insert {
                &mut stats.reference_insert_prepare_calls
            } else {
                &mut stats.reference_delete_prepare_calls
            };
            *calls = calls.checked_add(1).ok_or(StorageError::Integrity(
                "draft reference work counter overflow",
            ))?;
            let mut statement = connection.prepare(sql)?;
            for effect in run {
                let (id, ordinal, child, linked) = match effect {
                    Effect::ReferenceInsert {
                        id,
                        ordinal,
                        child,
                        linked,
                    }
                    | Effect::ReferenceDelete {
                        id,
                        ordinal,
                        child,
                        linked,
                    } => (id, ordinal, child, linked),
                    _ => unreachable!(),
                };
                let rows = if insert {
                    &mut stats.reference_insert_rows
                } else {
                    &mut stats.reference_delete_rows
                };
                let acknowledged = rows.checked_add(1).ok_or(StorageError::Integrity(
                    "draft reference work counter overflow",
                ))?;
                if statement.execute(params![
                    id.as_bytes().as_slice(),
                    ordinal,
                    child.as_bytes().as_slice(),
                    i64::from(*linked)
                ])? != 1
                {
                    return Err(StorageError::Integrity("draft exact effect cardinality"));
                }
                *rows = acknowledged;
            }
            drop(statement);
            effects = remaining;
        } else {
            apply(connection, first)?;
            effects = &effects[1..];
        }
    }
    Ok(())
}
fn apply(connection: &Connection, effect: &Effect) -> StorageResult<()> {
    let changed=match effect {
        Effect::HeaderInsert(value)=>connection.execute("INSERT INTO draft_headers VALUES(?1,?2,?3,?4,?5,?6,0,?7,NULL,?8)",params![value.id.as_bytes().as_slice(),value.form,value.role,value.stage,value.references,value.predecessors,value.charge as i64,value.digest.as_slice()])?,
        Effect::HeaderChange {before,after}=>connection.execute("UPDATE draft_headers SET stage=?1,cursor=?2,queued=?3 WHERE id=?4 AND stage=?5 AND cursor=?6 AND queued IS ?7 AND digest=?8",params![after.stage,after.cursor,after.queued.map(|value|value as i64),before.id.as_bytes().as_slice(),before.stage,before.cursor,before.queued.map(|value|value as i64),before.digest.as_slice()])?,
        Effect::HeaderDelete(value)=>connection.execute("DELETE FROM draft_headers WHERE id=?1 AND stage=?2 AND cursor=?3 AND queued IS ?4 AND digest=?5",params![value.id.as_bytes().as_slice(),value.stage,value.cursor,value.queued.map(|value|value as i64),value.digest.as_slice()])?,
        Effect::BodyInsert {id,body}=>connection.execute("INSERT INTO draft_bodies VALUES(?1,?2)",params![id.as_bytes().as_slice(),body])?,
        Effect::BodyDelete {id,body}=>connection.execute("DELETE FROM draft_bodies WHERE id=?1 AND value=?2",params![id.as_bytes().as_slice(),body])?,
        Effect::ReferenceInsert {id,ordinal,child,linked}=>connection.execute("INSERT INTO draft_references VALUES(?1,?2,?3,?4)",params![id.as_bytes().as_slice(),ordinal,child.as_bytes().as_slice(),i64::from(*linked)])?,
        Effect::ReferenceDelete {id,ordinal,child,linked}=>connection.execute("DELETE FROM draft_references WHERE id=?1 AND ordinal=?2 AND value=?3 AND linked=?4",params![id.as_bytes().as_slice(),ordinal,child.as_bytes().as_slice(),i64::from(*linked)])?,
        Effect::PredecessorInsert {id,ordinal,value,provenance}=>connection.execute("INSERT INTO draft_predecessors VALUES(?1,?2,?3,?4)",params![id.as_bytes().as_slice(),ordinal,value.as_bytes().as_slice(),provenance])?,
        Effect::PredecessorDelete {id,ordinal,value,provenance}=>connection.execute("DELETE FROM draft_predecessors WHERE id=?1 AND ordinal=?2 AND value=?3 AND provenance=?4",params![id.as_bytes().as_slice(),ordinal,value.as_bytes().as_slice(),provenance])?,
        Effect::Count {id,before:None,after:Some(after)}=>connection.execute("INSERT INTO draft_counts VALUES(?1,?2)",params![id.as_bytes().as_slice(),after.to_be_bytes().as_slice()])?,
        Effect::Count {id,before:Some(before),after:Some(after)}=>connection.execute("UPDATE draft_counts SET links=?1 WHERE id=?2 AND links=?3",params![after.to_be_bytes().as_slice(),id.as_bytes().as_slice(),before.to_be_bytes().as_slice()])?,
        Effect::Count {id,before:Some(before),after:None}=>connection.execute("DELETE FROM draft_counts WHERE id=?1 AND links=?2",params![id.as_bytes().as_slice(),before.to_be_bytes().as_slice()])?,
        Effect::Count {..}=>return Err(StorageError::Integrity("draft count transition")),
        Effect::JobInsert {sequence,id}=>connection.execute("INSERT INTO draft_jobs VALUES(?1,?2)",params![*sequence as i64,id.as_bytes().as_slice()])?,
        Effect::JobDelete {sequence,id}=>connection.execute("DELETE FROM draft_jobs WHERE sequence=?1 AND id=?2",params![*sequence as i64,id.as_bytes().as_slice()])?,
        Effect::ResolveInsert {id,canonical}=>connection.execute("INSERT INTO draft_committed VALUES(?1,?2)",params![id.as_bytes().as_slice(),canonical.as_bytes().as_slice()])?,
        Effect::ResolveDelete {id,canonical}=>connection.execute("DELETE FROM draft_committed WHERE id=?1 AND canonical=?2",params![id.as_bytes().as_slice(),canonical.as_bytes().as_slice()])?,
        Effect::EmissionInsert {canonical,draft}=>connection.execute("INSERT INTO draft_emissions VALUES(?1,0,?2)",params![canonical.as_bytes().as_slice(),draft.as_bytes().as_slice()])?,
        Effect::EmissionAccept {canonical,draft}=>connection.execute("UPDATE draft_emissions SET accepted=1,pending_draft=NULL WHERE canonical=?1 AND accepted=0 AND pending_draft=?2",params![canonical.as_bytes().as_slice(),draft.as_bytes().as_slice()])?,
        Effect::EmissionDelete {canonical}=>connection.execute("DELETE FROM draft_emissions WHERE canonical=?1 AND accepted=1",[canonical.as_bytes().as_slice()])?,
    };
    if changed != 1 {
        return Err(StorageError::Integrity("draft exact effect cardinality"));
    }
    Ok(())
}
pub(crate) fn effects() -> Vec<Effect> {
    Vec::with_capacity(128)
}
