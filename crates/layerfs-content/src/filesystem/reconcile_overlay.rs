//! Reconciliation choices share one private final-state overlay and one tree flush.
use super::delta_spool::Run;
use super::reconcile::{ReconcileChoice, ReconcileConflict, ReconcileConflictKind};
use super::reconcile_delta::ReconcileBudget;
use super::resolve::namespace;
use crate::object::access::{ObjectRead, ObjectStore};
use crate::tree::directory::{
    directory_apply_sorted_with_spill, directory_lookup, directory_page_after, DirectoryStateRoot,
    NamespaceCounters,
};
use crate::tree::inode::codec::{decode_inode_record, encode_inode_record};
use crate::tree::inode::{
    inode_table_apply_sorted_with_spill, inode_table_lookup_with_budget, InodeId, InodeKind,
    InodeRecordV1, InodeTableCounters, InodeTableRoot,
};
use crate::{CanonicalName, CanonicalPath, CoreError, CoreResult, ObjectId};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};

const NODE: usize = 114;
const EDGE: usize = 330;
const EDGE_KEY: usize = 295;
const PATH_FRAME: usize = 4387;

// Each sorted run has unique keys. Lower levels are newer; compaction chooses
// the newer value without emitting any canonical tree or retaining old payloads.
struct Map<const N: usize, const K: usize> {
    pending: Vec<[u8; N]>,
    levels: [Option<Run<N>>; 32],
    budget: u64,
    dir: std::path::PathBuf,
}
impl<const N: usize, const K: usize> Map<N, K> {
    fn new(budget: ReconcileBudget<'_>) -> CoreResult<Self> {
        let mut pending = Vec::new();
        let capacity = ((budget.memory_bytes / 16).min(64 * 1024) / N as u64) as usize;
        if capacity == 0 {
            return Err(CoreError::ObjectLimitExceeded);
        }
        pending
            .try_reserve_exact(capacity)
            .map_err(|_| CoreError::AllocationFailed)?;
        Ok(Self {
            pending,
            levels: std::array::from_fn(|_| None),
            budget: budget.spool_bytes / 4,
            dir: budget.scratch_dir.to_owned(),
        })
    }
    fn get(&self, key: &[u8]) -> CoreResult<Option<[u8; N]>> {
        if let Ok(at) = self.pending.binary_search_by(|row| row[..K].cmp(key)) {
            return Ok(Some(self.pending[at]));
        }
        for run in self.levels.iter().flatten() {
            if let Some(row) = lookup::<N, K>(run, key)? {
                return Ok(Some(row));
            }
        }
        Ok(None)
    }
    fn next(&self, prefix: &[u8], after: Option<&[u8]>) -> CoreResult<Option<[u8; N]>> {
        let mut best = None::<[u8; N]>;
        let at = self.pending.partition_point(|row| {
            after.map_or(row[..prefix.len()] < *prefix, |after| row[..K] <= *after)
        });
        if let Some(row) = self
            .pending
            .get(at)
            .filter(|row| row[..prefix.len()] == *prefix)
        {
            best = Some(*row);
        }
        for run in self.levels.iter().flatten() {
            let (mut lo, mut hi) = (0, run.count);
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                let row = run.at(mid)?;
                if after.map_or(row[..prefix.len()] < *prefix, |after| row[..K] <= *after) {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }
            if lo < run.count {
                let row = run.at(lo)?;
                if row[..prefix.len()] == *prefix && best.is_none_or(|best| row[..K] < best[..K]) {
                    best = Some(row);
                }
            }
        }
        best.map(|row| {
            self.get(&row[..K])
                .and_then(|row| row.ok_or(CoreError::MissingObject))
        })
        .transpose()
    }
    fn put(&mut self, row: [u8; N]) -> CoreResult<()> {
        if let Ok(at) = self.pending.binary_search_by(|old| old[..K].cmp(&row[..K])) {
            self.pending[at] = row;
            return Ok(());
        }
        if self.pending.len() == self.pending.capacity() {
            self.flush()?;
        }
        let disk = self.levels.iter().flatten().try_fold(0u64, |n, r| {
            n.checked_add(r.count * N as u64)
                .ok_or(CoreError::LengthOverflow)
        })?;
        if disk
            .checked_add((self.pending.len() as u64 + 1) * N as u64)
            .and_then(|n| n.checked_mul(2))
            .is_none_or(|n| n > self.budget)
        {
            return Err(CoreError::ObjectLimitExceeded);
        }
        let at = self
            .pending
            .binary_search_by(|old| old[..K].cmp(&row[..K]))
            .unwrap_err();
        self.pending.insert(at, row);
        Ok(())
    }
    fn merge(dir: &std::path::Path, mut old: Run<N>, mut new: Run<N>) -> CoreResult<Run<N>> {
        old.rewind()?;
        new.rewind()?;
        let mut out = Run::create(dir)?;
        let (mut a, mut b) = (old.next()?, new.next()?);
        while a.is_some() || b.is_some() {
            match (a, b) {
                (Some(left), Some(right)) if left[..K] < right[..K] => {
                    out.push(&left)?;
                    a = old.next()?;
                }
                (Some(left), Some(right)) => {
                    out.push(&right)?;
                    b = new.next()?;
                    if left[..K] == right[..K] {
                        a = old.next()?;
                    }
                }
                (Some(left), None) => {
                    out.push(&left)?;
                    a = old.next()?;
                }
                (None, Some(right)) => {
                    out.push(&right)?;
                    b = new.next()?;
                }
                (None, None) => break,
            }
        }
        old.remove()?;
        new.remove()?;
        out.rewind()?;
        Ok(out)
    }
    fn flush(&mut self) -> CoreResult<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let mut run = Run::create(&self.dir)?;
        for row in &self.pending {
            run.push(row)?;
        }
        self.pending.clear();
        run.rewind()?;
        for level in &mut self.levels {
            match level.take() {
                None => {
                    *level = Some(run);
                    return Ok(());
                }
                Some(old) => run = Self::merge(&self.dir, old, run)?,
            }
        }
        Err(CoreError::ObjectLimitExceeded)
    }
    fn finish(mut self) -> CoreResult<Run<N>> {
        self.flush()?;
        let mut result = None;
        for level in &mut self.levels {
            if let Some(old) = level.take() {
                result = Some(match result {
                    None => old,
                    Some(new) => Self::merge(&self.dir, old, new)?,
                });
            }
        }
        let mut run = match result {
            Some(run) => run,
            None => Run::create(&self.dir)?,
        };
        run.rewind()?;
        Ok(run)
    }
}
fn lookup<const N: usize, const K: usize>(run: &Run<N>, key: &[u8]) -> CoreResult<Option<[u8; N]>> {
    let (mut lo, mut hi) = (0, run.count);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if run.at(mid)?[..K] < *key {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if lo == run.count {
        return Ok(None);
    }
    let row = run.at(lo)?;
    Ok((row[..K] == *key).then_some(row))
}
#[derive(Clone, Copy)]
struct State {
    inode: InodeId,
    record: InodeRecordV1,
    generation: u64,
}
impl State {
    fn encode(self) -> [u8; NODE] {
        let mut row = [0; NODE];
        row[..32].copy_from_slice(self.inode.as_bytes());
        row[32] = 1;
        row[33..41].copy_from_slice(&self.record.namespace_ref_count.to_be_bytes());
        row[41] = self.record.kind as u8;
        row[42..74].copy_from_slice(self.record.content_root.as_bytes());
        row[74..106].copy_from_slice(self.record.metadata_root.as_bytes());
        row[106..].copy_from_slice(&self.generation.to_be_bytes());
        row
    }
    fn decode(row: [u8; NODE]) -> CoreResult<Self> {
        Ok(Self {
            inode: InodeId::from_slice(&row[..32])?,
            generation: u64::from_be_bytes(row[106..].try_into().unwrap()),
            record: InodeRecordV1 {
                namespace_ref_count: u64::from_be_bytes(row[33..41].try_into().unwrap()),
                kind: match row[41] {
                    1 => InodeKind::RegularFile,
                    2 => InodeKind::Directory,
                    3 => InodeKind::Symlink,
                    _ => return Err(CoreError::InvalidRecord("reconciliation inode kind")),
                },
                content_root: ObjectId::from_bytes(&row[42..74])?,
                metadata_root: ObjectId::from_bytes(&row[74..106])?,
            },
        })
    }
}
fn load(
    store: &impl ObjectRead,
    table: InodeTableRoot,
    inode: InodeId,
) -> CoreResult<Option<State>> {
    inode_table_lookup_with_budget(
        store,
        table,
        inode,
        32 * 1024,
        &mut InodeTableCounters::default(),
    )?
    .0
    .map(|id| {
        store
            .with_authenticated_canonical(id, decode_inode_record)
            .map(|record| State {
                inode,
                record,
                generation: 0,
            })
    })
    .transpose()
}
fn edge_key(parent: State, name: &CanonicalName) -> [u8; EDGE_KEY] {
    let mut key = [0; EDGE_KEY];
    key[..32].copy_from_slice(parent.inode.as_bytes());
    key[32..40].copy_from_slice(&parent.generation.to_be_bytes());
    key[40..40 + name.as_bytes().len()].copy_from_slice(name.as_bytes());
    key
}
fn edge_name(row: &[u8; EDGE]) -> CoreResult<CanonicalName> {
    let len = u16::from_be_bytes(row[295..297].try_into().unwrap()) as usize;
    if len > 255 {
        return Err(CoreError::InvalidRecord("reconciliation edge name length"));
    }
    CanonicalName::from_bytes(&row[40..40 + len])
}
fn edge_inode(row: &[u8; EDGE]) -> CoreResult<Option<InodeId>> {
    if row[297] == 0 {
        Ok(None)
    } else {
        InodeId::from_slice(&row[298..]).map(Some)
    }
}

struct Paths {
    file: File,
    bytes: u64,
    limit: u64,
}
impl Paths {
    fn new(budget: ReconcileBudget<'_>) -> CoreResult<Self> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = budget.scratch_dir.join(format!(
            "reconcile-paths-{}",
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|_| CoreError::Io)?;
        std::fs::remove_file(path).map_err(|_| CoreError::Io)?;
        Ok(Self {
            file,
            bytes: 0,
            limit: budget.spool_bytes / 8,
        })
    }
    fn push(&mut self, path: &CanonicalPath) -> CoreResult<()> {
        let n = path.as_bytes().len();
        let next = self
            .bytes
            .checked_add(n as u64 + 2)
            .ok_or(CoreError::LengthOverflow)?;
        if next > self.limit {
            return Err(CoreError::ObjectLimitExceeded);
        }
        self.file
            .write_all(&(n as u16).to_be_bytes())
            .map_err(|_| CoreError::Io)?;
        self.file
            .write_all(path.as_bytes())
            .map_err(|_| CoreError::Io)?;
        self.bytes = next;
        Ok(())
    }
    fn rewind(&mut self) -> CoreResult<()> {
        self.file
            .seek(SeekFrom::Start(0))
            .map(|_| ())
            .map_err(|_| CoreError::Io)
    }
    fn next(&mut self) -> CoreResult<Option<CanonicalPath>> {
        let mut len = [0; 2];
        if self.file.read(&mut len[..1]).map_err(|_| CoreError::Io)? == 0 {
            return Ok(None);
        }
        self.file
            .read_exact(&mut len[1..])
            .map_err(|_| CoreError::Io)?;
        let n = u16::from_be_bytes(len) as usize;
        if n > 4096 {
            return Err(CoreError::PathLimitExceeded);
        }
        let mut bytes = [0; 4096];
        self.file
            .read_exact(&mut bytes[..n])
            .map_err(|_| CoreError::Io)?;
        CanonicalPath::from_bytes(&bytes[..n]).map(Some)
    }
}

