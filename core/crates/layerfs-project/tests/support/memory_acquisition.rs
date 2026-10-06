//! External in-memory acquisition backing for port-path vectors.
//!
//! It keeps the port's order, window, identity and custody rules so Project's
//! flow is exercised without an engine. Its byte charges are its own simple
//! estimate, not the SQLite provider's, and it has exactly one session.
#![allow(dead_code)]
use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use layerfs_storage::port::{
    acquisition::{
        Abandoned, Acquisition, AcquisitionError, AcquisitionResult, AcquisitionWork, Begin,
        Directory, Discarded, Entry, EntryKey, FileRoot, Job, Limits, NativeIdentity, NewEntry,
        Owner, Phase, Placed, Unplaced, WRITE_ROW_BYTES, WRITE_WINDOW_BYTES, WRITE_WINDOW_ROWS,
    },
    PersistenceError,
};
use std::{
    collections::BTreeMap,
    ops::Bound::{Excluded, Unbounded},
    path::PathBuf,
    sync::Mutex,
};

const EPOCH: u64 = 1;
/// Parent key of the source root, ordered before every real parent.
const ROOT_PARENT: i64 = -1;

#[derive(Clone)]
struct Row {
    position: Option<u64>,
    kind: InodeKind,
    canonical: Option<u64>,
    metadata_root: ObjectId,
    content_root: Option<ObjectId>,
    native_path: Option<Vec<u8>>,
    native: Option<NativeIdentity>,
    bytes: u64,
}

#[derive(Clone)]
struct Native {
    identity: NativeIdentity,
    path: Vec<u8>,
    aliases: u64,
    root: Option<ObjectId>,
    bytes: u64,
}

#[derive(Clone)]
struct Operation {
    phase: Phase,
    entries: BTreeMap<(i64, Vec<u8>), Row>,
    /// Key of each positioned directory, by position.
    directories: BTreeMap<u64, (i64, Vec<u8>)>,
    natives: BTreeMap<u64, Native>,
    identities: BTreeMap<(u64, u64), u64>,
    work: AcquisitionWork,
}
impl Operation {
    fn charge(&mut self, rows: u64, bytes: u64) {
        let work = &mut self.work;
        work.held_rows += rows;
        work.held_bytes += bytes;
        work.peak_rows = work.peak_rows.max(work.held_rows);
        work.peak_bytes = work.peak_bytes.max(work.held_bytes);
    }
    /// Binds one positioned path; a different evidence changes nothing.
    fn bind(
        &mut self,
        position: u64,
        native: &NativeIdentity,
        path: &[u8],
    ) -> AcquisitionResult<u64> {
        if let Some(first) = self.identities.get(&(native.device, native.inode)) {
            let held = self.natives.get_mut(first).expect("identity row");
            if held.identity.evidence != native.evidence {
                return Err(AcquisitionError::Changed { position });
            }
            held.aliases += 1;
            return Ok(*first);
        }
        if position == 0 {
            return Err(AcquisitionError::Bounds);
        }
        let bytes = path.len() as u64 + 60;
        self.identities
            .insert((native.device, native.inode), position);
        self.natives.insert(
            position,
            Native {
                identity: *native,
                path: path.to_vec(),
                aliases: 0,
                root: None,
                bytes,
            },
        );
        self.charge(1, bytes);
        Ok(position)
    }
}

#[derive(Default)]
struct State {
    issued: u64,
    operations: BTreeMap<u64, Operation>,
    calls: BTreeMap<&'static str, u64>,
    /// Unit name, the call of it that fails, and its failure.
    fault: Option<(&'static str, u64, AcquisitionError)>,
    largest_read: usize,
    largest_write: usize,
}

/// One-session acquisition backing held in process memory.
pub struct MemoryAcquisition {
    state: Mutex<State>,
    placement: Option<PathBuf>,
    read_rows: usize,
}
impl Default for MemoryAcquisition {
    fn default() -> Self {
        Self::with_read_rows(Limits::MAXIMUM.rows)
    }
}
impl MemoryAcquisition {
    /// A backing whose read windows return at most `read_rows` rows.
    pub fn with_read_rows(read_rows: usize) -> Self {
        Self {
            state: Mutex::new(State::default()),
            placement: None,
            read_rows,
        }
    }
    /// A backing that reports `directory` as the home of its mutable files.
    pub fn placed_in(directory: PathBuf) -> Self {
        Self {
            placement: Some(directory),
            ..Self::default()
        }
    }
    /// Makes the `call`-th call (from one) of `unit` fail with `error`.
    pub fn fail(&self, unit: &'static str, call: u64, error: AcquisitionError) {
        self.state.lock().unwrap().fault = Some((unit, call, error));
    }
    /// Operations whose record still exists, with rows each still holds.
    pub fn operations(&self) -> Vec<(u64, Phase, u64)> {
        let state = self.state.lock().unwrap();
        let held =
            |operation: &Operation| (operation.entries.len() + operation.natives.len()) as u64;
        let listed = state.operations.iter();
        listed
            .map(|(id, operation)| (*id, operation.phase, held(operation)))
            .collect()
    }
    /// Operations ever begun.
    pub fn issued(&self) -> u64 {
        self.state.lock().unwrap().issued
    }
    /// Calls made of one unit.
    pub fn calls(&self, unit: &'static str) -> u64 {
        let state = self.state.lock().unwrap();
        state.calls.get(unit).copied().unwrap_or(0)
    }
    /// Most rows one read window returned and one write window carried.
    pub fn largest_windows(&self) -> (usize, usize) {
        let state = self.state.lock().unwrap();
        (state.largest_read, state.largest_write)
    }

