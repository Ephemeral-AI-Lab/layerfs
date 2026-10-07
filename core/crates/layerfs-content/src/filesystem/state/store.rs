//! Short guarded point transitions; no scope acquisition, release or fallback.
use super::{codec, DroppedParents};
use crate::filesystem::rows::view::OperationInput;
use crate::{ConstructionRecordApply, ConstructionRecordChange, IndexedConstructionBacking};
use crate::{ContentError, ContentResult, ObjectId};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};

type Backing<'a> = RefCell<&'a mut dyn IndexedConstructionBacking>;
pub(crate) struct SerialState<'a> {
    backing: Option<Backing<'a>>,
    parents: RefCell<BTreeMap<u64, bool>>,
    counts: RefCell<Option<Vec<u64>>>,
    dropped: Cell<usize>,
}
impl<'a> SerialState<'a> {
    pub fn new(backing: Option<&'a mut dyn IndexedConstructionBacking>) -> Self {
        Self {
            backing: backing.map(RefCell::new),
            parents: RefCell::new(BTreeMap::new()),
            counts: RefCell::new(None),
            dropped: Cell::new(0),
        }
    }
    pub fn backed(&self) -> bool {
        self.backing.is_some()
    }
    pub fn begin(&self, input: &dyn OperationInput) -> ContentResult<()> {
        if self.backed() {
            self.apply(vec![codec::change(
                codec::CONTEXT,
                0,
                None,
                codec::context(input),
            )])?;
        }
        Ok(())
    }
    pub(crate) fn read(&self, kind: u32, serial: u64) -> ContentResult<Option<Vec<u8>>> {
        let backing = self
            .backing
            .as_ref()
            .ok_or(ContentError::InvalidRecord("filesystem backing mode"))?;
        let value = backing.borrow_mut().get(codec::key(kind, serial))?;
        if value
            .as_ref()
            .is_some_and(|value| value.capacity() > codec::WINDOW)
        {
            return Err(ContentError::InvalidRecord(
                "filesystem backing value window",
            ));
        }
        Ok(value)
    }
    pub(crate) fn apply(&self, changes: Vec<ConstructionRecordChange>) -> ContentResult<()> {
        let mut bytes = changes
            .capacity()
            .checked_mul(std::mem::size_of::<ConstructionRecordChange>())
            .ok_or(ContentError::LengthOverflow)?;
        let mut previous = None;
        for change in &changes {
            if previous.is_some_and(|key| key >= change.key) {
                return Err(ContentError::InvalidRecord(
                    "filesystem backing target order",
                ));
            }
            previous = Some(change.key);
            let expected = match &change.expected {
                crate::ConstructionRecordExpected::Missing => 0,
                crate::ConstructionRecordExpected::ExactBytes(value) => value.capacity(),
            };
            bytes = bytes
                .checked_add(expected)
                .and_then(|bytes| bytes.checked_add(change.value.as_ref().map_or(0, Vec::capacity)))
                .ok_or(ContentError::LengthOverflow)?;
        }
        if bytes > codec::WINDOW {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "filesystem.backing_window",
                limit: codec::WINDOW as u64,
                actual: bytes as u64,
            });
        }
        let backing = self
            .backing
            .as_ref()
            .ok_or(ContentError::InvalidRecord("filesystem backing mode"))?;
        match backing.borrow_mut().apply(changes)? {
            ConstructionRecordApply::Applied => Ok(()),
            ConstructionRecordApply::NotApplied { .. } => Err(ContentError::ProviderFailure {
                what: "filesystem backing precondition",
            }),
        }
    }
    pub(crate) fn keys_after(
        &self,
        kind: u32,
        after: Option<[u8; 32]>,
    ) -> ContentResult<Vec<[u8; 32]>> {
        let backing = self
            .backing
            .as_ref()
            .ok_or(ContentError::InvalidRecord("filesystem backing mode"))?;
        let keys = backing.borrow_mut().keys_after(kind, after)?;
        Self::check_keys(&keys, keys.capacity(), after)?;
        Ok(keys)
    }
    pub(crate) fn first_keys(&self, kind: u32) -> ContentResult<Vec<[u8; 32]>> {
        let backing = self
            .backing
            .as_ref()
            .ok_or(ContentError::InvalidRecord("filesystem backing mode"))?;
        let keys = backing.borrow_mut().first_keys(kind, None)?;
        Self::check_keys(&keys, keys.capacity(), None)?;
        Ok(keys)
    }
    fn check_keys(
        keys: &[[u8; 32]],
        capacity: usize,
        after: Option<[u8; 32]>,
    ) -> ContentResult<()> {
        if capacity > 64 || keys.len() > 64 {
            return Err(ContentError::InvalidRecord("filesystem backing key window"));
        }
        let mut previous = after;
        for key in keys {
            if previous.is_some_and(|previous| previous >= *key) {
                return Err(ContentError::NonCanonicalOrdering);
            }
            previous = Some(*key);
        }
        Ok(())
    }
    pub fn declare_parent(&self, serial: u64, limit: usize) -> ContentResult<()> {
        let next = self
            .dropped
            .get()
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
        if self.backed() {
            self.apply(vec![codec::change(codec::PARENT, serial, None, vec![1, 0])])?;
        } else {
            let mut parents = self.parents.borrow_mut();
            parents.insert(serial, false);
            if parents.len() > limit {
                return Err(ContentError::ObjectLimitExceeded {
                    limit,
                    actual: parents.len(),
                });
            }
        }
        self.dropped.set(next);
        Ok(())
    }
    pub fn parent(&self, serial: u64) -> ContentResult<Option<bool>> {
        if self.backed() {
            self.read(codec::PARENT, serial)?
                .as_deref()
                .map(codec::parent)
                .transpose()
        } else {
            Ok(self.parents.borrow().get(&serial).copied())
        }
    }
    fn tracked(&self, input: &dyn OperationInput, serial: u64) -> ContentResult<bool> {
        Ok(serial != input.root_serial()
            && input.is_new(serial)?
            && input.directory_for(serial)?.is_some())
    }
    pub fn dropped_view<'s>(&'s self, input: &'s dyn OperationInput) -> DroppedView<'s, 'a> {
        DroppedView { state: self, input }
    }
    pub fn dropped(&self, input: &dyn OperationInput, serial: u64) -> ContentResult<bool> {
        if self.backed() {
            if !self.tracked(input, serial)? {
                return Ok(false);
            }
            Ok(!self
                .parent(serial)?
                .ok_or(ContentError::InvalidRecord("filesystem parent missing"))?)
        } else {
            Ok(self.parent(serial)? == Some(false))
        }
    }
    pub fn hold_parent(&self, input: &dyn OperationInput, serial: u64) -> ContentResult<()> {
        let next;
        if self.backed() {
            if !self.tracked(input, serial)? {
                return Ok(());
            }
            let old = self
                .read(codec::PARENT, serial)?
                .ok_or(ContentError::InvalidRecord("filesystem parent missing"))?;
            if codec::parent(&old)? {
                return Ok(());
            }
            next = self
                .dropped
                .get()
                .checked_sub(1)
                .ok_or(ContentError::InvalidRecord("filesystem dropped count"))?;
            self.apply(vec![codec::change(
                codec::PARENT,
                serial,
                Some(old),
                vec![1, 1],
            )])?;
        } else {
            let mut parents = self.parents.borrow_mut();
            let Some(held) = parents.get_mut(&serial) else {
                return Ok(());
            };
            if *held {
                return Ok(());
            }
            next = self
                .dropped
                .get()
                .checked_sub(1)
                .ok_or(ContentError::InvalidRecord("filesystem dropped count"))?;
            *held = true;
        }
        self.dropped.set(next);
        Ok(())
    }
    pub fn dropped_count(&self) -> usize {
        self.dropped.get()
    }
    pub fn finish_parents(&self) {
        if !self.backed() {
            self.parents.borrow_mut().retain(|_, held| !*held);
        }
    }
    pub fn initial_counts(&self, input: &dyn OperationInput) -> ContentResult<()> {
        if self.backed() {
            let mut serials = input.new_inodes()?;
            let mut seen = 0usize;
            let mut previous = 0u64;
            while let Some(serial) = serials.next_row()? {
                if seen >= input.new_rows() {
                    return Err(ContentError::InvalidRecord("new inode row count"));
                }
                if serial <= previous {
                    return Err(ContentError::NonCanonicalOrdering);
                }
                previous = serial;
                seen += 1;
                self.apply(vec![codec::change(
                    codec::COUNT,
                    serial,
                    None,
                    codec::encode_count(0),
                )])?;
            }
            if seen != input.new_rows() {
                return Err(ContentError::InvalidRecord("new inode row count"));
            }
        } else {
            *self.counts.borrow_mut() = Some(vec![0; input.new_rows()]);
        }
        Ok(())
    }
    pub fn note_binding(&self, serial: u64, position: usize) -> ContentResult<()> {
        if self.backed() {
            let old = self.read(codec::COUNT, serial)?;
            let before = old
                .as_deref()
                .map(codec::count)
                .transpose()?
                .ok_or(ContentError::InvalidRecord("filesystem count missing"))?;
            let after = before.checked_add(1).ok_or(ContentError::LengthOverflow)?;
            self.apply(vec![codec::change(
                codec::COUNT,
                serial,
                old,
                codec::encode_count(after),
            )])
        } else {
            let mut counts = self.counts.borrow_mut();
            let count = counts
                .as_mut()
                .and_then(|counts| counts.get_mut(position))
                .ok_or(ContentError::InvalidRecord("new inode position"))?;
            *count = count.checked_add(1).ok_or(ContentError::LengthOverflow)?;
            Ok(())
        }
    }
    pub fn count(&self, serial: u64, position: usize) -> ContentResult<u64> {
        if self.backed() {
            Ok(self
                .read(codec::COUNT, serial)?
                .as_deref()
                .map(codec::count)
                .transpose()?
                .ok_or(ContentError::InvalidRecord("filesystem count missing"))?)
        } else {
            self.counts
                .borrow()
                .as_ref()
                .and_then(|counts| counts.get(position))
                .copied()
                .ok_or(ContentError::InvalidRecord("new inode position"))
        }
    }
    pub fn put_root(&self, serial: u64, root: ObjectId) -> ContentResult<()> {
        self.apply(vec![codec::change(
            codec::ROOT,
            serial,
            None,
            codec::encode_root(root),
        )])
    }
    pub fn root(&self, serial: u64) -> ContentResult<Option<ObjectId>> {
        self.read(codec::ROOT, serial)?
            .as_deref()
            .map(codec::root)
            .transpose()
    }
}
pub(crate) struct DroppedView<'s, 'b> {
    state: &'s SerialState<'b>,
    input: &'s dyn OperationInput,
}
impl DroppedParents for DroppedView<'_, '_> {
    fn contains(&self, serial: u64) -> ContentResult<bool> {
        self.state.dropped(self.input, serial)
    }
}
