//! Monotone detached jobs and bounded outgoing-edge retirement.
use super::{
    draft_index as index, draft_links, draft_state::Effect, draft_write as write, session::Resource,
};
use crate::error::{StorageError, StorageResult};
use layerfs_content::{
    file::edit::{DraftJob, DraftScope},
    ObjectId,
};

pub(crate) fn retire(
    resource: &mut Resource,
    scope: &DraftScope,
    job: DraftJob,
) -> StorageResult<()> {
    let connection = resource.connection.as_ref().unwrap();
    if draft_links::next(connection)? != Some(job) {
        return Err(StorageError::Integrity("draft FIRST job expected"));
    }
    retire_exact(resource, scope, job)
}
pub(crate) fn retire_exact(
    resource: &mut Resource,
    scope: &DraftScope,
    job: DraftJob,
) -> StorageResult<()> {
    let connection = resource.connection.as_ref().unwrap();
    let queued: Vec<u8> = connection.query_row(
        "SELECT id FROM draft_jobs WHERE sequence=?1",
        [job.sequence as i64],
        |row| row.get(0),
    )?;
    if queued.capacity() > 32 || queued.as_slice() != job.id.as_bytes().as_slice() {
        return Err(StorageError::Integrity("draft exact queued target"));
    }
    let header = index::header(connection, job.id)?
        .ok_or(StorageError::Integrity("draft exact queued header"))?;
    if header.stage != 1
        || header.queued != Some(job.sequence)
        || index::count(connection, job.id)? != Some(job.links)
    {
        return Err(StorageError::Integrity("draft exact queued target"));
    }
    if job.links != 0 {
        let mut effects = write::effects();
        let mut next = header.clone();
        next.queued = None;
        effects.push(Effect::HeaderChange {
            before: header,
            after: next,
        });
        effects.push(Effect::JobDelete {
            sequence: job.sequence,
            id: job.id,
        });
        let after = write::ledger(resource.draft.as_ref().unwrap().ledger, &effects)?;
        write::transition(resource, "RevivedJob", effects, after)?;
        resource.draft.as_mut().unwrap().stats.jobs_consumed += 1;
        return Ok(());
    }
    // Check body checksum before any child debit, including corrupt cleanup.
    let record = index::get(connection, scope, job.id)?
        .ok_or(StorageError::Integrity("draft zero body missing"))?;
    let deferred_body_bytes = resource
        .draft
        .as_ref()
        .unwrap()
        .stats
        .deferred_body_bytes
        .checked_sub(record.deferred_body_charge())
        .ok_or(StorageError::Integrity("draft body telemetry arithmetic"))?;
    let references = index::references(connection, job.id)?;
    let predecessors = index::predecessors(connection, job.id)?;
    let mut effects = write::effects();
    let mut retiring = header.clone();
    retiring.stage = 2;
    retiring.cursor = 0;
    effects.push(Effect::HeaderChange {
        before: header,
        after: retiring,
    });
    let after = write::ledger(resource.draft.as_ref().unwrap().ledger, &effects)?;
    write::transition(resource, "RetiringStart", effects, after)?;
    for (wave, page) in references.chunks(24).enumerate() {
        let connection = resource.connection.as_ref().unwrap();
        let before = resource.draft.as_ref().unwrap().ledger;
        let mut proposed = before;
        let mut effects = write::effects();
        let header = index::header(connection, job.id)?.unwrap();
        if header.stage != 2 || header.cursor as usize != wave * 24 {
            return Err(StorageError::Integrity("draft Retiring cursor"));
        }
        let mut linked = Vec::with_capacity(page.len());
        for (local, (child, is_linked)) in page.iter().enumerate() {
            effects.push(Effect::ReferenceDelete {
                id: job.id,
                ordinal: (wave * 24 + local) as u16,
                child: *child,
                linked: *is_linked,
            });
            if *is_linked {
                linked.push(*child);
            }
        }
        draft_links::links(connection, &mut effects, &mut proposed, &linked, false)?;
        let mut next = header.clone();
        next.cursor += page.len() as u16;
        effects.push(Effect::HeaderChange {
            before: header,
            after: next,
        });
        let next_job = proposed.next_job;
        proposed = write::ledger(before, &effects)?;
        proposed.next_job = next_job;
        write::transition(resource, "RetiringReferences", effects, proposed)?;
        resource.draft.as_mut().unwrap().stats.links_retired += linked.len() as u64;
    }
    if !predecessors.is_empty() {
        let mut effects = write::effects();
        for (ordinal, (value, provenance)) in predecessors.iter().enumerate() {
            effects.push(Effect::PredecessorDelete {
                id: job.id,
                ordinal: ordinal as u8,
                value: *value,
                provenance: *provenance,
            });
        }
        let after = write::ledger(resource.draft.as_ref().unwrap().ledger, &effects)?;
        write::transition(resource, "RetiringPredecessors", effects, after)?;
    }
    let connection = resource.connection.as_ref().unwrap();
    let header = index::header(connection, job.id)?.unwrap();
    if header.stage != 2
        || header.cursor != header.references
        || !index::references(connection, job.id)?.is_empty()
        || !index::predecessors(connection, job.id)?.is_empty()
        || index::count(connection, job.id)? != Some(0)
    {
        return Err(StorageError::Integrity("draft final zero retirement"));
    }
    let (_, _, body) = record.private_body()?;
    let mut effects = write::effects();
    effects.push(Effect::HeaderDelete(header));
    effects.push(Effect::BodyDelete { id: job.id, body });
    effects.push(Effect::Count {
        id: job.id,
        before: Some(0),
        after: None,
    });
    effects.push(Effect::JobDelete {
        sequence: job.sequence,
        id: job.id,
    });
    let after = write::ledger(resource.draft.as_ref().unwrap().ledger, &effects)?;
    write::transition(resource, "Retired", effects, after)?;
    let stats = &mut resource.draft.as_mut().unwrap().stats;
    stats.deferred_body_bytes = deferred_body_bytes;
    stats.retired += 1;
    stats.jobs_consumed += 1;
    Ok(())
}
pub(crate) fn drain(resource: &mut Resource, scope: &DraftScope) -> StorageResult<()> {
    while let Some(job) = draft_links::next(resource.connection.as_ref().unwrap())? {
        retire(resource, scope, job)?;
    }
    Ok(())
}

