//! A merge's working state outside memory (#355): runs of chunked entries,
//! a spilled stack and points, all in the caller's indexed records.
//!
//! The serials a merge defers are written as one run of chunks of at most
//! [`CHUNK_BYTES`], read once in order. Chunks and points stay in memory up
//! to [`HELD_BYTES`]; past it they go to the records, and a chunk read once
//! is released at once. So a small merge writes none of its own state to the
//! Store and a large one holds the same memory as any other. A merge without
//! records (the in-lane step, bounded by its caller) uses [`Resident`]
//! records instead.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};

use crate::error::{ContentError, ContentResult};
use crate::{
    EditRecordApply, EditRecordChange, EditRecordExpected, EditRecordKey, IndexedEditBacking,
};

// Merge kinds share the operation's records with Content's filesystem kinds
// (0x4653_xxxx); 0x4D47 is "MG".
/// (run, chunk) to one chunk of sorted entries.
pub(super) const RUN: u32 = 0x4D47_0001;
/// (stack, chunk) to one chunk of a spilled stack.
pub(super) const STACK: u32 = 0x4D47_0002;
/// Parent serial to its change count and chunk count.
pub(super) const HEADER: u32 = 0x4D47_0004;
/// (parent, chunk) to one chunk of the parent's changed names.
pub(super) const CHANGES: u32 = 0x4D47_0005;
/// (parent, chunk) to the first name of that chunk.
pub(super) const FIRST: u32 = 0x4D47_0006;
/// Serial to its final inode value.
pub(super) const VALUE: u32 = 0x4D47_0007;
/// Fresh serial to its rank in serial order.
pub(super) const FRESH: u32 = 0x4D47_0008;
/// Source serial to the values given to it and the serial each received.
pub(super) const ASSIGNED: u32 = 0x4D47_0009;
/// A deleted `from` serial whose binding the merge keeps.
pub(super) const CANCEL_REMOVE: u32 = 0x4D47_000B;
/// A directory whose `from` move the merge cancels.
pub(super) const CANCEL_MOVE: u32 = 0x4D47_000C;
/// A directory `from` deleted that keeps some of its names.
pub(super) const SURVIVE: u32 = 0x4D47_000D;
/// A serial `into` lacks, revived with only what it keeps.
pub(super) const REVIVE: u32 = 0x4D47_000E;
/// A serial `into` deleted that `from` bound: resurrected as `from` holds it.
pub(super) const REVIVE_ALL: u32 = 0x4D47_000F;
/// Bindings of one directory outside its name points.
pub(super) const EXTRA: u32 = 0x4D47_0010;
/// A directory `from` moved: its new parent and name.
pub(super) const MOVED: u32 = 0x4D47_0011;
/// A directory merged into another at one name.
pub(super) const ABSORB: u32 = 0x4D47_0012;
/// A directory that absorbed another: the absorbed one and the base's.
pub(super) const TARGET: u32 = 0x4D47_0013;
/// One name's decided binding.
pub(super) const NAMED: u32 = 0x4D47_0014;
/// A directory whose header is written.
pub(super) const DONE: u32 = 0x4D47_0015;
/// A serial new on `from` that the merge leaves unbound.
pub(super) const DROP: u32 = 0x4D47_0016;
/// A file new on `from` with several links: how many the merge lost.
pub(super) const LOST: u32 = 0x4D47_0017;

/// Bytes of one chunk record.
pub(super) const CHUNK_BYTES: usize = 60 * 1024;
/// Bytes of chunks and points a merge holds in memory before the records
/// take the rest.
pub(super) const HELD_BYTES: usize = 8 << 20;
/// Bytes one held entry is charged besides its value.
const HELD_ENTRY: usize = 64;

/// Two numbers as an order-preserving record key.
pub(super) fn pair(a: u64, b: u64) -> [u8; 32] {
    let mut key = [0; 32];
    key[..8].copy_from_slice(&a.to_be_bytes());
    key[8..16].copy_from_slice(&b.to_be_bytes());
    key
}

/// One number as an order-preserving record key.
pub(super) fn single(a: u64) -> [u8; 32] {
    pair(a, 0)
}

/// The first number of a key.
pub(super) fn first(key: &[u8; 32]) -> u64 {
    u64::from_be_bytes(key[..8].try_into().expect("eight bytes"))
}

fn malformed() -> ContentError {
    ContentError::InvalidRecord("merge spill record")
}

