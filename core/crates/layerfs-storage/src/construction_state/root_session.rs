//! Real terminal roots completion before the current C1 coordinator returns.

use layerfs_content::filesystem::state::{GraphMemory, GraphStage, StateSeal};

use super::graph_state::Graph;
use super::plan::Plan;
use super::root_retire::{self, RootRetirement, RootRetirementProgress, RootRetirementStage};
use super::{root_reset, ScratchSession};
use crate::error::{StorageError, StorageResult};

impl ScratchSession {
    /// Retire one exact terminal root window; the first call ends ordinary reads.
    /// Only private profile4/5 known successful root state selects this capability.
    pub fn retire_roots_window(
        &mut self,
        seal: &StateSeal,
    ) -> StorageResult<RootRetirementProgress> {
        let result = (|| {
            let resource = self
                .resource
                .as_mut()
                .ok_or(StorageError::Integrity("construction roots released owner"))?;
            if !matches!(
                resource.plan,
                Plan::SitesGraphThenRoots { .. }
                    | Plan::AliasesSitesGraphThenRoots { .. }
                    | Plan::NamespaceSitesGraphThenRoots { .. }
                    | Plan::CanonicalSitesGraphThenRoots { .. }
            ) || resource.known_clean
                || resource.unknown.get()
                || resource.release_attempted
                || resource
                    .graph
                    .as_ref()
                    .is_none_or(|graph| graph.failed.get())
                || resource
                    .sites
                    .as_ref()
                    .is_none_or(|sites| sites.failed.get())
            {
                return Err(StorageError::Integrity(
                    "construction roots retirement owner/profile",
                ));
            }
            if resource.root_retirement.is_none() {
                resource.check_seal(seal)?;
                resource.verify()?;
                let graph = resource.graph.as_ref().unwrap();
                if graph.stage != GraphStage::Retired
                    || graph.failed.get()
                    || graph.attempt.is_some()
                    || resource
                        .sites
                        .as_ref()
                        .is_none_or(|sites| sites.failed.get())
                {
                    return Err(StorageError::Integrity(
                        "construction roots incomplete dependency",
                    ));
                }
                if resource.aliases.as_ref().is_some_and(|aliases| {
                    aliases.stage != 4
                        || aliases.failed.get()
                        || aliases.attempt.is_some()
                        || aliases.totals.remaining != 0
                }) {
                    return Err(StorageError::Integrity(
                        "construction roots alias retirement required",
                    ));
                }
                if resource.facts.as_ref().is_some_and(|facts| {
                    facts.failed.get()
                        || facts.attempt.is_some()
                        || facts.base.stage != 4
                        || facts.parent.stage != 5
                        || facts.base.remaining != 0
                        || facts.parent.remaining != 0
                }) {
                    return Err(StorageError::Integrity(
                        "construction roots facts retirement required",
                    ));
                }
                if resource.canonical_capacity.is_some() {
                    let counts = resource
                        .counts
                        .as_ref()
                        .ok_or(StorageError::Integrity("construction roots missing counts"))?;
                    if counts.failed.get()
                        || counts.attempt.is_some()
                        || counts.snapshot.stage != 6
                        || counts.live() != (0, 0)
                    {
                        return Err(StorageError::Integrity(
                            "construction roots count retirement required",
                        ));
                    }
                    if resource.releasing.as_ref().is_some_and(|release| {
                        release.failed.get()
                            || release.attempt.is_some()
                            || release.snapshot.stage != 5
                            || release.live_jobs() != 0
                            || release.snapshot.frames != 0
                    }) {
                        return Err(StorageError::Integrity(
                            "construction roots release retirement required",
                        ));
                    }
                    if counts.snapshot.seeds.is_some() && resource.releasing.is_none() {
                        return Err(StorageError::Integrity(
                            "construction roots missing descendant release",
                        ));
                    }
                }
                let maximum = resource.ledger.as_ref().and_then(|ledger| ledger.last());
                resource.root_retirement = Some(RootRetirement::new(
                    resource.graph_memory.as_ref().unwrap(),
                    seal.clone(),
                    maximum,
                )?);
                resource.logical_released = true;
            }
            let state = resource.root_retirement.as_ref().unwrap();
            if state.seal != *seal
                || state.attempt.is_some()
                || !matches!(
                    state.stage,
                    RootRetirementStage::Selected
                        | RootRetirementStage::Retiring
                        | RootRetirementStage::Retired
                )
            {
                return Err(StorageError::Integrity(
                    "construction roots exact retirement state",
                ));
            }
            if state.stage == RootRetirementStage::Retired {
                return Ok(state.progress());
            }
            resource.verify()?;
            let attempt = root_retire::prepare(
                resource.connection.as_ref().unwrap(),
                resource.graph_memory.as_ref().unwrap(),
                state,
            )?;
            resource.root_retirement.as_mut().unwrap().attempt = Some(attempt);
            resource.native.reserve()?;
            root_retire::commit(
                resource.connection.as_ref().unwrap(),
                resource.header.as_ref().unwrap().as_bytes(),
                resource.root_retirement.as_ref().unwrap(),
                resource.engine,
            )?;
            resource.native.observe_allocation()?;
            let state = resource.root_retirement.as_mut().unwrap();
            root_retire::acknowledge(state);
            Ok(state.progress())
        })();
        self.finish(result)
    }

