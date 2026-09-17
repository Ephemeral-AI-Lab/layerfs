//! Streaming attribute construction with exact sizing and one final encode.
//!
//! Entries arrive in strictly ascending key order. A pending leaf keeps a tail of
//! unresolved entries; its exact canonical size is arithmetic on the row widths,
//! so a push never clones the page and never encodes a candidate just to discover
//! whether it still fits. When the exact size would exceed the page ceiling the
//! entry starts a new group; at most two groups stay pending, and the third seals
//! the oldest. `finish` rebalances the tail (moving entries forward until the last
//! group reaches the 2/5 fill rule) and then seals the same way, so the partition
//! matches the reference builder's exactly.

use crate::error::{ContentError, ContentResult};
use crate::filesystem::attributes::codec::{
    encode_attribute_page, AttributeEntry, AttributePage, BRANCH_ROW_OVERHEAD, EMPTY_PAGE_BYTES,
    LEAF_ROW_OVERHEAD,
};
use crate::filesystem::attributes::keys::AttributeKey;
use crate::filesystem::limits::{MAXIMUM_PAGE_BYTES, MAXIMUM_TREE_LEVEL};
use crate::filesystem::objects::FilesystemObjects;
use crate::object::{FinalizedObject, ObjectId, ObjectRole};

/// Work one attribute construction performed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AttributeBuildWork {
    /// Leaves emitted.
    pub leaves: u64,
    /// Branches emitted.
    pub branches: u64,
    /// Largest number of entries held before their page was sealed.
    pub peak_pending_entries: usize,
    /// Largest number of child summaries held before their page was sealed.
    pub peak_pending_children: usize,
}

/// One group whose exact size decides whether the next row still fits.
enum Group {
    /// A pending leaf's entries.
    Entries(Vec<AttributeEntry>),
    /// A pending branch's child summaries.
    Children(Vec<(AttributeKey, u64, ObjectId)>),
}

impl Group {
    fn len(&self) -> usize {
        match self {
            Self::Entries(entries) => entries.len(),
            Self::Children(children) => children.len(),
        }
    }
}

/// One level's pending groups and their exact encoded row totals.
#[derive(Default)]
struct Level {
    groups: Vec<Group>,
    rows: Vec<u64>,
}

/// Streaming attribute-tree builder over one operation's object boundary.
pub struct AttributeTreeBuilder {
    levels: Vec<Level>,
    previous: Option<AttributeKey>,
    work: AttributeBuildWork,
}

impl Default for AttributeTreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AttributeTreeBuilder {
    /// Empty builder.
    pub fn new() -> Self {
        Self {
            levels: vec![Level {
                groups: Vec::new(),
                rows: Vec::new(),
            }],
            previous: None,
            work: AttributeBuildWork::default(),
        }
    }

    /// Work performed so far.
    pub const fn work(&self) -> AttributeBuildWork {
        self.work
    }

    /// Appends one entry in ascending key order.
    pub fn push(
        &mut self,
        objects: &mut FilesystemObjects<'_>,
        entry: AttributeEntry,
    ) -> ContentResult<()> {
        if self
            .previous
            .as_ref()
            .is_some_and(|previous| previous >= &entry.key)
        {
            return Err(ContentError::NonCanonicalOrdering);
        }
        self.previous = Some(entry.key.clone());
        let rows = (LEAF_ROW_OVERHEAD + entry.key.domain().len() + entry.key.key().len()) as u64;
        let level = &mut self.levels[0];
        if level.groups.is_empty() {
            level.groups.push(Group::Entries(Vec::new()));
            level.rows.push(0);
        }
        let last = level.groups.len() - 1;
        let pending = level.rows[last]
            .checked_add(rows)
            .ok_or(ContentError::LengthOverflow)?;
        if (EMPTY_PAGE_BYTES as u64)
            .checked_add(pending)
            .ok_or(ContentError::LengthOverflow)?
            > MAXIMUM_PAGE_BYTES as u64
        {
            level.groups.push(Group::Entries(Vec::new()));
            level.rows.push(0);
            let last = level.groups.len() - 1;
            level.rows[last] = rows;
            match &mut level.groups[last] {
                Group::Entries(entries) => entries.push(entry),
                _ => return Err(ContentError::InvalidRecord("attribute group")),
            }
        } else {
            level.rows[last] = pending;
            match &mut level.groups[last] {
                Group::Entries(entries) => entries.push(entry),
                _ => return Err(ContentError::InvalidRecord("attribute group")),
            }
        }
        self.note_pending();
        if self.levels[0].groups.len() == 3 {
            let sealed = self.levels[0].groups.remove(0);
            self.levels[0].rows.remove(0);
            if !matches!(sealed, Group::Entries(_)) {
                return Err(ContentError::InvalidRecord("attribute group level"));
            }
            self.seal(objects, 0, sealed)?;
        }
        Ok(())
    }

