//! Generic v2 mutation: reachable-subtree replacement over fixed fences.
//!
//! Every selected branch child stays nonempty, splits are byte-balanced near
//! the encoded midpoint, and a replaced hot node keeps its slot and reuse
//! epoch so ancestors naming it stay valid. A cold node stages cold
//! replacements; a hot node stages hot ones, so no cold edge conceals a
//! current hot target.
use super::{
    hot_cursor::{cached, resident, BoundUpdates, Cache, Cached, RESIDENT_BYTES},
    hot_directory::{Directory, Entry},
    index::Selection,
    keyed::{
        Cell, Child, Node, Target, HOT_SLOTS, MAX_KEY, MAX_LEVEL, MAX_VALUE, TAG_HOT, TARGET_BYTES,
    },
    page::{Kind, PageRef, BODY_BYTES},
    pages::{Cause, Counter, PageReservation, PageStore},
};
use crate::{backing::budget::Charge, WorkspaceError};
use std::{collections::BTreeMap, sync::Arc};

pub(super) type Update = (Vec<u8>, Option<Vec<u8>>);

const LEAF_CELL: usize = 4 + MAX_KEY + MAX_VALUE;
const BRANCH_CELL: usize = 2 + MAX_KEY + TARGET_BYTES;

fn cell_bytes(cell: &Cell) -> usize {
    4 + cell.key.len() + cell.value.len()
}

fn child_bytes(child: &Child) -> usize {
    2 + child.fence.len() + TARGET_BYTES
}

fn branch_body(children: &[Child]) -> usize {
    children.iter().map(child_bytes).sum::<usize>()
        - children.last().map_or(0, |child| child.fence.len())
}

/// The terminal child inherits the parent range. Balance the bytes actually
/// encoded after removing its finite separator, including both split halves.
fn branch_balance(mut children: Vec<Child>) -> Result<Vec<Vec<Child>>, WorkspaceError> {
    let total = branch_body(&children);
    if total <= BODY_BYTES {
        return Ok(vec![children]);
    }
    if children.len() < 2 {
        return Err(WorkspaceError::Capacity);
    }
    let last_width = children.last().ok_or(WorkspaceError::Io)?.fence.len();
    let raw: usize = children.iter().map(child_bytes).sum();
    let mut prefix = 0;
    let mut best = (usize::MAX, 1);
    for at in 1..children.len() {
        prefix += child_bytes(&children[at - 1]);
        let left = prefix - children[at - 1].fence.len();
        let right = raw - prefix - last_width;
        let distance = left.abs_diff(right);
        if distance < best.0 {
            best = (distance, at);
        }
    }
    let right = children.split_off(best.1);
    let mut groups = branch_balance(children)?;
    groups.extend(branch_balance(right)?);
    if groups
        .iter()
        .any(|group| branch_body(group) + BRANCH_CELL < BODY_BYTES / 2)
    {
        return Err(WorkspaceError::Capacity);
    }
    Ok(groups)
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
    pub(super) cache: Cache,
    admitted: BTreeMap<PageRef, (usize, u64)>,
    loaded: BTreeMap<PageRef, Arc<Cached>>,
    source_directory: Directory,
    normalize: u64,
    wanted: Vec<Vec<u8>>,
    pub(super) reservation: Option<PageReservation>,
}