/// The merge's records, shared by its passes and the update it feeds. Each
/// call borrows them for that call only.
pub(super) struct Spill<'r> {
    records: RefCell<&'r mut dyn IndexedEditBacking>,
    next: std::cell::Cell<u64>,
    held: RefCell<Held>,
}

/// The merge's own records kept in memory, within [`HELD_BYTES`].
#[derive(Default)]
struct Held {
    rows: BTreeMap<EditRecordKey, Vec<u8>>,
    bytes: usize,
    /// Kinds with a record in the records: only these are read there.
    spilled: std::collections::BTreeSet<u32>,
}

impl<'r> Spill<'r> {
    pub(super) fn new(records: &'r mut dyn IndexedEditBacking) -> Self {
        Self {
            records: RefCell::new(records),
            next: std::cell::Cell::new(0),
            held: RefCell::new(Held::default()),
        }
    }

    /// A number no other run or stack of this merge uses.
    pub(super) fn allocate(&self) -> u64 {
        let id = self.next.get();
        self.next.set(id + 1);
        id
    }

    pub(super) fn get(&self, kind: u32, key: [u8; 32]) -> ContentResult<Option<Vec<u8>>> {
        let key = EditRecordKey { kind, key };
        let held = self.held.borrow();
        if let Some(value) = held.rows.get(&key) {
            return Ok(Some(value.clone()));
        }
        if !held.spilled.contains(&kind) {
            return Ok(None);
        }
        drop(held);
        self.records.borrow_mut().get(key)
    }

    /// Reads a record and releases it from memory: a chunk read once. A
    /// spilled one stays in the records until the merge ends.
    pub(super) fn take(&self, kind: u32, key: [u8; 32]) -> ContentResult<Option<Vec<u8>>> {
        let key = EditRecordKey { kind, key };
        let mut held = self.held.borrow_mut();
        if let Some(value) = held.rows.remove(&key) {
            held.bytes -= HELD_ENTRY + value.len();
            return Ok(Some(value));
        }
        if !held.spilled.contains(&kind) {
            return Ok(None);
        }
        drop(held);
        self.records.borrow_mut().get(key)
    }

    /// Writes a record the caller knows does not exist yet in either place:
    /// a new number's chunk, or a key the caller has just found absent.
    pub(super) fn insert(&self, kind: u32, key: [u8; 32], value: Vec<u8>) -> ContentResult<()> {
        let key = EditRecordKey { kind, key };
        let mut held = self.held.borrow_mut();
        if held.bytes + HELD_ENTRY + value.len() <= HELD_BYTES {
            held.bytes += HELD_ENTRY + value.len();
            if held.rows.insert(key, value).is_some() {
                return Err(malformed());
            }
            return Ok(());
        }
        held.spilled.insert(kind);
        drop(held);
        self.change(key, EditRecordExpected::Missing, Some(value))
    }

    /// Replaces a record whose current bytes are `old`.
    pub(super) fn replace(
        &self,
        kind: u32,
        key: [u8; 32],
        old: Vec<u8>,
        value: Vec<u8>,
    ) -> ContentResult<()> {
        let key = EditRecordKey { kind, key };
        let mut held = self.held.borrow_mut();
        if let Some(current) = held.rows.get_mut(&key) {
            if *current != old {
                return Err(malformed());
            }
            let grown = value.len() as isize - current.len() as isize;
            *current = value;
            held.bytes = (held.bytes as isize + grown) as usize;
            return Ok(());
        }
        drop(held);
        self.change(key, EditRecordExpected::ExactBytes(old), Some(value))
    }

    fn change(
        &self,
        key: EditRecordKey,
        expected: EditRecordExpected,
        value: Option<Vec<u8>>,
    ) -> ContentResult<()> {
        let applied = self.records.borrow_mut().apply(vec![EditRecordChange {
            key,
            expected,
            value,
        }])?;
        match applied {
            EditRecordApply::Applied => Ok(()),
            EditRecordApply::NotApplied { .. } => Err(malformed()),
        }
    }

    /// At most 64 keys of one kind after `after`, held and spilled together.
    pub(super) fn keys_after(
        &self,
        kind: u32,
        after: Option<[u8; 32]>,
    ) -> ContentResult<Vec<[u8; 32]>> {
        let spilled = if self.held.borrow().spilled.contains(&kind) {
            self.records.borrow_mut().keys_after(kind, after)?
        } else {
            Vec::new()
        };
        let held = self.held.borrow();
        let start = EditRecordKey {
            kind,
            key: after.unwrap_or([0; 32]),
        };
        let mut keys: Vec<[u8; 32]> = held
            .rows
            .range(start..)
            .map(|(key, _)| *key)
            .take_while(|key| key.kind == kind)
            .filter(|key| Some(key.key) != after)
            .take(64)
            .map(|key| key.key)
            .collect();
        // A spilled window that is full ends before keys it did not return;
        // held keys past its last are left for the next window.
        let bound = (spilled.len() == 64).then(|| spilled[63]);
        keys.retain(|key| bound.is_none_or(|bound| *key <= bound));
        keys.extend(spilled);
        keys.sort_unstable();
        keys.dedup();
        keys.truncate(64);
        Ok(keys)
    }

