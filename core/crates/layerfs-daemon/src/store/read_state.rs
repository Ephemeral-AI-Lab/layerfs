//! Fixed scheduling state for the Store's opened read set.
use super::{
    read_handle::StoreReader,
    read_service::{ReadLimits, ReadServiceWork},
    PortError,
};
use layerfs_history::WorkspaceId;
use std::{collections::VecDeque, mem::size_of, sync::Arc, task::Waker, time::Instant};

pub(super) enum Disposition {
    Free,
    Waiting,
    Ready(usize, Box<StoreReader>),
    Leased,
    Failed,
}
pub(super) struct Slot {
    pub state: Disposition,
    pub lane: usize,
    pub waker: Option<Waker>,
    pub admitted: Instant,
    pub queue_wait_ns: u64,
}
pub(super) struct Lane {
    pub workspace: Option<WorkspaceId>,
    pub active: bool,
    pub outstanding: usize,
    pub queue: VecDeque<usize>,
}
pub(super) struct State {
    pub slots: Vec<Slot>,
    pub lanes: Vec<Lane>,
    pub next_lane: usize,
    pub idle: VecDeque<(usize, Box<StoreReader>)>,
    pub retired: Vec<Option<(Box<StoreReader>, Arc<PortError>)>>,
    pub stopping: bool,
    pub poisoned: bool,
    pub work: ReadServiceWork,
}
impl State {
    pub fn new(readers: Vec<StoreReader>, limits: ReadLimits) -> Self {
        let count = readers.len();
        let created = Instant::now();
        let slots = (0..limits.namespaces * limits.requests_per_namespace)
            .map(|_| Slot {
                state: Disposition::Free,
                lane: 0,
                waker: None,
                admitted: created,
                queue_wait_ns: 0,
            })
            .collect::<Vec<_>>();
        let lanes = (0..limits.namespaces)
            .map(|_| Lane {
                workspace: None,
                active: false,
                outstanding: 0,
                queue: VecDeque::with_capacity(limits.requests_per_namespace),
            })
            .collect::<Vec<_>>();
        let idle = readers
            .into_iter()
            .enumerate()
            .map(|(id, reader)| (id, Box::new(reader)))
            .collect::<VecDeque<_>>();
        let retired = (0..count).map(|_| None).collect::<Vec<_>>();
        let bytes = size_of::<Self>()
            + slots.capacity() * size_of::<Slot>()
            + lanes.capacity() * size_of::<Lane>()
            + idle.capacity() * size_of::<(usize, Box<StoreReader>)>()
            + retired.capacity() * size_of::<Option<(Box<StoreReader>, Arc<PortError>)>>()
            + count * size_of::<StoreReader>()
            + lanes
                .iter()
                .map(|lane| lane.queue.capacity() * size_of::<usize>())
                .sum::<usize>();
        Self {
            slots,
            lanes,
            next_lane: 0,
            idle,
            retired,
            stopping: false,
            poisoned: false,
            work: ReadServiceWork {
                readers: count,
                scheduler_bytes: bytes,
                ..ReadServiceWork::default()
            },
        }
    }
    pub fn remove(&mut self, index: usize) {
        let lane_index = self.slots[index].lane;
        self.slots[index].state = Disposition::Free;
        let lane = &mut self.lanes[lane_index];
        lane.outstanding -= 1;
        self.work.outstanding -= 1;
        if lane.outstanding == 0 {
            lane.active = false;
        }
    }
    pub fn terminal(&self) -> bool {
        self.stopping || self.work.quarantined == self.work.readers
    }
    /// One FIFO per Workspace; successive assignments rotate among active
    /// Workspace lanes. A granted reader is already removed from idle capacity.
    pub fn distribute(&mut self) {
        if self.terminal() {
            for lane in &mut self.lanes {
                for index in lane.queue.drain(..) {
                    self.slots[index].state = Disposition::Failed;
                    self.work.waiting -= 1;
                }
            }
            return;
        }
        while !self.idle.is_empty() {
            let mut selected = None;
            for offset in 0..self.lanes.len() {
                let lane = (self.next_lane + offset) % self.lanes.len();
                if let Some(index) = self.lanes[lane].queue.pop_front() {
                    selected = Some(index);
                    self.next_lane = (lane + 1) % self.lanes.len();
                    break;
                }
            }
            let Some(index) = selected else { break };
            let (id, reader) = self.idle.pop_front().expect("idle read capacity");
            let slot = &mut self.slots[index];
            self.work.waiting -= 1;
            self.work.assigned += 1;
            self.work.grants = self.work.grants.saturating_add(1);
            let wait = slot.admitted.elapsed().as_nanos().min(u64::MAX as u128) as u64;
            slot.queue_wait_ns = wait;
            self.work.queue_wait_ns = self.work.queue_wait_ns.saturating_add(wait);
            self.work.maximum_queue_wait_ns = self.work.maximum_queue_wait_ns.max(wait);
            slot.state = Disposition::Ready(id, reader);
        }
    }
}
