//! Operation driver: owner rounds over immutable facts, then one publication.
use crate::{
    InodeSerials, Operation, Outcome, OverlayJobs, SourceView, Time, Workspace, WorkspaceResult,
};
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
        let mut plan = self
            .prepare_mutation(view, operation, now, serial)
            .map_err(|failure| failure.error)?;
        loop {
            let result = overlay.namespace(plan.job().expect("prepared owner stage"));
            if let Some(outcome) = plan.accept(result)? {
                return Ok(outcome);
            }
            plan.supply(view)?;
        }
    }
}
