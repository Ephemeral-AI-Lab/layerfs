//! Exact selected discovery frontier and terminal/retirement acknowledgements.
use super::{AliasCapacity, AliasProgress, SiteMembership};
use crate::ContentResult;
/// One selected current serial and its most recent issued priority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AliasCurrent {
    /// Exact discovered directory.
    pub serial: u64,
    /// Sequence carried by its current fact.
    pub sequence: u64,
}
/// Exact finished frontier population; native provider matches its owned value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AliasSeal {
    /// Full selected immutable membership.
    pub members: SiteMembership,
    /// Exact directories discovered and completed once.
    pub records: u64,
    /// Last burned priority, including repeated pending discoveries.
    pub sequence: u64,
    /// Exact final serial maximum, absent only for an empty walk.
    pub maximum: Option<u64>,
}
/// Dedicated supplied authority; immutable base listings remain C1's ownership.
pub trait AliasFrontier {
    /// Same-file class admitted before the first alias mutation.
    fn alias_capacity(&self, members: &SiteMembership) -> ContentResult<AliasCapacity>;
    /// Begin once; absent root records a genuinely empty alias walk.
    fn alias_begin(&mut self, members: &SiteMembership, root: Option<u64>) -> ContentResult<()>;
    /// Ordered discovery occurrences; repeats move pending priority to the tail.
    fn alias_enqueue(&mut self, members: &SiteMembership, children: &[u64]) -> ContentResult<()>;
    /// Takes MAX priority once, or exact EOF with no current parent.
    fn alias_take(&mut self, members: &SiteMembership) -> ContentResult<Option<AliasCurrent>>;
    /// Persist exact acknowledged progress; unknown never permits resend.
    fn alias_advance(
        &mut self,
        members: &SiteMembership,
        current: AliasCurrent,
        before: &AliasProgress,
        after: &AliasProgress,
    ) -> ContentResult<()>;
    /// Complete once after the acknowledged Complete continuation.
    fn alias_complete(
        &mut self,
        members: &SiteMembership,
        current: AliasCurrent,
    ) -> ContentResult<()>;
    /// Require jobs/current empty and exact discovered/expanded equality.
    fn alias_finish(&mut self, members: &SiteMembership) -> ContentResult<AliasSeal>;
    /// Retire exact fact keys in bounded advancing transactions before Graph.
    fn alias_retire(&mut self, seal: &AliasSeal) -> ContentResult<()>;
    /// Terminal metadata only; foreign membership cannot consume another owner.
    fn alias_abandon(&mut self, members: &SiteMembership) -> ContentResult<()>;
}
