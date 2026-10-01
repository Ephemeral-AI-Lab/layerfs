//! Genuine exclusive pre-Save source/actor/window admission, transferred once.
use super::{
    small_file_source::{SmallData, SmallFileBuilder, VerifiedSmallFileRows},
    SpoolPreparation,
};
use crate::filesystem::state::{
    GraphMemory, GraphMemoryLease, GraphMode, GraphSubject, VerifiedSmallFileState,
};
use crate::{ContentError, ContentResult};
pub(super) struct ActorAdmission {
    pub(super) selector: Option<[u8; 32]>,
    pub(super) actor: GraphMemoryLease,
    pub(super) window: GraphMemoryLease,
}
/// Real actual-layout credit acquired before Stage Save and body effects.
pub struct PendingSmallFiles {
    pub(super) preparation: SpoolPreparation,
    pub(super) subject: GraphSubject,
    pub(super) memory: GraphMemory,
    pub(super) source: GraphMemoryLease,
    pub(super) actor: ActorAdmission,
}
impl PendingSmallFiles {
    /// Capture complete fixed class and the real future source/actor/helper overlap.
    pub fn new(
        selector: [u8; 32],
        subject: GraphSubject,
        preparation: SpoolPreparation,
    ) -> ContentResult<Self> {
        Self::admit(Some(selector), subject, preparation)
    }
    pub(super) fn compatibility(
        subject: GraphSubject,
        preparation: SpoolPreparation,
    ) -> ContentResult<Self> {
        Self::admit(None, subject, preparation)
    }
    fn admit(
        selector: Option<[u8; 32]>,
        subject: GraphSubject,
        preparation: SpoolPreparation,
    ) -> ContentResult<Self> {
        let d = preparation.declaration;
        if subject.source_id() != preparation.source_id()
            || subject.mode() != GraphMode::Update
            || d.directories != 0
            || d.fresh != 0
            || d.bindings != 0
            || d.wire_name_bytes != 0
            || !(1..=8).contains(&d.inodes)
        {
            return Err(ContentError::InvalidRecord("small file declaration"));
        }
        let source_bytes = std::mem::size_of::<SmallData>()
            + std::mem::size_of::<SmallFileBuilder>()
            + std::mem::size_of::<VerifiedSmallFileRows>()
            + std::mem::size_of::<Self>()
            + 4 * std::mem::size_of::<usize>();
        let actor_bytes = VerifiedSmallFileState::admitted_actor_bytes();
        let window_bytes = VerifiedSmallFileState::admitted_window_bytes();
        let memory = GraphMemory::new();
        let total = source_bytes + actor_bytes + window_bytes + GraphMemory::control_bytes();
        if total > memory.limit() {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "small file simultaneous working",
                limit: memory.limit() as u64,
                actual: total as u64,
            });
        }
        let mut credit = memory.reserve(source_bytes + actor_bytes + window_bytes)?;
        let source = credit.split(source_bytes)?;
        let actor = credit.split(actor_bytes)?;
        // Remaining exclusive credit is the prospective helper/page window.
        Ok(Self {
            preparation,
            subject,
            memory,
            source,
            actor: ActorAdmission {
                selector,
                actor,
                window: credit,
            },
        })
    }
    /// Exact captured subject; the actual immutable table is proved only during receive.
    pub fn subject(&self) -> &GraphSubject {
        &self.subject
    }
    /// Exact captured selector; compatibility sources explicitly have no Server selector.
    pub fn selector(&self) -> Option<[u8; 32]> {
        self.actor.selector
    }
    /// Current exclusive source/actor/window reservation before any dependent effect.
    pub fn working_bytes(&self) -> usize {
        self.memory.reserved_bytes()
    }
}
