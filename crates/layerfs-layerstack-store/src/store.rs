use crate::{Result, StoreError};
use std::path::Path;

#[derive(Clone)]
pub struct LayerStackStore {
    pub(crate) db: crate::schema::StoreDb,
}

impl LayerStackStore {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            db: crate::schema::StoreDb::create(path)?,
        })
    }

    pub fn connect(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            db: crate::schema::StoreDb::connect(path)?,
        })
    }

    pub fn path(&self) -> &Path {
        self.db.path()
    }

    #[doc(hidden)]
    pub fn data_version(&self) -> Result<u64> {
        self.db.data_version()
    }

    #[doc(hidden)]
    pub fn ensure_writable(&self) -> Result<()> {
        self.db.writer().map(drop).map_err(|error| match error {
            StoreError::StoreBusy => StoreError::StoreBusy,
            error => error,
        })
    }
}
