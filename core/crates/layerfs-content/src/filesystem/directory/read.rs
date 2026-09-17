//! Grouped directory lookups and bounded, resumable listing.
//!
//! A lookup walks one root-to-leaf path. A batch of lookups shares ancestor
//! acquisition instead of descending from the root once per name, and returns
//! results in demand order including duplicates. Listing is bounded by both entry
//! count and bytes, and its continuation is the last delivered name, so a caller
//! resumes exactly where it stopped across variable-length names and page
//! boundaries without rescanning or skipping.

use std::collections::BTreeMap;

use crate::error::{ContentError, ContentResult};
use crate::filesystem::path::PathName;
use crate::filesystem::sorted::finish::DirectoryRoot;
use crate::filesystem::sorted::format::{
    CompactDirectory, Format, Wire, DIRECTORY_LEAF_SERIAL_BYTES, NAME_LENGTH_BYTES,
};
use crate::object::{AuthenticatedObjects, ObjectId};

/// Work one directory read performed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DirectoryReadWork {
    /// Pages read.
    pub pages_read: u64,
    /// Canonical read waves issued.
    pub read_waves: u64,
    /// Names resolved.
    pub names_resolved: u64,
}

/// One bounded listing page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListingPage {
    /// Bindings in name order.
    pub entries: Vec<(PathName, u64)>,
    /// Name to resume after, when the page is not the end of the directory.
    pub continuation: Option<PathName>,
}

/// Resolves one name to its inode serial.
pub fn lookup(
    reader: &dyn AuthenticatedObjects,
    root: DirectoryRoot,
    name: &PathName,
) -> ContentResult<Option<u64>> {
    let mut work = DirectoryReadWork::default();
    Ok(lookup_counted(reader, root, name, &mut work)?.map(|entry| entry.1))
}

/// Resolves one name and reports the work it cost.
pub fn lookup_counted(
    reader: &dyn AuthenticatedObjects,
    root: DirectoryRoot,
    name: &PathName,
    work: &mut DirectoryReadWork,
) -> ContentResult<Option<(PathName, u64)>> {
    let mut id = root.0;
    let mut root_page = true;
    let mut expected: Option<(u8, Option<PathName>)> = None;
    let mut minimum: Option<PathName> = None;
    loop {
        let wire = load(
            reader,
            id,
            root_page,
            expected.as_ref(),
            minimum.as_ref(),
            work,
        )?;
        match wire.level {
            0 => {
                return Ok(wire
                    .entries
                    .into_iter()
                    .find(|entry| entry.key == *name)
                    .map(|entry| (entry.key, entry.value.expect("directory leaf value"))))
            }
            level => {
                let index = wire.entries.partition_point(|entry| entry.key < *name);
                let Some(entry) = wire.entries.get(index) else {
                    return Ok(None);
                };
                minimum = index
                    .checked_sub(1)
                    .map(|previous| wire.entries[previous].key.clone())
                    .or(minimum);
                id = entry
                    .child
                    .ok_or(ContentError::InvalidRecord("directory child"))?;
                root_page = false;
                expected = Some((
                    level
                        .checked_sub(1)
                        .ok_or(ContentError::MappingDepthExceeded)?,
                    Some(entry.key.clone()),
                ));
            }
        }
    }
}

/// Resolves many names, sharing each level's demand wave across the batch.
///
/// Requests that descend into the same child page are grouped, so one page is
/// read once per level instead of once per name, and the wave is issued as one
/// authenticated group read. Results keep demand order and include every
/// duplicate request.
pub fn lookup_many(
    reader: &dyn AuthenticatedObjects,
    root: DirectoryRoot,
    names: &[PathName],
    work: &mut DirectoryReadWork,
) -> ContentResult<Vec<Option<u64>>> {
    let mut answers = vec![None; names.len()];
    if names.is_empty() {
        return Ok(answers);
    }
    let mut level: Vec<(ObjectId, bool, Option<PathName>, Vec<usize>)> =
        vec![(root.0, true, None, (0..names.len()).collect())];
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
            let wire = decode_checked(&canonical, root_page, None, maximum.as_ref())?;
            if wire.level == 0 {
                for index in indices {
                    if let Ok(position) = wire
                        .entries
                        .binary_search_by(|entry| entry.key.cmp(&names[index]))
                    {
                        answers[index] = wire.entries[position]
                            .value
                            .expect("directory leaf value")
                            .into();
                    }
                }
                continue;
            }
            let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
            for index in indices {
                let position = wire
                    .entries
                    .partition_point(|entry| entry.key < names[index]);
                if position < wire.entries.len() {
                    groups.entry(position).or_default().push(index);
                }
            }
            for (position, group) in groups {
                let entry = &wire.entries[position];
                next.push((
                    entry
                        .child
                        .ok_or(ContentError::InvalidRecord("directory child"))?,
                    false,
                    Some(entry.key.clone()),
                    group,
                ));
            }
            let _ = level;
        }
        level = next;
        depth = depth.saturating_add(1);
    }
    work.names_resolved = work.names_resolved.saturating_add(names.len() as u64);
    Ok(answers)
}