    /// Seals every pending group and returns the tree root.
    pub fn finish(mut self, objects: &mut FilesystemObjects<'_>) -> ContentResult<ObjectId> {
        if self.levels[0].groups.is_empty() {
            return emit(
                objects,
                AttributePage::Leaf {
                    subtree_bytes: 0,
                    entries: Vec::new(),
                },
            );
        }
        rebalance_tail(&mut self.levels[0])?;
        let level = std::mem::take(&mut self.levels[0]);
        for group in level.groups {
            if !matches!(group, Group::Entries(_)) {
                return Err(ContentError::InvalidRecord("attribute group level"));
            }
            self.seal(objects, 0, group)?;
        }
        let mut index = 0_usize;
        loop {
            let pending = self
                .levels
                .get(index)
                .ok_or(ContentError::InvalidRecord("attribute level"))?;
            let children: usize = pending.groups.iter().map(Group::len).sum();
            let higher = self.levels[index + 1..]
                .iter()
                .any(|level| !level.groups.is_empty());
            if children == 1 && !higher {
                return match &pending.groups[0] {
                    Group::Children(children) => Ok(children[0].2),
                    Group::Entries(_) => Err(ContentError::InvalidRecord("attribute level")),
                };
            }
            if children == 0 {
                index = index
                    .checked_add(1)
                    .ok_or(ContentError::MappingDepthExceeded)?;
                continue;
            }
            let branch_level =
                u8::try_from(index + 1).map_err(|_| ContentError::MappingDepthExceeded)?;
            if branch_level > MAXIMUM_TREE_LEVEL {
                return Err(ContentError::MappingDepthExceeded);
            }
            let mut level = std::mem::take(&mut self.levels[index]);
            rebalance_branch_tail(&mut level)?;
            for group in level.groups {
                if !matches!(group, Group::Children(_)) {
                    return Err(ContentError::InvalidRecord("attribute group level"));
                }
                self.seal(objects, branch_level, group)?;
            }
            index = index
                .checked_add(1)
                .ok_or(ContentError::MappingDepthExceeded)?;
        }
    }

    fn note_pending(&mut self) {
        let entries: usize = self.levels[0].groups.iter().map(Group::len).sum();
        self.work.peak_pending_entries = self.work.peak_pending_entries.max(entries);
        let children: usize = self.levels[1..]
            .iter()
            .flat_map(|level| level.groups.iter())
            .map(Group::len)
            .sum();
        self.work.peak_pending_children = self.work.peak_pending_children.max(children);
    }

    /// Emits one group and pushes its summary into the next level up.
    fn seal(
        &mut self,
        objects: &mut FilesystemObjects<'_>,
        level: u8,
        group: Group,
    ) -> ContentResult<()> {
        let (page, count, _bytes, maximum) = match group {
            Group::Entries(entries) if level == 0 => {
                let bytes = entries.iter().try_fold(0_u64, |total, entry| {
                    total
                        .checked_add(
                            (LEAF_ROW_OVERHEAD + entry.key.domain().len() + entry.key.key().len())
                                as u64,
                        )
                        .ok_or(ContentError::LengthOverflow)
                })?;
                let maximum = entries
                    .last()
                    .map(|entry| entry.key.clone())
                    .ok_or(ContentError::NonCanonicalPagePartition)?;
                let count = entries.len() as u64;
                (
                    AttributePage::Leaf {
                        subtree_bytes: bytes,
                        entries,
                    },
                    count,
                    bytes,
                    maximum,
                )
            }
            Group::Children(children) if level > 0 => {
                let count = children.iter().try_fold(0_u64, |total, (_, count, _)| {
                    total
                        .checked_add(*count)
                        .ok_or(ContentError::LengthOverflow)
                })?;
                let bytes = children.iter().try_fold(0_u64, |total, (key, _, _)| {
                    total
                        .checked_add(
                            (BRANCH_ROW_OVERHEAD + key.domain().len() + key.key().len()) as u64,
                        )
                        .ok_or(ContentError::LengthOverflow)
                })?;
                let maximum = children
                    .last()
                    .map(|(key, _, _)| key.clone())
                    .ok_or(ContentError::NonCanonicalPagePartition)?;
                (
                    AttributePage::Branch {
                        level,
                        subtree_count: count,
                        subtree_bytes: bytes,
                        children: children
                            .iter()
                            .map(|(key, _, id)| (key.clone(), *id))
                            .collect(),
                    },
                    count,
                    bytes,
                    maximum,
                )
            }
            Group::Entries(_) | Group::Children(_) => {
                return Err(ContentError::InvalidRecord("attribute group level"))
            }
        };
        let id = emit(objects, page)?;
        if level == 0 {
            self.work.leaves = self.work.leaves.saturating_add(1);
        } else {
            self.work.branches = self.work.branches.saturating_add(1);
        }
        self.push_summary(objects, level, maximum, count, id)
    }

