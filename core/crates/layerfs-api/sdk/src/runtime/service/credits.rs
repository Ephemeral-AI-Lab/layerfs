//! Queue/result ownership and per-Workspace shares, never filesystem state.
use super::{ServiceClass, ServiceConfig, ServiceWork};
use crate::runtime::SaveId;
use layerfs_history::WorkspaceId;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

pub(super) struct Ledger {
    pub config: ServiceConfig,
    pub work: ServiceWork,
    workspaces: BTreeMap<WorkspaceId, usize>,
    saves: BTreeMap<SaveId, usize>,
    pub owners: Rc<Cell<usize>>,
}
impl Ledger {
    pub fn new(config: ServiceConfig, owners: Rc<Cell<usize>>) -> Self {
        Self {
            config,
            work: ServiceWork::default(),
            workspaces: BTreeMap::new(),
            saves: BTreeMap::new(),
            owners,
        }
    }
    pub fn save_owners(&self, save: SaveId) -> usize {
        self.saves.get(&save).copied().unwrap_or(0)
    }
}
pub(super) struct Credit {
    pub ledger: Rc<RefCell<Ledger>>,
    workspace: WorkspaceId,
    bytes: usize,
    save: Option<SaveId>,
}
impl Credit {
    pub fn acquire(
        ledger: &Rc<RefCell<Ledger>>,
        workspace: WorkspaceId,
        class: ServiceClass,
        bytes: usize,
        save: Option<SaveId>,
    ) -> Option<Self> {
        let mut state = ledger.borrow_mut();
        let limit = match class {
            ServiceClass::Accept => {
                state.config.bytes - state.config.read_reserve - state.config.control_reserve
            }
            ServiceClass::Demand => state.config.bytes - state.config.control_reserve,
            _ => state.config.bytes,
        };
        let jobs = state.workspaces.get(&workspace).copied().unwrap_or(0);
        if bytes > limit
            || state.work.credited_bytes > limit - bytes
            || state.work.outstanding >= state.config.jobs
            || jobs >= state.config.jobs_per_workspace
        {
            state.work.refused = state.work.refused.saturating_add(1);
            return None;
        }
        state.work.credited_bytes += bytes;
        state.work.peak_credited_bytes = state
            .work
            .peak_credited_bytes
            .max(state.work.credited_bytes);
        state.work.outstanding += 1;
        state.work.admitted = state.work.admitted.saturating_add(1);
        *state.workspaces.entry(workspace).or_default() += 1;
        if let Some(save) = save {
            *state.saves.entry(save).or_default() += 1;
        }
        drop(state);
        ledger.borrow().owners.set(ledger.borrow().owners.get() + 1);
        Some(Self {
            ledger: ledger.clone(),
            workspace,
            bytes,
            save,
        })
    }
    pub fn bind_save(&mut self, save: SaveId) {
        if self.save.is_none() {
            *self.ledger.borrow_mut().saves.entry(save).or_default() += 1;
            self.save = Some(save);
        }
    }
}
impl Drop for Credit {
    fn drop(&mut self) {
        let mut state = self.ledger.borrow_mut();
        state.owners.set(state.owners.get() - 1);
        state.work.credited_bytes -= self.bytes;
        state.work.outstanding -= 1;
        let count = state
            .workspaces
            .get_mut(&self.workspace)
            .expect("credited Workspace");
        *count -= 1;
        if *count == 0 {
            state.workspaces.remove(&self.workspace);
        }
        if let Some(save) = self.save {
            let count = state.saves.get_mut(&save).expect("credited Save");
            *count -= 1;
            if *count == 0 {
                state.saves.remove(&save);
            }
        }
    }
}
