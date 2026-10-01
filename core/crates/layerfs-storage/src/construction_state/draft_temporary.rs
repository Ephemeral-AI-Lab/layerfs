//! Checked temporary-summary pins and targeted consumed-body supersession.
use super::{
    draft_index as index, draft_links, draft_retire,
    draft_state::WORKING_BYTES,
    draft_state::{Attempt, Drafts, Effect},
    draft_write as write,
    session::Resource,
};
use crate::{StorageError, StorageResult};
use layerfs_content::{file::edit::DraftJob, file::mapping::NodeSummary, ObjectId};

fn working(ids: &[ObjectId]) -> StorageResult<()> {
    let compiled = std::mem::size_of::<Drafts>()
        + std::mem::size_of::<Attempt>()
        + 128 * std::mem::size_of::<Effect>()
        + 65_536
        + 32 * std::mem::size_of::<ObjectId>()
        + 32 * std::mem::size_of::<(ObjectId, u64)>()
        + 32 * std::mem::size_of::<NodeSummary>();
    if ids.len() > 32 || compiled > WORKING_BYTES {
        return Err(StorageError::Integrity("draft temporary compiled window"));
    }
    Ok(())
}
fn ready(resource: &Resource, ids: &[ObjectId]) -> StorageResult<()> {
    let connection = resource.connection.as_ref().unwrap();
    for id in ids {
        let count = index::count(connection, *id)?;
        let header = index::header(connection, *id)?;
        match (count, header) {
            (None, None) => {}
            (Some(_), Some(header)) if header.stage == 1 => {}
            _ => return Err(StorageError::Integrity("draft temporary Ready owner")),
        }
    }
    Ok(())
}
pub(crate) fn change(resource: &mut Resource, ids: &[ObjectId], add: bool) -> StorageResult<()> {
    working(ids)?;
    ready(resource, ids)?;
    let before = resource.draft.as_ref().unwrap().ledger;
    let mut proposed = before;
    let mut effects = write::effects();
    draft_links::links(
        resource.connection.as_ref().unwrap(),
        &mut effects,
        &mut proposed,
        ids,
        add,
    )?;
    if effects.is_empty() {
        return Ok(());
    }
    let next = proposed.next_job;
    proposed = write::ledger(before, &effects)?;
    proposed.next_job = next;
    write::transition(
        resource,
        if add {
            "TemporaryRetain"
        } else {
            "TemporaryRelease"
        },
        effects,
        proposed,
    )
}
pub(crate) fn supersede(
    resource: &mut Resource,
    scope: &layerfs_content::file::edit::DraftScope,
    id: ObjectId,
    expected: Option<ObjectId>,
) -> StorageResult<()> {
    working(&[id])?;
    let before = resource.draft.as_ref().unwrap().ledger;
    if before.selected != expected {
        return Err(StorageError::Integrity("draft selected root expected"));
    }
    ready(resource, &[id])?;
    let selected = expected == Some(id);
    let mut proposed = before;
    let mut effects = write::effects();
    let ids = [id, id];
    draft_links::links(
        resource.connection.as_ref().unwrap(),
        &mut effects,
        &mut proposed,
        &ids[..1 + usize::from(selected)],
        false,
    )?;
    let next = proposed.next_job;
    proposed = write::ledger(before, &effects)?;
    proposed.next_job = next;
    if selected {
        proposed.selected = None;
    }
    if !effects.is_empty() || selected {
        write::transition(resource, "TemporarySupersede", effects, proposed)?;
    }
    let connection = resource.connection.as_ref().unwrap();
    if index::count(connection, id)? == Some(0) {
        let header =
            index::header(connection, id)?.ok_or(StorageError::Integrity("draft zero header"))?;
        let sequence = header
            .queued
            .ok_or(StorageError::Integrity("draft zero queued job"))?;
        draft_retire::retire_exact(
            resource,
            scope,
            DraftJob {
                sequence,
                id,
                links: 0,
            },
        )?;
    }
    Ok(())
}
