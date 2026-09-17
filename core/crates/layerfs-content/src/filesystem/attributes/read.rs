//! Bounded attribute acquisition: multi-key reads and typed portable fields.
//!
//! Key demands descend the tree once per level with requests grouped by child
//! page, so a batch costs one authenticated read per shared page instead of one
//! traversal per key, and duplicate demands are answered in demand order. The
//! typed portable fields are read from their fixed-size value roots; every other
//! domain is returned as opaque bytes.

use std::collections::BTreeMap;

use crate::error::{ContentError, ContentResult};
use crate::filesystem::attributes::codec::{decode_attribute_page, AttributeEntry, AttributePage};
use crate::filesystem::attributes::keys::AttributeKey;
use crate::filesystem::attributes::portable::PortableMetadata;
use crate::filesystem::attributes::value::read_value;
use crate::filesystem::limits::{MAXIMUM_ATTRIBUTE_VALUE_BYTES, MAXIMUM_PAGE_BYTES};
use crate::object::inode_leaf::InodeKind;
use crate::object::{AuthenticatedObjects, ObjectId};

/// Work one attribute read performed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AttributeReadWork {
    /// Pages read.
    pub pages_read: u64,
    /// Canonical read waves issued.
    pub read_waves: u64,
    /// Value roots read.
    pub values_read: u64,
}

/// Resolves one key to its value root.
pub fn lookup(
    reader: &dyn AuthenticatedObjects,
    root: ObjectId,
    key: &AttributeKey,
    work: &mut AttributeReadWork,
) -> ContentResult<Option<ObjectId>> {
    let mut answers = lookup_many(reader, root, std::slice::from_ref(key), work)?;
    Ok(answers.pop().flatten().map(|entry| entry.value_root))
}

/// Resolves many keys, sharing every level's demand wave.
pub fn lookup_many(
    reader: &dyn AuthenticatedObjects,
    root: ObjectId,
    keys: &[AttributeKey],
    work: &mut AttributeReadWork,
) -> ContentResult<Vec<Option<AttributeEntry>>> {
    let mut answers: Vec<Option<AttributeEntry>> = vec![None; keys.len()];
    if keys.is_empty() {
        return Ok(answers);
    }
    let mut level: Vec<(ObjectId, bool, Option<AttributeKey>, Vec<usize>)> =
        vec![(root, true, None, (0..keys.len()).collect())];
    let mut depth = 0_u8;
    while !level.is_empty() {
        if depth > crate::filesystem::limits::MAXIMUM_TREE_LEVEL {
            return Err(ContentError::MappingDepthExceeded);
        }
        let ids = level.iter().map(|(id, ..)| *id).collect::<Vec<_>>();
        let pages = reader.read_canonical_batch(&ids)?;
        if pages.len() != ids.len() {
            return Err(ContentError::BatchCardinality {
                requested: ids.len(),
                returned: pages.len(),
            });
        }
        work.read_waves = work.read_waves.saturating_add(1);
        let mut next = Vec::new();
        for ((_, root_page, maximum, indices), canonical) in level.into_iter().zip(pages) {
            work.pages_read = work.pages_read.saturating_add(1);
            let page = decode_checked(&canonical, root_page, maximum.as_ref())?;
            match page {
                AttributePage::Leaf { entries, .. } => {
                    for index in indices {
                        if let Ok(position) =
                            entries.binary_search_by(|entry| entry.key.cmp(&keys[index]))
                        {
                            answers[index] = Some(entries[position].clone());
                        }
                    }
                }
                AttributePage::Branch { children, .. } => {
                    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
                    for index in indices {
                        let position = children.partition_point(|(key, _)| key < &keys[index]);
                        if position < children.len() {
                            groups.entry(position).or_default().push(index);
                        }
                    }
                    for (position, group) in groups {
                        let (maximum, child) = children[position].clone();
                        next.push((child, false, Some(maximum), group));
                    }
                }
            }
        }
        level = next;
        depth = depth.saturating_add(1);
    }
    Ok(answers)
}

/// Reads the typed portable fields of one inode.
///
/// Both fields are optional in the grammar but required by the checked portable
/// contract; a directory whose mode or mtime is missing, or whose stored value
/// does not satisfy the typed grammar, fails explicitly.
pub fn read_portable(
    reader: &dyn AuthenticatedObjects,
    root: ObjectId,
    kind: InodeKind,
    work: &mut AttributeReadWork,
) -> ContentResult<PortableMetadata> {
    let keys = [
        AttributeKey::new("portable".to_owned(), b"mode".to_vec())?,
        AttributeKey::new("portable".to_owned(), b"mtime".to_vec())?,
    ];
    let entries = lookup_many(reader, root, &keys, work)?;
    let mode_root = entries
        .first()
        .and_then(|entry| entry.as_ref())
        .map(|entry| entry.value_root)
        .ok_or(ContentError::InvalidRecord("mode missing"))?;
    let mtime_root = entries
        .get(1)
        .and_then(|entry| entry.as_ref())
        .map(|entry| entry.value_root)
        .ok_or(ContentError::InvalidRecord("mtime missing"))?;
    let mode_bytes = read_value(reader, mode_root, 4)?;
    work.values_read = work.values_read.saturating_add(1);
    let mtime_bytes = read_value(reader, mtime_root, 12)?;
    work.values_read = work.values_read.saturating_add(1);
    let (mtime_seconds, mtime_nanoseconds) = PortableMetadata::decode_mtime(&mtime_bytes)?;
    let metadata = PortableMetadata {
        mode: PortableMetadata::decode_mode(&mode_bytes, kind)?,
        mtime_seconds,
        mtime_nanoseconds,
    };
    metadata.validate(kind)?;
    Ok(metadata)
}

/// Reads one opaque attribute value with an explicit bound.
pub fn read_opaque(
    reader: &dyn AuthenticatedObjects,
    root: ObjectId,
    key: &AttributeKey,
    maximum_bytes: usize,
    work: &mut AttributeReadWork,
) -> ContentResult<Option<Vec<u8>>> {
    let Some(entry) = lookup(reader, root, key, work)? else {
        return Ok(None);
    };
    work.values_read = work.values_read.saturating_add(1);
    Ok(Some(read_value(reader, entry, maximum_bytes)?))
}

/// Reads one opaque attribute value under the profile's declared value bound.
pub fn read_value_bounded(
    reader: &dyn AuthenticatedObjects,
    root: ObjectId,
    key: &AttributeKey,
    work: &mut AttributeReadWork,
) -> ContentResult<Option<Vec<u8>>> {
    read_opaque(reader, root, key, MAXIMUM_ATTRIBUTE_VALUE_BYTES, work)
}

fn decode_checked(
    canonical: &[u8],
    root: bool,
    maximum: Option<&AttributeKey>,
) -> ContentResult<AttributePage> {
    if canonical.len() > MAXIMUM_PAGE_BYTES {
        return Err(ContentError::ObjectLimitExceeded {
            limit: MAXIMUM_PAGE_BYTES,
            actual: canonical.len(),
        });
    }
    let page = decode_attribute_page(canonical)?;
    if !root && !page.filled()? {
        return Err(ContentError::NonCanonicalPagePartition);
    }
    if let (Some(maximum), Some(last)) = (
        maximum,
        match &page {
            AttributePage::Leaf { entries, .. } => entries.last().map(|entry| &entry.key),
            AttributePage::Branch { children, .. } => children.last().map(|(key, _)| key),
        },
    ) {
        if last != maximum {
            return Err(ContentError::InvalidRecord("attribute child summary"));
        }
    }
    Ok(page)
}
