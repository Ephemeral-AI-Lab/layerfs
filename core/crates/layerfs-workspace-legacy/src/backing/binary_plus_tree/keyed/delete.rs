//! Path-local keyed deletion: one key out of one leaf, with sibling repair.
//!
//! Insertion copies the one path a key lives on. A removal copies the same
//! path and adds the repairs only a removal needs: a page the removal emptied
//! disappears from its parent, and a page the removal left under the declared
//! minimum body is rewritten with a neighbour, merged into one page when the
//! pair fits and split between two legal pages otherwise. Every non-root page
//! therefore keeps a body of at least `MIN_BODY`, and the published height
//! follows the key count rather than the deletion history: a root that keeps
//! one child is lifted out, exactly as the insertion path lifts a collapsed
//! node in.
use super::update::{check_index, key_limit};
use crate::{
    backing::{
        metadata::RootOwner,
        metadata_index::vector,
        metadata_pages::{Cell, PageData, PageRef, HEADER, MAX_CELLS, MIN_BODY, PAGE},
        segments::Window,
    },
    WorkspaceError,
};
use std::time::Instant;

impl RootOwner {
    /// Removes exactly one key from the tree at `root`.
    ///
    /// A key the tree does not hold leaves the tree unchanged; removing the
    /// last key answers `PageRef::NULL`. Every page the removal copies or
    /// rewrites is written through this owner, so the result carries the same
    /// ownership, reference accounting and cleanup obligations as any other
    /// publication of this root.
    pub fn delete(
        &self,
        root: PageRef,
        key: &[u8],
        window: &mut Window,
        deadline: Instant,
    ) -> Result<PageRef, WorkspaceError> {
        if root == PageRef::NULL {
            return Ok(PageRef::NULL);
        }
        let limit = key_limit(key)?;
        let Some(body) = self.remove(root, None, key, limit, window, deadline)? else {
            return Ok(PageRef::NULL);
        };
        // A root that keeps one child is that child: the level it held is no
        // longer a function of the key count, and a one-cell branch page is
        // below the minimum body a non-root page must carry.
        if body.level > 0 && body.cells.len() == 1 {
            return PageRef::parse(body.cells[0].value());
        }
        self.write_page(body, window, deadline)
    }

