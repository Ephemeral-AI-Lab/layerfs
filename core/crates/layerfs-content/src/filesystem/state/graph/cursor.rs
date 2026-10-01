//! Independent exact ordered-stream verification with sticky count/digest EOF.
use super::{
    GraphAdjacencySeal, GraphEdgePage, GraphMode, GraphNodeKey, GraphNodeLedger, GraphNodePage,
    GraphPageLimit, GraphProofPage, GraphProofSeal,
};
use crate::error::{ContentError, ContentResult};
/// Complete adjacency page fold, independent of provider terminal assertions.
pub struct GraphNodeCursor {
    seal: GraphAdjacencySeal,
    ledger: GraphNodeLedger,
    ended: bool,
    failure: Option<ContentError>,
}
impl GraphNodeCursor {
    /// Starts at the exact selected stream beginning.
    pub fn new(seal: GraphAdjacencySeal) -> Self {
        let ledger = GraphNodeLedger::adjacency(seal.scope().clone());
        Self {
            seal,
            ledger,
            ended: false,
            failure: None,
        }
    }
    /// Actual prior key for the next provider page.
    pub fn after(&self) -> Option<GraphNodeKey> {
        self.ledger.last()
    }
    /// Exact seen count.
    pub fn records(&self) -> u64 {
        self.ledger.records()
    }
    /// True only after complete count/digest EOF.
    pub const fn finished(&self) -> bool {
        self.ended
    }
    /// Charges no provider work; validates an already acquired exact page once.
    pub fn accept(&mut self, page: &GraphNodePage, limit: GraphPageLimit) -> ContentResult<()> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = self.fold(page, limit);
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }
    fn fold(&mut self, page: &GraphNodePage, limit: GraphPageLimit) -> ContentResult<()> {
        if self.ended || page.seal() != &self.seal {
            return Err(bad("graph node cursor seal/EOF"));
        }
        page.check_limit(limit)?;
        self.ledger.acknowledge(page.records())?;
        if page.last() != self.ledger.last()
            || self.ledger.records() > self.seal.nodes()
            || page.eof() != (self.ledger.records() == self.seal.nodes())
        {
            return Err(bad("graph node cursor count/EOF"));
        }
        if page.eof() {
            if self.ledger.digest() != *self.seal.node_digest() {
                return Err(bad("graph node cursor digest"));
            }
            self.ended = true;
        }
        Ok(())
    }
}
/// Exact selected-parent continuation; the caller folds complete arcs globally.
pub struct GraphEdgeCursor {
    seal: GraphAdjacencySeal,
    parent: u64,
    last: Option<u64>,
    maximum: Option<Option<u64>>,
    ended: bool,
    failure: Option<ContentError>,
}
impl GraphEdgeCursor {
    /// One positive selected parent, starting before its first arc.
    pub fn new(seal: GraphAdjacencySeal, parent: u64) -> ContentResult<Self> {
        super::GraphNodeKey::new(seal.scope(), parent)?;
        Ok(Self {
            seal,
            parent,
            last: None,
            maximum: None,
            ended: false,
            failure: None,
        })
    }
    /// Actual prior selected child.
    pub const fn after(&self) -> Option<u64> {
        self.last
    }
    /// Complete selected-parent EOF received.
    pub const fn finished(&self) -> bool {
        self.ended
    }
    /// Verifies exact parent/MAX/order/progress on one current page.
    pub fn accept(&mut self, page: &GraphEdgePage, limit: GraphPageLimit) -> ContentResult<()> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = self.fold(page, limit);
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }
    fn fold(&mut self, page: &GraphEdgePage, limit: GraphPageLimit) -> ContentResult<()> {
        if self.ended
            || page.seal() != &self.seal
            || page.parent() != self.parent
            || self.maximum.is_some_and(|max| max != page.maximum())
        {
            return Err(bad("graph edge cursor selection"));
        }
        page.check_limit(limit)?;
        if page
            .records()
            .first()
            .is_some_and(|edge| self.last.is_some_and(|last| edge.key().child() <= last))
            || page.records().is_empty() && page.last() != self.last
        {
            return Err(bad("graph edge cursor after"));
        }
        self.last = page.last();
        self.maximum = Some(page.maximum());
        self.ended = page.eof();
        Ok(())
    }
}
/// Full terminal proof and immutable projection folded together, without rows.
pub struct GraphProofCursor {
    seal: GraphProofSeal,
    full: GraphNodeLedger,
    projection: GraphNodeLedger,
    ended: bool,
    failure: Option<ContentError>,
}
impl GraphProofCursor {
    /// Independent full and normalized transcript owners for this exact proof.
    pub fn new(seal: GraphProofSeal) -> Self {
        let full = GraphNodeLedger::proof(seal.scope().clone());
        let projection = GraphNodeLedger::adjacency(seal.scope().clone());
        Self {
            seal,
            full,
            projection,
            ended: false,
            failure: None,
        }
    }
    /// Actual prior terminal key.
    pub fn after(&self) -> Option<GraphNodeKey> {
        self.full.last()
    }
    /// Only exact complete terminal count/digest EOF ends the stream.
    pub const fn finished(&self) -> bool {
        self.ended
    }
    /// Checks proof grammar plus full and immutable transcript before retirement.
    pub fn accept(&mut self, page: &GraphProofPage, limit: GraphPageLimit) -> ContentResult<()> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = self.fold(page, limit);
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }
    fn fold(&mut self, page: &GraphProofPage, limit: GraphPageLimit) -> ContentResult<()> {
        if self.ended || page.seal() != &self.seal {
            return Err(bad("graph proof cursor seal/EOF"));
        }
        page.check_limit(limit)?;
        for node in page.records() {
            if !node.expanded()
                || self.seal.scope().subject().mode() == GraphMode::Update
                    && (!node.completed() || node.on_stack() || !node.dfs_finished())
            {
                return Err(bad("graph proof terminal node"));
            }
            self.projection.acknowledge(&[node.projection()])?;
        }
        self.full.acknowledge(page.records())?;
        if page.last() != self.full.last()
            || self.full.records() > self.seal.nodes()
            || page.eof() != (self.full.records() == self.seal.nodes())
        {
            return Err(bad("graph proof cursor count/EOF"));
        }
        if page.eof() {
            if self.full.digest() != *self.seal.node_digest()
                || self.projection.digest() != *self.seal.adjacency().node_digest()
            {
                return Err(bad("graph proof cursor digest"));
            }
            self.ended = true;
        }
        Ok(())
    }
}
fn bad(reason: &'static str) -> ContentError {
    ContentError::InvalidOrderingRecord(reason)
}
