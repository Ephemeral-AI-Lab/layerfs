//! `TreeStore`: the harness's authenticated canonical-object provider.
//!
//! It is the **only** provider in this harness that authenticates on read. Every
//! value it hands back is re-identified with the frozen object identity function
//! and refused on a mismatch, so a construction that silently returned the wrong
//! bytes cannot be measured as if it had returned the right ones.
//!
//! It is also why the examples' `Provider` is not reused: that one is a linear
//! `Vec::find`, which is O(n^2) at 500 MiB and would make the *provider* the
//! measured cost rather than the operation under test.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId,
};

/// In-memory canonical objects, authenticated on every read.
#[derive(Default)]
pub struct TreeStore {
    /// Canonical objects, kept whole.
    ///
    /// A `Vec<u8>` would be enough to serve a C1 read, but a C2 row has to offer
    /// the same objects to a Store, and `FinalizedObject::new` takes a **role**:
    /// re-wrapping bytes under a guessed role would store the wrong envelope.
    /// Keeping the finalized object is the only faithful way to replay it.
    objects: HashMap<ObjectId, FinalizedObject>,
    insertion_order: Vec<ObjectId>,
    /// Every identity this provider was asked for, in demand order, with repeats.
    ///
    /// Behind a `RefCell` because [`AuthenticatedObjects::read_canonical_batch`]
    /// takes `&self` and the demand record is what the oracle's coverage check
    /// reads. It is the same shape the C1 example vehicles use, and it records what
    /// the *caller asked for* rather than what the provider believes it did.
    demanded: RefCell<Vec<ObjectId>>,
}

impl TreeStore {
    /// Empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of distinct objects held.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Whether the store holds nothing.
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Identities in insertion order.
    pub fn insertion_order(&self) -> &[ObjectId] {
        &self.insertion_order
    }

    /// Every identity ever demanded, in demand order, with repeats.
    pub fn demanded(&self) -> Vec<ObjectId> {
        self.demanded.borrow().clone()
    }

    /// Distinct identities demanded.
    pub fn distinct_demanded(&self) -> usize {
        let mut seen = self.demanded.borrow().clone();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    }

    /// Canonical bytes of one object, without authentication.
    pub fn canonical(&self, id: ObjectId) -> Option<&[u8]> {
        self.objects.get(&id).map(|object| object.canonical())
    }

    /// The finalized object, for a caller that has to offer it again.
    pub fn object(&self, id: ObjectId) -> Option<&FinalizedObject> {
        self.objects.get(&id)
    }

    /// A copy of the finalized object, for a Store's `accept`.
    ///
    /// `Store::accept` takes the object by value, so offering one costs a copy of
    /// its canonical bytes. When this happens inside a measured region it is
    /// harness work inside the product's timer, and the phase clock records the span
    /// so it is published as `handoff_ns` rather than absorbed into the golden
    /// number.
    pub fn cloned_object(&self, id: ObjectId) -> Option<FinalizedObject> {
        crate::support::phases::handoff(|| self.objects.get(&id).cloned())
    }

    /// Merges every object of `other` into this store.
    ///
    /// A filesystem operation emits only what it changed; a caller that applies
    /// several operations in sequence keeps one store per operation and reads
    /// through a [`PairProvider`], or absorbs each result here. Absorbing copies
    /// the objects, which is why the drivers that only need to *read* a chain use
    /// the provider instead.
    pub fn absorb(&mut self, other: &TreeStore) {
        for id in &other.insertion_order {
            if let Some(object) = other.objects.get(id) {
                self.insert_object(object.clone());
            }
        }
    }

    /// Stores one finalized object under its own identity.
    pub fn insert_object(&mut self, object: FinalizedObject) -> ObjectId {
        let id = object.id();
        if self.objects.insert(id, object).is_none() {
            self.insertion_order.push(id);
        }
        id
    }

    /// Writes every object to `directory` as `<hex id>.bin`.
    ///
    /// This is the producer the `prepare` verb needs: construction is a product
    /// operation, so Python cannot build the artifact itself.
    pub fn write_to_dir(&self, directory: &Path) -> std::io::Result<usize> {
        std::fs::create_dir_all(directory)?;
        let mut written = 0;
        for id in &self.insertion_order {
            let Some(object) = self.objects.get(id) else {
                continue;
            };
            std::fs::write(directory.join(format!("{id}.bin")), object.canonical())?;
            written += 1;
        }
        Ok(written)
    }

