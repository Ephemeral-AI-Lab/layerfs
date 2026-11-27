//! Ordered binding rows observed by the canonical captured directory merge.
use super::{
    super::{limits::MAXIMUM_NAME_BYTES, name::path::PathName},
    serial_state::{self, SerialState},
};
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::{
    ConstructionRecordChange, ContentError, ContentResult, IndexedConstructionBacking,
    IndexedRecordPage, ObjectId,
};

const PROVISIONAL: u32 = serial_state::BINDING_PROVISIONAL;
const CERTIFIED: u32 = serial_state::BINDING_CERTIFIED;
const BLOCK_ROWS: usize = 64;
const HINT_HEADER: usize = 17;
const MARKER_BYTES: usize = 41;
const BLOCK_BYTES: usize = BLOCK_ROWS * (HINT_HEADER + MAXIMUM_NAME_BYTES);
const APPLY_BLOCKS: usize = 3;
const PAGE_BYTES: usize = 65_536;
const _: () = assert!(
    APPLY_BLOCKS * (BLOCK_BYTES + std::mem::size_of::<ConstructionRecordChange>()) <= PAGE_BYTES
);

/// Certified binding rows of the captured namespace, held in its operation records.
#[derive(Debug, Default)]
pub struct BindingChanges {
    blocks: u64,
    rows: u64,
}

impl BindingChanges {
    /// Exact count after proving every directory which contributed rows.
    pub const fn len(&self) -> u64 {
        self.rows
    }

    /// Whether the certified change emits no binding hints.
    pub const fn is_empty(&self) -> bool {
        self.rows == 0
    }