    /// The unwritten body of the subtree at `root` without `key`, or `None`
    /// once that subtree holds nothing at all.
    fn remove(
        &self,
        root: PageRef,
        expected: Option<u8>,
        key: &[u8],
        limit: u8,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<Option<PageData>, WorkspaceError> {
        let page = self.arena.load(root, window, deadline)?;
        check_index(&page.cells, page.level, limit)?;
        if expected.is_some_and(|level| level != page.level || page.body() < MIN_BODY) {
            return Err(WorkspaceError::Io);
        }
        if page.level == 0 {
            let mut cells = vector(page.cells.len())?;
            for cell in page.cells {
                if cell.key() != key {
                    cells.push(cell);
                }
            }
            return Ok(node(page.level, cells));
        }
        // One key lives on exactly one path: the first child whose fence is not
        // below it, or the last child when no fence is.
        let at = page
            .cells
            .iter()
            .position(|cell| cell.key() >= key)
            .unwrap_or(page.cells.len() - 1);
        let child = PageRef::parse(page.cells[at].value())?;
        let rebuilt = self.remove(child, Some(page.level - 1), key, limit, window, deadline)?;
        let mut cells = page.cells;
        self.repair(&mut cells, at, rebuilt, limit, window, deadline)?;
        Ok(node(page.level, cells))
    }

    /// Puts one rebuilt child back under its parent, rewriting a neighbour with
    /// it when the removal left it under the minimum body. Every page written
    /// here is written through this owner.
    fn repair(
        &self,
        cells: &mut Vec<Cell>,
        at: usize,
        rebuilt: Option<PageData>,
        limit: u8,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        let Some(body) = rebuilt else {
            // The child holds nothing, so its fence no longer names a page.
            cells.remove(at);
            return Ok(());
        };
        let level = body.level;
        if body.body() >= MIN_BODY || cells.len() == 1 {
            // Either the child is a legal page on its own, or this node has no
            // neighbour to repair it with. A node that keeps one child is
            // repaired one level up, and a root that keeps one child is lifted
            // out by `delete`.
            let max = body.cells.last().ok_or(WorkspaceError::Io)?.key().to_vec();
            let r = self.write_page(body, window, deadline)?;
            cells[at] = Cell::new(&max, &r.bytes())?;
            return Ok(());
        }
        // The child underflows and has a neighbour at its own level. The pair
        // is rewritten into one page when it fits, and into two legal pages
        // otherwise: the pair's total is at least the neighbour's own legal
        // body, so it always carries at least one page's worth of cells.
        let sibling_at = if at + 1 < cells.len() { at + 1 } else { at - 1 };
        let sibling =
            self.arena
                .load(PageRef::parse(cells[sibling_at].value())?, window, deadline)?;
        check_index(&sibling.cells, sibling.level, limit)?;
        if sibling.level != level || sibling.body() < MIN_BODY {
            return Err(WorkspaceError::Io);
        }
        let (left, right) = if at < sibling_at {
            (body, sibling)
        } else {
            (sibling, body)
        };
        let mut merged = left.cells;
        merged.extend(right.cells);
        let bytes: usize = merged.iter().map(Cell::size).sum();
        let pages = if bytes <= PAGE - HEADER && merged.len() <= MAX_CELLS {
            vec![merged]
        } else {
            let (head, tail) = split_balanced(merged)?;
            vec![head, tail]
        };
        let mut fences = vector(pages.len())?;
        for cells in pages {
            let max = cells.last().ok_or(WorkspaceError::Io)?.key().to_vec();
            let r = self.write_page(PageData { level, cells }, window, deadline)?;
            fences.push(Cell::new(&max, &r.bytes())?);
        }
        let first = at.min(sibling_at);
        cells.remove(first);
        if fences.len() == 1 {
            cells[first] = fences.pop().ok_or(WorkspaceError::Io)?;
        } else {
            let mut fences = fences.into_iter();
            cells[first] = fences.next().ok_or(WorkspaceError::Io)?;
            cells.insert(first + 1, fences.next().ok_or(WorkspaceError::Io)?);
        }
        Ok(())
    }
}

/// One node body, or nothing when the key set it holds is empty.
fn node(level: u8, cells: Vec<Cell>) -> Option<PageData> {
    if cells.is_empty() {
        None
    } else {
        Some(PageData { level, cells })
    }
}

/// Splits one page's cells into two pages that are both legal - at least
/// `MIN_BODY` declared bytes and at most `MAX_CELLS` cells each - and that
/// differ in size as little as the page format allows.
///
/// A pair of legal pages can hold a key set that no legal pair can hold again:
/// two pages of 128 minimum-size cells total 256 cells in about 2 KiB, which is
/// more cells than one page may carry and fewer bytes than two pages must
/// carry. That shape is refused as `Capacity` rather than published as a page
/// the reader would reject.
fn split_balanced(cells: Vec<Cell>) -> Result<(Vec<Cell>, Vec<Cell>), WorkspaceError> {
    let total: usize = cells.iter().map(Cell::size).sum();
    let mut left = 0;
    let mut best: Option<(usize, usize)> = None;
    for cut in 1..cells.len() {
        left += cells[cut - 1].size();
        let right = total - left;
        if left < MIN_BODY || right < MIN_BODY || cut > MAX_CELLS || cells.len() - cut > MAX_CELLS {
            continue;
        }
        let imbalance = left.abs_diff(right);
        if best.is_none_or(|(known, _)| imbalance < known) {
            best = Some((imbalance, cut));
        }
    }
    let (_, cut) = best.ok_or(WorkspaceError::Capacity)?;
    let mut cells = cells;
    let right = cells.split_off(cut);
    Ok((cells, right))
}
