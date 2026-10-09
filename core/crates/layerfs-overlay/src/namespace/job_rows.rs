//! Inode rows one atomic job has read, kept for that job only.
use crate::{Inode, COMPOUND_INODES};
use std::cell::RefCell;

/// A local inode row as its job read it: the value, the generation of the
/// row that holds it and that row's payload-layer columns.
#[derive(Clone)]
pub(crate) struct LocalRow {
    pub inode: Inode,
    pub gen: i64,
    pub epoch: i64,
    pub height: i64,
}
/// The latest local row of the serials one atomic job read, so that a later
/// evaluation round and the job's own publication do not read them again. It
/// is a value on the job's stack: nothing of it outlives the job, and every
/// entry is dropped by the statement that writes its serial. It holds as many
/// serials as one compound job can publish; a serial beyond that is read by
/// its indexed statements as before.
pub(crate) struct JobRows(RefCell<[Option<(i64, Option<LocalRow>)>; COMPOUND_INODES]>);
impl JobRows {
    pub(crate) fn new() -> Self {
        Self(RefCell::new(Default::default()))
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
