//! Typed final-shape authority whose real edit route has no draft-tree state.
use super::{DraftCapacity, DraftJob, DraftRecord, DraftState, DraftStats, EditRequest};
use crate::policy::{ConstructionCapacities, ConstructionPolicy, Representation};
use crate::{AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, ObjectId};
use layerfs_telemetry::timer::TimingScope;
#[derive(Clone, Copy, Eq, PartialEq)]
enum Stage {
    Issued,
    Active,
    Complete,
    Abandoned,
}
struct ZeroDrafts {
    stage: Stage,
}
/// Once-owned declaration; only the checked no-draft wrapper can use its authority.
pub struct NoDraft {
    policy: ConstructionPolicy,
    root: ObjectId,
    base_len: u64,
    final_len: u64,
    state: ZeroDrafts,
}
impl NoDraft {
    /// Capture the actual validated Empty/Whole final shape before dependent effects.
    pub fn new(
        policy: ConstructionPolicy,
        root: ObjectId,
        base_len: u64,
        final_len: u64,
    ) -> ContentResult<Self> {
        let policy = policy.validated()?;
        if policy.representation(final_len) == Representation::Chunked {
            return Err(ContentError::InvalidEdit {
                what: "no-draft final shape",
            });
        }
        Ok(Self {
            policy,
            root,
            base_len,
            final_len,
            state: ZeroDrafts {
                stage: Stage::Issued,
            },
        })
    }
    /// Common producer reached its exact terminal; no native cleanup is fabricated.
    pub fn completed(&self) -> bool {
        self.state.stage == Stage::Complete
    }
    /// A failed operation cannot replay the declaration or grow state later.
    pub fn abandon(&mut self) {
        self.state.abandon();
    }
    fn begin(
        &mut self,
        policy: ConstructionPolicy,
        request: &EditRequest<'_>,
    ) -> ContentResult<()> {
        if request.edits.is_empty() && request.edits.final_len() != request.edits.base_len() {
            self.abandon();
            return Err(ContentError::InvalidEdit {
                what: "no-draft empty sequence length",
            });
        }
        if self.state.stage != Stage::Issued
            || self.policy != policy
            || self.root != request.root
            || self.base_len != request.edits.base_len()
            || self.final_len != request.edits.final_len()
        {
            self.abandon();
            return Err(ContentError::InvalidEdit {
                what: "no-draft foreign shape/replay",
            });
        }
        self.state.stage = Stage::Active;
        Ok(())
    }
}
/// Same canonical producer with a private zero-growth authority and checked shape.
#[allow(clippy::too_many_arguments)]
pub fn apply_edits_without_drafts(
    policy: ConstructionPolicy,
    capacities: &ConstructionCapacities,
    reader: &dyn AuthenticatedObjects,
    request: EditRequest<'_>,
    consumer: &mut dyn FinalizedConsumer,
    authority: &mut NoDraft,
    scope: TimingScope<'_>,
) -> ContentResult<crate::file::content::ConstructedFile> {
    authority.begin(policy, &request)?;
    super::apply_edits_with_state(
        policy,
        capacities,
        reader,
        request,
        consumer,
        &mut authority.state,
        scope,
    )
}
fn forbidden<T>() -> ContentResult<T> {
    Err(ContentError::InvalidEdit {
        what: "no-draft tree authority",
    })
}
impl DraftState for ZeroDrafts {
    fn capacity(&self) -> ContentResult<DraftCapacity> {
        if self.stage != Stage::Active {
            return forbidden();
        }
        // Only common format validation uses this class. No tree operation can
        // access it or admit its population through the opaque NoDraft wrapper.
        Ok(DraftCapacity::default())
    }
    fn hold(&mut self, _id: ObjectId, _record: DraftRecord) -> ContentResult<()> {
        forbidden()
    }
    fn get(&mut self, _id: ObjectId) -> ContentResult<Option<DraftRecord>> {
        forbidden()
    }
    fn select_root(&mut self, _before: Option<ObjectId>, _after: ObjectId) -> ContentResult<()> {
        forbidden()
    }
    fn retain_temporaries(&mut self, _ids: &[ObjectId]) -> ContentResult<()> {
        forbidden()
    }
    fn release_temporaries(&mut self, _ids: &[ObjectId]) -> ContentResult<()> {
        forbidden()
    }
    fn supersede(&mut self, _id: ObjectId, _selected: Option<ObjectId>) -> ContentResult<()> {
        forbidden()
    }
    fn next_job(&mut self) -> ContentResult<Option<DraftJob>> {
        forbidden()
    }
    fn retire_job(&mut self, _job: DraftJob) -> ContentResult<()> {
        forbidden()
    }
    fn resolved(&mut self, _id: ObjectId) -> ContentResult<Option<ObjectId>> {
        forbidden()
    }
    fn begin_emission(&mut self, _draft: ObjectId, _canonical: ObjectId) -> ContentResult<bool> {
        forbidden()
    }
    fn accepted(&mut self, _draft: ObjectId, _canonical: ObjectId) -> ContentResult<()> {
        forbidden()
    }
    fn finish(&mut self) -> ContentResult<()> {
        if self.stage != Stage::Active {
            return forbidden();
        }
        self.stage = Stage::Complete;
        Ok(())
    }
    fn abandon(&mut self) {
        self.stage = Stage::Abandoned;
    }
    fn stats(&self) -> DraftStats {
        DraftStats::default()
    }
}
