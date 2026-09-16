//! Iterative dependency-chain reconstruction.
//!
//! A DELTA record is decoded only after its whole chain has been read, checked and
//! authenticated, root first. The walk is iterative: no recursion depth depends on
//! the stored chain length, the number of steps is bounded by the role's accepted
//! depth, and the canonical, encoded and record work of one chain are charged
//! against fixed budgets that do not grow with the cutoff or the depth. Chronology
//! is enforced by the locator order, so a cycle cannot be expressed: every base
//! must be located strictly before its dependent.

use std::collections::BTreeMap;

use rusqlite::Connection;

use layerfs_content::ObjectId;

use crate::encoding::codec::DecompressionWorkspace;
use crate::encoding::decode::decode_canonical;
use crate::encoding::full::raw_payload;
use crate::error::{StorageError, StorageResult};
use crate::pack::layout::PackLane;
use crate::policy::StorageCapacities;
use crate::sqlite::lookup::{self, ObjectLocation};

/// Work performed while reconstructing dependency chains.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChainCounters {
    /// Objects read, including requested objects.
    pub objects: u64,
    /// Chain steps taken, one per dependency edge.
    pub edges: u64,
    /// Digits of packed record bytes read.
    pub encoded_bytes: u64,
    /// Canonical bytes reconstructed.
    pub canonical_bytes: u64,
    /// Longest chain reconstructed.
    pub max_depth: u64,
}

/// One read wave's chain resolver: shared pack cache, ceiling and counters.
pub struct Resolver<'a> {
    connection: &'a Connection,
    ceiling: i64,
    capacities: &'a StorageCapacities,
    packs: &'a mut BTreeMap<i64, Vec<u8>>,
    workspace: &'a mut DecompressionWorkspace,
    counters: &'a mut ChainCounters,
}

impl<'a> Resolver<'a> {
    /// Builds a resolver over one read wave's caches.
    pub fn new(
        connection: &'a Connection,
        ceiling: i64,
        capacities: &'a StorageCapacities,
        packs: &'a mut BTreeMap<i64, Vec<u8>>,
        workspace: &'a mut DecompressionWorkspace,
        counters: &'a mut ChainCounters,
    ) -> Self {
        Self {
            connection,
            ceiling,
            capacities,
            packs,
            workspace,
            counters,
        }
    }

    /// Reconstructs and authenticates one stored canonical object.
    pub fn resolve(&mut self, id: ObjectId) -> StorageResult<Vec<u8>> {
        let root = lookup::location(self.connection, id, self.ceiling)?
            .ok_or(StorageError::ObjectMissing(id))?;
        self.resolve_at(root)
    }

    /// Reconstructs one object as a **dependency** of a larger operation.
    ///
    /// The dependency bytes are charged to the chain budget including the object
    /// itself, so a selection can refuse to build a chain that a later read could
    /// not reconstruct.
    pub fn resolve_dependency(&mut self, id: ObjectId) -> StorageResult<Vec<u8>> {
        let root = lookup::location(self.connection, id, self.ceiling)?
            .ok_or(StorageError::ObjectMissing(id))?;
        self.resolve_charged(root, true)
    }

    /// Reconstructs one object whose location the caller has already resolved.
    ///
    /// One resolution owns one chain-work allowance: the canonical, encoded and
    /// decoded budgets are charged per chain and restart here, so a wave that
    /// reads many independent objects cannot spend a sibling's allowance and a
    /// single deep chain cannot hide behind a shallow neighbour.
    pub fn resolve_at(&mut self, root: ObjectLocation) -> StorageResult<Vec<u8>> {
        self.resolve_charged(root, false)
    }

