//! Exact supplied graph owner, bounded construction and logical terminalization.

use super::graph_state::{GraphAttempt, GraphAttemptKind};
use super::session::Resource;
use super::sites::SiteStage;
use super::{graph_build, graph_index, ScratchSession};
use crate::error::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{
    GraphBuildAck, GraphCapacity, GraphEdgeChange, GraphMode, GraphNode, GraphNodeChange,
    GraphNodeKey, GraphScope, GraphStage,
};
use layerfs_content::ContentError;

impl ScratchSession {
    /// Pure captured-context selection before any Sites effects. No query,
    /// mutation, quota observation or failure-capsule transfer occurs here.
    pub fn graph_select(&self, scope: &GraphScope) -> StorageResult<()> {
        self.ensure_graph_scope(scope)?;
        let resource = self.resource.as_ref().unwrap();
        resource.check_live()?;
        let graph = resource.graph.as_ref().unwrap();
        if resource
            .aliases
            .as_ref()
            .is_some_and(|aliases| aliases.failed.get())
            || resource.release_attempted
            || resource.unknown.get()
            || resource.native.quarantined
            || graph.failed.get()
            || graph.stage == GraphStage::Rejected
            || resource
                .sites
                .as_ref()
                .is_none_or(|sites| sites.failed.get())
            || resource.native.reserved_bytes != scope.capacity().scratch_bytes()
            || resource.graph_subject.as_ref() != Some(scope.subject())
        {
            return Err(StorageError::Integrity(
                "construction scratch graph selection unavailable",
            ));
        }
        Ok(())
    }

    pub(crate) fn ensure_graph_scope(&self, scope: &GraphScope) -> StorageResult<()> {
        if self
            .resource
            .as_ref()
            .filter(|resource| !resource.logical_released)
            .and_then(|resource| resource.graph.as_ref())
            .is_none_or(|graph| &graph.scope != scope)
        {
            return Err(StorageError::Content(ContentError::InvalidOrderingRecord(
                "graph foreign scope",
            )));
        }
        Ok(())
    }

    pub(crate) fn graph_resource(&self, scope: &GraphScope) -> StorageResult<&Resource> {
        self.ensure_graph_scope(scope)?;
        let resource = self.resource.as_ref().unwrap();
        resource.check_live()?;
        resource.graph.as_ref().unwrap().check_scope(scope)?;
        if let Some(aliases) = &resource.aliases {
            if aliases.stage != 4
                || aliases.failed.get()
                || aliases.attempt.is_some()
                || aliases.current.is_some()
                || aliases.totals.remaining != 0
            {
                return Err(StorageError::Integrity(
                    "construction graph alias retirement required",
                ));
            }
            let connection = resource.verify()?;
            if !super::alias_index::empty(connection, "alias_facts")?
                || !super::alias_index::empty(connection, "alias_jobs")?
            {
                return Err(StorageError::Integrity(
                    "construction graph alias retirement EOF",
                ));
            }
        }
        if resource.release_attempted
            || resource.unknown.get()
            || resource.native.quarantined
            || resource
                .sites
                .as_ref()
                .is_none_or(|sites| sites.stage != SiteStage::Retired || sites.failed.get())
        {
            return Err(StorageError::Integrity(
                "construction scratch graph owner unavailable",
            ));
        }
        Ok(resource)
    }

    pub(crate) fn graph_resource_mut(
        &mut self,
        scope: &GraphScope,
    ) -> StorageResult<&mut Resource> {
        self.graph_resource(scope)?;
        Ok(self.resource.as_mut().unwrap())
    }

    /// Exact selected graph capacity, without native/SQL effects or a whole-input fit claim.
    pub fn graph_capacity(&self, scope: &GraphScope) -> StorageResult<GraphCapacity> {
        self.ensure_graph_scope(scope)?;
        self.finish((|| {
            let resource = self.graph_resource(scope)?;
            Ok(resource.graph.as_ref().unwrap().scope.capacity())
        })())
    }

    /// Metadata-only own-attempt terminalization; foreign scope does not poison the owner.
    pub fn graph_abandon(&mut self, scope: &GraphScope) -> StorageResult<()> {
        self.ensure_graph_scope(scope)?;
        self.resource
            .as_ref()
            .unwrap()
            .graph
            .as_ref()
            .unwrap()
            .failed
            .set(true);
        self.finish(Ok(()))
    }

    /// Enroll one unordered bounded Update seed batch before expansion closes enrollment.
    pub fn graph_seed_batch(
        &mut self,
        scope: &GraphScope,
        keys: &[GraphNodeKey],
    ) -> StorageResult<GraphBuildAck> {
        self.ensure_graph_scope(scope)?;
        let result = (|| {
            let resource = self.graph_resource(scope)?;
            let graph = resource.graph.as_ref().unwrap();
            if scope.subject().mode() != GraphMode::Update
                || !matches!(graph.stage, GraphStage::Deferred | GraphStage::Seeding)
            {
                return Err(StorageError::Integrity(
                    "construction scratch graph seed phase",
                ));
            }
            graph_build::known_growth(graph, keys, None)?;
            let attempt = graph_build::seeds(resource.verify()?, graph, keys)?;
            self.apply_graph_build(scope, attempt, None)
        })();
        self.finish(result)
    }

    /// Enroll only the actual Fresh root from the pre-admitted subject.
    pub fn graph_root(&mut self, scope: &GraphScope) -> StorageResult<GraphBuildAck> {
        self.ensure_graph_scope(scope)?;
        let result = (|| {
            let resource = self.graph_resource(scope)?;
            let attempt = graph_build::root(resource.graph.as_ref().unwrap())?;
            self.apply_graph_build(scope, attempt, None)
        })();
        self.finish(result)
    }