    /// The records as the update's backing, borrowed per call.
    pub(super) fn backing(&self) -> Shared<'_, 'r> {
        Shared(self)
    }
}

/// [`Spill`]'s records lent to the update for its own kinds.
pub(super) struct Shared<'s, 'r>(&'s Spill<'r>);

impl IndexedEditBacking for Shared<'_, '_> {
    fn contains(&mut self, key: EditRecordKey) -> ContentResult<bool> {
        self.0.records.borrow_mut().contains(key)
    }
    fn get(&mut self, key: EditRecordKey) -> ContentResult<Option<Vec<u8>>> {
        self.0.records.borrow_mut().get(key)
    }
    fn get_many(&mut self, keys: &[EditRecordKey]) -> ContentResult<Vec<Option<Vec<u8>>>> {
        self.0.records.borrow_mut().get_many(keys)
    }
    fn apply(&mut self, changes: Vec<EditRecordChange>) -> ContentResult<EditRecordApply> {
        self.0.records.borrow_mut().apply(changes)
    }
    fn first_keys(
        &mut self,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> ContentResult<Vec<[u8; 32]>> {
        self.0.records.borrow_mut().first_keys(kind, excluded)
    }
    fn keys_after(&mut self, kind: u32, after: Option<[u8; 32]>) -> ContentResult<Vec<[u8; 32]>> {
        self.0.records.borrow_mut().keys_after(kind, after)
    }
}

/// Records kept in memory, for a merge its caller bounds.
#[derive(Default)]
pub(super) struct Resident(BTreeMap<EditRecordKey, Vec<u8>>);

impl IndexedEditBacking for Resident {
    fn contains(&mut self, key: EditRecordKey) -> ContentResult<bool> {
        Ok(self.0.contains_key(&key))
    }
    fn get(&mut self, key: EditRecordKey) -> ContentResult<Option<Vec<u8>>> {
        Ok(self.0.get(&key).cloned())
    }
    fn apply(&mut self, changes: Vec<EditRecordChange>) -> ContentResult<EditRecordApply> {
        for (index, change) in changes.iter().enumerate() {
            let actual = self.0.get(&change.key);
            let matches = match &change.expected {
                EditRecordExpected::Missing => actual.is_none(),
                EditRecordExpected::ExactBytes(bytes) => actual == Some(bytes),
            };
            if !matches {
                return Ok(EditRecordApply::NotApplied {
                    index,
                    key: change.key,
                    actual: actual.cloned(),
                });
            }
        }
        for change in changes {
            match change.value {
                Some(value) => self.0.insert(change.key, value),
                None => self.0.remove(&change.key),
            };
        }
        Ok(EditRecordApply::Applied)
    }
    fn first_keys(
        &mut self,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> ContentResult<Vec<[u8; 32]>> {
        Ok(self
            .window(kind, None)
            .filter(|key| Some(*key) != excluded)
            .take(64)
            .collect())
    }
    fn keys_after(&mut self, kind: u32, after: Option<[u8; 32]>) -> ContentResult<Vec<[u8; 32]>> {
        Ok(self.window(kind, after).take(64).collect())
    }
}

impl Resident {
    fn window(&self, kind: u32, after: Option<[u8; 32]>) -> impl Iterator<Item = [u8; 32]> + '_ {
        let start = EditRecordKey {
            kind,
            key: after.unwrap_or([0; 32]),
        };
        self.0
            .range(start..)
            .map(|(key, _)| *key)
            .take_while(move |key| key.kind == kind)
            .filter(move |key| Some(key.key) != after)
            .map(|key| key.key)
    }
}

/// Entries of one chunk: each a 4-byte length and its bytes.
fn decode_chunk(bytes: &[u8]) -> ContentResult<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let length = bytes
            .get(at..at + 4)
            .ok_or_else(malformed)?
            .try_into()
            .map(u32::from_be_bytes)
            .map_err(|_| malformed())? as usize;
        out.push(
            bytes
                .get(at + 4..at + 4 + length)
                .ok_or_else(malformed)?
                .to_vec(),
        );
        at += 4 + length;
    }
    Ok(out)
}