    fn resolve_charged(
        &mut self,
        root: ObjectLocation,
        charge_self: bool,
    ) -> StorageResult<Vec<u8>> {
        *self.counters = ChainCounters::default();
        let id = root.object_id;
        let role_depth = self.capacities.delta_depth_for_role(root.role);
        let mut chain: Vec<ObjectLocation> = Vec::with_capacity(usize::from(role_depth) + 1);
        let mut current = root;
        loop {
            chain.push(current);
            let Some(base) = current.base_object_id else {
                break;
            };
            if chain.len() > usize::from(role_depth) {
                return Err(StorageError::Integrity("dependency chain depth"));
            }
            let location = lookup::location(self.connection, base, self.ceiling)?
                .ok_or(StorageError::ObjectMissing(base))?;
            if location.role != current.role {
                return Err(StorageError::Integrity("dependency role"));
            }
            if locator_key(&location) >= locator_key(&current) {
                return Err(StorageError::Integrity("dependency chronology"));
            }
            current = location;
        }
        let depth = (chain.len() - 1) as u64;
        self.counters.edges = depth;
        self.counters.max_depth = depth;
        let mut canonical: Option<Vec<u8>> = None;
        for (position, location) in chain.iter().rev().enumerate() {
            // Every object except the requested one is a dependency and is charged
            // to the chain budget; the requested object is bounded by the canonical
            // object limit, not by a dependency budget.
            let charged = charge_self || position + 1 != chain.len();
            let decoded = {
                let base_bytes = match &canonical {
                    Some(bytes) => Some(raw_payload(bytes, location.role)?),
                    None => None,
                };
                self.decode_at(location, base_bytes, charged)?
            };
            if ObjectId::for_bytes(&decoded) != location.object_id {
                return Err(StorageError::Integrity("dependency identity"));
            }
            canonical = Some(decoded);
        }
        let canonical = canonical.ok_or(StorageError::ObjectMissing(id))?;
        Ok(canonical)
    }

    fn decode_at(
        &mut self,
        location: &ObjectLocation,
        base: Option<&[u8]>,
        charged: bool,
    ) -> StorageResult<Vec<u8>> {
        self.counters.objects = self.counters.objects.saturating_add(1);
        if charged {
            if location.canonical_length as u64 > self.capacities.chain_canonical_limit {
                return Err(StorageError::Integrity("dependency canonical work"));
            }
            let canonical = self
                .counters
                .canonical_bytes
                .saturating_add(location.canonical_length as u64);
            if canonical > self.capacities.chain_canonical_limit {
                return Err(StorageError::Integrity("dependency canonical work"));
            }
            self.counters.canonical_bytes = canonical;
        }
        let record_bytes = {
            let pack = pack_of(self.packs, self.connection, location.pack_id)?;
            record_width(pack, location)?
        };
        if charged {
            let encoded = self.counters.encoded_bytes.saturating_add(record_bytes);
            if encoded > self.capacities.chain_encoded_limit {
                return Err(StorageError::Integrity("dependency encoded work"));
            }
            self.counters.encoded_bytes = encoded;
        }
        let pack = pack_of(self.packs, self.connection, location.pack_id)?;
        decode_canonical(pack, location, self.capacities, base, self.workspace)
    }

    /// Records that one dependency edge was taken.
    pub fn note_edge(&mut self, depth: u64) {
        self.counters.edges = self.counters.edges.saturating_add(1);
        self.counters.max_depth = self.counters.max_depth.max(depth);
    }
}

/// Reads one pack body through the wave's shared cache.
fn pack_of<'b>(
    packs: &'b mut BTreeMap<i64, Vec<u8>>,
    connection: &Connection,
    pack_id: i64,
) -> StorageResult<&'b [u8]> {
    if let std::collections::btree_map::Entry::Vacant(slot) = packs.entry(pack_id) {
        let bytes = lookup::pack_bytes(connection, pack_id)?;
        slot.insert(bytes);
    }
    packs
        .get(&pack_id)
        .map(Vec::as_slice)
        .ok_or(StorageError::Integrity("pack cache"))
}

/// Strict ordering key of one locator.
pub fn locator_key(location: &ObjectLocation) -> (i64, usize, usize) {
    (
        location.pack_id,
        location.group_number,
        location.record_number,
    )
}

/// Stored width of the body a locator names, charged to the chain budget.
///
/// A compressed group is charged its full decoded body: the decoded bytes are
/// what reconstruction actually materializes, and the compressed extent is
/// already bounded by the pack.
pub fn record_width(pack: &[u8], location: &ObjectLocation) -> StorageResult<u64> {
    let header = crate::pack::layout::parse_header(pack)?;
    let view = crate::pack::layout::group_view(pack, header, location.group_number)?;
    if matches!(header.lane, PackLane::WholeFile) {
        return Ok(view.end.saturating_sub(view.start) as u64);
    }
    match view.codec {
        crate::pack::layout::GroupCodec::Zstandard => Ok(view.decoded_length as u64),
        crate::pack::layout::GroupCodec::Raw => {
            let body = pack
                .get(view.start..view.end)
                .ok_or(StorageError::Integrity("group body range"))?;
            let record = crate::encoding::decode::framed_record(body, location.record_number)?;
            Ok(record.len() as u64)
        }
    }
}
