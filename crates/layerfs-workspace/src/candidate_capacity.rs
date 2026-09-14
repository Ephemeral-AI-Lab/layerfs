//! Canonical construction has an explicit domain beside the live-overlay limits.
//! The same host admission counts both; output keeps the reservation until the
//! last private object/admission owner drops. All disk growth uses one scope.
use crate::overlay_budget::{Budget, ConstructionReservation};
use layerfs_layerstack_store::{
    ObjectBuffer, Result, ScratchBudget, ScratchLimits, ScratchUsage, StoreError,
    WorkspaceAdmission,
};
use layerfs_workspace_core::ResourcePolicy;
use std::sync::Arc;

const MIB: u64 = 1024 * 1024;
// Existing host admission is128MiB; the default live Workspace envelope is32MiB.
// Keep that entire live allowance available beside one96MiB canonical domain.
// This envelope includes two private-output indexes/validation traversals,
// canonical operand/producer slabs, the comparison reader and final-delta map.
const MEMORY: u64 = 96 * MIB;
//64 frontier levels, their merge inputs/output, task/result journals, two V3
// journals and bounded producer/consumer spill/SQLite/order files fit128 FDs.
const FILES: usize = 128;
const INDEX_BYTES: usize = 4 * 1024 * 1024;
const ORDER_BYTES: usize = 64 * 1024;

pub(crate) struct CandidateCapacity {
    pub(crate) scratch: Arc<ScratchBudget>,
    pub(crate) memory_bytes: u64,
    pub(crate) disk_bytes: u64,
    pub(crate) files: usize,
}
impl CandidateCapacity {
    pub(crate) fn acquire(budget: &Budget, policy: ResourcePolicy) -> Result<Self> {
        let _ = ScratchBudget::reclaim_pending(8);
        // Source payload ownership already has its own reserve. This additional
        // domain covers a private canonical copy plus declared metadata scratch.
        let disk = policy
            .max_spool_bytes
            .checked_add(policy.overlay.max_scratch_bytes)
            .ok_or(StoreError::InvalidInput("canonical scratch allowance"))?;
        let memory = MEMORY
            .checked_add(policy.max_final_delta_memory_bytes.saturating_sub(8 * MIB))
            .ok_or(StoreError::InvalidInput("canonical memory allowance"))?;
        let owner: Arc<ConstructionReservation> =
            Arc::new(budget.reserve_construction(memory, disk, FILES)?);
        let scratch = ScratchBudget::new(
            ScratchLimits {
                bytes: disk,
                files: FILES,
            },
            owner,
        )?;
        Ok(Self {
            scratch,
            memory_bytes: memory,
            disk_bytes: disk,
            files: FILES,
        })
    }
    pub(crate) fn configure(&self, objects: &mut ObjectBuffer<'_>) -> Result<()> {
        objects.limit_private_indexes(INDEX_BYTES, ORDER_BYTES)?;
        objects.set_private_scratch(self.scratch.clone())?;
        objects.retain_private_owner(self.scratch.clone())
    }
    pub(crate) fn retain_admission(&self, admission: &mut WorkspaceAdmission) -> Result<()> {
        admission.retain_private_owner(self.scratch.clone())
    }
    pub(crate) fn usage(&self) -> ScratchUsage {
        self.scratch.usage()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay_budget::{HostAdmission, HostLimits};
    use layerfs_fuse::live_runtime::LiveRuntime;
    use layerfs_layerstack_store::{EntityName, LayerStackInitialization, LayerStackStore};
    use std::fs;
    #[test]
    fn actual_private_output_and_admission_keep_resource_owner_until_both_drop() {
        let directory =
            std::env::temp_dir().join(format!("candidate-capacity-{}", crate::WorkspaceId::new()));
        fs::create_dir_all(directory.join("source")).unwrap();
        let runtime = LiveRuntime::new().unwrap();
        let host = HostAdmission::new(runtime.scheduler(), HostLimits::default()).unwrap();
        let policy = ResourcePolicy::default();
        let resources = Budget::open(&directory, policy, host.clone()).unwrap();
        let baseline = host.usage();
        let store = LayerStackStore::create(directory.join("store.sqlite")).unwrap();
        let init = store
            .initialize_layerstack(
                EntityName::new("capacity").unwrap(),
                LayerStackInitialization::Directory(directory.join("source")),
            )
            .unwrap();
        let root = store.layer(init.genesis_layer_id).unwrap().unwrap().root_id;
        let reader = store.snapshot_reader(root);
        let capacity = CandidateCapacity::acquire(&resources.budget, policy).unwrap();
        let scratch = capacity.scratch.clone();
        let mut objects = ObjectBuffer::bounded_output(Some(&reader)).unwrap();
        capacity.configure(&mut objects).unwrap();
        // Repeated bytes deduplicate identical CDC chunks and need not spill.
        // A fixed varied input exercises actual private output allocation.
        let mut random = 7_u64;
        let bytes = (0..2 * 1024 * 1024)
            .map(|_| {
                random ^= random << 13;
                random ^= random >> 7;
                random ^= random << 17;
                random as u8
            })
            .collect::<Vec<_>>();
        let built = objects
            .build_complete_with_predecessor(std::io::Cursor::new(bytes), 2 * 1024 * 1024)
            .unwrap();
        assert!(
            scratch.usage().observed_allocated_bytes > 0,
            "real spill must be accounted"
        );
        let mut admission = store.workspace_admission([0xE1; 16]).unwrap();
        capacity.retain_admission(&mut admission).unwrap();
        drop(capacity);
        drop(scratch);
        assert!(host.usage().memory_bytes > baseline.memory_bytes);
        drop(built);
        assert!(
            host.usage().memory_bytes > baseline.memory_bytes,
            "admission still owns its buffers"
        );
        drop(admission);
        assert_eq!(host.usage().memory_bytes, baseline.memory_bytes);
        assert_eq!(
            host.usage().reserved_disk_bytes,
            baseline.reserved_disk_bytes
        );
        assert_eq!(host.usage().reserved_files, baseline.reserved_files);
        drop(reader);
        drop(store);
        drop(resources);
        assert_eq!(host.usage().memory_bytes, 0);
        drop(runtime);
        fs::remove_dir_all(directory).unwrap();
    }
}
