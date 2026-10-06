//! Exact bound history attempts; no transport disconnect implies publication.
use crate::runtime::{Binding, CompletionPhase, RuntimeError, RuntimeResult, SaveId, Sessions};
use layerfs_content::{FilesystemRoot, ObjectId};
use layerfs_history::error::StageDisposition;
use layerfs_history::{
    CommitStagedOutcome, CommitStagedRequest, DiscardOutcome, DiscardRequest, HistoryCatalog,
    StageRecord, StageRequest,
};

/// Bounded history receipts attached to one finished Save capability.
/// Reads inspect retained results; they never repeat an attempted operation.
#[derive(Debug, Default)]
pub struct HistoryReceipts {
    stage: Option<RuntimeResult<StageRecord>>,
    commit: Option<RuntimeResult<CommitStagedOutcome>>,
    discard: Option<RuntimeResult<DiscardOutcome>>,
}
impl HistoryReceipts {
    /// Stage admission/provider result, absent until an attempt starts.
    pub fn stage(&self) -> Option<&RuntimeResult<StageRecord>> {
        self.stage.as_ref()
    }
    /// Exact conditional transition result, absent until attempted.
    pub fn commit(&self) -> Option<&RuntimeResult<CommitStagedOutcome>> {
        self.commit.as_ref()
    }
    /// Exact-token discard result, absent until explicitly attempted.
    pub fn discard(&self) -> Option<&RuntimeResult<DiscardOutcome>> {
        self.discard.as_ref()
    }
    pub(crate) fn retains_custody(&self) -> bool {
        fn unknown<T>(attempt: &Option<RuntimeResult<T>>) -> bool {
            matches!(attempt, Some(Err(RuntimeError::History(error))) if error.unknown())
        }
        if unknown(&self.stage) || unknown(&self.commit) || unknown(&self.discard) {
            return true;
        }
        if self.commit.as_ref().is_some_and(Result::is_ok)
            || self.discard.as_ref().is_some_and(Result::is_ok)
        {
            return false;
        }
        if let Some(Err(RuntimeError::History(layerfs_history::HistoryError::WithStage {
            stage: StageDisposition::Absent(_),
            ..
        }))) = &self.commit
        {
            return false;
        }
        self.stage.as_ref().is_some_and(Result::is_ok)
    }
}
impl Sessions<'_> {
    /// Reads the exact retained history results under fresh authority validation.
    /// An absent receipt is not an unfenced provider/history absence claim.
    pub fn history_receipts(
        &self,
        binding: &Binding,
        save: SaveId,
    ) -> RuntimeResult<&HistoryReceipts> {
        self.check_binding(binding)?;
        Ok(&self.slot(binding, save)?.history)
    }
    /// One saved candidate admission and stage attempt. All authority/expectation
    /// fields come from the bound snapshot; callers supply only root/generation.
    /// SaveFinish must already have succeeded. This checks the saved root's
    /// grammar/scope/profile; full contextual topology validation remains the
    /// owning content construction obligation.
    pub fn stage_saved(
        &mut self,
        binding: &Binding,
        save: SaveId,
        root: ObjectId,
        generation: u64,
    ) -> RuntimeResult<&RuntimeResult<StageRecord>> {
        self.check_binding(binding)?;
        let slot = self.slot(binding, save)?;
        if slot.history.stage.is_some() {
            return Err(RuntimeError::AlreadyAttempted);
        }
        let completion = slot
            .completion
            .as_ref()
            .ok_or(RuntimeError::Invalid("stage before SaveFinish"))?;
        if completion.phase() != CompletionPhase::Finish || completion.outcome().is_err() {
            return Err(RuntimeError::Invalid(
                "stage requires successful SaveFinish",
            ));
        }
        // Session-slot iteration is bounded by configured admission, not files,
        // edits or Commit count. Do not create two local owners of one stage.
        if self.slots.iter().any(|other| {
            other
                .binding
                .as_ref()
                .is_some_and(|b| b.workspace == binding.workspace)
                && other.history.retains_custody()
        }) {
            return Err(RuntimeError::RetainedCustody);
        }
        let result = self.admit_stage(binding, root, generation);
        self.slots[save.slot].history.stage = Some(result);
        Ok(self.slots[save.slot]
            .history
            .stage
            .as_ref()
            .expect("stage receipt retained"))
    }
    fn admit_stage(
        &self,
        binding: &Binding,
        candidate_root: ObjectId,
        generation: u64,
    ) -> RuntimeResult<StageRecord> {
        if generation > i64::MAX as u64 {
            return Err(RuntimeError::Invalid("history generation"));
        }
        struct RootReply(Option<FilesystemRoot>);
        impl crate::ObjectReply for RootReply {
            fn object(&mut self, _: ObjectId, canonical: &[u8]) -> RuntimeResult<()> {
                self.0 = Some(FilesystemRoot::decode(canonical)?);
                Ok(())
            }
        }
        let mut reply = RootReply(None);
        self.read_objects(binding, None, &[candidate_root], &mut reply)?;
        let root = reply.0.ok_or(RuntimeError::Invalid("candidate demand"))?;
        if root.scope().object() != binding.snapshot.scope
            || root.profile() != binding.snapshot.profile
        {
            return Err(RuntimeError::Invalid("candidate scope/profile"));
        }
        let snapshot = &binding.snapshot;
        Ok(self.history.stage_changes(&StageRequest {
            workspace: binding.workspace,
            branch: snapshot.branch.id,
            expected_head: snapshot.branch.head_commit,
            expected_base: snapshot.branch.base_layer,
            expected_root: snapshot.effective_root,
            construction_base_root: snapshot.effective_root,
            intended_commit_base: snapshot.branch.base_layer,
            candidate_root,
            scope: snapshot.scope,
            profile: snapshot.profile,
            generation,
        })?)
    }
    /// Attempts the exact acknowledged stage once. A conflict retains the
    /// provider's deciding-transaction stage disposition; unknown stays terminal.
    pub fn commit_saved(
        &mut self,
        binding: &Binding,
        save: SaveId,
    ) -> RuntimeResult<&RuntimeResult<CommitStagedOutcome>> {
        self.check_binding(binding)?;
        let history = &self.slot(binding, save)?.history;
        if history.commit.is_some() || history.discard.is_some() {
            return Err(RuntimeError::AlreadyAttempted);
        }
        let stage = history
            .stage
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .ok_or(RuntimeError::Invalid("no acknowledged stage"))?;
        let request = CommitStagedRequest {
            workspace: stage.workspace,
            token: stage.token,
        };
        let result = self.history.commit_staged(&request).map_err(Into::into);
        self.slots[save.slot].history.commit = Some(result);
        Ok(self.slots[save.slot]
            .history
            .commit
            .as_ref()
            .expect("transition receipt retained"))
    }
    /// Explicit exact-token discard of a known stage. A lost/unknown transition
    /// or discard cannot be resent or guessed from a later provider read.
    pub fn discard_saved(
        &mut self,
        binding: &Binding,
        save: SaveId,
    ) -> RuntimeResult<&RuntimeResult<DiscardOutcome>> {
        self.check_binding(binding)?;
        let history = &self.slot(binding, save)?.history;
        if history.discard.is_some() {
            return Err(RuntimeError::AlreadyAttempted);
        }
        if history.commit.as_ref().is_some_and(|r| match r {
            Ok(_) => true,
            Err(RuntimeError::History(e)) => e.unknown(),
            _ => false,
        }) {
            return Err(RuntimeError::RetainedCustody);
        }
        let stage = history
            .stage
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .ok_or(RuntimeError::Invalid("no acknowledged stage"))?;
        let request = DiscardRequest {
            workspace: stage.workspace,
            token: stage.token,
        };
        let result = self.history.discard_stage(&request).map_err(Into::into);
        self.slots[save.slot].history.discard = Some(result);
        Ok(self.slots[save.slot]
            .history
            .discard
            .as_ref()
            .expect("discard receipt retained"))
    }
}
