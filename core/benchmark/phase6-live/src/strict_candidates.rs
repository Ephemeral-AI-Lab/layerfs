//! Bounded domain-scoped persistence of the existing C2 admitted-FULL ring.
use crate::strict_catalog::{CandidateRow as SqlRow, CatalogScope, PlacementDomain};
use crate::strict_client::Catalog;
use layerfs_storage::{
    encoding::delta::candidates::{CandidateCatalog, CandidateRow},
    StorageError, StorageResult,
};
pub struct Access<'a> {
    pub catalog: &'a dyn Catalog,
    pub scope: CatalogScope,
    pub save: i64,
    pub domain: PlacementDomain,
}
fn error(message: String) -> StorageError {
    eprintln!("strict candidate catalog: {message}");
    StorageError::Integrity("strict candidate catalog")
}
impl CandidateCatalog for Access<'_> {
    fn candidate_page(&self, after: u64, limit: usize) -> StorageResult<Vec<CandidateRow>> {
        self.catalog
            .candidate_page(self.scope, self.domain, after, limit)
            .map_err(error)
            .map(|rows| {
                rows.into_iter()
                    .map(|r| CandidateRow {
                        slot: r.slot,
                        stamp: r.stamp,
                        id: r.id,
                        signature: r.signature,
                    })
                    .collect()
            })
    }
    fn write_candidates(&self, rows: &[CandidateRow]) -> StorageResult<()> {
        if rows.len() > crate::strict_catalog::PAGE {
            return Err(StorageError::Integrity("candidate page admission"));
        }
        let rows: Vec<_> = rows
            .iter()
            .map(|r| SqlRow {
                slot: r.slot,
                stamp: r.stamp,
                id: r.id,
                signature: r.signature,
            })
            .collect();
        self.catalog
            .stage_candidates(self.scope, self.save, self.domain, &rows)
            .map_err(error)
    }
}
