//! Exact private attempt facts and replayable numbered edit records.
use super::{
    owner::Facts,
    state::{decode, key, Shared, CONTEXT, EDITS},
};
use crate::OverlayOperationRecords;
use layerfs_content::{ContentError, ContentResult, Edit};

/// Offset of the byte that says the sealed edits are a script: edits that
/// may change length anywhere, each at its own offset in the final file.
const SCRIPTED: usize = 3;
/// `sealed` is none for the open context and otherwise says whether the file
/// holds pieces, which selects the form its edits are checked against.
pub(super) fn encode(facts: Facts, count: usize, sealed: Option<bool>) -> ContentResult<Vec<u8>> {
    let mut value = Vec::with_capacity(256);
    value.extend_from_slice(&[
        2,
        u8::from(sealed.is_some()),
        u8::from(facts.file_root.is_some()),
        u8::from(sealed == Some(true)),
    ]);
    for number in [
        facts.reader.capture().engine(),
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
pub(super) fn verify<P: OverlayOperationRecords + ?Sized>(
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
pub(super) fn edit<P: OverlayOperationRecords + ?Sized>(
    shared: &Shared<'_, P>,
    expected: &[u8],
    facts: Facts,
    count: usize,
    index: usize,
) -> ContentResult<Edit> {
    // A cached edit was validated against the sealed context when it was
    // read; only a read of the records needs that check again.
    if let Some((cached, edit)) = shared.borrow().cache {
        if cached == index {
            return Ok(edit);
        }
    }
    if index >= count {
        verify(shared, expected)?;
        return Err(shared.borrow_mut().content(ContentError::InvalidEdit {
            what: "captured file edit index",
        }));
    }
    let number = u64::try_from(index).map_err(|_| ContentError::LengthOverflow)?;
    // The sealed context and the edit are one read job; the context is
    // checked first, as when it was read alone.
    let mut owner = shared.borrow_mut();
    let [actual, raw] = owner.get_pair([key(CONTEXT, 0), key(EDITS, number)])?;
    if actual.as_deref() != Some(expected) {
        return Err(owner.content(ContentError::InvalidRecord("captured file context")));
    }
    let raw = raw.ok_or(ContentError::InvalidRecord("missing captured edit"));
    let edit = match raw.and_then(|raw| decode(&raw)) {
        Ok(edit) => edit,
        Err(original) => return Err(owner.content(original)),
    };
    let trailing = index + 1 == count && facts.base_size != facts.final_size;
    let valid = if expected.get(SCRIPTED) == Some(&1) {
        // A script edit removes base bytes and supplies final bytes at its
        // own final offset; the first one starts where the base still does.
        edit.start() <= edit.end()
            && (edit.start() < edit.end() || edit.replacement_len() != 0)
            && edit
                .start()
                .checked_add(edit.replacement_len())
                .is_some_and(|end| end <= facts.final_size)
            && (index != 0 || edit.end() <= facts.base_size)
    } else if trailing && facts.final_size < facts.base_size {
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
