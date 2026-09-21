//! Generic attribute patches that preserve untouched keys and opaque values.
//!
//! A patch is a strictly sorted list of `set(key, bytes)` and `remove(key)`
//! requests. The base tree is streamed in key order with the patch list, so every
//! key the patch does not mention keeps its exact stored value root without being
//! decoded, and the new tree is rebuilt once. Both portable typed keys and opaque
//! generic domains go through this one route: there is no per-domain branch and no
//! platform dispatch.

use crate::error::{ContentError, ContentResult};
use crate::filesystem::attributes::build::AttributeTreeBuilder;
use crate::filesystem::attributes::codec::{AttributeEntry, AttributePage};
use crate::filesystem::attributes::keys::AttributeKey;
use crate::filesystem::attributes::read::decode_checked;
use crate::filesystem::attributes::value::emit_value;
use crate::filesystem::objects::FilesystemObjects;
use crate::object::{AuthenticatedObjects, ObjectId};

/// Requested patch operation for one key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AttributePatch {
    /// Store these bytes under this key.
    Set {
        /// Key to set.
        key: AttributeKey,
        /// New opaque value bytes.
        value: Vec<u8>,
    },
    /// Remove this key.
    Remove {
        /// Key to remove.
        key: AttributeKey,
    },
}

impl AttributePatch {
    /// The key this patch addresses.
    pub fn key(&self) -> &AttributeKey {
        match self {
            Self::Set { key, .. } | Self::Remove { key } => key,
        }
    }
}

/// Work one attribute patch performed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AttributePatchWork {
    /// Base entries streamed.
    pub base_entries: u64,
    /// Base pages read.
    pub base_pages: u64,
    /// Keys set.
    pub set: u64,
    /// Keys removed.
    pub removed: u64,
    /// Keys preserved without decoding their values.
    pub preserved: u64,
    /// Value objects emitted.
    pub values_emitted: u64,
}

/// Applies strictly sorted unique patches to one attribute tree.
///
/// Returns the new root. An empty patch list returns the base root unchanged,
/// because no page is rebuilt.
pub fn apply_patches(
    reader: &dyn AuthenticatedObjects,
    objects: &mut FilesystemObjects<'_>,
    base: ObjectId,
    patches: &[AttributePatch],
) -> ContentResult<(ObjectId, AttributePatchWork)> {
    let mut work = AttributePatchWork::default();
    if patches.is_empty() {
        return Ok((base, work));
    }
    if patches
        .windows(2)
        .any(|pair| pair[0].key() >= pair[1].key())
    {
        return Err(ContentError::NonCanonicalOrdering);
    }
    let mut builder = AttributeTreeBuilder::new();
    let mut cursor = PageCursor::new(reader, base, &mut work)?;
    let mut next_base = cursor.next(reader, &mut work)?;
    let mut index = 0_usize;
    while next_base.is_some() || index < patches.len() {
        let take_patch = match (&next_base, patches.get(index)) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(entry), Some(patch)) => patch.key() <= &entry.key,
        };
        if take_patch {
            let patch = &patches[index];
            index += 1;
            let (key, value) = match patch {
                AttributePatch::Set { key, value } => (key, Some(value)),
                AttributePatch::Remove { key } => (key, None),
            };
            let is_base_key = next_base.as_ref().is_some_and(|entry| &entry.key == key);
            if let Some(value) = value {
                let value_root = emit_value(objects, value)?;
                work.values_emitted = work.values_emitted.saturating_add(1);
                builder.push(
                    objects,
                    AttributeEntry {
                        key: key.clone(),
                        value_root,
                    },
                )?;
                work.set = work.set.saturating_add(1);
            } else {
                work.removed = work.removed.saturating_add(1);
            }
            if is_base_key {
                next_base = cursor.next(reader, &mut work)?;
            }
            continue;
        }
        let entry = next_base
            .take()
            .ok_or(ContentError::InvalidRecord("attribute cursor"))?;
        builder.push(objects, entry)?;
        work.preserved = work.preserved.saturating_add(1);
        next_base = cursor.next(reader, &mut work)?;
    }
    let root = builder.finish(objects)?;
    Ok((root, work))
}

/// Visits every stored key and value root in order, without decoding values.
pub fn visit_keys(
    reader: &dyn AuthenticatedObjects,
    root: ObjectId,
    visitor: impl FnMut(&AttributeKey, &ObjectId) -> ContentResult<()>,
) -> ContentResult<()> {
    let mut work = AttributePatchWork::default();
    visit_keys_counted(reader, root, &mut work, visitor)
}

/// Visits every key of one attribute tree, reporting the work it cost.
///
/// The visitor is called in key order and may stop the walk by returning an
/// error, which is how a caller enforces its own bound on the set it collects.
pub fn visit_keys_counted(
    reader: &dyn AuthenticatedObjects,
    root: ObjectId,
    work: &mut AttributePatchWork,
    mut visitor: impl FnMut(&AttributeKey, &ObjectId) -> ContentResult<()>,
) -> ContentResult<()> {
    let mut cursor = PageCursor::new(reader, root, work)?;
    while let Some(entry) = cursor.next(reader, work)? {
        visitor(&entry.key, &entry.value_root)?;
    }
    Ok(())
}

/// In-order leaf cursor over one attribute tree, bounded by one leaf at a time.
struct PageCursor {
    pending: Vec<(ObjectId, Option<(u8, AttributeKey)>)>,
    leaf: std::vec::IntoIter<AttributeEntry>,
}

impl PageCursor {
    fn new(
        reader: &dyn AuthenticatedObjects,
        root: ObjectId,
        work: &mut AttributePatchWork,
    ) -> ContentResult<Self> {
        let mut cursor = Self {
            pending: vec![(root, None)],
            leaf: Vec::new().into_iter(),
        };
        cursor.advance(reader, work)?;
        Ok(cursor)
    }

    /// Next entry in key order, or `None` at the end of the tree.
    fn next(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        work: &mut AttributePatchWork,
    ) -> ContentResult<Option<AttributeEntry>> {
        if self.leaf.len() == 0 {
            self.advance(reader, work)?;
        }
        Ok(self.leaf.next())
    }

    /// Loads leaves until one has a row, then keeps the rest for the next call.
    fn advance(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        work: &mut AttributePatchWork,
    ) -> ContentResult<()> {
        while self.leaf.len() == 0 {
            let Some((id, expected)) = self.pending.pop() else {
                return Ok(());
            };
            let canonical = reader.read_canonical(id)?;
            work.base_pages = work.base_pages.saturating_add(1);
            let page = decode_checked(&canonical, expected.as_ref())?;
            match page {
                AttributePage::Leaf { entries, .. } => {
                    work.base_entries = work.base_entries.saturating_add(entries.len() as u64);
                    self.leaf = entries.into_iter();
                }
                AttributePage::Branch {
                    level, children, ..
                } => {
                    for (key, child) in children.into_iter().rev() {
                        self.pending.push((child, Some((level - 1, key))));
                    }
                }
            }
        }
        Ok(())
    }
}
