//! Fixed-size work counts for one extent splice; no page is read for telemetry.
use super::splice::PieceStore;
use crate::{
    backing::{
        metadata_pages::{self, PageRef, PAGE, RECORD},
        segments::Window,
    },
    WorkspaceError,
};
use std::cell::Cell;

#[derive(Clone, Copy, Default)]
struct Work {
    leaf_visits: u64,
    branch_visits: u64,
    leaf_visit_records: u64,
    branch_visit_children: u64,
    leaf_writes: u64,
    branch_writes: u64,
    leaf_write_records: u64,
    branch_write_children: u64,
    leaf_min: u64,
    leaf_max: u64,
    child_edges_added: u64,
    custody_edges_added: u64,
}

pub(super) struct Counted<'a, S: PieceStore + ?Sized> {
    store: &'a S,
    work: Cell<Work>,
}

impl<'a, S: PieceStore + ?Sized> Counted<'a, S> {
    pub fn new(store: &'a S) -> Self {
        Self {
            store,
            work: Cell::new(Work::default()),
        }
    }
    pub fn report(&self, root_height: u8) {
        if std::env::var_os("LAYERFS_COMPLEXITY_DIAGNOSTIC").is_none() {
            return;
        }
        let w = self.work.get();
        eprintln!(
            "LFS_EXTENT_SPLICE v=1 root_height={} leaf_visits={} branch_visits={} leaf_visit_records={} branch_visit_children={} leaf_writes={} branch_writes={} leaf_write_records={} branch_write_children={} leaf_min={} leaf_max={} child_edges_added={} custody_edges_added={}",
            root_height, w.leaf_visits, w.branch_visits, w.leaf_visit_records,
            w.branch_visit_children, w.leaf_writes, w.branch_writes,
            w.leaf_write_records, w.branch_write_children, w.leaf_min, w.leaf_max,
            w.child_edges_added, w.custody_edges_added,
        );
    }
}

impl<S: PieceStore + ?Sized> PieceStore for Counted<'_, S> {
    fn read(&self, page: PageRef, window: &mut Window) -> Result<[u8; PAGE], WorkspaceError> {
        let bytes = self.store.read(page, window)?;
        if let Ok(used) = metadata_pages::body_used(&bytes) {
            let mut w = self.work.get();
            if bytes[48] == 0 {
                w.leaf_visits = w.leaf_visits.saturating_add(1);
                w.leaf_visit_records = w.leaf_visit_records.saturating_add((used / RECORD) as u64);
            } else {
                w.branch_visits = w.branch_visits.saturating_add(1);
                w.branch_visit_children = w
                    .branch_visit_children
                    .saturating_add((used / metadata_pages::ChildRef::BYTES) as u64);
            }
            self.work.set(w);
        }
        Ok(bytes)
    }
    fn write<T>(
        &self,
        level: u8,
        sponsor: PageRef,
        window: &mut Window,
        encode: impl FnOnce(PageRef, &mut [u8]) -> Result<T, WorkspaceError>,
    ) -> Result<T, WorkspaceError> {
        let occupancy = Cell::new(None);
        let value = self.store.write(level, sponsor, window, |page, bytes| {
            let value = encode(page, bytes)?;
            if let Ok(used) = metadata_pages::body_used(bytes) {
                let count = used
                    / if level == 0 {
                        RECORD
                    } else {
                        metadata_pages::ChildRef::BYTES
                    };
                let custody = if level == 0 {
                    bytes[metadata_pages::HEADER..metadata_pages::HEADER + used]
                        .chunks_exact(RECORD)
                        .filter(|record| record[0] == 1)
                        .count()
                } else {
                    0
                };
                occupancy.set(Some((count as u64, custody as u64)));
            }
            Ok(value)
        })?;
        if let Some((count, custody)) = occupancy.get() {
            let mut w = self.work.get();
            if level == 0 {
                w.leaf_writes = w.leaf_writes.saturating_add(1);
                w.leaf_write_records = w.leaf_write_records.saturating_add(count);
                w.leaf_min = if w.leaf_writes == 1 {
                    count
                } else {
                    w.leaf_min.min(count)
                };
                w.leaf_max = w.leaf_max.max(count);
                w.custody_edges_added = w.custody_edges_added.saturating_add(custody);
            } else {
                w.branch_writes = w.branch_writes.saturating_add(1);
                w.branch_write_children = w.branch_write_children.saturating_add(count);
                w.child_edges_added = w.child_edges_added.saturating_add(count);
            }
            self.work.set(w);
        }
        Ok(value)
    }
    fn incarnation(&self) -> [u8; 32] {
        self.store.incarnation()
    }
}
