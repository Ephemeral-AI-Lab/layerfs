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
use std::collections::{HashMap, HashSet};
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

/// One batch's prefix of a chain: **which** identities it may serve, not a copy.
///
/// A batch in a chain must be served the chain *as it stood before that batch*, and
/// a standalone probe measured that handing it the complete chain instead returns
/// `cycle check work limit` (`tests/namespace_batch_probe.rs`). That requirement is
/// about the object **set** a reader may serve, so the set is what is kept.
///
/// The alternative — a `TreeStore` snapshot per batch, which is what the driver held
/// until round 22 — retains the sum over batches of the chain so far, because
/// [`TreeStore::absorb`] **copies**: 25 snapshots holding 66,824 objects and
/// 164,347,158 bytes to serve a chain whose final state is 4,221 objects and
/// 12,452,785 bytes, one snapshot per batch where 24 of the 25 are strict subsets of
/// the next (`report.md` section 11.4 of round 21). The identities cost 4.5 MB where
/// the copies cost 164 MB, and serve the identical set.
#[derive(Clone, Default)]
pub struct PrefixKeys {
    allowed: HashSet<ObjectId>,
}

impl PrefixKeys {
    /// Empty: a reader over this serves nothing, which is batch 0's prefix.
    pub fn new() -> Self {
        Self::default()
    }

    /// Admits every identity in `ids`.
    pub fn extend(&mut self, ids: &[ObjectId]) {
        self.allowed.extend(ids.iter().copied());
    }

    /// Identities admitted.
    ///
    /// There is deliberately no `is_empty`: an empty prefix is batch 0's state and
    /// is not a condition any caller branches on, so naming one would add a
    /// predicate with no reader. `clippy::len_without_is_empty` asks for it anyway,
    /// which is why the allowance is here rather than a method.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.allowed.len()
    }

    /// Whether `id` may be served.
    pub fn admits(&self, id: ObjectId) -> bool {
        self.allowed.contains(&id)
    }
}

/// A reader over a complete chain, restricted to one batch's prefix.
///
/// **Why this is a reader and not a copy.** The driver that builds a namespace in
/// batches needs, for batch `index`, the objects the *previous* batches produced.
/// Building that set costs what the chain costs, and the batch loop's chain grows to
/// the whole namespace; so a snapshot per batch retains the sum over batches of the
/// chain so far. Holding the complete chain **once** and admitting only the
/// identities a batch's prefix contained serves exactly the same objects, and fails
/// identically: an identity outside the prefix is [`ContentError::MissingObject`],
/// which is the error a reader over a snapshot that lacks the object also returns.
///
/// The set-membership test can only *refuse*. Everything served still goes through
/// [`TreeStore`]'s own re-identification, so a filtered reader is not a weaker one.
pub struct PrefixProvider<'a> {
    /// The complete chain, built before the timer.
    pub chain: &'a TreeStore,
    /// Identities the batch this reader serves may see.
    pub allowed: &'a PrefixKeys,
    demanded: RefCell<Vec<ObjectId>>,
    served: std::cell::Cell<u64>,
    refused: std::cell::Cell<u64>,
    /// Live copies of every identity this reader has ever served.
    ///
    /// Kept because the coverage check counts *distinct* identities, and a filter
    /// that admitted one object before denying another is a defect a served count
    /// alone cannot see.
    served_ids: RefCell<HashSet<ObjectId>>,
}

impl<'a> PrefixProvider<'a> {
    /// Wraps a complete chain and one batch's prefix.
    pub fn new(chain: &'a TreeStore, allowed: &'a PrefixKeys) -> Self {
        Self {
            chain,
            allowed,
            demanded: RefCell::new(Vec::new()),
            served: std::cell::Cell::new(0),
            refused: std::cell::Cell::new(0),
            served_ids: RefCell::new(HashSet::new()),
        }
    }

    /// Every identity demanded, in demand order, with repeats.
    pub fn demanded(&self) -> Vec<ObjectId> {
        self.demanded.borrow().clone()
    }

    /// Objects served, and objects refused as outside the prefix.
    pub fn served(&self) -> (u64, u64) {
        (self.served.get(), self.refused.get())
    }

    /// Distinct identities served.
    pub fn distinct_served(&self) -> usize {
        self.served_ids.borrow().len()
    }
}

impl AuthenticatedObjects for PrefixProvider<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.demanded.borrow_mut().extend_from_slice(ids);
        let mut values = Vec::with_capacity(ids.len());
        for id in ids {
            // The prefix decides *whether* this reader may see the object at all;
            // holding the complete chain is what makes the answer cheap. A refusal
            // is the same error a snapshot lacking the object returns, because that
            // is what the batch is owed: the chain as it stood before it.
            let found = if self.allowed.admits(*id) {
                self.chain.object(*id)
            } else {
                self.refused.set(self.refused.get() + 1);
                None
            };
            match found {
                Some(object) if ObjectId::for_bytes(object.canonical()) == *id => {
                    self.served.set(self.served.get() + 1);
                    self.served_ids.borrow_mut().insert(*id);
                    values.push(object.canonical().to_vec());
                }
                // A stored object that does not re-identify is a hard failure, not
                // a miss: the provider contract distinguishes them, and collapsing
                // them would hide corruption behind an absence.
                Some(_) => return Err(ContentError::IdentityMismatch),
                None => {
                    eprintln!(
                        "PROBE missing {id} admitted={} chain={} allowed={} served={} refused={} demanded={}",
                        self.allowed.admits(*id),
                        self.chain.len(),
                        self.allowed.len(),
                        self.served.get(),
                        self.refused.get(),
                        self.demanded.borrow().len()
                    );
                    return Err(ContentError::MissingObject);
                }
            }
        }
        Ok(values)
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
    nanos: u64,
}

