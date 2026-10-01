//! Finite external contract provider. Resident maps are explicitly not a RAM/SQL proof.
#![allow(dead_code)]
#[path = "site_state.rs"]
mod sites;
use layerfs_content::filesystem::rows::BindingSourceId;
use layerfs_content::filesystem::state::*;
use layerfs_content::filesystem::{FilesystemRootId, InodeScope};
use layerfs_content::{ContentError, ContentResult};
use std::collections::{BTreeMap, BTreeSet};

pub fn scopes(
    source: BindingSourceId,
    namespace: InodeScope,
    base: Option<FilesystemRootId>,
    root: u64,
    capacity: GraphCapacity,
) -> GraphConstructionScopes {
    let mut selection = StateSelection::issue([0x97; 32]).unwrap();
    selection.bind_owner([0x28; 32]).unwrap();
    let subject = GraphSubject::new(source, namespace, base, root, capacity).unwrap();
    GraphConstructionScopes::new(selection, subject).unwrap()
}
pub struct ObservedGraph {
    pub scope: GraphScope,
    pub sites: sites::ObservedSites,
    pub roots: ResidentState,
    pub nodes: BTreeMap<GraphNodeKey, GraphNode>,
    pub edges: BTreeMap<GraphEdgeKey, GraphEdge>,
    pub unexpanded: BTreeSet<GraphNodeKey>,
    pub totals: GraphTotals,
    pub stage: GraphStage,
    pub failed: bool,
    pub adjacency: Option<GraphAdjacencySeal>,
    pub proof: Option<GraphProofSeal>,
    pub current: Option<GraphNodeKey>,
    pub discovery: u32,
    pub pop_root: Option<GraphNodeKey>,
    pub popped: u64,
    pub any_seed: bool,
    pub singleton_loop: bool,
    pub abandonments: u64,
    pub retirements: u64,
    pub expansions: u64,
    pub max_targets: usize,
    pub fail_at: Option<&'static str>,
    pub corrupt_ack: bool,
}
impl ObservedGraph {
    pub fn new(scopes: &GraphConstructionScopes, directories: usize, bindings: usize) -> Self {
        let old = SiteConstructionScopes::new(
            scopes.roots().selection().clone(),
            scopes.graph().subject().source_id(),
        )
        .unwrap();
        Self {
            scope: scopes.graph().clone(),
            sites: sites::ObservedSites::new(&old, directories, bindings),
            roots: ResidentState::new(scopes.roots().clone(), directories).unwrap(),
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            unexpanded: BTreeSet::new(),
            totals: GraphTotals::default(),
            stage: GraphStage::Deferred,
            failed: false,
            adjacency: None,
            proof: None,
            current: None,
            discovery: 0,
            pop_root: None,
            popped: 0,
            any_seed: false,
            singleton_loop: false,
            abandonments: 0,
            retirements: 0,
            expansions: 0,
            max_targets: 0,
            fail_at: None,
            corrupt_ack: false,
        }
    }
    pub fn direct(scopes: &GraphConstructionScopes) -> Self {
        let mut state = Self::new(scopes, 0, 0);
        state.sites.stage = 4;
        state
    }
    fn selected(&self, scope: &GraphScope) -> ContentResult<()> {
        if scope != &self.scope {
            return Err(bad("oracle graph scope"));
        }
        if self.failed {
            return Err(bad("oracle abandoned graph"));
        }
        Ok(())
    }
    fn operation(&self, name: &'static str) -> ContentResult<()> {
        if self.fail_at == Some(name) {
            return Err(ContentError::ProviderFailure { what: name });
        }
        Ok(())
    }
    fn building(&mut self, scope: &GraphScope) -> ContentResult<()> {
        self.selected(scope)?;
        if self.sites.stage != 4 || self.sites.abandoned {
            return Err(bad("oracle sites not retired"));
        }
        if self.stage == GraphStage::Deferred {
            self.stage = GraphStage::Seeding;
        }
        if !matches!(self.stage, GraphStage::Seeding | GraphStage::Expanding) {
            return Err(bad("oracle graph build stage"));
        }
        Ok(())
    }
    fn sealed(&self, seal: &GraphAdjacencySeal) -> ContentResult<()> {
        self.selected(seal.scope())?;
        if self.adjacency.as_ref() != Some(seal) {
            return Err(bad("oracle graph selected seal"));
        }
        Ok(())
    }
    fn root_ready(&self) -> ContentResult<()> {
        if self.stage != GraphStage::Retired || self.failed || self.sites.abandoned {
            return Err(bad("oracle graph not retired"));
        }
        Ok(())
    }
    fn seal_now(&self) -> GraphAdjacencySeal {
        let mut nodes = GraphNodeLedger::adjacency(self.scope.clone());
        for row in self.nodes.values() {
            nodes.acknowledge(&[row.projection()]).unwrap();
        }
        let mut edges = GraphEdgeLedger::new(self.scope.clone());
        for row in self.edges.values() {
            edges.acknowledge(&[*row]).unwrap();
        }
        GraphAdjacencySeal::new(
            self.scope.clone(),
            nodes.records(),
            edges.records(),
            nodes.digest(),
            edges.digest(),
        )
        .unwrap()
    }
    fn node_after(&self, after: Option<GraphNodeKey>) -> impl Iterator<Item = &GraphNode> + Clone {
        let start = after
            .map(std::ops::Bound::Excluded)
            .unwrap_or(std::ops::Bound::Unbounded);
        self.nodes
            .range((start, std::ops::Bound::Unbounded))
            .map(|(_, node)| node)
    }
    fn parent_edges(&self, parent: u64) -> impl DoubleEndedIterator<Item = &GraphEdge> + Clone {
        let start = GraphEdgeKey::new(&self.scope, parent, 1).unwrap();
        let end = GraphEdgeKey::new(&self.scope, parent, i64::MAX as u64).unwrap();
        self.edges.range(start..=end).map(|(_, edge)| edge)
    }
    fn first_edge(&self, parent: GraphNode) -> Option<GraphEdge> {
        self.edges
            .values()
            .find(|edge| {
                edge.key().parent() == parent.key().serial()
                    && edge.key().child() > parent.after_child()
            })
            .copied()
    }
}
impl EffectiveGraphState for ObservedGraph {
    fn graph_select(&self, scope: &GraphScope) -> ContentResult<()> {
        self.selected(scope)?;
        self.operation("graph select")
    }
    fn graph_capacity(&self, scope: &GraphScope) -> ContentResult<GraphCapacity> {
        self.selected(scope)?;
        Ok(scope.capacity())
    }
    fn graph_seed_batch(
        &mut self,
        scope: &GraphScope,
        seeds: &[GraphNodeKey],
    ) -> ContentResult<GraphBuildAck> {
        self.building(scope)?;
        self.operation("graph seeds")?;
        if self.stage != GraphStage::Seeding
            || scope.subject().mode() != GraphMode::Update
            || seeds.len() > 128
        {
            return Err(bad("oracle seed stage"));
        }
        let mut changes = BTreeMap::new();
        for key in seeds {
            GraphNodeKey::decode(scope, key.as_bytes())?;
            let before = self.nodes.get(key).copied();
            let after = match before {
                Some(row) => row.seeded(scope)?,
                None => GraphNode::birth(scope, key.serial(), true)?,
            };
            changes.insert(*key, GraphNodeChange::new(scope, before, after)?);
        }
        let add = changes
            .values()
            .filter(|row| row.before().is_none())
            .count() as u64;
        scope
            .capacity()
            .check_growth(self.totals.nodes(), self.totals.edges(), add, 0)?;
        let after = GraphTotals::new(
            self.totals.nodes() + add,
            self.totals.edges(),
            self.totals.multiplicity(),
        )?;
        let rows = exact(changes.values().copied(), changes.len());
        let ack = GraphBuildAck::new(scope.clone(), self.totals, after, rows, Vec::new(), None)?;
        for change in ack.nodes() {
            self.nodes.insert(change.after().key(), change.after());
            if !change.after().expanded() {
                self.unexpanded.insert(change.after().key());
            }
        }
        self.totals = after;
        Ok(ack)
    }
    fn graph_root(&mut self, scope: &GraphScope) -> ContentResult<GraphBuildAck> {
        self.building(scope)?;
        if scope.subject().mode() != GraphMode::Fresh || !self.nodes.is_empty() {
            return Err(bad("oracle fresh root"));
        }
        self.operation("graph root")?;
        let root = GraphNode::birth(scope, scope.subject().root_serial(), false)?;
        let after = GraphTotals::new(1, 0, 0)?;
        let ack = GraphBuildAck::new(
            scope.clone(),
            self.totals,
            after,
            vec![GraphNodeChange::new(scope, None, root)?],
            Vec::new(),
            None,
        )?;
        self.nodes.insert(root.key(), root);
        self.unexpanded.insert(root.key());
        self.totals = after;
        self.stage = GraphStage::Expanding;
        Ok(ack)
    }
    fn graph_unexpanded(&mut self, scope: &GraphScope) -> ContentResult<Option<GraphNode>> {
        self.building(scope)?;
        self.stage = GraphStage::Expanding;
        self.operation("graph select")?;
        Ok(self
            .unexpanded
            .iter()
            .next()
            .and_then(|key| self.nodes.get(key))
            .copied())
    }
    fn graph_append(
        &mut self,
        scope: &GraphScope,
        parent: &GraphNode,
        children: &[GraphNodeKey],
    ) -> ContentResult<GraphBuildAck> {
        self.building(scope)?;
        self.operation("graph append")?;
        if self.stage != GraphStage::Expanding
            || parent.expanded()
            || self.nodes.get(&parent.key()) != Some(parent)
            || children.len() > 63
        {
            return Err(bad("oracle append selection"));
        }
        let mut grouped = BTreeMap::new();
        for child in children {
            GraphNodeKey::decode(scope, child.as_bytes())?;
            *grouped.entry(*child).or_insert(0u32) += 1;
        }
        let mut nc = BTreeMap::new();
        nc.insert(
            parent.key(),
            GraphNodeChange::new(scope, Some(*parent), *parent)?,
        );
        let mut ec = BTreeMap::new();
        for (key, count) in grouped {
            let before = self.nodes.get(&key).copied();
            let row = before.unwrap_or(GraphNode::birth(scope, key.serial(), false)?);
            let after = row.add_incoming(scope, count)?;
            nc.insert(key, GraphNodeChange::new(scope, before, after)?);
            let key = GraphEdgeKey::new(scope, parent.key().serial(), key.serial())?;
            let before = self.edges.get(&key).copied();
            let after = match before {
                Some(row) => row.add(scope, count)?,
                None => GraphEdge::new(scope, key.parent(), key.child(), count)?,
            };
            ec.insert(key, GraphEdgeChange::new(scope, before, after)?);
        }
        let addn = nc.values().filter(|row| row.before().is_none()).count() as u64;
        let adde = ec.values().filter(|row| row.before().is_none()).count() as u64;
        scope
            .capacity()
            .check_growth(self.totals.nodes(), self.totals.edges(), addn, adde)?;
        let after = GraphTotals::new(
            self.totals.nodes() + addn,
            self.totals.edges() + adde,
            self.totals.multiplicity() + children.len() as u64,
        )?;
        let updated = nc.get(&parent.key()).unwrap().after();
        let ack = GraphBuildAck::new(
            scope.clone(),
            self.totals,
            after,
            exact(nc.values().copied(), nc.len()),
            exact(ec.values().copied(), ec.len()),
            Some(updated),
        )?;
        self.max_targets = self.max_targets.max(nc.len() + ec.len());
        for change in ack.nodes() {
            self.nodes.insert(change.after().key(), change.after());
            if !change.after().expanded() {
                self.unexpanded.insert(change.after().key());
            }
        }
        for change in ack.edges() {
            self.edges.insert(change.after().key(), change.after());
        }
        self.totals = after;
        Ok(ack)
    }
    fn graph_expanded(
        &mut self,
        scope: &GraphScope,
        parent: &GraphNode,
    ) -> ContentResult<GraphNode> {
        self.building(scope)?;
        self.operation("graph expanded")?;
        if self.nodes.get(&parent.key()) != Some(parent) {
            return Err(bad("oracle expansion current"));
        }
        let key = GraphEdgeKey::new(scope, parent.key().serial(), parent.key().serial())?;
        let after = parent.finish_expansion(scope, self.edges.contains_key(&key))?;
        self.nodes.insert(after.key(), after);
        self.unexpanded.remove(&after.key());
        self.expansions += 1;
        Ok(after)
    }
    fn graph_seal(&mut self, scope: &GraphScope) -> ContentResult<GraphAdjacencySeal> {
        self.building(scope)?;
        self.operation("graph seal")?;
        if self.nodes.values().any(|row| !row.expanded()) {
            return Err(bad("oracle unexpanded seal"));
        }
        let seal = self.seal_now();
        self.adjacency = Some(seal.clone());
        self.stage = GraphStage::AdjacencySealed;
        Ok(seal)
    }
    fn graph_node(
        &mut self,
        seal: &GraphAdjacencySeal,
        key: GraphNodeKey,
    ) -> ContentResult<Option<GraphNode>> {
        self.sealed(seal)?;
        self.operation("graph node")?;
        GraphNodeKey::decode(seal.scope(), key.as_bytes())?;
        Ok(self.nodes.get(&key).copied())
    }
    fn graph_node_page(
        &mut self,
        seal: &GraphAdjacencySeal,
        after: Option<GraphNodeKey>,
        limit: GraphPageLimit,
    ) -> ContentResult<GraphNodePage> {
        self.sealed(seal)?;
        self.operation("graph node page")?;
        let fit = limit.fitting_nodes();
        let iterator = self.node_after(after);
        let available = iterator.clone().take(fit + 1).count();
        if available != 0 && fit == 0 {
            limit.check(1, 1, 318, 60)?;
        }
        let records = exact(
            iterator.take(fit).map(|row| row.projection()),
            available.min(fit),
        );
        let eof = available <= fit;
        let page = GraphNodePage::after(seal.clone(), after, records, eof)?;
        page.check_limit(limit)?;
        Ok(page)
    }
    fn graph_edge_page(
        &mut self,
        seal: &GraphAdjacencySeal,
        parent: u64,
        after: Option<u64>,
        limit: GraphPageLimit,
    ) -> ContentResult<GraphEdgePage> {
        self.sealed(seal)?;
        self.operation("graph edge page")?;
        let max = self
            .parent_edges(parent)
            .next_back()
            .map(|row| row.key().child());
        let fit = limit.fitting_edges();
        let iter = self
            .parent_edges(parent)
            .filter(|row| after.is_none_or(|after| row.key().child() > after));
        let count = iter.clone().take(fit + 1).count();
        if count != 0 && fit == 0 {
            limit.check(1, 1, 318, 43)?;
        }
        let records = exact(iter.take(fit).copied(), count.min(fit));
        let eof = records.last().map(|row| row.key().child()).or(after) == max;
        let page = GraphEdgePage::after(seal.clone(), parent, after, max, records, eof)?;
        page.check_limit(limit)?;
        Ok(page)
    }
    fn graph_begin_scc(&mut self, seal: &GraphAdjacencySeal) -> ContentResult<()> {
        self.sealed(seal)?;
        self.operation("graph begin SCC")?;
        if self.stage != GraphStage::AdjacencySealed
            || seal.scope().subject().mode() != GraphMode::Update
        {
            return Err(bad("oracle solver phase"));
        }
        self.stage = GraphStage::Solving;
        Ok(())
    }
    fn graph_cas(
        &mut self,
        seal: &GraphAdjacencySeal,
        mutations: &[GraphMutation],
    ) -> ContentResult<GraphMutationAck> {
        self.sealed(seal)?;
        self.operation("graph CAS")?;
        GraphMutationLimit::default().check(seal.scope(), mutations)?;
        if self.stage != GraphStage::Solving || mutations.len() != 1 {
            return Err(bad("oracle finite solver batch"));
        }
        let mutation = mutations[0];
        for (before, _) in mutation.changes() {
            if let Some(before) = before {
                if self.nodes.get(&before.key()) != Some(&before) {
                    return Err(bad("oracle CAS expected"));
                }
            }
        }
        match mutation {
            GraphMutation::EnterRoot { before, after } => {
                if self.current.is_some()
                    || after != before.enter(seal.scope(), self.discovery + 1, 0)?
                {
                    return Err(bad("oracle enter root"));
                }
                self.discovery += 1;
                self.current = Some(after.key());
            }
            GraphMutation::Descend {
                parent_before,
                parent_after,
                child_before,
                child_after,
                edge,
            } => {
                if self.current != Some(parent_before.key())
                    || self.first_edge(parent_before) != Some(edge)
                    || child_before.key().serial() != edge.key().child()
                    || child_after
                        != child_before.enter(
                            seal.scope(),
                            self.discovery + 1,
                            parent_before.key().serial(),
                        )?
                    || parent_after
                        != parent_before.advance(
                            seal.scope(),
                            edge.key().child(),
                            parent_before.lowlink(),
                        )?
                {
                    return Err(bad("oracle atomic descend"));
                }
                self.discovery += 1;
                self.current = Some(child_after.key());
            }
            GraphMutation::Advance {
                before,
                after,
                edge,
            } => {
                let child = self
                    .nodes
                    .get(&GraphNodeKey::new(seal.scope(), edge.key().child())?)
                    .copied()
                    .ok_or(bad("oracle absent child"))?;
                let low = if child.on_stack() {
                    before.lowlink().min(child.discovery())
                } else if child.completed() {
                    before.lowlink()
                } else {
                    return Err(bad("oracle white advance"));
                };
                if self.current != Some(before.key())
                    || self.first_edge(before) != Some(edge)
                    || after != before.advance(seal.scope(), edge.key().child(), low)?
                {
                    return Err(bad("oracle advance"));
                }
            }
            GraphMutation::Finish { before, after } => {
                if self.current != Some(before.key())
                    || self.first_edge(before).is_some()
                    || after != before.finish(seal.scope())?
                {
                    return Err(bad("oracle finish EOF"));
                }
            }
            GraphMutation::Return {
                parent_before,
                parent_after,
                child,
            } => {
                let row = *self
                    .nodes
                    .get(&child)
                    .ok_or(bad("oracle missing returned child"))?;
                if self.current != Some(child)
                    || parent_after != parent_before.returned(seal.scope(), row)?
                {
                    return Err(bad("oracle returned child"));
                }
                self.current = Some(parent_after.key());
            }
            GraphMutation::LeaveRoot { finished_root } => {
                if self.current != Some(finished_root.key())
                    || self.nodes.get(&finished_root.key()) != Some(&finished_root)
                    || !finished_root.completed()
                    || finished_root.parent() != 0
                {
                    return Err(bad("oracle root departure"));
                }
                self.current = None;
            }
        }
        let mut changes = Vec::new();
        for (before, after) in mutation.changes() {
            if let (Some(before), Some(after)) = (before, after) {
                changes.push(GraphNodeChange::new(seal.scope(), Some(before), after)?);
            }
        }
        changes.sort_by_key(|row| row.after().key());
        for row in &changes {
            self.nodes.insert(row.after().key(), row.after());
        }
        GraphMutationAck::new(
            seal.scope().clone(),
            if self.corrupt_ack {
                Vec::new()
            } else {
                exact(changes.iter().copied(), changes.len())
            },
        )
    }
    fn graph_pop(
        &mut self,
        seal: &GraphAdjacencySeal,
        root: &GraphNode,
        limit: GraphMutationLimit,
    ) -> ContentResult<GraphPopAck> {
        self.sealed(seal)?;
        self.operation("graph pop")?;
        if self.stage != GraphStage::Solving
            || self.current != Some(root.key())
            || root.lowlink() != root.discovery()
            || !root.dfs_finished()
        {
            return Err(bad("oracle pop root"));
        }
        if self.pop_root.is_none() {
            self.pop_root = Some(root.key());
            self.popped = 0;
            self.any_seed = false;
            self.singleton_loop = false;
        }
        if self.pop_root != Some(root.key()) {
            return Err(bad("oracle pop selection"));
        }
        let mut selected: Vec<_> = self
            .nodes
            .values()
            .filter(|row| row.on_stack() && row.discovery() >= root.discovery())
            .copied()
            .collect();
        selected.sort_by_key(|row| std::cmp::Reverse(row.discovery()));
        let mut members = Vec::with_capacity(limit.records().min(selected.len()));
        let mut done = false;
        for row in selected.into_iter().take(limit.records()) {
            let completed = row.complete(seal.scope(), root.discovery())?;
            self.nodes.insert(row.key(), completed);
            members.push(completed);
            self.popped += 1;
            self.any_seed |= row.seed();
            self.singleton_loop = self.popped == 1 && row.self_loop();
            if row.key() == root.key() {
                done = true;
                break;
            }
        }
        let rejected = done && self.any_seed && (self.popped > 1 || self.singleton_loop);
        let disposition = if rejected {
            GraphPopDisposition::RejectedCycle
        } else if done {
            GraphPopDisposition::Complete
        } else {
            GraphPopDisposition::More
        };
        let ack = GraphPopAck::new(
            seal.scope().clone(),
            root.key(),
            root.discovery(),
            members,
            GraphPopTotals::new(self.popped, self.any_seed, self.singleton_loop)?,
            disposition,
        )?;
        if rejected {
            self.stage = GraphStage::Rejected;
            self.failed = true;
        }
        if done {
            self.pop_root = None;
        }
        Ok(ack)
    }
    fn graph_finish(&mut self, seal: &GraphAdjacencySeal) -> ContentResult<GraphProofSeal> {
        self.sealed(seal)?;
        self.operation("graph finish")?;
        if self.seal_now() != *seal
            || self.nodes.values().any(|row| {
                !row.expanded()
                    || seal.scope().subject().mode() == GraphMode::Update
                        && (!row.completed() || row.on_stack())
            })
        {
            return Err(bad("oracle proof invariant"));
        }
        let mut ledger = GraphNodeLedger::proof(seal.scope().clone());
        for row in self.nodes.values() {
            ledger.acknowledge(&[*row])?;
        }
        let proof = GraphProofSeal::new(seal.clone(), ledger.digest());
        self.proof = Some(proof.clone());
        self.stage = GraphStage::Proved;
        Ok(proof)
    }
    fn graph_proof_page(
        &mut self,
        seal: &GraphProofSeal,
        after: Option<GraphNodeKey>,
        limit: GraphPageLimit,
    ) -> ContentResult<GraphProofPage> {
        self.sealed(seal.adjacency())?;
        self.operation("graph proof page")?;
        if self.proof.as_ref() != Some(seal) {
            return Err(bad("oracle selected proof"));
        }
        let fit = limit.fitting_proof_records();
        let iter = self.node_after(after);
        let count = iter.clone().take(fit + 1).count();
        if count != 0 && fit == 0 {
            limit.check(1, 1, 350, 60)?;
        }
        let rows = exact(iter.take(fit).copied(), count.min(fit));
        let page = GraphProofPage::after(seal.clone(), after, rows, count <= fit)?;
        page.check_limit(limit)?;
        Ok(page)
    }
    fn graph_retire(&mut self, seal: &GraphProofSeal) -> ContentResult<()> {
        self.sealed(seal.adjacency())?;
        self.operation("graph retire")?;
        if self.proof.as_ref() != Some(seal) {
            return Err(bad("oracle retire proof"));
        }
        self.nodes.clear();
        self.edges.clear();
        self.unexpanded.clear();
        self.stage = GraphStage::Retired;
        self.retirements += 1;
        Ok(())
    }
    fn graph_abandon(&mut self, scope: &GraphScope) -> ContentResult<()> {
        if scope != &self.scope {
            return Err(bad("oracle abandon selected scope"));
        }
        self.failed = true;
        self.abandonments += 1;
        Ok(())
    }
}
fn exact<T>(iter: impl Iterator<Item = T>, count: usize) -> Vec<T> {
    let mut rows = Vec::with_capacity(count);
    rows.extend(iter);
    assert_eq!(rows.capacity(), count);
    rows
}
fn bad(reason: &'static str) -> ContentError {
    ContentError::InvalidOrderingRecord(reason)
}

