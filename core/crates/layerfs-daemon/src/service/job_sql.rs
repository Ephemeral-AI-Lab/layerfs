//! Actual statement families of one original owner job, in exact owned rows.
use layerfs_overlay::{DatabaseWork, StatementKind, StatementWork};
use std::mem::size_of;

/// Statement families of one Overlay connection snapshot.
pub(crate) const FAMILIES: usize = size_of::<DatabaseWork>() / size_of::<StatementWork>();
/// Distinct families one Lifecycle job can execute in maintained source: the
/// pre-BEGIN freelist read, BEGIN, COMMIT or ROLLBACK (never both), and at most
/// four of the Workspace/Lease/Reclaim/Capture/Inode/Frontier/OperationRecord domains.
/// Lifecycle jobs never park, so no readiness family is added. This bounds the
/// admitted receipt charge only: a further actual family is retained and
/// charged when observed, never dropped.
pub(crate) const LIFECYCLE_FAMILIES: usize = 7;
const _: () = assert!(FAMILIES <= u16::BITS as usize && LIFECYCLE_FAMILIES <= FAMILIES);

/// Statement work attributed to one original job, including every readiness
/// turn which parked it. A family has a row only when it observed work; values
/// are the connection deltas of this job's exclusive turns, never derived from
/// its class or command name.
#[derive(Debug, Default)]
pub struct JobSql {
    mask: u16,
    rows: Box<[StatementWork]>,
}
impl JobSql {
    /// Number of families with observed work.
    pub fn families(&self) -> usize {
        self.rows.len()
    }
    /// This job's row for one family, absent when the job did no such work.
    pub fn family(&self, kind: StatementKind) -> Option<&StatementWork> {
        let bit = 1_u16 << kind as usize;
        if self.mask & bit == 0 {
            return None;
        }
        self.rows.get((self.mask & (bit - 1)).count_ones() as usize)
    }
    /// Sum of the observed families. Inclusive spans overlap service time.
    pub fn total(&self) -> StatementWork {
        let mut total = StatementWork::default();
        for row in &self.rows {
            total.accumulate(*row);
        }
        total
    }
    /// Family-indexed form comparable with an owner connection delta.
    pub fn expanded(&self) -> DatabaseWork {
        let mut all = DatabaseWork::default();
        let mut rows = self.rows.iter();
        for (index, to) in all.statements.iter_mut().enumerate() {
            if self.mask & (1 << index) != 0 {
                if let Some(row) = rows.next() {
                    *to = *row;
                }
            }
        }
        all
    }
    /// Owned row bytes retained with this receipt.
    pub(crate) fn heap_bytes(&self) -> usize {
        self.rows.len() * size_of::<StatementWork>()
    }
    /// Adds one exclusive turn. The previous rows are released before their
    /// exact successor is allocated, so both never coexist.
    pub(crate) fn accumulate(&mut self, delta: &DatabaseWork) {
        let empty = StatementWork::default();
        if delta.statements.iter().all(|row| *row == empty) {
            return;
        }
        let mut all = self.expanded();
        self.mask = 0;
        self.rows = Box::default();
        all.accumulate(*delta);
        let count = all.statements.iter().filter(|row| **row != empty).count();
        let mut rows = Vec::with_capacity(count);
        for (index, row) in all.statements.iter().enumerate() {
            if *row != empty {
                self.mask |= 1 << index;
                rows.push(*row);
            }
        }
        self.rows = rows.into_boxed_slice();
    }
}