    /// Reads rows in parent and name order from the original operation scope.
    pub fn visit(
        self,
        records: &mut dyn IndexedConstructionBacking,
        visit: &mut dyn FnMut(u64, u64, &PathName) -> ContentResult<()>,
    ) -> ContentResult<()> {
        let mut cursor = Cursor::new(self.blocks);
        let mut load = |block, after| match records.entries_after(CERTIFIED, after)? {
            Some(page) => Ok(page),
            None => point(CERTIFIED, block, self.blocks, |key| records.get(key)),
        };
        let mut previous = None;
        let mut seen = 0;
        while let Some(row) = cursor.next(&mut load)? {
            let Event::Binding(parent, child, name) = row else {
                return Err(integrity());
            };
            check_order(&mut previous, parent, &name)?;
            seen += 1;
            if seen > self.rows {
                return Err(integrity());
            }
            visit(child, parent, &name)?;
        }
        if seen != self.rows {
            return Err(integrity());
        }
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct BindingTransitions {
    provisional: Blocks,
    certified: Blocks,
    cursor: Cursor,
    first: Option<Event>,
    unproven: bool,
    rows: u64,
}

impl BindingTransitions {
    pub(super) fn note(
        &mut self,
        state: &SerialState<'_>,
        parent: u64,
        name: &PathName,
        before: Option<u64>,
        after: Option<u64>,
    ) -> ContentResult<()> {
        if let Some(child) = after.filter(|value| Some(*value) != before) {
            self.provisional
                .binding(state, PROVISIONAL, parent, child, name)?;
        }
        Ok(())
    }

    pub(super) fn parent(
        &mut self,
        state: &SerialState<'_>,
        parent: u64,
        root: ObjectId,
    ) -> ContentResult<()> {
        self.provisional.marker(state, parent, root)
    }

    pub(super) fn begin_proof(&mut self, state: &SerialState<'_>) -> ContentResult<()> {
        self.provisional.flush(state, PROVISIONAL)?;
        self.cursor = Cursor::new(self.provisional.written);
        Ok(())
    }

    pub(super) fn prove(
        &mut self,
        state: &SerialState<'_>,
        serial: u64,
        before: Option<InodeValue>,
        final_value: Option<InodeValue>,
    ) -> ContentResult<()> {
        let mut load =
            |block, after| provisional_page(state, block, after, self.provisional.written);
        if self.first.is_none() {
            self.first = self.cursor.next(&mut load)?;
        }
        let parent = self.first.as_ref().map(Event::parent);
        if parent.is_some_and(|parent| parent < serial) {
            return Err(integrity());
        }
        if parent != Some(serial) {
            let old = directory_root(before);
            let new = directory_root(final_value);
            self.unproven |= new.is_some() && new != old;
            return Ok(());
        }
        let mut previous = None;
        let mut current = self.first.take();
        let mut seen = 0u64;
        loop {
            match current.take().ok_or_else(integrity)? {
                Event::Binding(parent, child, name) if parent == serial => {
                    check_order(&mut previous, parent, &name)?;
                    if final_value.is_some() {
                        self.certified
                            .binding(state, CERTIFIED, parent, child, &name)?;
                        seen = seen.checked_add(1).ok_or(ContentError::LengthOverflow)?;
                    }
                }
                Event::Parent(parent, root) if parent == serial => {
                    if final_value.is_some_and(|value| {
                        value.kind != InodeKind::Directory || value.content_root != root
                    }) {
                        return Err(integrity());
                    }
                    self.rows = self
                        .rows
                        .checked_add(seen)
                        .ok_or(ContentError::LengthOverflow)?;
                    return Ok(());
                }
                _ => return Err(integrity()),
            }
            current = self.cursor.next(&mut load)?;
        }
    }

    pub(super) fn finish(
        &mut self,
        state: &SerialState<'_>,
    ) -> ContentResult<Option<BindingChanges>> {
        if self.first.is_some()
            || self
                .cursor
                .next(&mut |block, after| {
                    provisional_page(state, block, after, self.provisional.written)
                })?
                .is_some()
        {
            return Err(integrity());
        }
        if self.unproven {
            return Ok(None);
        }
        self.certified.flush(state, CERTIFIED)?;
        Ok(Some(BindingChanges {
            blocks: self.certified.written,
            rows: self.rows,
        }))
    }
}

#[derive(Default)]
struct Blocks {
    bytes: Vec<u8>,
    rows: usize,
    written: u64,
    pending: Vec<ConstructionRecordChange>,
}

impl Blocks {
    fn binding(
        &mut self,
        state: &SerialState<'_>,
        kind: u32,
        parent: u64,
        child: u64,
        name: &PathName,
    ) -> ContentResult<()> {
        if self.rows == BLOCK_ROWS {
            self.stage(state, kind)?;
        }
        self.bytes.extend_from_slice(&[name.as_bytes().len() as u8]);
        self.bytes.extend_from_slice(&parent.to_be_bytes());
        self.bytes.extend_from_slice(&child.to_be_bytes());
        self.bytes.extend_from_slice(name.as_bytes());
        self.rows += 1;
        Ok(())
    }

    fn marker(
        &mut self,
        state: &SerialState<'_>,
        parent: u64,
        root: ObjectId,
    ) -> ContentResult<()> {
        if self.rows == BLOCK_ROWS {
            self.stage(state, PROVISIONAL)?;
        }
        self.bytes.push(0);
        self.bytes.extend_from_slice(&parent.to_be_bytes());
        self.bytes.extend_from_slice(root.as_bytes());
        self.rows += 1;
        Ok(())
    }

    fn flush(&mut self, state: &SerialState<'_>, kind: u32) -> ContentResult<()> {
        self.stage(state, kind)?;
        if !self.pending.is_empty() {
            state.apply(std::mem::take(&mut self.pending))?;
        }
        Ok(())
    }

    fn stage(&mut self, state: &SerialState<'_>, kind: u32) -> ContentResult<()> {
        if self.rows != 0 {
            if self.pending.capacity() == 0 {
                self.pending = Vec::with_capacity(APPLY_BLOCKS);
            }
            let index = self
                .written
                .checked_add(1)
                .ok_or(ContentError::LengthOverflow)?;
            let value = std::mem::take(&mut self.bytes)
                .into_boxed_slice()
                .into_vec();
            self.pending
                .push(serial_state::change(kind, index, None, value));
            if self.pending.len() == APPLY_BLOCKS {
                state.apply(std::mem::take(&mut self.pending))?;
            }
            self.written = index;
            self.rows = 0;
        }
        Ok(())
    }
}

#[derive(Default)]
struct Cursor {
    blocks: u64,
    taken: u64,
    page: std::vec::IntoIter<([u8; 32], Vec<u8>)>,
    after: Option<[u8; 32]>,
    rows: usize,
    bytes: Vec<u8>,
    at: usize,
}

impl Cursor {
    fn new(blocks: u64) -> Self {
        Self {
            blocks,
            ..Self::default()
        }
    }

    fn next(
        &mut self,
        load: &mut impl FnMut(u64, Option<[u8; 32]>) -> ContentResult<IndexedRecordPage>,
    ) -> ContentResult<Option<Event>> {
        if self.at == self.bytes.len() {
            if self.taken == self.blocks {
                return Ok(None);
            }
            if self.page.len() == 0 {
                self.bytes = Vec::new();
                let (mut page, ended) = load(self.taken + 1, self.after)?;
                self.validate(&page, ended)?;
                for (_, bytes) in &mut page {
                    if bytes.capacity() > bytes.len() {
                        *bytes = std::mem::take(bytes).into_boxed_slice().into_vec();
                    }
                }
                self.page = page.into_boxed_slice().into_vec().into_iter();
            }
            let (key, bytes) = self.page.next().ok_or_else(integrity)?;
            self.taken += 1;
            self.after = Some(key);
            self.bytes = bytes;
            self.rows = 0;
            self.at = 0;
        }
        if self.rows == BLOCK_ROWS {
            return Err(integrity());
        }
        let row = &self.bytes[self.at..];
        let name_len = usize::from(row[0]);
        let width = if name_len == 0 {
            MARKER_BYTES
        } else {
            HINT_HEADER + name_len
        };
        let row = row.get(..width).ok_or_else(integrity)?;
        let parent = number(&row[1..9])?;
        let event = if name_len == 0 {
            Event::Parent(parent, ObjectId::from_bytes(&row[9..])?)
        } else {
            Event::Binding(
                parent,
                number(&row[9..17])?,
                PathName::from_bytes(&row[17..])?,
            )
        };
        self.at += width;
        self.rows += 1;
        Ok(Some(event))
    }

    fn validate(&self, page: &[([u8; 32], Vec<u8>)], ended: bool) -> ContentResult<()> {
        if page.is_empty()
            || page.len() > BLOCK_ROWS
            || page.len() as u64 > self.blocks - self.taken
        {
            return Err(integrity());
        }
        let last = page.len() as u64 == self.blocks - self.taken;
        // The Overlay keyset uses LIMIT 64. A full final page therefore
        // does not affirm EOF, even when the sealed count covers it exactly.
        if ended && !last || !ended && last && page.len() != BLOCK_ROWS {
            return Err(integrity());
        }
        let mut bytes = 0;
        for (offset, (key, value)) in page.iter().enumerate() {
            if serial_state::serial(*key)? != self.taken + 1 + offset as u64
                || value.is_empty()
                || value.len() > BLOCK_BYTES
            {
                return Err(integrity());
            }
            bytes += value.len();
        }
        if bytes > PAGE_BYTES {
            return Err(integrity());
        }
        Ok(())
    }
}

fn provisional_page(
    state: &SerialState<'_>,
    block: u64,
    after: Option<[u8; 32]>,
    total: u64,
) -> ContentResult<IndexedRecordPage> {
    match state.entries_after(PROVISIONAL, after)? {
        Some(page) => Ok(page),
        None => point(PROVISIONAL, block, total, |_| {
            state.raw_record(PROVISIONAL, block)
        }),
    }
}

fn point(
    kind: u32,
    block: u64,
    total: u64,
    read: impl FnOnce(crate::ConstructionRecordKey) -> ContentResult<Option<Vec<u8>>>,
) -> ContentResult<IndexedRecordPage> {
    let key = serial_state::key(kind, block);
    let value = read(key)?.ok_or_else(integrity)?;
    Ok((vec![(key.key, value)], block == total))
}

enum Event {
    Binding(u64, u64, PathName),
    Parent(u64, ObjectId),
}
impl Event {
    fn parent(&self) -> u64 {
        match self {
            Self::Binding(parent, ..) | Self::Parent(parent, _) => *parent,
        }
    }
}

fn directory_root(value: Option<InodeValue>) -> Option<ObjectId> {
    value
        .filter(|value| value.kind == InodeKind::Directory)
        .map(|value| value.content_root)
}

fn number(bytes: &[u8]) -> ContentResult<u64> {
    let value = u64::from_be_bytes(bytes.try_into().map_err(|_| integrity())?);
    if value == 0 {
        Err(integrity())
    } else {
        Ok(value)
    }
}

fn integrity() -> ContentError {
    ContentError::InvalidRecord("captured binding transitions")
}

fn check_order(
    previous: &mut Option<(u64, PathName)>,
    parent: u64,
    name: &PathName,
) -> ContentResult<()> {
    if previous
        .as_ref()
        .is_some_and(|(prior_parent, prior_name)| (*prior_parent, prior_name) >= (parent, name))
    {
        return Err(ContentError::NonCanonicalOrdering);
    }
    *previous = Some((parent, name.clone()));
    Ok(())
}
