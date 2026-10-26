//! The fixed table of write-recognizer slots, shared by every Workspace of
//! one daemon.
//!
//! A slot is the fixed-size state of one file's writes: no byte of a write
//! and no committed byte. A slot may hold one carried chunk, the identity so
//! far of the committed chunk that crossed the end of the file's last write;
//! it is fixed in size and counted in the allowance. The table is the
//! recognizer's whole allowance and never grows; the least recently used
//! file makes room, and a file without a slot starts over, which only costs
//! a missed claim.
use layerfs_workspace::Route;
use layerfs_workspace::{RecognizeWork, WriteRun};
use std::sync::{Mutex, MutexGuard};

/// Files whose writes are remembered at once.
pub const RECOGNIZER_SLOTS: usize = 64;

struct Slot {
    route: Route,
    serial: u64,
    run: WriteRun,
    used: u64,
}
struct Table {
    slots: Vec<Slot>,
    clock: u64,
    evictions: u64,
    work: RecognizeWork,
}
pub struct Recognizer(Mutex<Table>);
/// The table's occupancy, fixed allowance and cumulative pass work.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecognizerWork {
    pub slots: usize,
    pub capacity_bytes: usize,
    pub evictions: u64,
    pub passes: RecognizeWork,
}
impl Recognizer {
    pub(super) fn new() -> Self {
        Self(Mutex::new(Table {
            slots: Vec::with_capacity(RECOGNIZER_SLOTS),
            clock: 0,
            evictions: 0,
            work: RecognizeWork::default(),
        }))
    }
    /// The table holds plain values and every state of it is usable: a
    /// panic elsewhere while it was held loses at most one file's run.
    fn table(&self) -> MutexGuard<'_, Table> {
        self.0.lock().unwrap_or_else(|poison| poison.into_inner())
    }
    /// Takes one file's run out of the table for the length of a pass, so no
    /// lock is held while bytes are hashed. A file with no slot, or whose
    /// run another request holds, starts with one that knows nothing.
    pub fn take(&self, route: Route, serial: u64) -> WriteRun {
        let mut table = self.table();
        let found = table
            .slots
            .iter()
            .position(|slot| slot.serial == serial && slot.route == route);
        found.map_or_else(WriteRun::default, |index| {
            table.slots.swap_remove(index).run
        })
    }
    /// Returns a run with the work of its pass. The least recently used
    /// file makes room in a full table.
    pub fn put(&self, route: Route, serial: u64, run: WriteRun, work: &RecognizeWork) {
        let mut table = self.table();
        table.work.add(work);
        table.clock += 1;
        let slot = Slot {
            route,
            serial,
            run,
            used: table.clock,
        };
        let same = table
            .slots
            .iter()
            .position(|slot| slot.serial == serial && slot.route == route);
        let oldest = (table.slots.len() == RECOGNIZER_SLOTS)
            .then(|| {
                let slots = table.slots.iter().enumerate();
                slots
                    .min_by_key(|(_, slot)| slot.used)
                    .map(|(index, _)| index)
            })
            .flatten();
        match same.or(oldest) {
            Some(index) => {
                table.evictions += u64::from(same.is_none());
                table.slots[index] = slot;
            }
            None => table.slots.push(slot),
        }
    }
    /// Records that the write of `[offset, end)` of one file was published
    /// as `revision`. A file whose run a request holds, or that has no slot,
    /// records nothing: its carried chunk is then not completed.
    pub fn published(&self, route: Route, serial: u64, (offset, end): (u64, u64), revision: i64) {
        let mut table = self.table();
        let mut slots = table.slots.iter_mut();
        if let Some(slot) = slots.find(|slot| slot.serial == serial && slot.route == route) {
            slot.run.published(offset, end, revision);
        }
    }
    pub fn work(&self) -> RecognizerWork {
        let table = self.table();
        RecognizerWork {
            slots: table.slots.len(),
            capacity_bytes: RECOGNIZER_SLOTS
                * (std::mem::size_of::<Slot>() + WriteRun::CARRY_BYTES),
            evictions: table.evictions,
            passes: table.work,
        }
    }
}