    fn push_summary(
        &mut self,
        objects: &mut FilesystemObjects<'_>,
        child_level: u8,
        key: AttributeKey,
        count: u64,
        id: ObjectId,
    ) -> ContentResult<()> {
        let index = usize::from(child_level);
        while self.levels.len() <= index + 1 {
            self.levels.push(Level {
                groups: Vec::new(),
                rows: Vec::new(),
            });
        }
        let branch_level =
            u8::try_from(index + 1).map_err(|_| ContentError::MappingDepthExceeded)?;
        if branch_level > MAXIMUM_TREE_LEVEL {
            return Err(ContentError::MappingDepthExceeded);
        }
        let rows = (BRANCH_ROW_OVERHEAD + key.domain().len() + key.key().len()) as u64;
        let level = &mut self.levels[index + 1];
        if level.groups.is_empty() {
            level.groups.push(Group::Children(Vec::new()));
            level.rows.push(0);
        }
        let last = level.groups.len() - 1;
        let pending = level.rows[last]
            .checked_add(rows)
            .ok_or(ContentError::LengthOverflow)?;
        if (EMPTY_PAGE_BYTES as u64)
            .checked_add(pending)
            .ok_or(ContentError::LengthOverflow)?
            > MAXIMUM_PAGE_BYTES as u64
        {
            level.groups.push(Group::Children(Vec::new()));
            level.rows.push(rows);
        } else {
            level.rows[last] = pending;
        }
        let last = level.groups.len() - 1;
        match &mut level.groups[last] {
            Group::Children(children) => children.push((key, count, id)),
            _ => return Err(ContentError::InvalidRecord("attribute group")),
        }
        self.note_pending();
        if self.levels[index + 1].groups.len() == 3 {
            let sealed = self.levels[index + 1].groups.remove(0);
            self.levels[index + 1].rows.remove(0);
            if !matches!(sealed, Group::Children(_)) {
                return Err(ContentError::InvalidRecord("attribute group level"));
            }
            return self.seal(objects, branch_level, sealed);
        }
        Ok(())
    }
}

/// Moves entries forward until the last leaf group reaches the 2/5 fill rule.
fn rebalance_tail(level: &mut Level) -> ContentResult<()> {
    if level.groups.len() < 2 {
        return Ok(());
    }
    let last = level.groups.len() - 1;
    while (EMPTY_PAGE_BYTES as u64 + level.rows[last]) < (2 * MAXIMUM_PAGE_BYTES / 5) as u64 {
        let moved = match &mut level.groups[last - 1] {
            Group::Entries(entries) => entries
                .pop()
                .ok_or(ContentError::NonCanonicalPagePartition)?,
            Group::Children(_) => return Err(ContentError::InvalidRecord("attribute group")),
        };
        let rows = (LEAF_ROW_OVERHEAD + moved.key.domain().len() + moved.key.key().len()) as u64;
        level.rows[last - 1] -= rows;
        level.rows[last] += rows;
        match &mut level.groups[last] {
            Group::Entries(entries) => entries.insert(0, moved),
            Group::Children(_) => return Err(ContentError::InvalidRecord("attribute group")),
        }
    }
    Ok(())
}

/// Moves children forward until the last branch group reaches the fill rule.
fn rebalance_branch_tail(pending: &mut Level) -> ContentResult<()> {
    if pending.groups.len() < 2 {
        return Ok(());
    }
    let last = pending.groups.len() - 1;
    while (EMPTY_PAGE_BYTES as u64 + pending.rows[last]) < (2 * MAXIMUM_PAGE_BYTES / 5) as u64 {
        let moved = match &mut pending.groups[last - 1] {
            Group::Children(children) => children
                .pop()
                .ok_or(ContentError::NonCanonicalPagePartition)?,
            Group::Entries(_) => return Err(ContentError::InvalidRecord("attribute group")),
        };
        let rows = (BRANCH_ROW_OVERHEAD + moved.0.domain().len() + moved.0.key().len()) as u64;
        pending.rows[last - 1] -= rows;
        pending.rows[last] += rows;
        match &mut pending.groups[last] {
            Group::Children(children) => children.insert(0, moved),
            Group::Entries(_) => return Err(ContentError::InvalidRecord("attribute group")),
        }
    }
    Ok(())
}

/// Emits one attribute page and returns its identity.
fn emit(objects: &mut FilesystemObjects<'_>, page: AttributePage) -> ContentResult<ObjectId> {
    let role = if page.level() == 0 {
        ObjectRole::AttributeLeaf
    } else {
        ObjectRole::AttributeBranch
    };
    let canonical = encode_attribute_page(&page)?;
    let references = match &page {
        AttributePage::Leaf { entries, .. } => entries
            .iter()
            .map(|entry| entry.value_root)
            .collect::<Vec<_>>(),
        AttributePage::Branch { children, .. } => {
            children.iter().map(|(_, id)| *id).collect::<Vec<_>>()
        }
    };
    let object = FinalizedObject::new(role, canonical)?.with_references(references);
    objects.emit(object)
}

/// Builds one complete attribute tree from strictly sorted unique entries.
pub fn build_attribute_tree(
    objects: &mut FilesystemObjects<'_>,
    entries: impl Iterator<Item = ContentResult<AttributeEntry>>,
) -> ContentResult<(ObjectId, AttributeBuildWork)> {
    let mut builder = AttributeTreeBuilder::new();
    for entry in entries {
        builder.push(objects, entry?)?;
    }
    let work = builder.work();
    let root = builder.finish(objects)?;
    Ok((root, work))
}
