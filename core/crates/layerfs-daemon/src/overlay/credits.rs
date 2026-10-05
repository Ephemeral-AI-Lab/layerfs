//! Credits remain owned through queued, executing and retained-result lifetimes.
use crate::{commands::ServiceClass, queue::Shared};
use std::sync::Weak;

pub(crate) struct Credit {
    pub shared: Weak<Shared>,
    pub namespace: i64,
    pub bytes: usize,
    pub class: ServiceClass,
}
impl Drop for Credit {
    fn drop(&mut self) {
        let Some(shared) = self.shared.upgrade() else {
            return;
        };
        if let Ok(mut state) = shared.state.lock() {
            state.work.credited_bytes -= self.bytes;
            state.work.outstanding -= 1;
            if let Some(lane) = state.lanes.get_mut(&self.namespace) {
                if self.class == ServiceClass::Lifecycle {
                    lane.lifecycle -= 1;
                } else {
                    lane.ordinary -= 1;
                }
                if lane.lifecycle == 0 && lane.ordinary == 0 {
                    state.lanes.remove(&self.namespace);
                    state.rotation.retain(|ns| *ns != self.namespace);
                }
            }
        }
        shared.wake.notify_all();
    }
}
