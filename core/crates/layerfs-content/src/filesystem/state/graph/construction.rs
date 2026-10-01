//! Same native owner through Sites1, Graph2 and Roots3.
use super::{EffectiveGraphState, GraphScope, GraphSubject};
use crate::error::ContentResult;
use crate::filesystem::state::{
    BindingSiteState, IndexedState, SiteScope, StateScope, StateSelection, StateTable,
};
/// Exact scopes of the complete new construction lane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphConstructionScopes {
    sites: SiteScope,
    graph: GraphScope,
    roots: StateScope,
}
impl GraphConstructionScopes {
    /// Requires bound selection and the pre-admitted static graph subject.
    pub fn new(selection: StateSelection, subject: GraphSubject) -> ContentResult<Self> {
        Ok(Self {
            sites: SiteScope::new(
                StateScope::new(selection.clone(), 1, StateTable::BindingSites)?,
                subject.source_id(),
            )?,
            graph: GraphScope::new(selection.clone(), subject)?,
            roots: StateScope::new(selection, 3, StateTable::DirectoryRoots)?,
        })
    }
    /// Sites retain their original source-bound phase1.
    pub fn sites(&self) -> &SiteScope {
        &self.sites
    }
    /// Effective graph under the exact selected static context.
    pub fn graph(&self) -> &GraphScope {
        &self.graph
    }
    /// Roots are enabled only after known graph retirement.
    pub fn roots(&self) -> &StateScope {
        &self.roots
    }
}
/// Common construction capabilities sharing one admitted owner.
pub trait GraphConstructionState: IndexedState + BindingSiteState + EffectiveGraphState {
    /// Explicit delegation without Rust1.85 trait upcasting.
    fn indexed(&mut self) -> &mut dyn IndexedState;
}
impl<T: IndexedState + BindingSiteState + EffectiveGraphState> GraphConstructionState for T {
    fn indexed(&mut self) -> &mut dyn IndexedState {
        self
    }
}
