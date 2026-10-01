//! Explicit supplied frontier profile; historical graph compatibility stays separate.
use super::{AliasFrontier, GraphConstructionState};
/// Profile5 supplies the same native owner through Sites plus alias retirement.
pub trait AliasGraphConstructionState: GraphConstructionState + AliasFrontier {}
impl<T: GraphConstructionState + AliasFrontier> AliasGraphConstructionState for T {}
