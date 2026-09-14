//! Private, runtime-only COW B+tree. Published page bodies are immutable. Disk
//! reference counts retain the graph; an Arc pins only a root, never its pages.
//! No page cache is retained: one bounded page is decoded per traversal level.
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind};
use std::ops::Bound;
use std::os::unix::fs::{FileExt, MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub(crate) const PAGE_BYTES: usize = 4096;
const HEADER_BYTES: usize = 64;
const BODY_OFFSET: usize = 2 * HEADER_BYTES;
const BODY_BYTES: usize = PAGE_BYTES - BODY_OFFSET;
const MAX_KEYS: usize = 7;
const MIN_KEYS: usize = 3;
const MAX_CHILDREN: usize = 8;
const MIN_CHILDREN: usize = 4;
const MAX_KEY_BYTES: usize = 384;
const INLINE_BYTES: usize = 128;
const MAX_SCAN_BYTES: usize = 4 * 1024 * 1024;
const MAX_HEIGHT: usize = 64;
const NONE: u64 = u64::MAX;
const FREE: u64 = 0;
const LIVE: u64 = 1;
const RECLAIM: u64 = 2;
const RETIRING: u64 = 3;
const REVERSE: u64 = 4;
const MAGIC: u64 = 0x3158495357464c;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    pub max_pages: u64,
    pub max_roots: usize,
    pub max_key_bytes: usize,
    pub max_value_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_pages: 262_144,
            max_roots: 4096,
            max_key_bytes: MAX_KEY_BYTES,
            max_value_bytes: 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Stats {
    pub allocated_pages: u64,
    pub live_pages: u64,
    pub data_pages: u64,
    pub catalog_bytes: u64,
    pub relocated_pages: u64,
    pub pending_roots: usize,
    pub root_leases: usize,
    pub page_reads: u64,
    pub page_writes: u64,
    pub reclaimed_pages: u64,
    /// Actual filesystem allocation; free slots remain charged until truncation.
    pub physical_bytes: u64,
    pub reclamation_pending: bool,
}
/// Metadata leaves own these opaque disk-backed payload tokens independently of
/// the caller's in-memory leases. Errors must leave the requested count unchanged;
/// implementations resolve any ambiguous I/O before returning a result.
pub(crate) trait ExternalOwner: Send + Sync {
    fn retain(&self, token: u64) -> io::Result<()>;
    fn release(&self, token: u64) -> io::Result<()>;
}
#[derive(Clone)]
pub(crate) struct Index(Arc<Inner>);
struct Inner {
    storage: Mutex<Storage>,
    retired: Mutex<VecDeque<Option<u64>>>,
    roots: AtomicUsize,
    limits: Limits,
}
#[derive(Clone)]
pub(crate) struct Root(Arc<RootOwner>);
struct RootOwner {
    owner: Arc<Inner>,
    page: Option<u64>,
}
impl std::fmt::Debug for Root {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("OverlayIndexRoot")
            .field(&self.0.page)
            .finish()
    }
}
impl Drop for RootOwner {
    fn drop(&mut self) {
        // Admission reserved this queue slot before the root was constructed.
        // No graph walk, allocator update, or physical I/O on root release.
        self.owner.retired.lock().unwrap().push_back(self.page);
    }
}
struct Storage {
    file: Arc<File>,
    catalog: Arc<File>,
    max_pages: u64,
    slots: u64,
    relocated: u64,
    pending_truncate: Option<u64>,
    high_water: u64,
    live_pages: u64,
    free: u64,
    reclaim: u64,
    reads: u64,
    writes: u64,
    reclaimed: u64,
    // A failed prepare retains at most one page's acquired child references.
    // They remain charged and are retried before any further allocation.
    rollback: Vec<Reference>,
    external: Option<Arc<dyn ExternalOwner>>,
    abandoned: Option<(u64, u64)>,
    // One exact desired-header receipt, retained across ambiguous physical I/O.
    // No later ownership arithmetic runs until this write intent is resolved.
    pending_header: Option<(u64, Header)>,
    deferred_error: Option<io::Error>,
    #[cfg(test)]
    fail_write_after: Option<usize>,
    #[cfg(test)]
    uncertain_write_once: bool,
    #[cfg(test)]
    header_read_error: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    fail_truncate_once: bool,
    #[cfg(test)]
    corrupt_move_once: bool,
}
#[derive(Clone, Copy, Debug)]
struct Header {
    sequence: u64,
    refs: u64,
    next: u64,
    state: u64,
    cursor: u64,
    location: u64,
}
#[derive(Clone)]
enum Value {
    Inline(Vec<u8>),
    Overflow { page: u64, len: usize },
}
#[derive(Clone)]
struct Entry {
    key: Vec<u8>,
    value: Value,
    external: Option<u64>,
    linked: Option<u64>,
}
#[derive(Clone, Copy)]
enum Reference {
    Page(u64),
    External(u64),
}
#[derive(Clone)]
struct Child {
    lower: Vec<u8>,
    page: u64,
}
#[derive(Clone)]
enum Node {
    Leaf(Vec<Entry>),
    Branch(Vec<Child>),
    Overflow { next: Option<u64>, bytes: Vec<u8> },
}
impl Node {
    fn references(&self) -> Vec<Reference> {
        match self {
            Self::Leaf(entries) => entries
                .iter()
                .flat_map(|e| {
                    let page = match e.value {
                        Value::Overflow { page, .. } => Some(Reference::Page(page)),
                        Value::Inline(_) => None,
                    };
                    page.into_iter()
                        .chain(e.external.map(Reference::External))
                        .chain(e.linked.map(Reference::Page))
                })
                .collect(),
            Self::Branch(children) => children.iter().map(|c| Reference::Page(c.page)).collect(),
            Self::Overflow { next, .. } => next.iter().copied().map(Reference::Page).collect(),
        }
    }
    fn lower(&self) -> io::Result<&[u8]> {
        match self {
            Self::Leaf(entries) => entries.first().map(|e| e.key.as_slice()),
            Self::Branch(children) => children.first().map(|c| c.lower.as_slice()),
            Self::Overflow { .. } => None,
        }
        .ok_or_else(|| corrupt("empty index page"))
    }
    fn underfull(&self) -> bool {
        match self {
            Self::Leaf(e) => e.len() < MIN_KEYS,
            Self::Branch(c) => c.len() < MIN_CHILDREN,
            _ => false,
        }
    }
    fn can_lend(&self) -> bool {
        match self {
            Self::Leaf(e) => e.len() > MIN_KEYS,
            Self::Branch(c) => c.len() > MIN_CHILDREN,
            _ => false,
        }
    }
}
fn corrupt(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}
fn quota(message: &str) -> io::Error {
    io::Error::new(ErrorKind::StorageFull, message)
}

