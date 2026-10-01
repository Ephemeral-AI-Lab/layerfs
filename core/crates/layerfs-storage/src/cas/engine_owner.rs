//! Same captured engine borrow at actual native transaction/wave boundaries.
use super::owner::MutationOwner;
use crate::{engine::ConnectionClass, sqlite::connection, StorageError, StorageResult};
impl MutationOwner {
    pub(super) fn validate_engine(&mut self) -> StorageResult<()> {
        let Some(engine) = self.engine else {
            return Ok(());
        };
        match connection::verify_guarded(&self.connection, engine, ConnectionClass::ContentWrite) {
            Ok(()) => Ok(()),
            Err(original) if self.transaction_open => {
                // No COMMIT/rollback/refund after the required boundary failed.
                // Existing Save quarantine retains the complete native/codec owner.
                self.quarantine();
                Err(StorageError::UnknownOutcome {
                    original: Box::new(original),
                })
            }
            Err(original) => {
                // The operation refusal is known, but this acknowledged Save's
                // cleanup is not authorized. Preserve its last native owner.
                self.terminal = true;
                self.cleanup_attempted = true;
                self.cleanup_completed = false;
                Err(original)
            }
        }
    }
}
