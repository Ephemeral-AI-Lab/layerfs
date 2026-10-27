//! The merge plan and the merged root (#355).
//!
//! The plan's values, fresh serials and directory changes are point records
//! in the merge's records; the merged root is one backed update of the
//! `into` side over them, so it is built by the same canonical producer, on
//! the same route, as a Workspace Commit. A file value written at a name
//! keeps its source serial when the serial is new to that side and `from`
//! binds it at no other name, already holds that value there, or is changed
//! in place at its only binding; otherwise it gets a fresh serial, so one
//! serial never carries two values and a new file `from` also binds at other
//! names keeps its own value there.

use layerfs_telemetry::timer::{Active, TimingScope};

use super::decide::Resolver;
use super::rows::{PlanCounts, PlanRows};
use super::spill::{single, Spill, ASSIGNED, FRESH, VALUE};
use crate::compare::tree::{same_value, EntryFacts};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::update::update_filesystem_streamed_backed;
use crate::filesystem::{FilesystemObjects, FilesystemRootId};
use crate::object::inode_leaf::{
    decode_inode_value, encode_inode_value, InodeValue, INODE_VALUE_BYTES,
};
use crate::policy::{ConstructionCapacities, ConstructionPolicy};

/// Construction inputs of the merged files and root.
pub(super) struct Writer<'a> {
    pub(super) policy: ConstructionPolicy,
    pub(super) capacities: &'a ConstructionCapacities,
    pub(super) fresh: &'a mut dyn FnMut() -> ContentResult<u64>,
    pub(super) timing: &'a TimingScope<'a, Active>,
}

/// The plan's point records and totals.
pub(super) struct Plan<'s, 'r> {
    spill: &'s Spill<'r>,
    pub(super) counts: PlanCounts,
    /// Source serials with an assignment record, approximately: a serial it
    /// does not hold has none, so most lookups need no read.
    assigned: Filter,
}

/// A fixed-size Bloom filter of serials.
struct Filter(Vec<u64>);

/// Bits of [`Filter`]: a constant memory at any merge size.
const FILTER_BITS: u64 = 1 << 23;

impl Filter {
    fn new() -> Self {
        Self(vec![0; (FILTER_BITS / 64) as usize])
    }

    fn bits(serial: u64) -> [u64; 3] {
        let mut x = serial;
        [0; 3].map(|_| {
            // splitmix64
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            (z ^ (z >> 31)) % FILTER_BITS
        })
    }

    fn add(&mut self, serial: u64) {
        for bit in Self::bits(serial) {
            self.0[(bit / 64) as usize] |= 1 << (bit % 64);
        }
    }

    fn may_hold(&self, serial: u64) -> bool {
        Self::bits(serial)
            .iter()
            .all(|bit| self.0[(bit / 64) as usize] & (1 << (bit % 64)) != 0)
    }
}

impl<'s, 'r> Plan<'s, 'r> {
    pub(super) fn new(spill: &'s Spill<'r>) -> Self {
        Self {
            spill,
            counts: PlanCounts::default(),
            assigned: Filter::new(),
        }
    }
}

/// The merged root: the plan applied to the `into` side, or that side itself
/// when the plan is empty.
pub(super) fn finish(
    objects: &mut FilesystemObjects<'_>,
    resolver: &Resolver<'_>,
    into: usize,
    mut plan: Plan<'_, '_>,
    writer: &Writer<'_>,
) -> ContentResult<FilesystemRootId> {
    if plan.counts.headers == 0 && plan.counts.values == 0 {
        return Ok(resolver.roots[into]);
    }
    plan.rank()?;
    let tree = resolver.trees[into];
    let input = PlanRows::new(
        plan.spill,
        resolver.roots[into],
        tree.root.scope(),
        tree.root_serial(),
        plan.counts,
    );
    let mut records = plan.spill.backing();
    Ok(writer
        .timing
        .child("merge.update")
        .run(|_| update_filesystem_streamed_backed(objects, &input, &mut records, None))?
        .root)
}

fn assigned_entries(bytes: &[u8]) -> ContentResult<Vec<(InodeValue, u64)>> {
    bytes
        .chunks(INODE_VALUE_BYTES + 8)
        .map(|chunk| {
            if chunk.len() != INODE_VALUE_BYTES + 8 {
                return Err(ContentError::InvalidRecord("merge assignment"));
            }
            let serial = u64::from_be_bytes(chunk[..8].try_into().expect("eight bytes"));
            Ok((decode_inode_value(&chunk[8..])?, serial))
        })
        .collect()
}

