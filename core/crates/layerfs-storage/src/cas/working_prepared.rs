//! Pre-birth native/codec/index owners, retained intact for an uncertain birth.

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::encoding::delta::candidates::Candidates;
use crate::encoding::pool::PoolIndex;
use crate::encoding::{CompressionWorkspace, DecompressionWorkspace};
use crate::error::StorageResult;

use super::working::{IndexLease, SaveLease};

pub(crate) struct PreparedSave {
    pub(crate) connection: Connection,
    pub(crate) engine: Option<&'static crate::engine::EngineGuard>,
    pub(crate) compression: Option<CompressionWorkspace>,
    pub(crate) decompression: Option<DecompressionWorkspace>,
    pub(crate) pool_index: Option<Arc<Mutex<PoolIndex>>>,
    pub(crate) candidates: Option<Arc<Mutex<Candidates>>>,
    pub(crate) published_pool: Arc<Mutex<PoolIndex>>,
    pub(crate) published_candidates: Arc<Mutex<Candidates>>,
    pub(crate) published_index_credit: Arc<IndexLease>,
    // Credit follows actual field destruction, including the native connection.
    pub(crate) working: SaveLease,
}
impl PreparedSave {
    pub(crate) fn new(
        connection: Connection,
        engine: Option<&'static crate::engine::EngineGuard>,
        published_pool: Arc<Mutex<PoolIndex>>,
        published_candidates: Arc<Mutex<Candidates>>,
        published_index_credit: Arc<IndexLease>,
        working: SaveLease,
    ) -> Self {
        Self {
            connection,
            engine,
            compression: None,
            decompression: None,
            pool_index: None,
            candidates: None,
            published_pool,
            published_candidates,
            published_index_credit,
            working,
        }
    }
    pub(crate) fn codecs(&mut self) -> StorageResult<()> {
        self.compression = Some(CompressionWorkspace::new()?);
        self.decompression = Some(DecompressionWorkspace::new()?);
        Ok(())
    }
}