impl<'a> CountingConsumer<'a> {
    /// Wraps a consumer.
    pub fn new(inner: &'a mut dyn FinalizedConsumer) -> Self {
        Self {
            inner,
            accepted: 0,
            nanos: 0,
        }
    }

    /// Objects forwarded so far.
    pub fn accepted(&self) -> u64 {
        self.accepted
    }

    /// Wall time this consumer's own `accept` calls took, in nanoseconds.
    ///
    /// The consumer path and the construction that feeds it are one span at the
    /// call site (`pipeline.span_build_ns`), and a span cannot say how much of
    /// itself is the product's `accept` and how much is the caller's tree build.
    /// Charging the calls here separates them without touching the product: this
    /// is harness wall time around the product call, published beside the span.
    pub fn nanos(&self) -> u64 {
        self.nanos
    }
}

impl FinalizedConsumer for CountingConsumer<'_> {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let started = std::time::Instant::now();
        let result = self.inner.accept(object);
        self.nanos += started.elapsed().as_nanos() as u64;
        result?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_content::ObjectRole;

    /// One canonical bytes-role object over `value`.
    ///
    /// Built by hand rather than through `codec::encode_bytes_object`, which is
    /// `pub(crate)` to the product: `FinalizedObject::new` decodes what it is given,
    /// so a test that fed it arbitrary bytes would fail on framing instead of on the
    /// behaviour under test. The header is `magic + kind + payload_len + value_len`,
    /// big-endian, and `payload_len` counts the length field with the value.
    fn object_of(value: &[u8]) -> FinalizedObject {
        let mut canonical = Vec::new();
        canonical.extend_from_slice(b"LFSO");
        canonical.push(1);
        canonical.extend_from_slice(&((value.len() + 4) as u32).to_be_bytes());
        canonical.extend_from_slice(&(value.len() as u32).to_be_bytes());
        canonical.extend_from_slice(value);
        FinalizedObject::new(ObjectRole::Chunk, canonical).expect("canonical object")
    }

    /// A store of `count` distinct objects, deterministic in `tag`.
    fn store_of(count: u8, tag: u8) -> TreeStore {
        let mut store = TreeStore::new();
        for index in 0..count {
            store.insert_object(object_of(&[tag, index, 0x5a, 0xa5]));
        }
        store
    }

    /// The identity a snapshot of `ids` would serve, built the way the driver used
    /// to build one: a `TreeStore` holding copies.
    fn snapshot_of(chain: &TreeStore, ids: &[ObjectId]) -> TreeStore {
        let mut snapshot = TreeStore::new();
        for id in ids {
            snapshot.insert_object(chain.object(*id).expect("held").clone());
        }
        snapshot
    }

    #[test]
    fn prefix_keys_admit_the_snapshot_and_nothing_else() {
        let chain = store_of(6, 1);
        let all: Vec<ObjectId> = chain.insertion_order().to_vec();
        // A prefix of the first four, as a batch's base would be.
        let mut keys = PrefixKeys::new();
        keys.extend(&all[..4]);
        let reader = PrefixProvider::new(&chain, &keys);

        for id in &all[..4] {
            let served = reader.read_canonical_batch(&[*id]).expect("served");
            assert_eq!(served[0], chain.canonical(*id).expect("held").to_vec());
        }
        // The two the prefix does not admit are refused, and refused the same way a
        // snapshot that never held them refuses: an absence, not a corruption.
        for id in &all[4..] {
            assert!(matches!(
                reader.read_canonical_batch(&[*id]),
                Err(ContentError::MissingObject)
            ));
        }
        assert_eq!(reader.served(), (4, 2));
        assert_eq!(reader.distinct_served(), 4);
    }

    #[test]
    fn a_batch_demanding_a_later_object_gets_the_snapshot_error() {
        // The defect this guards: the provider holds the complete chain, so it
        // *could* answer a demand the batch is not owed. Both readers must refuse.
        let chain = store_of(6, 2);
        let all: Vec<ObjectId> = chain.insertion_order().to_vec();
        let mut keys = PrefixKeys::new();
        keys.extend(&all[..3]);
        let snapshot = snapshot_of(&chain, &all[..3]);
        let filtered = PrefixProvider::new(&chain, &keys);
        let demanded = [all[0], all[5]];

        let from_snapshot = snapshot.read_canonical_batch(&demanded);
        let from_filtered = filtered.read_canonical_batch(&demanded);
        assert!(matches!(
            from_snapshot,
            Err(ContentError::MissingObject)
        ));
        assert!(matches!(
            from_filtered,
            Err(ContentError::MissingObject)
        ));
    }

    #[test]
    fn a_cumulative_prefix_serves_bytes_and_demand_order_identically() {
        // What the driver relies on: the key set for batch `index` is the snapshot's
        // object set, so the same demands return the same bytes in the same order.
        let chain = store_of(8, 3);
        let all: Vec<ObjectId> = chain.insertion_order().to_vec();
        let snapshot = snapshot_of(&chain, &all[..5]);
        let mut keys = PrefixKeys::new();
        keys.extend(&all[..5]);
        let filtered = PrefixProvider::new(&chain, &keys);

        let wave = [all[4], all[1], all[3], all[1]];
        let expected = snapshot.read_canonical_batch(&wave).expect("snapshot serves");
        let observed = filtered.read_canonical_batch(&wave).expect("filtered serves");
        assert_eq!(expected, observed);
        assert_eq!(snapshot.demanded(), filtered.demanded());
        assert_eq!(filtered.served(), (4, 0));
        assert_eq!(filtered.distinct_served(), 3);
    }
}
