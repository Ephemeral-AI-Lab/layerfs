//! Generic v2 mutation: reachable-subtree replacement over fixed fences.
//!
//! Every selected branch child stays nonempty, splits are byte-balanced near
//! the encoded midpoint, and a replaced hot node keeps its slot and reuse
//! epoch so ancestors naming it stay valid. A cold node stages cold
//! replacements; a hot node stages hot ones, so no cold edge conceals a
//! current hot target.
use super::{
    hot_directory::{Directory, Entry},
    index::Selection,
    keyed::{
        Cell, Child, Node, Target, HOT_SLOTS, MAX_KEY, MAX_LEVEL, MAX_VALUE, TAG_HOT, TARGET_BYTES,
    },
    page::{Kind, PageRef, BODY_BYTES},
    pages::PageStore,
    resolve::Resolver,
};
use crate::{backing::budget::Charge, WorkspaceError};

pub(super) type Update = (Vec<u8>, Option<Vec<u8>>);

const LEAF_CELL: usize = 4 + MAX_KEY + MAX_VALUE;
const BRANCH_CELL: usize = 2 + MAX_KEY + TARGET_BYTES;

fn cell_bytes(cell: &Cell) -> usize {
    4 + cell.key.len() + cell.value.len()
}

fn child_bytes(child: &Child) -> usize {
    2 + child.fence.len() + TARGET_BYTES
}

fn level_kind(level: u8) -> Result<Kind, WorkspaceError> {
    match level {
        0 => Ok(Kind::IndexLeaf),
        level if level <= MAX_LEVEL => Ok(Kind::IndexBranch),
        _ => Err(WorkspaceError::Io),
    }
}

/// Byte-balanced grouping: split at the cell boundary nearest the encoded
/// midpoint until every group fits one body. A split of an overflowing node
/// leaves at least `BODY_BYTES/2 - max_cell` bytes on each side, which is the
/// capacity the split reserves for later bounded insertions.
fn balance<T>(
    items: Vec<T>,
    width: fn(&T) -> usize,
    max_cell: usize,
) -> Result<Vec<Vec<T>>, WorkspaceError> {
    let total: usize = items.iter().map(width).sum();
    if total <= BODY_BYTES {
        return Ok(vec![items]);
    }
    if items.len() < 2 {
        return Err(WorkspaceError::Capacity);
    }
    let half = total / 2;
    let mut best = 1;
    let mut distance = usize::MAX;
    let mut prefix = 0;
    for at in 1..items.len() {
        prefix += width(&items[at - 1]);
        let current = prefix.abs_diff(half);
        if current < distance {
            distance = current;
            best = at;
        }
    }
    let mut items = items;
    let tail_items = items.split_off(best);
    let mut groups = balance(items, width, max_cell)?;
    let mut rest = balance(tail_items, width, max_cell)?;
    if groups
        .iter()
        .chain(rest.iter())
        .any(|group| group.iter().map(width).sum::<usize>() > BODY_BYTES)
        || groups
            .iter()
            .chain(rest.iter())
            .any(|group| group.iter().map(width).sum::<usize>() + max_cell < BODY_BYTES / 2)
    {
        return Err(WorkspaceError::Capacity);
    }
    groups.append(&mut rest);
    Ok(groups)
}

/// One staged mutation: the selected directory copy plus every page it staged.
pub(super) struct Mutation {
    pub(super) directory: Directory,
    pub(super) directory_changed: bool,
    pub(super) slot_epochs: [u64; HOT_SLOTS],
    pub(super) created: Vec<PageRef>,
    pub(super) replaced: Vec<PageRef>,
    pub(super) charge: Charge,
}

impl Mutation {
    pub(super) fn new(
        selection: &Selection,
        slot_epochs: [u64; HOT_SLOTS],
        charge: Charge,
    ) -> Result<Self, WorkspaceError> {
        Ok(Self {
            directory: selection
                .directory
                .as_ref()
                .map(|directory| directory.value.clone())
                .unwrap_or_default(),
            directory_changed: false,
            slot_epochs,
            created: Vec::new(),
            replaced: Vec::new(),
            charge,
        })
    }

    fn reserve(&mut self, pages: usize, body: usize) -> Result<(), WorkspaceError> {
        let needed = pages
            .checked_mul(512)
            .and_then(|total| total.checked_add(body))
            .and_then(|total| total.checked_add(64 * 1024))
            .ok_or(WorkspaceError::Capacity)?;
        self.charge.resize(needed)
    }

    fn slot(&mut self) -> Result<(usize, u64), WorkspaceError> {
        let slot = (0..HOT_SLOTS)
            .find(|slot| self.directory.entry(*slot).is_none())
            .ok_or(WorkspaceError::Capacity)?;
        let epoch = self.slot_epochs[slot]
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        self.slot_epochs[slot] = epoch;
        self.reserve(1, 0)?;
        Ok((slot, epoch))
    }

    fn page(
        &mut self,
        store: &PageStore,
        node: &Node,
        generation: u64,
        revision: u64,
        level: u8,
        keep: Option<(usize, u64)>,
    ) -> Result<Target, WorkspaceError> {
        let body = node.encode()?;
        self.reserve(1, body.len())?;
        let reference =
            store.create(node.kind(), generation, revision, node.len() as u16, &body)?;
        self.created.push(reference);
        match keep {
            Some((slot, epoch)) => {
                self.directory.set(
                    slot,
                    Entry {
                        epoch,
                        page: reference,
                        level,
                        kind: node.kind(),
                    },
                )?;
                self.directory_changed = true;
                Target::hot(slot, epoch)
            }
            None => Ok(Target::cold(reference)),
        }
    }

