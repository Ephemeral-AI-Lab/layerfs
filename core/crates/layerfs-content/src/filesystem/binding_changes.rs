//! Ordered binding rows observed by the canonical captured directory merge.
use super::{
    limits::MAXIMUM_NAME_BYTES,
    path::PathName,
    state::{self, SerialState},
};
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::{ContentError, ContentResult, IndexedConstructionBacking, ObjectId};

const PROVISIONAL: u32 = 0x4653_0031;
const CERTIFIED: u32 = 0x4653_0032;
const BLOCK_ROWS: usize = 64;
const HINT_HEADER: usize = 17;
const MARKER_BYTES: usize = 41;
const BLOCK_BYTES: usize = BLOCK_ROWS * (HINT_HEADER + MAXIMUM_NAME_BYTES);
const _: () =
    assert!(BLOCK_BYTES + std::mem::size_of::<crate::ConstructionRecordChange>() <= 65_536);

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
        let mut load = |block| records.get(state::key(CERTIFIED, block));
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
    group_has_rows: bool,
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
            self.group_has_rows = true;
        }
        Ok(())
    }

    pub(super) fn parent(
        &mut self,
        state: &SerialState<'_>,
        parent: u64,
        root: ObjectId,
    ) -> ContentResult<()> {
        if self.group_has_rows {
            self.provisional.marker(state, parent, root)?;
            self.group_has_rows = false;
        }
        Ok(())
    }

    pub(super) fn begin_proof(&mut self, state: &SerialState<'_>) -> ContentResult<()> {
        if self.group_has_rows {
            return Err(integrity());
        }
        self.provisional.flush(state, PROVISIONAL)?;
        self.cursor = Cursor::new(self.provisional.written);
        Ok(())
    }

    pub(super) fn prove(
        &mut self,
        state: &SerialState<'_>,
        serial: u64,
        final_value: Option<InodeValue>,
    ) -> ContentResult<()> {
        let mut load = |block| state.read(PROVISIONAL, block);
        if self.first.is_none() {
            self.first = self.cursor.next(&mut load)?;
        }
        let Some(first) = &self.first else {
            return Ok(());
        };
        match first.parent().cmp(&serial) {
            std::cmp::Ordering::Greater => return Ok(()),
            std::cmp::Ordering::Less => return Err(integrity()),
            std::cmp::Ordering::Equal => (),
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
                Event::Parent(parent, root) if parent == serial && previous.is_some() => {
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

    pub(super) fn finish(&mut self, state: &SerialState<'_>) -> ContentResult<BindingChanges> {
        if self.first.is_some()
            || self
                .cursor
                .next(&mut |block| state.read(PROVISIONAL, block))?
                .is_some()
        {
            return Err(integrity());
        }
        self.certified.flush(state, CERTIFIED)?;
        Ok(BindingChanges {
            blocks: self.certified.written,
            rows: self.rows,
        })
    }
}

#[derive(Default)]
struct Blocks {
    bytes: Vec<u8>,
    rows: usize,
    written: u64,
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
            self.flush(state, kind)?;
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
            self.flush(state, PROVISIONAL)?;
        }
        self.bytes.push(0);
        self.bytes.extend_from_slice(&parent.to_be_bytes());
        self.bytes.extend_from_slice(root.as_bytes());
        self.rows += 1;
        Ok(())
    }

    fn flush(&mut self, state: &SerialState<'_>, kind: u32) -> ContentResult<()> {
        if self.rows != 0 {
            let index = self
                .written
                .checked_add(1)
                .ok_or(ContentError::LengthOverflow)?;
            let value = std::mem::take(&mut self.bytes)
                .into_boxed_slice()
                .into_vec();
            state.apply(vec![state::change(kind, index, None, value)])?;
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
        load: &mut impl FnMut(u64) -> ContentResult<Option<Vec<u8>>>,
    ) -> ContentResult<Option<Event>> {
        if self.at == self.bytes.len() {
            if self.taken == self.blocks {
                return Ok(None);
            }
            self.taken += 1;
            self.bytes = load(self.taken)?.ok_or_else(integrity)?;
            if self.bytes.is_empty() || self.bytes.capacity() > BLOCK_BYTES {
                return Err(integrity());
            }
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
