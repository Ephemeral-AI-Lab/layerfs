//! Canonical bytes a Save already resolved, kept within a byte bound (#364).
//!
//! An object's identity is the hash of its canonical bytes, so a kept value is
//! exactly what resolving the same identity again would return: it was
//! authenticated when first resolved. Keeping it skips that object's delta
//! chain, decompression and hash on every later read in the same Save. The
//! least recently read values are released first; a released value is
//! resolved again when asked, so the bound costs work and never correctness.
//!
//! The same bounded store keeps the canonical bytes of the whole-file objects
//! a Save stored FULL, for a trial against one of them before it is published
//! (`save/select.rs`, #810); that copy has its own bound.
use layerfs_content::ObjectId;
use std::collections::{BTreeMap, HashMap};

/// What one kept value is charged besides its bytes.
const ENTRY_CHARGE: usize = 96;

pub(super) struct Resolved {
    values: HashMap<ObjectId, (Vec<u8>, u64)>,
    order: BTreeMap<u64, ObjectId>,
    clock: u64,
    charged: usize,
    bound: usize,
}

impl Resolved {
    /// Empty, keeping at most `bound` bytes and charges.
    pub(super) fn new(bound: usize) -> Self {
        Self {
            values: HashMap::new(),
            order: BTreeMap::new(),
            clock: 0,
            charged: 0,
            bound,
        }
    }

    /// A copy of the kept value of `id`, marked as the most recent.
    pub(super) fn get(&mut self, id: ObjectId) -> Option<Vec<u8>> {
        let (value, touched) = self.values.get_mut(&id)?;
        self.order.remove(touched);
        self.clock += 1;
        *touched = self.clock;
        self.order.insert(self.clock, id);
        Some(value.clone())
    }

    /// Keeps `value` as `id`'s canonical bytes; a value larger than the whole
    /// bound is not kept.
    pub(super) fn keep(&mut self, id: ObjectId, value: &[u8]) {
        let charge = value.len() + ENTRY_CHARGE;
        if charge > self.bound || self.values.contains_key(&id) {
            return;
        }
        while self.charged + charge > self.bound {
            let Some((_, oldest)) = self.order.pop_first() else {
                break;
            };
            if let Some((released, _)) = self.values.remove(&oldest) {
                self.charged -= released.len() + ENTRY_CHARGE;
            }
        }
        self.clock += 1;
        self.order.insert(self.clock, id);
        self.values.insert(id, (value.to_vec(), self.clock));
        self.charged += charge;
    }
}
