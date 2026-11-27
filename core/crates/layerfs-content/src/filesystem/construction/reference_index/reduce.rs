//! Direct fixed reference rows, fresh membership and touched witnesses.
use super::window::{Step, Window};
use super::{
    super::reference::{meaning, record::Row, reduce::ReferenceWork},
    wire,
};
use crate::filesystem::construction::{
    input::view::OperationInput,
    serial_state::{DroppedParents, SerialState},
};
use crate::object::inode_leaf::InodeValue;
use crate::{ConstructionRecordChange, ContentError, ContentResult};

/// Rows one job of a touched-row pass reads: three records each, inside the
/// 64 keys of one read job.
pub(super) const PASS_ROWS: usize = 21;
/// Queued rows one release job reads with their node states: four records
/// each, inside the 64 keys of one read job.
pub(super) const RELEASE_ROWS: usize = 16;

pub(super) struct IndexedReducer<'s, 'b> {
    pub state: &'s SerialState<'b>,
    input: &'s dyn OperationInput,
    dropped: &'s dyn DroppedParents,
    pub work: ReferenceWork,
    pub touched: u64,
    pub(super) window: Window,
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
            window: Window::default(),
        }
    }
    pub fn declare_new(&mut self, serial: u64) -> ContentResult<()> {
        if serial == 0 || !self.input.is_new(serial)? || self.dropped.contains(serial)? {
            return Err(ContentError::InvalidRecord("filesystem fresh declaration"));
        }
        self.defer(serial, Step::Declare)
    }
    /// Whether the input declares the serial fresh in the result.
    pub(super) fn expected(&self, serial: u64) -> ContentResult<bool> {
        Ok(self.input.is_new(serial)? && !self.dropped.contains(serial)?)
    }
    /// A stored row is only read once every held meaning is written.
    fn settled(&self) -> ContentResult<()> {
        if self.window.is_empty() {
            Ok(())
        } else {
            Err(ContentError::InvalidRecord("filesystem reference window"))
        }
    }
    /// The fresh member, the row and the touched witness of one serial, read
    /// as one job, and whether the serial is fresh. The three are checked in
    /// the order they were once read in.
    #[allow(clippy::type_complexity)]
    fn stored(&self, serial: u64) -> ContentResult<(bool, Option<Vec<u8>>, Option<Vec<u8>>)> {
        self.settled()?;
        let expected = self.expected(serial)?;
        let [member, row, touch] = self
            .state
            .read_kinds([wire::FRESH, wire::ROW, wire::TOUCH], serial)?;
        self.fresh(expected, member.as_deref())?;
        Ok((expected, row, touch))
    }
    /// Checks one serial's fresh member against what the input declares.
    pub(super) fn fresh(&self, expected: bool, member: Option<&[u8]>) -> ContentResult<()> {
        if let Some(bytes) = member {
            wire::membership(bytes)?;
        }
        if expected != member.is_some() {
            return Err(ContentError::InvalidRecord("filesystem fresh membership"));
        }
        Ok(())
    }
    /// The required rows of at most [`PASS_ROWS`] serials, their three records
    /// each read in one job. Every row is checked as `row` checks one, in the
    /// order of `serials`.
    pub fn required_many(&self, serials: &[u64]) -> ContentResult<Vec<Row>> {
        if serials.len() > PASS_ROWS {
            return Err(ContentError::InvalidRecord("filesystem touched window"));
        }
        let mut rows = Vec::with_capacity(serials.len());
        self.read_rows(serials, None, |_, row, _| {
            rows.push(row?);
            Ok(())
        })?;
        Ok(rows)
    }
    /// One job for the member, row and witness of every serial and, when
    /// asked, one more record of each. `each` is given, in the order of
    /// `serials`, the index, the required row checked as `row` checks one, and
    /// the extra record: it decides when a row's refusal is returned.
    pub fn read_rows(
        &self,
        serials: &[u64],
        also: Option<u32>,
        mut each: impl FnMut(usize, ContentResult<Row>, Option<Vec<u8>>) -> ContentResult<()>,
    ) -> ContentResult<()> {
        self.settled()?;
        let width = 3 + usize::from(also.is_some());
        let mut keys = Vec::with_capacity(width * serials.len());
        for serial in serials {
            keys.extend([wire::FRESH, wire::ROW, wire::TOUCH].map(|kind| wire::key(kind, *serial)));
            keys.extend(also.map(|kind| wire::key(kind, *serial)));
        }
        let mut values = self.state.read_keys(&keys)?.into_iter();
        // The input answers for the whole job at once; a serial it calls
        // fresh is then asked of the dropped parents one at a time.
        let fresh = self.input.are_new(serials)?;
        if fresh.len() != serials.len() {
            return Err(ContentError::InvalidRecord("filesystem fresh answers"));
        }
        for (index, serial) in serials.iter().enumerate() {
            let mut next = || values.next().ok_or(ContentError::UnexpectedEof);
            let (member, raw, touch) = (next()?, next()?, next()?);
            let extra = if also.is_some() { next()? } else { None };
            let expected = if fresh[index] {
                self.dropped.contains(*serial).map(|dropped| !dropped)
            } else {
                Ok(false)
            };
            let row = expected.and_then(|expected| {
                self.fresh(expected, member.as_deref())?;
                self.decoded(*serial, expected, raw.as_deref(), touch.as_deref())?
                    .ok_or(ContentError::InvalidRecord(
                        "filesystem reference row missing",
                    ))
            });
            each(index, row, extra)?;
        }
        Ok(())
    }
    /// Decodes one serial's row and checks it against its fresh tag and its
    /// touched witness.
    pub(super) fn decoded(
        &self,
        serial: u64,
        fresh: bool,
        raw: Option<&[u8]>,
        touch: Option<&[u8]>,
    ) -> ContentResult<Option<Row>> {
        let row = raw.map(|bytes| wire::row(bytes, serial)).transpose()?;
        if row.is_some_and(|row| matches!(row, Row::Count { .. }) != fresh) {
            return Err(ContentError::InvalidRecord(
                "filesystem reference fresh tag",
            ));
        }
        if let Some(bytes) = touch {
            wire::membership(bytes)?;
        }
        if row.is_some() != touch.is_some() {
            return Err(ContentError::InvalidRecord(
                "filesystem reference row missing",
            ));
        }
        Ok(row)
    }
    fn prepare(
        &self,
        serial: u64,
        mutation: impl FnOnce(&mut Row) -> ContentResult<()>,
    ) -> ContentResult<Transition> {
        if serial == 0 {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
        let (fresh, old, touch) = self.stored(serial)?;
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
        self.note(serial, Step::Retained)
    }
    pub fn note_removed_binding(&mut self, serial: u64) -> ContentResult<()> {
        self.note(serial, Step::Removed)
    }
    pub fn note_value(&mut self, serial: u64, value: InodeValue) -> ContentResult<()> {
        self.note(serial, Step::Value(value))
    }
    fn note(&mut self, serial: u64, step: Step) -> ContentResult<()> {
        if serial == 0 {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
        self.defer(serial, step)
    }
}
