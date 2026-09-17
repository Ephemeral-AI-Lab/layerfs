//! Shared ancestor acquisition for ordered inode demands.
//!
//! A batch of demands descends the table once per level: requests that share a
//! child page share its single authenticated read, and one filesystem operation
//! reuses its checked root and scope instead of reloading them. Results keep
//! demand order, including duplicates, and an absent serial is reported as absent
//! rather than as a missing object.

use std::collections::BTreeMap;

use crate::error::{ContentError, ContentResult};
use crate::filesystem::limits::MAXIMUM_PAGE_BYTES;
use crate::filesystem::sorted::format::{CompactInodes, Format};
use crate::object::inode_leaf::InodeValue;
use crate::object::{AuthenticatedObjects, ObjectId};

/// Work one inode read performed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InodeReadWork {
    /// Pages read.
    pub pages_read: u64,
    /// Canonical read waves issued.
    pub read_waves: u64,
    /// Serial demands answered.
    pub demands: u64,
}

/// A checked inode-table root bound to its scope and root serial.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InodeTable {
    /// Root page identity.
    pub root: ObjectId,
    /// Root directory serial this table belongs to.
    pub root_serial: u64,
}

/// Reads one inode value by serial.
pub fn lookup(
    reader: &dyn AuthenticatedObjects,
    table: InodeTable,
    serial: u64,
    work: &mut InodeReadWork,
) -> ContentResult<Option<InodeValue>> {
    if serial == 0 {
        return Err(ContentError::InvalidRecord("inode serial"));
    }
    let mut id = table.root;
    let mut root_page = true;
    let mut expected: Option<(u8, u64)> = None;
    loop {
        let canonical = reader.read_canonical(id)?;
        work.pages_read = work.pages_read.saturating_add(1);
        work.read_waves = work.read_waves.saturating_add(1);
        let page = page(&canonical, root_page, expected)?;
        match page {
            Page::Leaf { maximum, rows } => {
                work.demands = work.demands.saturating_add(1);
                if serial > maximum {
                    return Ok(None);
                }
                return Ok(rows
                    .into_iter()
                    .find(|(key, _)| *key == serial)
                    .map(|(_, value)| value));
            }
            Page::Branch { level, children } => {
                let index = children.partition_point(|(key, _)| *key < serial);
                let Some((maximum, child)) = children.get(index).copied() else {
                    return Ok(None);
                };
                id = child;
                root_page = false;
                expected = Some((
                    level
                        .checked_sub(1)
                        .ok_or(ContentError::MappingDepthExceeded)?,
                    maximum,
                ));
            }
        }
    }
}

/// Reads many inode values, sharing every level's demand wave.
pub fn lookup_many(
    reader: &dyn AuthenticatedObjects,
    table: InodeTable,
    serials: &[u64],
    work: &mut InodeReadWork,
) -> ContentResult<Vec<Option<InodeValue>>> {
    let mut answers = vec![None; serials.len()];
    if serials.is_empty() {
        return Ok(answers);
    }
    for serial in serials {
        if *serial == 0 {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
    }
    let mut level: Vec<(ObjectId, bool, Option<(u8, u64)>, Vec<usize>)> =
        vec![(table.root, true, None, (0..serials.len()).collect())];
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
        for ((_, root_page, expected, indices), canonical) in level.into_iter().zip(pages) {
            work.pages_read = work.pages_read.saturating_add(1);
            match page(&canonical, root_page, expected)? {
                Page::Leaf { rows, .. } => {
                    for index in indices {
                        work.demands = work.demands.saturating_add(1);
                        answers[index] = rows
                            .iter()
                            .find(|(key, _)| *key == serials[index])
                            .map(|(_, value)| *value);
                    }
                }
                Page::Branch { level, children } => {
                    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
                    for index in indices {
                        let position = children.partition_point(|(key, _)| *key < serials[index]);
                        if position < children.len() {
                            groups.entry(position).or_default().push(index);
                        }
                    }
                    for (position, group) in groups {
                        let (maximum, child) = children[position];
                        next.push((child, false, Some((level - 1, maximum)), group));
                    }
                }
            }
        }
        level = next;
        depth = depth.saturating_add(1);
    }
    Ok(answers)
}

/// One decoded inode page, reduced to what a demand walk needs.
enum Page {
    Leaf {
        maximum: u64,
        rows: Vec<(u64, InodeValue)>,
    },
    Branch {
        level: u8,
        children: Vec<(u64, ObjectId)>,
    },
}

fn page(canonical: &[u8], root: bool, expected: Option<(u8, u64)>) -> ContentResult<Page> {
    if canonical.len() > MAXIMUM_PAGE_BYTES {
        return Err(ContentError::ObjectLimitExceeded {
            limit: MAXIMUM_PAGE_BYTES,
            actual: canonical.len(),
        });
    }
    let wire = <CompactInodes as Format>::decode(canonical)?;
    if !root && !<CompactInodes as Format>::filled(wire.size, wire.entries.len(), wire.level) {
        return Err(ContentError::NonCanonicalPagePartition);
    }
    if let Some((level, maximum)) = expected {
        if wire.level != level || wire.entries.last().map(|entry| entry.key) != Some(maximum) {
            return Err(ContentError::InvalidRecord("inode child summary"));
        }
    }
    if wire.level == 0 {
        Ok(Page::Leaf {
            maximum: wire.entries.last().map_or(0, |entry| entry.key),
            rows: wire
                .entries
                .into_iter()
                .map(|entry| {
                    Ok((
                        entry.key,
                        entry
                            .value
                            .ok_or(ContentError::InvalidRecord("inode leaf value"))?,
                    ))
                })
                .collect::<ContentResult<Vec<_>>>()?,
        })
    } else {
        Ok(Page::Branch {
            level: wire.level,
            children: wire
                .entries
                .into_iter()
                .map(|entry| {
                    Ok((
                        entry.key,
                        entry
                            .child
                            .ok_or(ContentError::InvalidRecord("inode child"))?,
                    ))
                })
                .collect::<ContentResult<Vec<_>>>()?,
        })
    }
}