struct Overlay<'a> {
    base: ObjectId,
    table: InodeTableRoot,
    root: InodeId,
    nodes: Map<NODE, 32>,
    edges: Map<EDGE, EDGE_KEY>,
    generation: u64,
    budget: ReconcileBudget<'a>,
}
impl<'a> Overlay<'a> {
    fn new(
        store: &impl ObjectRead,
        base: ObjectId,
        budget: ReconcileBudget<'a>,
    ) -> CoreResult<Self> {
        let ns = namespace(store, base)?;
        if budget.memory_bytes < 256 * 1024 {
            return Err(CoreError::ObjectLimitExceeded);
        }
        Ok(Self {
            base,
            table: InodeTableRoot(ns.inode_table_root),
            root: ns.root_directory_inode,
            nodes: Map::new(budget)?,
            edges: Map::new(budget)?,
            generation: 0,
            budget,
        })
    }
    fn state(&self, store: &impl ObjectRead, inode: InodeId) -> CoreResult<Option<State>> {
        match self.nodes.get(inode.as_bytes())? {
            Some(row) => State::decode(row).map(Some),
            None => load(store, self.table, inode),
        }
    }
    fn binding(
        &self,
        store: &impl ObjectRead,
        parent: State,
        name: &CanonicalName,
    ) -> CoreResult<Option<InodeId>> {
        if let Some(row) = self.edges.get(&edge_key(parent, name))? {
            edge_inode(&row)
        } else {
            directory_lookup(
                store,
                DirectoryStateRoot(parent.record.content_root),
                name,
                &mut NamespaceCounters::default(),
            )
        }
    }
    fn next_child(
        &self,
        store: &impl ObjectRead,
        parent: State,
        mut after: Option<CanonicalName>,
        source: bool,
    ) -> CoreResult<Option<(CanonicalName, InodeId)>> {
        loop {
            let base = directory_page_after(
                store,
                DirectoryStateRoot(parent.record.content_root),
                after.as_ref(),
                1,
                8192,
                &mut NamespaceCounters::default(),
            )?
            .entries
            .into_iter()
            .next();
            let overlay = if source {
                None
            } else {
                let mut prefix = [0; 40];
                prefix[..32].copy_from_slice(parent.inode.as_bytes());
                prefix[32..].copy_from_slice(&parent.generation.to_be_bytes());
                let key = after.as_ref().map(|name| edge_key(parent, name));
                self.edges
                    .next(&prefix, key.as_ref().map(|key| key.as_slice()))?
            };
            match (base, overlay) {
                (None, None) => return Ok(None),
                (Some(base), None) => return Ok(Some(base)),
                (base, Some(row)) => {
                    let name = edge_name(&row)?;
                    if let Some(base) = base.filter(|(base, _)| base < &name) {
                        return Ok(Some(base));
                    }
                    if let Some(inode) = edge_inode(&row)? {
                        return Ok(Some((name, inode)));
                    }
                    after = Some(name);
                }
            }
        }
    }
    fn resolve(&self, store: &impl ObjectRead, path: &CanonicalPath) -> CoreResult<State> {
        let mut node = self
            .state(store, self.root)?
            .ok_or(CoreError::MissingObject)?;
        for name in path.components() {
            if node.record.kind != InodeKind::Directory {
                return Err(CoreError::InvalidRecord(
                    "path component is not a directory",
                ));
            }
            let inode = self
                .binding(store, node, &CanonicalName::from_bytes(name)?)?
                .ok_or(CoreError::MissingObject)?;
            node = self.state(store, inode)?.ok_or(CoreError::MissingObject)?;
        }
        Ok(node)
    }
    fn push_task(&self, run: &mut Run<33>, inode: InodeId, add: bool) -> CoreResult<()> {
        if (run.count + 1)
            .checked_mul(33)
            .is_none_or(|n| n > self.budget.spool_bytes / 8)
        {
            return Err(CoreError::ObjectLimitExceeded);
        }
        let mut row = [0; 33];
        row[..32].copy_from_slice(inode.as_bytes());
        row[32] = u8::from(add);
        run.push(&row).map_err(Into::into)
    }
    fn children(
        &self,
        store: &impl ObjectRead,
        parent: State,
        source: bool,
        tasks: &mut Run<33>,
    ) -> CoreResult<()> {
        let mut after = None;
        while let Some((name, child)) = self.next_child(store, parent, after, source)? {
            self.push_task(tasks, child, true)?;
            after = Some(name);
        }
        Ok(())
    }
    fn release_tasks(&mut self, store: &impl ObjectRead, mut tasks: Run<33>) -> CoreResult<()> {
        while tasks.count != 0 {
            let row = tasks.at(tasks.count - 1)?;
            tasks.truncate(tasks.count - 1)?;
            let inode = InodeId::from_slice(&row[..32])?;
            let mut node = self.state(store, inode)?.ok_or(CoreError::MissingObject)?;
            node.record.namespace_ref_count = node
                .record
                .namespace_ref_count
                .checked_sub(1)
                .ok_or(CoreError::InvalidRecord("namespace reference count"))?;
            if node.record.namespace_ref_count == 0 && node.record.kind == InodeKind::Directory {
                self.children(store, node, false, &mut tasks)?;
            }
            self.nodes.put(node.encode())?;
        }
        tasks.remove().map_err(Into::into)
    }
    fn release(&mut self, store: &impl ObjectRead, inode: InodeId) -> CoreResult<()> {
        let mut tasks = Run::create(self.budget.scratch_dir)?;
        self.push_task(&mut tasks, inode, false)?;
        self.release_tasks(store, tasks)
    }
    fn copy(
        &mut self,
        store: &impl ObjectRead,
        source: InodeTableRoot,
        inode: InodeId,
        add: bool,
    ) -> CoreResult<()> {
        let mut tasks = Run::create(self.budget.scratch_dir)?;
        self.push_task(&mut tasks, inode, add)?;
        while tasks.count != 0 {
            let row = tasks.at(tasks.count - 1)?;
            tasks.truncate(tasks.count - 1)?;
            let inode = InodeId::from_slice(&row[..32])?;
            let old = self.state(store, inode)?;
            let live = old
                .is_some_and(|state| state.record.namespace_ref_count != 0 || inode == self.root);
            if let Some(old) = old.filter(|state| live && state.record.kind == InodeKind::Directory)
            {
                let mut removed = Run::create(self.budget.scratch_dir)?;
                self.children(store, old, false, &mut removed)?;
                self.release_tasks(store, removed)?;
            }
            let mut node = load(store, source, inode)?.ok_or(CoreError::MissingObject)?;
            node.record.namespace_ref_count = old
                .map_or(0, |old| old.record.namespace_ref_count)
                .checked_add(u64::from(row[32] != 0))
                .ok_or(CoreError::LengthOverflow)?;
            self.generation = self
                .generation
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
            node.generation = self.generation;
            self.nodes.put(node.encode())?;
            if node.record.kind == InodeKind::Directory {
                self.children(store, node, true, &mut tasks)?;
            }
        }
        tasks.remove().map_err(Into::into)
    }
    fn replace_path(
        &mut self,
        store: &impl ObjectRead,
        source_root: ObjectId,
        mut path: CanonicalPath,
    ) -> CoreResult<()> {
        let source = namespace(store, source_root)?;
        let current = namespace(store, self.base)?;
        if source.profile_id != current.profile_id || source.root_directory_inode != self.root {
            return Err(CoreError::InvalidRecord("namespace identity mismatch"));
        }
        let (parent, name) = loop {
            if path.is_root() {
                *self = Self::new(store, source_root, self.budget)?;
                return Ok(());
            }
            let parent = super::reconcile::parent_path(&path)?;
            let name = CanonicalName::from_bytes(
                path.components().last().ok_or(CoreError::RootMutation)?,
            )?;
            match self.resolve(store, &parent) {
                Ok(node) if node.record.kind == InodeKind::Directory => break (node, name),
                Ok(_) | Err(CoreError::MissingObject | CoreError::InvalidRecord(_)) => {
                    path = parent
                }
                Err(error) => return Err(error),
            }
        };
        let old = self.binding(store, parent, &name)?;
        let desired = super::reconcile::lookup_path_inode(store, source_root, &path)?;
        if old != desired {
            if let Some(inode) = old {
                self.release(store, inode)?;
            }
        }
        if let Some(inode) = desired {
            self.copy(
                store,
                InodeTableRoot(source.inode_table_root),
                inode,
                old != desired,
            )?;
        }
        if old != desired {
            let mut row = [0; EDGE];
            row[..EDGE_KEY].copy_from_slice(&edge_key(parent, &name));
            row[295..297].copy_from_slice(&(name.as_bytes().len() as u16).to_be_bytes());
            if let Some(inode) = desired {
                row[297] = 1;
                row[298..].copy_from_slice(inode.as_bytes());
            }
            self.edges.put(row)?;
        }
        Ok(())
    }
    fn aliases(
        &self,
        store: &impl ObjectRead,
        target: InodeId,
        source: Option<ObjectId>,
        paths: &mut Paths,
    ) -> CoreResult<()> {
        let ns = namespace(store, source.unwrap_or(self.base))?;
        let table = source.map(|_| InodeTableRoot(ns.inode_table_root));
        let mut stack = Run::<PATH_FRAME>::create(self.budget.scratch_dir)?;
        let mut initial = [0; PATH_FRAME];
        initial[..32].copy_from_slice(ns.root_directory_inode.as_bytes());
        stack.push(&initial)?;
        if target == ns.root_directory_inode {
            paths.push(&CanonicalPath::root())?;
        }
        while stack.count != 0 {
            let mut row = stack.at(stack.count - 1)?;
            let inode = InodeId::from_slice(&row[..32])?;
            let node = match table {
                Some(table) => load(store, table, inode)?,
                None => self.state(store, inode)?,
            }
            .ok_or(CoreError::MissingObject)?;
            let path_len = u16::from_be_bytes(row[32..34].try_into().unwrap()) as usize;
            let cursor_len = u16::from_be_bytes(row[4130..4132].try_into().unwrap()) as usize;
            if path_len > 4096 || cursor_len > 255 {
                return Err(CoreError::InvalidRecord("reconciliation path frame"));
            }
            let cursor = if cursor_len == 0 {
                None
            } else {
                Some(CanonicalName::from_bytes(&row[4132..4132 + cursor_len])?)
            };
            let Some((name, child)) = self.next_child(store, node, cursor, source.is_some())?
            else {
                stack.truncate(stack.count - 1)?;
                continue;
            };
            row[4130..4132].copy_from_slice(&(name.as_bytes().len() as u16).to_be_bytes());
            row[4132..].fill(0);
            row[4132..4132 + name.as_bytes().len()].copy_from_slice(name.as_bytes());
            stack.truncate(stack.count - 1)?;
            stack.push(&row)?;
            let next_len = path_len + usize::from(path_len != 0) + name.as_bytes().len();
            if next_len > 4096 {
                return Err(CoreError::PathLimitExceeded);
            }
            let mut next = [0; PATH_FRAME];
            next[..32].copy_from_slice(child.as_bytes());
            next[32..34].copy_from_slice(&(next_len as u16).to_be_bytes());
            next[34..34 + path_len].copy_from_slice(&row[34..34 + path_len]);
            let offset = 34 + path_len;
            if path_len != 0 {
                next[offset] = b'/';
            }
            next[offset + usize::from(path_len != 0)..34 + next_len]
                .copy_from_slice(name.as_bytes());
            if child == target {
                paths.push(&CanonicalPath::from_bytes(&next[34..34 + next_len])?)?;
            }
            let child_node = match table {
                Some(table) => load(store, table, child)?,
                None => self.state(store, child)?,
            }
            .ok_or(CoreError::MissingObject)?;
            if child_node.record.kind == InodeKind::Directory {
                for i in 0..stack.count {
                    if stack.at(i)?[..32] == *child.as_bytes() {
                        return Err(CoreError::InvalidRecord("directory cycle"));
                    }
                }
                if (stack.count + 1)
                    .checked_mul(PATH_FRAME as u64)
                    .is_none_or(|n| n > self.budget.spool_bytes / 8)
                {
                    return Err(CoreError::ObjectLimitExceeded);
                }
                stack.push(&next)?;
            }
        }
        stack.remove().map_err(Into::into)
    }
    fn finish<S: ObjectStore>(self, store: &mut S) -> CoreResult<ObjectId> {
        let ns = namespace(store, self.base)?;
        let mut nodes = self.nodes.finish()?;
        let mut edges = self.edges.finish()?;
        let mut directories = Run::<NODE>::create(self.budget.scratch_dir)?;
        let mut next = edges.next()?;
        let scratch = self.budget.memory_bytes.saturating_sub(256 * 1024) as usize;
        while let Some(first) = next {
            let inode = InodeId::from_slice(&first[..32])?;
            let node = match lookup::<NODE, 32>(&nodes, inode.as_bytes())? {
                Some(row) => State::decode(row)?,
                None => load(store, self.table, inode)?.ok_or(CoreError::MissingObject)?,
            };
            let input = std::iter::from_fn(|| loop {
                let row = next.filter(|row| row[..32] == *inode.as_bytes())?;
                match edges.next() {
                    Ok(row) => next = row,
                    Err(error) => {
                        next = None;
                        return Some(Err(error.into()));
                    }
                }
                if node.record.kind != InodeKind::Directory
                    || (node.record.namespace_ref_count == 0 && inode != self.root)
                    || u64::from_be_bytes(row[32..40].try_into().unwrap()) != node.generation
                {
                    continue;
                }
                return Some((|| Ok((edge_name(&row)?, edge_inode(&row)?)))());
            });
            if node.record.kind == InodeKind::Directory
                && (node.record.namespace_ref_count != 0 || inode == self.root)
            {
                let (root, _) = directory_apply_sorted_with_spill(
                    store,
                    DirectoryStateRoot(node.record.content_root),
                    input,
                    scratch,
                    self.budget.scratch_dir,
                    self.budget.spool_bytes / 8,
                )?;
                let mut node = node;
                node.record.content_root = root.0;
                if (directories.count + 1) * NODE as u64 > self.budget.spool_bytes / 8 {
                    return Err(CoreError::ObjectLimitExceeded);
                }
                directories.push(&node.encode())?;
            } else {
                for row in input {
                    row?;
                }
            }
        }
        edges.remove()?;
        nodes.rewind()?;
        directories.rewind()?;
        let (mut node, mut directory) = (nodes.next()?, directories.next()?);
        let mut pairs = Run::<65>::create(self.budget.scratch_dir)?;
        while node.is_some() || directory.is_some() {
            let row = match (node, directory) {
                (Some(a), Some(b)) if a[..32] < b[..32] => {
                    node = nodes.next()?;
                    a
                }
                (Some(a), Some(b)) => {
                    directory = directories.next()?;
                    if a[..32] == b[..32] {
                        node = nodes.next()?;
                    }
                    b
                }
                (Some(a), None) => {
                    node = nodes.next()?;
                    a
                }
                (None, Some(b)) => {
                    directory = directories.next()?;
                    b
                }
                _ => break,
            };
            let state = State::decode(row)?;
            let before = load(store, self.table, state.inode)?;
            let present = state.record.namespace_ref_count != 0 || state.inode == self.root;
            if before.is_none() && !present
                || before.is_some_and(|old| present && old.record == state.record)
            {
                continue;
            }
            let mut pair = [0; 65];
            pair[..32].copy_from_slice(state.inode.as_bytes());
            if present {
                state.record.validate(state.inode == self.root)?;
                let id = store.put_owned(encode_inode_record(state.record)?)?;
                pair[32] = 1;
                pair[33..].copy_from_slice(id.as_bytes());
            }
            if (pairs.count + 1) * 65 > self.budget.spool_bytes / 8 {
                return Err(CoreError::ObjectLimitExceeded);
            }
            pairs.push(&pair)?;
        }
        nodes.remove()?;
        directories.remove()?;
        pairs.rewind()?;
        let input = std::iter::from_fn(|| match pairs.next() {
            Ok(Some(row)) => Some((|| {
                Ok((
                    InodeId::from_slice(&row[..32])?,
                    if row[32] == 0 {
                        None
                    } else {
                        Some(ObjectId::from_bytes(&row[33..])?)
                    },
                ))
            })()),
            Ok(None) => None,
            Err(error) => Some(Err(error.into())),
        });
        let (table, _) = inode_table_apply_sorted_with_spill(
            store,
            self.table,
            input,
            scratch,
            self.budget.scratch_dir,
            self.budget.spool_bytes / 8,
        )?;
        pairs.remove()?;
        store.put_owned(crate::tree::directory::codec::encode_namespace_root(
            crate::tree::NamespaceRootV1 {
                inode_table_root: table.0,
                ..ns
            },
        )?)
    }
}

