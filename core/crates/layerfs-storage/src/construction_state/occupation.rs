//! Same-file logical population admission at every owning growth boundary.
use super::{session::Resource, sites::SiteStage};
use crate::StorageResult;
use layerfs_content::filesystem::state::{
    CanonicalCapacity, CanonicalOccupancy, FactCapacity, FactOccupancy, GraphStage,
};
impl Resource {
    pub(crate) fn canonical_occupancy(&self) -> CanonicalOccupancy {
        let (counts, zeros) = self.counts.as_ref().map_or((0, 0), |s| s.live());
        let (jobs, frames) = self
            .releasing
            .as_ref()
            .map_or((0, 0), |s| (s.live_jobs(), s.snapshot.frames));
        CanonicalOccupancy {
            namespace: self.namespace_occupancy(),
            counts,
            zeros,
            jobs,
            frames,
        }
    }
    pub(crate) fn canonical_alias_capacity(
        &self,
    ) -> Option<(CanonicalCapacity, CanonicalOccupancy)> {
        self.canonical_capacity
            .map(|capacity| (capacity, self.canonical_occupancy()))
    }
    pub(crate) fn canonical_growth(
        &self,
        counts: Option<u64>,
        zeros: Option<u64>,
        jobs: Option<u64>,
        frames: Option<u64>,
    ) -> StorageResult<()> {
        let mut live = self.canonical_occupancy();
        if let Some(n) = counts {
            live.counts = n;
        }
        if let Some(n) = zeros {
            live.zeros = n;
        }
        if let Some(n) = jobs {
            live.jobs = n;
        }
        if let Some(n) = frames {
            live.frames = n;
        }
        if let Some(capacity) = self.canonical_capacity {
            capacity.check(live)?;
        }
        Ok(())
    }

    /// Acknowledged live rows; retired transcripts are retained metadata, not rows.
    pub(crate) fn namespace_occupancy(&self) -> FactOccupancy {
        let (facts, parents) = self.facts.as_ref().map_or((0, 0), |owner| {
            let base = match owner.base.stage {
                3 => owner.base.remaining,
                4 => 0,
                _ => owner.base.records,
            };
            let parent = match owner.parent.stage {
                4 => owner.parent.remaining,
                5 => 0,
                _ => owner.parent.records,
            };
            (base, parent)
        });
        let sites = self.sites.as_ref().map_or(0, |sites| {
            if sites.stage == SiteStage::BirthOpen {
                sites.records
            } else {
                sites.remaining
            }
        });
        let (alias_facts, alias_jobs) = self.aliases.as_ref().map_or((0, 0), |aliases| {
            (
                if aliases.stage >= 3 {
                    aliases.totals.remaining
                } else {
                    aliases.totals.facts
                },
                aliases.totals.jobs,
            )
        });
        let (graph_nodes, graph_edges) = self.graph.as_ref().map_or((0, 0), |graph| {
            if matches!(graph.stage, GraphStage::Retiring | GraphStage::Retired) {
                (graph.remaining_nodes, graph.remaining_edges)
            } else {
                (graph.totals.nodes(), graph.totals.edges())
            }
        });
        let roots = self.ledger.as_ref().map_or(0, |ledger| ledger.records());
        // Retirement validates every ordinal against this exact terminal seal
        // before advancing its ledger; that acknowledged prefix never exceeds it.
        let retired = self
            .root_retirement
            .as_ref()
            .map_or(0, |state| state.ledger.records());
        FactOccupancy {
            facts,
            parents,
            sites,
            alias_facts,
            alias_jobs,
            graph_nodes,
            graph_edges,
            roots: roots - retired,
        }
    }
    fn namespace_check(&self, live: FactOccupancy) -> StorageResult<()> {
        if let Some(facts) = &self.facts {
            facts.capacity.check(live)?;
        }
        if let Some(capacity) = self.canonical_capacity {
            let mut aggregate = self.canonical_occupancy();
            aggregate.namespace = live;
            capacity.check(aggregate)?;
        }
        Ok(())
    }
    /// Snapshot before alias state borrow, checked after exact enqueue planning.
    pub(crate) fn namespace_alias_capacity(&self) -> Option<(FactCapacity, FactOccupancy)> {
        self.facts
            .as_ref()
            .map(|owner| (owner.capacity, self.namespace_occupancy()))
    }
    /// Proposed exact Fact/Parent row counts before their owning SQL mutation.
    pub(crate) fn namespace_fact_growth(
        &self,
        base: Option<u64>,
        parent: Option<u64>,
    ) -> StorageResult<()> {
        let mut live = self.namespace_occupancy();
        if let Some(base) = base {
            live.facts = base;
        }
        if let Some(parent) = parent {
            live.parents = parent;
        }
        self.namespace_check(live)
    }
    pub(crate) fn namespace_sites_growth(&self, count: u64) -> StorageResult<()> {
        let mut live = self.namespace_occupancy();
        live.sites = count;
        self.namespace_check(live)
    }
    pub(crate) fn namespace_graph_growth(&self, nodes: u64, edges: u64) -> StorageResult<()> {
        let mut live = self.namespace_occupancy();
        live.graph_nodes = nodes;
        live.graph_edges = edges;
        self.namespace_check(live)
    }
    pub(crate) fn namespace_roots_growth(&self, count: u64) -> StorageResult<()> {
        let mut live = self.namespace_occupancy();
        live.roots = count;
        self.namespace_check(live)
    }
}
