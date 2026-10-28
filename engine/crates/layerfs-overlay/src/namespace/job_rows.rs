//! Inode and name rows one atomic job has read, kept for that job only.
use crate::{Inode, NameLayers, COMPOUND_DIRECTORY_ENTRIES, COMPOUND_INODES};
use std::cell::RefCell;

/// A local inode row as its job read it: the value, the generation of the
/// row that holds it and that row's payload-layer columns.
#[derive(Clone)]
pub(crate) struct LocalRow {
    pub inode: Inode,
    pub gen: i64,
    pub epoch: i64,
    pub height: i64,
    pub referenced: bool,
    pub sequence: i64,
}
/// The latest local row of the serials one atomic job read, so that a later
/// evaluation round and the job's own publication do not read them again. It
/// is a value on the job's stack: nothing of it outlives the job, and every
/// entry is dropped by the statement that writes its serial. It holds as many
/// serials as one compound job can publish; a serial beyond that is read by
/// its indexed statements as before.
///
/// The same holds for the name rows of the job: the active and lower rows
/// of as many names as one compound job can bind. A job is one exclusive
/// turn of the only connection, and it writes a name's row after its last
/// read of that name, so a kept read is the row until the job writes it.
pub(crate) struct JobRows(
    RefCell<[Option<SeenRow>; COMPOUND_INODES]>,
    RefCell<[Option<SeenName>; COMPOUND_DIRECTORY_ENTRIES]>,
);
/// One read of one serial: the serial and its local row, None when the
/// serial has no local row.
type SeenRow = (i64, Option<LocalRow>);
/// One read of one name: its parent, its bytes and its local rows.
type SeenName = (i64, Vec<u8>, NameLayers);
impl JobRows {
    pub(crate) fn new() -> Self {
        Self(RefCell::default(), RefCell::default())
    }
    /// The local rows this job read for one name, if it read them.
    pub(crate) fn name(&self, parent: i64, name: &[u8]) -> Option<NameLayers> {
        self.1
            .borrow()
            .iter()
            .flatten()
            .find(|(key, bytes, _)| *key == parent && bytes == name)
            .map(|(_, _, layers)| *layers)
    }
    /// Keeps one name read while a place is free.
    pub(crate) fn record_name(&self, parent: i64, name: &[u8], layers: NameLayers) {
        if let Some(free) = self.1.borrow_mut().iter_mut().find(|slot| slot.is_none()) {
            *free = Some((parent, name.to_vec(), layers));
        }
    }
    /// What this job read for `serial`: outer None is not read, inner None
    /// is no local row.
    pub(crate) fn seen(&self, serial: i64) -> Option<Option<Inode>> {
        self.0
            .borrow()
            .iter()
            .flatten()
            .find(|(key, _)| *key == serial)
            .map(|(_, row)| row.as_ref().map(|row| row.inode.clone()))
    }
    /// Keeps one read while a place is free.
    pub(crate) fn record(&self, serial: i64, row: &Option<LocalRow>) {
        if let Some(free) = self.0.borrow_mut().iter_mut().find(|slot| slot.is_none()) {
            *free = Some((serial, row.clone()));
        }
    }
    /// Removes and returns what this job read for `serial`, for the
    /// statement that is about to write its row.
    pub(crate) fn take(&self, serial: i64) -> Option<Option<LocalRow>> {
        self.0
            .borrow_mut()
            .iter_mut()
            .find(|slot| matches!(slot, Some((key, _)) if *key == serial))
            .and_then(Option::take)
            .map(|(_, row)| row)
    }
}
