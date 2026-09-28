//! Selected v2 resolution: exact floor, search and scans through tagged
//! targets, fixed fences and one optional fixed-slot directory.
use super::{
    index::{IndexEntry, Selection},
    keyed::{Node, Target, MAX_LEVEL},
    page::{Kind, PageRef},
    pages::PageStore,
};
use crate::WorkspaceError;

pub(super) const SCAN_LIMIT: usize = 128;

pub(super) struct Resolver<'a> {
    store: &'a PageStore,
    selection: &'a Selection,
    seen: u64,
}

impl<'a> Resolver<'a> {
    pub(super) fn new(store: &'a PageStore, selection: &'a Selection) -> Self {
        Self {
            store,
            selection,
            seen: 0,
        }
    }

    fn expected_kind(level: u8) -> Result<Kind, WorkspaceError> {
        if level > MAX_LEVEL {
            return Err(WorkspaceError::Io);
        }
        Ok(if level == 0 {
            Kind::IndexLeaf
        } else {
            Kind::IndexBranch
        })
    }

    fn read(&self, page: PageRef, kind: Kind) -> Result<Node, WorkspaceError> {
        if self.store.kind(page)? != kind {
            return Err(WorkspaceError::Io);
        }
        let stored = self.store.read(page, kind)?;
        Node::decode(&stored, self.store.incarnation(), page, kind)
    }

    /// Resolve one tagged target at an expected level. A hot target must be
    /// selected by this view's directory with that level and reuse epoch; an
    /// absent, superseded or kind-mismatched slot refuses instead of falling
    /// back to another physical page.
    pub(super) fn node(&mut self, target: Target, level: u8) -> Result<Node, WorkspaceError> {
        let kind = Self::expected_kind(level)?;
        if target.tag == super::keyed::TAG_COLD {
            return self.read(target.cold_page()?, kind);
        }
        let slot = target.hot_slot()?;
        let bit = 1u64 << slot;
        if self.seen & bit != 0 {
            return Err(WorkspaceError::Io);
        }
        let directory = self
            .selection
            .directory
            .as_ref()
            .ok_or(WorkspaceError::Io)?;
        let entry = directory.value.resolve(slot, target.epoch, kind)?;
        if entry.level != level {
            return Err(WorkspaceError::Io);
        }
        self.seen |= bit;
        self.read(entry.page, kind)
    }

    /// Index of the child whose partition contains `key`, or `None` when the
    /// key is beyond the last finite fence.
    fn partition(children: &[super::keyed::Child], key: &[u8]) -> Option<usize> {
        let at = children
            .partition_point(|child| !child.fence.is_empty() && child.fence.as_slice() <= key);
        (at < children.len()).then_some(at)
    }

    pub(super) fn get(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, WorkspaceError> {
        let Some(mut target) = self.selection.root else {
            return Ok(None);
        };
        let mut level = self.selection.height;
        loop {
            match self.node(target, level)? {
                Node::Leaf(cells) => {
                    return Ok(cells
                        .binary_search_by(|cell| cell.key.as_slice().cmp(key))
                        .ok()
                        .map(|at| cells[at].value.clone()))
                }
                Node::Branch(children) => {
                    let Some(at) = Self::partition(&children, key) else {
                        return Ok(None);
                    };
                    target = children[at].target;
                    level -= 1;
                }
            }
        }
    }

    fn rightmost(
        &mut self,
        target: Target,
        level: u8,
    ) -> Result<Option<IndexEntry>, WorkspaceError> {
        let mut target = target;
        let mut level = level;
        loop {
            match self.node(target, level)? {
                Node::Leaf(cells) => {
                    return Ok(cells
                        .last()
                        .map(|cell| (cell.key.clone(), cell.value.clone())))
                }
                Node::Branch(children) => {
                    let child = children.last().ok_or(WorkspaceError::Io)?;
                    target = child.target;
                    level -= 1;
                }
            }
        }
    }

    pub(super) fn floor(&mut self, key: &[u8]) -> Result<Option<IndexEntry>, WorkspaceError> {
        let Some(root) = self.selection.root else {
            return Ok(None);
        };
        self.floor_at(root, self.selection.height, key)
    }

    /// Partition descent plus, when the partition's first key already exceeds
    /// the request, the maximum of the preceding nonempty partition.
    fn floor_at(
        &mut self,
        target: Target,
        level: u8,
        key: &[u8],
    ) -> Result<Option<IndexEntry>, WorkspaceError> {
        match self.node(target, level)? {
            Node::Leaf(cells) => Ok(
                match cells.binary_search_by(|cell| cell.key.as_slice().cmp(key)) {
                    Ok(at) => Some((cells[at].key.clone(), cells[at].value.clone())),
                    Err(0) => None,
                    Err(at) => Some((cells[at - 1].key.clone(), cells[at - 1].value.clone())),
                },
            ),
            Node::Branch(children) => {
                let Some(at) = Self::partition(&children, key) else {
                    let last = children.last().ok_or(WorkspaceError::Io)?;
                    return self.rightmost(last.target, level - 1);
                };
                if let Some(found) = self.floor_at(children[at].target, level - 1, key)? {
                    return Ok(Some(found));
                }
                if at == 0 {
                    return Ok(None);
                }
                self.rightmost(children[at - 1].target, level - 1)
            }
        }
    }

    pub(super) fn scan(
        &mut self,
        lower: &[u8],
        upper: &[u8],
        limit: usize,
    ) -> Result<Vec<IndexEntry>, WorkspaceError> {
        if lower >= upper || limit == 0 || limit > SCAN_LIMIT {
            return Err(WorkspaceError::InvalidInput);
        }
        let mut entries = Vec::new();
        if let Some(root) = self.selection.root {
            self.scan_at(
                root,
                self.selection.height,
                lower,
                upper,
                limit,
                &mut entries,
            )?;
        }
        Ok(entries)
    }

    fn scan_at(
        &mut self,
        target: Target,
        level: u8,
        lower: &[u8],
        upper: &[u8],
        limit: usize,
        entries: &mut Vec<IndexEntry>,
    ) -> Result<(), WorkspaceError> {
        match self.node(target, level)? {
            Node::Leaf(cells) => {
                let start = cells.partition_point(|cell| cell.key.as_slice() < lower);
                for cell in cells.into_iter().skip(start) {
                    if cell.key.as_slice() >= upper || entries.len() == limit {
                        break;
                    }
                    entries.push((cell.key, cell.value));
                }
            }
            Node::Branch(children) => {
                let start = children.partition_point(|child| {
                    !child.fence.is_empty() && child.fence.as_slice() <= lower
                });
                let mut bound: Option<&[u8]> =
                    (start > 0).then(|| children[start - 1].fence.as_slice());
                for child in &children[start..] {
                    if bound.is_some_and(|fence| fence >= upper) {
                        break;
                    }
                    self.scan_at(child.target, level - 1, lower, upper, limit, entries)?;
                    if entries.len() == limit {
                        break;
                    }
                    if !child.fence.is_empty() {
                        bound = Some(child.fence.as_slice());
                    }
                }
            }
        }
        Ok(())
    }
}
