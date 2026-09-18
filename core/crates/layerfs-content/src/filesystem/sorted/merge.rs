//! Sorted final-state merge: split, sibling redistribution and untouched reuse.
//!
//! Only the last sibling of a page stays private until its neighbour establishes
//! the final partition. A child whose key range holds no change is emitted as its
//! stored identity without being materialized, so an update costs the changed
//! path plus the siblings that prove its partition. Input order is checked once:
//! a duplicate or unsorted change fails the attempt instead of being normalized.

use crate::error::{ContentError, ContentResult};
use crate::filesystem::sorted::format::Format;
use crate::filesystem::sorted::page::{Engine, Entry, Node, Page, ReadPage};
use crate::object::ObjectId;

/// One strictly ordered final-state change stream.
pub(crate) struct Changes<I, K, V> {
    source: I,
    pub(crate) next: Option<(K, Option<V>)>,
}

impl<I, K, V> Changes<I, K, V>
where
    I: Iterator<Item = ContentResult<(K, Option<V>)>>,
    K: Ord,
{
    /// Starts the stream by checking its first row.
    pub(crate) fn new(mut source: I) -> ContentResult<Self> {
        let next = source.next().transpose()?;
        Ok(Self { source, next })
    }

    /// True while the next change still belongs to the current subtree.
    pub(crate) fn in_range(&self, bound: Option<&K>) -> bool {
        self.next
            .as_ref()
            .is_some_and(|next| bound.is_none_or(|bound| &next.0 <= bound))
    }

    /// Takes the next change, rejecting a duplicate or descending key.
    pub(crate) fn take(&mut self) -> ContentResult<(K, Option<V>)> {
        let current = self.next.take().ok_or(ContentError::UnexpectedEof)?;
        self.next = self.source.next().transpose()?;
        if self.next.as_ref().is_some_and(|next| next.0 <= current.0) {
            return Err(ContentError::NonCanonicalOrdering);
        }
        Ok(current)
    }
}

#[allow(clippy::needless_lifetimes)]
impl<'o, 'e, F: Format> Engine<'o, 'e, F> {
    /// Appends one entry, splitting the page when it no longer fits.
    pub(crate) fn push(
        &mut self,
        page: &mut Page<F::Key, F::Value>,
        entry: Entry<F::Key, F::Value>,
    ) -> ContentResult<Option<Node<F::Key, F::Value>>> {
        if page.entries.len() == F::page_items(page.level) {
            return Err(ContentError::ObjectLimitExceeded {
                limit: F::page_items(page.level),
                actual: page.entries.len() + 1,
            });
        }
        // Keep the outermost child private: a neighbouring parent's underfull
        // singleton chain can still redistribute its rows. Interior children
        // cannot meet that chain after this final-state merge.
        if page.level > 0 && page.entries.len() > 1 {
            let last = page.entries.len() - 1;
            self.persist_entry(&mut page.entries[last], page.level)?;
        }
        self.append_entry(page, entry)?;
        let size = crate::filesystem::sorted::format::EMPTY_PAGE_BYTES + page.widths;
        if F::fits(size, page.entries.len(), page.level) {
            return Ok(None);
        }
        let widths = page
            .entries
            .iter()
            .map(|entry| F::width(&entry.key, page.level))
            .collect::<Vec<_>>();
        let split = crate::filesystem::sorted::format::nearest_half(&widths);
        let mut right = self.page(page.level)?;
        for entry in page.entries.drain(split..) {
            // The row leaves this page and lands in `right`, whose own running
            // total `append_entry` maintains.
            page.widths = page.widths.saturating_sub(F::width(&entry.key, page.level));
            self.append_entry(&mut right, entry)?;
        }
        page.origin = None;
        let left = std::mem::replace(page, right);
        self.node(left).map(Some)
    }

    /// Merges one underfull neighbour pair and re-partitions them together.
    #[allow(clippy::type_complexity)]
    pub(crate) fn merge(
        &mut self,
        left: Node<F::Key, F::Value>,
        right: Node<F::Key, F::Value>,
        output: &mut dyn FnMut(&mut Self, Node<F::Key, F::Value>) -> ContentResult<()>,
    ) -> ContentResult<()> {
        let mut left = self.materialize(left)?;
        let right = self.materialize(right)?;
        if left.level != right.level {
            return Err(ContentError::WrongLogicalRole);
        }
        left.origin = None;
        if left.level == 0 {
            for entry in right.entries {
                if let Some(node) = self.push(&mut left, entry)? {
                    output(self, node)?;
                }
            }
        } else {
            let mut page = self.page(left.level)?;
            let mut pending = None;
            let level = left.level - 1;
            for entry in left.entries.into_iter().chain(right.entries) {
                self.sibling(
                    &mut pending,
                    Self::child(entry, level),
                    &mut |engine, child| {
                        let entry = engine.entry(child)?;
                        if let Some(node) = engine.push(&mut page, entry)? {
                            output(engine, node)?;
                        }
                        Ok(())
                    },
                )?;
            }
            if let Some(child) = pending {
                let entry = self.entry(child)?;
                if let Some(node) = self.push(&mut page, entry)? {
                    output(self, node)?;
                }
            }
            left = page;
        }
        output(self, self.node(left)?)
    }