pub fn replace_choices_from_snapshots_bounded<S: ObjectStore>(
    store: &mut S,
    working: ObjectId,
    branch: ObjectId,
    layer: ObjectId,
    conflicts: &[ReconcileConflict],
    choices: &[ReconcileChoice],
    budget: ReconcileBudget<'_>,
) -> CoreResult<ObjectId> {
    if conflicts.len() != choices.len() {
        return Err(CoreError::InvalidRecord("reconciliation choice count"));
    }
    if choices
        .iter()
        .all(|choice| *choice == ReconcileChoice::WorkingTree)
    {
        return Ok(working);
    }
    let mut overlay = Overlay::new(store, working, budget)?;
    for (conflict, choice) in conflicts.iter().zip(choices) {
        let source = match choice {
            ReconcileChoice::Branch => branch,
            ReconcileChoice::Layer => layer,
            ReconcileChoice::WorkingTree => continue,
        };
        let mut paths = Paths::new(budget)?;
        for path in &conflict.affected_paths {
            paths.push(path)?;
        }
        if conflict.kind == ReconcileConflictKind::HardLink {
            overlay.aliases(store, conflict.inode, None, &mut paths)?;
            overlay.aliases(store, conflict.inode, Some(source), &mut paths)?;
        }
        paths.rewind()?;
        while let Some(path) = paths.next()? {
            overlay.replace_path(store, source, path)?;
        }
    }
    overlay.finish(store)
}

