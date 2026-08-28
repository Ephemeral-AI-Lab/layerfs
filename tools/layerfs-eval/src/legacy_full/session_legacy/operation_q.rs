use super::super::OperationCounters;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OperationQObservation {
    pub current_bytes: u64,
    pub high_water_bytes: u64,
}

#[derive(Default)]
pub(crate) struct OperationQ {
    current: AtomicU64,
    high_water: AtomicU64,
}

impl OperationQ {
    pub(crate) fn reserve(self: &Arc<Self>) -> OperationReservation {
        let current = self
            .current
            .fetch_add(super::super::OPERATION_Q_BOUND_BYTES, Ordering::AcqRel)
            + super::super::OPERATION_Q_BOUND_BYTES;
        self.high_water.fetch_max(current, Ordering::AcqRel);
        OperationReservation(self.clone())
    }

    pub(super) fn observation(&self) -> OperationQObservation {
        OperationQObservation {
            current_bytes: self.current.load(Ordering::Acquire),
            high_water_bytes: self.high_water.load(Ordering::Acquire),
        }
    }
}

pub(crate) struct OperationReservation(Arc<OperationQ>);

impl OperationReservation {
    pub(crate) fn finish(self, counters: &mut OperationCounters) {
        let queue = self.0.clone();
        let active = queue.observation();
        counters.operation_q_current_bytes = active.current_bytes;
        counters.operation_q_high_water_bytes = active.high_water_bytes;
        drop(self);
        counters.operation_q_terminal_bytes = queue.observation().current_bytes;
    }
}

impl Drop for OperationReservation {
    fn drop(&mut self) {
        self.0
            .current
            .fetch_sub(super::super::OPERATION_Q_BOUND_BYTES, Ordering::AcqRel);
    }
}
