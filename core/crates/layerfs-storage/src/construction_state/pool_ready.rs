//! Exact successful reset and complete internal working lifetime before idle reuse.
use super::{graph_state::Graph, root_retire::RootRetirement, session::Resource};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::GraphMemory;
impl Resource {
    pub(crate) fn pool_ready(&self, status_handles: usize) -> StorageResult<()> {
        if !(4..=8).contains(&self.plan.version())
            || !self.known_clean
            || !self.logical_released
            || self.unknown.get()
            || self.native.quarantined
            || self.release_attempted
            || self.pool_attempt.is_some()
        {
            return Err(StorageError::Integrity(
                "scratch pool successful reset required",
            ));
        }
        if self.sites.as_ref().is_some_and(|s| s.failed.get())
            || self
                .graph
                .as_ref()
                .is_some_and(|g| g.failed.get() || g.attempt.is_some())
            || self
                .aliases
                .as_ref()
                .is_some_and(|a| a.failed.get() || a.attempt.is_some())
            || self
                .facts
                .as_ref()
                .is_some_and(|f| f.failed.get() || f.attempt.is_some())
            || self
                .counts
                .as_ref()
                .is_some_and(|c| c.failed.get() || c.attempt.is_some())
            || self
                .releasing
                .as_ref()
                .is_some_and(|r| r.failed.get() || r.attempt.is_some())
            || self
                .draft
                .as_ref()
                .is_some_and(|d| d.failed.get() || d.attempt.is_some() || !d.ledger.ended)
        {
            return Err(StorageError::Integrity("scratch pool failed/pending owner"));
        }
        self.verify()?;
        if let Some(memory) = &self.graph_memory {
            let mut bytes = GraphMemory::control_bytes();
            let mut handles = 1 + status_handles;
            if self.graph.is_some() {
                bytes += std::mem::size_of::<Graph>();
                handles += 2;
            }
            if self.aliases.is_some() {
                bytes += super::graph_layout::ALIAS_PERSISTENT;
                handles += 2;
            }
            if self.facts.is_some() {
                bytes += super::graph_layout::FACT_PERSISTENT;
                handles += 2;
            }
            if let Some(c) = &self.counts {
                bytes += std::mem::size_of::<super::count_state::Counts>()
                    + std::mem::size_of::<super::count_state::CountsOwner>();
                handles += 2;
                if c.retirement.is_some() {
                    bytes += std::mem::size_of::<super::count_state::RetireProof>()
                        + std::mem::size_of::<super::count_state::RetireOwner>();
                    handles += 1;
                }
            }
            if self.releasing.is_some() {
                bytes += std::mem::size_of::<super::release_state::Release>()
                    + std::mem::size_of::<super::release_state::ReleaseOwner>()
                    + std::mem::size_of::<super::release_state::FoldOwner>()
                    + std::mem::size_of::<layerfs_content::filesystem::state::ZeroLedger>();
                handles += 3;
            }
            if self.root_retirement.is_some() {
                bytes += std::mem::size_of::<RootRetirement>();
                handles += 1;
            }
            if memory.reserved_bytes() != bytes || memory.handles() != handles {
                return Err(StorageError::Integrity(
                    "scratch pool held data/metadata consumer",
                ));
            }
        }
        Ok(())
    }
}
