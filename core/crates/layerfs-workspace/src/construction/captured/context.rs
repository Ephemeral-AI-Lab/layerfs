//! Exact private attempt facts and replayable numbered edit records.
use super::{
    owner::Facts,
    state::{decode, key, Shared, CONTEXT, EDITS},
};
use crate::OverlayScratch;
use layerfs_content::{ContentError, ContentResult, Edit};

pub(super) fn encode(facts: Facts, count: usize, sealed: bool) -> ContentResult<Vec<u8>> {
    let mut value = Vec::with_capacity(256);
    value.extend_from_slice(&[1, u8::from(sealed), u8::from(facts.file_root.is_some())]);
    for number in [
        facts.reader.capture().route().namespace() as u64,
        facts.reader.owner_id(),
        facts.reader.installed_floor(),
        facts.reader.capture().generation.number() as u64,
        facts.reader.capture().revision as u64,
        facts.records_scope.file_scope,
        facts.serial,
        facts.base_size,
        facts.final_size,
        u64::try_from(count).map_err(|_| ContentError::LengthOverflow)?,
    ] {
        value.extend_from_slice(&number.to_be_bytes());
    }
    value.extend_from_slice(&facts.reader.root());
    value.extend_from_slice(facts.scope.object().as_bytes());
    value.extend_from_slice(facts.profile.as_bytes());
    value.extend_from_slice(layerfs_content::file::mapping::profile_id().as_bytes());
    value.extend_from_slice(&facts.file_root.map_or([0; 32], |root| root.to_bytes()));
    Ok(value)
}
pub(super) fn verify<P: OverlayScratch + ?Sized>(
    shared: &Shared<'_, P>,
    expected: &[u8],
) -> ContentResult<()> {
    let mut owner = shared.borrow_mut();
    let actual = owner.get(key(CONTEXT, 0))?;
    if actual.as_deref() != Some(expected) {
        return Err(owner.content(ContentError::InvalidRecord("captured file context")));
    }
    Ok(())
}
pub(super) fn edit<P: OverlayScratch + ?Sized>(
    shared: &Shared<'_, P>,
    expected: &[u8],
    facts: Facts,
    count: usize,
    index: usize,
) -> ContentResult<Edit> {
    verify(shared, expected)?;
    let mut owner = shared.borrow_mut();
    if index >= count {
        return Err(owner.content(ContentError::InvalidEdit {
            what: "captured file edit index",
        }));
    }
    if let Some((cached, edit)) = owner.cache {
        if cached == index {
            return Ok(edit);
        }
    }
    let number = u64::try_from(index).map_err(|_| ContentError::LengthOverflow)?;
    let raw = owner
        .get(key(EDITS, number))?
        .ok_or(ContentError::InvalidRecord("missing captured edit"));
    let edit = match raw.and_then(|raw| decode(&raw)) {
        Ok(edit) => edit,
        Err(original) => return Err(owner.content(original)),
    };
    let trailing = index + 1 == count && facts.base_size != facts.final_size;
    let valid = if trailing && facts.final_size < facts.base_size {
        edit == Edit::delete(facts.final_size, facts.base_size)
    } else if trailing {
        edit == Edit::insert(facts.base_size, facts.final_size - facts.base_size)
    } else {
        edit.start() < edit.end()
            && edit.end() <= facts.base_size.min(facts.final_size)
            && edit.replacement_len() == edit.end() - edit.start()
    };
    if !valid {
        return Err(owner.content(ContentError::InvalidRecord("captured normalized edit")));
    }
    owner.cache = Some((index, edit));
    Ok(edit)
}
