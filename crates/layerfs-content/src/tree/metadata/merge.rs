use super::tree::{MetadataCursor, MetadataTreeBuilder};
use crate::file::rope::ObjectStore;
use crate::{CoreResult, ObjectId};

/// Three-way merges ordered metadata entries with only bounded tree frontiers.
/// Conflicting values for the same key return `None`.
pub fn merge_metadata_roots<S: ObjectStore>(
    store: &mut S,
    base: ObjectId,
    source: ObjectId,
    destination: ObjectId,
) -> CoreResult<Option<ObjectId>> {
    if source == base || source == destination {
        return Ok(Some(destination));
    }
    if destination == base {
        return Ok(Some(source));
    }
    let mut base_cursor = MetadataCursor::new(base);
    let mut source_cursor = MetadataCursor::new(source);
    let mut destination_cursor = MetadataCursor::new(destination);
    let mut base_entry = base_cursor.next(store)?;
    let mut source_entry = source_cursor.next(store)?;
    let mut destination_entry = destination_cursor.next(store)?;
    let mut builder = MetadataTreeBuilder::new();
    loop {
        let key = [
            base_entry.as_ref().map(|entry| &entry.key),
            source_entry.as_ref().map(|entry| &entry.key),
            destination_entry.as_ref().map(|entry| &entry.key),
        ]
        .into_iter()
        .flatten()
        .min()
        .cloned();
        let Some(key) = key else {
            return builder.finish(store).map(Some);
        };
        let base_value = if base_entry.as_ref().is_some_and(|entry| entry.key == key) {
            let value = base_entry.take();
            base_entry = base_cursor.next(store)?;
            value
        } else {
            None
        };
        let source_value = if source_entry.as_ref().is_some_and(|entry| entry.key == key) {
            let value = source_entry.take();
            source_entry = source_cursor.next(store)?;
            value
        } else {
            None
        };
        let destination_value = if destination_entry
            .as_ref()
            .is_some_and(|entry| entry.key == key)
        {
            let value = destination_entry.take();
            destination_entry = destination_cursor.next(store)?;
            value
        } else {
            None
        };
        let selected = if source_value == base_value || source_value == destination_value {
            destination_value
        } else if destination_value == base_value {
            source_value
        } else {
            return Ok(None);
        };
        if let Some(entry) = selected {
            builder.push(store, entry)?;
        }
    }
}