pub(crate) fn finish(resource: &mut Resource, scope: &DraftScope) -> StorageResult<()> {
    let connection = resource.connection.as_ref().unwrap();
    let pending: i64 = connection.query_row(
        "SELECT COUNT(*) FROM draft_emissions WHERE accepted=0",
        [],
        |row| row.get(0),
    )?;
    if pending != 0 {
        return Err(StorageError::Integrity("draft final pending emission"));
    }
    let selected = resource.draft.as_ref().unwrap().ledger.selected;
    draft_links::select(resource, selected, None)?;
    drain(resource, scope)?;
    // Fixed keyset windows; each acknowledged removal advances through the exact
    // owned relation. No final whole-set Vec or prefix rescanning of live rows.
    loop {
        let connection = resource.connection.as_ref().unwrap();
        let mut statement =
            connection.prepare("SELECT id,canonical FROM draft_committed ORDER BY id LIMIT 64")?;
        let mut rows = statement.query([])?;
        let mut effects = write::effects();
        while let Some(row) = rows.next()? {
            effects.push(Effect::ResolveDelete {
                id: ObjectId::from_bytes(&row.get::<_, Vec<u8>>(0)?)?,
                canonical: ObjectId::from_bytes(&row.get::<_, Vec<u8>>(1)?)?,
            });
        }
        drop(rows);
        drop(statement);
        if effects.is_empty() {
            break;
        }
        let after = write::ledger(resource.draft.as_ref().unwrap().ledger, &effects)?;
        write::transition(resource, "RetireResolved", effects, after)?;
    }
    loop {
        let connection = resource.connection.as_ref().unwrap();
        let mut statement = connection.prepare(
            "SELECT canonical FROM draft_emissions WHERE accepted=1 ORDER BY canonical LIMIT 64",
        )?;
        let mut rows = statement.query([])?;
        let mut effects = write::effects();
        while let Some(row) = rows.next()? {
            effects.push(Effect::EmissionDelete {
                canonical: ObjectId::from_bytes(&row.get::<_, Vec<u8>>(0)?)?,
            });
        }
        drop(rows);
        drop(statement);
        if effects.is_empty() {
            break;
        }
        let after = write::ledger(resource.draft.as_ref().unwrap().ledger, &effects)?;
        write::transition(resource, "RetireAccepted", effects, after)?;
    }
    let actual:i64=resource.connection.as_ref().unwrap().query_row("SELECT (SELECT COUNT(*) FROM draft_headers)+(SELECT COUNT(*) FROM draft_bodies)+(SELECT COUNT(*) FROM draft_references)+(SELECT COUNT(*) FROM draft_predecessors)+(SELECT COUNT(*) FROM draft_counts)+(SELECT COUNT(*) FROM draft_jobs)+(SELECT COUNT(*) FROM draft_committed)+(SELECT COUNT(*) FROM draft_emissions)",[],|row|row.get(0))?;
    let mut after = resource.draft.as_ref().unwrap().ledger;
    if actual != 0 || after.records != 0 || after.bytes != 0 {
        return Err(StorageError::Integrity("draft final exact EOF"));
    }
    after.ended = true;
    write::transition(resource, "Finished", write::effects(), after)
}
