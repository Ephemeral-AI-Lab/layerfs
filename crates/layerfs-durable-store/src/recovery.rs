use crate::{DurableStore, RequestId, Result};
use layerfs_storage::{EngineError, FullStorage};
use std::time::{SystemTime, UNIX_EPOCH};

const ABANDONED_SYNC_SECONDS: u64 = 24 * 60 * 60;
const STARTUP_SYNC_REAP_LIMIT: usize = 64;

pub(crate) fn reap_startup_abandoned_sync(storage: &FullStorage) -> Result<()> {
    let cutoff = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| std::io::Error::other(error.to_string()))?
        .as_secs()
        .saturating_sub(ABANDONED_SYNC_SECONDS);
    let cutoff = i64::try_from(cutoff).map_err(|_| EngineError::CounterOverflow)?;
    for _ in 0..STARTUP_SYNC_REAP_LIMIT {
        if storage.reap_one_abandoned_sync(cutoff)?.is_none() {
            break;
        }
    }
    Ok(())
}

impl DurableStore {
    pub fn abort_sync_transfer(&self, owner: RequestId, direction: &str) -> Result<u64> {
        Ok(self.storage.abort_sync_transfer(owner, direction)?)
    }

    pub fn reap_one_abandoned_sync(
        &self,
        older_than_unix_seconds: i64,
    ) -> Result<Option<(RequestId, String, u64)>> {
        Ok(self
            .storage
            .reap_one_abandoned_sync(older_than_unix_seconds)?)
    }

    pub fn sync_custody_rows(&self, owner: RequestId, direction: &str) -> Result<u64> {
        Ok(self.storage.sync_custody_rows(owner, direction)?)
    }
}
