//! Fixed source-bound sites/root phases and explicit Rust1.85 delegation.
use super::{BindingSiteState, IndexedState, SiteScope, StateScope, StateSelection, StateTable};
use crate::error::ContentResult;
use crate::filesystem::rows::BindingSourceId;
/// The two closed capabilities of the new actual construction owner.
pub trait SiteConstructionState: IndexedState + BindingSiteState {
    /// Explicit delegation, without trait upcasting.
    fn indexed(&mut self) -> &mut dyn IndexedState;
}
impl<T: IndexedState + BindingSiteState> SiteConstructionState for T {
    fn indexed(&mut self) -> &mut dyn IndexedState {
        self
    }
}
/// Same live native owner, sites1/table3/source then roots2/table1.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SiteConstructionScopes {
    sites: SiteScope,
    roots: StateScope,
}
impl SiteConstructionScopes {
    /// Requires already bound native selection and actual opaque prepared source.
    pub fn new(selection: StateSelection, source: BindingSourceId) -> ContentResult<Self> {
        Ok(Self {
            sites: SiteScope::new(
                StateScope::new(selection.clone(), 1, StateTable::BindingSites)?,
                source,
            )?,
            roots: StateScope::new(selection, 2, StateTable::DirectoryRoots)?,
        })
    }
    /// Exact immutable source association for sites and facts.
    pub fn sites(&self) -> &SiteScope {
        &self.sites
    }
    /// Root phase becomes available only after confirmed site retirement.
    pub fn roots(&self) -> &StateScope {
        &self.roots
    }
}
