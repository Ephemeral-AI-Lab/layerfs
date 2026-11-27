//! Shared ancestor acquisition for ordered inode demands.
//!
//! A batch of demands descends the table once per level: requests that share a
//! child page share its single authenticated read, and one filesystem operation
//! reuses its checked root and scope instead of reloading them. Results keep
//! demand order, including duplicates, and an absent serial is reported as absent
//! rather than as a missing object.

use std::collections::BTreeMap;

use crate::error::{ContentError, ContentResult};
use crate::filesystem::decoded::{self, Role};
use crate::filesystem::sorted::format::{CompactInodes, Format, Wire};
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
        let decoded = decoded::page(reader, id, Role::Inodes)?;
        work.pages_read = work.pages_read.saturating_add(1);
        work.read_waves = work.read_waves.saturating_add(1);
        match page(decoded.inode_wire()?, root_page, expected)? {
            Page::Leaf { maximum, rows } => {
                work.demands = work.demands.saturating_add(1);
                if serial > maximum {
                    return Ok(None);
                }
                return Ok(rows.find(serial));
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
    type Demand = (ObjectId, bool, Option<(u8, u64)>, Vec<usize>);
    let mut level: Vec<Demand> = vec![(table.root, true, None, (0..serials.len()).collect())];
    let mut depth = 0_u8;
    while !level.is_empty() {
        if depth > crate::filesystem::limits::MAXIMUM_TREE_LEVEL {
            return Err(ContentError::MappingDepthExceeded);
        }
        let ids = level.iter().map(|(id, ..)| *id).collect::<Vec<_>>();
        let pages = decoded::pages(reader, &ids, Role::Inodes)?;
        work.read_waves = work.read_waves.saturating_add(1);
        let mut next = Vec::new();
        for ((_, root_page, expected, indices), decoded) in level.into_iter().zip(pages) {
            work.pages_read = work.pages_read.saturating_add(1);
            match page(decoded.inode_wire()?, root_page, expected)? {
                Page::Leaf { rows, .. } => {
                    for index in indices {
                        work.demands = work.demands.saturating_add(1);
                        answers[index] = rows.find(serials[index]);
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

/// Reads inode values from several tables at once: answer `i` is
/// `demands[i].1` in table `demands[i].0`, in demand order, duplicates
/// included.
///
/// Every table descends together, one level per wave, so the waves number
/// the height of the tallest table, not the tables. A page that several
/// tables share is read once per wave and checked separately for each place
/// a table found it (root or not, the level and largest key its parent
/// recorded), with the same checks and answers as [`lookup_many`] on each
/// table alone. A wave reads at most `pages_per_demand` pages per demand
/// (at least one), so a wave whose distinct pages exceed it takes more than
/// one demand.
pub fn lookup_tables(
    reader: &dyn AuthenticatedObjects,
    demands: &[(InodeTable, u64)],
    pages_per_demand: usize,
    work: &mut InodeReadWork,
) -> ContentResult<Vec<Option<InodeValue>>> {
    let mut answers = vec![None; demands.len()];
    if demands.iter().any(|(_, serial)| *serial == 0) {
        return Err(ContentError::InvalidRecord("inode serial"));
    }
    type Place = (ObjectId, bool, Option<(u8, u64)>);
    let mut level: BTreeMap<Place, Vec<usize>> = BTreeMap::new();
    for (index, (table, _)) in demands.iter().enumerate() {
        level
            .entry((table.root, true, None))
            .or_default()
            .push(index);
    }
    let mut depth = 0_u8;
    while !level.is_empty() {
        if depth > crate::filesystem::limits::MAXIMUM_TREE_LEVEL {
            return Err(ContentError::MappingDepthExceeded);
        }
        let mut ids = level.keys().map(|(id, ..)| *id).collect::<Vec<_>>();
        ids.dedup();
        let mut pages = BTreeMap::new();
        for wave in ids.chunks(pages_per_demand.max(1)) {
            let read = decoded::pages(reader, wave, Role::Inodes)?;
            work.read_waves = work.read_waves.saturating_add(1);
            pages.extend(wave.iter().copied().zip(read));
        }
        let mut next: BTreeMap<Place, Vec<usize>> = BTreeMap::new();
        for ((id, root_page, expected), indices) in level {
            let decoded = pages.get(&id).ok_or(ContentError::BatchCardinality {
                requested: ids.len(),
                returned: pages.len(),
            })?;
            work.pages_read = work.pages_read.saturating_add(1);
            match page(decoded.inode_wire()?, root_page, expected)? {
                Page::Leaf { rows, .. } => {
                    for index in indices {
                        work.demands = work.demands.saturating_add(1);
                        answers[index] = rows.find(demands[index].1);
                    }
                }
                Page::Branch { level, children } => {
                    for index in indices {
                        let position = children.partition_point(|(key, _)| *key < demands[index].1);
                        if let Some((maximum, child)) = children.get(position).copied() {
                            next.entry((child, false, Some((level - 1, maximum))))
                                .or_default()
                                .push(index);
                        }
                    }
                }
            }
        }
        level = next;
        depth = depth.saturating_add(1);
    }
    Ok(answers)
}

/// Rows of the one leaf holding the first serial after `after`, in serial
/// order; `None` starts before every serial. Empty only at the end of the table.
///
/// A sequential pass pays one root-to-leaf descent per leaf window and keeps no
/// cursor. Unlike a point demand it also refuses a page whose first key does not
/// follow its left sibling's summary, so no row can hide behind an earlier one.
pub fn rows_after(
    reader: &dyn AuthenticatedObjects,
    table: InodeTable,
    after: Option<u64>,
    work: &mut InodeReadWork,
) -> ContentResult<Vec<(u64, InodeValue)>> {
    let mut id = table.root;
    let mut expected: Option<(u8, u64)> = None;
    let mut floor: Option<u64> = None;
    loop {
        let decoded = decoded::page(reader, id, Role::Inodes)?;
        work.pages_read = work.pages_read.saturating_add(1);
        work.read_waves = work.read_waves.saturating_add(1);
        let (first, next) = match page(decoded.inode_wire()?, expected.is_none(), expected)? {
            Page::Leaf { rows, .. } => {
                let first = rows.0.first().map(|entry| entry.key);
                let rows = rows
                    .iter()
                    .filter(|(key, _)| after.is_none_or(|after| *key > after))
                    .collect();
                (first, Err(rows))
            }
            Page::Branch { level, children } => {
                let index = children.partition_point(|(key, _)| after.is_some_and(|a| *key <= a));
                (
                    children.first().map(|(key, _)| *key),
                    Ok((level, children, index)),
                )
            }
        };
        if floor
            .zip(first)
            .is_some_and(|(floor, first)| first <= floor)
        {
            return Err(ContentError::NonCanonicalOrdering);
        }
        match next {
            Err(rows) => {
                work.demands = work.demands.saturating_add(1);
                return Ok(rows);
            }
            Ok((level, children, index)) => {
                let Some((maximum, child)) = children.get(index).copied() else {
                    return Ok(Vec::new());
                };
                if index > 0 {
                    floor = Some(children[index - 1].0);
                }
                id = child;
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

/// One checked inode page, as a demand walk reads it.
enum Page<'w> {
    Leaf {
        maximum: u64,
        rows: Rows<'w>,
    },
    Branch {
        level: u8,
        children: Vec<(u64, ObjectId)>,
    },
}
/// A leaf's rows; every one carries a value.
struct Rows<'w>(&'w [crate::filesystem::sorted::format::WireEntry<u64, InodeValue>]);
impl Rows<'_> {
    fn iter(&self) -> impl Iterator<Item = (u64, InodeValue)> + '_ {
        self.0
            .iter()
            .filter_map(|entry| entry.value.map(|value| (entry.key, value)))
    }
    fn find(&self, serial: u64) -> Option<InodeValue> {
        self.0
            .iter()
            .find(|entry| entry.key == serial)
            .and_then(|entry| entry.value)
    }
}

/// The checks a read applies to one decoded page where it found it: the
/// fill rule off the root, the level and largest key its parent recorded,
/// and a value in every leaf row and a child in every branch row.
fn page(
    wire: &Wire<u64, InodeValue>,
    root: bool,
    expected: Option<(u8, u64)>,
) -> ContentResult<Page<'_>> {
    if !root && !<CompactInodes as Format>::filled(wire.size, wire.entries.len(), wire.level) {
        return Err(ContentError::NonCanonicalPagePartition);
    }
    if let Some((level, maximum)) = expected {
        if wire.level != level || wire.entries.last().map(|entry| entry.key) != Some(maximum) {
            return Err(ContentError::InvalidRecord("inode child summary"));
        }
    }
    if wire.level == 0 {
        if wire.entries.iter().any(|entry| entry.value.is_none()) {
            return Err(ContentError::InvalidRecord("inode leaf value"));
        }
        Ok(Page::Leaf {
            maximum: wire.entries.last().map_or(0, |entry| entry.key),
            rows: Rows(&wire.entries),
        })
    } else {
        Ok(Page::Branch {
            level: wire.level,
            children: wire
                .entries
                .iter()
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
