//! Grouped reads of the base directory roots one update window rewrites.
//!
//! A Commit that touches many directories rewrites each from its stored root
//! page. Read one at a time, every page is its own provider demand, which on
//! a remote Store is one round trip or more per directory (#810 F3). The
//! update loop instead names, for each row of its window, the base root it
//! will rewrite (if any); [`RootsAhead`] reads them in grouped demands of at
//! most [`PAGES_AHEAD`] pages, in row order, and hands each page to its row.
//!
//! Bounds: at most [`PAGES_AHEAD`] canonical pages of at most
//! [`MAXIMUM_PAGE_BYTES`] each are held at once, 2 MiB, whatever the size of
//! the operation; a page is dropped when its row takes it. One grouped demand
//! is one attempt: its failure is the provider's original error and ends the
//! operation, exactly as a failed point read did, with no retry and no
//! fallback to point reads.
use std::collections::VecDeque;

use crate::error::{ContentError, ContentResult};
use crate::filesystem::limits::MAXIMUM_PAGE_BYTES;
use crate::filesystem::objects::FilesystemObjects;
use crate::object::ObjectId;

/// Most base directory pages one grouped demand reads and holds: 256 pages
/// of at most 8,192 bytes, 2 MiB.
pub(crate) const PAGES_AHEAD: usize = 256;
const _: () = assert!(PAGES_AHEAD * MAXIMUM_PAGE_BYTES <= 2 * 1024 * 1024);

/// The base roots of one window's rows, read ahead in grouped demands.
pub(crate) struct RootsAhead {
    /// Each row's base root, by row position in the window.
    wanted: Vec<Option<ObjectId>>,
    /// Pages read and not yet taken, with their row positions, in row order.
    pages: VecDeque<(usize, Vec<u8>)>,
    /// The first row position no grouped demand has covered yet.
    covered: usize,
}

impl RootsAhead {
    /// The rows' base roots: `None` for a row that rewrites no stored root.
    pub(crate) fn new(wanted: Vec<Option<ObjectId>>) -> Self {
        Self {
            wanted,
            pages: VecDeque::new(),
            covered: 0,
        }
    }

    /// The base root page of row `row`, if it named one. Rows take their
    /// pages in strictly increasing order; a row's first take of a page not
    /// yet read reads the next group.
    pub(crate) fn take(
        &mut self,
        objects: &mut FilesystemObjects<'_>,
        row: usize,
    ) -> ContentResult<Option<Vec<u8>>> {
        if self.wanted.get(row).copied().flatten().is_none() {
            return Ok(None);
        }
        if row >= self.covered {
            if !self.pages.is_empty() {
                return Err(ContentError::InvalidRecord("directory read-ahead order"));
            }
            self.read_group(objects, row)?;
        }
        match self.pages.pop_front() {
            Some((at, page)) if at == row => Ok(Some(page)),
            _ => Err(ContentError::InvalidRecord("directory read-ahead order")),
        }
    }

    /// Reads the roots of the next at most [`PAGES_AHEAD`] naming rows from
    /// `from` on, as one grouped demand.
    fn read_group(
        &mut self,
        objects: &mut FilesystemObjects<'_>,
        from: usize,
    ) -> ContentResult<()> {
        let mut rows = Vec::with_capacity(PAGES_AHEAD);
        let mut ids = Vec::with_capacity(PAGES_AHEAD);
        let mut end = from;
        while end < self.wanted.len() && ids.len() < PAGES_AHEAD {
            if let Some(id) = self.wanted[end] {
                rows.push(end);
                ids.push(id);
            }
            end += 1;
        }
        let pages = objects.read_batch(&ids)?;
        self.pages = rows.into_iter().zip(pages).collect();
        self.covered = end;
        Ok(())
    }
}
