//! Delta-chain hints (#758): validation, binding, group prefetch and fallback.
//!
//! A chain member says where a base might be stored. The walk binds it to the
//! base the child record names and checks it there; reconstruction then hashes
//! every object. A hint that disagrees is dropped and counted, and the read
//! goes on level by level from authoritative locates.

use super::{
    fetch::{bad_position, check_info},
    units::GroupPlan,
    Fetch,
};
use crate::{
    encoding::{PackAcquisition, PackCache, PackUnit},
    error::{StorageError, StorageResult},
    location::{ChainMember, LocatedChains, LocatedObject, ObjectLocation, PackInfo},
    pack::layout,
    policy::{DEPENDENCY_PACK_CACHE_BYTES, READ_OBJECT_LIMIT},
    port::{AcquiredPackRead, GroupRead, GroupReadAnswer, PackReadChoice, PackReadPlan},
    port::{PersistenceError, PACK_READ_PREFIX_BYTES},
};
use layerfs_content::{ObjectId, ObjectRole};
use std::collections::BTreeMap;

/// Positions one chain contributes at most: its root and every member.
const CHAIN_ENTRIES: usize = layerfs_content::MAXIMUM_DELTA_MAX_DEPTH as usize + 1;

/// Chains of located roots, the walk's position in each, and pack directories.
/// Bounded: cleared whole when a bound is reached.
#[derive(Default)]
pub(super) struct Hints {
    chains: BTreeMap<ObjectId, (u64, Vec<ChainMember>)>,
    /// A child's base is member `depth` of `root`'s chain of that generation.
    next: BTreeMap<ObjectId, (ObjectId, u64, usize)>,
    directories: BTreeMap<i64, (PackInfo, Vec<u8>)>,
    directory_bytes: usize,
    generation: u64,
}
impl Hints {
    /// `child`'s chain position: root, generation, depth and the remaining members.
    fn rest(&self, child: ObjectId) -> Option<(ObjectId, u64, usize, &[ChainMember])> {
        let &(root, generation, depth) = self.next.get(&child)?;
        let (current, members) = self.chains.get(&root)?;
        (*current == generation).then(|| (root, generation, depth, &members[depth..]))
    }
}

/// Reasons that a provider's answer is itself malformed: a defect, which
/// fails hard and is never taken for a stale hint and replayed.
const PROVIDER_DEFECTS: &[&str] = &[
    "delta chain hint",
    "port locator",
    "port chain locator",
    "port pack descriptor",
    "locator cardinality",
    "inconsistent pack descriptor",
    "group read reply",
    "metadata pack bound",
    "metadata pack cardinality/bytes",
    "metadata pack descriptor",
    "payload descriptor",
    "selected descriptor binding",
    "selected strategy/extent reply",
    "selected whole binding",
    "selected pack binding",
    "unrequested inode value",
    "metadata ordinal missing",
    "content index over slot bound",
];

/// Whether `error` may come from a stale hint: an object, pack or record that
/// is not where the hint said, or bytes that are not what it named. Never an
/// I/O outcome or a provider defect, which are not replayed.
pub(crate) fn hint_failure(error: &StorageError) -> bool {
    match error {
        StorageError::ObjectMissing(_) => true,
        StorageError::Integrity(reason) => !PROVIDER_DEFECTS.contains(reason),
        other => missing_pack(other),
    }
}

/// A provider's `Missing`: the pack a position names is not stored.
fn missing_pack(error: &StorageError) -> bool {
    let StorageError::Io(io) = error else {
        return false;
    };
    io.get_ref()
        .and_then(|inner| inner.downcast_ref::<PersistenceError>())
        .is_some_and(|inner| *inner == PersistenceError::Missing)
}