    /// Indexed unique expansion frontier. The first Update call closes Seed enrollment.
    pub fn graph_unexpanded(&mut self, scope: &GraphScope) -> StorageResult<Option<GraphNode>> {
        self.ensure_graph_scope(scope)?;
        let result = (|| {
            let resource = self.graph_resource(scope)?;
            let graph = resource.graph.as_ref().unwrap();
            if matches!(graph.stage, GraphStage::Deferred | GraphStage::Seeding) {
                if scope.subject().mode() != GraphMode::Update {
                    return Err(StorageError::Integrity(
                        "construction scratch fresh root not enrolled",
                    ));
                }
                let attempt =
                    GraphAttempt::new(GraphAttemptKind::CloseSeeds, graph, GraphStage::Expanding)?;
                self.apply_graph_build(scope, attempt, None)?;
            }
            let resource = self.graph_resource(scope)?;
            if resource.graph.as_ref().unwrap().stage != GraphStage::Expanding {
                return Err(StorageError::Integrity(
                    "construction scratch graph expansion ended",
                ));
            }
            graph_index::unexpanded(resource.verify()?, scope)
        })();
        self.finish(result)
    }

    /// Group up to63 raw source children into exact combined node/edge changes.
    pub fn graph_append(
        &mut self,
        scope: &GraphScope,
        parent: &GraphNode,
        children: &[GraphNodeKey],
    ) -> StorageResult<GraphBuildAck> {
        self.ensure_graph_scope(scope)?;
        let result = (|| {
            let resource = self.graph_resource(scope)?;
            let graph = resource.graph.as_ref().unwrap();
            if graph.stage != GraphStage::Expanding
                || parent.expanded()
                || *parent != parent.projection()
            {
                return Err(StorageError::Integrity(
                    "construction scratch graph append phase",
                ));
            }
            graph_build::known_growth(graph, children, Some(*parent))?;
            let attempt = graph_build::append(resource.verify()?, graph, *parent, children)?;
            self.apply_graph_build(scope, attempt, Some(parent.key()))
        })();
        self.finish(result)
    }

    /// Complete the caller's acknowledged immutable EffectiveEntries EOF once.
    pub fn graph_expanded(
        &mut self,
        scope: &GraphScope,
        parent: &GraphNode,
    ) -> StorageResult<GraphNode> {
        self.ensure_graph_scope(scope)?;
        let result = (|| {
            let resource = self.graph_resource(scope)?;
            GraphNode::decode(scope, &parent.encode())?;
            if resource.graph.as_ref().unwrap().stage != GraphStage::Expanding
                || parent.expanded()
                || *parent != parent.projection()
            {
                return Err(StorageError::Integrity(
                    "construction scratch graph expansion phase",
                ));
            }
            let attempt = graph_build::expanded(
                resource.verify()?,
                resource.graph.as_ref().unwrap(),
                *parent,
            )?;
            let value = attempt.new_nodes[0].unwrap();
            self.apply_graph_build(scope, attempt, Some(parent.key()))?;
            Ok(value)
        })();
        self.finish(result)
    }

    fn apply_graph_build(
        &mut self,
        scope: &GraphScope,
        attempt: GraphAttempt,
        parent: Option<GraphNodeKey>,
    ) -> StorageResult<GraphBuildAck> {
        self.graph_resource(scope)?
            .namespace_graph_growth(attempt.proposed.nodes(), attempt.proposed.edges())?;
        let ack = build_ack(scope, &attempt, parent)?;
        let resource = self.graph_resource_mut(scope)?;
        resource.verify()?;
        resource.graph.as_mut().unwrap().attempt = Some(attempt);
        resource.native.reserve()?;
        let graph = resource.graph.as_ref().unwrap();
        graph_build::commit(
            resource.connection.as_ref().unwrap(),
            resource.engine,
            graph,
            graph.attempt.as_ref().unwrap(),
        )?;
        resource.native.observe_allocation()?;
        let graph = resource.graph.as_mut().unwrap();
        let attempt = graph.attempt.take().unwrap();
        graph.stage = attempt.proposed_stage;
        graph.totals = attempt.proposed;
        graph.seed_count = attempt.seed_count;
        graph.maximum_node = attempt.maximum_node;
        graph.maximum_edge = attempt.maximum_edge;
        Ok(ack)
    }
}

pub(crate) fn build_ack(
    scope: &GraphScope,
    attempt: &GraphAttempt,
    parent: Option<GraphNodeKey>,
) -> StorageResult<GraphBuildAck> {
    let memory = attempt.memory.memory().reserve(
        std::mem::size_of::<GraphBuildAck>()
            + attempt.nodes * std::mem::size_of::<GraphNodeChange>()
            + attempt.edges * std::mem::size_of::<GraphEdgeChange>(),
    )?;
    let mut nodes = graph_index::window(attempt.nodes)?;
    let mut edges = graph_index::window(attempt.edges)?;
    for index in 0..attempt.nodes {
        nodes.push(GraphNodeChange::new(
            scope,
            attempt.old_nodes[index],
            attempt.new_nodes[index].unwrap(),
        )?);
    }
    for index in 0..attempt.edges {
        edges.push(GraphEdgeChange::new(
            scope,
            attempt.old_edges[index],
            attempt.new_edges[index].unwrap(),
        )?);
    }
    nodes.sort_unstable_by_key(|row| row.after().key());
    edges.sort_unstable_by_key(|row| row.after().key());
    let parent = parent.and_then(|key| {
        nodes
            .iter()
            .find(|row| row.after().key() == key)
            .map(|row| row.after())
    });
    Ok(GraphBuildAck::new(
        scope.clone(),
        attempt.prior,
        attempt.proposed,
        nodes,
        edges,
        parent,
    )?
    .with_memory(memory)?)
}
