//! Direct fixed reference rows, fresh membership and touched witnesses.
use super::{indexed_wire as wire, meaning, record::Row, reduce::ReferenceWork};
use crate::filesystem::{
    rows::view::OperationInput,
    state::{DroppedParents, SerialState},
};
use crate::object::inode_leaf::InodeValue;
use crate::{ConstructionRecordChange, ContentError, ContentResult};

pub(super) struct IndexedReducer<'s, 'b> {
    pub state: &'s SerialState<'b>,
    input: &'s dyn OperationInput,
    dropped: &'s dyn DroppedParents,
    pub work: ReferenceWork,
    pub touched: u64,
}
pub(super) struct Transition {
    pub before: Row,
    pub row: Row,
    pub changes: Vec<ConstructionRecordChange>,
    created: bool,
}
impl<'s, 'b> IndexedReducer<'s, 'b> {
    pub fn new(
        state: &'s SerialState<'b>,
        input: &'s dyn OperationInput,
        dropped: &'s dyn DroppedParents,
    ) -> Self {
        Self {
            state,
            input,
            dropped,
            work: ReferenceWork::default(),
            touched: 0,
        }
    }
    pub fn declare_new(&mut self, serial: u64) -> ContentResult<()> {
        if serial == 0 || !self.input.is_new(serial)? || self.dropped.contains(serial)? {
            return Err(ContentError::InvalidRecord("filesystem fresh declaration"));
        }
        self.state.apply(vec![
            wire::change(wire::FRESH, serial, None, Some(vec![1])),
            wire::change(wire::ROW, serial, None, None),
            wire::change(wire::TOUCH, serial, None, None),
        ])
    }
    fn is_new(&self, serial: u64) -> ContentResult<bool> {
        let expected = self.input.is_new(serial)? && !self.dropped.contains(serial)?;
        let member = self.state.read(wire::FRESH, serial)?;
        if let Some(bytes) = member.as_deref() {
            wire::membership(bytes)?;
        }
        if expected != member.is_some() {
            return Err(ContentError::InvalidRecord("filesystem fresh membership"));
        }
        Ok(expected)
    }
    pub fn row(&self, serial: u64) -> ContentResult<Option<Row>> {
        let fresh = self.is_new(serial)?;
        let raw = self.state.read(wire::ROW, serial)?;
        let row = raw
            .as_deref()
            .map(|bytes| wire::row(bytes, serial))
            .transpose()?;
        if row.is_some_and(|row| matches!(row, Row::Count { .. }) != fresh) {
            return Err(ContentError::InvalidRecord(
                "filesystem reference fresh tag",
            ));
        }
        let touch = self.state.read(wire::TOUCH, serial)?;
        if let Some(bytes) = touch.as_deref() {
            wire::membership(bytes)?;
        }
        if row.is_some() != touch.is_some() {
            return Err(ContentError::InvalidRecord(
                "filesystem reference row missing",
            ));
        }
        Ok(row)
    }
    pub fn required(&self, serial: u64) -> ContentResult<Row> {
        self.row(serial)?.ok_or(ContentError::InvalidRecord(
            "filesystem reference row missing",
        ))
    }
    fn prepare(
        &self,
        serial: u64,
        mutation: impl FnOnce(&mut Row) -> ContentResult<()>,
    ) -> ContentResult<Transition> {
        if serial == 0 {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
        let fresh = self.is_new(serial)?;
        let old = self.state.read(wire::ROW, serial)?;
        let created = old.is_none();
        let mut row = match old.as_deref() {
            Some(bytes) => wire::row(bytes, serial)?,
            None if fresh => Row::Count {
                serial,
                value: None,
                count: 0,
            },
            None => Row::Effect {
                serial,
                value: None,
                delta: 0,
            },
        };
        if matches!(row, Row::Count { .. }) != fresh {
            return Err(ContentError::InvalidRecord(
                "filesystem reference fresh tag",
            ));
        }
        let touch = self.state.read(wire::TOUCH, serial)?;
        if let Some(bytes) = touch.as_deref() {
            wire::membership(bytes)?;
        }
        if old.is_some() != touch.is_some() {
            return Err(ContentError::InvalidRecord(
                "filesystem reference row missing",
            ));
        }
        let before = row;
        mutation(&mut row)?;
        // Even an unchanged touched witness is an exact coupled guard. It is not
        // a second resident set and a lost witness cannot reset an existing row.
        let changes = vec![
            wire::change(wire::ROW, serial, old, Some(row.encode()?.to_vec())),
            wire::change(wire::TOUCH, serial, touch, Some(vec![1])),
        ];
        Ok(Transition {
            before,
            row,
            changes,
            created,
        })
    }
    pub fn prepare_removed(&self, serial: u64) -> ContentResult<Transition> {
        self.prepare(serial, meaning::removed)
    }
    pub fn apply_transition(
        &mut self,
        mut transition: Transition,
        additional: Vec<ConstructionRecordChange>,
    ) -> ContentResult<()> {
        let touched = if transition.created {
            self.touched
                .checked_add(1)
                .ok_or(ContentError::LengthOverflow)?
        } else {
            self.touched
        };
        transition.changes.extend(additional);
        transition.changes.sort_by_key(|change| change.key);
        self.state.apply(transition.changes)?;
        self.touched = touched;
        self.work.rows_touched = self
            .work
            .rows_touched
            .saturating_add(u64::from(transition.created));
        Ok(())
    }
    pub fn note_retained_binding(&mut self, serial: u64) -> ContentResult<()> {
        let transition = self.prepare(serial, meaning::retained)?;
        self.apply_transition(transition, Vec::new())
    }
    pub fn note_removed_binding(&mut self, serial: u64) -> ContentResult<()> {
        let transition = self.prepare_removed(serial)?;
        self.apply_transition(transition, Vec::new())
    }
    pub fn note_value(&mut self, serial: u64, value: InodeValue) -> ContentResult<()> {
        let transition = self.prepare(serial, |row| {
            meaning::value(row, value);
            Ok(())
        })?;
        self.apply_transition(transition, Vec::new())
    }
}