    /// Loads every `<hex id>.bin` in `directory`, refusing any whose bytes do not
    /// re-identify to the name it was stored under.
    ///
    /// A refusal is returned as a count rather than swallowed: an artifact that
    /// does not authenticate is `INCOMPLETE`, never an empty provider that makes a
    /// construction fail for the wrong reason.
    pub fn load_from_dir(directory: &Path) -> std::io::Result<(Self, usize)> {
        let mut store = Self::new();
        let mut refused = 0;
        let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(directory)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "bin"))
            .collect();
        entries.sort();
        for path in entries {
            let canonical = std::fs::read(&path)?;
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default();
            // A loaded object is re-wrapped under its own identity; its role is
            // recovered from the envelope only when the caller replays it through
            // a path that needs one. `load_from_dir` therefore serves C1 reads and
            // the `--emit-objects` round trip, and a C2 replay uses the in-process
            // store instead.
            match stem.parse::<ObjectId>() {
                Ok(expected) if expected == ObjectId::for_bytes(&canonical) => {
                    if let Ok(object) =
                        FinalizedObject::new(layerfs_content::ObjectRole::Chunk, canonical)
                    {
                        store.insert_object(object);
                    } else {
                        refused += 1;
                    }
                }
                _ => refused += 1,
            }
        }
        Ok((store, refused))
    }
}

impl FinalizedConsumer for TreeStore {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.insert_object(object);
        Ok(())
    }
}

impl AuthenticatedObjects for TreeStore {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.demanded.borrow_mut().extend_from_slice(ids);
        let mut values = Vec::with_capacity(ids.len());
        for id in ids {
            match self.objects.get(id) {
                Some(object) if ObjectId::for_bytes(object.canonical()) == *id => {
                    values.push(object.canonical().to_vec());
                }
                // A stored object that does not re-identify is a hard failure, not
                // a miss: the provider contract distinguishes them, and collapsing
                // them would hide corruption behind an absence.
                Some(_) => return Err(ContentError::IdentityMismatch),
                None => return Err(ContentError::MissingObject),
            }
        }
        Ok(values)
    }
}

/// Coverage: what the caller demanded, expressed against what it was given.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Coverage {
    /// Distinct identities demanded.
    pub demanded: u64,
    /// Identities the provider holds.
    pub held: u64,
    /// Canonical bytes the provider holds.
    pub held_bytes: u64,
}

impl Coverage {
    /// Reads a coverage reading off a store.
    pub fn of(store: &TreeStore) -> Self {
        Self {
            demanded: store.distinct_demanded() as u64,
            held: store.len() as u64,
            held_bytes: store
                .insertion_order()
                .iter()
                .filter_map(|id| store.canonical(*id))
                .map(|canonical| canonical.len() as u64)
                .sum(),
        }
    }

    /// Whether every identity the caller demanded was one the provider held.
    pub fn every_demand_served(&self) -> bool {
        self.demanded <= self.held
    }
}

/// A provider over two stores: the result first, then the base it was built from.
///
/// An edit emits only what it changed; everything it kept is still addressed by
/// identity in the base. Reading the result back therefore needs both, and the
/// alternative — copying the result into the base's map — would double the
/// harness's own memory inside the very window whose memory is being measured.
pub struct PairProvider<'a> {
    /// Objects the operation emitted.
    pub result: &'a TreeStore,
    /// Objects the operation started from.
    pub base: &'a TreeStore,
    demanded: RefCell<Vec<ObjectId>>,
    /// Objects served by `result`, counted where they are served.
    ///
    /// This is the reading that makes a fixture-served update falsifiable: an
    /// operation that demanded nothing from `result` did not read back what it
    /// emitted, so the fixture was not load-bearing and the row's whole
    /// construction would be describing a different operation than it claims.
    served_result: std::cell::Cell<u64>,
    /// Objects served by `base`.
    served_base: std::cell::Cell<u64>,
}

impl<'a> PairProvider<'a> {
    /// Wraps two stores.
    pub fn new(result: &'a TreeStore, base: &'a TreeStore) -> Self {
        Self {
            result,
            base,
            demanded: RefCell::new(Vec::new()),
            served_result: std::cell::Cell::new(0),
            served_base: std::cell::Cell::new(0),
        }
    }