    /// One unit against a live operation. A failed unit changes nothing.
    fn unit<T>(
        &self,
        name: &'static str,
        owner: Owner,
        body: impl FnOnce(&mut Operation) -> AcquisitionResult<T>,
    ) -> AcquisitionResult<T> {
        let mut state = self.state.lock().unwrap();
        let call = state.calls.entry(name).or_insert(0);
        *call += 1;
        let call = *call;
        if let Some((_, _, error)) = state
            .fault
            .take_if(|(unit, at, _)| *unit == name && *at == call)
        {
            return Err(error);
        }
        if owner.epoch != EPOCH {
            return Err(AcquisitionError::Stale);
        }
        let operation = state.operations.get_mut(&owner.operation);
        let operation = operation.ok_or(AcquisitionError::Stale)?;
        let before = operation.clone();
        let result = body(operation);
        if result.is_err() {
            *operation = before;
        }
        result
    }
    fn read<T>(
        &self,
        name: &'static str,
        owner: Owner,
        limits: Limits,
        body: impl FnOnce(&Operation, usize) -> AcquisitionResult<Vec<T>>,
    ) -> AcquisitionResult<Vec<T>> {
        let rows = limits.clamped().rows.min(self.read_rows);
        let window = self.unit(name, owner, |operation| body(operation, rows))?;
        let mut state = self.state.lock().unwrap();
        state.largest_read = state.largest_read.max(window.len());
        Ok(window)
    }
    fn written(&self, rows: usize, bytes: usize) -> AcquisitionResult<()> {
        if rows > WRITE_WINDOW_ROWS || bytes > WRITE_WINDOW_BYTES {
            return Err(AcquisitionError::Bounds);
        }
        let mut state = self.state.lock().unwrap();
        state.largest_write = state.largest_write.max(rows);
        Ok(())
    }
}

fn well_formed(entry: &NewEntry) -> bool {
    let path = entry.native_path.is_some();
    let shape = match entry.kind {
        InodeKind::Directory => path && entry.native.is_none(),
        InodeKind::RegularFile => path && entry.native.is_some(),
        InodeKind::Symlink => !path && entry.native.is_none(),
    };
    let child = entry.key.parent.is_some();
    shape
        && (entry.kind == InodeKind::Symlink) == entry.target_root.is_some()
        && child != (entry.position == Some(0))
        && child != entry.key.name.is_empty()
        && (child || entry.kind == InodeKind::Directory)
        && entry.key.name.len() <= 255
}

fn parent_key(parent: Option<u64>) -> i64 {
    parent.map_or(ROOT_PARENT, |parent| parent as i64)
}

impl Acquisition for MemoryAcquisition {
    fn begin(&self, _facts: &Begin) -> AcquisitionResult<Owner> {
        let mut state = self.state.lock().unwrap();
        state.issued += 1;
        let operation = state.issued;
        state.operations.insert(
            operation,
            Operation {
                phase: Phase::Scanning,
                entries: BTreeMap::new(),
                directories: BTreeMap::new(),
                natives: BTreeMap::new(),
                identities: BTreeMap::new(),
                work: AcquisitionWork::default(),
            },
        );
        Ok(Owner {
            operation,
            epoch: EPOCH,
        })
    }

    fn placement(&self) -> Option<PathBuf> {
        self.placement.clone()
    }

