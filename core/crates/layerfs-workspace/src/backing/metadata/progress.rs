//! Charged completion credit shared by saved results and active C5 pages.
use super::{MetadataCharge, MetadataHost, ESCROW};
use crate::WorkspaceError;
use std::sync::{Arc, Mutex, Weak};

pub struct ProgressFund {
    pub host: Weak<MetadataHost>,
    available: Mutex<FundState>,
    generation: Mutex<Option<u64>>,
    _charge: MetadataCharge,
}
struct FundState {
    bytes: u64,
    finished: bool,
}
impl ProgressFund {
    pub fn new(host: &Arc<MetadataHost>) -> Result<Arc<Self>, WorkspaceError> {
        Ok(Arc::new(Self {
            host: Arc::downgrade(host),
            available: Mutex::new(FundState {
                bytes: 0,
                finished: false,
            }),
            generation: Mutex::new(None),
            _charge: host.memory(256)?,
        }))
    }
    pub(crate) fn reserved(
        host: &Arc<MetadataHost>,
        generation: u64,
    ) -> Result<Arc<Self>, WorkspaceError> {
        let fund = Self::new(host)?;
        host.reserve(ESCROW)?;
        if let Err(error) = fund.install(CompletionReserve {
            generation,
            bytes: ESCROW,
        }) {
            host.release(0, ESCROW)?;
            return Err(error);
        }
        Ok(fund)
    }
    pub(crate) fn untouched(&self, generation: u64) -> Result<bool, WorkspaceError> {
        let owner = self.generation.lock().map_err(|_| WorkspaceError::Io)?;
        let available = self.available.lock().map_err(|_| WorkspaceError::Io)?;
        Ok(*owner == Some(generation) && available.bytes == ESCROW && !available.finished)
    }
    pub fn install(&self, reserve: CompletionReserve) -> Result<(), WorkspaceError> {
        let mut generation = self.generation.lock().map_err(|_| WorkspaceError::Io)?;
        if generation.is_some() {
            return Err(WorkspaceError::Io);
        }
        *generation = Some(reserve.generation);
        self.available.lock().map_err(|_| WorkspaceError::Io)?.bytes = reserve.bytes;
        Ok(())
    }
    pub fn take(&self, bytes: u64) -> Result<(), WorkspaceError> {
        let mut available = self.available.lock().map_err(|_| WorkspaceError::Io)?;
        if available.finished {
            return Err(WorkspaceError::Busy);
        }
        available.bytes = available
            .bytes
            .checked_sub(bytes)
            .ok_or(WorkspaceError::Capacity)?;
        Ok(())
    }
    pub fn give(&self, bytes: u64) -> Result<(), WorkspaceError> {
        let mut available = self.available.lock().map_err(|_| WorkspaceError::Io)?;
        if available.finished {
            return self
                .host
                .upgrade()
                .ok_or(WorkspaceError::Closed)?
                .release(0, bytes);
        }
        available.bytes = available
            .bytes
            .checked_add(bytes)
            .ok_or(WorkspaceError::Io)?;
        Ok(())
    }
    pub fn finish(&self) -> Result<(), WorkspaceError> {
        let mut available = self.available.lock().map_err(|_| WorkspaceError::Io)?;
        if available.finished {
            return Err(WorkspaceError::Busy);
        }
        self.host
            .upgrade()
            .ok_or(WorkspaceError::Closed)?
            .release(0, available.bytes)?;
        available.bytes = 0;
        available.finished = true;
        Ok(())
    }
    pub fn recycle(&self, bytes: u64) -> Result<(), WorkspaceError> {
        let host = self.host.upgrade().ok_or(WorkspaceError::Closed)?;
        let mut available = self.available.lock().map_err(|_| WorkspaceError::Io)?;
        if available.finished {
            return host.release(bytes, 0);
        }
        let mut state = host.payloads.state.lock().map_err(|_| WorkspaceError::Io)?;
        state.allocated = state
            .allocated
            .checked_sub(bytes)
            .ok_or(WorkspaceError::Io)?;
        state.metadata_allocated = state
            .metadata_allocated
            .checked_sub(bytes)
            .ok_or(WorkspaceError::Io)?;
        state.reserved = state
            .reserved
            .checked_add(bytes)
            .ok_or(WorkspaceError::Io)?;
        state.metadata_reserved = state
            .metadata_reserved
            .checked_add(bytes)
            .ok_or(WorkspaceError::Io)?;
        available.bytes = available
            .bytes
            .checked_add(bytes)
            .ok_or(WorkspaceError::Io)?;
        Ok(())
    }
}
impl Drop for ProgressFund {
    fn drop(&mut self) {
        if let (Some(host), Ok(available)) = (self.host.upgrade(), self.available.get_mut()) {
            let _ = host.release(0, available.bytes);
        }
    }
}
pub struct CompletionReserve {
    pub generation: u64,
    pub bytes: u64,
}
