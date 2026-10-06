//! Ready-only Workspace rotation and fixed service classes.
use super::Ticket;
use layerfs_history::WorkspaceId;
use std::collections::{BTreeMap, VecDeque};

struct Lane {
    classes: [VecDeque<Ticket>; 6],
    next: usize,
}
pub(super) struct Queue {
    lanes: BTreeMap<WorkspaceId, Lane>,
    ready: VecDeque<WorkspaceId>,
    next_class: BTreeMap<WorkspaceId, usize>,
}
impl Queue {
    pub fn new() -> Self {
        Self {
            lanes: BTreeMap::new(),
            ready: VecDeque::new(),
            next_class: BTreeMap::new(),
        }
    }
    pub fn push(&mut self, workspace: WorkspaceId, class: usize, ticket: Ticket) {
        if let std::collections::btree_map::Entry::Vacant(entry) = self.lanes.entry(workspace) {
            self.ready.push_back(workspace);
            entry.insert(Lane {
                classes: std::array::from_fn(|_| VecDeque::new()),
                next: self.next_class.get(&workspace).copied().unwrap_or(0),
            });
        }
        self.lanes.get_mut(&workspace).expect("ready lane").classes[class].push_back(ticket);
    }
    pub fn take(&mut self) -> Option<Ticket> {
        let workspace = self.ready.pop_front()?;
        let lane = self.lanes.get_mut(&workspace).expect("ready Workspace");
        let mut ticket = None;
        for _ in 0..6 {
            let class = lane.next;
            lane.next = (class + 1) % 6;
            if let Some(found) = lane.classes[class].pop_front() {
                ticket = Some(found);
                break;
            }
        }
        self.next_class.insert(workspace, lane.next);
        if lane.classes.iter().all(VecDeque::is_empty) {
            self.lanes.remove(&workspace);
        } else {
            self.ready.push_back(workspace);
        }
        ticket
    }
    pub fn cancel(&mut self, workspace: WorkspaceId, keep: impl Fn(Ticket) -> bool) {
        if let Some(lane) = self.lanes.get_mut(&workspace) {
            for class in &mut lane.classes {
                class.retain(|id| keep(*id));
            }
            if lane.classes.iter().all(VecDeque::is_empty) {
                self.lanes.remove(&workspace);
                self.ready.retain(|id| *id != workspace);
            }
        }
    }
    pub fn forget_workspace(&mut self, workspace: WorkspaceId) {
        if !self.lanes.contains_key(&workspace) {
            self.next_class.remove(&workspace);
        }
    }
}