    pub(super) fn emit(
        &mut self,
        store: &PageStore,
        generation: u64,
        revision: u64,
        level: u8,
        at: Option<Target>,
        nodes: Vec<(Node, Vec<u8>)>,
    ) -> Result<Vec<Child>, WorkspaceError> {
        let hot = match at {
            Some(target) if target.tag == TAG_HOT => Some((target.hot_slot()?, target.epoch)),
            _ => None,
        };
        if nodes.is_empty() {
            if let Some((slot, _)) = hot {
                self.directory.clear(slot);
                self.directory_changed = true;
            }
            return Ok(Vec::new());
        }
        let mut children = Vec::with_capacity(nodes.len());
        for (position, (node, fence)) in nodes.iter().enumerate() {
            let keep = match (position, hot) {
                (0, hot) => hot,
                (_, Some(_)) => Some(self.slot()?),
                (_, None) => None,
            };
            children.push(Child {
                fence: fence.clone(),
                target: self.page(store, node, generation, revision, level, keep)?,
            });
        }
        Ok(children)
    }

    fn merge(cells: Vec<Cell>, updates: &[Update]) -> Result<Vec<Cell>, WorkspaceError> {
        let mut old = cells.into_iter().peekable();
        let mut merged = Vec::new();
        for (key, value) in updates {
            while old
                .peek()
                .is_some_and(|cell| cell.key.as_slice() < key.as_slice())
            {
                merged.push(old.next().ok_or(WorkspaceError::Io)?);
            }
            if old.peek().is_some_and(|cell| cell.key == *key) {
                old.next();
            }
            if let Some(value) = value {
                merged.push(Cell {
                    key: key.clone(),
                    value: value.clone(),
                });
            }
        }
        merged.extend(old);
        Ok(merged)
    }

    fn leaf_groups(cells: Vec<Cell>, upper: &[u8]) -> Result<Vec<(Node, Vec<u8>)>, WorkspaceError> {
        if cells.is_empty() {
            return Ok(Vec::new());
        }
        let mut groups = balance(cells, cell_bytes, LEAF_CELL)?
            .into_iter()
            .peekable();
        let mut nodes = Vec::new();
        while let Some(group) = groups.next() {
            let fence = match groups.peek() {
                Some(next) => next.first().ok_or(WorkspaceError::Io)?.key.clone(),
                None => upper.to_vec(),
            };
            nodes.push((Node::Leaf(group), fence));
        }
        Ok(nodes)
    }

    pub(super) fn branch_groups(
        children: Vec<Child>,
        upper: &[u8],
    ) -> Result<Vec<(Node, Vec<u8>)>, WorkspaceError> {
        if children.is_empty() {
            return Ok(Vec::new());
        }
        let mut nodes = Vec::new();
        for group in balance(children, child_bytes, BRANCH_CELL)? {
            let last = group.last().ok_or(WorkspaceError::Io)?.fence.clone();
            let fence = if last.is_empty() {
                upper.to_vec()
            } else {
                last
            };
            nodes.push((Node::Branch(group), fence));
        }
        Ok(nodes)
    }

    fn previous(&mut self, target: Target, level: u8) -> Result<PageRef, WorkspaceError> {
        if target.tag == TAG_HOT {
            let slot = target.hot_slot()?;
            let entry = self
                .directory
                .resolve(slot, target.epoch, level_kind(level)?)?;
            return Ok(entry.page);
        }
        target.cold_page()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn change(
        &mut self,
        store: &PageStore,
        selection: &Selection,
        at: Option<Target>,
        level: u8,
        updates: &[Update],
        upper: &[u8],
        generation: u64,
        revision: u64,
    ) -> Result<Vec<Child>, WorkspaceError> {
        if updates.is_empty() {
            return Ok(match at {
                Some(target) => vec![Child {
                    fence: upper.to_vec(),
                    target,
                }],
                None => Vec::new(),
            });
        }
        let kind = level_kind(level)?;
        let Some(target) = at else {
            if level != 0 {
                return Err(WorkspaceError::Io);
            }
            let nodes = Self::leaf_groups(
                updates
                    .iter()
                    .filter_map(|(key, value)| {
                        value.as_ref().map(|value| Cell {
                            key: key.clone(),
                            value: value.clone(),
                        })
                    })
                    .collect(),
                upper,
            )?;
            return self.emit(store, generation, revision, level, None, nodes);
        };
        let previous = self.previous(target, level)?;
        let mut resolver = Resolver::new(store, selection);
        let node = resolver.node(target, level)?;
        let nodes = match node {
            Node::Leaf(cells) => {
                let merged = Self::merge(cells, updates)?;
                Self::leaf_groups(merged, upper)?
            }
            Node::Branch(children) => {
                let count = children.len();
                let mut merged = Vec::new();
                let mut cursor = 0;
                for (position, child) in children.into_iter().enumerate() {
                    let end = if position + 1 == count || child.fence.is_empty() {
                        updates.len()
                    } else {
                        cursor
                            + updates[cursor..]
                                .partition_point(|(key, _)| key.as_slice() < child.fence.as_slice())
                    };
                    if end == cursor {
                        merged.push(child);
                    } else {
                        let mut replaced = self.change(
                            store,
                            selection,
                            Some(child.target),
                            level - 1,
                            &updates[cursor..end],
                            &child.fence,
                            generation,
                            revision,
                        )?;
                        merged.append(&mut replaced);
                    }
                    cursor = end;
                }
                Self::branch_groups(merged, upper)?
            }
        };
        let _ = kind;
        let children = self.emit(store, generation, revision, level, Some(target), nodes)?;
        self.replaced.push(previous);
        Ok(children)
    }
}
