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

use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId,
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
    pub fn cloned_object(&self, id: ObjectId) -> Option<FinalizedObject> {
        self.objects.get(&id).cloned()
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
                    if let Ok(object) = FinalizedObject::new(
                        layerfs_content::ObjectRole::Chunk,
                        canonical,
                    ) {
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
}

impl<'a> PairProvider<'a> {
    /// Wraps two stores.
    pub fn new(result: &'a TreeStore, base: &'a TreeStore) -> Self {
        Self {
            result,
            base,
            demanded: RefCell::new(Vec::new()),
        }
    }

    /// Every identity demanded, in demand order, with repeats.
    pub fn demanded(&self) -> Vec<ObjectId> {
        self.demanded.borrow().clone()
    }
}

impl AuthenticatedObjects for PairProvider<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.demanded.borrow_mut().extend_from_slice(ids);
        let mut values = Vec::with_capacity(ids.len());
        for id in ids {
            let found = self
                .result
                .object(*id)
                .or_else(|| self.base.object(*id));
            match found {
                Some(object) if ObjectId::for_bytes(object.canonical()) == *id => {
                    values.push(object.canonical().to_vec());
                }
                Some(_) => return Err(ContentError::IdentityMismatch),
                None => return Err(ContentError::MissingObject),
            }
        }
        Ok(values)
    }
}
