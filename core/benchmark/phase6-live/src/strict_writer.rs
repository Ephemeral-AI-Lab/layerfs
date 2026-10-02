//! One serialized construction producer; strict physical placement and C2 selection.
use crate::strict_client::Catalog;
use crate::{
    minio::{digest, Minio},
    strict_catalog::{LogicalUse, PhysicalBase, PlacementDomain, Reference, Registration},
    strict_private::{Member, PendingRecord, PrivateAccess, PrivateWindow, Tail},
    strict_read::{placement, Access},
};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole,
};
use layerfs_storage::{
    access::{ObjectLocation, ValueGroupRow},
    cas::{PoolCounters, SaveProfile},
    encoding::{
        delta::{
            candidates::{signature, Candidates},
            read::{BodyCaches, ChainCounters, Resolver},
            select::{select, DeltaCounters, DepthCache, SelectInput},
        },
        pool::{select_pooled, value_group, PoolIndex, PoolReader, PooledSelectInput},
        CompressionWorkspace, DecompressionWorkspace, EncodedRecord, GroupCache,
    },
    pack::{
        assemble::{assemble_consuming, build_group, framed_group_length},
        layout::{assembled_length, EncodedGroup, PackLane},
    },
    PackAccess, StorageCapacities,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::{Arc, Mutex},
};
#[path = "strict_writer_pool.rs"]
mod pooling;

fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Default, Clone, Debug)]
pub struct Counts {
    pub objects: u64,
    pub exact_reuses: u64,
    pub payload_packs: u64,
    pub metadata_packs: u64,
    pub payload_bytes: u64,
    pub metadata_bytes: u64,
    pub private_group_seals: u64,
    pub peak_private_objects: usize,
    pub peak_private_canonical: usize,
}
pub struct Writer {
    pub catalog: Arc<dyn Catalog>,
    pub provider: Minio,
    pub save: i64,
    pub capacities: StorageCapacities,
    pub counts: Counts,
    pub delta: DeltaCounters,
    pub pool_counts: PoolCounters,
    window: PrivateWindow,
    encoder: CompressionWorkspace,
    decoder: DecompressionWorkspace,
    candidates: Candidates,
    candidate_domain: PlacementDomain,
    depths: DepthCache,
    packs: BTreeMap<i64, Vec<u8>>,
    pool: PoolReader,
    pool_index: PoolIndex,
    chain: ChainCounters,
    chain_total: ChainCounters,
    profile: SaveProfile,
    arbitration: Mutex<()>,
    ordinal_next: u64,
    ordinal_end: u64,
    ordinal_reservations: usize,
    failed: bool,
    ready: bool,
    definite_provider_failure: bool,
}
impl Writer {
    pub fn new(
        catalog: Arc<dyn Catalog>,
        provider: Minio,
        capacities: StorageCapacities,
    ) -> Result<Self, String> {
        let save = catalog.begin_save()?;
        Self::from_save(catalog, provider, capacities, save)
    }
    pub fn new_owned(
        catalog: Arc<dyn Catalog>,
        provider: Minio,
        capacities: StorageCapacities,
        context: &crate::strict_catalog::SaveContext,
    ) -> Result<Self, String> {
        let save = catalog.begin_save_owned(context)?;
        Self::from_save(catalog, provider, capacities, save)
    }
    fn from_save(
        catalog: Arc<dyn Catalog>,
        provider: Minio,
        capacities: StorageCapacities,
        save: i64,
    ) -> Result<Self, String> {
        let resources = (|| {
            Ok::<_, String>((
                CompressionWorkspace::new().map_err(error)?,
                DecompressionWorkspace::new().map_err(error)?,
                Candidates::new().map_err(error)?,
            ))
        })();
        let (encoder, decoder, candidates) = match resources {
            Ok(resources) => resources,
            Err(cause) => {
                return match catalog.abandon(save) {
                    Ok(()) => Err(cause),
                    Err(abort) => Err(format!(
                        "{cause}; allocation abort acknowledgement unavailable: {abort}"
                    )),
                };
            }
        };
        Ok(Self {
            catalog,
            provider,
            save,
            capacities,
            counts: Default::default(),
            delta: Default::default(),
            pool_counts: Default::default(),
            window: Default::default(),
            encoder,
            decoder,
            candidates,
            candidate_domain: PlacementDomain::FilePayload,
            depths: DepthCache::new(),
            packs: Default::default(),
            pool: PoolReader::new(),
            pool_index: PoolIndex::new(),
            chain: Default::default(),
            chain_total: Default::default(),
            profile: Default::default(),
            arbitration: Mutex::new(()),
            ordinal_next: 0,
            ordinal_end: 0,
            ordinal_reservations: 0,
            failed: false,
            ready: false,
            definite_provider_failure: false,
        })
    }
    fn invalidate_snapshots(&mut self) {
        self.packs.clear();
        self.pool.release_packs();
    }
    pub fn read(
        &mut self,
        logical_use: LogicalUse,
        ids: &[ObjectId],
    ) -> Result<Vec<Vec<u8>>, String> {
        if self.failed {
            return Err("failed writer; no private reads".into());
        }
        if ids.len() > 4096 {
            return Err("construction read object window".into());
        }
        let scope = self.catalog.capture(Some(self.save))?;
        let access = PrivateAccess {
            persisted: Access {
                catalog: self.catalog.as_ref(),
                provider: &self.provider,
                scope,
                logical_use,
            },
            window: &self.window,
            save: self.save,
        };
        let mut total = 0;
        for id in ids {
            let pending = self.window.pending.values().flatten().find(|r| {
                r.member.id == *id
                    && (r.member.logical_use == logical_use
                        || r.aliases.iter().any(|m| m.logical_use == logical_use))
            });
            let length = if let Some(record) = pending {
                record.member.length
            } else {
                access
                    .location(*id, scope.ceiling)
                    .map_err(error)?
                    .ok_or("required domain placement missing")?
                    .canonical_length
            };
            total += length;
            if total > crate::read_window::BYTES {
                return Err("construction read byte window".into());
            }
        }
        let mut groups = GroupCache::new();
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(record) = self.window.pending.values().flatten().find(|r| {
                r.member.id == *id
                    && (r.member.logical_use == logical_use
                        || r.aliases.iter().any(|m| m.logical_use == logical_use))
            }) {
                out.push(record.canonical.clone());
                continue;
            }
            let mut chain = ChainCounters::default();
            let mut resolver = Resolver::new(
                &access,
                scope.ceiling,
                &self.capacities,
                BodyCaches {
                    packs: &mut self.packs,
                    pool: &mut self.pool,
                },
                &mut groups,
                &mut self.decoder,
                &mut chain,
            );
            out.push(resolver.resolve(*id).map_err(error)?.0);
            layerfs_storage::encoding::delta::read::accumulate(&mut self.chain_total, chain);
        }
        Ok(out)
    }
    pub fn offer(
        &mut self,
        logical_use: LogicalUse,
        object: FinalizedObject,
    ) -> Result<(), String> {
        if self.failed || self.ready {
            return Err("writer terminal; no resend".into());
        }
        let result = self.offer_inner(logical_use, object);
        if let Err(cause) = &result {
            self.failed = true;
            // Any provider or SQL failure may have acquired custody. Retain every
            // representation and invalidate derivations; never guess deletion.
            self.candidates.invalidate();
            self.pool_index.invalidate();
            if let Err(custody) = self.stop_failed_save(cause) {
                return Err(format!("{cause}; {custody}"));
            }
        }
        result
    }
    fn offer_inner(
        &mut self,
        logical_use: LogicalUse,
        object: FinalizedObject,
    ) -> Result<(), String> {
        if object.canonical_len() > crate::pending::BYTES {
            return Err("single construction object window".into());
        }
        let domain = placement(logical_use, object.role());
        if self.window.members() >= crate::pending::OBJECTS
            || self
                .window
                .canonical()
                .saturating_add(object.canonical_len())
                > crate::pending::BYTES
        {
            self.flush_all()?;
        }
        let advisory: Vec<_> = object.predecessors().ids().collect();
        if object.role() == ObjectRole::WholeFile {
            self.activate_candidates(domain)?;
        }
        let sig = if object.role() == ObjectRole::WholeFile {
            Some(signature(
                layerfs_storage::encoding::raw_payload(object.canonical(), object.role())
                    .map_err(error)?,
            ))
        } else {
            None
        };
        let references = self.reference_facts(logical_use, &object)?;
        // Raw pending equality does not promote an unplaced record into a C2 base.
        if let Some(record) = self
            .window
            .pending
            .iter_mut()
            .filter(|((d, _), _)| *d == domain)
            .flat_map(|(_, records)| records)
            .find(|r| r.member.id == object.id())
        {
            if record.member.role != object.role() || record.canonical != object.canonical() {
                return Err("pending domain exact CAS mismatch".into());
            }
            if let Some(member) = std::iter::once(&record.member)
                .chain(record.aliases.iter())
                .find(|m| m.logical_use == logical_use)
            {
                if member.references != references {
                    return Err("pending immutable logical-use reference mismatch".into());
                }
            } else {
                let mut member = record.member.clone();
                member.logical_use = logical_use;
                member.references = references;
                record.aliases.push(member);
            }
            self.counts.exact_reuses += 1;
            return Ok(());
        }
        // Preserve pending_base_for: placing a local group does not PUT its pack.
        let proposed = sig.and_then(|s| self.candidates.find(object.id(), &s));
        if object.role() == ObjectRole::WholeFile
            && advisory
                .iter()
                .chain(proposed.iter())
                .any(|id| self.window.pending_id(domain, *id))
        {
            self.seal_pending()?;
        }
        // Referenced payloads pay for their ACK before SQL metadata staging.
        if domain == PlacementDomain::Metadata {
            self.flush_domain(PlacementDomain::FilePayload)?;
        }
        let existing = if let Some((tail, member)) = self.window.member(domain, object.id()) {
            Some((
                ObjectLocation {
                    object_id: member.id,
                    role: member.role,
                    canonical_length: member.length,
                    pack_id: tail.order,
                    group_number: member.group,
                    record_number: member.record,
                },
                member.logical_use,
            ))
        } else {
            self.catalog
                .location(self.catalog.capture(Some(self.save))?, domain, object.id())?
                .map(|l| (l.location, logical_use))
        };
        if let Some((location, existing_use)) = existing {
            if let Some((tail, _)) = self.window.member(domain, object.id()) {
                let key = (domain, tail.lane.index());
                let tail = self
                    .window
                    .tails
                    .get_mut(&key)
                    .ok_or("private tail missing")?;
                if !tail
                    .members
                    .iter()
                    .any(|m| m.id == object.id() && m.logical_use == logical_use)
                {
                    let mut member = tail
                        .members
                        .iter()
                        .find(|m| m.id == object.id())
                        .ok_or("private member missing")?
                        .clone();
                    member.logical_use = logical_use;
                    member.references = references;
                    tail.members.push(member);
                }
            } else {
                self.catalog.register_use(
                    self.catalog.capture(Some(self.save))?,
                    self.save,
                    domain,
                    object.id(),
                    logical_use,
                    &references,
                )?;
            }
            if location.role != object.role()
                || location.canonical_length != object.canonical_len()
                || self.read(existing_use, &[object.id()])?.remove(0) != object.canonical()
            {
                return Err("domain exact CAS mismatch".into());
            }
            self.counts.exact_reuses += 1;
            return Ok(());
        }
        // Equality across domains never substitutes for required placement.
        if let Some((role, length)) = self.catalog.descriptor(object.id())? {
            if role != object.role() || length != object.canonical_len() {
                return Err("global canonical descriptor mismatch".into());
            }
            let (other_domain, other_use) = match domain {
                PlacementDomain::FilePayload => {
                    (PlacementDomain::Metadata, LogicalUse::MetadataGraph)
                }
                PlacementDomain::Metadata => {
                    (PlacementDomain::FilePayload, LogicalUse::RegularFileGraph)
                }
            };
            if self
                .catalog
                .location(
                    self.catalog.capture(Some(self.save))?,
                    other_domain,
                    object.id(),
                )?
                .is_some()
                && self.read(other_use, &[object.id()])?.remove(0) != object.canonical()
            {
                return Err("dual-domain authenticated exact bytes mismatch".into());
            }
        }
        let record = if object.role() == ObjectRole::InodeLeaf {
            self.pooled_record(&object, &advisory)?
        } else {
            // One admitted-FULL index is activated by domain, never duplicated.
            let scope = self.catalog.capture(Some(self.save))?;
            let access = PrivateAccess {
                persisted: Access {
                    catalog: self.catalog.as_ref(),
                    provider: &self.provider,
                    scope,
                    logical_use,
                },
                window: &self.window,
                save: self.save,
            };
            select(
                &mut SelectInput {
                    connection: &access,
                    arbitration: &self.arbitration,
                    wave_held: false,
                    capacities: &self.capacities,
                    candidates: &mut self.candidates,
                    depths: &mut self.depths,
                    packs: &mut self.packs,
                    pool: &mut self.pool,
                    decode: &mut self.decoder,
                    chain: &mut self.chain,
                    chain_total: &mut self.chain_total,
                    signature: sig,
                    counters: &mut self.delta,
                    profile: &mut self.profile,
                },
                object.id(),
                object.canonical(),
                object.role(),
                &advisory,
                &mut self.encoder,
            )
            .map_err(error)?
        };
        let key = (domain, record.lane.index());
        let pending = self.window.pending.get(&key);
        let bytes = pending.map_or(0, |r| r.iter().map(|r| r.encoded.record.len()).sum());
        let canonical = pending.map_or(0, |r| r.iter().map(|r| r.member.length).sum());
        let count = pending.map_or(0, Vec::len);
        if count > 0
            && (record.lane == PackLane::Singleton
                || framed_group_length(count + 1, bytes + record.record.len()).map_err(error)?
                    > layerfs_storage::policy::GROUP_TARGET
                || canonical.saturating_add(object.canonical_len())
                    > layerfs_storage::policy::GROUP_CANONICAL_BYTES_LIMIT as usize)
        {
            self.seal_key(key)?;
        }
        let member = Member {
            id: object.id(),
            role: object.role(),
            length: object.canonical_len(),
            logical_use,
            references,
            base: record.base,
            group: 0,
            record: 0,
        };
        self.window
            .pending
            .entry(key)
            .or_default()
            .push(PendingRecord {
                member,
                aliases: Vec::new(),
                encoded: record,
                canonical: object.canonical().to_vec(),
            });
        self.counts.objects += 1;
        self.counts.peak_private_objects =
            self.counts.peak_private_objects.max(self.window.members());
        self.counts.peak_private_canonical = self
            .counts
            .peak_private_canonical
            .max(self.window.canonical());
        if self.window.pending[&key][0].encoded.lane == PackLane::Singleton {
            self.seal_key(key)?;
            self.close_tail(key)?;
        }
        Ok(())
    }
    fn reference_facts(
        &self,
        logical_use: LogicalUse,
        object: &FinalizedObject,
    ) -> Result<Vec<Reference>, String> {
        let mut typed: BTreeMap<ObjectId, BTreeSet<LogicalUse>> = BTreeMap::new();
        if object.role() == ObjectRole::InodeLeaf {
            let leaf = layerfs_content::inode_leaf::InodeLeaf::decode(object.canonical())
                .map_err(error)?;
            for row in leaf.rows {
                let value =
                    layerfs_content::inode_leaf::decode_inode_value(&row.value).map_err(error)?;
                typed
                    .entry(value.metadata_root)
                    .or_default()
                    .insert(LogicalUse::MetadataGraph);
                typed.entry(value.content_root).or_default().insert(
                    if value.kind == layerfs_content::inode_leaf::InodeKind::RegularFile {
                        LogicalUse::RegularFileGraph
                    } else {
                        LogicalUse::MetadataGraph
                    },
                );
            }
        }
        let mut references = BTreeSet::new();
        for id in object.references() {
            let role = self
                .window
                .tails
                .values()
                .flat_map(|t| &t.members)
                .find(|m| m.id == *id)
                .map(|m| m.role)
                .or_else(|| {
                    self.window
                        .pending
                        .values()
                        .flatten()
                        .find(|r| r.member.id == *id)
                        .map(|r| r.member.role)
                })
                .or(self.catalog.descriptor(*id)?.map(|d| d.0))
                .ok_or("reference producer missing")?;
            if let Some(uses) = typed.get(id) {
                for usage in uses {
                    references.insert(Reference {
                        id: *id,
                        domain: placement(*usage, role),
                        logical_use: *usage,
                    });
                }
            } else {
                references.insert(Reference {
                    id: *id,
                    domain: placement(logical_use, role),
                    logical_use,
                });
            }
        }
        Ok(references.into_iter().collect())
    }
    fn seal_pending(&mut self) -> Result<(), String> {
        let keys: Vec<_> = self.window.pending.keys().copied().collect();
        for key in keys {
            self.seal_key(key)?;
        }
        Ok(())
    }
    fn seal_key(&mut self, key: (PlacementDomain, usize)) -> Result<(), String> {
        let Some(records) = self.window.pending.remove(&key) else {
            return Ok(());
        };
        if records.is_empty() {
            return Ok(());
        }
        let lane = records[0].encoded.lane;
        let mut members = Vec::with_capacity(records.len());
        let mut bytes = Vec::with_capacity(records.len());
        for (number, mut record) in records.into_iter().enumerate() {
            record.member.record = number;
            members.push(record.member);
            for mut alias in record.aliases {
                alias.record = number;
                members.push(alias);
            }
            bytes.push(record.encoded.record);
        }
        let group = build_group(lane, &bytes, Some(&mut self.encoder)).map_err(error)?;
        self.append_group(key, lane, group, members)?;
        self.counts.private_group_seals += 1;
        Ok(())
    }
    fn append_group(
        &mut self,
        key: (PlacementDomain, usize),
        lane: PackLane,
        group: EncodedGroup,
        mut members: Vec<Member>,
    ) -> Result<(), String> {
        if let Some(tail) = self.window.tails.get(&key) {
            if !tail.fits(&group)? {
                self.close_tail(key)?;
            }
        }
        let tail = self.window.tails.entry(key).or_insert_with(|| Tail {
            domain: key.0,
            lane,
            order: 0,
            digest: [0; 32],
            generation: 0,
            groups: Vec::new(),
            members: Vec::new(),
            used: 0,
        });
        let number = tail.groups.len();
        for member in &mut members {
            member.group = number;
        }
        tail.groups.push(group);
        tail.members.extend(members);
        tail.used = assembled_length(lane, &tail.groups).map_err(error)?;
        let snapshot = tail.bytes()?;
        if snapshot.len() > crate::strict_catalog::BODY_LIMIT {
            return Err("adapter physical body window".into());
        }
        let new_digest = digest(&snapshot);
        let order = self.catalog.reserve_body(key.0, self.save, new_digest)?;
        if tail.order != 0 {
            for row in self
                .window
                .groups
                .values_mut()
                .filter(|r| r.pack_id == tail.order)
            {
                row.pack_id = order;
            }
            self.catalog.discard_reserved_body(self.save, tail.order)?;
        }
        tail.order = order;
        tail.digest = new_digest;
        self.window.generation = self
            .window
            .generation
            .checked_add(1)
            .ok_or("private generation overflow")?;
        tail.generation = self.window.generation;
        self.invalidate_snapshots();
        Ok(())
    }
    fn close_tail(&mut self, key: (PlacementDomain, usize)) -> Result<(), String> {
        let Some(tail) = self.window.tails.remove(&key) else {
            return Ok(());
        };
        let bytes = assemble_consuming(tail.lane, tail.groups).map_err(error)?;
        if digest(&bytes) != tail.digest {
            return Err("sealed body generation mismatch".into());
        }
        if tail.domain == PlacementDomain::FilePayload {
            if let Err(failure) = self.provider.put_classified(&tail.digest, &bytes) {
                self.definite_provider_failure = !failure.is_unknown();
                return Err(failure.to_string());
            }
            self.catalog
                .acknowledge_body(self.save, tail.order, tail.digest, None)?;
            self.counts.payload_packs += 1;
            self.counts.payload_bytes += bytes.len() as u64;
        } else {
            self.catalog
                .acknowledge_body(self.save, tail.order, tail.digest, Some(&bytes))?;
            self.counts.metadata_packs += 1;
            self.counts.metadata_bytes += bytes.len() as u64;
        }
        if tail.lane == PackLane::PooledMetadata {
            let ordinals: Vec<_> = self
                .window
                .groups
                .iter()
                .filter(|(_, row)| row.pack_id == tail.order)
                .map(|(ordinal, _)| *ordinal)
                .collect();
            let groups: Vec<_> = ordinals
                .iter()
                .map(|ordinal| self.window.groups[ordinal])
                .collect();
            self.catalog.insert_groups(
                self.catalog.capture(Some(self.save))?,
                self.save,
                &groups,
            )?;
            for ordinal in ordinals {
                self.window.groups.remove(&ordinal);
            }
        }
        let mut rows = Vec::new();
        for member in tail.members {
            let base = if let Some(id) = member.base {
                let order = if rows
                    .iter()
                    .any(|r: &Registration| r.location.object_id == id)
                {
                    tail.order
                } else {
                    self.catalog
                        .location(self.catalog.capture(Some(self.save))?, tail.domain, id)?
                        .ok_or("selected base custody missing")?
                        .location
                        .pack_id
                };
                Some(PhysicalBase {
                    id,
                    domain: tail.domain,
                    body: order,
                })
            } else {
                None
            };
            rows.push(Registration {
                location: ObjectLocation {
                    object_id: member.id,
                    role: member.role,
                    canonical_length: member.length,
                    pack_id: tail.order,
                    group_number: member.group,
                    record_number: member.record,
                },
                domain: tail.domain,
                logical_use: member.logical_use,
                references: member.references,
                base,
            });
        }
        let mut start = 0;
        while start < rows.len() {
            let mut end = start;
            let (mut canonical, mut edges) = (0u64, 0usize);
            while end < rows.len() && end - start < crate::strict_catalog::PAGE {
                let row = &rows[end];
                let next_bytes = canonical + row.location.canonical_length as u64;
                let next_edges = edges + row.references.len();
                if next_bytes > layerfs_storage::policy::TRANSACTION_CANONICAL_BYTES_LIMIT
                    || next_edges > 8191
                {
                    break;
                }
                canonical = next_bytes;
                edges = next_edges;
                end += 1;
            }
            if end == start {
                return Err("single registration exceeds SQL page".into());
            }
            self.catalog.register(
                self.catalog.capture(Some(self.save))?,
                self.save,
                &rows[start..end],
            )?;
            start = end;
        }
        self.invalidate_snapshots();
        Ok(())
    }
    fn flush_domain(&mut self, domain: PlacementDomain) -> Result<(), String> {
        let keys: Vec<_> = self
            .window
            .pending
            .keys()
            .filter(|(d, _)| *d == domain)
            .copied()
            .collect();
        for key in keys {
            self.seal_key(key)?;
        }
        let mut keys: Vec<_> = self
            .window
            .tails
            .keys()
            .filter(|(d, _)| *d == domain)
            .copied()
            .collect();
        // Attribute chunks precede the ordinary mappings which reference them.
        keys.sort_by_key(|key| {
            if key.1 == PackLane::PooledMetadata.index() {
                0
            } else if key.1 == PackLane::Native.index() {
                1
            } else {
                key.1 + 2
            }
        });
        for key in keys {
            self.close_tail(key)?;
        }
        Ok(())
    }
    fn flush_all(&mut self) -> Result<(), String> {
        self.flush_domain(PlacementDomain::FilePayload)?;
        self.flush_domain(PlacementDomain::Metadata)?;
        self.persist_candidates()
    }
    fn persist_candidates(&mut self) -> Result<(), String> {
        let access = crate::strict_candidates::Access {
            catalog: self.catalog.as_ref(),
            scope: self.catalog.capture(Some(self.save))?,
            save: self.save,
            domain: self.candidate_domain,
        };
        self.candidates.flush_catalog(&access).map_err(error)?;
        Ok(())
    }
    fn activate_candidates(&mut self, domain: PlacementDomain) -> Result<(), String> {
        if self.candidate_domain != domain {
            self.flush_domain(self.candidate_domain)?;
            self.persist_candidates()?;
            self.candidate_domain = domain;
            self.candidates.invalidate();
        }
        if self.candidates.needs_load() {
            let access = crate::strict_candidates::Access {
                catalog: self.catalog.as_ref(),
                scope: self.catalog.capture(Some(self.save))?,
                save: self.save,
                domain,
            };
            self.candidates.reload_catalog(&access).map_err(error)?;
        }
        Ok(())
    }
    pub fn finish_storage(&mut self) -> Result<(), String> {
        if self.failed || self.ready {
            return Err("writer terminal; no finish resend".into());
        }
        let result = self
            .flush_all()
            .and_then(|_| {
                self.catalog
                    .release_ordinals(self.save, self.ordinal_next, self.ordinal_end)
            })
            .and_then(|_| self.catalog.finish_storage(self.save));
        match result {
            Ok(()) => {
                self.ready = true;
                Ok(())
            }
            Err(e) => {
                self.failed = true;
                if let Err(custody) = self.stop_failed_save(&e) {
                    return Err(format!("{e}; {custody}"));
                }
                Err(e)
            }
        }
    }
    pub(crate) fn stop_preparation(&mut self, cause: &str) -> Result<(), String> {
        // A consumer failure has already performed its single custody action.
        if self.failed {
            return Ok(());
        }
        if self.ready {
            return Err("ready save cannot be preparation-aborted".into());
        }
        self.failed = true;
        self.stop_failed_save(cause)
    }
    fn stop_failed_save(&self, cause: &str) -> Result<(), String> {
        if self.definite_provider_failure
            || crate::strict_catalog::is_definite_catalog_error(cause)
            || cause.starts_with("definite strict refusal: ")
        {
            self.catalog
                .abandon(self.save)
                .map_err(|e| format!("definite failure; abort acknowledgment uncertain: {e}"))
        } else {
            self.catalog.quarantine(self.save).map_err(|e| {
                format!("unknown storage custody; quarantine acknowledgment unavailable: {e}")
            })
        }
    }
}

#[derive(Clone)]
pub struct Consumer {
    pub owner: Rc<RefCell<Writer>>,
    pub logical_use: LogicalUse,
}
impl FinalizedConsumer for Consumer {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.owner
            .borrow_mut()
            .offer(self.logical_use, object)
            .map_err(|e| {
                eprintln!("strict consumer: {e}");
                ContentError::Io
            })
    }
}
#[derive(Clone)]
pub struct ConstructionReader {
    pub owner: Rc<RefCell<Writer>>,
    pub logical_use: LogicalUse,
}
impl AuthenticatedObjects for ConstructionReader {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.owner
            .borrow_mut()
            .read(self.logical_use, ids)
            .map_err(|e| {
                eprintln!("strict construction reader: {e}");
                ContentError::Io
            })
    }
}
