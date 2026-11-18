//! Hash maps keyed by object identity, for the lookups of one mark.
//!
//! A walk asks several maps about every object it reaches. An ordered map
//! pays a 32-byte comparison at each level for every question, and the
//! standard hasher runs its keyed permutation over all 32 bytes; either costs
//! more than the rest of the walk's bookkeeping. An identity is a BLAKE3
//! digest, so its first eight bytes are already uniform: [`IdState`] takes
//! those and mixes them with a key drawn per map.
//!
//! The key matters. Identities come from stored content, so a writer can
//! choose content whose identities share bits. Without a secret key such a
//! set would fall into one chain of the table and turn every lookup into a
//! scan. Results never depend on the key: nothing here is iterated in map
//! order without sorting first.
use layerfs_content::ObjectId;
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, Hasher, RandomState};

/// A map from object identity.
pub(super) type IdMap<V> = HashMap<ObjectId, V, IdState>;
/// A set of object identities.
pub(super) type IdSet = HashSet<ObjectId, IdState>;

/// One map's secret: a mask and an odd multiplier.
#[derive(Clone, Copy)]
pub(super) struct IdState {
    mask: u64,
    multiplier: u64,
}

impl Default for IdState {
    fn default() -> Self {
        // The standard source of per-process random hashing keys.
        let random = RandomState::new();
        Self {
            mask: random.hash_one(0u8),
            multiplier: random.hash_one(1u8) | 1,
        }
    }
}

impl BuildHasher for IdState {
    type Hasher = IdHasher;

    fn build_hasher(&self) -> IdHasher {
        IdHasher {
            state: *self,
            word: 0,
        }
    }
}

/// Folds an identity's leading bytes; see the module comment.
pub(super) struct IdHasher {
    state: IdState,
    word: u64,
}

impl Hasher for IdHasher {
    fn write(&mut self, bytes: &[u8]) {
        // An identity arrives as one 32-byte write, and its first eight bytes
        // are enough. A shorter write is padded with zeros, so the hasher
        // stays correct, if weaker, for any other key.
        let mut word = [0u8; 8];
        for (index, byte) in bytes.iter().take(8).enumerate() {
            word[index] = *byte;
        }
        self.word = self.word.rotate_left(29) ^ u64::from_le_bytes(word);
    }

    fn write_usize(&mut self, _length: usize) {
        // The length prefix of a fixed-width identity carries nothing.
    }

    fn finish(&self) -> u64 {
        // The multiplication spreads every input bit upward; the fold brings
        // the high half back down, where the table takes its slot index.
        let mixed = (self.word ^ self.state.mask).wrapping_mul(self.state.multiplier);
        mixed ^ (mixed >> 32)
    }
}