impl Index {
    pub(crate) fn temporary(directory: &Path, limits: Limits) -> io::Result<Self> {
        Self::create(directory, limits, None)
    }
    pub(crate) fn temporary_with_owner(
        directory: &Path,
        limits: Limits,
        owner: Arc<dyn ExternalOwner>,
    ) -> io::Result<Self> {
        Self::create(directory, limits, Some(owner))
    }
    fn create(
        directory: &Path,
        limits: Limits,
        external: Option<Arc<dyn ExternalOwner>>,
    ) -> io::Result<Self> {
        if limits.max_pages == 0
            || limits.max_pages > u64::MAX / (PAGE_BYTES + 4 * HEADER_BYTES) as u64
            || limits.max_roots < 4
            || limits.max_key_bytes == 0
            || limits.max_key_bytes > MAX_KEY_BYTES
            || limits.max_value_bytes > MAX_SCAN_BYTES
        {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "invalid overlay index limits",
            ));
        }
        let mut retired = VecDeque::new();
        retired
            .try_reserve_exact(limits.max_roots)
            .map_err(|_| quota("overlay root/reclamation memory admission exhausted"))?;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let make_file = || -> io::Result<File> {
            loop {
                let path = directory.join(format!(
                    ".overlay-index-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&path)
                {
                    Ok(file) => {
                        fs::remove_file(path)?;
                        break Ok(file);
                    }
                    Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(e),
                }
            }
        };
        let file = make_file()?;
        let catalog = make_file()?;
        Ok(Self(Arc::new(Inner {
            storage: Mutex::new(Storage {
                file: Arc::new(file),
                catalog: Arc::new(catalog),
                max_pages: limits.max_pages,
                slots: 0,
                relocated: 0,
                pending_truncate: None,
                high_water: 0,
                live_pages: 0,
                free: NONE,
                reclaim: NONE,
                reads: 0,
                writes: 0,
                reclaimed: 0,
                rollback: Vec::with_capacity(MAX_KEYS * 3),
                external,
                abandoned: None,
                pending_header: None,
                deferred_error: None,
                #[cfg(test)]
                fail_write_after: None,
                #[cfg(test)]
                uncertain_write_once: false,
                #[cfg(test)]
                header_read_error: false.into(),
                #[cfg(test)]
                fail_truncate_once: false,
                #[cfg(test)]
                corrupt_move_once: false,
            }),
            retired: Mutex::new(retired),
            roots: AtomicUsize::new(0),
            limits,
        })))
    }
    fn ticket(&self) -> io::Result<Root> {
        self.0
            .roots
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < self.0.limits.max_roots).then_some(n + 1)
            })
            .map_err(|_| quota("overlay root/reclamation admission exhausted"))?;
        Ok(Root(Arc::new(RootOwner {
            owner: self.0.clone(),
            page: None,
        })))
    }
    pub(crate) fn empty_root(&self) -> io::Result<Root> {
        self.ticket()
    }
    pub(crate) fn owns(&self, root: &Root) -> bool {
        Arc::ptr_eq(&self.0, &root.0.owner)
    }
    fn check(&self, root: &Root, key: &[u8]) -> io::Result<()> {
        if !self.owns(root) {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "foreign overlay index root",
            ));
        }
        if key.len() > self.0.limits.max_key_bytes {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "overlay index key too large",
            ));
        }
        Ok(())
    }
    pub(crate) fn get(&self, root: &Root, key: &[u8]) -> io::Result<Option<Vec<u8>>> {
        self.check(root, key)?;
        let mut storage = self.0.storage.lock().unwrap();
        storage
            .lookup(root.0.page, key)?
            .map(|entry| storage.value(&entry.value, self.0.limits.max_value_bytes))
            .transpose()
    }
    /// Greatest key <= `key`, reached by one bounded B+tree search path.
    pub(crate) fn floor(&self, root: &Root, key: &[u8]) -> io::Result<Option<(Vec<u8>, Vec<u8>)>> {
        self.check(root, key)?;
        let mut storage = self.0.storage.lock().unwrap();
        let mut page = root.0.page;
        for _ in 0..MAX_HEIGHT {
            let Some(id) = page else {
                return Ok(None);
            };
            match storage.node(id)? {
                Node::Leaf(entries) => {
                    let index = entries.partition_point(|entry| entry.key.as_slice() <= key);
                    let Some(index) = index.checked_sub(1) else {
                        return Ok(None);
                    };
                    return Ok(Some((
                        entries[index].key.clone(),
                        storage.value(&entries[index].value, self.0.limits.max_value_bytes)?,
                    )));
                }
                Node::Branch(children) => {
                    page = Some(children[child_position(&children, key)?].page)
                }
                _ => return Err(corrupt("overflow in index path")),
            }
        }
        Err(corrupt("overlay index height exceeded"))
    }
    /// Acquires a bounded root owner while the source graph still owns the link.
    pub(crate) fn get_linked(
        &self,
        root: &Root,
        key: &[u8],
    ) -> io::Result<Option<(Vec<u8>, Option<Root>)>> {
        self.check(root, key)?;
        let mut storage = self.0.storage.lock().unwrap();
        let Some(entry) = storage.lookup(root.0.page, key)? else {
            return Ok(None);
        };
        let value = storage.value(&entry.value, self.0.limits.max_value_bytes)?;
        let linked = entry
            .linked
            .map(|page| self.retain(&mut storage, page))
            .transpose()?;
        Ok(Some((value, linked)))
    }
    pub(crate) fn set_linked(
        &self,
        root: &Root,
        key: &[u8],
        value: &[u8],
        linked: Option<&Root>,
    ) -> io::Result<Root> {
        if let Some(linked) = linked {
            self.check(linked, &[])?;
        }
        self.set_record(root, key, value, None, linked.and_then(|r| r.0.page))
    }
    pub(crate) fn set(&self, root: &Root, key: &[u8], value: &[u8]) -> io::Result<Root> {
        self.set_owned(root, key, value, None)
    }
    pub(crate) fn set_owned(
        &self,
        root: &Root,
        key: &[u8],
        value: &[u8],
        external: Option<u64>,
    ) -> io::Result<Root> {
        self.set_record(root, key, value, external, None)
    }
    fn set_record(
        &self,
        root: &Root,
        key: &[u8],
        value: &[u8],
        external: Option<u64>,
        linked: Option<u64>,
    ) -> io::Result<Root> {
        self.check(root, key)?;
        if external == Some(NONE) {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "reserved overlay payload token",
            ));
        }
        if value.len() > self.0.limits.max_value_bytes {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "overlay index value too large",
            ));
        }
        self.reclaim(16)?;
        let mut storage = self.0.storage.lock().unwrap();
        if external.is_some() && storage.external.is_none() {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "overlay payload owner missing",
            ));
        }
        let mut overflow = None;
        let value = if value.len() <= INLINE_BYTES {
            Value::Inline(value.to_vec())
        } else {
            for chunk in value.rchunks(BODY_BYTES - 13) {
                let next = overflow.as_ref().and_then(|r: &Root| r.0.page);
                overflow = Some(self.write(
                    &mut storage,
                    Node::Overflow {
                        next,
                        bytes: chunk.to_vec(),
                    },
                )?);
                // Only the newest chain root needs a foreground owner.
                self.release_retired(&mut storage, 2)?;
            }
            Value::Overflow {
                page: overflow.as_ref().unwrap().0.page.unwrap(),
                len: value.len(),
            }
        };
        let entry = Entry {
            key: key.to_vec(),
            value,
            external,
            linked,
        };
        let mut pages = if let Some(page) = root.0.page {
            self.insert(&mut storage, page, entry, 0)?
        } else {
            vec![self.write(&mut storage, Node::Leaf(vec![entry]))?]
        };
        if pages.len() == 1 {
            return Ok(pages.pop().unwrap());
        }
        let children = pages
            .iter()
            .map(|r| storage.child(r))
            .collect::<io::Result<Vec<_>>>()?;
        self.write(&mut storage, Node::Branch(children))
    }
    fn insert(
        &self,
        storage: &mut Storage,
        page: u64,
        entry: Entry,
        height: usize,
    ) -> io::Result<Vec<Root>> {
        if height >= MAX_HEIGHT {
            return Err(corrupt("overlay index height exceeded"));
        }
        let mut holds = Vec::new();
        let mut node = storage.node(page)?;
        let right = match &mut node {
            Node::Leaf(entries) => {
                match entries.binary_search_by(|e| e.key.cmp(&entry.key)) {
                    Ok(i) => entries[i] = entry,
                    Err(i) => entries.insert(i, entry),
                }
                (entries.len() > MAX_KEYS).then(|| Node::Leaf(entries.split_off(entries.len() / 2)))
            }
            Node::Branch(children) => {
                let i = child_position(children, &entry.key)?;
                holds = self.insert(storage, children[i].page, entry, height + 1)?;
                let replacements = holds
                    .iter()
                    .map(|r| storage.child(r))
                    .collect::<io::Result<Vec<_>>>()?;
                children.splice(i..=i, replacements);
                (children.len() > MAX_CHILDREN)
                    .then(|| Node::Branch(children.split_off(children.len() / 2)))
            }
            _ => return Err(corrupt("overflow in index path")),
        };
        let mut result = vec![self.write(storage, node)?];
        if let Some(right) = right {
            result.push(self.write(storage, right)?);
        }
        drop(holds);
        self.release_retired(storage, 4)?;
        Ok(result)
    }
    pub(crate) fn remove(&self, root: &Root, key: &[u8]) -> io::Result<Root> {
        self.check(root, key)?;
        let Some(page) = root.0.page else {
            return Ok(root.clone());
        };
        self.reclaim(16)?;
        let mut storage = self.0.storage.lock().unwrap();
        let Some((node, holds)) = self.delete(&mut storage, page, key, 0)? else {
            return Ok(root.clone());
        };
        let result = match node {
            Node::Leaf(ref entries) if entries.is_empty() => self.empty_root(),
            Node::Branch(ref children) if children.len() == 1 => {
                self.retain(&mut storage, children[0].page)
            }
            _ => self.write(&mut storage, node),
        };
        drop(holds);
        result
    }
    fn delete(
        &self,
        storage: &mut Storage,
        page: u64,
        key: &[u8],
        height: usize,
    ) -> io::Result<Option<(Node, Vec<Root>)>> {
        if height >= MAX_HEIGHT {
            return Err(corrupt("overlay index height exceeded"));
        }
        let mut node = storage.node(page)?;
        let mut holds = Vec::new();
        match &mut node {
            Node::Leaf(entries) => match entries.binary_search_by(|e| e.key.as_slice().cmp(key)) {
                Ok(i) => {
                    entries.remove(i);
                }
                Err(_) => return Ok(None),
            },
            Node::Branch(children) => {
                let mut i = child_position(children, key)?;
                let Some((mut child, child_holds)) =
                    self.delete(storage, children[i].page, key, height + 1)?
                else {
                    return Ok(None);
                };
                if child.underfull() && children.len() > 1 {
                    let sibling_index = if i > 0 { i - 1 } else { i + 1 };
                    let mut sibling = storage.node(children[sibling_index].page)?;
                    if sibling.can_lend() {
                        rebalance(&mut child, &mut sibling, sibling_index < i)?;
                        let sibling_root = self.write(storage, sibling)?;
                        children[sibling_index] = storage.child(&sibling_root)?;
                        holds.push(sibling_root);
                    } else {
                        child = merge(child, sibling, sibling_index < i)?;
                        children.remove(sibling_index);
                        if sibling_index < i {
                            i -= 1;
                        }
                    }
                }
                let replacement = self.write(storage, child)?;
                children[i] = storage.child(&replacement)?;
                holds.push(replacement);
                drop(child_holds);
                self.release_retired(storage, 4)?;
            }
            _ => return Err(corrupt("overflow in index path")),
        }
        Ok(Some((node, holds)))
    }
    fn retain(&self, storage: &mut Storage, page: u64) -> io::Result<Root> {
        storage.retry_rollback()?;
        self.release_retired(storage, 1)?;
        storage.report_write_error()?;
        let mut root = self.ticket()?;
        storage.increment(page)?;
        if let Err(error) = storage.report_write_error() {
            storage.rollback.push(Reference::Page(page));
            return Err(error);
        }
        Arc::get_mut(&mut root.0).unwrap().page = Some(page);
        Ok(root)
    }
    fn write(&self, storage: &mut Storage, node: Node) -> io::Result<Root> {
        let mut root = self.ticket()?;
        let page = storage.allocate(&node, self.0.limits.max_pages)?;
        Arc::get_mut(&mut root.0).unwrap().page = Some(page);
        Ok(root)
    }
    /// Seek and traverse at most `limit` records. Resume with Excluded(last_key).
    /// Batches are explicitly bounded, including their worst-case output bytes.
    pub(crate) fn scan(
        &self,
        root: &Root,
        start: Bound<&[u8]>,
        end: Bound<&[u8]>,
        limit: usize,
    ) -> io::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.check(root, &[])?;
        if limit > 1024 {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "overlay scan batch too large",
            ));
        }
        let mut storage = self.0.storage.lock().unwrap();
        let mut output = Vec::new();
        if let Some(page) = root.0.page {
            storage.scan(
                page,
                (start, end),
                limit,
                (&mut output, &mut 0),
                0,
                self.0.limits.max_value_bytes,
            )?;
        }
        Ok(output)
    }
    fn release_retired(&self, storage: &mut Storage, limit: usize) -> io::Result<()> {
        for _ in 0..limit {
            let Some(page) = self.0.retired.lock().unwrap().pop_front() else {
                break;
            };
            if let Some(id) = page {
                if let Err(error) = storage.decrement(id) {
                    self.0.retired.lock().unwrap().push_front(page);
                    return Err(error);
                }
            }
            self.0.roots.fetch_sub(1, Ordering::AcqRel);
        }
        Ok(())
    }
    /// Each step frees at most one page and visits only that page's bounded links.
    /// Failed cleanup leaves its cursor and charges intact for the next call.
    pub(crate) fn reclaim(&self, max_pages: usize) -> io::Result<usize> {
        let mut storage = self.0.storage.lock().unwrap();
        storage.retry_rollback()?;
        self.release_retired(&mut storage, max_pages)?;
        let mut reclaimed = 0;
        for _ in 0..max_pages {
            if storage.reclaim == NONE {
                break;
            }
            reclaimed += usize::from(storage.reclaim_one()?);
        }
        storage.finish_pending_header()?;
        storage.finish_pending_truncate()?;
        // Dense data-slot evacuation releases obsolete physical pages while old
        // roots remain leased. The bounded catalog's highwater stays charged.
        if storage.live_pages == 0 && storage.reclaim == NONE && storage.abandoned.is_none() {
            storage.file.set_len(0)?;
            storage.catalog.set_len(0)?;
            storage.high_water = 0;
            storage.slots = 0;
            storage.free = NONE;
        }
        storage.report_write_error()?;
        Ok(reclaimed)
    }
    /// Explicit fsync only: resolve the one bounded pending ownership write,
    /// then flush actual backing descriptors without an installation/storage
    /// lock. This does not drain the graph or create crash-recoverable roots.
    pub(crate) fn sync_data(&self) -> io::Result<()> {
        let files = {
            let mut storage = self.0.storage.lock().unwrap();
            storage.finish_pending_header()?;
            storage.report_write_error()?;
            [storage.file.clone(), storage.catalog.clone()]
        };
        for file in files {
            file.sync_data()?;
        }
        Ok(())
    }
    pub(crate) fn stats(&self) -> io::Result<Stats> {
        let storage = self.0.storage.lock().unwrap();
        let pending_roots = self.0.retired.lock().unwrap().len();
        Ok(Stats {
            allocated_pages: storage.high_water,
            live_pages: storage.live_pages,
            data_pages: storage.slots,
            catalog_bytes: storage.catalog.metadata()?.blocks() * 512,
            relocated_pages: storage.relocated,
            pending_roots,
            root_leases: self.0.roots.load(Ordering::Acquire),
            page_reads: storage.reads,
            page_writes: storage.writes,
            reclaimed_pages: storage.reclaimed,
            physical_bytes: (storage.file.metadata()?.blocks()
                + storage.catalog.metadata()?.blocks())
                * 512,
            reclamation_pending: pending_roots != 0
                || storage.reclaim != NONE
                || storage.abandoned.is_some()
                || !storage.rollback.is_empty()
                || storage.pending_header.is_some()
                || storage.pending_truncate.is_some(),
        })
    }
}
fn child_position(children: &[Child], key: &[u8]) -> io::Result<usize> {
    if children.is_empty() {
        return Err(corrupt("empty branch"));
    }
    Ok(children
        .partition_point(|c| c.lower.as_slice() <= key)
        .saturating_sub(1))
}
fn rebalance(child: &mut Node, sibling: &mut Node, before: bool) -> io::Result<()> {
    match (child, sibling) {
        (Node::Leaf(c), Node::Leaf(s)) => {
            if before {
                c.insert(0, s.pop().unwrap());
            } else {
                c.push(s.remove(0));
            }
        }
        (Node::Branch(c), Node::Branch(s)) => {
            if before {
                c.insert(0, s.pop().unwrap());
            } else {
                c.push(s.remove(0));
            }
        }
        _ => return Err(corrupt("inconsistent index depth")),
    }
    Ok(())
}
fn merge(child: Node, sibling: Node, before: bool) -> io::Result<Node> {
    let (left, right) = if before {
        (sibling, child)
    } else {
        (child, sibling)
    };
    match (left, right) {
        (Node::Leaf(mut l), Node::Leaf(r)) => {
            l.extend(r);
            Ok(Node::Leaf(l))
        }
        (Node::Branch(mut l), Node::Branch(r)) => {
            l.extend(r);
            Ok(Node::Branch(l))
        }
        _ => Err(corrupt("inconsistent index depth")),
    }
}