    /// Emits the previous node when both neighbours are filled, else redistributes.
    #[allow(clippy::type_complexity)]
    pub(crate) fn sibling(
        &mut self,
        pending: &mut Option<Node<F::Key, F::Value>>,
        next: Node<F::Key, F::Value>,
        output: &mut dyn FnMut(&mut Self, Node<F::Key, F::Value>) -> ContentResult<()>,
    ) -> ContentResult<()> {
        let Some(previous) = pending.take() else {
            *pending = Some(next);
            return Ok(());
        };
        if Self::filled(&previous) && Self::filled(&next) {
            output(self, previous)?;
            *pending = Some(next);
        } else {
            self.merge(previous, next, &mut |engine, node| {
                if let Some(previous) = pending.replace(node) {
                    output(engine, previous)?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }

    /// Merges one subtree's changes into its final pages.
    #[allow(clippy::type_complexity)]
    pub(crate) fn edit<I>(
        &mut self,
        id: Option<ObjectId>,
        read: ReadPage<F::Key, F::Value>,
        bound: Option<&F::Key>,
        changes: &mut Changes<I, F::Key, F::Value>,
        output: &mut dyn FnMut(&mut Self, Node<F::Key, F::Value>) -> ContentResult<()>,
    ) -> ContentResult<()>
    where
        I: Iterator<Item = ContentResult<(F::Key, Option<F::Value>)>>,
    {
        if !changes.in_range(bound) {
            self.work.untouched_subtrees = self.work.untouched_subtrees.saturating_add(1);
            return output(
                self,
                Node::existing(id.ok_or(ContentError::MissingObject)?, &read.wire),
            );
        }
        let mut page = self.page(read.wire.level)?;
        page.origin = id;
        if page.level == 0 {
            let mut old = read
                .wire
                .entries
                .into_iter()
                .map(|entry| (entry.key, entry.value))
                .peekable();
            while old.peek().is_some() || changes.in_range(bound) {
                let delta_first = changes.in_range(bound)
                    && old.peek().is_none_or(|old| {
                        old.0 >= changes.next.as_ref().expect("pending change").0
                    });
                let (key, value) = if delta_first {
                    let (key, value) = changes.take()?;
                    self.work.change_keys = self.work.change_keys.saturating_add(1);
                    let previous = if old.peek().is_some_and(|old| old.0 == key) {
                        old.next().map(|(_, value)| value)
                    } else {
                        None
                    };
                    (self.observe)(previous.flatten(), value.clone())?;
                    (key, value)
                } else {
                    let (key, value) = old.next().ok_or(ContentError::UnexpectedEof)?;
                    (key, value)
                };
                if let Some(value) = value {
                    let entry = Entry {
                        bytes: F::width(&key, 0) as u64,
                        key,
                        id: None,
                        value: Some(value),
                        count: 1,
                        size: 0,
                        items: 0,
                        pending: None,
                    };
                    if let Some(node) = self.push(&mut page, entry)? {
                        output(self, node)?;
                    }
                }
            }
        } else {
            let level = page.level;
            let entries = read.wire.entries;
            let count = entries.len();
            let mut old_count = 0_u64;
            let mut old_bytes = 0_u64;
            let mut pending = None;
            let mut start = 0;
            while start < count {
                // One bounded authenticated group serves this chunk: the provider
                // resolves every location in one demand wave. The chunk narrows to
                // the remaining budget instead of failing where a point read would
                // have succeeded; every page is still read, authenticated, decoded
                // and checked per child in ascending key order.
                let width = crate::filesystem::sorted::page::BATCH_CHILDREN.min(count - start);
                let (chunk, fetched, mut retained) =
                    self.batch_children(&entries[start..], width)?;
                let mut available = fetched;
                for (index, entry) in entries[start..start + chunk].iter().enumerate() {
                    let child_id = entry
                        .child
                        .ok_or(ContentError::InvalidRecord("branch row"))?;
                    let child = if let Some(retained) = &mut retained {
                        let position = available
                            .iter()
                            .position(|(id, _)| *id == child_id)
                            .ok_or(ContentError::MissingObject)?;
                        let (_, canonical) = available.remove(position);
                        let child = self.decode_page(false, &canonical)?;
                        retained.shrink(canonical.capacity());
                        drop(canonical);
                        child
                    } else {
                        self.read(child_id, false)?
                    };
                    self.check_child(level, &entry.key, &child.wire)?;
                    old_count = old_count
                        .checked_add(child.wire.count)
                        .ok_or(ContentError::LengthOverflow)?;
                    old_bytes = old_bytes
                        .checked_add(child.wire.bytes)
                        .ok_or(ContentError::LengthOverflow)?;
                    let child_bound = if start + index + 1 == count {
                        bound
                    } else {
                        Some(&entry.key)
                    };
                    self.edit(
                        Some(child_id),
                        child,
                        child_bound,
                        changes,
                        &mut |engine, node| {
                            engine.sibling(&mut pending, node, &mut |engine, node| {
                                let entry = engine.entry(node)?;
                                if let Some(node) = engine.push(&mut page, entry)? {
                                    output(engine, node)?;
                                }
                                Ok(())
                            })
                        },
                    )?;
                }
                start += chunk;
            }
            if old_count != read.wire.count || old_bytes != read.wire.bytes {
                return Err(ContentError::InvalidRecord("tree subtree summary"));
            }
            if let Some(node) = pending {
                let entry = self.entry(node)?;
                if let Some(node) = self.push(&mut page, entry)? {
                    output(self, node)?;
                }
            }
        }
        if !page.entries.is_empty() {
            let node = self.node(page)?;
            output(self, node)?;
        }
        Ok(())
    }
}
