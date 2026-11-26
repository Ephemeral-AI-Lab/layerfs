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
    encoding::{delta::read::cached_base, PackAcquisition, PackCache, PackUnit},
    error::{StorageError, StorageResult},
    location::{ChainMember, LocatedChains, LocatedObject, ObjectLocation, PackInfo},
    pack::layout,
    policy::{DEPENDENCY_PACK_CACHE_BYTES, READ_OBJECT_LIMIT},
    port::PersistenceError,
    port::{AcquiredPackRead, GroupRead, GroupReadAnswer, PackReadChoice, PackReadPlan},
};
use layerfs_content::{ObjectId, ObjectRole};
use std::collections::{BTreeMap, BTreeSet};

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
    /// Packs whose descriptor only a bound member gave: forgotten with the
    /// hints and at the next demand, unless confirmed first.
    packs: BTreeSet<i64>,
}
impl Hints {
    /// Keeps one checked pack directory within the bound.
    fn directory(&mut self, pack: PackInfo, prefix: Vec<u8>) {
        if self.directory_bytes + prefix.len() > DEPENDENCY_PACK_CACHE_BYTES {
            (self.directories, self.directory_bytes) = Default::default();
        }
        self.directory_bytes += prefix.len();
        self.directories.insert(pack.pack_id, (pack, prefix));
    }
    /// `child`'s chain position: root, generation, depth and the remaining members.
    fn rest(&self, child: ObjectId) -> Option<(ObjectId, u64, usize, &[ChainMember])> {
        let &(root, generation, depth) = self.next.get(&child)?;
        let (current, members) = self.chains.get(&root)?;
        (*current == generation).then(|| (root, generation, depth, &members[depth..]))
    }
}

