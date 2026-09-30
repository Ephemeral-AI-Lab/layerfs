//! Fixed claim/root phase selection and explicit Rust1.85 port delegation.
use super::{BindingClaimState, IndexedState, StateScope, StateSelection, StateTable};
use crate::error::ContentResult;

/// One owner exposes only the two closed construction capabilities.
pub trait ConstructionState: IndexedState + BindingClaimState {
    /// Explicit delegation avoids a trait-upcast requirement.
    fn indexed(&mut self) -> &mut dyn IndexedState;
}
impl<T: IndexedState + BindingClaimState> ConstructionState for T {
    fn indexed(&mut self) -> &mut dyn IndexedState {
        self
    }
}

/// Fixed claims1/table2 then roots2/table1, bound to one live owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstructionScopes {
    claims: StateScope,
    roots: StateScope,
}
impl ConstructionScopes {
    /// Requires an already bound selection; neither raw bytes nor token suffice.
    pub fn new(selection: StateSelection) -> ContentResult<Self> {
        Ok(Self {
            claims: StateScope::new(selection.clone(), 1, StateTable::BindingClaims)?,
            roots: StateScope::new(selection, 2, StateTable::DirectoryRoots)?,
        })
    }
    /// Exclusive claims must retire before roots become eligible.
    pub fn claims(&self) -> &StateScope {
        &self.claims
    }
    /// Directory roots use this fixed successor phase.
    pub fn roots(&self) -> &StateScope {
        &self.roots
    }
}
