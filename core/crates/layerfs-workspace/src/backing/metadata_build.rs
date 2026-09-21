//! Streaming construction of the bounded current dirty index during reconciliation.
use super::{
    metadata::RootOwner,
    metadata_index::vector,
    metadata_pages::{Cell, PageData, PageRef, HEADER, MAX_CELLS, MIN_BODY, PAGE},
    segments::Window,
};
use crate::WorkspaceError;
use std::time::Instant;
impl RootOwner {
    pub fn build_ordered(
        &self,
        mut next: impl FnMut(&mut Window) -> Result<Option<Cell>, WorkspaceError>,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<PageRef, WorkspaceError> {
        let mut cells: Vec<Cell> = vector(MAX_CELLS)?;
        let mut branches = vector(25)?;
        let mut body = 0;
        let mut total = 0;
        let mut previous: Option<Cell> = None;
        while let Some(cell) = next(window)? {
            total += 1;
            if total > 256
                || cell.key_len > 17
                || !matches!(cell.key()[0], b'D' | b'I' | b'N')
                || previous.as_ref().is_some_and(|old| old.key() >= cell.key())
            {
                return Err(WorkspaceError::Io);
            }
            if cells.len() == MAX_CELLS || body + cell.size() > PAGE - HEADER {
                let mut left_bytes = 0;
                let mut cut = 0;
                for c in &cells {
                    if left_bytes >= body / 2 {
                        break;
                    }
                    left_bytes += c.size();
                    cut += 1;
                }
                if left_bytes < MIN_BODY || body - left_bytes < MIN_BODY || branches.len() == 24 {
                    return Err(WorkspaceError::Capacity);
                }
                let mut right = vector(MAX_CELLS)?;
                right.extend(cells.drain(cut..));
                let max = cells.last().ok_or(WorkspaceError::Io)?.clone();
                let page = self.write_page(PageData { level: 0, cells }, window, deadline)?;
                branches.push(Cell::new(max.key(), &page.bytes())?);
                cells = right;
                body -= left_bytes;
            }
            body += cell.size();
            previous = Some(cell.clone());
            cells.push(cell);
        }
        if cells.is_empty() {
            return Ok(PageRef::NULL);
        }
        if !branches.is_empty() && body < MIN_BODY {
            return Err(WorkspaceError::Io);
        }
        let max = cells.last().ok_or(WorkspaceError::Io)?.clone();
        let leaf = self.write_page(PageData { level: 0, cells }, window, deadline)?;
        if branches.is_empty() {
            return Ok(leaf);
        }
        branches.push(Cell::new(max.key(), &leaf.bytes())?);
        self.write_page(
            PageData {
                level: 1,
                cells: branches,
            },
            window,
            deadline,
        )
    }
}
