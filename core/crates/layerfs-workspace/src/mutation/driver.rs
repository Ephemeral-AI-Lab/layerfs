//! Operation driver: owner rounds over immutable facts, then one publication.
use crate::{
    BaseFacts, InodeSerials, JobOutcome, NamespaceJob, Operation, Outcome, OverlayJobs, SourceView,
    Time, Workspace, WorkspaceError, WorkspaceResult,
};
use layerfs_content::ContentError;
use layerfs_overlay::OverlayError;

impl Workspace {
    /// Performs one ordinary namespace operation over an owned source window.
    ///
    /// Each owner round evaluates the operation against the rows current in
    /// that job. A round that lacks an immutable base fact publishes nothing
    /// and names the fact; it is fetched here, outside the SQL owner, and the
    /// facts only accumulate. The round that can decide either refuses with no
    /// effect or publishes every effect in one transaction. That publication is
    /// the single attempt: nothing here repeats it, and an error from it is
    /// returned as the exact outcome.
    pub fn mutate(
        &self,
        overlay: &impl OverlayJobs,
        allocator: &dyn InodeSerials,
        view: &SourceView,
        operation: Operation,
        now: Time,
    ) -> WorkspaceResult<Outcome> {
        if view.source().route() != self.route() {
            return Err(OverlayError::Stale.into());
        }
        let serial = if operation.creates() {
            Some(self.next_serial(allocator)?)
        } else {
            None
        };
        let mut job = NamespaceJob {
            source: view.source(),
            root: view.root_serial(),
            operation,
            now,
            serial,
            facts: BaseFacts::default(),
        };
        loop {
            match overlay.namespace(&job)? {
                JobOutcome::Applied { publication, inode } => {
                    return Ok(Outcome::Applied {
                        publication,
                        stat: inode.map(Into::into),
                    })
                }
                JobOutcome::Unchanged { inode } => {
                    return Ok(Outcome::Unchanged {
                        stat: inode.map(Into::into),
                    })
                }
                JobOutcome::Refused(refusal) => return Err(WorkspaceError::Refused(refusal)),
                JobOutcome::Needs(needs) => {
                    if needs.is_empty() {
                        return Err(ContentError::InvalidRecord("empty namespace needs").into());
                    }
                    view.supply(&mut job.facts, needs, job.operation.destination_path())?;
                }
            }
        }
    }
}
