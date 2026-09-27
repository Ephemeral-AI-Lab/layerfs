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
const MAX_FROZEN: usize = 32;

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
    generation: u64,
    released: bool,
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
                frozen: BTreeMap::new(),
                retired: Vec::new(),
                retired_charge: budget.reserve(0)?,
            }),
            _charge: budget.reserve(4096 + MAX_FROZEN * 64)?,
        }))
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

    fn split(
        &self,
        node: Node,
        generation: u64,
        revision: u64,
        created: &mut Vec<PageRef>,
    ) -> Result<Vec<Child>, WorkspaceError> {
        if node.body_len() <= BODY_BYTES {
            return Ok(vec![self.write(&node, generation, revision, created)?]);
        }
        if node.len() < 2 {
            return Err(WorkspaceError::Capacity);
        }
        let target = node.body_len() / 2;
        let middle = match &node {
            Node::Leaf(cells) => {
                let mut bytes = 0;
                cells
                    .iter()
                    .position(|cell| {
                        bytes += 4 + cell.key.len() + cell.value.len();
                        bytes >= target
                    })
                    .unwrap_or(0)
                    + 1
            }
            Node::Branch(children) => {
                let mut bytes = 0;
                children
                    .iter()
                    .position(|child| {
                        bytes += 2 + child.max.len() + 16;
                        bytes >= target
                    })
                    .unwrap_or(0)
                    + 1
            }
        }
        .clamp(1, node.len() - 1);
        let (left, right) = match node {
            Node::Leaf(mut cells) => {
                let right = cells.split_off(middle);
                (Node::Leaf(cells), Node::Leaf(right))
            }
            Node::Branch(mut children) => {
                let right = children.split_off(middle);
                (Node::Branch(children), Node::Branch(right))
            }
        };
        if left.body_len() > BODY_BYTES || right.body_len() > BODY_BYTES {
            return Err(WorkspaceError::Capacity);
        }
        Ok(vec![
            self.write(&left, generation, revision, created)?,
            self.write(&right, generation, revision, created)?,
        ])
    }

    fn change(
        &self,
        old: Option<PageRef>,
        key: &[u8],
        value: Option<&[u8]>,
        depth: usize,
        context: &mut ChangeContext,
    ) -> Result<Vec<Child>, WorkspaceError> {
        if depth > MAX_LEVEL {
            return Err(WorkspaceError::Capacity);
        }
        let Some(reference) = old else {
            return match value {
                Some(value) => self.split(
                    Node::Leaf(vec![Cell {
                        key: key.to_vec(),
                        value: value.to_vec(),
                    }]),
                    context.generation,
                    context.revision,
                    &mut context.created,
                ),
                None => Ok(Vec::new()),
            };
        };
        let node = self.node(reference)?;
        let replacement = match node {
            Node::Leaf(mut cells) => {
                match cells.binary_search_by(|cell| cell.key.as_slice().cmp(key)) {
                    Ok(at) => match value {
                        Some(value) => cells[at].value = value.to_vec(),
                        None => {
                            cells.remove(at);
                        }
                    },
                    Err(at) => {
                        if let Some(value) = value {
                            cells.insert(
                                at,
                                Cell {
                                    key: key.to_vec(),
                                    value: value.to_vec(),
                                },
                            );
                        } else {
                            return Ok(vec![Child {
                                max: cells.last().ok_or(WorkspaceError::Io)?.key.clone(),
                                page: reference,
                            }]);
                        }
                    }
                }
                if cells.is_empty() {
                    Vec::new()
                } else {
                    self.split(
                        Node::Leaf(cells),
                        context.generation,
                        context.revision,
                        &mut context.created,
                    )?
                }
            }
            Node::Branch(mut children) => {
                let at = children
                    .iter()
                    .position(|child| child.max.as_slice() >= key)
                    .unwrap_or(children.len() - 1);
                let child = children[at].page;
                let changed = self.change(Some(child), key, value, depth + 1, context)?;
                children.splice(at..=at, changed);
                match children.len() {
                    0 => Vec::new(),
                    1 => children,
                    _ => self.split(
                        Node::Branch(children),
                        context.generation,
                        context.revision,
                        &mut context.created,
                    )?,
                }
            }
        };
        context.replaced.push(reference);
        Ok(replacement)
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, WorkspaceError> {
        let root = self.state.lock().map_err(|_| WorkspaceError::Io)?.root;
        self.get_at(root, key)
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

    /// Updates are sorted and unique. A candidate is unobservable until the
    /// surrounding Workspace publication accepts it.
    pub fn prepare(
        self: &Arc<Self>,
        updates: &[(Vec<u8>, Option<Vec<u8>>)],
    ) -> Result<IndexCandidate, WorkspaceError> {
        if updates.is_empty()
            || updates.len() > 256
            || updates.windows(2).any(|pair| pair[0].0 >= pair[1].0)
            || updates.iter().any(|(key, value)| {
                key.is_empty()
                    || key.len() > 256
                    || value.as_ref().is_some_and(|value| value.len() > 512)
            })
        {
            return Err(WorkspaceError::InvalidInput);
        }
        let scratch = self.store.budget().reserve(128 * 1024)?;
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
        let next_revision = revision + 1;
        let mut new_root = root;
        let mut context = ChangeContext {
            generation,
            revision: next_revision,
            created: Vec::new(),
            replaced: Vec::new(),
        };
        let result = (|| {
            for (key, value) in updates {
                let mut changed = self.change(new_root, key, value.as_deref(), 0, &mut context)?;
                new_root = match changed.len() {
                    0 => None,
                    1 => Some(changed.remove(0).page),
                    _ => {
                        if let Some(root) = new_root {
                            if self.height(root)? >= MAX_LEVEL {
                                return Err(WorkspaceError::Capacity);
                            }
                        }
                        let branch = Node::Branch(changed);
                        Some(
                            self.write(&branch, generation, next_revision, &mut context.created)?
                                .page,
                        )
                    }
                };
            }
            Ok::<(), WorkspaceError>(())
        })();
        if let Err(error) = result {
            let mut cleanup = Ok(());
            for page in context.created.into_iter().rev() {
                if let Err(failure) = self.store.release(page) {
                    cleanup = Err(failure);
                }
            }
            let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            state.pending = false;
            if cleanup.is_err() {
                state.stopped = true;
            }
            return Err(cleanup.err().unwrap_or(error));
        }
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

    pub fn capture(self: &Arc<Self>) -> Result<IndexSnapshot, WorkspaceError> {
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.pending || state.stopped || state.frozen.len() == MAX_FROZEN {
            return Err(WorkspaceError::Busy);
        }
        let generation = state.generation;
        state.generation = generation.checked_add(1).ok_or(WorkspaceError::Capacity)?;
        *state.frozen.entry(generation).or_default() += 1;
        Ok(IndexSnapshot {
            index: self.clone(),
            root: state.root,
            generation,
            released: false,
        })
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
        let generation = state.generation;
        let mut retire = Vec::with_capacity(self.replaced.len());
        for reference in &self.replaced {
            let page = self
                .index
                .store
                .read(*reference, self.index.store.kind(*reference)?)?;
            retire.push(Retired {
                page: *reference,
                birth: page.generation(),
                retired: generation,
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
    pub fn root(&self) -> Option<PageRef> {
        self.root
    }
    pub fn release(mut self) -> Result<(), WorkspaceError> {
        let mut state = self.index.state.lock().map_err(|_| WorkspaceError::Io)?;
        let count = state
            .frozen
            .get_mut(&self.generation)
            .ok_or(WorkspaceError::Io)?;
        *count -= 1;
        if *count == 0 {
            state.frozen.remove(&self.generation);
        }
        self.released = true;
        drop(state);
        self.index.maintain()?;
        Ok(())
    }
}
