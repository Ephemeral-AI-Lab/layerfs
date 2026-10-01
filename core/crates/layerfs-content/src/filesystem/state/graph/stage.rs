//! Frozen private lifecycle codes, separate from physical/Unknown custody.
use crate::error::{ContentError, ContentResult};
/// Selected stage of one graph owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphStage {
    /// Awaiting known Sites retirement.
    Deferred = 0,
    /// Update seed enrollment, before first expansion selection.
    Seeding = 1,
    /// One native unexpanded-node frontier.
    Expanding = 2,
    /// Immutable acknowledged adjacency.
    AdjacencySealed = 3,
    /// External iterative SCC state.
    Solving = 4,
    /// Known complete selected semantic proof.
    Proved = 5,
    /// Exact bounded key retirement underway.
    Retiring = 6,
    /// Known empty/observed graph enables roots.
    Retired = 7,
    /// Known semantic cyclic-seed refusal; no dependent work.
    Rejected = 8,
}
impl GraphStage {
    /// Private persisted code.
    pub const fn code(self) -> u8 {
        self as u8
    }
    /// Refuses every unallocated stage.
    pub fn from_code(code: u8) -> ContentResult<Self> {
        match code {
            0 => Ok(Self::Deferred),
            1 => Ok(Self::Seeding),
            2 => Ok(Self::Expanding),
            3 => Ok(Self::AdjacencySealed),
            4 => Ok(Self::Solving),
            5 => Ok(Self::Proved),
            6 => Ok(Self::Retiring),
            7 => Ok(Self::Retired),
            8 => Ok(Self::Rejected),
            _ => Err(ContentError::InvalidOrderingRecord("graph stage")),
        }
    }
}
