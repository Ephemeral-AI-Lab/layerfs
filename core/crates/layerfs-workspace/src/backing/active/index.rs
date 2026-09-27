use super::{
    keyed::{Cell, Child, Node},
    page::{PageRef, BODY_BYTES},
    pages::PageStore,
};
use crate::{backing::budget::Charge, WorkspaceError};
use std::{
    collections::BTreeMap,
    mem::size_of,
    sync::{Arc, Mutex},
};

const MAX_LEVEL: usize = 7;
const MAX_CAPTURES: usize = 32;
const MAX_PINNED_REVISIONS: usize = MAX_CAPTURES + 128;
pub type IndexEntry = (Vec<u8>, Vec<u8>);

struct Retired {
    page: PageRef,
    birth: u64,
    retired: u64,
}

struct ChangeContext {
    generation: u64,
    revision: u64,
    created: Vec<PageRef>,
    replaced: Vec<PageRef>,
}

struct State {
    root: Option<PageRef>,
    generation: u64,
    revision: u64,
    pending: bool,
    stopped: bool,
    captured: usize,
    frozen: BTreeMap<u64, usize>,
    retired: Vec<Retired>,
    retired_charge: Charge,
}

/// A pooled, ordered on-disk index. Every mutation stages complete page
/// versions; the current root changes only after the caller publishes its
/// matching pack and Workspace state tuple.
pub struct Index {
    store: Arc<PageStore>,
    state: Mutex<State>,
    _charge: Charge,
}

pub struct IndexCandidate {
    index: Arc<Index>,
    expected_root: Option<PageRef>,
    expected_revision: u64,
    generation: u64,
    root: Option<PageRef>,
    created: Vec<PageRef>,
    replaced: Vec<PageRef>,
    _scratch: Charge,
    finished: bool,
}

pub struct IndexSnapshot {
    index: Arc<Index>,
    root: Option<PageRef>,
    revision: u64,
    captured: bool,
    released: bool,
}

pub struct ScanPage {
    entries: Vec<IndexEntry>,
    _charge: Charge,
}

impl ScanPage {
    pub fn entries(&self) -> &[IndexEntry] {
        &self.entries
    }
}

impl Drop for IndexCandidate {
    fn drop(&mut self) {
        if !self.finished {
            if let Ok(mut state) = self.index.state.lock() {
                state.stopped = true;
            }
        }
    }
}

impl Drop for IndexSnapshot {
    fn drop(&mut self) {
        if !self.released {
            if let Ok(mut state) = self.index.state.lock() {
                state.stopped = true;
            }
        }
    }
}

impl Index {
    pub fn new(store: Arc<PageStore>) -> Result<Arc<Self>, WorkspaceError> {
        let budget = store.budget();
        Ok(Arc::new(Self {
            store,
            state: Mutex::new(State {
                root: None,
                generation: 1,
                revision: 0,
                pending: false,
                stopped: false,
                captured: 0,
                frozen: BTreeMap::new(),
                retired: Vec::new(),
                retired_charge: budget.reserve(0)?,
            }),
            _charge: budget.reserve(4096 + MAX_PINNED_REVISIONS * 128)?,
        }))
    }

    pub(crate) fn budget(&self) -> Arc<crate::backing::budget::Budget> {
        self.store.budget()
    }