pub fn replace_paths_from_snapshot_bounded<S: ObjectStore>(
    store: &mut S,
    destination: ObjectId,
    source: ObjectId,
    paths: &[CanonicalPath],
    budget: ReconcileBudget<'_>,
) -> CoreResult<ObjectId> {
    let before = namespace(store, destination)?;
    let selected = namespace(store, source)?;
    if before.profile_id != selected.profile_id
        || before.root_directory_inode != selected.root_directory_inode
    {
        return Err(CoreError::InvalidRecord("namespace identity mismatch"));
    }
    if paths.is_empty() {
        return Ok(destination);
    }
    let mut overlay = Overlay::new(store, destination, budget)?;
    for path in paths {
        overlay.replace_path(store, source, path.clone())?;
    }
    overlay.finish(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_overlay_compaction_keeps_latest_values() {
        let dir = std::env::temp_dir();
        let budget = ReconcileBudget {
            scratch_dir: &dir,
            memory_bytes: 256 * 1024,
            spool_bytes: 1024 * 1024,
        };
        let mut map = Map::<16, 8>::new(budget).unwrap();
        for key in 0..2500u64 {
            let mut row = [0; 16];
            row[..8].copy_from_slice(&key.to_be_bytes());
            row[8..].copy_from_slice(&1u64.to_be_bytes());
            map.put(row).unwrap();
        }
        for key in 0..500u64 {
            let mut row = [0; 16];
            row[..8].copy_from_slice(&key.to_be_bytes());
            row[8..].copy_from_slice(&2u64.to_be_bytes());
            map.put(row).unwrap();
        }
        assert_eq!(
            u64::from_be_bytes(
                map.get(&7u64.to_be_bytes()).unwrap().unwrap()[8..]
                    .try_into()
                    .unwrap()
            ),
            2
        );
        let mut run = map.finish().unwrap();
        assert_eq!(run.count, 2500);
        for key in 0..2500u64 {
            let row = run.next().unwrap().unwrap();
            assert_eq!(&row[..8], &key.to_be_bytes());
            assert_eq!(
                u64::from_be_bytes(row[8..].try_into().unwrap()),
                if key < 500 { 2 } else { 1 }
            );
        }
        assert_eq!(run.next().unwrap(), None);
    }
}