/// Lists bindings strictly after `after`, bounded by count and bytes.
///
/// The continuation is the last name delivered, so a resumed call starts exactly
/// after it. Names are charged by their real encoded width, so a page stops on
/// the entry that would exceed the byte bound rather than on a fixed count, and
/// a subtree whose largest key is not past the cursor is skipped without a read.
pub fn list_after(
    reader: &dyn AuthenticatedObjects,
    root: DirectoryRoot,
    after: Option<&PathName>,
    max_entries: usize,
    max_bytes: usize,
    work: &mut DirectoryReadWork,
) -> ContentResult<ListingPage> {
    if max_entries == 0 || max_bytes == 0 {
        return Err(ContentError::InvalidRecord("listing limit"));
    }
    let mut entries: Vec<(PathName, u64)> = Vec::new();
    let mut bytes = 0_usize;
    let mut stack: Vec<(ObjectId, bool, Option<PathName>)> = vec![(root.0, true, None)];
    while let Some((id, root_page, maximum)) = stack.pop() {
        if after
            .zip(maximum.as_ref())
            .is_some_and(|(cursor, maximum)| maximum <= cursor)
        {
            continue;
        }
        let canonical = reader.read_canonical(id)?;
        work.pages_read = work.pages_read.saturating_add(1);
        work.read_waves = work.read_waves.saturating_add(1);
        let wire = decode_checked(&canonical, root_page, None, maximum.as_ref())?;
        if wire.level == 0 {
            for entry in wire.entries {
                if after.is_some_and(|cursor| entry.key <= *cursor) {
                    continue;
                }
                let width =
                    NAME_LENGTH_BYTES + DIRECTORY_LEAF_SERIAL_BYTES + entry.key.as_bytes().len();
                if entries.len() == max_entries || bytes + width > max_bytes {
                    return Ok(ListingPage {
                        continuation: entries.last().map(|(name, _)| name.clone()),
                        entries,
                    });
                }
                bytes += width;
                entries.push((entry.key, entry.value.expect("directory leaf value")));
            }
            continue;
        }
        for entry in wire.entries.into_iter().rev() {
            stack.push((
                entry
                    .child
                    .ok_or(ContentError::InvalidRecord("directory child"))?,
                false,
                Some(entry.key),
            ));
        }
    }
    Ok(ListingPage {
        entries,
        continuation: None,
    })
}

/// Decodes one page and applies the context checks a read must keep.
fn decode_checked(
    canonical: &[u8],
    root: bool,
    expected_level: Option<u8>,
    maximum: Option<&PathName>,
) -> ContentResult<Wire<PathName, u64>> {
    if canonical.len() > crate::filesystem::limits::MAXIMUM_PAGE_BYTES {
        return Err(ContentError::ObjectLimitExceeded {
            limit: crate::filesystem::limits::MAXIMUM_PAGE_BYTES,
            actual: canonical.len(),
        });
    }
    let wire = <CompactDirectory as Format>::decode(canonical)?;
    if (!root
        && !<CompactDirectory as Format>::filled(canonical.len(), wire.entries.len(), wire.level))
        || (wire.level > 0 && wire.entries.len() < 2)
    {
        return Err(ContentError::NonCanonicalPagePartition);
    }
    if expected_level.is_some_and(|level| wire.level != level) {
        return Err(ContentError::InvalidRecord("directory child summary"));
    }
    if maximum.is_some_and(|maximum| wire.entries.last().map(|entry| &entry.key) != Some(maximum)) {
        return Err(ContentError::InvalidRecord("directory child summary"));
    }
    Ok(wire)
}

fn load(
    reader: &dyn AuthenticatedObjects,
    id: ObjectId,
    root: bool,
    expected: Option<&(u8, Option<PathName>)>,
    minimum: Option<&PathName>,
    work: &mut DirectoryReadWork,
) -> ContentResult<Wire<PathName, u64>> {
    work.pages_read = work.pages_read.saturating_add(1);
    work.read_waves = work.read_waves.saturating_add(1);
    let canonical = reader.read_canonical(id)?;
    let wire = decode_checked(
        &canonical,
        root,
        expected.map(|(level, _)| *level),
        expected.and_then(|(_, maximum)| maximum.as_ref()),
    )?;
    if minimum.is_some_and(|minimum| {
        wire.entries
            .first()
            .is_some_and(|entry| &entry.key <= minimum)
    }) {
        return Err(ContentError::NonCanonicalOrdering);
    }
    Ok(wire)
}
