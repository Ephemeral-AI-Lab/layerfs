//! Bounded Creating batches and exact Ready acknowledgement.
use super::{
    draft_index as index, draft_links,
    draft_state::{Effect, Header},
    draft_write as write,
    session::Resource,
};
use crate::error::{StorageError, StorageResult};
use layerfs_content::{
    file::edit::{DraftRecord, DraftScope},
    ObjectId,
};

pub(crate) fn hold(
    resource: &mut Resource,
    scope: &DraftScope,
    id: ObjectId,
    record: DraftRecord,
) -> StorageResult<()> {
    record.validate()?;
    if index::header(resource.connection.as_ref().unwrap(), id)?.is_some() {
        if index::get(resource.connection.as_ref().unwrap(), scope, id)?.as_ref() != Some(&record) {
            return Err(StorageError::Integrity("draft duplicate Ready value"));
        }
        return Ok(());
    }
    let (form, role, body) = record.private_body()?;
    let ids = record.references();
    let predecessors = record.private_predecessors();
    if role == 4 && ids.contains(&id) {
        return Err(StorageError::Integrity("draft self reference"));
    }
    if matches!(&record,DraftRecord::Page(object) if object.id()!=id) {
        return Err(StorageError::Integrity("draft Page identity"));
    }
    let prior = resource.draft.as_ref().unwrap().ledger;
    let deferred_body_bytes = resource
        .draft
        .as_ref()
        .unwrap()
        .stats
        .deferred_body_bytes
        .checked_add(record.deferred_body_charge())
        .ok_or(StorageError::Integrity("draft body telemetry arithmetic"))?;
    let rows = 4 + ids.len() as u64 + predecessors.len() as u64;
    let required = prior
        .bytes
        .checked_add(record.charge())
        .ok_or(StorageError::Integrity("draft creation byte arithmetic"))?;
    if prior
        .records
        .checked_add(rows)
        .is_none_or(|count| count > scope.capacity().records())
        || required as u64 > scope.capacity().encoded_bytes()
    {
        return Err(StorageError::CapacityExceeded {
            what: "draft aggregate metadata",
            limit: scope.capacity().encoded_bytes(),
            actual: required as u64,
        });
    }
    let connection = resource.connection.as_ref().unwrap();
    let mut references = Vec::with_capacity(ids.len());
    for child in ids {
        let header = if role == 4 {
            index::header(connection, child)?
        } else {
            None
        };
        if header.as_ref().is_some_and(|header| header.stage != 1) {
            return Err(StorageError::Integrity("draft child not Ready"));
        }
        references.push((child, header.is_some()));
    }
    // Complete checked multiplicities precede Creating/native effects; a bad
    // child count cannot leave a deterministic partial parent creation.
    let mut multiplicities: Vec<(ObjectId, u64)> = Vec::with_capacity(references.len());
    for (child, linked) in &references {
        if !linked {
            continue;
        }
        if let Some((_, count)) = multiplicities.iter_mut().find(|(id, _)| id == child) {
            *count = count
                .checked_add(1)
                .ok_or(StorageError::Integrity("draft creation multiplicity"))?;
        } else {
            multiplicities.push((*child, 1));
        }
    }
    for (child, multiplicity) in multiplicities {
        index::count(connection, child)?
            .ok_or(StorageError::Integrity("draft Ready child count missing"))?
            .checked_add(multiplicity)
            .ok_or(StorageError::Integrity("draft creation link overflow"))?;
    }
    let header = Header {
        id,
        form,
        role,
        stage: 0,
        references: references.len() as u16,
        predecessors: predecessors.len() as u8,
        cursor: 0,
        charge: record.charge() - 71,
        queued: None,
        digest: index::digest(scope, id, form, role, &body, &references, &predecessors),
    };
    // Prospective complete shape precedes first native reserve/transaction.
    resource.draft.as_mut().unwrap().stats.peak_bytes = resource
        .draft
        .as_ref()
        .unwrap()
        .stats
        .peak_bytes
        .max(required);
    let stats = &mut resource.draft.as_mut().unwrap().stats;
    stats.peak_deferred_body_bytes = stats.peak_deferred_body_bytes.max(deferred_body_bytes);
    let mut effects = write::effects();
    effects.push(Effect::HeaderInsert(header));
    effects.push(Effect::BodyInsert { id, body });
    effects.push(Effect::Count {
        id,
        before: None,
        after: Some(0),
    });
    let after = write::ledger(prior, &effects)?;
    write::transition(resource, "CreatingStart", effects, after)?;
    resource.draft.as_mut().unwrap().stats.deferred_body_bytes = deferred_body_bytes;
    for (wave, page) in references.chunks(32).enumerate() {
        let connection = resource.connection.as_ref().unwrap();
        let before = resource.draft.as_ref().unwrap().ledger;
        let mut proposed = before;
        let mut effects = write::effects();
        let header = index::header(connection, id)?.unwrap();
        if header.stage != 0 || header.cursor as usize != wave * 32 {
            return Err(StorageError::Integrity("draft Creating cursor"));
        }
        let mut linked = Vec::with_capacity(page.len());
        for (local, (child, is_linked)) in page.iter().enumerate() {
            effects.push(Effect::ReferenceInsert {
                id,
                ordinal: (wave * 32 + local) as u16,
                child: *child,
                linked: *is_linked,
            });
            if *is_linked {
                linked.push(*child);
            }
        }
        draft_links::links(connection, &mut effects, &mut proposed, &linked, true)?;
        let mut next = header.clone();
        next.cursor += page.len() as u16;
        effects.push(Effect::HeaderChange {
            before: header,
            after: next,
        });
        let next_job = proposed.next_job;
        proposed = write::ledger(before, &effects)?;
        proposed.next_job = next_job;
        write::transition(resource, "CreatingReferences", effects, proposed)?;
    }
    if !predecessors.is_empty() {
        let mut effects = write::effects();
        for (ordinal, (value, provenance)) in predecessors.iter().enumerate() {
            effects.push(Effect::PredecessorInsert {
                id,
                ordinal: ordinal as u8,
                value: *value,
                provenance: *provenance,
            });
        }
        let after = write::ledger(resource.draft.as_ref().unwrap().ledger, &effects)?;
        write::transition(resource, "CreatingPredecessors", effects, after)?;
    }
    let connection = resource.connection.as_ref().unwrap();
    let header = index::header(connection, id)?.unwrap();
    let actual_body = index::body(connection, id)?;
    let actual_refs = index::references(connection, id)?;
    let actual_preds = index::predecessors(connection, id)?;
    if header.stage != 0
        || header.cursor != header.references
        || actual_refs != references
        || actual_preds != predecessors
        || index::digest(
            scope,
            id,
            header.form,
            header.role,
            &actual_body,
            &actual_refs,
            &actual_preds,
        ) != header.digest
    {
        return Err(StorageError::Integrity("draft exact Ready totals"));
    }
    let prior = resource.draft.as_ref().unwrap().ledger;
    let sequence = prior.next_job;
    let mut ready = header.clone();
    ready.stage = 1;
    ready.queued = Some(sequence);
    let mut effects = write::effects();
    effects.push(Effect::HeaderChange {
        before: header,
        after: ready,
    });
    effects.push(Effect::JobInsert { sequence, id });
    let mut after = write::ledger(prior, &effects)?;
    after.next_job = sequence
        .checked_add(1)
        .ok_or(StorageError::Integrity("draft Ready job sequence"))?;
    write::transition(resource, "Ready", effects, after)?;
    resource.draft.as_mut().unwrap().stats.created += 1;
    Ok(())
}
