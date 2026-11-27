//! On-demand logical counts and allocation of one Workspace.
use crate::{db::unsigned, AllocationState, Overlay, OverlayResult, StatementKind};
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StoredCounts {
    pub namespaces: u64,
    pub inode_rows: u64,
    pub directory_entry_rows: u64,
    pub payload_cells: u64,
    pub payload_bytes: u64,
    pub shrink_rows: u64,
    /// Reference pieces: rows that name committed content and hold no bytes.
    pub reference_rows: u64,
    pub operation_record_rows: u64,
    pub operation_record_bytes: u64,
    pub owner_rows: u64,
    pub source_rows: u64,
    pub owner_details: u64,
    /// Pending reply tickets, held in the engine's memory.
    pub reply_tickets: u64,
    pub maintenance_targets: u64,
    pub ready_targets: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Resources {
    pub counts: StoredCounts,
    pub open: crate::OpenStateCounts,
    pub allocation: AllocationState,
    pub database_pages: u64,
    pub free_pages: u64,
    /// Declared logical bytes/metadata upper bound, not exclusive physical debt.
    pub debt_upper_bytes: u64,
}
const COUNTS: &str = "SELECT
    ?1,
    (SELECT count(*) FROM fs_inode),
    (SELECT count(*) FROM fs_entry),
    (SELECT count(*) FROM fs_data),
    (SELECT coalesce(sum(length(data)+ifnull(length(validity),0)),0) FROM fs_data),
    (SELECT count(*) FROM fs_truncate),
    (SELECT count(*) FROM commit_record),
    (SELECT coalesce(sum(length(value)),0) FROM commit_record),
    ?2,
    ?3,
    ?4,
    (SELECT count(*) FROM cleanup_task),
    (SELECT coalesce(sum(ready),0) FROM cleanup_task),
    (SELECT count(*) FROM fs_data_ref)";
impl Overlay {
    /// The exact counts and backing allocation of this database's Workspace.
    /// Allocated pages do not establish exclusive physical cleanup debt.
    pub fn resources(&self) -> OverlayResult<Resources> {
        self.available()?;
        // This explicit diagnostic visits rows on demand and projects lengths,
        // never payload values. Ordinary mutations maintain no accounting row.
        let (pins, sources, details) = {
            let open = self.open.borrow();
            let details = open.captured.len() + open.operations.len();
            (open.reads.len() + details, open.sources.rows.len(), details)
        };
        let counts = self
            .query(
                StatementKind::Startup,
                COUNTS,
                &[
                    &i64::from(self.workspace.get().is_some()),
                    &crate::db::integer(pins as u64)?,
                    &crate::db::integer(sources as u64)?,
                    &crate::db::integer(details as u64)?,
                ],
                8,
                |r| {
                    Ok(StoredCounts {
                        namespaces: unsigned(r, 0)?,
                        inode_rows: unsigned(r, 1)?,
                        directory_entry_rows: unsigned(r, 2)?,
                        payload_cells: unsigned(r, 3)?,
                        payload_bytes: unsigned(r, 4)?,
                        shrink_rows: unsigned(r, 5)?,
                        reference_rows: unsigned(r, 13)?,
                        operation_record_rows: unsigned(r, 6)?,
                        operation_record_bytes: unsigned(r, 7)?,
                        owner_rows: unsigned(r, 8)?,
                        source_rows: unsigned(r, 9)?,
                        owner_details: unsigned(r, 10)?,
                        reply_tickets: 0,
                        maintenance_targets: unsigned(r, 11)?,
                        ready_targets: unsigned(r, 12)?,
                    })
                },
            )?
            .pop()
            .unwrap_or_default();
        // Reply tickets are held in the engine's memory, not in a row.
        let counts = StoredCounts {
            reply_tickets: self.tickets.count(),
            ..counts
        };
        let (database_pages, free_pages) = self.pages()?;
        let metadata = counts
            .namespaces
            .saturating_add(counts.inode_rows)
            .saturating_add(counts.directory_entry_rows)
            .saturating_add(counts.payload_cells)
            .saturating_add(counts.shrink_rows)
            .saturating_add(counts.reference_rows)
            .saturating_add(counts.operation_record_rows)
            .saturating_add(counts.owner_rows)
            .saturating_add(counts.source_rows)
            .saturating_add(counts.owner_details)
            .saturating_add(counts.reply_tickets)
            .saturating_add(counts.maintenance_targets);
        let debt_upper_bytes = if counts.maintenance_targets == 0 {
            0
        } else {
            counts
                .payload_bytes
                .saturating_add(counts.operation_record_bytes)
                .saturating_add(metadata.saturating_mul(1024))
        };
        Ok(Resources {
            counts,
            open: self.open.borrow().snapshot(),
            allocation: self.allocation()?,
            database_pages,
            free_pages,
            debt_upper_bytes,
        })
    }
}