    pub fn generation_revision(&self) -> Result<(u64, u64), WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        Ok((state.generation, state.revision))
    }

    pub fn frozen_between(&self, birth: u64, retired: u64) -> Result<bool, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        Ok(state.frozen.range(birth..retired).next().is_some())
    }

    pub fn frozen_count(&self) -> Result<usize, WorkspaceError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .frozen
            .len())
    }

    fn node(&self, reference: PageRef) -> Result<Node, WorkspaceError> {
        let kind = self.store.kind(reference)?;
        let page = self.store.read(reference, kind)?;
        Node::decode(&page, self.store.incarnation(), reference, kind)
    }

    fn write(
        &self,
        node: &Node,
        generation: u64,
        revision: u64,
        created: &mut Vec<PageRef>,
    ) -> Result<Child, WorkspaceError> {
        let body = node.encode()?;
        let reference =
            self.store
                .create(node.kind(), generation, revision, node.len() as u16, &body)?;
        created.push(reference);
        Ok(Child {
            max: node.max().to_vec(),
            page: reference,
        })
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        self.get_at(state.root, key)
    }

    pub fn floor(&self, key: &[u8]) -> Result<Option<IndexEntry>, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        state
            .root
            .map(|root| self.floor_node(root, key, 0))
            .transpose()
            .map(Option::flatten)
    }

    fn floor_node(
        &self,
        reference: PageRef,
        key: &[u8],
        depth: usize,
    ) -> Result<Option<IndexEntry>, WorkspaceError> {
        if depth > MAX_LEVEL {
            return Err(WorkspaceError::Io);
        }
        match self.node(reference)? {
            Node::Leaf(cells) => {
                let at = match cells.binary_search_by(|cell| cell.key.as_slice().cmp(key)) {
                    Ok(at) => Some(at),
                    Err(0) => None,
                    Err(at) => Some(at - 1),
                };
                Ok(at.map(|at| (cells[at].key.clone(), cells[at].value.clone())))
            }
            Node::Branch(children) => {
                let at = children
                    .iter()
                    .position(|child| child.max.as_slice() >= key)
                    .unwrap_or(children.len() - 1);
                if let Some(found) = self.floor_node(children[at].page, key, depth + 1)? {
                    return Ok(Some(found));
                }
                if at == 0 {
                    return Ok(None);
                }
                self.floor_node(children[at - 1].page, &children[at - 1].max, depth + 1)
            }
        }
    }

    pub fn scan(
        &self,
        lower: &[u8],
        upper: &[u8],
        limit: usize,
    ) -> Result<ScanPage, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        self.scan_at(state.root, lower, upper, limit)
    }

    fn scan_at(
        &self,
        root: Option<PageRef>,
        lower: &[u8],
        upper: &[u8],
        limit: usize,
    ) -> Result<ScanPage, WorkspaceError> {
        if lower >= upper || limit == 0 || limit > 128 {
            return Err(WorkspaceError::InvalidInput);
        }
        let charge = self.store.budget().reserve(64 * 1024 + limit * 1024)?;
        let mut entries = Vec::new();
        if let Some(root) = root {
            self.scan_node(root, lower, upper, limit, 0, &mut entries)?;
        }
        Ok(ScanPage {
            entries,
            _charge: charge,
        })
    }

    fn scan_node(
        &self,
        reference: PageRef,
        lower: &[u8],
        upper: &[u8],
        limit: usize,
        depth: usize,
        entries: &mut Vec<IndexEntry>,
    ) -> Result<(), WorkspaceError> {
        if depth > MAX_LEVEL {
            return Err(WorkspaceError::Io);
        }
        match self.node(reference)? {
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
                for child in children {
                    if child.max.as_slice() < lower {
                        continue;
                    }
                    self.scan_node(child.page, lower, upper, limit, depth + 1, entries)?;
                    if entries.len() == limit || child.max.as_slice() >= upper {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    fn get_at(
        &self,
        mut root: Option<PageRef>,
        key: &[u8],
    ) -> Result<Option<Vec<u8>>, WorkspaceError> {
        for _ in 0..=MAX_LEVEL {
            let Some(reference) = root else {
                return Ok(None);
            };
            match self.node(reference)? {
                Node::Leaf(cells) => {
                    return Ok(cells
                        .binary_search_by(|cell| cell.key.as_slice().cmp(key))
                        .ok()
                        .map(|at| cells[at].value.clone()))
                }
                Node::Branch(children) => {
                    root = children
                        .iter()
                        .find(|child| child.max.as_slice() >= key)
                        .map(|child| child.page)
                }
            }
        }
        Err(WorkspaceError::Io)
    }

    fn height(&self, mut root: PageRef) -> Result<usize, WorkspaceError> {
        for depth in 0..=MAX_LEVEL {
            match self.node(root)? {
                Node::Leaf(_) => return Ok(depth),
                Node::Branch(children) => root = children.first().ok_or(WorkspaceError::Io)?.page,
            }
        }
        Err(WorkspaceError::Capacity)
    }

    fn bulk_leaves(
        &self,
        cells: Vec<Cell>,
        context: &mut ChangeContext,
    ) -> Result<Vec<Child>, WorkspaceError> {
        let mut result = Vec::new();
        let mut group = Vec::new();
        let mut used = 0;
        for cell in cells {
            let size = 4 + cell.key.len() + cell.value.len();
            if size > BODY_BYTES {
                return Err(WorkspaceError::Capacity);
            }
            if used + size > BODY_BYTES {
                result.push(self.write(
                    &Node::Leaf(std::mem::take(&mut group)),
                    context.generation,
                    context.revision,
                    &mut context.created,
                )?);
                used = 0;
            }
            used += size;
            group.push(cell);
        }
        if !group.is_empty() {
            result.push(self.write(
                &Node::Leaf(group),
                context.generation,
                context.revision,
                &mut context.created,
            )?);
        }
        Ok(result)
    }

    fn bulk_branches(
        &self,
        children: Vec<Child>,
        context: &mut ChangeContext,
    ) -> Result<Vec<Child>, WorkspaceError> {
        if children.len() <= 1 {
            return Ok(children);
        }
        let mut result = Vec::new();
        let mut group = Vec::new();
        let mut used = 0;
        for child in children {
            let size = 2 + child.max.len() + 16;
            if size > BODY_BYTES {
                return Err(WorkspaceError::Capacity);
            }
            if used + size > BODY_BYTES {
                result.push(self.write(
                    &Node::Branch(std::mem::take(&mut group)),
                    context.generation,
                    context.revision,
                    &mut context.created,
                )?);
                used = 0;
            }
            used += size;
            group.push(child);
        }
        if !group.is_empty() {
            result.push(self.write(
                &Node::Branch(group),
                context.generation,
                context.revision,
                &mut context.created,
            )?);
        }
        Ok(result)
    }

    fn bulk_change(
        &self,
        old: Option<PageRef>,
        updates: &[(Vec<u8>, Option<Vec<u8>>)],
        depth: usize,
        context: &mut ChangeContext,
    ) -> Result<Vec<Child>, WorkspaceError> {
        if depth > MAX_LEVEL {
            return Err(WorkspaceError::Capacity);
        }
        let Some(reference) = old else {
            let cells = updates
                .iter()
                .filter_map(|(key, value)| {
                    value.as_ref().map(|value| Cell {
                        key: key.clone(),
                        value: value.clone(),
                    })
                })
                .collect();
            return self.bulk_leaves(cells, context);
        };
        if updates.is_empty() {
            let node = self.node(reference)?;
            return Ok(vec![Child {
                max: node.max().to_vec(),
                page: reference,
            }]);
        }
        let replacement = match self.node(reference)? {
            Node::Leaf(cells) => {
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
                self.bulk_leaves(merged, context)?
            }
            Node::Branch(children) => {
                let mut merged = Vec::new();
                let mut at = 0;
                let count = children.len();
                for (position, child) in children.into_iter().enumerate() {
                    let end = if position + 1 == count {
                        updates.len()
                    } else {
                        at + updates[at..]
                            .partition_point(|(key, _)| key.as_slice() <= child.max.as_slice())
                    };
                    if end == at {
                        merged.push(child);
                    } else {
                        merged.extend(self.bulk_change(
                            Some(child.page),
                            &updates[at..end],
                            depth + 1,
                            context,
                        )?);
                    }
                    at = end;
                }
                self.bulk_branches(merged, context)?
            }
        };
        context.replaced.push(reference);
        Ok(replacement)
    }

    pub fn prepare_bulk(
        self: &Arc<Self>,
        updates: &[(Vec<u8>, Option<Vec<u8>>)],
    ) -> Result<IndexCandidate, WorkspaceError> {
        self.prepare(updates)
    }

    /// Sorted unique updates share one reached-leaf traversal and one candidate
    /// root. A clean Commit may publish an empty update to advance its revision.
    pub fn prepare(
        self: &Arc<Self>,
        updates: &[(Vec<u8>, Option<Vec<u8>>)],
    ) -> Result<IndexCandidate, WorkspaceError> {
        if updates.windows(2).any(|pair| pair[0].0 >= pair[1].0)
            || updates.iter().any(|(key, value)| {
                key.is_empty()
                    || key.len() > 272
                    || value.as_ref().is_some_and(|value| value.len() > 512)
            })
        {
            return Err(WorkspaceError::InvalidInput);
        }
        let mut scratch = self.store.budget().reserve(128 * 1024)?;
        self.maintain()?;
        let (root, generation, revision) = {
            let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            if state.pending || state.stopped {
                return Err(WorkspaceError::Busy);
            }
            state
                .revision
                .checked_add(1)
                .ok_or(WorkspaceError::Capacity)?;
            state.pending = true;
            (state.root, state.generation, state.revision)
        };
        let mut context = ChangeContext {
            generation,
            revision: revision + 1,
            created: Vec::new(),
            replaced: Vec::new(),
        };
        let result = (|| {
            let budget = updates
                .iter()
                .try_fold(128 * 1024usize, |bytes, (key, value)| {
                    bytes
                        .checked_add(128 + key.len() + value.as_ref().map_or(0, Vec::len))
                        .ok_or(WorkspaceError::Capacity)
                })?;
            scratch.resize(budget)?;
            let mut children = self.bulk_change(root, updates, 0, &mut context)?;
            let mut height = root.map(|root| self.height(root)).transpose()?.unwrap_or(0);
            while children.len() > 1 {
                if height >= MAX_LEVEL {
                    return Err(WorkspaceError::Capacity);
                }
                children = self.bulk_branches(children, &mut context)?;
                height += 1;
            }
            Ok::<_, WorkspaceError>(children.pop().map(|child| child.page))
        })();
        let new_root = match result {
            Ok(root) => root,
            Err(error) => {
                let mut cleanup = None;
                for page in &context.created {
                    if let Err(failure) = self.store.release(*page) {
                        cleanup = Some(failure);
                    }
                }
                let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                state.pending = false;
                if cleanup.is_some() {
                    state.stopped = true;
                }
                return Err(cleanup.unwrap_or(error));
            }
        };
        Ok(IndexCandidate {
            index: self.clone(),
            expected_root: root,
            expected_revision: revision,
            generation,
            root: new_root,
            created: context.created,
            replaced: context.replaced,
            _scratch: scratch,
            finished: false,
        })
    }

    fn pin(self: &Arc<Self>, capture: bool) -> Result<IndexSnapshot, WorkspaceError> {
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let revision = state.revision;
        if state.pending
            || state.stopped
            || (capture && state.captured == MAX_CAPTURES)
            || (state.frozen.len() == MAX_PINNED_REVISIONS && !state.frozen.contains_key(&revision))
        {
            return Err(WorkspaceError::Busy);
        }
        if capture {
            let next_generation = state
                .generation
                .checked_add(1)
                .ok_or(WorkspaceError::Capacity)?;
            let next_revision = state
                .revision
                .checked_add(1)
                .ok_or(WorkspaceError::Capacity)?;
            state.generation = next_generation;
            state.revision = next_revision;
            state.captured += 1;
        }
        *state.frozen.entry(revision).or_default() += 1;
        Ok(IndexSnapshot {
            index: self.clone(),
            root: state.root,
            revision,
            captured: capture,
            released: false,
        })
    }

    pub fn capture(self: &Arc<Self>) -> Result<IndexSnapshot, WorkspaceError> {
        self.pin(true)
    }

    pub fn pin_current(self: &Arc<Self>) -> Result<IndexSnapshot, WorkspaceError> {
        self.pin(false)
    }

    pub fn maintain(&self) -> Result<u64, WorkspaceError> {
        let mut released = 0;
        loop {
            let selected = {
                let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                let at = state.retired.iter().position(|item| {
                    state
                        .frozen
                        .range(item.birth..item.retired)
                        .next()
                        .is_none()
                });
                at.map(|at| state.retired.swap_remove(at))
            };
            let Some(item) = selected else { break };
            match self.store.release(item.page) {
                Ok(bytes) => released += bytes,
                Err(error) => {
                    let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                    state.retired.push(item);
                    state.stopped = true;
                    return Err(error);
                }
            }
        }
        Ok(released)
    }
}

impl IndexCandidate {
    pub fn root(&self) -> Option<PageRef> {
        self.root
    }

    pub fn publish(mut self) -> Result<u64, WorkspaceError> {
        let mut state = self.index.state.lock().map_err(|_| WorkspaceError::Io)?;
        if !state.pending
            || state.root != self.expected_root
            || state.revision != self.expected_revision
            || state.generation != self.generation
        {
            return Err(WorkspaceError::Busy);
        }
        let next = state
            .revision
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        let mut retire = Vec::with_capacity(self.replaced.len());
        for reference in &self.replaced {
            let page = self
                .index
                .store
                .read(*reference, self.index.store.kind(*reference)?)?;
            retire.push(Retired {
                page: *reference,
                birth: page.revision(),
                retired: next,
            });
        }
        let growth = state
            .retired
            .len()
            .checked_add(retire.len())
            .and_then(|entries| entries.checked_mul(size_of::<Retired>()))
            .and_then(|bytes| bytes.checked_mul(2))
            .ok_or(WorkspaceError::Capacity)?;
        let required = growth.max(state.retired.capacity() * size_of::<Retired>());
        state.retired_charge.resize(required)?;
        state
            .retired
            .try_reserve_exact(retire.len())
            .map_err(|_| WorkspaceError::Capacity)?;
        let actual = state.retired.capacity() * size_of::<Retired>();
        state.retired_charge.resize(actual.max(required))?;
        state.retired.extend(retire);
        state.root = self.root;
        state.revision = next;
        state.pending = false;
        self.finished = true;
        Ok(next)
    }

    pub fn abort(mut self) -> Result<(), WorkspaceError> {
        for page in self.created.iter().rev() {
            self.index.store.release(*page)?;
        }
        self.index
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .pending = false;
        self.finished = true;
        Ok(())
    }
}

impl IndexSnapshot {
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, WorkspaceError> {
        self.index.get_at(self.root, key)
    }
    pub fn floor(&self, key: &[u8]) -> Result<Option<IndexEntry>, WorkspaceError> {
        self.root
            .map(|root| self.index.floor_node(root, key, 0))
            .transpose()
            .map(Option::flatten)
    }
    pub fn scan(
        &self,
        lower: &[u8],
        upper: &[u8],
        limit: usize,
    ) -> Result<ScanPage, WorkspaceError> {
        self.index.scan_at(self.root, lower, upper, limit)
    }
    pub fn root(&self) -> Option<PageRef> {
        self.root
    }
    pub fn release(mut self) -> Result<(), WorkspaceError> {
        let mut state = self.index.state.lock().map_err(|_| WorkspaceError::Io)?;
        let count = state
            .frozen
            .get_mut(&self.revision)
            .ok_or(WorkspaceError::Io)?;
        *count -= 1;
        if *count == 0 {
            state.frozen.remove(&self.revision);
        }
        if self.captured {
            state.captured -= 1;
        }
        self.released = true;
        drop(state);
        self.index.maintain()?;
        Ok(())
    }
}
