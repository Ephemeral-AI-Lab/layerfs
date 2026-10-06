//! Explicit bounded reclamation of free pages in a reclaimable Store.
use super::connection::Session;
use crate::backend::{records::BackendError, Transaction};

/// Largest physical page-removal job, independent of total Store size.
pub const RECLAMATION_PAGE_LIMIT: u32 = 512;

/// One acknowledged physical maintenance job; logical objects are unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpaceReclamation {
    /// Free pages observed in the job's transaction before reclamation.
    pub free_pages_before: u64,
    /// Free pages removed by this job, at most the requested page budget.
    pub reclaimed_pages: u64,
    /// Free pages still available for a later job or ordinary inserts.
    pub remaining_free_pages: u64,
}

impl Session {
    pub(crate) fn reclaim_space(&self, pages: u32) -> Result<SpaceReclamation, BackendError> {
        if pages == 0 || pages > RECLAMATION_PAGE_LIMIT {
            return Err(BackendError::Integrity);
        }
        // Existing non-reclaimable Stores are never converted implicitly.
        if self.profile.auto_vacuum != 2 {
            return Err(BackendError::Integrity);
        }
        self.run(true, |tx| job(tx, pages))
    }
}

/// One bounded job within the caller's existing short transaction.
pub(crate) fn job(tx: &Transaction<'_>, pages: u32) -> Result<SpaceReclamation, BackendError> {
    let free = |tx: &Transaction<'_>| -> Result<u64, BackendError> {
        let value = tx.query("PRAGMA freelist_count", vec![])?;
        let count: i64 = value.first().ok_or(BackendError::Integrity)?.get(0)?;
        u64::try_from(count).map_err(|_| BackendError::Integrity)
    };
    let before = free(tx)?;
    if before != 0 {
        // SQLite moves/truncates at most this many pages. The enclosing
        // ordinary short transaction supplies the selected sync and
        // definite/uncertain outcome handling; no whole-file rebuild.
        tx.query(&format!("PRAGMA incremental_vacuum({pages})"), vec![])?;
    }
    let remaining = free(tx)?;
    let reclaimed = before
        .checked_sub(remaining)
        .filter(|count| *count <= u64::from(pages))
        .ok_or(BackendError::Integrity)?;
    Ok(SpaceReclamation {
        free_pages_before: before,
        reclaimed_pages: reclaimed,
        remaining_free_pages: remaining,
    })
}
