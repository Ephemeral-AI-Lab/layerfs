//! Authenticated chain elements one long-lived reader has already rebuilt.
//!
//! A pooled leaf or a program page stored as a delta is rebuilt from its
//! whole chain, base first. A reader that walks many versions of the same
//! page, as a mark does over every retained Commit, meets the same bases
//! again and again: a chain of depth `d` costs `d + 1` record reads, delta
//! applications and identity hashes for its root, and then `d` more for its
//! direct base, and so on. This memo keeps what a reconstruction already
//! rebuilt and authenticated, so a later chain stops at the first element
//! held here instead of walking to its end.
//!
//! What is held is exactly what the next element's delta is applied to: the
//! canonical page of a program role, or the physical body of a pooled leaf.
//! An entry is admitted only after the reader checked the element's canonical
//! bytes against the identity its locator states, and an identity names one
//! byte string, so an entry answers for that identity for as long as it is
//! held. With the bytes it keeps the chain facts a full walk would have
//! measured below the element (depth, canonical and record bytes, highest
//! pack), so a reconstruction that stops here enforces the same depth cap,
//! work allowances and visibility ceiling as one that walks on.
//!
//! Owner: the reader, and through it the reader's operation. Bound: the byte
//! limit the owner chose, in two generations; when the young one reaches half
//! the limit the old one is dropped. Release: with the reader.
use layerfs_content::{ObjectId, ObjectRole};
use std::collections::HashMap;

/// One rebuilt chain element and the chain below it.
#[derive(Clone, Debug)]
pub(super) struct Known {
    /// The role every element of the chain has.
    pub(super) role: ObjectRole,
    /// A program page's canonical bytes, or a pooled leaf's physical body.
    pub(super) bytes: Vec<u8>,
    /// Dependency edges below this element.
    pub(super) depth: u8,
    /// Canonical bytes of the chain, this element included.
    pub(super) canonical: u64,
    /// Record bytes of the chain, this element included.
    pub(super) encoded: u64,
    /// The highest pack any element of the chain is stored in.
    pub(super) highest_pack: i64,
}

/// A byte-bounded memo of [`Known`] elements by identity.
#[derive(Debug)]
pub(super) struct ChainMemo {
    young: HashMap<ObjectId, Known>,
    old: HashMap<ObjectId, Known>,
    young_bytes: usize,
    limit: usize,
}

impl ChainMemo {
    /// A memo that holds about `limit` bytes of rebuilt elements at most.
    pub(super) fn new(limit: usize) -> Self {
        Self {
            young: HashMap::new(),
            old: HashMap::new(),
            young_bytes: 0,
            limit,
        }
    }

    /// The held element `id` of `role`, when its whole chain is visible
    /// under `ceiling`.
    pub(super) fn get(&mut self, id: &ObjectId, role: ObjectRole, ceiling: i64) -> Option<Known> {
        let known = match self.young.get(id) {
            Some(known) => known,
            None => {
                // Used again: it moves to the young generation and outlives
                // the next drop of the old one.
                let known = self.old.remove(id)?;
                self.admit(*id, known);
                self.young.get(id)?
            }
        };
        (known.role == role && known.highest_pack <= ceiling).then(|| known.clone())
    }

    /// Keeps one element whose canonical identity the reader has checked.
    pub(super) fn insert(&mut self, id: ObjectId, known: Known) {
        if known.bytes.len() > self.limit / 2 || self.young.contains_key(&id) {
            return;
        }
        self.old.remove(&id);
        self.admit(id, known);
    }

    fn admit(&mut self, id: ObjectId, known: Known) {
        if self.young_bytes + known.bytes.len() > self.limit / 2 {
            self.old = std::mem::take(&mut self.young);
            self.young_bytes = 0;
        }
        self.young_bytes += known.bytes.len();
        self.young.insert(id, known);
    }
}