impl BindingSiteState for ObservedGraph {
    fn site_capacity(&self, scope: &SiteScope) -> ContentResult<SiteCapacity> {
        self.sites.site_capacity(scope)
    }
    fn site_get(&mut self, scope: &SiteScope, key: SiteKey) -> ContentResult<Option<SiteRecord>> {
        self.sites.site_get(scope, key)
    }
    fn site_insert_batch(
        &mut self,
        scope: &SiteScope,
        records: &[SiteRecord],
    ) -> ContentResult<ClaimAdmission> {
        self.sites.site_insert_batch(scope, records)
    }
    fn site_close_membership(&mut self, expected: &SiteBirthSeal) -> ContentResult<SiteMembership> {
        self.sites.site_close_membership(expected)
    }
    fn site_parent_present(
        &mut self,
        members: &SiteMembership,
        parent: u64,
    ) -> ContentResult<bool> {
        self.sites.site_parent_present(members, parent)
    }
    fn site_parent_page(
        &mut self,
        members: &SiteMembership,
        parent: u64,
        after: Option<u32>,
        limit: SiteParentPageLimit,
    ) -> ContentResult<SiteParentPage> {
        self.sites.site_parent_page(members, parent, after, limit)
    }
    fn site_observe_base_batch(
        &mut self,
        members: &SiteMembership,
        observations: &[SiteObservation],
    ) -> ContentResult<()> {
        self.sites.site_observe_base_batch(members, observations)
    }
    fn site_final_seal(&mut self, members: &SiteMembership) -> ContentResult<SiteSeal> {
        self.sites.site_final_seal(members)
    }
    fn site_sealed_page(
        &mut self,
        seal: &SiteSeal,
        after: Option<SiteKey>,
        limit: SitePageLimit,
    ) -> ContentResult<SitePage> {
        self.sites.site_sealed_page(seal, after, limit)
    }
    fn site_retire(&mut self, seal: &SiteSeal) -> ContentResult<()> {
        self.sites.site_retire(seal)
    }
    fn site_abandon(&mut self, scope: &SiteScope) -> ContentResult<()> {
        self.sites.site_abandon(scope)
    }
}

impl IndexedState for ObservedGraph {
    fn capacity(&self, scope: &StateScope) -> ContentResult<StateCapacity> {
        self.root_ready()?;
        self.roots.capacity(scope)
    }
    fn append(&mut self, scope: &StateScope, rows: &[StateRecord]) -> ContentResult<()> {
        self.root_ready()?;
        self.roots.append(scope, rows)
    }
    fn seal(&mut self, scope: &StateScope) -> ContentResult<StateSeal> {
        self.root_ready()?;
        self.roots.seal(scope)
    }
    fn get(&mut self, seal: &StateSeal, key: StateKey) -> ContentResult<Option<StateRecord>> {
        self.root_ready()?;
        self.roots.get(seal, key)
    }
    fn page(
        &mut self,
        seal: &StateSeal,
        after: Option<StateKey>,
        limit: PageLimit,
    ) -> ContentResult<StatePage> {
        self.root_ready()?;
        self.roots.page(seal, after, limit)
    }
    fn release(&mut self, scope: &StateScope) -> ContentResult<()> {
        self.root_ready()?;
        self.roots.release(scope)
    }
}