/// One written run: its number and chunk count.
#[derive(Clone, Copy, Debug)]
pub(super) struct Run {
    id: u64,
    chunks: u64,
}

/// Writes entries, in the caller's order, as one run.
pub(super) struct RunWriter {
    id: u64,
    chunks: u64,
    chunk: Vec<u8>,
}

impl RunWriter {
    pub(super) fn new(spill: &Spill<'_>) -> Self {
        Self {
            id: spill.allocate(),
            chunks: 0,
            chunk: Vec::new(),
        }
    }

    pub(super) fn push(&mut self, spill: &Spill<'_>, entry: &[u8]) -> ContentResult<()> {
        if 4 + entry.len() > CHUNK_BYTES {
            return Err(malformed());
        }
        if self.chunk.len() + 4 + entry.len() > CHUNK_BYTES {
            self.flush(spill)?;
        }
        self.chunk
            .extend_from_slice(&(entry.len() as u32).to_be_bytes());
        self.chunk.extend_from_slice(entry);
        Ok(())
    }

    fn flush(&mut self, spill: &Spill<'_>) -> ContentResult<()> {
        if !self.chunk.is_empty() {
            spill.insert(
                RUN,
                pair(self.id, self.chunks),
                std::mem::take(&mut self.chunk),
            )?;
            self.chunks += 1;
        }
        Ok(())
    }

    pub(super) fn finish(mut self, spill: &Spill<'_>) -> ContentResult<Run> {
        self.flush(spill)?;
        Ok(Run {
            id: self.id,
            chunks: self.chunks,
        })
    }
}

/// Reads one run in order.
pub(super) struct RunReader {
    run: Run,
    /// Chunks read so far.
    read: u64,
    entries: VecDeque<Vec<u8>>,
}

impl RunReader {
    /// Reads the run once, releasing each chunk as it goes.
    pub(super) fn new(run: Run) -> Self {
        Self {
            run,
            read: 0,
            entries: VecDeque::new(),
        }
    }

    pub(super) fn next(&mut self, spill: &Spill<'_>) -> ContentResult<Option<Vec<u8>>> {
        while self.entries.is_empty() {
            if self.read == self.run.chunks {
                return Ok(None);
            }
            let chunk = self.read;
            self.read += 1;
            let bytes = spill
                .take(RUN, pair(self.run.id, chunk))?
                .ok_or_else(malformed)?;
            self.entries = decode_chunk(&bytes)?.into();
        }
        Ok(self.entries.pop_front())
    }
}

/// A last-in first-out stack whose older entries spill to chunks.
pub(super) struct SpillStack {
    id: u64,
    top: Vec<Vec<u8>>,
    bytes: usize,
    /// Slots of the chunks below `top`, last on top; a slot is never reused.
    slots: Vec<u64>,
    next: u64,
}

impl SpillStack {
    pub(super) fn new(spill: &Spill<'_>) -> Self {
        Self {
            id: spill.allocate(),
            top: Vec::new(),
            bytes: 0,
            slots: Vec::new(),
            next: 0,
        }
    }

    pub(super) fn push(&mut self, spill: &Spill<'_>, entry: Vec<u8>) -> ContentResult<()> {
        if self.bytes + 4 + entry.len() > CHUNK_BYTES {
            let mut chunk = Vec::with_capacity(self.bytes);
            for entry in self.top.drain(..) {
                chunk.extend_from_slice(&(entry.len() as u32).to_be_bytes());
                chunk.extend_from_slice(&entry);
            }
            spill.insert(STACK, pair(self.id, self.next), chunk)?;
            self.slots.push(self.next);
            self.next += 1;
            self.bytes = 0;
        }
        self.bytes += 4 + entry.len();
        self.top.push(entry);
        Ok(())
    }

    pub(super) fn pop(&mut self, spill: &Spill<'_>) -> ContentResult<Option<Vec<u8>>> {
        if self.top.is_empty() {
            if let Some(slot) = self.slots.pop() {
                let bytes = spill
                    .take(STACK, pair(self.id, slot))?
                    .ok_or_else(malformed)?;
                self.top = decode_chunk(&bytes)?;
                self.bytes = self.top.iter().map(|entry| 4 + entry.len()).sum();
            }
        }
        let entry = self.top.pop();
        if let Some(entry) = &entry {
            self.bytes -= 4 + entry.len();
        }
        Ok(entry)
    }
}
