//! Backed logical counts and real shared allocation observations.
use crate::{db::unsigned, AllocationState, Overlay, OverlayResult, Route, StatementKind};
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StoredCounts {
    pub wait_refs: u64,
    pub namespaces: u64,
    pub inode_rows: u64,
    pub directory_entry_rows: u64,
    pub payload_cells: u64,
    pub payload_bytes: u64,
    pub shrink_rows: u64,
    pub operation_record_rows: u64,
    pub operation_record_bytes: u64,
    pub orphan_rows: u64,
    pub owner_rows: u64,
    pub source_rows: u64,
    pub owner_details: u64,
    pub reply_tickets: u64,
    pub retire_targets: u64,
    pub maintenance_targets: u64,
    pub ready_targets: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Resources {
    pub counts: StoredCounts,
    pub allocation: AllocationState,
    pub database_pages: u64,
    pub free_pages: u64,
    /// Declared logical bytes/metadata upper bound, not exclusive physical debt.
    pub debt_upper_bytes: u64,
}
impl Overlay {
    /// None is the daemon aggregate; a route reads one namespace's exact counts.
    /// Page/file observations remain shared and never imply exclusive attribution.
    pub fn resources(&self, route: Option<Route>) -> OverlayResult<Resources> {
        self.available()?;
        let ns = if let Some(route) = route {
            self.state(route)?;
            route.namespace()
        } else {
            0
        };
        let counts=self.query(StatementKind::Startup,
            "SELECT namespaces,inode_rows,directory_entry_rows,payload_cells,payload_bytes,shrink_rows,operation_record_rows,operation_record_bytes,orphan_rows,owner_rows,source_rows,owner_details,reply_tickets,retire_targets,maintenance_targets,ready_targets,wait_refs FROM accounting WHERE ns=?1",&[&ns],8,
            |r|Ok(StoredCounts {
                wait_refs:unsigned(r,16)?,
                namespaces:unsigned(r,0)?,
                inode_rows:unsigned(r,1)?,
                directory_entry_rows:unsigned(r,2)?,
                payload_cells:unsigned(r,3)?,
                payload_bytes:unsigned(r,4)?,
                shrink_rows:unsigned(r,5)?,
                operation_record_rows:unsigned(r,6)?,
                operation_record_bytes:unsigned(r,7)?,
                orphan_rows:unsigned(r,8)?,
                owner_rows:unsigned(r,9)?,
                source_rows:unsigned(r,10)?,
                owner_details:unsigned(r,11)?,
                reply_tickets:unsigned(r,12)?,
                retire_targets:unsigned(r,13)?,
                maintenance_targets:unsigned(r,14)?,
                ready_targets:unsigned(r,15)?,
            }))?.pop().unwrap_or_default();
        let (database_pages, free_pages) = self.pages()?;
        let metadata = counts
            .namespaces
            .saturating_add(counts.inode_rows)
            .saturating_add(counts.directory_entry_rows)
            .saturating_add(counts.payload_cells)
            .saturating_add(counts.shrink_rows)
            .saturating_add(counts.operation_record_rows)
            .saturating_add(counts.orphan_rows)
            .saturating_add(counts.owner_rows)
            .saturating_add(counts.source_rows)
            .saturating_add(counts.owner_details)
            .saturating_add(counts.wait_refs)
            .saturating_add(counts.reply_tickets)
            .saturating_add(counts.retire_targets)
            .saturating_add(counts.maintenance_targets);
        let debt_upper_bytes = if counts.maintenance_targets + counts.retire_targets == 0 {
            0
        } else {
            counts
                .payload_bytes
                .saturating_add(counts.operation_record_bytes)
                .saturating_add(metadata.saturating_mul(1024))
        };
        Ok(Resources {
            counts,
            allocation: self.allocation()?,
            database_pages,
            free_pages,
            debt_upper_bytes,
        })
    }
    pub fn explain_accounting(&self) -> OverlayResult<Vec<String>> {
        self.query(StatementKind::Explain,"EXPLAIN QUERY PLAN SELECT payload_cells,payload_bytes,maintenance_targets FROM accounting WHERE ns=?1",
            &[&0_i64],8,|r|r.get(3))
    }
}