    /// Every identity demanded, in demand order, with repeats.
    pub fn demanded(&self) -> Vec<ObjectId> {
        self.demanded.borrow().clone()
    }

    /// Objects served from the result store, and from the base.
    pub fn served(&self) -> (u64, u64) {
        (self.served_result.get(), self.served_base.get())
    }
}

impl AuthenticatedObjects for PairProvider<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.demanded.borrow_mut().extend_from_slice(ids);
        let mut values = Vec::with_capacity(ids.len());
        for id in ids {
            let from_result = self.result.object(*id);
            let found = from_result.or_else(|| self.base.object(*id));
            match found {
                Some(object) if ObjectId::for_bytes(object.canonical()) == *id => {
                    if from_result.is_some() {
                        self.served_result.set(self.served_result.get() + 1);
                    } else {
                        self.served_base.set(self.served_base.get() + 1);
                    }
                    values.push(object.canonical().to_vec());
                }
                Some(_) => return Err(ContentError::IdentityMismatch),
                None => return Err(ContentError::MissingObject),
            }
        }
        Ok(values)
    }
}

/// A consumer that forwards to another consumer and counts what it forwarded.
///
/// The integrated pipeline needs to know that every object C1 emitted reached
/// the save operation. `SaveHandoff` reports the *storage* failure and not the
/// count, so the count is taken here, on the one path the objects actually take,
/// rather than reconstructed from the operation's own counters afterwards.
pub struct CountingConsumer<'a> {
    inner: &'a mut dyn FinalizedConsumer,
    accepted: u64,
}

impl<'a> CountingConsumer<'a> {
    /// Wraps a consumer.
    pub fn new(inner: &'a mut dyn FinalizedConsumer) -> Self {
        Self { inner, accepted: 0 }
    }

    /// Objects forwarded so far.
    pub fn accepted(&self) -> u64 {
        self.accepted
    }
}

impl FinalizedConsumer for CountingConsumer<'_> {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.inner.accept(object)?;
        self.accepted += 1;
        Ok(())
    }
}

/// A consumer and a reader over the **same** in-flight objects.
///
/// A filesystem *update* reads back objects it emitted earlier in the same
/// operation, so a reader that only sees the base cannot serve it. The product's
/// own external tests solve this with an overlay: the reader checks what the
/// operation has emitted so far, then falls back to the base. This is that
/// overlay, and it is why an update-shaped row cannot use a purely discarding
/// measured phase.
pub struct SharedStore {
    inner: Rc<RefCell<TreeStore>>,
}

impl SharedStore {
    /// A fresh shared store.
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(TreeStore::new())),
        }
    }

    /// A reader that sees this store's objects before `base`.
    pub fn reader<'a>(&self, base: &'a TreeStore) -> SharedReader<'a> {
        SharedReader {
            emitted: Rc::clone(&self.inner),
            base,
        }
    }

    /// Absorbs everything emitted so far into `target`.
    ///
    /// A caller that runs a chain of operations keeps one store per operation and
    /// reads through a [`PairProvider`]; a caller that has to hand a *later*
    /// operation the objects an *earlier* one emitted absorbs them here, which
    /// copies rather than aliases.
    pub fn absorb_into(&self, target: &mut TreeStore) {
        target.absorb(&self.inner.borrow());
    }

    /// Number of objects emitted so far.
    pub fn len(&self) -> usize {
        self.inner.borrow().len()
    }

    /// Whether nothing has been emitted.
    pub fn is_empty(&self) -> bool {
        self.inner.borrow().is_empty()
    }
}

impl Default for SharedStore {
    fn default() -> Self {
        Self::new()
    }
}

impl FinalizedConsumer for SharedStore {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.inner.borrow_mut().accept(object)
    }
}

/// A reader over the in-flight objects first, then the base they were built from.
pub struct SharedReader<'a> {
    emitted: Rc<RefCell<TreeStore>>,
    base: &'a TreeStore,
}

impl AuthenticatedObjects for SharedReader<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.emitted
            .borrow()
            .read_canonical_batch(ids)
            .or_else(|_| self.base.read_canonical_batch(ids))
    }
}