    fn put_entries(&self, owner: Owner, entries: &[NewEntry]) -> AcquisitionResult<Vec<u64>> {
        let bytes = entries.iter().map(|entry| {
            entry.key.name.len() + entry.native_path.as_ref().map_or(0, Vec::len) + WRITE_ROW_BYTES
        });
        self.written(entries.len(), bytes.sum())?;
        self.unit("put_entries", owner, |operation| {
            let mut canonical = Vec::new();
            for entry in entries {
                if !well_formed(entry) {
                    return Err(AcquisitionError::Bounds);
                }
                let key = (parent_key(entry.key.parent), entry.key.name.clone());
                if operation.entries.contains_key(&key) {
                    return Err(PersistenceError::Malformed.into());
                }
                let path = entry.native_path.as_deref();
                let mut first = None;
                if let (Some(own), Some(native), Some(path)) = (entry.position, &entry.native, path)
                {
                    first = Some(operation.bind(own, native, path)?);
                    canonical.extend(first);
                }
                let directory = entry.kind == InodeKind::Directory;
                if let Some(position) = entry.position.filter(|_| directory) {
                    operation.directories.insert(position, key.clone());
                }
                let bytes = (entry.key.name.len() + path.map_or(0, <[u8]>::len)) as u64 + 72;
                operation.entries.insert(
                    key,
                    Row {
                        position: entry.position,
                        kind: entry.kind,
                        canonical: first,
                        metadata_root: entry.metadata_root,
                        content_root: entry.target_root,
                        native_path: entry.native_path.clone().filter(|_| directory),
                        native: entry.native.filter(|_| entry.position.is_none()),
                        bytes,
                    },
                );
                operation.charge(1, bytes);
            }
            Ok(canonical)
        })
    }

