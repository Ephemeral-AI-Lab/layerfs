//! Supplied graph/alias/base-fact/new-parent infrastructure on one selected owner.
use super::{AliasGraphConstructionState, FactState, ParentEligibilityState};
/// Profile7's concrete roles; counts/release remain independently named later ports.
pub trait NamespaceConstructionState:
    AliasGraphConstructionState + FactState + ParentEligibilityState
{
}
impl<T: AliasGraphConstructionState + FactState + ParentEligibilityState> NamespaceConstructionState
    for T
{
}
