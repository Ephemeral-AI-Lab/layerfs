//! Supplied whole namespace construction with independent narrow canonical roles.
use super::{CountState, NamespaceConstructionState, ReleaseState};
/// Profile8 composes real namespace/count/release authorities on one selected owner.
pub trait CanonicalConstructionState:
    NamespaceConstructionState + CountState + ReleaseState
{
}
impl<T: NamespaceConstructionState + CountState + ReleaseState> CanonicalConstructionState for T {}
