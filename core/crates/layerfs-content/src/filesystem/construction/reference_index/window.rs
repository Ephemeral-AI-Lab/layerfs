//! The reducer's write-behind window: the declarations and the binding and
//! value meanings of at most 21 serials, read as one job and written as one.
use super::reduce::{IndexedReducer, PASS_ROWS};
use super::{
    super::reference::{meaning, record::Row},
    wire,
};
use crate::object::inode_leaf::InodeValue;
use crate::{ContentError, ContentResult};

/// Deferred meanings one window holds at most; a serial may repeat.
const WINDOW_STEPS: usize = 64;

#[derive(Clone, Copy)]
pub(super) enum Step {
    Declare,
    Retained,
    Removed,
    Value(InodeValue),
}
/// Deferred meanings in arrival order and their distinct serials in key
/// order, each with whether the input declares it fresh: asked when the
/// serial enters the window. No list grows with the operation.
#[derive(Default)]
pub(super) struct Window {
    steps: Vec<(u64, Step)>,
    serials: Vec<u64>,
    fresh: Vec<bool>,
}
impl Window {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}
/// One serial of a window. A serial the window declares is written with all
/// three guards absent; any other is read first and guarded by what was read.
#[derive(Default)]
struct Slot {
    declared: bool,
    member: Option<Vec<u8>>,
    stored: (Option<Vec<u8>>, Option<Vec<u8>>),
    row: Option<Row>,
}
impl IndexedReducer<'_, '_> {
    /// Holds one meaning for the window's single write. The window is written
    /// first when it is full, and before a second declaration of a serial it
    /// already holds, which the stored guards must refuse as they always did.
    pub(super) fn defer(&mut self, serial: u64, step: Step) -> ContentResult<()> {
        let held = self.window.serials.binary_search(&serial).is_ok();
        if self.window.steps.len() == WINDOW_STEPS
            || (!held && self.window.serials.len() == PASS_ROWS)
            || (held && matches!(step, Step::Declare))
        {
            self.flush()?;
        }
        if let Err(at) = self.window.serials.binary_search(&serial) {
            // A declaration was already checked against the input.
            let fresh = matches!(step, Step::Declare) || self.expected(serial)?;
            self.window.serials.insert(at, serial);
            self.window.fresh.insert(at, fresh);
        }
        self.window.steps.push((serial, step));
        Ok(())
    }
    /// Reads the window's undeclared serials in one job, applies every held
    /// meaning in arrival order — a later meaning of a serial to the result
    /// of its earlier one — and writes all rows in one guarded job. A refusal
    /// writes nothing of the window.
    pub(super) fn flush(&mut self) -> ContentResult<()> {
        if self.window.is_empty() {
            return Ok(());
        }
        let Window {
            steps,
            serials,
            fresh,
        } = std::mem::take(&mut self.window);
        let slot = |serial: &u64| {
            serials
                .binary_search(serial)
                .map_err(|_| ContentError::InvalidRecord("filesystem reference window"))
        };
        let mut slots = Vec::with_capacity(serials.len());
        slots.resize_with(serials.len(), Slot::default);
        let mut first = [true; PASS_ROWS];
        for (serial, step) in &steps {
            let index = slot(serial)?;
            slots[index].declared |= first[index] && matches!(step, Step::Declare);
            first[index] = false;
        }
        let mut keys = Vec::with_capacity(3 * serials.len());
        for (serial, _) in serials
            .iter()
            .zip(&slots)
            .filter(|(_, slot)| !slot.declared)
        {
            keys.extend([wire::FRESH, wire::ROW, wire::TOUCH].map(|kind| wire::key(kind, *serial)));
        }
        if !keys.is_empty() {
            let mut values = self.state.read_keys(&keys)?.into_iter();
            for slot in slots.iter_mut().filter(|slot| !slot.declared) {
                let mut next = || values.next().ok_or(ContentError::UnexpectedEof);
                (slot.member, slot.stored) = (next()?, (next()?, next()?));
            }
        }
        for (serial, step) in &steps {
            let index = slot(serial)?;
            let slot = &mut slots[index];
            if matches!(step, Step::Declare) {
                continue;
            }
            let mut row = match slot.row {
                Some(row) => row,
                None => self.opened(*serial, fresh[index], slot)?,
            };
            match step {
                Step::Retained => meaning::retained(&mut row)?,
                Step::Removed => meaning::removed(&mut row)?,
                Step::Value(value) => meaning::value(&mut row, *value),
                Step::Declare => {}
            }
            slot.row = Some(row);
        }
        let mut created = 0_u64;
        let mut changes = Vec::with_capacity(3 * serials.len());
        for (serial, slot) in serials.iter().zip(slots) {
            created += u64::from(slot.stored.0.is_none() && slot.row.is_some());
            let row = slot.row.map(|row| row.encode()).transpose()?;
            let witness = row.is_some().then(|| vec![1]);
            if slot.declared {
                changes.push(wire::change(wire::FRESH, *serial, None, Some(vec![1])));
            }
            // Even an unchanged touched witness is an exact coupled guard.
            changes.push(wire::change(
                wire::ROW,
                *serial,
                slot.stored.0,
                row.map(|row| row.to_vec()),
            ));
            changes.push(wire::change(wire::TOUCH, *serial, slot.stored.1, witness));
        }
        let touched = self
            .touched
            .checked_add(created)
            .ok_or(ContentError::LengthOverflow)?;
        changes.sort_by_key(|change| change.key);
        self.state.apply(changes)?;
        self.touched = touched;
        self.work.rows_touched = self.work.rows_touched.saturating_add(created);
        Ok(())
    }
    /// The row a serial's first held meaning starts from, with the checks a
    /// single row read makes. A serial this window declared has no stored
    /// member to check: the one write guards its absence.
    fn opened(&self, serial: u64, fresh: bool, slot: &Slot) -> ContentResult<Row> {
        if !slot.declared {
            self.fresh(fresh, slot.member.as_deref())?;
        }
        let stored = self.decoded(
            serial,
            fresh,
            slot.stored.0.as_deref(),
            slot.stored.1.as_deref(),
        )?;
        Ok(stored.unwrap_or(if fresh {
            Row::Count {
                serial,
                value: None,
                count: 0,
            }
        } else {
            Row::Effect {
                serial,
                value: None,
                delta: 0,
            }
        }))
    }
}