    fn unplaced_children(
        &self,
        owner: Owner,
        parent: u64,
        after: Option<&[u8]>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<Unplaced>> {
        self.read("unplaced_children", owner, limits, |operation, rows| {
            let parent = parent as i64;
            // A child's name is never empty, so the empty name precedes them all.
            let start = Excluded((parent, after.map_or(Vec::new(), <[u8]>::to_vec)));
            let children = operation.entries.range((start, Unbounded));
            Ok(children
                .take_while(|((held, _), _)| *held == parent)
                .filter(|(_, row)| row.position.is_none())
                .take(rows)
                .map(|((_, name), row)| Unplaced {
                    name: name.clone(),
                    kind: row.kind,
                    native: row.native,
                })
                .collect())
        })
    }

    fn place_children(
        &self,
        owner: Owner,
        parent: u64,
        placed: &[Placed],
    ) -> AcquisitionResult<Vec<u64>> {
        let bytes = placed.iter().map(|child| {
            child.name.len()
                + child.native.as_ref().map_or(0, |(_, path)| path.len())
                + WRITE_ROW_BYTES
        });
        self.written(placed.len(), bytes.sum())?;
        self.unit("place_children", owner, |operation| {
            let mut canonical = Vec::new();
            for child in placed {
                let changed = AcquisitionError::Changed {
                    position: child.position,
                };
                let key = (parent as i64, child.name.clone());
                let row = operation.entries.get(&key).ok_or(changed.clone())?;
                let regular = row.kind == InodeKind::RegularFile;
                if row.position.is_some() || regular != child.native.is_some() {
                    return Err(changed);
                }
                let directory = row.kind == InodeKind::Directory;
                let mut first = None;
                if let Some((native, path)) = &child.native {
                    first = Some(operation.bind(child.position, native, path)?);
                    canonical.extend(first);
                }
                let row = operation.entries.get_mut(&key).expect("checked above");
                row.position = Some(child.position);
                row.canonical = first;
                row.native = None;
                if directory {
                    operation.directories.insert(child.position, key);
                }
            }
            Ok(canonical)
        })
    }

    fn directories(
        &self,
        owner: Owner,
        after: Option<u64>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<Directory>> {
        self.read("directories", owner, limits, |operation, rows| {
            let start = after.map_or(Unbounded, Excluded);
            let listed = operation.directories.range((start, Unbounded)).take(rows);
            Ok(listed
                .map(|(position, key)| Directory {
                    position: *position,
                    native_path: operation.entries[key].native_path.clone().expect("path"),
                })
                .collect())
        })
    }

    fn directory_path(&self, owner: Owner, position: u64) -> AcquisitionResult<Option<Vec<u8>>> {
        self.unit("directory_path", owner, |operation| {
            let key = operation.directories.get(&position);
            Ok(key.and_then(|key| operation.entries[key].native_path.clone()))
        })
    }

    fn jobs(
        &self,
        owner: Owner,
        after: Option<u64>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<Job>> {
        self.read("jobs", owner, limits, |operation, rows| {
            let start = after.map_or(Unbounded, Excluded);
            let listed = operation.natives.range((start, Unbounded)).take(rows);
            Ok(listed
                .map(|(position, native)| Job {
                    position: *position,
                    native: native.identity,
                    native_path: native.path.clone(),
                })
                .collect())
        })
    }

    fn job(&self, owner: Owner, position: u64) -> AcquisitionResult<Option<Job>> {
        self.unit("job", owner, |operation| {
            Ok(operation.natives.get(&position).map(|native| Job {
                position,
                native: native.identity,
                native_path: native.path.clone(),
            }))
        })
    }

    fn complete_files(&self, owner: Owner, roots: &[(u64, ObjectId)]) -> AcquisitionResult<()> {
        self.written(roots.len(), roots.len() * 48)?;
        self.unit("complete_files", owner, |operation| {
            for (position, root) in roots {
                let native = operation.natives.get_mut(position);
                let native = native.filter(|native| native.root.is_none());
                let native = native.ok_or(AcquisitionError::Changed {
                    position: *position,
                })?;
                native.root = Some(*root);
                native.bytes += 32;
            }
            operation.charge(0, roots.len() as u64 * 32);
            Ok(())
        })
    }

    fn file_roots(
        &self,
        owner: Owner,
        after: Option<u64>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<FileRoot>> {
        self.read("file_roots", owner, limits, |operation, rows| {
            let start = after.map_or(Unbounded, Excluded);
            let listed = operation.natives.range((start, Unbounded)).take(rows);
            Ok(listed
                .map(|(position, native)| FileRoot {
                    position: *position,
                    aliases: native.aliases,
                    root: native.root,
                })
                .collect())
        })
    }

    fn entries(
        &self,
        owner: Owner,
        after: Option<&EntryKey>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<Entry>> {
        self.read("entries", owner, limits, |operation, rows| {
            let start = match after {
                Some(key) => Excluded((parent_key(key.parent), key.name.clone())),
                None => Unbounded,
            };
            let listed = operation.entries.range((start, Unbounded)).take(rows);
            listed
                .map(|((parent, name), row)| {
                    Ok(Entry {
                        key: EntryKey {
                            parent: (*parent >= 0).then_some(*parent as u64),
                            name: name.clone(),
                        },
                        position: row.position.ok_or(PersistenceError::Malformed)?,
                        kind: row.kind,
                        canonical: row.canonical,
                        metadata_root: row.metadata_root,
                        content_root: row.content_root,
                    })
                })
                .collect()
        })
    }

    fn set_directory_roots(
        &self,
        owner: Owner,
        roots: &[(u64, ObjectId)],
    ) -> AcquisitionResult<()> {
        self.written(roots.len(), roots.len() * 48)?;
        self.unit("set_directory_roots", owner, |operation| {
            for (position, root) in roots {
                let key = operation.directories.get(position);
                let row = key.and_then(|key| operation.entries.get_mut(key));
                let row = row.filter(|row| row.content_root.is_none());
                let row = row.ok_or(AcquisitionError::Changed {
                    position: *position,
                })?;
                row.content_root = Some(*root);
                row.bytes += 32;
            }
            operation.charge(0, roots.len() as u64 * 32);
            Ok(())
        })
    }

    fn advance(&self, owner: Owner, phase: Phase) -> AcquisitionResult<()> {
        self.unit("advance", owner, |operation| {
            operation.phase = phase;
            Ok(())
        })
    }

    fn discard(&self, owner: Owner, rows: usize) -> AcquisitionResult<Discarded> {
        self.unit("discard", owner, |operation| {
            let mut budget = rows.min(WRITE_WINDOW_ROWS);
            let (mut removed, mut bytes) = (0u64, 0u64);
            while budget > 0 {
                let Some((_, row)) = operation.entries.pop_first() else {
                    break;
                };
                (removed, bytes, budget) = (removed + 1, bytes + row.bytes, budget - 1);
            }
            while budget > 0 {
                let Some((_, native)) = operation.natives.pop_first() else {
                    break;
                };
                (removed, bytes, budget) = (removed + 1, bytes + native.bytes, budget - 1);
            }
            if operation.entries.is_empty() {
                operation.directories.clear();
            }
            if operation.natives.is_empty() {
                operation.identities.clear();
            }
            let work = &mut operation.work;
            work.held_rows -= removed;
            work.held_bytes -= bytes;
            work.removed_rows += removed;
            work.removed_bytes += bytes;
            Ok(Discarded {
                rows: removed,
                bytes,
                remaining_rows: work.held_rows,
            })
        })
    }

    fn release(&self, owner: Owner) -> AcquisitionResult<()> {
        self.unit("release", owner, |operation| {
            if operation.work.held_rows != 0 {
                return Err(PersistenceError::Refused {
                    status: "acquisition working rows remain".into(),
                }
                .into());
            }
            Ok(())
        })?;
        self.state
            .lock()
            .unwrap()
            .operations
            .remove(&owner.operation);
        Ok(())
    }

    fn work(&self, owner: Owner) -> AcquisitionResult<AcquisitionWork> {
        self.unit("work", owner, |operation| Ok(operation.work))
    }

    fn abandoned(&self, _after: Option<u64>, _limit: usize) -> AcquisitionResult<Vec<Abandoned>> {
        Ok(Vec::new())
    }

    fn discard_abandoned(&self, _abandoned: Owner, _rows: usize) -> AcquisitionResult<Discarded> {
        Err(AcquisitionError::Stale)
    }
}
