//! One known-clean same-descriptor rebind attempt, with exact Unknown custody.
use super::{
    plan::Plan,
    profile, rebind_index,
    rebind_state::{Attempt, Phase},
    ScratchSession,
};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{GraphSubject, StateSelection};
impl ScratchSession {
    pub(crate) fn rebind(
        &mut self,
        selection: StateSelection,
        plan: Plan,
        subject: Option<GraphSubject>,
    ) -> StorageResult<()> {
        let result = (|| {
            let r = self
                .resource
                .as_mut()
                .ok_or(StorageError::Integrity("scratch rebind released owner"))?;
            r.check_engine()?;
            r.pool_ready(0)?;
            r.pool_attempt = Some(Attempt::prepare(r, selection, plan, subject)?);
            r.native.reserve()?;
            let c = r.connection.as_ref().unwrap();
            crate::sqlite::write::begin_immediate(c)?;
            let change = (|| {
                if r.plan.version() == 6 {
                    super::pool_draft::verify_transaction(c, r)?;
                } else {
                    super::root_reset::verify_transaction(c, r)?;
                }
                rebind_index::apply(c, r, &r.pool_attempt.as_ref().unwrap().proposed)
            })();
            profile::finish_write_guarded(c, change, r.engine)?;
            r.pool_attempt.as_mut().unwrap().phase = Phase::CommitKnown;
            r.native.observe_allocation()?;
            r.pool_attempt.as_mut().unwrap().phase = Phase::NativeKnown;
            rebind_index::verify(
                r.connection.as_ref().unwrap(),
                &r.pool_attempt.as_ref().unwrap().proposed,
            )?;
            let Attempt {
                proposed, memory, ..
            } = r.pool_attempt.take().unwrap();
            let context = *proposed;
            context.apply(r);
            drop(memory);
            r.verify()?;
            Ok(())
        })();
        self.finish(result)
    }
}