impl Mutation {
    pub(super) fn new(
        selection: &Selection,
        slot_epochs: [u64; HOT_SLOTS],
        charge: Charge,
        cache: Cache,
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
            cache,
            admitted: BTreeMap::new(),
            loaded: BTreeMap::new(),
            source_directory: selection
                .directory
                .as_ref()
                .map(|directory| directory.value.clone())
                .unwrap_or_default(),
            normalize: 0,
            wanted: Vec::new(),
            reservation: None,
        })
    }

    /// Best-effort path union admission, before any index candidate is staged.
    /// A slot or resident-byte refusal leaves the generic mutation selected.
    pub(super) fn admit(
        &mut self,
        store: &PageStore,
        selection: &Selection,
        keys: &[Vec<u8>],
        allow_cold: bool,
    ) -> Result<bool, WorkspaceError> {
        self.wanted = keys.to_vec();
        let mut paths = BTreeMap::new();
        let mut used = 0u64;
        for key in keys {
            store.count(Counter::Seek, 1);
            let Some(mut target) = selection.root else {
                return Ok(false);
            };
            let mut level = selection.height;
            loop {
                let page = self.previous(target, level)?;
                let node = match self.load(store, selection, target, level) {
                    Ok(node) => node,
                    Err(WorkspaceError::Capacity) => {
                        self.loaded.clear();
                        return Ok(false);
                    }
                    Err(error) => return Err(error),
                };
                if target.tag == TAG_HOT {
                    used |= 1 << target.hot_slot()?;
                }
                if target.tag != TAG_HOT {
                    self.loaded.insert(page, node.clone());
                    paths.insert(page, (node.clone(), level));
                }
                match &node.node {
                    Node::Leaf(_) => break,
                    Node::Branch(children) => {
                        let at = children.partition_point(|child| {
                            !child.fence.is_empty() && child.fence.as_slice() <= key.as_slice()
                        });
                        target = children.get(at).ok_or(WorkspaceError::Io)?.target;
                        level = level.checked_sub(1).ok_or(WorkspaceError::Io)?;
                    }
                }
            }
        }
        let mut parents = [None; HOT_SLOTS];
        for slot in 0..HOT_SLOTS {
            if let Some(node) = &self.cache[slot] {
                if let Node::Branch(children) = &node.node {
                    for child in children {
                        if child.target.tag == TAG_HOT {
                            parents[child.target.hot_slot()?] = Some(slot);
                        }
                    }
                }
            }
        }
        let obsolete: Vec<_> = (0..HOT_SLOTS)
            .filter(|slot| self.directory.entry(*slot).is_some() && used & (1 << slot) == 0)
            .collect();
        for slot in &obsolete {
            let mut at = Some(*slot);
            for _ in 0..=MAX_LEVEL {
                let Some(slot) = at else { break };
                self.normalize |= 1 << slot;
                at = parents[slot];
            }
            let entry = self.directory.entry(*slot).ok_or(WorkspaceError::Io)?;
            if let Some(node) = self.cache[*slot].take() {
                self.loaded.insert(entry.page, node);
            }
            self.directory.clear(*slot);
            self.directory_changed = true;
        }
        store.count(Counter::Normalization, obsolete.len() as u64);
        let free: Vec<_> = (0..HOT_SLOTS)
            .filter(|slot| self.directory.entry(*slot).is_none())
            .collect();
        if !allow_cold {
            paths.clear();
        }
        let added_bytes: usize = paths.values().map(|(node, _)| node.bytes).sum();
        if paths.len() > free.len()
            || resident(&self.cache) + added_bytes > RESIDENT_BYTES
            || free
                .iter()
                .take(paths.len())
                .any(|slot| self.slot_epochs[*slot] == u64::MAX)
        {
            return Ok(false);
        }
        let admitted = paths.len();
        for ((page, (node, level)), slot) in paths.into_iter().zip(free) {
            let epoch = self.slot_epochs[slot] + 1;
            self.slot_epochs[slot] = epoch;
            self.admitted.insert(page, (slot, epoch));
            self.cache[slot] = Some(node);
            self.directory.set(
                slot,
                Entry {
                    epoch,
                    page,
                    level,
                    kind: level_kind(level)?,
                },
            )?;
            self.directory_changed = true;
        }
        if admitted > 0 {
            self.normalize |= used;
            store.count(Counter::Admission, 1);
        }
        Ok(true)
    }

    fn load(
        &mut self,
        store: &PageStore,
        selection: &Selection,
        target: Target,
        level: u8,
    ) -> Result<Arc<Cached>, WorkspaceError> {
        store.count(Counter::Visit, 1);
        if cached(&self.cache, &self.directory, target).is_some() {
            store.count(Counter::CacheHit, 1);
            return Ok(self.cache[target.hot_slot()?]
                .as_ref()
                .ok_or(WorkspaceError::Io)?
                .clone());
        }
        let page = if target.tag == TAG_HOT {
            selection
                .directory
                .as_ref()
                .ok_or(WorkspaceError::Io)?
                .value
                .resolve(target.hot_slot()?, target.epoch, level_kind(level)?)?
                .page
        } else {
            target.cold_page()?
        };
        if let Some(node) = self.loaded.get(&page) {
            return Ok(node.clone());
        }
        let stored = store.read(page, level_kind(level)?)?;
        let node = Cached::decode(store, page, &stored)?;
        Ok(node)
    }

    fn reserve(&mut self, pages: usize, body: usize) -> Result<(), WorkspaceError> {
        let needed = pages
            .checked_add(self.created.len())
            .and_then(|pages| pages.checked_add(self.replaced.len()))
            .ok_or(WorkspaceError::Capacity)?
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
        if node.kind() != level_kind(level)? {
            return Err(WorkspaceError::Io);
        }
        if let Node::Branch(children) = node {
            for child in children {
                let kind = level_kind(level.checked_sub(1).ok_or(WorkspaceError::Io)?)?;
                if child.target.tag == TAG_HOT {
                    let entry = self.directory.resolve(
                        child.target.hot_slot()?,
                        child.target.epoch,
                        kind,
                    )?;
                    if entry.level + 1 != level {
                        return Err(WorkspaceError::Io);
                    }
                } else if store.kind(child.target.cold_page()?)? != kind {
                    return Err(WorkspaceError::Io);
                }
            }
        }
        let encode_time = store.stamp(Cause::NodeEncode);
        let body = node.encode()?;
        drop(encode_time);
        self.reserve(1, body.len())?;
        let (reference, stored) = store.create_from(
            node.kind(),
            generation,
            revision,
            node.len() as u16,
            &body,
            self.reservation.as_mut(),
        )?;
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
                self.cache[slot] = Some(Cached::decode(store, reference, &stored)?);
                if resident(&self.cache) > RESIDENT_BYTES {
                    return Err(WorkspaceError::Capacity);
                }
                Target::hot(slot, epoch)
            }
            None => Ok(Target::cold(reference)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit(
        &mut self,
        store: &PageStore,
        generation: u64,
        revision: u64,
        level: u8,
        at: Option<Target>,
        nodes: Vec<(Node, Vec<u8>)>,
        lower: &[u8],
    ) -> Result<Vec<Child>, WorkspaceError> {
        let hot = match at {
            Some(target) if target.tag == TAG_HOT => {
                let slot = target.hot_slot()?;
                self.directory
                    .entry(slot)
                    .filter(|entry| entry.epoch == target.epoch)
                    .map(|_| (slot, target.epoch))
            }
            Some(target) => self.admitted.get(&target.cold_page()?).copied(),
            _ => None,
        };
        if nodes.is_empty() {
            if let Some((slot, _)) = hot {
                self.directory.clear(slot);
                self.cache[slot] = None;
                self.directory_changed = true;
            }
            return Ok(Vec::new());
        }
        let mut children = Vec::with_capacity(nodes.len());
        if nodes.len() > 1 {
            store.split(
                level,
                nodes
                    .iter()
                    .map(|(node, _)| node.body_len())
                    .min()
                    .ok_or(WorkspaceError::Io)?,
            );
        }
        let mut bound = lower;
        let needed: Vec<_> = nodes.iter().map(|(node, fence)| {
            let wanted = self.wanted.iter().any(|key| key.as_slice() >= bound && (fence.is_empty() || key.as_slice() < fence.as_slice()));
            bound = fence;
            (wanted && hot.is_some()) || matches!(node, Node::Branch(children) if children.iter().any(|child| child.target.tag == TAG_HOT))
        }).collect();
        let keep_at = needed.iter().rposition(|needed| *needed);
        if hot.is_some() && keep_at.is_none() {
            let (slot, _) = hot.ok_or(WorkspaceError::Io)?;
            self.directory.clear(slot);
            self.cache[slot] = None;
            self.directory_changed = true;
        }
        for (position, (node, fence)) in nodes.iter().enumerate() {
            let keep = if needed[position] {
                if keep_at == Some(position) && hot.is_some() {
                    hot
                } else {
                    Some(self.slot()?)
                }
            } else {
                None
            };
            children.push(Child {
                fence: fence.clone(),
                target: self.page(store, node, generation, revision, level, keep)?,
            });
        }
        Ok(children)
    }

    pub(super) fn merge(cells: Vec<Cell>, updates: &[Update]) -> Result<Vec<Cell>, WorkspaceError> {
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
        let groups = branch_balance(children)?;
        let count = groups.len();
        for (at, mut group) in groups.into_iter().enumerate() {
            let last = group.last_mut().ok_or(WorkspaceError::Io)?;
            let fence = if at + 1 == count {
                upper.to_vec()
            } else {
                last.fence.clone()
            };
            last.fence.clear();
            nodes.push((Node::Branch(group), fence));
        }
        Ok(nodes)
    }

    fn previous(&mut self, target: Target, level: u8) -> Result<PageRef, WorkspaceError> {
        if target.tag == TAG_HOT {
            let slot = target.hot_slot()?;
            let entry = self
                .source_directory
                .resolve(slot, target.epoch, level_kind(level)?)?;
            return Ok(entry.page);
        }
        target.cold_page()
    }

    fn admitted_target(&self, target: Target) -> Result<Target, WorkspaceError> {
        if target.tag == TAG_HOT {
            let slot = target.hot_slot()?;
            return Ok(
                if self
                    .directory
                    .entry(slot)
                    .is_some_and(|entry| entry.epoch == target.epoch)
                {
                    target
                } else {
                    Target::cold(
                        self.source_directory
                            .entry(slot)
                            .ok_or(WorkspaceError::Io)?
                            .page,
                    )
                },
            );
        }
        match self.admitted.get(&target.cold_page()?) {
            Some((slot, epoch)) => Target::hot(*slot, *epoch),
            None => Ok(target),
        }
    }

    pub(super) fn want(&mut self, keys: &[Vec<u8>]) {
        self.wanted = keys.to_vec();
    }

    pub(super) fn direct(
        &mut self,
        store: &PageStore,
        selection: &Selection,
        generation: u64,
        revision: u64,
        groups: &BoundUpdates,
    ) -> Result<Vec<Child>, WorkspaceError> {
        let mut pending = BTreeMap::<usize, Vec<(Target, Vec<Child>)>>::new();
        let mut ranges = BTreeMap::<usize, (Vec<u8>, Vec<u8>, Option<usize>)>::new();
        let mut root = vec![Child {
            fence: Vec::new(),
            target: selection.root.ok_or(WorkspaceError::Io)?,
        }];
        for (slot, updates) in &groups.groups {
            let entry = self.directory.entry(*slot).ok_or(WorkspaceError::Io)?;
            let node = self.cache[*slot].as_ref().ok_or(WorkspaceError::Io)?;
            let Node::Leaf(cells) = &node.node else {
                return Err(WorkspaceError::Io);
            };
            let merge_time = store.stamp(Cause::ActualMerge);
            let merged = Self::merge(cells.clone(), updates)?;
            drop(merge_time);
            if merged.len() == cells.len()
                && merged
                    .iter()
                    .zip(cells)
                    .all(|(a, b)| a.key == b.key && a.value == b.value)
            {
                continue;
            }
            store.count(Counter::Visit, 1);
            let binding = groups.bindings.get(slot).ok_or(WorkspaceError::Io)?;
            let nodes = Self::leaf_groups(merged, &binding.fence)?;
            let target = Target::hot(*slot, entry.epoch)?;
            let children = self.emit(
                store,
                generation,
                revision,
                0,
                Some(target),
                nodes,
                &binding.lower,
            )?;
            self.replaced.push(entry.page);
            if children.len() == 1 && children[0].target == target {
                continue;
            }
            store.count(Counter::Carry, 1);
            let mut lower = Vec::new();
            let mut fence = Vec::new();
            let mut parent = None;
            for (position, (at, epoch, _)) in binding.ancestors.iter().enumerate() {
                ranges.insert(*at, (lower.clone(), fence.clone(), parent));
                let target = Target::hot(*at, *epoch)?;
                let node =
                    cached(&self.cache, &self.directory, target).ok_or(WorkspaceError::Io)?;
                let Node::Branch(cells) = &node.node else {
                    return Err(WorkspaceError::Io);
                };
                let next = match binding.ancestors.get(position + 1) {
                    Some((slot, epoch, _)) => Target::hot(*slot, *epoch)?,
                    None => Target::hot(*slot, entry.epoch)?,
                };
                let index = cells
                    .iter()
                    .position(|child| child.target == next)
                    .ok_or(WorkspaceError::Io)?;
                if index > 0 {
                    lower = cells[index - 1].fence.clone();
                }
                if !cells[index].fence.is_empty() {
                    fence = cells[index].fence.clone();
                }
                parent = Some(*at);
            }
            if let Some(parent) = parent {
                pending.entry(parent).or_default().push((target, children));
            } else {
                root = children;
            }
        }
        while !pending.is_empty() {
            let slot = *pending
                .keys()
                .min_by_key(|slot| self.directory.entry(**slot).map(|entry| entry.level))
                .ok_or(WorkspaceError::Io)?;
            let changes = pending.remove(&slot).ok_or(WorkspaceError::Io)?;
            let entry = self.directory.entry(slot).ok_or(WorkspaceError::Io)?;
            let node = self.cache[slot].as_ref().ok_or(WorkspaceError::Io)?;
            let Node::Branch(children) = &node.node else {
                return Err(WorkspaceError::Io);
            };
            let mut merged = Vec::new();
            for child in children {
                if let Some((_, replacement)) =
                    changes.iter().find(|(target, _)| *target == child.target)
                {
                    merged.extend_from_slice(replacement);
                } else {
                    merged.push(child.clone());
                }
            }
            store.count(Counter::Visit, 1);
            let (lower, fence, parent) = ranges.get(&slot).ok_or(WorkspaceError::Io)?;
            let nodes = Self::branch_groups(merged, fence)?;
            let target = Target::hot(slot, entry.epoch)?;
            let replacement = self.emit(
                store,
                generation,
                revision,
                entry.level,
                Some(target),
                nodes,
                lower,
            )?;
            self.replaced.push(entry.page);
            if replacement.len() == 1 && replacement[0].target == target {
                continue;
            }
            store.count(Counter::Carry, 1);
            if let Some(parent) = parent {
                pending
                    .entry(*parent)
                    .or_default()
                    .push((target, replacement));
            } else {
                root = replacement;
            }
        }
        Ok(root)
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
        lower: &[u8],
        generation: u64,
        revision: u64,
    ) -> Result<Vec<Child>, WorkspaceError> {
        let heating = at.is_some_and(|target| {
            if target.tag == TAG_HOT {
                target
                    .hot_slot()
                    .ok()
                    .is_some_and(|slot| self.normalize & (1 << slot) != 0)
            } else {
                target
                    .cold_page()
                    .ok()
                    .is_some_and(|page| self.admitted.contains_key(&page))
            }
        });
        if updates.is_empty() && !heating {
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
            return self.emit(store, generation, revision, level, None, nodes, lower);
        };
        let previous = self.previous(target, level)?;
        let selected = self.load(store, selection, target, level)?;
        let _node_copy = store.budget().reserve(selected.bytes)?;
        let node = selected.node.clone();
        let nodes = match node {
            Node::Leaf(cells) => {
                if updates.is_empty() {
                    return Ok(vec![Child {
                        fence: upper.to_vec(),
                        target: self.admitted_target(target)?,
                    }]);
                }
                let merged = Self::merge(cells, updates)?;
                Self::leaf_groups(merged, upper)?
            }
            Node::Branch(children) => {
                let count = children.len();
                let original = children.clone();
                let mut merged = Vec::new();
                let mut cursor = 0;
                let mut child_lower = lower.to_vec();
                for (position, child) in children.into_iter().enumerate() {
                    let end = if position + 1 == count || child.fence.is_empty() {
                        updates.len()
                    } else {
                        cursor
                            + updates[cursor..]
                                .partition_point(|(key, _)| key.as_slice() < child.fence.as_slice())
                    };
                    let child_heating = if child.target.tag == TAG_HOT {
                        self.normalize & (1 << child.target.hot_slot()?) != 0
                    } else {
                        child
                            .target
                            .cold_page()
                            .ok()
                            .is_some_and(|page| self.admitted.contains_key(&page))
                    };
                    let child_upper = if child.fence.is_empty() {
                        upper.to_vec()
                    } else {
                        child.fence.clone()
                    };
                    if end == cursor && !child_heating {
                        merged.push(child);
                    } else {
                        let mut replaced = self.change(
                            store,
                            selection,
                            Some(child.target),
                            level - 1,
                            &updates[cursor..end],
                            &child_upper,
                            &child_lower,
                            generation,
                            revision,
                        )?;
                        merged.append(&mut replaced);
                    }
                    cursor = end;
                    child_lower = child_upper;
                }
                if merged == original {
                    return Ok(vec![Child {
                        fence: upper.to_vec(),
                        target: self.admitted_target(target)?,
                    }]);
                }
                Self::branch_groups(merged, upper)?
            }
        };
        let _ = kind;
        let children = self.emit(
            store,
            generation,
            revision,
            level,
            Some(target),
            nodes,
            lower,
        )?;
        self.replaced.push(previous);
        Ok(children)
    }
}
