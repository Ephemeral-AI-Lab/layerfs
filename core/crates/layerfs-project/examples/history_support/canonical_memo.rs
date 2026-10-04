//! Bounded authenticated metadata reuse within one native proof invocation.
//! Empty at proof start; file-content reads and oracle comparisons bypass it.
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use std::{cell::RefCell, collections::BTreeMap};
pub const BYTE_LIMIT: usize = 2 * 1024 * 1024;
pub const ROW_LIMIT: usize = 512;
#[derive(Clone, Copy, Debug, Default)]
pub struct Counters {
    pub hits: u64,
    pub misses: u64,
    pub source_calls: u64,
    pub source_objects: u64,
    pub evictions: u64,
    pub peak_bytes: usize,
    pub peak_rows: usize,
}
#[derive(Default)]
pub struct Memo {
    rows: BTreeMap<ObjectId, (Vec<u8>, u64)>,
    bytes: usize,
    clock: u64,
    counters: Counters,
}
impl Memo {
    pub fn counters(&self) -> Counters {
        self.counters
    }
    pub fn retained(&self) -> (usize, usize) {
        (self.bytes, self.rows.len())
    }
    fn get(&mut self, id: ObjectId) -> Option<Vec<u8>> {
        self.clock = self.clock.saturating_add(1);
        if let Some((bytes, age)) = self.rows.get_mut(&id) {
            *age = self.clock;
            self.counters.hits += 1;
            Some(bytes.clone())
        } else {
            self.counters.misses += 1;
            None
        }
    }
    fn remember(&mut self, id: ObjectId, body: &[u8]) {
        if body.len() > layerfs_content::inode_leaf::MAXIMUM_NODE_OBJECT_BYTES {
            return;
        }
        if let Some((previous, _)) = self.rows.remove(&id) {
            self.bytes -= previous.len();
        }
        while self.bytes + body.len() > BYTE_LIMIT || self.rows.len() >= ROW_LIMIT {
            let victim = self
                .rows
                .iter()
                .min_by_key(|(_, (_, age))| *age)
                .map(|(id, _)| *id);
            if let Some(id) = victim {
                let (bytes, _) = self.rows.remove(&id).expect("selected memo entry");
                self.bytes -= bytes.len();
                self.counters.evictions += 1;
            }
        }
        self.clock = self.clock.saturating_add(1);
        self.bytes += body.len();
        self.rows.insert(id, (body.to_vec(), self.clock));
        self.counters.peak_bytes = self.counters.peak_bytes.max(self.bytes);
        self.counters.peak_rows = self.counters.peak_rows.max(self.rows.len());
    }
}
pub struct Reader<'a> {
    source: &'a dyn AuthenticatedObjects,
    memo: &'a RefCell<Memo>,
}
impl<'a> Reader<'a> {
    pub fn new(source: &'a dyn AuthenticatedObjects, memo: &'a RefCell<Memo>) -> Self {
        Self { source, memo }
    }
}
impl AuthenticatedObjects for Reader<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        if ids.len() > ROW_LIMIT {
            return Err(ContentError::ObjectLimitExceeded {
                limit: ROW_LIMIT,
                actual: ids.len(),
            });
        }
        let mut out = vec![Vec::new(); ids.len()];
        let mut wanted = Vec::new();
        let mut positions = Vec::new();
        for (position, id) in ids.iter().enumerate() {
            if let Some(body) = self.memo.borrow_mut().get(*id) {
                out[position] = body;
            } else {
                wanted.push(*id);
                positions.push(position);
            }
        }
        if !wanted.is_empty() {
            {
                let mut memo = self.memo.borrow_mut();
                memo.counters.source_calls += 1;
                memo.counters.source_objects += wanted.len() as u64;
            }
            let bodies = self.source.read_canonical_batch(&wanted)?;
            if bodies.len() != wanted.len() {
                return Err(ContentError::BatchCardinality {
                    requested: wanted.len(),
                    returned: bodies.len(),
                });
            }
            for ((id, position), body) in wanted.into_iter().zip(positions).zip(bodies) {
                // Cache only an actual authenticated source result, never oracle
                // bytes or a producer's expected result. Reject wrong identities.
                if ObjectId::for_bytes(&body) != id {
                    return Err(ContentError::IdentityMismatch);
                }
                self.memo.borrow_mut().remember(id, &body);
                out[position] = body;
            }
        }
        Ok(out)
    }
}
