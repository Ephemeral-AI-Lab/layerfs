//! Exact Pending owner, known acceptance and paged draft resolutions.
use super::{draft_index as index, draft_state::Effect, draft_write as write, session::Resource};
use crate::error::{StorageError, StorageResult};
use layerfs_content::ObjectId;
use rusqlite::{params, OptionalExtension};

pub(crate) fn begin(
    resource: &mut Resource,
    draft: ObjectId,
    canonical: ObjectId,
) -> StorageResult<bool> {
    let connection = resource.connection.as_ref().unwrap();
    if index::header(connection, draft)?.is_none_or(|header| header.stage != 1)
        || index::resolved(connection, draft)?.is_some()
    {
        return Err(StorageError::Integrity(
            "draft emission Ready unresolved owner",
        ));
    }
    let prior = resource.draft.as_ref().unwrap().ledger;
    let existing = index::emission(connection, canonical)?;
    if existing == Some(false) {
        return Err(StorageError::Integrity("draft emission already Pending"));
    }
    let required = prior
        .bytes
        .checked_add(87 + if existing.is_none() { 95 } else { 0 })
        .ok_or(StorageError::Integrity("draft emission prospective bytes"))?;
    let count = prior
        .records
        .checked_add(1 + u64::from(existing.is_none()))
        .ok_or(StorageError::Integrity(
            "draft emission prospective records",
        ))?;
    let capacity = resource.draft.as_ref().unwrap().scope.capacity();
    if count > capacity.records() || required as u64 > capacity.encoded_bytes() {
        return Err(StorageError::CapacityExceeded {
            what: "draft emission/resolution",
            limit: capacity.encoded_bytes(),
            actual: required as u64,
        });
    }
    resource.draft.as_mut().unwrap().stats.peak_bytes = resource
        .draft
        .as_ref()
        .unwrap()
        .stats
        .peak_bytes
        .max(required);
    if existing == Some(true) {
        return Ok(false);
    }
    let mut effects = write::effects();
    effects.push(Effect::EmissionInsert { canonical, draft });
    let after = write::ledger(prior, &effects)?;
    write::transition(resource, "EmissionPending", effects, after)?;
    Ok(true)
}
pub(crate) fn accepted(
    resource: &mut Resource,
    draft: ObjectId,
    canonical: ObjectId,
) -> StorageResult<()> {
    let connection = resource.connection.as_ref().unwrap();
    if index::resolved(connection, draft)?.is_some()
        || index::header(connection, draft)?.is_none_or(|header| header.stage != 1)
    {
        return Err(StorageError::Integrity(
            "draft accepted unresolved Ready owner",
        ));
    }
    let stage = index::emission(connection, canonical)?
        .ok_or(StorageError::Integrity("draft accepted emission absent"))?;
    let mut effects = write::effects();
    if !stage {
        let pending: Option<Vec<u8>> = connection
            .query_row(
                "SELECT pending_draft FROM draft_emissions WHERE canonical=?1",
                params![canonical.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        if pending.as_deref() != Some(draft.as_bytes().as_slice()) {
            return Err(StorageError::Integrity("draft Pending exact owner"));
        }
        effects.push(Effect::EmissionAccept { canonical, draft });
    }
    effects.push(Effect::ResolveInsert {
        id: draft,
        canonical,
    });
    let after = write::ledger(resource.draft.as_ref().unwrap().ledger, &effects)?;
    write::transition(resource, "EmissionAcceptedResolved", effects, after)
}