impl Plan<'_, '_> {
    /// Final value of a serial.
    pub(super) fn value(&mut self, serial: u64, value: InodeValue) -> ContentResult<()> {
        let key = single(serial);
        let bytes = encode_inode_value(value).to_vec();
        match self.spill.get(VALUE, key)? {
            None => {
                self.counts.values += 1;
                self.spill.insert(VALUE, key, bytes)
            }
            Some(old) => self.spill.replace(VALUE, key, old, bytes),
        }
    }

    /// A serial new to the `into` side; its rank is written by [`Self::rank`].
    pub(super) fn fresh(&mut self, serial: u64) -> ContentResult<()> {
        if self.spill.get(FRESH, single(serial))?.is_none() {
            self.counts.fresh += 1;
            self.spill.insert(FRESH, single(serial), Vec::new())?;
        }
        Ok(())
    }

    /// Writes each fresh serial's rank in serial order.
    fn rank(&mut self) -> ContentResult<()> {
        let mut rank = 0u64;
        let mut after = None;
        loop {
            let keys = self.spill.keys_after(FRESH, after)?;
            let Some(last) = keys.last() else {
                return Ok(());
            };
            after = Some(*last);
            for key in keys {
                self.spill
                    .replace(FRESH, key, Vec::new(), rank.to_be_bytes().to_vec())?;
                rank += 1;
            }
        }
    }

    /// The serial that carries `value` for the entry `like`.
    pub(super) fn assign(
        &mut self,
        resolver: &Resolver<'_>,
        into: usize,
        writer: &mut Writer<'_>,
        like: EntryFacts,
        value: InodeValue,
        here: Option<EntryFacts>,
    ) -> ContentResult<u64> {
        let source = like.serial;
        let stored = if self.assigned.may_hold(source) {
            self.spill.get(ASSIGNED, single(source))?
        } else {
            None
        };
        let given = stored
            .as_deref()
            .map(assigned_entries)
            .transpose()?
            .unwrap_or_default();
        if let Some((_, serial)) = given.iter().find(|(held, _)| same_value(held, &value)) {
            return Ok(*serial);
        }
        let first = given.is_empty();
        let serial = match resolver.value(into, source)? {
            _ if !first => None,
            // `from` binds the file at other names too, which keep its own
            // value: the value written here is another file.
            None if like.value.namespace_ref_count > 1 => None,
            None => {
                self.fresh(source)?;
                Some(source)
            }
            Some(held) if same_value(&held, &value) => {
                self.record(source, stored, value, source)?;
                return Ok(source);
            }
            Some(held)
                if held.namespace_ref_count == 1
                    && here.is_some_and(|entry| entry.serial == source) =>
            {
                Some(source)
            }
            Some(_) => None,
        };
        let serial = match serial {
            Some(serial) => serial,
            None => {
                let fresh = (writer.fresh)()?;
                if resolver.value(into, fresh)?.is_some()
                    || self.spill.get(VALUE, single(fresh))?.is_some()
                {
                    return Err(ContentError::InvalidRecord("merge fresh serial"));
                }
                self.fresh(fresh)?;
                fresh
            }
        };
        self.value(serial, value)?;
        self.record(source, stored, value, serial)?;
        Ok(serial)
    }

    /// True when the plan holds a final value for `serial`.
    pub(super) fn valued(&self, serial: u64) -> ContentResult<bool> {
        Ok(self.spill.get(VALUE, single(serial))?.is_some())
    }

    /// Adds one assignment to a source serial's record.
    fn record(
        &mut self,
        source: u64,
        stored: Option<Vec<u8>>,
        value: InodeValue,
        serial: u64,
    ) -> ContentResult<()> {
        self.assigned.add(source);
        let mut bytes = stored.clone().unwrap_or_default();
        bytes.extend_from_slice(&serial.to_be_bytes());
        bytes.extend_from_slice(&encode_inode_value(value));
        match stored {
            None => self.spill.insert(ASSIGNED, single(source), bytes),
            Some(old) => self.spill.replace(ASSIGNED, single(source), old, bytes),
        }
    }
}