    /// Reset fixed profile4/5 owner rows after known exact retirement.
    /// Native bytes stay charged; uncertain or held dependent ownership refuses.
    pub fn reset_completed_roots(&mut self, seal: &StateSeal) -> StorageResult<()> {
        let result = (|| {
            let resource = self
                .resource
                .as_mut()
                .ok_or(StorageError::Integrity("construction roots released owner"))?;
            if resource.known_clean
                || resource.unknown.get()
                || resource.release_attempted
                || resource
                    .graph
                    .as_ref()
                    .is_none_or(|graph| graph.failed.get())
                || resource
                    .sites
                    .as_ref()
                    .is_none_or(|sites| sites.failed.get())
            {
                return Err(StorageError::Integrity(
                    "construction roots reset unavailable",
                ));
            }
            let state = resource
                .root_retirement
                .as_ref()
                .ok_or(StorageError::Integrity(
                    "construction roots missing retirement",
                ))?;
            if state.seal != *seal
                || state.stage != RootRetirementStage::Retired
                || state.attempt.is_some()
                || state.ledger.seal() != *seal
            {
                return Err(StorageError::Integrity(
                    "construction roots reset terminal seal",
                ));
            }
            let memory = resource.graph_memory.as_ref().unwrap();
            let expected = GraphMemory::control_bytes()
                + std::mem::size_of::<Graph>()
                + std::mem::size_of::<RootRetirement>()
                + if resource.aliases.is_some() {
                    super::graph_layout::ALIAS_PERSISTENT
                } else {
                    0
                }
                + if resource.facts.is_some() {
                    super::graph_layout::FACT_PERSISTENT
                } else {
                    0
                }
                + resource.canonical_persistent_bytes();
            if memory.reserved_bytes() != expected {
                return Err(StorageError::Integrity(
                    "construction roots held dependent working owner",
                ));
            }
            resource.verify()?;
            resource.native.reserve()?;
            resource.root_retirement.as_mut().unwrap().stage = RootRetirementStage::Resetting;
            root_reset::commit(resource.connection.as_ref().unwrap(), resource)?;
            resource.native.observe_allocation()?;
            root_reset::verify(resource.connection.as_ref().unwrap(), resource)?;
            resource.root_retirement.as_mut().unwrap().stage = RootRetirementStage::Clean;
            resource.known_clean = true;
            Ok(())
        })();
        self.finish(result)
    }
}