impl Fetch {
    /// Validates one chain locate's answer and keeps its chains. A malformed
    /// answer is a provider defect and fails hard.
    pub(super) fn install_chains(
        &self,
        located: &BTreeMap<ObjectId, ObjectRole>,
        answer: LocatedChains,
    ) -> StorageResult<()> {
        const BAD: StorageError = StorageError::Integrity("port chain locator");
        let mut roots = std::collections::BTreeSet::new();
        let mut members = 0;
        for chain in &answer.chains {
            let depth = located.get(&chain.root).map(|role| self.depth_for(*role));
            if !roots.insert(chain.root)
                || !depth.is_some_and(|depth| (1..=depth).contains(&chain.members.len()))
                || chain.members.iter().any(|m| {
                    check_info(m.pack).is_err()
                        || bad_position(m.group_number, m.record_number, m.canonical_length)
                        || m.canonical_length == 0
                })
            {
                return Err(BAD);
            }
            members += chain.members.len() as u64;
        }
        for directory in &answer.directories {
            check_directory(directory.pack, &directory.prefix).map_err(|_| BAD)?;
        }
        let rows = answer.chains.len() as u64;
        self.note(|c| {
            c.chain_rows += rows;
            c.chain_members += members;
        });
        if !self.trust_hints.get() {
            return Ok(());
        }
        let mut hints = self.hints.borrow_mut();
        for chain in answer.chains {
            if hints.chains.len() >= READ_OBJECT_LIMIT
                || hints.next.len() >= READ_OBJECT_LIMIT * CHAIN_ENTRIES
            {
                (hints.chains, hints.next) = Default::default();
            }
            hints.generation += 1;
            let generation = hints.generation;
            hints.chains.insert(chain.root, (generation, chain.members));
            hints.next.insert(chain.root, (chain.root, generation, 0));
        }
        for directory in answer.directories {
            if hints.directory_bytes + directory.prefix.len() > DEPENDENCY_PACK_CACHE_BYTES {
                (hints.directories, hints.directory_bytes) = Default::default();
            }
            hints.directory_bytes += directory.prefix.len();
            let entry = (directory.pack, directory.prefix);
            hints.directories.insert(entry.0.pack_id, entry);
        }
        Ok(())
    }
    fn depth_for(&self, role: ObjectRole) -> usize {
        usize::from(
            self.capacities
                .get()
                .map_or(layerfs_content::MAXIMUM_DELTA_MAX_DEPTH, |capacities| {
                    capacities.delta_depth_for_role(role)
                }),
        )
    }
    /// Whether `id`'s base has a chain member to bind.
    pub(crate) fn has_hint(&self, id: ObjectId) -> bool {
        self.hints.borrow().rest(id).is_some()
    }
    /// Binds the base `child`'s record names to its member and keeps it as a
    /// hinted locator, so the walk's own locate sends nothing. A member whose
    /// id or end disagrees with the record is stale: its chain is dropped.
    /// Pooled leaves and program pages are rebuilt by the pool reader, which
    /// has no rebuild of its own, so their bases are located as before.
    pub(super) fn bind(&self, child: &ObjectLocation, base: Option<ObjectId>) -> StorageResult<()> {
        if !self.bind_hints.get()
            || !self.trust_hints.get()
            || child.role == ObjectRole::InodeLeaf
            || crate::encoding::pool::branch::is_program_role(child.role)
        {
            return Ok(());
        }
        let mut hints = self.hints.borrow_mut();
        let Some((root, generation, depth, rest)) = hints.rest(child.object_id) else {
            return Ok(());
        };
        let (member, base) = match (rest.first().copied(), base) {
            (None, None) => return Ok(()),
            (Some(m), Some(x)) if m.object_id.is_none_or(|id| id == x) && self.known(&m) => (m, x),
            _ => {
                hints.chains.remove(&root);
                drop(hints);
                return self.stale();
            }
        };
        hints.next.insert(base, (root, generation, depth + 1));
        drop(hints);
        if self.locators.borrow().contains_key(&base) {
            return Ok(());
        }
        self.remember_pack(member.pack)?;
        let location = ObjectLocation {
            object_id: base,
            role: child.role,
            canonical_length: member.canonical_length,
            pack_id: member.pack.pack_id,
            group_number: member.group_number,
            record_number: member.record_number,
            base: None,
        };
        let row = LocatedObject {
            location,
            pack: member.pack,
        };
        self.keep_locator(row, true);
        self.absent.borrow_mut().remove(&base);
        self.note(|c| c.chain_hinted_bases += 1);
        Ok(())
    }
    /// A member whose pack descriptor does not contradict a known one.
    fn known(&self, member: &ChainMember) -> bool {
        let descriptors = self.descriptors.borrow();
        descriptors
            .get(&member.pack.pack_id)
            .is_none_or(|old| *old == member.pack)
    }
    fn stale(&self) -> StorageResult<()> {
        self.note(|c| c.chain_hint_stale += 1);
        if self.strict_hints.get() {
            return Err(StorageError::Integrity("delta chain hint"));
        }
        Ok(())
    }
    /// After a failure that a hint may have caused, on the walk of `chain`
    /// (empty: any walk): when a hinted locator was on it, evicts every hinted
    /// locator, uses no hint until the next demand and returns true, so the
    /// caller rebuilds once from authoritative locates.
    pub(crate) fn drop_hints(&self, chain: &[ObjectId]) -> StorageResult<bool> {
        let involved = {
            let cache = self.locators.borrow();
            chain.is_empty() || chain.iter().any(|id| cache.is_hinted(id))
        };
        if !involved || self.forget_hints() == 0 {
            return Ok(false);
        }
        self.stale().map(|()| true)
    }
    /// Uses no hint until the next demand; returns how many hinted locators
    /// were evicted.
    fn forget_hints(&self) -> usize {
        self.trust_hints.set(false);
        *self.hints.borrow_mut() = Hints::default();
        self.locators.borrow_mut().remove_hinted()
    }
    /// Fetches the uncached groups of `locations` and of their chains' members
    /// in one `read_groups` call of at most [`DEPENDENCY_PACK_CACHE_BYTES`],
    /// into `packs`. Nothing happens unless a member is involved, so rows with
    /// no chain keep today's acquisition. Packs left out are acquired as before.
    ///
    /// A pack only members name is a hint: a member whose descriptor
    /// contradicts a known one is left out, and a failure of a read planned
    /// for members alone makes the hints stale (counted, not used again this
    /// demand) while the read goes on as before. A failure of a pack that
    /// `locations` themselves name stays hard.
    pub(crate) fn prefetch_groups(
        &self,
        locations: &[ObjectLocation],
        packs: &mut PackCache,
    ) -> StorageResult<()> {
        let batched = self.metadata.batched_chain_reads();
        if !self.trust_hints.get() || !batched && self.hints.borrow().chains.is_empty() {
            return Ok(());
        }
        // Each pack with its groups and whether an authoritative row names it.
        let mut wanted: Vec<(PackInfo, Vec<usize>, bool)> = Vec::new();
        let mut members = false;
        {
            let hints = self.hints.borrow();
            let descriptors = self.descriptors.borrow();
            let mut add = |info: PackInfo, group: usize, own: bool| match wanted
                .iter_mut()
                .find(|(known, _, _)| known.pack_id == info.pack_id)
            {
                Some((_, groups, owned)) => {
                    *owned |= own;
                    if !groups.contains(&group) {
                        groups.push(group);
                    }
                }
                None => wanted.push((info, vec![group], own)),
            };
            for location in locations {
                if let Some(info) = descriptors.get(&location.pack_id) {
                    add(*info, location.group_number, true);
                }
                let rest = hints
                    .rest(location.object_id)
                    .map_or(&[][..], |hint| hint.3);
                members |= !rest.is_empty();
                for member in rest.iter().filter(|m| self.known(m)) {
                    add(member.pack, member.group_number, false);
                }
                if batched {
                    members |=
                        self.local_chain(location, &mut |info, group| add(info, group, true));
                }
            }
        }
        if !members {
            return Ok(());
        }
        let (mut reads, mut planned, mut bytes) = (Vec::new(), Vec::new(), 0);
        for (info, mut groups, own) in wanted {
            groups.sort_unstable();
            let published = self.serve_published.get() && self.published_has(info.pack_id);
            let mut missing = Vec::new();
            for group in groups {
                if packs.group(self, info.pack_id, group)?.is_none() {
                    missing.push(group);
                }
            }
            if missing.is_empty() || published {
                continue;
            }
            let Some((read, cost)) = self.plan_read(info, &missing) else {
                continue;
            };
            if bytes + cost <= DEPENDENCY_PACK_CACHE_BYTES {
                bytes += cost;
                reads.push(read);
                planned.push((missing, own));
            }
        }
        if reads.is_empty() {
            return Ok(());
        }
        let hinted_only = planned.iter().any(|(_, own)| !own);
        let mut answers = Vec::new();
        match self.metadata.read_groups(&reads, &mut answers) {
            Err(PersistenceError::Missing | PersistenceError::Malformed) if hinted_only => {
                self.forget_hints();
                return self.stale();
            }
            other => other?,
        }
        if answers.len() != reads.len() {
            return Err(StorageError::Integrity("group read reply"));
        }
        let ranges: usize = reads.iter().map(|read| read.ranges.len()).sum();
        self.note(|c| {
            c.group_reads += 1;
            c.group_read_packs += reads.len() as u64;
            c.group_read_ranges += ranges as u64;
        });
        let mut stale = false;
        for ((read, answer), (groups, own)) in reads.iter().zip(answers).zip(&planned) {
            let wanted = groups.iter().copied().collect();
            let admitted = self.admit(read, answer, groups).and_then(|acquisition| {
                packs.admit_groups(self, read.pack.pack_id, &wanted, groups, acquisition)
            });
            match admitted {
                Err(_) if !own => stale = true,
                other => {
                    other?;
                }
            }
        }
        if stale {
            self.forget_hints();
            return self.stale();
        }
        Ok(())
    }
    /// Adds the groups of `location`'s chain as this handle's cache knows
    /// it: published rows carry their `base`, followed through authoritative
    /// locators only (a lost row was invalidated, so its winner is located
    /// again) until a base is not cached or the record is FULL.
    fn local_chain(
        &self,
        location: &ObjectLocation,
        add: &mut impl FnMut(PackInfo, usize),
    ) -> bool {
        let cache = self.locators.borrow();
        let mut base = location.base;
        let mut found = false;
        for _ in 0..CHAIN_ENTRIES {
            let Some(row) = base.and_then(|id| cache.authoritative(&id)) else {
                break;
            };
            add(row.pack, row.location.group_number);
            found = true;
            base = row.location.base;
        }
        found
    }
    /// A Save slice's stored bases (`ids`, none of them private to the Save)
    /// and their chains in one group read, when the provider batches them.
    pub(crate) fn prefetch_bases(
        &self,
        ids: &[ObjectId],
        packs: &mut PackCache,
    ) -> StorageResult<()> {
        if !self.metadata.batched_chain_reads() {
            return Ok(());
        }
        let bases: Vec<ObjectLocation> = {
            let cache = self.locators.borrow();
            ids.iter()
                .filter_map(|id| cache.authoritative(id).map(|row| row.location))
                .collect()
        };
        self.prefetch_groups(&bases, packs)
    }
    /// One pack's request and its returned-byte cost: the planned ranges where
    /// a directory is known, else the whole pack; `None` when the request
    /// would not fit one call.
    fn plan_read(&self, info: PackInfo, groups: &[usize]) -> Option<(GroupRead, usize)> {
        let hints = self.hints.borrow();
        let whole = Some((
            GroupRead {
                pack: info,
                ranges: Vec::new(),
            },
            info.length,
        ))
        .filter(|_| info.length <= DEPENDENCY_PACK_CACHE_BYTES);
        let Some((_, prefix)) = hints.directories.get(&info.pack_id).filter(|d| d.0 == info) else {
            return whole;
        };
        let mut plan = GroupPlan::new(info.pack_id, groups, false, PACK_READ_PREFIX_BYTES);
        match plan.select(info, prefix) {
            Ok(PackReadChoice::Ranges(ranges))
                if crate::port::check_ranges(info, prefix.len(), &ranges).is_ok() =>
            {
                let cost = ranges.iter().map(|range| range.length).sum();
                Some((GroupRead { pack: info, ranges }, cost))
            }
            Ok(_) => whole,
            // A member naming a group the directory lacks is a stale position:
            // the walk meets it there and falls back.
            Err(_) => None,
        }
    }
    /// Checks one answer and turns it into the acquisition the pack cache admits.
    fn admit(
        &self,
        read: &GroupRead,
        answer: GroupReadAnswer,
        groups: &[usize],
    ) -> StorageResult<PackAcquisition> {
        let bad = StorageError::Integrity("group read reply");
        match answer {
            GroupReadAnswer::Whole(pack) => {
                let (info, body) = pack.into_parts();
                let length = body.len() as u64;
                let whole = self.whole_reply(info == read.pack, "group read reply", info, body)?;
                self.note(|c| {
                    c.group_read_whole += 1;
                    c.group_read_bytes += length;
                });
                Ok(whole)
            }
            GroupReadAnswer::Ranges(bodies) => {
                let hints = self.hints.borrow();
                let directory = hints.directories.get(&read.pack.pack_id);
                let prefix = directory.filter(|d| d.0 == read.pack).map(|d| &d.1);
                let lengths = bodies.iter().map(Vec::len);
                let (Some(prefix), false) = (prefix, read.ranges.is_empty()) else {
                    return Err(bad);
                };
                if !lengths.eq(read.ranges.iter().map(|range| range.length)) {
                    return Err(bad);
                }
                let mut plan =
                    GroupPlan::new(read.pack.pack_id, groups, false, PACK_READ_PREFIX_BYTES);
                let acquired = AcquiredPackRead::acquire(read.pack, &mut plan, |at, out| {
                    let end = at + out.len();
                    if end <= prefix.len() {
                        out.copy_from_slice(&prefix[at..end]);
                        return Ok(());
                    }
                    let (range, body) = read
                        .ranges
                        .iter()
                        .zip(&bodies)
                        .find(|(range, _)| range.offset <= at && end <= range.offset + range.length)
                        .ok_or(PersistenceError::Malformed)?;
                    out.copy_from_slice(&body[at - range.offset..end - range.offset]);
                    Ok(())
                });
                if let Some(error) = plan.error {
                    return Err(error);
                }
                let AcquiredPackRead::Units(units) = acquired? else {
                    return Err(bad);
                };
                self.remember_pack(read.pack)?;
                let returned: usize = bodies.iter().map(Vec::len).sum();
                self.note(|c| c.group_read_bytes += returned as u64);
                Ok(PackAcquisition::Units(PackUnit::from_acquired(units)?))
            }
        }
    }
}

/// A directory must be exactly `body[0..body_offset)` of a valid pack of its lane.
fn check_directory(info: PackInfo, prefix: &[u8]) -> StorageResult<()> {
    check_info(info)?;
    let header = layout::parse_directory_header(prefix, info.length)?;
    if prefix.len() != header.body_offset
        || crate::location::PackDomain::for_lane(header.lane) != info.domain
    {
        return Err(StorageError::Integrity("pack directory width"));
    }
    layout::directory_group_views(prefix, header).map(|_| ())
}
