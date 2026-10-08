//! Owned semantic continuation, independent of the caller's executor.
use crate::{
    BaseFacts, JobOutcome, NamespaceJob, Need, Operation, Outcome, SourceView, Time, Workspace,
    WorkspaceError, WorkspaceResult,
};
use layerfs_content::{filesystem::identity::MAXIMUM_INODE_SERIAL, ContentError};
use layerfs_overlay::OverlayError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationStage {
    Owner,
    Base,
    Finished,
}

/// Inputs and facts for exactly one mutation. Source ownership remains with
/// the caller until this plan and all result/reply consumers have finished.
/// An error finishes the plan; no later step can replay the failed operation.
pub struct MutationPlan {
    job: NamespaceJob,
    needs: Vec<Need>,
    stage: MutationStage,
}

/// Preparation has no effects and returns the exact owned operation on refusal.
#[derive(Debug)]
pub struct MutationInputFailure {
    pub error: WorkspaceError,
    pub operation: Operation,
}

impl Workspace {
    /// Prepare without SQL, provider I/O or serial allocation. For a creating
    /// operation, the caller supplies one serial from this scope's allocator.
    /// Native callers reserve it in a separately admitted service step.
    pub fn prepare_mutation(
        &self,
        view: &SourceView,
        operation: Operation,
        now: Time,
        serial: Option<u64>,
    ) -> Result<MutationPlan, Box<MutationInputFailure>> {
        let error = if view.source().route() != self.route() {
            Some(OverlayError::Stale.into())
        } else if operation.creates() != serial.is_some()
            || serial.is_some_and(|serial| serial == 0 || serial > MAXIMUM_INODE_SERIAL)
        {
            Some(ContentError::InvalidRecord("mutation reserved serial").into())
        } else {
            None
        };
        if let Some(error) = error {
            return Err(Box::new(MutationInputFailure { error, operation }));
        }
        Ok(MutationPlan {
            job: NamespaceJob {
                source: view.source(),
                root: view.root_serial(),
                operation,
                now,
                serial,
                facts: BaseFacts::default(),
            },
            needs: Vec::new(),
            stage: MutationStage::Owner,
        })
    }
}
impl MutationPlan {
    pub const fn stage(&self) -> MutationStage {
        self.stage
    }
    /// The next deciding job is available only before its outcome is accepted.
    /// The service must preserve its original pending handle across a park.
    pub fn job(&self) -> Option<&NamespaceJob> {
        (self.stage == MutationStage::Owner).then_some(&self.job)
    }
    /// Original input remains available after success or failure for custody.
    pub const fn operation(&self) -> &Operation {
        &self.job.operation
    }
    /// Heap retained by the semantic plan, distinct from service job credits.
    pub fn charge(&self) -> usize {
        self.job.charge() + self.needs.capacity() * (std::mem::size_of::<Need>() + 255)
    }
    /// Accept exactly the original owner result. Only a nonempty Needs result
    /// permits another round; attempted failures and publication are terminal.
    pub fn accept(
        &mut self,
        result: WorkspaceResult<JobOutcome>,
    ) -> WorkspaceResult<Option<Outcome>> {
        let previous = std::mem::replace(&mut self.stage, MutationStage::Finished);
        if previous != MutationStage::Owner {
            return Err(ContentError::InvalidRecord("mutation owner stage").into());
        }
        match result? {
            JobOutcome::Applied { publication, inode } => Ok(Some(Outcome::Applied {
                publication,
                stat: inode.map(Into::into),
            })),
            JobOutcome::Unchanged { inode } => Ok(Some(Outcome::Unchanged {
                stat: inode.map(Into::into),
            })),
            JobOutcome::Refused(refusal) => Err(WorkspaceError::Refused(refusal)),
            JobOutcome::Needs(needs) => {
                if needs.is_empty() {
                    return Err(ContentError::InvalidRecord("empty namespace needs").into());
                }
                self.needs = needs;
                self.stage = MutationStage::Base;
                Ok(None)
            }
        }
    }
    /// Fetch requested immutable facts only with admitted provider capacity.
    /// No SQL job is called here. The exact source must still be owned. Failure
    /// leaves all accumulated facts, needs and original input in this plan.
    pub fn supply(&mut self, view: &SourceView) -> WorkspaceResult<()> {
        let previous = std::mem::replace(&mut self.stage, MutationStage::Finished);
        if previous != MutationStage::Base {
            return Err(ContentError::InvalidRecord("mutation base stage").into());
        }
        if view.source() != self.job.source {
            return Err(OverlayError::Stale.into());
        }
        view.supply(
            &mut self.job.facts,
            &self.needs,
            self.job.operation.destination_path(),
        )?;
        self.needs.clear();
        self.stage = MutationStage::Owner;
        Ok(())
    }
}
