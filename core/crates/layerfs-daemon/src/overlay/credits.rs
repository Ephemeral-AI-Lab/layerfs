//! Credits remain owned through queued, executing and retained-result lifetimes.
use crate::{
    commands::ServiceClass,
    queue::Shared,
    service::job_sql::{FAMILIES, LIFECYCLE_FAMILIES},
};
use layerfs_overlay::StatementWork;
use std::{
    mem::size_of,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Weak,
    },
};

pub(crate) struct Credit {
    pub shared: Weak<Shared>,
    pub namespace: i64,
    pub bytes: AtomicUsize,
    pub class: ServiceClass,
}
impl Credit {
    /// Charges retained receipt rows beyond this job's admitted allowance. Only
    /// a Lifecycle job exceeding its maintained family bound can owe any; the
    /// rows are kept and the excess is reported instead of being hidden.
    pub fn cover_receipt(&self, bytes: usize) {
        let allowed = if self.class == ServiceClass::Lifecycle {
            LIFECYCLE_FAMILIES
        } else {
            FAMILIES
        } * size_of::<StatementWork>();
        let Some(excess) = bytes.checked_sub(allowed).filter(|excess| *excess != 0) else {
            return;
        };
        let Some(shared) = self.shared.upgrade() else {
            return;
        };
        if let Ok(mut state) = shared.state.lock() {
            self.bytes.fetch_add(excess, Ordering::Relaxed);
            let work = &mut state.work;
            work.credited_bytes += excess;
            work.peak_credited_bytes = work.peak_credited_bytes.max(work.credited_bytes);
            work.receipt_overruns = work.receipt_overruns.saturating_add(1);
            work.receipt_overrun_bytes = work.receipt_overrun_bytes.saturating_add(excess);
        };
    }
}
impl Drop for Credit {
    fn drop(&mut self) {
        let Some(shared) = self.shared.upgrade() else {
            return;
        };
        if let Ok(mut state) = shared.state.lock() {
            state.work.credited_bytes -= *self.bytes.get_mut();
            state.work.outstanding -= 1;
            state.release(self.namespace, self.class);
        }
        shared.wake.notify_all();
    }
}