impl Storage {
    fn header(&self, page: u64) -> io::Result<Option<Header>> {
        #[cfg(test)]
        if self.header_read_error.swap(false, Ordering::SeqCst) {
            return Err(io::Error::other("injected ownership readback failure"));
        }
        let mut bytes = [0; BODY_OFFSET];
        match self
            .catalog
            .read_exact_at(&mut bytes, page * (2 * HEADER_BYTES) as u64)
        {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        }
        let left = Header::decode(&bytes[..HEADER_BYTES]);
        let right = Header::decode(&bytes[HEADER_BYTES..]);
        Ok(match (left, right) {
            (Some(l), Some(r)) => Some(if l.sequence > r.sequence { l } else { r }),
            (l, r) => l.or(r),
        })
    }
    fn live_header(&self, page: u64) -> io::Result<Header> {
        if page >= self.high_water {
            return Err(corrupt("overlay page outside arena"));
        }
        self.header(page)?
            .ok_or_else(|| corrupt("missing overlay ownership header"))
    }
    fn raw_write(&mut self, catalog: bool, bytes: &[u8], offset: u64) -> io::Result<()> {
        let file = if catalog { &self.catalog } else { &self.file };
        #[cfg(test)]
        if let Some(remaining) = &mut self.fail_write_after {
            if *remaining == 0 {
                self.fail_write_after = None;
                if self.uncertain_write_once {
                    self.uncertain_write_once = false;
                    file.write_all_at(bytes, offset)?;
                    if catalog {
                        self.header_read_error.store(true, Ordering::SeqCst);
                    }
                    return Err(io::Error::other(
                        "injected complete write with failed receipt",
                    ));
                }
                // Exercise a torn write, not only a pre-I/O rejection.
                file.write_all_at(&bytes[..bytes.len() / 2], offset)?;
                return Err(io::Error::other("injected overlay short write"));
            }
            *remaining -= 1;
        }
        file.write_all_at(bytes, offset)
    }
    fn put_header(&mut self, page: u64, mut header: Header) -> io::Result<()> {
        self.finish_pending_header()?;
        header.sequence = self.header(page)?.map_or(Ok(1), |h| {
            h.sequence
                .checked_add(1)
                .ok_or_else(|| corrupt("overlay header sequence exhausted"))
        })?;
        let bytes = header.encode();
        let offset = page * (2 * HEADER_BYTES) as u64 + (header.sequence % 2) * HEADER_BYTES as u64;
        if let Err(error) = self.raw_write(true, &bytes, offset) {
            // A failed/short write can never destroy the other valid slot. If
            // the new slot completed despite an error, recognize its receipt.
            match self.header(page) {
                Ok(Some(receipt)) if receipt.sequence == header.sequence => return Ok(()),
                Ok(_) => return Err(error),
                Err(_) => {
                    // Transfer the exact write intent into a bounded owner. The
                    // caller records the logical acquisition/release normally,
                    // so abandoning a prepare can still undo it exactly once.
                    // The public operation surfaces the retained I/O error.
                    self.pending_header = Some((page, header));
                    self.deferred_error.get_or_insert(error);
                    return Ok(());
                }
            }
        }
        Ok(())
    }
    fn finish_pending_header(&mut self) -> io::Result<()> {
        if let Some((page, header)) = self.pending_header {
            let offset =
                page * (2 * HEADER_BYTES) as u64 + (header.sequence % 2) * HEADER_BYTES as u64;
            // Rewriting the identical sequence/count is idempotent even when
            // the first write applied. Never recompute refs +/- 1 on a retry.
            self.raw_write(true, &header.encode(), offset)?;
            self.pending_header = None;
        }
        Ok(())
    }
    fn report_write_error(&mut self) -> io::Result<()> {
        match self.deferred_error.take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    fn increment(&mut self, page: u64) -> io::Result<()> {
        self.finish_pending_header()?;
        let mut header = self.live_header(page)?;
        if header.state != LIVE || header.refs == 0 {
            return Err(corrupt("retain of unowned overlay page"));
        }
        header.refs = header
            .refs
            .checked_add(1)
            .ok_or_else(|| quota("overlay reference count exhausted"))?;
        self.put_header(page, header)
    }
    fn decrement(&mut self, page: u64) -> io::Result<()> {
        self.finish_pending_header()?;
        let mut header = self.live_header(page)?;
        if header.state != LIVE || header.refs == 0 {
            return Err(corrupt("release of unowned overlay page"));
        }
        header.refs -= 1;
        if header.refs == 0 {
            header.state = RECLAIM;
            header.next = self.reclaim;
            header.cursor = 0;
        }
        self.put_header(page, header)?;
        if header.refs == 0 {
            self.reclaim = page;
        }
        Ok(())
    }
    fn retain_reference(&mut self, reference: Reference) -> io::Result<()> {
        match reference {
            Reference::Page(page) => self.increment(page),
            Reference::External(token) => self
                .external
                .as_ref()
                .ok_or_else(|| corrupt("overlay payload owner missing"))?
                .retain(token),
        }
    }
    fn release_reference(&mut self, reference: Reference) -> io::Result<()> {
        match reference {
            Reference::Page(page) => self.decrement(page),
            Reference::External(token) => self
                .external
                .as_ref()
                .ok_or_else(|| corrupt("overlay payload owner missing"))?
                .release(token),
        }
    }
    fn allocate(&mut self, node: &Node, max_pages: u64) -> io::Result<u64> {
        self.retry_rollback()?;
        let body = node.encode()?;
        let page = if self.free != NONE {
            let page = self.free;
            let header = self.live_header(page)?;
            if header.state != FREE {
                return Err(corrupt("owned page on overlay free list"));
            }
            self.free = header.next;
            page
        } else {
            if self.high_water >= max_pages {
                return Err(quota("overlay metadata page quota exhausted"));
            }
            let page = self.high_water;
            self.high_water += 1;
            page
        };
        let location = self.slots;
        self.slots += 1;
        self.live_pages += 1;
        self.abandoned = Some((page, location));
        self.raw_write(
            false,
            &body,
            location * PAGE_BYTES as u64 + BODY_OFFSET as u64,
        )?;
        for child in node.references() {
            self.retain_reference(child)?;
            self.rollback.push(child);
        }
        self.put_header(
            page,
            Header {
                sequence: 0,
                refs: 1,
                next: NONE,
                state: LIVE,
                cursor: 0,
                location,
            },
        )?;
        self.write_reverse(location, page)?;
        self.report_write_error()?;
        self.rollback.clear();
        self.abandoned = None;
        self.writes += 1;
        Ok(page)
    }
    fn retry_rollback(&mut self) -> io::Result<()> {
        self.finish_pending_header()?;
        self.finish_pending_truncate()?;
        while let Some(page) = self.rollback.last().copied() {
            self.release_reference(page)?;
            self.rollback.pop();
        }
        if let Some((page, _)) = self.abandoned {
            self.free_page(page)?;
            self.abandoned = None;
        }
        Ok(())
    }
    fn write_reverse(&mut self, slot: u64, page: u64) -> io::Result<()> {
        self.put_header(
            self.max_pages + slot,
            Header {
                sequence: 0,
                refs: page,
                next: NONE,
                state: REVERSE,
                cursor: 0,
                location: slot,
            },
        )
    }
    fn reverse(&self, slot: u64) -> io::Result<u64> {
        let header = self
            .header(self.max_pages + slot)?
            .ok_or_else(|| corrupt("missing overlay reverse location"))?;
        if header.state != REVERSE || header.location != slot || header.refs >= self.high_water {
            return Err(corrupt("invalid overlay reverse location"));
        }
        Ok(header.refs)
    }
    fn finish_pending_truncate(&mut self) -> io::Result<()> {
        if let Some(length) = self.pending_truncate {
            #[cfg(test)]
            if self.fail_truncate_once {
                self.fail_truncate_once = false;
                return Err(io::Error::other("injected metadata truncation failure"));
            }
            self.file.set_len(length)?;
            self.pending_truncate = None;
        }
        Ok(())
    }
    fn free_page(&mut self, page: u64) -> io::Result<()> {
        self.finish_pending_header()?;
        self.finish_pending_truncate()?;
        let location = match self.abandoned {
            Some((id, slot)) if id == page => slot,
            _ => self.live_header(page)?.location,
        };
        if location >= self.slots {
            return Err(corrupt("invalid retired overlay location"));
        }
        let last = self.slots - 1;
        if location != last {
            let source = self.reverse(last)?;
            let mut header = self.live_header(source)?;
            if source == page || (header.location != last && header.location != location) {
                return Err(corrupt("inconsistent overlay evacuation source"));
            }
            let mut bytes = [0; PAGE_BYTES];
            self.file
                .read_exact_at(&mut bytes, last * PAGE_BYTES as u64)?;
            self.raw_write(false, &bytes, location * PAGE_BYTES as u64)?;
            #[cfg(test)]
            if self.corrupt_move_once {
                self.corrupt_move_once = false;
                self.file.write_all_at(
                    &[bytes[BODY_OFFSET + 1] ^ 1],
                    location * PAGE_BYTES as u64 + BODY_OFFSET as u64 + 1,
                )?;
            }
            let mut verify = [0; PAGE_BYTES];
            self.file
                .read_exact_at(&mut verify, location * PAGE_BYTES as u64)?;
            if bytes != verify {
                return Err(corrupt("overlay evacuation verification failed"));
            }
            header.location = location;
            self.put_header(source, header)?;
            self.write_reverse(location, source)?;
        }
        self.put_header(
            page,
            Header {
                sequence: 0,
                refs: 0,
                next: self.free,
                state: FREE,
                cursor: 0,
                location: NONE,
            },
        )?;
        self.free = page;
        self.live_pages -= 1;
        self.slots -= 1;
        self.reclaimed += 1;
        self.relocated += u64::from(location != last);
        // Completion of the logical release precedes fallible physical cleanup;
        // the caller can advance the reclaim queue without replaying releases.
        self.pending_truncate = Some(self.slots * PAGE_BYTES as u64);
        Ok(())
    }
    fn node(&mut self, page: u64) -> io::Result<Node> {
        let header = self.live_header(page)?;
        if header.state != LIVE && header.state != RECLAIM {
            return Err(corrupt("read of free overlay page"));
        }
        if header.location >= self.slots {
            return Err(corrupt("overlay location outside arena"));
        }
        let mut body = [0; BODY_BYTES];
        self.file.read_exact_at(
            &mut body,
            header.location * PAGE_BYTES as u64 + BODY_OFFSET as u64,
        )?;
        self.reads += 1;
        Node::decode(&body)
    }
    fn lookup(&mut self, mut page: Option<u64>, key: &[u8]) -> io::Result<Option<Entry>> {
        for _ in 0..MAX_HEIGHT {
            let Some(id) = page else {
                return Ok(None);
            };
            match self.node(id)? {
                Node::Leaf(entries) => {
                    return Ok(entries
                        .binary_search_by(|e| e.key.as_slice().cmp(key))
                        .ok()
                        .map(|i| entries[i].clone()))
                }
                Node::Branch(children) => {
                    page = Some(children[child_position(&children, key)?].page)
                }
                _ => return Err(corrupt("overflow in index path")),
            }
        }
        Err(corrupt("overlay index height exceeded"))
    }
    fn child(&mut self, root: &Root) -> io::Result<Child> {
        let page = root.0.page.ok_or_else(|| corrupt("empty child root"))?;
        Ok(Child {
            lower: self.node(page)?.lower()?.to_vec(),
            page,
        })
    }
    fn value(&mut self, value: &Value, maximum: usize) -> io::Result<Vec<u8>> {
        match value {
            Value::Inline(bytes) => Ok(bytes.clone()),
            Value::Overflow { page, len } => {
                if *len > maximum {
                    return Err(corrupt("overlay value exceeds admitted bound"));
                }
                let mut bytes = Vec::with_capacity(*len);
                let mut next = Some(*page);
                while let Some(page) = next {
                    let Node::Overflow {
                        next: tail,
                        bytes: chunk,
                    } = self.node(page)?
                    else {
                        return Err(corrupt("invalid overflow page"));
                    };
                    if chunk.is_empty() || chunk.len() > len.saturating_sub(bytes.len()) {
                        return Err(corrupt("invalid overflow length"));
                    }
                    bytes.extend(chunk);
                    next = tail;
                }
                if bytes.len() != *len {
                    return Err(corrupt("truncated overlay value"));
                }
                Ok(bytes)
            }
        }
    }
    fn scan(
        &mut self,
        page: u64,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        limit: usize,
        output_state: (&mut Vec<(Vec<u8>, Vec<u8>)>, &mut usize),
        height: usize,
        maximum: usize,
    ) -> io::Result<()> {
        let (start, end) = bounds;
        let (output, output_bytes) = output_state;
        if output.len() >= limit || *output_bytes >= MAX_SCAN_BYTES {
            return Ok(());
        }
        if height >= MAX_HEIGHT {
            return Err(corrupt("overlay index height exceeded"));
        }
        match self.node(page)? {
            Node::Leaf(entries) => {
                for entry in entries {
                    if below(&entry.key, start) {
                        continue;
                    }
                    if above(&entry.key, end) || output.len() == limit {
                        break;
                    }
                    let value_len = match &entry.value {
                        Value::Inline(v) => v.len(),
                        Value::Overflow { len, .. } => *len,
                    };
                    let bytes = entry.key.len() + value_len;
                    if !output.is_empty() && bytes > MAX_SCAN_BYTES.saturating_sub(*output_bytes) {
                        *output_bytes = MAX_SCAN_BYTES;
                        break;
                    }
                    *output_bytes += bytes;
                    output.push((entry.key, self.value(&entry.value, maximum)?));
                }
            }
            Node::Branch(children) => {
                let begin = match start {
                    Bound::Included(k) | Bound::Excluded(k) => child_position(&children, k)?,
                    Bound::Unbounded => 0,
                };
                for child in &children[begin..] {
                    if above(&child.lower, end)
                        || output.len() == limit
                        || *output_bytes >= MAX_SCAN_BYTES
                    {
                        break;
                    }
                    self.scan(
                        child.page,
                        (start, end),
                        limit,
                        (output, output_bytes),
                        height + 1,
                        maximum,
                    )?;
                }
            }
            _ => return Err(corrupt("overflow in index path")),
        }
        Ok(())
    }
    fn reclaim_one(&mut self) -> io::Result<bool> {
        self.finish_pending_header()?;
        // Ownership changes are journaled in the page's cursor. A cursor update
        // must precede the child's release. Its reserved zero-reference parent
        // remains on the disk stack until all child releases are complete.
        let page = self.reclaim;
        if page == NONE {
            return Ok(false);
        }
        let mut header = self.live_header(page)?;
        if (header.state != RECLAIM && header.state != RETIRING) || header.refs != 0 {
            return Err(corrupt("invalid overlay reclaim queue"));
        }
        let children = if header.state == RETIRING {
            Vec::new()
        } else {
            self.node(page)?.references()
        };
        if let Some(&child) = children.get(header.cursor as usize) {
            // Keep the pending release in bounded runtime state if either I/O
            // fails; retry_rollback drains it before another allocation/reclaim.
            header.cursor += 1;
            self.put_header(page, header)?;
            self.rollback.push(child);
            self.retry_rollback()?;
            return Ok(false);
        }
        if header.state != RETIRING {
            header.state = RETIRING;
            self.put_header(page, header)?;
        }
        self.free_page(page)?;
        self.reclaim = header.next;
        Ok(true)
    }
}
fn below(key: &[u8], start: Bound<&[u8]>) -> bool {
    match start {
        Bound::Included(k) => key < k,
        Bound::Excluded(k) => key <= k,
        Bound::Unbounded => false,
    }
}
fn above(key: &[u8], end: Bound<&[u8]>) -> bool {
    match end {
        Bound::Included(k) => key > k,
        Bound::Excluded(k) => key >= k,
        Bound::Unbounded => false,
    }
}
impl Header {
    fn encode(self) -> [u8; HEADER_BYTES] {
        let mut bytes = [0; HEADER_BYTES];
        for (i, word) in [
            MAGIC,
            self.sequence,
            self.refs,
            self.next,
            self.state,
            self.cursor,
            self.location,
        ]
        .into_iter()
        .enumerate()
        {
            bytes[i * 8..i * 8 + 8].copy_from_slice(&word.to_le_bytes());
        }
        let check = checksum(&bytes[..56]);
        bytes[56..].copy_from_slice(&check.to_le_bytes());
        bytes
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        let word = |i: usize| u64::from_le_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap());
        if word(0) != MAGIC || word(7) != checksum(&bytes[..56]) {
            return None;
        }
        Some(Self {
            sequence: word(1),
            refs: word(2),
            next: word(3),
            state: word(4),
            cursor: word(5),
            location: word(6),
        })
    }
}
fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
impl Node {
    fn encode(&self) -> io::Result<[u8; BODY_BYTES]> {
        let mut body = Vec::with_capacity(BODY_BYTES);
        match self {
            Self::Leaf(entries) => {
                if entries.len() > MAX_KEYS {
                    return Err(corrupt("oversized overlay leaf"));
                }
                body.push(1);
                body.push(entries.len() as u8);
                for entry in entries {
                    put_key(&mut body, &entry.key)?;
                    body.extend(entry.external.unwrap_or(NONE).to_le_bytes());
                    body.extend(entry.linked.unwrap_or(NONE).to_le_bytes());
                    match &entry.value {
                        Value::Inline(bytes) => {
                            body.push(0);
                            body.extend((bytes.len() as u32).to_le_bytes());
                            body.extend(bytes);
                        }
                        Value::Overflow { page, len } => {
                            body.push(1);
                            body.extend((*len as u32).to_le_bytes());
                            body.extend(page.to_le_bytes());
                        }
                    }
                }
            }
            Self::Branch(children) => {
                if children.is_empty() || children.len() > MAX_CHILDREN {
                    return Err(corrupt("invalid overlay branch size"));
                }
                body.push(2);
                body.push(children.len() as u8);
                for child in children {
                    put_key(&mut body, &child.lower)?;
                    body.extend(child.page.to_le_bytes());
                }
            }
            Self::Overflow { next, bytes } => {
                body.push(3);
                body.extend(next.unwrap_or(NONE).to_le_bytes());
                body.extend((bytes.len() as u32).to_le_bytes());
                body.extend(bytes);
            }
        }
        if body.len() > BODY_BYTES {
            return Err(corrupt("overlay node exceeds page"));
        }
        let mut result = [0; BODY_BYTES];
        result[..body.len()].copy_from_slice(&body);
        Ok(result)
    }
    fn decode(bytes: &[u8]) -> io::Result<Self> {
        let mut reader = Decoder(bytes);
        match reader.byte()? {
            1 => {
                let count = reader.byte()? as usize;
                if count > MAX_KEYS {
                    return Err(corrupt("oversized overlay leaf"));
                }
                let mut entries = Vec::with_capacity(count);
                for _ in 0..count {
                    let key = reader.key()?;
                    let external = reader.u64()?;
                    let external = (external != NONE).then_some(external);
                    let linked = reader.u64()?;
                    let linked = (linked != NONE).then_some(linked);
                    let kind = reader.byte()?;
                    let len = reader.u32()? as usize;
                    let value = match kind {
                        0 if len <= INLINE_BYTES => Value::Inline(reader.take(len)?.to_vec()),
                        1 => Value::Overflow {
                            page: reader.u64()?,
                            len,
                        },
                        _ => return Err(corrupt("invalid overlay leaf value")),
                    };
                    if entries.last().is_some_and(|e: &Entry| e.key >= key) {
                        return Err(corrupt("unordered overlay leaf"));
                    }
                    entries.push(Entry {
                        key,
                        value,
                        external,
                        linked,
                    });
                }
                Ok(Self::Leaf(entries))
            }
            2 => {
                let count = reader.byte()? as usize;
                if count == 0 || count > MAX_CHILDREN {
                    return Err(corrupt("invalid overlay branch size"));
                }
                let mut children = Vec::with_capacity(count);
                for _ in 0..count {
                    let lower = reader.key()?;
                    let page = reader.u64()?;
                    if children.last().is_some_and(|c: &Child| c.lower >= lower) {
                        return Err(corrupt("unordered overlay branch"));
                    }
                    children.push(Child { lower, page });
                }
                Ok(Self::Branch(children))
            }
            3 => {
                let next = reader.u64()?;
                let len = reader.u32()? as usize;
                Ok(Self::Overflow {
                    next: (next != NONE).then_some(next),
                    bytes: reader.take(len)?.to_vec(),
                })
            }
            _ => Err(corrupt("invalid overlay page tag")),
        }
    }
}
fn put_key(body: &mut Vec<u8>, key: &[u8]) -> io::Result<()> {
    if key.len() > MAX_KEY_BYTES {
        return Err(corrupt("overlay key exceeds page format"));
    }
    body.extend((key.len() as u16).to_le_bytes());
    body.extend(key);
    Ok(())
}
struct Decoder<'a>(&'a [u8]);
impl<'a> Decoder<'a> {
    fn take(&mut self, len: usize) -> io::Result<&'a [u8]> {
        if len > self.0.len() {
            return Err(corrupt("truncated overlay page"));
        }
        let (result, rest) = self.0.split_at(len);
        self.0 = rest;
        Ok(result)
    }
    fn byte(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn key(&mut self) -> io::Result<Vec<u8>> {
        let len = u16::from_le_bytes(self.take(2)?.try_into().unwrap()) as usize;
        if len > MAX_KEY_BYTES {
            return Err(corrupt("oversized overlay key"));
        }
        Ok(self.take(len)?.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn index(max_pages: u64) -> Index {
        Index::temporary(
            &std::env::temp_dir(),
            Limits {
                max_pages,
                ..Limits::default()
            },
        )
        .unwrap()
    }
    fn drain(index: &Index) {
        for _ in 0..20_000 {
            index.reclaim(64).unwrap();
            let storage = index.0.storage.lock().unwrap();
            if storage.reclaim == NONE && index.0.retired.lock().unwrap().is_empty() {
                return;
            }
        }
        panic!("reclamation did not drain");
    }
    #[test]
    fn retained_roots_split_merge_seek_and_release_without_resident_page_graph() {
        let index = index(4096);
        let mut root = index.empty_root().unwrap();
        let mut expected = BTreeMap::new();
        // Permuted inserts exercise both sides of every split; distinct retained
        // roots must survive both later deletion and physical slot reuse.
        for i in 0u32..257 {
            let n = (i * 97) % 257;
            let key = n.to_be_bytes().to_vec();
            let value = vec![(n % 251) as u8; if n % 19 == 0 { 9000 } else { 37 }];
            root = index.set(&root, &key, &value).unwrap();
            expected.insert(key, value);
            index.reclaim(32).unwrap();
        }
        let snapshot = root.clone();
        let before = index.stats().unwrap();
        let copied = snapshot.clone();
        assert_eq!(index.stats().unwrap().page_reads, before.page_reads);
        assert_eq!(index.stats().unwrap().page_writes, before.page_writes);
        drop(copied);
        let mut seen = Vec::new();
        let mut last = None;
        loop {
            let batch = index
                .scan(
                    &snapshot,
                    last.as_deref().map_or(Bound::Unbounded, Bound::Excluded),
                    Bound::Unbounded,
                    11,
                )
                .unwrap();
            if batch.is_empty() {
                break;
            }
            last = batch.last().map(|(key, _)| key.clone());
            seen.extend(batch);
        }
        assert_eq!(seen, expected.clone().into_iter().collect::<Vec<_>>());
        let key = 100u32.to_be_bytes();
        assert_eq!(
            index
                .scan(&snapshot, Bound::Included(&key), Bound::Included(&key), 2)
                .unwrap()
                .len(),
            1
        );
        assert!(index
            .scan(&snapshot, Bound::Excluded(&key), Bound::Included(&key), 2)
            .unwrap()
            .is_empty());
        for i in 0u32..257 {
            let key = ((i * 113) % 257).to_be_bytes();
            root = index.remove(&root, &key).unwrap();
            assert_eq!(index.get(&root, &key).unwrap(), None);
            assert_eq!(
                index.get(&snapshot, &key).unwrap().as_ref(),
                expected.get(key.as_slice())
            );
            index.reclaim(64).unwrap();
        }
        assert!(index
            .scan(&root, Bound::Unbounded, Bound::Unbounded, 10)
            .unwrap()
            .is_empty());
        drain(&index);
        assert!(index.stats().unwrap().live_pages > 0);
        drop(snapshot);
        drop(root);
        drain(&index);
        let stats = index.stats().unwrap();
        assert_eq!(stats.live_pages, 0);
        assert_eq!(stats.allocated_pages, 0);
        assert_eq!(stats.physical_bytes, 0);
        assert_eq!(stats.root_leases, 0);
    }
    #[test]
    fn quota_and_torn_prepare_keep_source_readable_and_reclaim_abandoned_pages() {
        let index = index(3);
        let empty = index.empty_root().unwrap();
        let root = index.set(&empty, b"stable", b"original").unwrap();
        let error = index.set(&root, b"large", &[7; 20_000]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::StorageFull);
        assert_eq!(
            index.get(&root, b"stable").unwrap(),
            Some(b"original".to_vec())
        );
        drain(&index);
        let good = index.set(&root, b"stable", b"after quota").unwrap();
        assert_eq!(
            index.get(&root, b"stable").unwrap(),
            Some(b"original".to_vec())
        );
        drop(good);
        drain(&index);
        for failed_write in 0..2 {
            index.0.storage.lock().unwrap().fail_write_after = Some(failed_write);
            assert!(index.set(&root, b"stable", b"not published").is_err());
            assert_eq!(
                index.get(&root, b"stable").unwrap(),
                Some(b"original".to_vec())
            );
            drain(&index);
        }
        drop(root);
        drop(empty);
        drain(&index);
        assert_eq!(index.stats().unwrap().live_pages, 0);
    }
    #[test]
    fn root_admission_and_foreign_roots_fail_before_installation() {
        let index = Index::temporary(
            &std::env::temp_dir(),
            Limits {
                max_roots: 4,
                ..Limits::default()
            },
        )
        .unwrap();
        let roots = (0..4)
            .map(|_| index.empty_root().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            index.empty_root().unwrap_err().kind(),
            ErrorKind::StorageFull
        );
        let foreign = self::index(32);
        assert_eq!(
            foreign.get(&roots[0], b"key").unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
        drop(roots);
        drain(&index);
        assert!(index.empty_root().is_ok());
    }
    #[test]
    fn disk_owned_links_and_payload_tokens_survive_root_drop_and_failed_reclaim() {
        struct Owner {
            refs: AtomicUsize,
            fail_release: std::sync::atomic::AtomicBool,
        }
        impl ExternalOwner for Owner {
            fn retain(&self, token: u64) -> io::Result<()> {
                assert_eq!(token, 19);
                self.refs.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            fn release(&self, token: u64) -> io::Result<()> {
                assert_eq!(token, 19);
                if self.fail_release.swap(false, Ordering::SeqCst) {
                    return Err(io::Error::other("injected payload release failure"));
                }
                assert!(self.refs.fetch_sub(1, Ordering::SeqCst) > 0);
                Ok(())
            }
        }
        let owner = Arc::new(Owner {
            refs: AtomicUsize::new(1),
            fail_release: false.into(),
        });
        let index =
            Index::temporary_with_owner(&std::env::temp_dir(), Limits::default(), owner.clone())
                .unwrap();
        let empty = index.empty_root().unwrap();
        let range = index
            .set_owned(&empty, b"range", b"payload descriptor", Some(19))
            .unwrap();
        let directory = index
            .set_linked(&empty, b"inode", b"fixed attributes", Some(&range))
            .unwrap();
        drop(range);
        drain(&index);
        assert_eq!(owner.refs.load(Ordering::SeqCst), 2);
        let (_, reader) = index.get_linked(&directory, b"inode").unwrap().unwrap();
        let reader = reader.unwrap();
        drop(directory);
        drain(&index);
        assert_eq!(
            index.get(&reader, b"range").unwrap(),
            Some(b"payload descriptor".to_vec())
        );
        // A failed leaf publication releases acquired external ownership exactly
        // once; its original linked/read roots remain readable.
        index.0.storage.lock().unwrap().fail_write_after = Some(1);
        assert!(index
            .set_owned(&reader, b"range", b"failed replacement", Some(19))
            .is_err());
        drain(&index);
        assert_eq!(owner.refs.load(Ordering::SeqCst), 2);
        drop(reader);
        owner.fail_release.store(true, Ordering::SeqCst);
        assert!(index.reclaim(64).is_err());
        assert_eq!(owner.refs.load(Ordering::SeqCst), 2);
        assert!(index.stats().unwrap().reclamation_pending);
        drain(&index);
        assert_eq!(owner.refs.load(Ordering::SeqCst), 1);
        drop(empty);
        drain(&index);
        assert_eq!(index.stats().unwrap().physical_bytes, 0);
    }
    #[test]
    fn floor_descends_without_scanning_predecessor_prefix_and_batches_bound_bytes() {
        let index = index(4096);
        let mut root = index.empty_root().unwrap();
        for n in 1u32..130 {
            root = index
                .set(&root, &(n * 2).to_be_bytes(), &n.to_be_bytes())
                .unwrap();
        }
        assert_eq!(index.floor(&root, &0u32.to_be_bytes()).unwrap(), None);
        let before = index.stats().unwrap().page_reads;
        assert_eq!(
            index
                .floor(&root, &201u32.to_be_bytes())
                .unwrap()
                .unwrap()
                .0,
            200u32.to_be_bytes()
        );
        assert!(index.stats().unwrap().page_reads - before <= 4);
        assert_eq!(
            index
                .floor(&root, &202u32.to_be_bytes())
                .unwrap()
                .unwrap()
                .0,
            202u32.to_be_bytes()
        );
        assert_eq!(
            index
                .floor(&root, &999u32.to_be_bytes())
                .unwrap()
                .unwrap()
                .0,
            258u32.to_be_bytes()
        );
        let large = Index::temporary(
            &std::env::temp_dir(),
            Limits {
                max_value_bytes: MAX_SCAN_BYTES,
                ..Limits::default()
            },
        )
        .unwrap();
        let empty = large.empty_root().unwrap();
        let root = large.set(&empty, b"a", &vec![3; 3 * 1024 * 1024]).unwrap();
        let root = large.set(&root, b"b", &vec![7; 3 * 1024 * 1024]).unwrap();
        assert_eq!(
            large
                .scan(&root, Bound::Unbounded, Bound::Unbounded, 1024)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            large
                .scan(&root, Bound::Excluded(b"a"), Bound::Unbounded, 1024)
                .unwrap()[0]
                .0,
            b"b"
        );
    }
    #[test]
    fn uncertain_ownership_write_readback_cannot_leak_a_child_reference() {
        let index = index(64);
        let empty = index.empty_root().unwrap();
        let root = index.set(&empty, b"retained", &[23; 9000]).unwrap();
        drain(&index);
        {
            let mut storage = index.0.storage.lock().unwrap();
            // Replacement leaf body is write 0; retaining its old overflow
            // child is write 1. The latter applies, but loses both receipts.
            storage.fail_write_after = Some(1);
            storage.uncertain_write_once = true;
        }
        assert!(index.set(&root, b"new", b"candidate").is_err());
        assert_eq!(index.get(&root, b"retained").unwrap(), Some(vec![23; 9000]));
        drain(&index);
        drop(root);
        drop(empty);
        drain(&index);
        assert_eq!(
            index.stats().unwrap().live_pages,
            0,
            "unpublished child's retained ref leaked"
        );
    }
    #[test]
    fn reclaim_header_faults_never_double_release_or_lose_cleanup() {
        // Four pages / three edges require at most 4*5 + 3*2 + 1 writes,
        // including relocation. Stop only after the first untriggered position;
        // every earlier failed boundary must preserve ownership and drain.
        for uncertain in [false, true] {
            let mut triggered = 0;
            let mut exhausted = false;
            for boundary in 0..=27 {
                let index = index(64);
                let empty = index.empty_root().unwrap();
                let root = index.set(&empty, b"value", &[29; 9000]).unwrap();
                drain(&index);
                drop(root);
                drop(empty);
                {
                    let mut storage = index.0.storage.lock().unwrap();
                    storage.fail_write_after = Some(boundary);
                    storage.uncertain_write_once = uncertain;
                }
                let result = index.reclaim(128);
                let hit = index
                    .0
                    .storage
                    .lock()
                    .unwrap()
                    .fail_write_after
                    .take()
                    .is_none();
                if hit {
                    assert!(result.is_err(), "fault {boundary} did not surface");
                    triggered += 1;
                } else {
                    result.unwrap();
                }
                drain(&index);
                let stats = index.stats().unwrap();
                assert_eq!(
                    stats.live_pages, 0,
                    "fault {boundary}, uncertain {uncertain}"
                );
                assert_eq!(stats.root_leases, 0);
                assert_eq!(stats.physical_bytes, 0);
                assert!(!stats.reclamation_pending);
                if !hit {
                    exhausted = true;
                    break;
                }
            }
            assert!(exhausted, "fault coverage did not reach final write");
            assert!(triggered >= 11);
        }
    }
    #[test]
    fn uncertain_read_lease_and_cleanup_retry_keep_the_source_owned() {
        let index = index(64);
        let empty = index.empty_root().unwrap();
        let range = index.set(&empty, b"range", &[31; 9000]).unwrap();
        let directory = index
            .set_linked(&empty, b"inode", b"attributes", Some(&range))
            .unwrap();
        drop(range);
        drain(&index);
        {
            let mut storage = index.0.storage.lock().unwrap();
            storage.fail_write_after = Some(0);
            storage.uncertain_write_once = true;
        }
        assert!(index.get_linked(&directory, b"inode").is_err());
        assert!(index.stats().unwrap().reclamation_pending);
        // The first cleanup retry also fails. The exact pending write remains
        // owned; source reads do not need successful metadata writeback.
        index.0.storage.lock().unwrap().fail_write_after = Some(0);
        assert!(index.reclaim(64).is_err());
        assert_eq!(
            index.get(&directory, b"inode").unwrap(),
            Some(b"attributes".to_vec())
        );
        drain(&index);
        let (_, reader) = index.get_linked(&directory, b"inode").unwrap().unwrap();
        let reader = reader.unwrap();
        assert_eq!(index.get(&reader, b"range").unwrap(), Some(vec![31; 9000]));
        drop(directory);
        drop(empty);
        drain(&index);
        assert_eq!(index.get(&reader, b"range").unwrap(), Some(vec![31; 9000]));
        drop(reader);
        drain(&index);
        assert_eq!(index.stats().unwrap().live_pages, 0);
    }
    #[test]
    fn physical_pages_relocate_and_reclaim_under_a_retained_snapshot() {
        let index = index(256);
        let empty = index.empty_root().unwrap();
        let snapshot = index.set(&empty, b"stable", b"snapshot bytes").unwrap();
        let mut root = index
            .set(&snapshot, b"temporary", &[41; 64 * 1024])
            .unwrap();
        let peak = index.stats().unwrap();
        root = index.remove(&root, b"temporary").unwrap();
        drain(&index);
        let compact = index.stats().unwrap();
        assert!(compact.relocated_pages > 0);
        assert_eq!(compact.data_pages, compact.live_pages);
        assert!(
            compact.physical_bytes < peak.physical_bytes,
            "dead data blocks were not physically released"
        );
        assert_eq!(
            index.get(&snapshot, b"stable").unwrap(),
            Some(b"snapshot bytes".to_vec())
        );
        assert_eq!(
            index.get(&root, b"stable").unwrap(),
            Some(b"snapshot bytes".to_vec())
        );
        let pages = compact.allocated_pages;
        for n in 0..200u32 {
            root = index.set(&root, b"reused", &n.to_le_bytes()).unwrap();
            drain(&index);
        }
        assert!(
            index.stats().unwrap().allocated_pages <= pages + 1,
            "dead IDs grew with mutation history"
        );
        assert_eq!(index.get(&snapshot, b"reused").unwrap(), None);
        drop(root);
        drop(snapshot);
        drop(empty);
        drain(&index);
        assert_eq!(index.stats().unwrap().physical_bytes, 0);
    }
    #[test]
    fn maximum_sized_leaf_keeps_fixed_inode_records_inline() {
        let entries = (0..MAX_KEYS)
            .map(|n| {
                let mut key = vec![b'x'; MAX_KEY_BYTES];
                *key.last_mut().unwrap() = n as u8;
                Entry {
                    key,
                    value: Value::Inline(vec![n as u8; INLINE_BYTES]),
                    external: None,
                    linked: None,
                }
            })
            .collect();
        let encoded = Node::Leaf(entries).encode().unwrap();
        let Node::Leaf(decoded) = Node::decode(&encoded).unwrap() else {
            panic!("wrong page kind");
        };
        assert_eq!(decoded.len(), MAX_KEYS);
        assert!(decoded
            .iter()
            .all(|e| matches!(&e.value, Value::Inline(bytes) if bytes.len() == 128)));
        assert!(Node::Leaf(decoded).references().is_empty());
    }
    #[test]
    fn repeated_linked_reads_reuse_admitted_root_tickets() {
        let index = Index::temporary(
            &std::env::temp_dir(),
            Limits {
                max_roots: 8,
                ..Limits::default()
            },
        )
        .unwrap();
        let empty = index.empty_root().unwrap();
        let range = index.set(&empty, b"range", b"readable").unwrap();
        let directory = index
            .set_linked(&empty, b"inode", b"attributes", Some(&range))
            .unwrap();
        drop(range);
        drain(&index);
        for _ in 0..32 {
            let (_, reader) = index.get_linked(&directory, b"inode").unwrap().unwrap();
            assert_eq!(
                index.get(&reader.unwrap(), b"range").unwrap(),
                Some(b"readable".to_vec())
            );
        }
        drop(directory);
        drop(empty);
        drain(&index);
        assert_eq!(index.stats().unwrap().root_leases, 0);
    }
    #[test]
    fn relocation_faults_keep_a_live_tail_page_readable() {
        for uncertain in [false, true] {
            let mut exhausted = false;
            for boundary in 0..=27 {
                let index = index(64);
                let empty = index.empty_root().unwrap();
                let garbage = index.set(&empty, b"garbage", &[43; 9000]).unwrap();
                let snapshot = index.set(&empty, b"live", b"unchanged tail bytes").unwrap();
                drain(&index);
                drop(garbage);
                {
                    let mut storage = index.0.storage.lock().unwrap();
                    storage.fail_write_after = Some(boundary);
                    storage.uncertain_write_once = uncertain;
                }
                let result = index.reclaim(128);
                let hit = index
                    .0
                    .storage
                    .lock()
                    .unwrap()
                    .fail_write_after
                    .take()
                    .is_none();
                if hit {
                    assert!(result.is_err(), "fault {boundary} did not surface");
                } else {
                    result.unwrap();
                }
                assert_eq!(
                    index.get(&snapshot, b"live").unwrap(),
                    Some(b"unchanged tail bytes".to_vec())
                );
                drain(&index);
                assert_eq!(
                    index.get(&snapshot, b"live").unwrap(),
                    Some(b"unchanged tail bytes".to_vec())
                );
                assert_eq!(index.stats().unwrap().live_pages, 1);
                drop(snapshot);
                drop(empty);
                drain(&index);
                assert_eq!(index.stats().unwrap().physical_bytes, 0);
                if !hit {
                    exhausted = true;
                    break;
                }
            }
            assert!(exhausted, "fault coverage did not reach final move write");
        }
    }
    #[test]
    fn failed_copy_verification_and_truncation_preserve_source_and_charges() {
        for corrupt_copy in [true, false] {
            let index = index(64);
            let empty = index.empty_root().unwrap();
            let garbage = index.set(&empty, b"garbage", &[47; 9000]).unwrap();
            let snapshot = index.set(&empty, b"live", b"source bytes").unwrap();
            drain(&index);
            let peak = index.stats().unwrap();
            drop(garbage);
            {
                let mut storage = index.0.storage.lock().unwrap();
                storage.corrupt_move_once = corrupt_copy;
                storage.fail_truncate_once = !corrupt_copy;
            }
            assert!(index.reclaim(128).is_err());
            assert_eq!(
                index.get(&snapshot, b"live").unwrap(),
                Some(b"source bytes".to_vec())
            );
            let failed = index.stats().unwrap();
            assert!(failed.reclamation_pending);
            if !corrupt_copy {
                assert!(
                    failed.physical_bytes
                        > failed.catalog_bytes + failed.data_pages * PAGE_BYTES as u64,
                    "failed physical truncation was incorrectly marked free"
                );
            }
            drain(&index);
            assert!(index.stats().unwrap().physical_bytes < peak.physical_bytes);
            assert_eq!(
                index.get(&snapshot, b"live").unwrap(),
                Some(b"source bytes".to_vec())
            );
            drop(snapshot);
            drop(empty);
            drain(&index);
            assert_eq!(index.stats().unwrap().physical_bytes, 0);
        }
    }
    #[test]
    fn queued_root_releases_are_reported_as_pending_cleanup() {
        let index = index(64);
        let empty = index.empty_root().unwrap();
        let root = index.set(&empty, b"key", b"value").unwrap();
        drop(root);
        let pending = index.stats().unwrap();
        assert!(pending.pending_roots > 0);
        assert!(
            pending.reclamation_pending,
            "queued owner release was reported settled"
        );
        drop(empty);
        for _ in 0..64 {
            if !index.stats().unwrap().reclamation_pending {
                break;
            }
            index.reclaim(8).unwrap();
        }
        assert_eq!(index.stats().unwrap().physical_bytes, 0);
    }
    #[test]
    fn explicit_sync_flushes_data_and_catalog_and_preserves_root_ownership() {
        let index = index(64);
        let empty = index.empty_root().unwrap();
        let snapshot = index.set(&empty, b"key", b"old").unwrap();
        let current = index.set(&snapshot, b"key", b"new").unwrap();
        drop(index.set(&current, b"discarded", b"private").unwrap());
        let before = index.stats().unwrap();
        index.sync_data().unwrap();
        let after = index.stats().unwrap();
        assert_eq!(
            before.pending_roots, after.pending_roots,
            "fsync drained root owners"
        );
        assert_eq!(before.reclaimed_pages, after.reclaimed_pages);
        assert_eq!(index.get(&snapshot, b"key").unwrap(), Some(b"old".to_vec()));
        assert_eq!(index.get(&current, b"key").unwrap(), Some(b"new".to_vec()));
        // A socket is an actual descriptor that cannot fsync. Both backing
        // descriptors must surface that error; a metadata-only no-op cannot pass.
        for catalog in [false, true] {
            let (socket, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
            let fd: std::os::fd::OwnedFd = socket.into();
            let invalid = Arc::new(File::from(fd));
            let original = {
                let mut storage = index.0.storage.lock().unwrap();
                if catalog {
                    std::mem::replace(&mut storage.catalog, invalid)
                } else {
                    std::mem::replace(&mut storage.file, invalid)
                }
            };
            assert!(index.sync_data().is_err());
            let mut storage = index.0.storage.lock().unwrap();
            if catalog {
                storage.catalog = original;
            } else {
                storage.file = original;
            }
        }
        index.sync_data().unwrap();
    }
}