/// One pack a page plans: its uncached wanted groups, sorted, whether an
/// authoritative row names it, and its request with its returned-byte cost.
#[derive(Clone)]
struct Planned {
    info: PackInfo,
    groups: Vec<usize>,
    own: bool,
    read: Option<(GroupRead, usize)>,
}
impl Planned {
    fn cost(&self) -> usize {
        self.read.as_ref().map_or(0, |read| read.1)
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
            hints.directory(directory.pack, directory.prefix);
        }
        Ok(())
    }
    /// Keeps the directories of the packs this handle published, so a group
    /// read plans their ranges (#758), when the provider batches chains.
    pub(super) fn published_directories(&self, packs: &[crate::port::PublishedPack]) {
        if !self.metadata.batched_chain_reads() {
            return;
        }
        let mut hints = self.hints.borrow_mut();
        for pack in packs {
            let body = pack.body();
            if let Ok(header) = layout::parse_header(body) {
                hints.directory(pack.info(), body[..header.body_offset].to_vec());
            }
        }
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
    /// has no rebuild of its own, so a read locates their bases as before;
    /// only a Save's check of its program-page bases binds theirs
    /// ([`Self::binding_pages`]), and it walks them again from authoritative
    /// locates when the check fails.
    pub(super) fn bind(&self, child: &ObjectLocation, base: Option<ObjectId>) -> StorageResult<()> {
        if !self.binds(child.role) {
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
        self.remember_member_pack(member.pack);
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
    /// Whether [`Self::bind`] binds a member under a child of `role` now.
    fn binds(&self, role: ObjectRole) -> bool {
        self.bind_hints.get()
            && self.trust_hints.get()
            && role != ObjectRole::InodeLeaf
            && (!crate::encoding::pool::branch::is_program_role(role) || self.bind_pages.get())
    }
    /// A walk that binds no member under `child` (a pooled leaf; a program
    /// page outside a Save's check of its bases; any walk of a Save's own
    /// demand) is about to locate `base`, which `child`'s record names (#768):
    /// the deeper base ids its chain lets cached bytes name
    /// ([`Self::chain_ahead`]) join that locate, so the walk's later levels
    /// find their bases located. Nothing happens when `base` is already
    /// cached, when the walk binds, or when there is nothing to guess: the
    /// walk then locates as before. The rows are authoritative and every
    /// level still takes its base id from the record at its base's location.
    pub(crate) fn locate_ahead(
        &self,
        child: &ObjectLocation,
        base: ObjectId,
        packs: &PackCache,
        workspace: &mut crate::encoding::codec::DecompressionWorkspace,
    ) -> StorageResult<()> {
        if self.binds(child.role) {
            return Ok(());
        }
        let groups = crate::encoding::GroupCache::new();
        let ahead = self.chain_ahead(&[base], &[(*child, base)], packs, &groups, workspace);
        if ahead.is_empty() {
            return Ok(());
        }
        self.absent.borrow_mut().remove(&base);
        self.locate(&[&[base][..], &ahead].concat())
    }
    /// A member whose pack descriptor does not contradict a known one.
    fn known(&self, member: &ChainMember) -> bool {
        let descriptors = self.descriptors.borrow();
        descriptors
            .get(&member.pack.pack_id)
            .is_none_or(|old| *old == member.pack)
    }
    /// Keeps the descriptor of a bound member's pack for the walk that
    /// bound it. One no other source gave is a hint: forgotten with the hints
    /// and at the next demand, unless an authoritative descriptor confirms it
    /// first ([`Self::remember_pack`]).
    fn remember_member_pack(&self, info: PackInfo) {
        let mut cache = self.descriptors.borrow_mut();
        // `known` refused a member that contradicts a kept descriptor.
        if cache.contains_key(&info.pack_id) {
            return;
        }
        if cache.len() >= READ_OBJECT_LIMIT {
            cache.clear();
            self.forget_member_marks();
        }
        cache.insert(info.pack_id, info);
        self.hints.borrow_mut().packs.insert(info.pack_id);
    }
    /// Whether `pack`'s kept descriptor came from a member alone; it no
    /// longer counts as one.
    pub(super) fn confirm_member_pack(&self, pack: i64) -> bool {
        self.hints.borrow_mut().packs.remove(&pack)
    }
    /// Forgets which descriptors came from members (their descriptors were
    /// cleared with the rest).
    pub(super) fn forget_member_marks(&self) {
        self.hints.borrow_mut().packs.clear();
    }
    /// Forgets every descriptor that only a bound member gave.
    pub(super) fn forget_member_packs(&self) {
        let packs = std::mem::take(&mut self.hints.borrow_mut().packs);
        let mut cache = self.descriptors.borrow_mut();
        for pack in packs {
            cache.remove(&pack);
        }
    }
    pub(super) fn stale(&self) -> StorageResult<()> {
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
    /// A failure a stale hint may have caused, at an element a hint located
    /// or reached through one, in a walk that has no rebuild of its own (a
    /// Save's check of a program-page base, `save/wave.rs`): uses no hint
    /// until the next demand and counts the stale hint (strict refuses),
    /// unless `counted`: the walk already counted that stale hint (a member
    /// that disagreed with its record, or a descriptor that contradicted a
    /// member's), which is counted once.
    pub(crate) fn hint_stale(&self, counted: bool) -> StorageResult<()> {
        self.forget_hints();
        match counted {
            true => Ok(()),
            false => self.stale(),
        }
    }
    /// Uses no hint until the next demand; returns how many hinted locators
    /// were evicted. The descriptors only their members gave go with them.
    pub(super) fn forget_hints(&self) -> usize {
        self.trust_hints.set(false);
        self.forget_member_packs();
        *self.hints.borrow_mut() = Hints::default();
        self.locators.borrow_mut().remove_hinted()
    }
    /// Keeps every hinted locator, and the descriptor of its pack, as
    /// authoritative. Called only after a reconstruction hashed every element
    /// of the chain that bound them (a Save's check of its bases,
    /// `save/wave.rs`).
    pub(crate) fn promote_hints(&self) {
        self.forget_member_marks();
        let promoted = self.locators.borrow_mut().promote_hinted() as u64;
        self.note(|c| c.chain_verified_bases += promoted);
    }
    /// The ids further down the chains of `edges` (each a walked child and
    /// the base its record names) that a locate of this level can carry
    /// (#758): member `k` of a child's chain says where its level-`k` base
    /// may be stored, so the base id the record there names, read from bytes
    /// the caches already hold ([`cached_base`]), is a guess for level
    /// `k + 1`. Nothing is trusted: the guesses are only located with the
    /// level's own ids, so their rows are authoritative, and the walk still
    /// takes every base id from the record at its base's authoritative
    /// location; a wrong guess only leaves the next level to locate its base
    /// as before. Empty unless this level locates anyway (`level` holds an
    /// id the cache lacks), and never more than fills that locate's page, so
    /// that locate is the only statement the guesses join. Only the chains of
    /// edges whose base this level has to locate are followed: a base already
    /// cached was bound from a member or located earlier, and a chain a read
    /// binds binds its deeper bases with no locate (fix 3b). So this serves
    /// the chains a read does not bind (program pages, pooled leaves) and a
    /// Save's reads, which bind none.
    ///
    /// Presence checks set no locator-cache referenced bit and the pack and
    /// decoded-group caches are only peeked. Effects that remain: a wrong
    /// guess's row is cached (or, when absent, remembered as absent), adding
    /// locator-cache pressure, and a guessed row's authoritative descriptor
    /// that contradicts a marked member descriptor makes the hints stale
    /// ([`Fetch::remember_pack`]). A group a guess needs decoded is
    /// decompressed into a decoded-group cache of this call's own (least
    /// recently used eviction, bounded by
    /// [`crate::policy::DECODED_GROUP_CACHE_BYTES`], dropped on return) and
    /// reused while it is held there: a call whose groups fit that bound
    /// decodes each once, but a body over the bound is never kept and a call
    /// that interleaves more distinct groups than fit decodes an evicted one
    /// again. Every decompression is counted in `prefetch_group_decodes`.
    pub(crate) fn chain_ahead(
        &self,
        level: &[ObjectId],
        edges: &[(ObjectLocation, ObjectId)],
        packs: &PackCache,
        groups: &crate::encoding::GroupCache,
        workspace: &mut crate::encoding::codec::DecompressionWorkspace,
    ) -> Vec<ObjectId> {
        if !self.trust_hints.get() {
            return Vec::new();
        }
        let cached =
            |id: &ObjectId| self.locators.borrow().holds(id) || self.absent.borrow().contains(id);
        let missing: BTreeSet<_> = level.iter().filter(|id| !cached(id)).collect();
        if missing.is_empty() {
            return Vec::new();
        }
        let room = READ_OBJECT_LIMIT.saturating_sub(missing.len());
        let hints = self.hints.borrow();
        let mut ahead = BTreeSet::new();
        let mut decoded = crate::encoding::GroupCache::new();
        let mut decodes = 0_u64;
        'edges: for (child, base) in edges {
            if !missing.contains(base) {
                continue;
            }
            let Some((_, _, _, rest)) = hints.rest(child.object_id) else {
                continue;
            };
            let mut id = *base;
            for member in rest.iter().take_while(|m| self.known(m)) {
                if ahead.len() >= room {
                    break 'edges;
                }
                let location = ObjectLocation {
                    object_id: id,
                    role: child.role,
                    canonical_length: member.canonical_length,
                    pack_id: member.pack.pack_id,
                    group_number: member.group_number,
                    record_number: member.record_number,
                    base: None,
                };
                let next = cached_base(
                    packs,
                    groups,
                    &mut decoded,
                    workspace,
                    &mut decodes,
                    &location,
                );
                let Some(next) = next else {
                    break;
                };
                if !cached(&next) && !missing.contains(&next) {
                    ahead.insert(next);
                }
                id = next;
            }
        }
        if decodes > 0 {
            self.note(|c| c.prefetch_group_decodes += decodes);
        }
        ahead.into_iter().collect()
    }
    /// Fetches the uncached groups `locations` and their chains need in one
    /// `read_groups` call, as the first page of [`Self::page`] does. Nothing
    /// happens unless a member or a local chain is involved, so rows with no
    /// chain keep today's acquisition.
    pub(crate) fn prefetch_groups(
        &self,
        locations: &[ObjectLocation],
        packs: &mut PackCache,
    ) -> StorageResult<()> {
        let compact_inode_packs = self.compact_inode_packs.get()
            && locations
                .iter()
                .all(|row| matches!(row.role, ObjectRole::InodeLeaf | ObjectRole::InodeBranch));
        self.fetch_page([locations], packs, true, compact_inode_packs)
            .map(drop)
    }
    /// Pages a demand (#758): fetches the uncached groups of the longest
    /// leading run of `consumers` (each the rows one consumer reads next) and
    /// of their chains in one `read_groups` call of at most
    /// [`DEPENDENCY_PACK_CACHE_BYTES`] of returned bytes, into `packs`, and
    /// returns that run's length. The caller consumes the run before it asks
    /// for the next page, so the pack cache still holds every group of it. A
    /// first consumer that does not fit alone has the packs that fit read and
    /// counts as covered. Returns 0, reading nothing, when the provider has no
    /// chains to batch: the caller acquires as before.
    pub(crate) fn page<'a>(
        &self,
        consumers: impl IntoIterator<Item = &'a [ObjectLocation]>,
        packs: &mut PackCache,
    ) -> StorageResult<usize> {
        self.fetch_page(consumers, packs, false, false)
    }
    /// [`Self::page`]; with `chained`, nothing is read unless a member or a
    /// local chain is involved.
    ///
    /// A pack only members name is a hint: a member whose descriptor
    /// contradicts a known one is left out, and a failure of a read planned
    /// for members alone makes the hints stale (counted, not used again this
    /// demand) while the read goes on as before. A failure of a pack that
    /// the rows themselves name stays hard.
    fn fetch_page<'a>(
        &self,
        consumers: impl IntoIterator<Item = &'a [ObjectLocation]>,
        packs: &mut PackCache,
        chained: bool,
        compact_inode_packs: bool,
    ) -> StorageResult<usize> {
        let batched = self.metadata.batched_chain_reads();
        if !self.trust_hints.get() || !batched && self.hints.borrow().chains.is_empty() {
            return Ok(0);
        }
        let (mut page, mut bytes, mut covered, mut members) = (Vec::<Planned>::new(), 0, 0, false);
        // Cached entries the page keeps resident, counted in its bytes.
        let mut kept = BTreeSet::new();
        for rows in consumers {
            let mut needs = Vec::new();
            members |= self.needs(rows, batched, &mut needs);
            // A retained unit can serve an authoritative group while another
            // group needs the whole pack. Its read still has pack authority.
            let authoritative: BTreeSet<_> = if compact_inode_packs {
                needs
                    .iter()
                    .filter_map(|(info, _, own)| own.then_some(info.pack_id))
                    .collect()
            } else {
                BTreeSet::new()
            };
            // This consumer's packs, each with the page's groups and its own,
            // and the cached entries it adds to the page.
            let mut trial: Vec<Planned> = Vec::new();
            let mut keep = BTreeMap::new();
            for (info, group, own) in needs {
                // A held body is served to every read; a published one
                // only while serving published bodies (`published_has`).
                if self.published_has(info.pack_id) {
                    continue;
                }
                if packs.group(self, info.pack_id, group)?.is_some() {
                    keep.extend(
                        packs
                            .held(info.pack_id, group)
                            .filter(|e| !kept.contains(&e.0)),
                    );
                    continue;
                }
                let at = match trial.iter().position(|p| p.info.pack_id == info.pack_id) {
                    Some(at) => at,
                    None => {
                        let known = page.iter().find(|p| p.info.pack_id == info.pack_id);
                        trial.push(known.cloned().unwrap_or(Planned {
                            info,
                            groups: Vec::new(),
                            own: false,
                            read: None,
                        }));
                        trial.len() - 1
                    }
                };
                trial[at].own |= own || authoritative.contains(&info.pack_id);
                if let Err(slot) = trial[at].groups.binary_search(&group) {
                    trial[at].groups.insert(slot, group);
                }
            }
            let cost = |page: &[Planned], id: i64| {
                page.iter()
                    .find(|p| p.info.pack_id == id)
                    .map_or(0, Planned::cost)
            };
            let mut next = bytes + keep.values().sum::<usize>();
            for planned in &mut trial {
                next -= cost(&page, planned.info.pack_id);
                planned.read = self.plan_read(planned.info, &planned.groups, compact_inode_packs);
                next += planned.cost();
            }
            if next > DEPENDENCY_PACK_CACHE_BYTES && covered > 0 {
                break;
            }
            bytes += keep.values().sum::<usize>();
            kept.extend(keep.into_keys());
            for planned in trial {
                // A first consumer over the bound keeps the packs that fit.
                let after = bytes - cost(&page, planned.info.pack_id) + planned.cost();
                if after > DEPENDENCY_PACK_CACHE_BYTES {
                    continue;
                }
                bytes = after;
                match page
                    .iter()
                    .position(|p| p.info.pack_id == planned.info.pack_id)
                {
                    Some(at) => page[at] = planned,
                    None => page.push(planned),
                }
            }
            covered += 1;
            if next > DEPENDENCY_PACK_CACHE_BYTES {
                break;
            }
        }
        if chained && !members {
            return Ok(covered);
        }
        let (reads, planned): (Vec<GroupRead>, Vec<(Vec<usize>, bool)>) = page
            .into_iter()
            .filter_map(|p| p.read.map(|(read, _)| (read, (p.groups, p.own))))
            .unzip();
        if reads.is_empty() {
            return Ok(covered);
        }
        let mut answers = Vec::new();
        if compact_inode_packs {
            self.metadata.read_groups_classified(&reads, &mut answers)?;
        } else {
            let hinted_only = planned.iter().any(|(_, own)| !own);
            let mut ordinary = Vec::new();
            match self.metadata.read_groups(&reads, &mut ordinary) {
                Err(PersistenceError::Missing | PersistenceError::Malformed) if hinted_only => {
                    self.forget_hints();
                    return self.stale().map(|()| covered);
                }
                other => other?,
            }
            answers.extend(ordinary.into_iter().map(Ok));
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
            let answer = match answer {
                Err(PersistenceError::Missing | PersistenceError::Malformed) if !own => {
                    stale = true;
                    continue;
                }
                answer => answer?,
            };
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
            return self.stale().map(|()| covered);
        }
        Ok(covered)
    }
    /// The groups `rows` need, each with its pack and whether an
    /// authoritative row names it: their own, their chains' members and,
    /// when the provider batches them, their local chains. True when a member
    /// or a local chain is involved.
    fn needs(
        &self,
        rows: &[ObjectLocation],
        batched: bool,
        out: &mut Vec<(PackInfo, usize, bool)>,
    ) -> bool {
        let hints = self.hints.borrow();
        let mut members = false;
        for location in rows {
            let info = self.descriptors.borrow().get(&location.pack_id).copied();
            out.extend(info.map(|info| (info, location.group_number, true)));
            let rest = hints
                .rest(location.object_id)
                .map_or(&[][..], |hint| hint.3);
            members |= !rest.is_empty();
            let known = rest.iter().filter(|m| self.known(m));
            out.extend(known.map(|m| (m.pack, m.group_number, false)));
            if batched {
                members |= self.local_chain(location, &mut |info, group| {
                    out.push((info, group, true));
                });
            }
        }
        members
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
    /// The authoritative locations this handle holds for `ids`, in order.
    pub(crate) fn located(&self, ids: impl Iterator<Item = ObjectId>) -> Vec<ObjectLocation> {
        let cache = self.locators.borrow();
        ids.filter_map(|id| cache.authoritative(&id).map(|row| row.location))
            .collect()
    }
    /// One pack's request and its returned-byte cost: the planned ranges where
    /// a directory is known, else the whole pack; `None` when the request
    /// would not fit one call.
    fn plan_read(
        &self,
        info: PackInfo,
        groups: &[usize],
        compact_inode_packs: bool,
    ) -> Option<(GroupRead, usize)> {
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
        let mut plan = GroupPlan::ranged(info.pack_id, groups);
        match plan.select(info, prefix) {
            Ok(PackReadChoice::Ranges(ranges))
                if crate::port::check_ranges(info, prefix.len(), &ranges).is_ok() =>
            {
                if compact_inode_packs
                    && info.domain == crate::location::PackDomain::Metadata
                    && info.length <= crate::policy::PACK_LIMIT / 2
                {
                    return whole;
                }
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
                let mut plan = GroupPlan::ranged(read.pack.pack_id, groups);
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
                drop(hints);
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
