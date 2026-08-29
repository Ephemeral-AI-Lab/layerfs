use layerfs_storage_core::{Result, StorageError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourcePolicy {
    pub max_spool_bytes: u64,
}

impl Default for ResourcePolicy {
    fn default() -> Self {
        Self {
            max_spool_bytes: 1024 * 1024 * 1024,
        }
    }
}

impl ResourcePolicy {
    pub(crate) fn check(self, spool_bytes: u64) -> Result<()> {
        if spool_bytes <= self.max_spool_bytes {
            Ok(())
        } else {
            Err(StorageError::InvalidInput("workspace spool limit"))
        }
    }
}
