//! Prepared whole-pack admission with epoch-validated final collision checks.
use super::diagnostic;
use super::{
    pack, read, AuthenticatedCanonicalObject, ObjectInsertMetrics, ADMISSION_BATCH_BYTES,
    OBJECT_PAGE_COUNT,
};
use crate::schema::StoreDb;
use crate::{Result, StoreError};
use layerfs_content::ObjectId;
use rusqlite::{limits::Limit, params_from_iter, types::Value, Connection};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::time::Instant;

#[cfg(test)]
thread_local! {
    static INPUT_ASSOCIATION_TRACE: std::cell::RefCell<Option<Vec<(usize, bool, usize, usize)>>> = const { std::cell::RefCell::new(None) };
}

struct PreparedObject {
    id: ObjectId,
    length: usize,
    pack: usize,
    group: usize,
    record: usize,
    canonical: Range<usize>,
    // Compressed records retain the original authenticated comparison operand.
    retained: Option<Vec<u8>>,
    // Index into this admission's bounded signature vector; other roles own none.
    small_signature: Option<std::num::NonZeroU16>,
    delta: bool,
    diagnostic_terminal: u8,
}

#[allow(dead_code)]
struct UndiagnosedPreparedObject {
    id: ObjectId,
    length: usize,
    pack: usize,
    group: usize,
    record: usize,
    canonical: Range<usize>,
    retained: Option<Vec<u8>>,
    small_signature: Option<std::num::NonZeroU16>,
    delta: bool,
}
const _: () = {
    assert!(
        std::mem::size_of::<PreparedObject>() == std::mem::size_of::<UndiagnosedPreparedObject>()
    );
    assert!(
        std::mem::align_of::<PreparedObject>() == std::mem::align_of::<UndiagnosedPreparedObject>()
    );
};

/// One prepared pack. A lane's final pack keeps its group vector instead of the
/// assembled bytes so the same session can append it to a still-open pack row
/// without holding two copies of the same payload.
#[derive(Clone)]
enum PackBody {
    Assembled(Vec<u8>),
    Retained(Vec<pack::EncodedGroup>),
}

#[derive(Clone)]
struct PreparedPack {
    version: u32,
    body: PackBody,
}

impl PreparedPack {
    fn assembled(bytes: Vec<u8>) -> Self {
        let version = u32::from_le_bytes(bytes[8..12].try_into().expect("pack header version"));
        Self {
            version,
            body: PackBody::Assembled(bytes),
        }
    }

    /// Keep the group vector of a lane's final pack. It is assembled exactly once
    /// when the pack is published, or handed to the session's open pack.
    fn retained(version: u32, groups: Vec<pack::EncodedGroup>) -> Self {
        Self {
            version,
            body: PackBody::Retained(groups),
        }
    }

    fn version(&self) -> u32 {
        self.version
    }

    fn group_count(&self) -> usize {
        match &self.body {
            PackBody::Assembled(bytes) => {
                u32::from_le_bytes(bytes[12..16].try_into().expect("pack header groups")) as usize
            }
            PackBody::Retained(groups) => groups.len(),
        }
    }

    fn len(&self) -> usize {
        match &self.body {
            PackBody::Assembled(bytes) => bytes.len(),
            PackBody::Retained(groups) => {
                pack::assembled_length(self.version, groups).unwrap_or(usize::MAX)
            }
        }
    }

    /// Retained and assembled ownership charged against the frozen physical
    /// budget. The two forms are mutually exclusive, so a lane tail is charged
    /// once whether it is held as groups or as assembled bytes.
    fn charged_capacity(&self) -> usize {
        match &self.body {
            PackBody::Assembled(bytes) => bytes.capacity(),
            PackBody::Retained(groups) => groups
                .iter()
                .map(|group| group.bytes.capacity() + std::mem::size_of::<pack::EncodedGroup>())
                .sum(),
        }
    }

    /// Exact prepared pack bytes. A retained lane tail is assembled here through
    /// the same `assemble_version` dispatch publication uses, so framing
    /// assertions still observe the bytes that will be stored.
    #[cfg(test)]
    fn prepared_bytes(&self) -> Vec<u8> {
        match &self.body {
            PackBody::Assembled(bytes) => bytes.clone(),
            PackBody::Retained(groups) => pack::assemble_version(self.version, groups).unwrap(),
        }
    }

    /// Assemble a retained tail into its exact bytes for publication and return
    /// the group vector so the open-pack path can keep it.
    fn materialize(&mut self) -> Result<Option<Vec<pack::EncodedGroup>>> {
        match &mut self.body {
            PackBody::Assembled(_) => Ok(None),
            PackBody::Retained(groups) => {
                let bytes = pack::assemble_version(self.version, groups)?;
                let groups = std::mem::take(groups);
                self.body = PackBody::Assembled(bytes);
                Ok(Some(groups))
            }
        }
    }
}

impl std::ops::Deref for PreparedPack {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match &self.body {
            PackBody::Assembled(bytes) => bytes,
            PackBody::Retained(_) => &[],
        }
    }
}

pub(crate) struct PreparedAdmission {
    session: std::sync::Arc<super::AdmissionSession>,
    final_batch: bool,
    absence_epoch: Option<u64>,
    packs: Vec<PreparedPack>,
    objects: Vec<PreparedObject>,
    small_signatures: Vec<[u64; 8]>,
    pool_groups: Vec<PreparedValueGroup>,
    pending_values: BTreeMap<[u8; 73], u32>,
    metrics: ObjectInsertMetrics,
    native_base_max_pack: i64,
    canonical_live_capacity: usize,
    oversized_backing: usize,
}

impl PreparedAdmission {
    pub(crate) fn prepare_missing(db: &StoreDb, missing: super::MissingBatch) -> Result<Self> {
        let session = missing.1.clone();
        session.resolve(Self::prepare_missing_inner(db, missing))
    }

    fn prepare_missing_inner(db: &StoreDb, missing: super::MissingBatch) -> Result<Self> {
        if !db.same_instance(&missing.1.db) {
            return Err(StoreError::Integrity("admission Store ownership"));
        }
        missing.1.ensure_active()?;
        let objects = missing.0;
        let length = objects
            .iter()
            .try_fold(0usize, |sum, object| sum.checked_add(object.bytes.len()))
            .ok_or(StoreError::Integrity("admission length overflow"))?;
        // Output ownership is bounded independently from collision-read waves.
        // Maximal canonical objects retain their existing isolated treatment.
        if objects.len() > super::PHYSICAL_ADMISSION_BATCH_COUNT
            || length > ADMISSION_BATCH_BYTES
            || (objects.len() > 1 && length > 2 * super::INITIALIZATION_SLAB_BYTES)
        {
            return Err(StoreError::Integrity("prepared admission bound"));
        }
        let ids = objects.iter().map(|object| object.id).collect::<Vec<_>>();
        let candidates = ids.iter().copied().collect::<BTreeSet<_>>();
        if candidates.len() != ids.len() {
            return Err(StoreError::Integrity("admission duplicate ownership"));
        }
        let count = ids.len();
        drop(ids);
        drop(candidates);
        let metrics = ObjectInsertMetrics {
            submitted_rows: count as u64,
            ..Default::default()
        };

        let canonical_live_capacity = objects.iter().map(|o| o.bytes.capacity()).sum::<usize>();
        if canonical_live_capacity > 6 * 1024 * 1024 {
            return Err(StoreError::Io(std::io::Error::other(
                "canonical data reservation",
            )));
        }
        let mut prepared = Self {
            session: missing.1,
            final_batch: missing.2,
            absence_epoch: missing.3,
            // No pack-pointer growth during either lane: at most one pack/object.
            packs: Vec::with_capacity(count),
            objects: Vec::with_capacity(count),
            small_signatures: Vec::new(),
            pool_groups: Vec::new(),
            pending_values: BTreeMap::new(),
            metrics,
            native_base_max_pack: 0,
            canonical_live_capacity,
            oversized_backing: 0,
        };
        let mut stats = crate::PhysicalStorageReceipt::default();
        let result = prepared.prepare_full(db, objects, &mut stats);
        db.note_physical(stats);
        result?;
        Ok(prepared)
    }

    fn prepare_full(
        &mut self,
        db: &StoreDb,
        objects: Vec<AuthenticatedCanonicalObject>,
        stats: &mut crate::PhysicalStorageReceipt,
    ) -> Result<()> {
        for object in &objects {
            db.check_canonical_format(&object.bytes)?;
        }
        let input_associations =
            objects.capacity() * std::mem::size_of::<AuthenticatedCanonicalObject>();
        let mut search = DeltaSearch {
            input_associations,
            ..Default::default()
        };
        let (small, objects): (Vec<_>, Vec<_>) = objects
            .into_iter()
            .partition(|object| object.is_small_content());
        self.prepare_small(db, small, stats)?;
        // Native and legacy lanes preserve canonical input order independently.
        // Publish the native lane first; no prepared record can become a base.
        let (native, objects): (Vec<_>, Vec<_>) = objects
            .into_iter()
            .partition(|object| db.native_format() && object.is_file_payload());
        // The original input Vec has dropped; both actual lane allocations live.
        search.input_associations = (native.capacity() + objects.capacity())
            * std::mem::size_of::<AuthenticatedCanonicalObject>();
        self.prepare_native(db, native, &mut search, stats)?;
        let (metadata, ordinary_objects): (Vec<_>, Vec<_>) = objects
            .into_iter()
            .partition(|object| db.compact_namespace() && read::metadata_leaf(&object.bytes));
        let ordinary_capacity = ordinary_objects.capacity();
        for (lane, objects) in [metadata, ordinary_objects].into_iter().enumerate() {
            // The source IntoIter keeps its allocation during intermediate
            // flushes; the other lane remains owned by this outer iterator.
            let sibling_capacity = if lane == 0 { ordinary_capacity } else { 0 };
            let live_capacity = objects.capacity() + sibling_capacity;
            search.input_associations =
                live_capacity * std::mem::size_of::<AuthenticatedCanonicalObject>();
            #[cfg(test)]
            INPUT_ASSOCIATION_TRACE.with(|trace| {
                if let Some(rows) = trace.borrow_mut().as_mut() {
                    rows.push((lane, false, live_capacity, search.input_associations));
                }
            });
            let metadata_lane = objects
                .first()
                .is_some_and(|object| read::metadata_leaf(&object.bytes));
            let pack_limit = if metadata_lane {
                128 * 1024
            } else {
                pack::PACK_LIMIT
            };
            let mut ordinary = Vec::new();
            let mut bytes = 0usize;
            let mut count = 0usize;
            for object in objects {
                if object.bytes.len() + 9 > pack::GROUP_LIMIT {
                    self.prepare_ordinary(db, std::mem::take(&mut ordinary), &mut search, stats)?;
                    bytes = 0;
                    count = 0;
                    stats.full_alternative_bytes += (object.bytes.len() + 9) as u64;
                    stats.selected_encoded_bytes += (object.bytes.len() + 9) as u64;
                    self.prepare_singleton(object)?;
                    continue;
                }
                // Charge one directory entry per possible group. This conservative
                // incremental bound avoids growing-prefix group recounts.
                let next = object.bytes.len() + 5 + 20;
                if bytes + next + 16 > pack_limit || count == pack::RECORD_COUNT_LIMIT {
                    self.prepare_ordinary(db, std::mem::take(&mut ordinary), &mut search, stats)?;
                    bytes = 0;
                    count = 0;
                }
                bytes += next;
                count += 1;
                ordinary.push(object);
            }
            // The exhausted source IntoIter has dropped before this final flush.
            // Only an unconsumed sibling lane still owns upstream vector slots.
            search.input_associations =
                sibling_capacity * std::mem::size_of::<AuthenticatedCanonicalObject>();
            #[cfg(test)]
            INPUT_ASSOCIATION_TRACE.with(|trace| {
                if let Some(rows) = trace.borrow_mut().as_mut() {
                    rows.push((lane, true, sibling_capacity, search.input_associations));
                }
            });
            self.prepare_ordinary(db, ordinary, &mut search, stats)?;
        }
        Ok(())
    }

    fn physical_backing(&self) -> usize {
        self.packs
            .iter()
            .map(PreparedPack::charged_capacity)
            .sum::<usize>()
            - self.oversized_backing
            + std::mem::size_of_val(&self.small_signatures)
            + self.small_signatures.capacity() * std::mem::size_of::<[u64; 8]>()
    }

    fn data_reserve(&self, extra: usize) -> Result<()> {
        let owned = self.canonical_live_capacity
            + self
                .packs
                .iter()
                .map(PreparedPack::charged_capacity)
                .sum::<usize>();
        if owned + extra > 6 * 1024 * 1024 {
            return Err(StoreError::Io(std::io::Error::other(
                "prepared data reservation",
            )));
        }
        Ok(())
    }

    fn native_scratch(
        &self,
        pending: &Vec<NativePrepared>,
        groups: &Vec<pack::EncodedGroup>,
        input_associations: usize,
        extra: usize,
    ) -> Result<()> {
        let owned = self.physical_backing()
            + pending.iter().map(|p| p.record.capacity()).sum::<usize>()
            + groups.iter().map(|g| g.bytes.capacity()).sum::<usize>();
        let associations = input_associations
            + self.objects.capacity() * std::mem::size_of::<PreparedObject>()
            + self.packs.capacity() * std::mem::size_of::<PreparedPack>()
            + pending.capacity() * std::mem::size_of::<NativePrepared>()
            + groups.capacity() * std::mem::size_of::<pack::EncodedGroup>();
        if owned + associations + extra > 2 * 1024 * 1024 {
            return Err(StoreError::Io(std::io::Error::other(
                "native scratch reservation",
            )));
        }
        Ok(())
    }

    fn prepare_small(
        &mut self,
        db: &StoreDb,
        objects: Vec<AuthenticatedCanonicalObject>,
        stats: &mut crate::PhysicalStorageReceipt,
    ) -> Result<()> {
        if objects.is_empty() {
            return Ok(());
        }
        if !db.small_content_format() {
            return Err(StoreError::Integrity(
                "SmallContent write requires schema 8",
            ));
        }
        // Reserve the complete prospective allocation before reserve_exact;
        // existing backing also covers an old allocation during reallocation.
        // Metadata/native slots own no signature, and producers' values survive.
        let signatures = self
            .small_signatures
            .len()
            .checked_add(objects.len())
            .and_then(|count| count.checked_mul(std::mem::size_of::<[u64; 8]>()))
            .ok_or(StoreError::Integrity("small signature reservation"))?;
        if self.physical_backing()
            + signatures
            + self.objects.capacity() * std::mem::size_of::<PreparedObject>()
            + 1024 * 1024
            > 2 * 1024 * 1024
        {
            return Err(StoreError::Integrity("SmallContent physical output budget"));
        }
        self.small_signatures.reserve_exact(objects.len());
        let predecessors = objects
            .iter()
            .filter_map(|o| o.1.prior_ids[0])
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let locations = db.object_locations(&predecessors)?;
        drop(predecessors);
        let mut groups = Vec::new();
        let mut group_bytes = 0;
        let mut encoder = None;
        for object in objects {
            // Static codec workspace is charged to data; operands and handoff to physical output.
            // Signature-vector capacity is included by physical_backing.
            self.data_reserve(3 * 1024 * 1024)?;
            if self.physical_backing()
                + group_bytes * 2
                + self.objects.capacity() * std::mem::size_of::<PreparedObject>()
                + 1024 * 1024
                > 2 * 1024 * 1024
            {
                return Err(StoreError::Integrity("SmallContent physical output budget"));
            }
            let raw = layerfs_content::file::content::small_bytes(&object.bytes)?
                .ok_or(StoreError::Integrity("SmallContent role"))?;
            let mut small_signature = None;
            let mut anchor = if let Some(prior) = object.1.prior_ids[0] {
                // Decoder and encoder never overlap; carry authenticated closure facts forward.
                drop(encoder.take());
                if db.small_chain_format() {
                    db.small_predecessor(prior, locations.get(&prior).copied(), object.bytes.len())?
                        .map(|p| (p.canonical, p.location, p.encoded_closure, 2))
                } else {
                    db.small_anchor(prior, locations.get(&prior).copied())?
                        .map(|(canonical, location)| (canonical, location, 0, 1))
                }
            } else {
                None
            };
            if anchor.is_none() {
                if let Some(candidates) = &self.session.small_candidates {
                    // Output producers precompute this signature on their
                    // parallel path when no explicit predecessor was annotated;
                    // every other object still computes it here from the same
                    // immutable bytes.
                    let signature = object
                        .1
                        .small_signature
                        .unwrap_or_else(|| super::small_candidates::signature(raw));
                    small_signature = Some(signature);
                    let candidate = candidates
                        .lock()
                        .map_err(|_| StoreError::Integrity("small candidate cache"))?
                        .find(object.id, &signature);
                    if let Some(id) = candidate {
                        drop(encoder.take());
                        let location = db
                            .object_locations(&[id])?
                            .remove(&id)
                            .ok_or(StoreError::Integrity("selected small candidate missing"))?;
                        let (canonical, location) = db
                            .small_anchor(id, Some(location))?
                            .ok_or(StoreError::Integrity("selected small candidate role"))?;
                        if canonical.id != id {
                            return Err(StoreError::Integrity(
                                "selected small candidate is not FULL",
                            ));
                        }
                        anchor = Some((canonical, location, 0, 1));
                    }
                }
            }
            if encoder.is_none() {
                encoder = Some(pack::NativeEncoder::new_small()?);
            }
            let started = Instant::now();
            let full = encoder.as_mut().unwrap().compress(raw, None)?;
            stats.encoding_calls += 1;
            stats.full_alternative_bytes +=
                (full.len() + if db.compact_framing() { 5 } else { 25 }) as u64;
            let mut base = None;
            let mut kind = 0;
            let mut frame = full;
            if let Some((anchor, location, encoded_closure, candidate_kind)) = anchor {
                let prefix = layerfs_content::file::content::small_bytes(&anchor.bytes)?
                    .ok_or(StoreError::Integrity("SmallContent anchor role"))?;
                stats.usable_bases += 1;
                stats.candidate_trials += 1;
                let delta = encoder.as_mut().unwrap().compress(raw, Some(prefix))?;
                stats.encoding_calls += 1;
                if delta.len() + 32 < frame.len()
                    && (candidate_kind != 2
                        || encoded_closure + delta.len() + 41 <= super::delta::CHAIN_ENCODED_LIMIT)
                {
                    base = Some(anchor.id);
                    kind = candidate_kind;
                    frame = delta;
                    self.native_base_max_pack = self.native_base_max_pack.max(location.pack);
                }
            }
            stats.encoding_ns += started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
            let delta = base.is_some();
            let group = super::delta::encode(kind, raw.len(), base, frame)?;
            if 16 + 16 * (groups.len() + 1) + group_bytes + group.bytes.len() > pack::PACK_LIMIT
                || groups.len() == pack::GROUP_COUNT_LIMIT
            {
                self.packs
                    .push(PreparedPack::assembled(if db.compact_framing() {
                        pack::assemble_compact_small(&groups)?
                    } else {
                        pack::assemble_small(&groups)?
                    }));
                groups.clear();
                group_bytes = 0;
            }
            stats.eligible_targets += 1;
            stats.full_selected += u64::from(!delta);
            stats.delta_selected += u64::from(delta);
            stats.selected_encoded_bytes +=
                (group.bytes.len() - if db.compact_framing() { 8 } else { 0 }) as u64;
            let small_signature = if let Some(signature) = small_signature {
                let index = u16::try_from(self.small_signatures.len() + 1)
                    .ok()
                    .and_then(std::num::NonZeroU16::new)
                    .ok_or(StoreError::Integrity("small signature index bound"))?;
                self.small_signatures.push(signature);
                Some(index)
            } else {
                None
            };
            self.objects.push(PreparedObject {
                id: object.id,
                length: object.bytes.len(),
                pack: self.packs.len(),
                group: groups.len(),
                record: 0,
                canonical: 0..0,
                retained: Some(object.0.bytes),
                small_signature,
                delta,
                diagnostic_terminal: if delta {
                    diagnostic::DELTA
                } else {
                    diagnostic::NO_DELTA
                },
            });
            group_bytes += group.bytes.len();
            groups.push(group);
        }
        drop(encoder);
        if !groups.is_empty() {
            // The lane's final pack keeps its group vector so the same session
            // can append to its row instead of opening another one.
            let version = if db.compact_framing() { 4 } else { 3 };
            self.packs.push(PreparedPack::retained(version, groups));
        }
        Ok(())
    }

    fn prepare_native(
        &mut self,
        db: &StoreDb,
        objects: Vec<AuthenticatedCanonicalObject>,
        search: &mut DeltaSearch,
        stats: &mut crate::PhysicalStorageReceipt,
    ) -> Result<()> {
        if objects.is_empty() {
            return Ok(());
        }
        let input_associations = search.input_associations;
        let planned_associations = input_associations
            + objects.len() * std::mem::size_of::<NativePrepared>()
            + objects.len().min(pack::GROUP_COUNT_LIMIT)
                * std::mem::size_of::<pack::EncodedGroup>()
            + self.objects.capacity() * std::mem::size_of::<PreparedObject>()
            + self.packs.capacity() * std::mem::size_of::<PreparedPack>();
        if planned_associations > 2 * 1024 * 1024 {
            return Err(StoreError::Io(std::io::Error::other(
                "native association reservation",
            )));
        }
        let mut pending = Vec::<NativePrepared>::with_capacity(objects.len());
        let mut groups =
            Vec::<pack::EncodedGroup>::with_capacity(objects.len().min(pack::GROUP_COUNT_LIMIT));
        let mut full_group_length = 4usize;
        let mut encoder = None;
        for object in objects {
            self.native_scratch(
                &pending,
                &groups,
                input_associations,
                pack::NATIVE_ENCODE_WORKSPACE
                    + 3 * (pack::NATIVE_FRAME_LIMIT + 37)
                    + pack::NATIVE_RAW_LIMIT
                    + 21,
            )?;
            let raw = layerfs_content::file::extent_codec::decode_chunk_payload(
                layerfs_content::decode_bytes_object(&object.bytes)?,
            )?;
            let started = Instant::now();
            let full = (|| {
                if encoder.is_none() {
                    encoder = Some(pack::NativeEncoder::new()?);
                }
                encoder.as_mut().unwrap().compress(raw, None)
            })();
            let elapsed = super::elapsed_ns(started);
            stats.native_full_encode_calls += 1;
            stats.native_full_encode_ns += elapsed;
            stats.encoding_calls += 1;
            stats.encoding_ns += elapsed;
            let full = full?;
            stats.native_full_frame_count += 1;
            stats.native_full_frame_bytes += full.len() as u64;
            // Grouping is frozen by the complete FULL alternative, not the
            // eventual PREFIX size. Pack assembly still uses actual group bytes.
            let next = 4 + 5 + full.len();
            if full_group_length + next > pack::GROUP_LIMIT {
                self.flush_native_group(
                    &mut pending,
                    &mut groups,
                    input_associations,
                    full.capacity(),
                    &mut encoder,
                    stats,
                )?;
                full_group_length = 4;
            }
            full_group_length += next;
            let mut terminal = diagnostic::state(&object);
            let mut chosen = None;
            let mut fallback = NativeFallback::NoHint;
            stats.eligible_targets += 1;
            stats.absent_predecessors += u64::from(!object.1.has_predecessor);
            if let Some(id) = object.prior_ids().iter().flatten().next().copied() {
                // Reader and encoder each own bounded scratch; never overlap them.
                drop(encoder.take());
                search.reads.begin_target();
                if search.trials == 512
                    || self
                        .native_scratch(
                            &pending,
                            &groups,
                            input_associations,
                            full.capacity() + 1024 * 1024,
                        )
                        .is_err()
                {
                    fallback = NativeFallback::Budget;
                    stats.budget_skips += 1;
                    if search.trials == 512 {
                        stats.match_budget_skips += 1;
                        stats.diag_event_match_budget_count += 1;
                        stats.diag_event_match_budget_bytes += object.bytes.len() as u64;
                    } else {
                        stats.memory_budget_skips += 1;
                        stats.diag_event_memory_budget_count += 1;
                        stats.diag_event_memory_budget_bytes += object.bytes.len() as u64;
                    }
                } else {
                    stats.predecessor_hints += 1;
                    match db.read_native_prior(id, &mut search.reads)? {
                        read::NativePriorOutcome::Unavailable => {
                            fallback = NativeFallback::Unavailable
                        }
                        read::NativePriorOutcome::UnsupportedLegacyDelta => {
                            fallback = NativeFallback::LegacyDelta
                        }
                        read::NativePriorOutcome::UnsupportedRole => {
                            fallback = NativeFallback::Role
                        }
                        read::NativePriorOutcome::Budget => {
                            fallback = NativeFallback::Budget;
                            stats.budget_skips += 1;
                            stats.fetch_budget_skips += 1;
                            stats.diag_event_fetch_budget_count += 1;
                            stats.diag_event_fetch_budget_bytes += object.bytes.len() as u64;
                        }
                        read::NativePriorOutcome::Available {
                            canonical,
                            location,
                            depth,
                            raw_closure,
                        } => {
                            if depth >= 4
                                || raw_closure
                                    .checked_add(raw.len())
                                    .is_none_or(|n| n > 1024 * 1024)
                            {
                                fallback = NativeFallback::Depth;
                            } else if self
                                .native_scratch(
                                    &pending,
                                    &groups,
                                    input_associations,
                                    pack::NATIVE_ENCODE_WORKSPACE
                                        + full.capacity()
                                        + 2 * (pack::NATIVE_FRAME_LIMIT + 37)
                                        + canonical.bytes.capacity(),
                                )
                                .is_err()
                            {
                                fallback = NativeFallback::Budget;
                                stats.budget_skips += 1;
                                stats.memory_budget_skips += 1;
                                stats.diag_event_memory_budget_count += 1;
                                stats.diag_event_memory_budget_bytes += object.bytes.len() as u64;
                            } else {
                                stats.usable_bases += 1;
                                stats.candidate_trials += 1;
                                stats.diag_event_base += 1;
                                stats.diag_event_base_bytes += object.bytes.len() as u64;
                                search.trials += 1;
                                let prefix =
                                    layerfs_content::file::extent_codec::decode_chunk_payload(
                                        layerfs_content::decode_bytes_object(&canonical.bytes)?,
                                    )?;
                                // The reader owns a separate canonical allocation;
                                // target and prefix cannot overlap as codec operands.
                                let started = Instant::now();
                                let result = (|| {
                                    encoder = Some(pack::NativeEncoder::new()?);
                                    encoder.as_mut().unwrap().compress(raw, Some(prefix))
                                })();
                                let elapsed = super::elapsed_ns(started);
                                stats.native_prefix_encode_calls += 1;
                                stats.native_prefix_encode_ns += elapsed;
                                stats.encoding_calls += 1;
                                stats.encoding_ns += elapsed;
                                match result {
                                    Ok(frame) => {
                                        stats.native_prefix_frame_count += 1;
                                        stats.native_prefix_frame_bytes += frame.len() as u64;
                                        stats.diag_event_candidate += 1;
                                        stats.diag_event_candidate_bytes +=
                                            object.bytes.len() as u64;
                                        if 37 + frame.len() < 5 + full.len() {
                                            self.native_base_max_pack =
                                                self.native_base_max_pack.max(location.pack);
                                            chosen = Some(pack::native_encode_record(
                                                raw.len(),
                                                Some(id),
                                                &frame,
                                            )?);
                                            terminal = diagnostic::DELTA;
                                        } else {
                                            fallback = NativeFallback::FullWins;
                                        }
                                    }
                                    Err(StoreError::Io(_)) => {
                                        // Codec helper has no I/O: this variant denotes
                                        // its bounded workspace/output resource failure.
                                        fallback = NativeFallback::Budget;
                                        stats.budget_skips += 1;
                                        stats.memory_budget_skips += 1;
                                        stats.diag_event_memory_budget_count += 1;
                                        stats.diag_event_memory_budget_bytes +=
                                            object.bytes.len() as u64;
                                    }
                                    Err(error) => return Err(error),
                                }
                            }
                        }
                    }
                }
            } else {
                stats.targets_without_hints += 1;
            }
            let delta = chosen.is_some();
            if !delta {
                fallback.note(object.bytes.len(), stats);
                if terminal == diagnostic::BASE {
                    terminal = match fallback {
                        NativeFallback::Budget => diagnostic::BUDGET,
                        NativeFallback::FullWins => diagnostic::NO_DELTA,
                        _ => diagnostic::BASE,
                    };
                }
                if matches!(fallback, NativeFallback::Budget) {
                    stats.diag_event_budget += 1;
                    stats.diag_event_budget_bytes += object.bytes.len() as u64;
                }
            }
            let record = match chosen {
                Some(record) => record,
                None => pack::native_encode_record(raw.len(), None, &full)?,
            };
            stats.diag_invalid += u64::from(object.1.diagnostic_grants != 0);
            pending.push(NativePrepared {
                canonical: object.0,
                record,
                delta,
                terminal,
            });
        }
        drop(encoder.take());
        self.flush_native_group(
            &mut pending,
            &mut groups,
            input_associations,
            0,
            &mut encoder,
            stats,
        )?;
        if !groups.is_empty() {
            let length =
                16 + 16 * groups.len() + groups.iter().map(|g| g.bytes.len()).sum::<usize>();
            self.native_scratch(&pending, &groups, input_associations, length)?;
            self.data_reserve(length)?;
            self.packs.push(PreparedPack::retained(2, groups));
        }
        Ok(())
    }

    fn flush_native_group(
        &mut self,
        pending: &mut Vec<NativePrepared>,
        groups: &mut Vec<pack::EncodedGroup>,
        input_associations: usize,
        live_full_capacity: usize,
        encoder: &mut Option<pack::NativeEncoder>,
        stats: &mut crate::PhysicalStorageReceipt,
    ) -> Result<()> {
        if pending.is_empty() {
            return Ok(());
        }
        let group_length =
            4 + 4 * pending.len() + pending.iter().map(|p| p.record.len()).sum::<usize>();
        let assembled_length =
            16 + 16 * groups.len() + groups.iter().map(|g| g.bytes.len()).sum::<usize>();
        // Keep scratch reuse only when the unchanged physical ceiling also fits
        // old records, copied group/pack and references. Releasing the encoder
        // restores the previous assembly ownership without changing pack layout.
        let extra = live_full_capacity
            + group_length
            + assembled_length
            + pending.len() * std::mem::size_of::<&[u8]>();
        if encoder.is_some()
            && self
                .native_scratch(
                    pending,
                    groups,
                    input_associations,
                    extra + pack::NATIVE_ENCODE_WORKSPACE,
                )
                .is_err()
        {
            drop(encoder.take());
        }
        self.native_scratch(
            pending,
            groups,
            input_associations,
            extra
                + if encoder.is_some() {
                    pack::NATIVE_ENCODE_WORKSPACE
                } else {
                    0
                },
        )?;
        let refs = pending
            .iter()
            .map(|p| p.record.as_slice())
            .collect::<Vec<_>>();
        let group = pack::native_group(&refs)?;
        drop(refs);
        let next_bytes = 16
            + 16 * (groups.len() + 1)
            + group.bytes.len()
            + groups.iter().map(|g| g.bytes.len()).sum::<usize>();
        let next_records = group.records + groups.iter().map(|g| g.records).sum::<usize>();
        if next_bytes > pack::PACK_LIMIT
            || groups.len() == pack::GROUP_COUNT_LIMIT
            || next_records > pack::RECORD_COUNT_LIMIT
        {
            self.data_reserve(assembled_length)?;
            let bytes = pack::assemble_native(groups)?;
            self.packs.push(PreparedPack::assembled(bytes));
            groups.clear();
        }
        let pack = self.packs.len();
        let group_number = groups.len();
        for (record_number, entry) in pending.drain(..).enumerate() {
            let length = entry.canonical.bytes.len();
            self.objects.push(PreparedObject {
                id: entry.canonical.id,
                length,
                pack,
                group: group_number,
                record: record_number,
                canonical: 0..0,
                // Native FULL is internally compressed too: final CAS must retain
                // the authentic canonical operand, never compare frame bytes.
                retained: Some(entry.canonical.bytes),
                small_signature: None,
                delta: entry.delta,
                diagnostic_terminal: entry.terminal,
            });
        }
        stats.selected_encoded_bytes += group.bytes.len() as u64;
        groups.push(group);
        Ok(())
    }

    fn prepare_ordinary(
        &mut self,
        db: &StoreDb,
        mut objects: Vec<AuthenticatedCanonicalObject>,
        search: &mut DeltaSearch,
        stats: &mut crate::PhysicalStorageReceipt,
    ) -> Result<()> {
        if objects.is_empty() {
            return Ok(());
        }
        let metadata = db.compact_namespace()
            && objects
                .iter()
                .all(|object| read::metadata_leaf(&object.bytes));
        let mut values = if metadata {
            Some(prepare_values(
                db,
                &objects,
                self.pool_groups
                    .last()
                    .map(|group| group.first + group.count as u64),
                &mut self.pending_values,
                stats,
            )?)
        } else {
            None
        };
        let mut groups = Vec::<Vec<usize>>::new();
        let mut pending = [Vec::new(), Vec::new()];
        let mut sizes = [4usize, 4usize];
        for (index, object) in objects.iter().enumerate() {
            let content = is_content(&object.bytes)?;
            let role = usize::from(content);
            let target = if content { 32 * 1024 } else { 16 * 1024 };
            let next = 5 + values
                .as_ref()
                .map_or(object.bytes.len(), |values| values.physical[index].len());
            if !pending[role].is_empty() && sizes[role] + next > target {
                groups.push(std::mem::take(&mut pending[role]));
                sizes[role] = 4;
            }
            pending[role].push(index);
            sizes[role] += next;
        }
        for group in pending {
            if !group.is_empty() {
                groups.push(group);
            }
        }
        let mut encoded = Vec::with_capacity(groups.len());
        let pack_index = self.packs.len();
        let mut offset = 16 + 16 * groups.len();
        let fixed_associations = search.input_associations
            + values.as_ref().map_or(0, |values| values.backing())
            + self.pool_groups.capacity() * std::mem::size_of::<PreparedValueGroup>()
            + self.packs.capacity() * std::mem::size_of::<PreparedPack>()
            + self.objects.capacity() * std::mem::size_of::<PreparedObject>()
            + objects.capacity() * std::mem::size_of::<AuthenticatedCanonicalObject>()
            + groups.capacity() * std::mem::size_of::<Vec<usize>>()
            + groups
                .iter()
                .map(|group| group.capacity() * std::mem::size_of::<usize>())
                .sum::<usize>()
            + encoded.capacity() * std::mem::size_of::<pack::EncodedGroup>();
        let mut backing = self.physical_backing();
        for (group_number, group) in groups.iter().enumerate() {
            let mut deltas = Vec::with_capacity(group.len());
            // All live delta capacities sum to at most the group's FULL decoded
            // size. Matching scratch is dropped before either codec invocation.
            let associations = fixed_associations
                + group.len()
                    * (std::mem::size_of::<Option<Vec<u8>>>() + std::mem::size_of::<&[u8]>());
            // Worst codec phase: static 1-MiB context, RAW group and complete
            // compressBound output. Optional B additionally keeps A and programs.
            // Canonical comparison operands belong to the other <=6 MiB; count
            // their vector associations here as a conservative duplicate charge.
            let required = backing + associations + 1024 * 1024 + 2 * (pack::GROUP_LIMIT + 1024);
            #[cfg(test)]
            if std::env::var_os("LAYERFS_CARDINALITY_DIAGNOSTIC_INPUT").is_some() {
                static PRINTED: std::sync::atomic::AtomicBool =
                    std::sync::atomic::AtomicBool::new(false);
                #[allow(dead_code)]
                struct InlineSignatureSlot {
                    id: ObjectId,
                    length: usize,
                    pack: usize,
                    group: usize,
                    record: usize,
                    canonical: Range<usize>,
                    retained: Option<Vec<u8>>,
                    small_signature: Option<[u64; 8]>,
                    delta: bool,
                    diagnostic_terminal: u8,
                }
                let old_required = required
                    + self.objects.capacity()
                        * (std::mem::size_of::<InlineSignatureSlot>()
                            - std::mem::size_of::<PreparedObject>())
                    - std::mem::size_of_val(&self.small_signatures)
                    - self.small_signatures.capacity() * std::mem::size_of::<[u64; 8]>();
                if old_required > 2 * 1024 * 1024
                    && !PRINTED.swap(true, std::sync::atomic::Ordering::Relaxed)
                {
                    let mut lanes = [0usize; 7];
                    for object in &self.objects {
                        let version = self
                            .packs
                            .get(object.pack)
                            .map_or(0, |pack| pack.version() as usize);
                        if version < lanes.len() {
                            lanes[version] += 1;
                        }
                    }
                    eprintln!("cardinality same-shape old_required={old_required} new_required={required} slots={} occupied={} signature_capacity={} signature_bytes={} prior_lanes_by_pack_version={lanes:?}", self.objects.capacity(), self.objects.len(), self.small_signatures.capacity(), self.small_signatures.capacity() * std::mem::size_of::<[u64;8]>());
                }
            }
            if required > 2 * 1024 * 1024 {
                #[cfg(test)]
                eprintln!(
                    "physical reservation required={required} backing={backing} associations={associations} slots={} slot_bytes={} input_associations={} lane_capacity={} lane_slot_bytes={} metadata={metadata}",
                    self.objects.capacity(), std::mem::size_of::<PreparedObject>(),
                    search.input_associations, objects.capacity(),
                    std::mem::size_of::<AuthenticatedCanonicalObject>(),
                );
                return Err(StoreError::Io(std::io::Error::other(
                    "physical encoding reservation",
                )));
            }
            let optional = required + 2 * (pack::GROUP_LIMIT + 1024) <= 2 * 1024 * 1024;
            for index in group {
                let object = &objects[*index];
                let mut terminal = diagnostic::state(object);
                let before_bases = stats.usable_bases;
                let before_fetch = stats.fetch_budget_skips;
                let before_match = stats.match_budget_skips;
                let before_instruction = stats.instruction_budget_skips;
                let before_memory = stats.memory_budget_skips;
                if optional {
                    deltas.push(
                        search.candidate(
                            db,
                            object,
                            values
                                .as_ref()
                                .map(|values| values.physical[*index].as_slice()),
                            stats,
                        )?,
                    );
                } else {
                    stats.memory_budget_skips += 1;
                    stats.budget_skips += 1;
                    deltas.push(None);
                }
                if terminal != 0 {
                    let has_candidate = deltas.last().unwrap().is_some();
                    let budget = stats.fetch_budget_skips != before_fetch
                        || stats.match_budget_skips != before_match
                        || stats.instruction_budget_skips != before_instruction
                        || stats.memory_budget_skips != before_memory;
                    let base = stats.usable_bases != before_bases;
                    stats.diag_event_base += u64::from(base);
                    stats.diag_event_base_bytes += u64::from(base) * object.bytes.len() as u64;
                    stats.diag_event_budget += u64::from(budget);
                    stats.diag_event_budget_bytes += u64::from(budget) * object.bytes.len() as u64;
                    stats.diag_event_candidate += u64::from(has_candidate);
                    stats.diag_event_candidate_bytes +=
                        u64::from(has_candidate) * object.bytes.len() as u64;
                    if stats.fetch_budget_skips != before_fetch {
                        stats.diag_event_fetch_budget_count += 1;
                        stats.diag_event_fetch_budget_bytes += object.bytes.len() as u64;
                    }
                    if stats.match_budget_skips != before_match {
                        stats.diag_event_match_budget_count += 1;
                        stats.diag_event_match_budget_bytes += object.bytes.len() as u64;
                    }
                    if stats.instruction_budget_skips != before_instruction {
                        stats.diag_event_instruction_budget_count += 1;
                        stats.diag_event_instruction_budget_bytes += object.bytes.len() as u64;
                    }
                    if stats.memory_budget_skips != before_memory {
                        stats.diag_event_memory_budget_count += 1;
                        stats.diag_event_memory_budget_bytes += object.bytes.len() as u64;
                    }
                    if has_candidate {
                        terminal = diagnostic::DELTA;
                    } else if terminal == diagnostic::BASE {
                        terminal = if budget {
                            diagnostic::BUDGET
                        } else if base {
                            diagnostic::NO_DELTA
                        } else {
                            diagnostic::BASE
                        };
                    }
                }
                // Grant credit is consumed by initial CAS before MissingBatch.
                // From here no object can be spilled/rebuffered: reuse this byte
                // for terminal state until the PreparedObject takes ownership.
                stats.diag_invalid += u64::from(objects[*index].1.diagnostic_grants != 0);
                objects[*index].1.diagnostic_grants = terminal;
            }
            let canonical = group
                .iter()
                .map(|index| {
                    values
                        .as_ref()
                        .map_or(objects[*index].bytes.as_slice(), |values| {
                            values.physical[*index].as_slice()
                        })
                })
                .collect::<Vec<_>>();
            let (selected, mixed) = pack::encode_group(&canonical, &deltas, stats)?;
            let mut cursor = offset + 4 + 4 * group.len();
            for (record_number, index) in group.iter().enumerate() {
                let object = &mut objects[*index];
                let delta = mixed && deltas[record_number].is_some();
                let record_length = if delta {
                    deltas[record_number].as_ref().unwrap().len()
                } else {
                    1 + values
                        .as_ref()
                        .map_or(object.bytes.len(), |values| values.physical[*index].len())
                };
                let end = cursor + record_length;
                let mut diagnostic_terminal = object.1.diagnostic_grants;
                if diagnostic_terminal == diagnostic::DELTA && !mixed {
                    diagnostic_terminal = diagnostic::MIXED_REJECTION;
                    stats.diag_event_mixed_rejection += 1;
                    stats.diag_event_mixed_rejection_bytes += object.bytes.len() as u64;
                }
                let canonical_length = object.bytes.len();
                let retained = if metadata || delta || selected.codec == pack::Codec::Zstandard {
                    Some(std::mem::take(&mut object.0.bytes))
                } else {
                    self.canonical_live_capacity -= object.0.bytes.capacity();
                    object.0.bytes = Vec::new();
                    None
                };
                self.objects.push(PreparedObject {
                    id: object.id,
                    length: canonical_length,
                    pack: pack_index,
                    group: group_number,
                    record: record_number,
                    canonical: cursor + 1..end,
                    retained,
                    small_signature: None,
                    delta,
                    diagnostic_terminal,
                });
                cursor = end;
            }
            offset += selected.bytes.len();
            backing += selected.bytes.capacity();
            encoded.push(selected);
        }
        if let Some(values) = values.take() {
            for (mut catalogue, group) in values.groups {
                catalogue.pack = pack_index;
                catalogue.group = encoded.len();
                self.pool_groups.push(catalogue);
                encoded.push(group);
            }
        }
        let length = 16 + 16 * encoded.len() + encoded.iter().map(|g| g.bytes.len()).sum::<usize>();
        self.data_reserve(length)?;
        if backing + fixed_associations + length > 2 * 1024 * 1024 {
            return Err(StoreError::Io(std::io::Error::other(
                "legacy assembly reservation",
            )));
        }
        self.packs.push(PreparedPack::retained(
            if metadata { 6 } else { 1 },
            encoded,
        ));
        Ok(())
    }

    fn prepare_singleton(&mut self, object: AuthenticatedCanonicalObject) -> Result<()> {
        let id = object.id;
        let length = object.bytes.len();
        let total = length + 41;
        let mut prefix = Vec::with_capacity(41);
        prefix.extend_from_slice(b"LFPACK\0\0\x01\x00\x00\x00\x01\x00\x00\x00");
        for value in [32usize, length + 9, length + 9] {
            prefix.extend_from_slice(&(value as u32).to_le_bytes());
        }
        prefix.extend_from_slice(&[0; 4]);
        prefix.extend_from_slice(&1u32.to_le_bytes());
        prefix.extend_from_slice(&((length + 1) as u32).to_le_bytes());
        prefix.push(0);
        // The existing private temporary-file owner avoids a second resident
        // multi-MiB canonical copy. Its exact-byte digest is held only here.
        let (mut file, path) = super::spill::temporary_file("prepared-pack")?;
        let _path = super::spill::TempPath::new(path);
        let mut digest = blake3::Hasher::new();
        digest.update(&prefix);
        digest.update(&object.bytes);
        file.write_all(&prefix)?;
        file.write_all(&object.bytes)?;
        self.canonical_live_capacity -= object.bytes.capacity();
        drop(object);
        self.data_reserve(total)?;
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = vec![0; total];
        file.read_exact(&mut bytes)?;
        let mut trailing = [0];
        if file.read(&mut trailing)? != 0 || blake3::hash(&bytes) != digest.finalize() {
            return Err(StoreError::Integrity("prepared pack spool identity"));
        }
        self.objects.push(PreparedObject {
            id,
            length,
            pack: self.packs.len(),
            group: 0,
            record: 0,
            canonical: 41..total,
            retained: None,
            small_signature: None,
            delta: false,
            diagnostic_terminal: 0,
        });
        // This exception belongs only to the existing constructed version-1
        // RAW singleton; ordinary/native packs never enter this data-only lane.
        self.oversized_backing += bytes.capacity();
        self.packs.push(PreparedPack::assembled(bytes));
        Ok(())
    }

    pub(crate) fn publish<T>(
        self,
        db: &StoreDb,
        statement_number: &mut u64,
        publish: impl FnOnce(&Connection, &ObjectInsertMetrics, &mut u64) -> Result<T>,
    ) -> Result<(T, super::AdmissionBatchMetrics)> {
        let session = self.session.clone();
        session.resolve(self.publish_inner(db, statement_number, publish))
    }

    fn publish_inner<T>(
        mut self,
        db: &StoreDb,
        statement_number: &mut u64,
        publish: impl FnOnce(&Connection, &ObjectInsertMetrics, &mut u64) -> Result<T>,
    ) -> Result<(T, super::AdmissionBatchMetrics)> {
        if !db.same_instance(&self.session.db) {
            return Err(StoreError::Integrity("admission Store ownership"));
        }
        self.session.ensure_active()?;
        // Assemble each lane's retained final pack exactly once and detach its
        // group vector, so the same session can extend its open pack row instead
        // of publishing another partially used row.
        let mut open_groups: Vec<Option<Vec<pack::EncodedGroup>>> =
            Vec::with_capacity(self.packs.len());
        for pack in self.packs.iter_mut() {
            open_groups.push(pack.materialize()?);
        }
        let ids = self
            .objects
            .iter()
            .map(|object| object.id)
            .collect::<Vec<_>>();
        let mut supplied = self
            .objects
            .iter()
            .map(|object| {
                (
                    object.id,
                    object
                        .retained
                        .as_deref()
                        .unwrap_or_else(|| &self.packs[object.pack][object.canonical.clone()]),
                )
            })
            .collect::<Vec<_>>();

        // Absence was authenticated by the exact earlier lookup under this
        // exclusive owner. An intervening publication requires the normal CAS
        // recheck; every positive collision still compares canonical bytes.
        let late = if self.absence_epoch
            == Some(
                self.session
                    .publication_epoch
                    .load(std::sync::atomic::Ordering::Acquire),
            ) {
            BTreeMap::new()
        } else {
            db.object_locations(&ids)?
        };
        drop(ids);
        let retained = self.physical_backing()
            + self.objects.capacity() * std::mem::size_of::<PreparedObject>()
            + self.packs.capacity() * std::mem::size_of::<PreparedPack>();
        compare(db, &late, &mut supplied, &mut self.metrics, retained)?;
        let mut winners = vec![Vec::new(); self.packs.len()];
        for object in &self.objects {
            if !late.contains_key(&object.id) {
                winners[object.pack].push(object);
            }
        }
        let connection = db.writer()?;
        let canonical_bytes = self.objects.iter().map(|object| object.length as u64).sum();
        self.session.begin_batch(
            &connection,
            self.metrics.submitted_rows,
            canonical_bytes,
            &mut self.metrics.sql,
        )?;
        let started = Instant::now();
        let mut diagnostic_stats =
            self.insert(&connection, &winners, &mut open_groups, statement_number)?;
        self.metrics.insert_ns += super::elapsed_ns(started);
        self.metrics.objects = winners.iter().map(|objects| objects.len() as u64).sum();
        self.metrics.bytes = winners
            .iter()
            .flatten()
            .map(|object| object.length as u64)
            .sum();
        self.metrics.returned_ids = self.metrics.objects;
        let result = publish(&connection, &self.metrics, statement_number)?;
        self.session
            .note_published_ids(winners.iter().flatten().map(|object| &object.id))?;
        if self.final_batch || !self.session.coalesce {
            self.session
                .commit_pending(&connection, &mut self.metrics.sql, self.final_batch)?;
        }
        // Epoch tracks connection-visible object publications, not disk commits.
        self.session
            .publication_epoch
            .fetch_update(
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
                |epoch| epoch.checked_add(1),
            )
            .map_err(|_| StoreError::Integrity("admission publication epoch overflow"))?;
        if self.final_batch {
            self.session.retain();
        }
        drop(connection);
        if !self.final_batch {
            if let Some(candidates) = &self.session.small_candidates {
                for object in winners.iter().flatten().filter(|object| !object.delta) {
                    if !matches!(self.packs[object.pack].version(), 3 | 4) {
                        continue;
                    }
                    let canonical = object
                        .retained
                        .as_ref()
                        .ok_or(StoreError::Integrity("selected small candidate ownership"))?;
                    let raw = layerfs_content::file::content::small_bytes(canonical)?
                        .ok_or(StoreError::Integrity("selected small candidate role"))?;
                    // Preparation already scanned this immutable value for candidate
                    // lookup. Explicit-predecessor paths that skipped the search
                    // compute the signature lazily here, before the winner is visible.
                    let signature = object
                        .small_signature
                        .map(|index| self.small_signatures[usize::from(index.get()) - 1])
                        .unwrap_or_else(|| super::small_candidates::signature(raw));
                    candidates
                        .lock()
                        .map_err(|_| StoreError::Integrity("small candidate cache"))?
                        .insert(object.id, signature);
                }
            }
        }
        for object in &self.objects {
            if object.diagnostic_terminal == 0 {
                continue;
            }
            if late.contains_key(&object.id) {
                diagnostic_stats.diag_race_count += 1;
                diagnostic_stats.diag_race_bytes += object.length as u64;
            } else {
                if object.delta {
                    diagnostic_stats.diag_new_delta_count += 1;
                    diagnostic_stats.diag_new_delta_bytes += object.length as u64;
                } else {
                    diagnostic_stats.diag_new_full_count += 1;
                    diagnostic_stats.diag_new_full_bytes += object.length as u64;
                }
                if self.packs[object.pack].version() == 2 {
                    if object.delta {
                        diagnostic_stats.native_admitted_prefix_count += 1;
                        diagnostic_stats.native_admitted_prefix_bytes += object.length as u64;
                    } else {
                        diagnostic_stats.native_admitted_full_count += 1;
                        diagnostic_stats.native_admitted_full_bytes += object.length as u64;
                    }
                }
                diagnostic::terminal(
                    object.diagnostic_terminal,
                    object.length,
                    &mut diagnostic_stats,
                );
            }
        }
        db.note_physical(diagnostic_stats);
        db.note_physical(crate::PhysicalStorageReceipt {
            full_selected: winners
                .iter()
                .flatten()
                .filter(|object| !object.delta)
                .count() as u64,
            delta_selected: winners
                .iter()
                .flatten()
                .filter(|object| object.delta)
                .count() as u64,
            ..Default::default()
        });
        Ok((
            result,
            super::AdmissionBatchMetrics {
                insert: self.metrics,
                begin_ns: self.metrics.sql.begin_ns,
                commit_ns: self.metrics.sql.commit_ns,
            },
        ))
    }

    fn insert(
        &self,
        transaction: &Connection,
        winners: &[Vec<&PreparedObject>],
        open_groups: &mut [Option<Vec<pack::EncodedGroup>>],
        statement_number: &mut u64,
    ) -> Result<crate::PhysicalStorageReceipt> {
        let mut diagnostic_stats = crate::PhysicalStorageReceipt::default();
        let mut next: i64 = transaction
            .prepare_cached("SELECT COALESCE(MAX(pack_id),0) FROM object_packs")?
            .query_row([], |row| row.get(0))?;
        // Bases came from selected immutable locations before preparation.
        // All newly assigned IDs exceed this transaction's existing maximum.
        if self.native_base_max_pack > next {
            return Err(StoreError::Integrity("native base publication chronology"));
        }
        let keep_pools = winners
            .iter()
            .enumerate()
            .any(|(index, objects)| !objects.is_empty() && self.packs[index].version() == 6);
        let mut pool_packs = BTreeMap::new();
        let mut pool_offsets = BTreeMap::new();
        // A lane's first pack may extend this session's open pack. Later packs of
        // the same lane always open a new row so a lane's rows stay ordered.
        let mut fresh_lanes = std::collections::BTreeSet::new();
        let mut packs = Vec::new();
        let mut locators = Vec::new();
        for (index, objects) in winners.iter().enumerate() {
            if objects.is_empty()
                && !(keep_pools && self.pool_groups.iter().any(|group| group.pack == index))
            {
                continue;
            }
            let version = self.packs[index].version();
            let mut merged = None;
            if !fresh_lanes.contains(&version) {
                if let Some(groups) = open_groups[index].take() {
                    match self.session.append_open_pack(
                        transaction,
                        version,
                        &groups,
                        statement_number,
                    )? {
                        Some(target) => merged = Some(target),
                        None => open_groups[index] = Some(groups),
                    }
                }
            }
            let (pack, group_offset) = match merged {
                Some(target) => target,
                None => {
                    next = next
                        .checked_add(1)
                        .filter(|id| *id > 0)
                        .ok_or(StoreError::Integrity("pack identity exhausted"))?;
                    fresh_lanes.insert(version);
                    packs.push((next, &self.packs[index][..]));
                    if let Some(groups) = open_groups[index].take() {
                        self.session.note_open_pack(
                            version,
                            next,
                            self.packs[index].len(),
                            groups,
                        )?;
                    }
                    (next, 0)
                }
            };
            // One selected pack per lane contribution: a pack appended to the
            // session's open row and a newly opened row both publish exactly the
            // records and groups selected here. The row count is physical.
            diagnostic_stats.diag_selected_pack_count += 1;
            diagnostic_stats.diag_selected_pack_last_id = pack as u64;
            diagnostic_stats.diag_selected_pack_bytes += self.packs[index].len() as u64;
            diagnostic_stats.diag_selected_pack_groups += self.packs[index].group_count() as u64;
            let first = self.objects.partition_point(|object| object.pack < index);
            let last = self.objects.partition_point(|object| object.pack <= index);
            diagnostic_stats.diag_selected_pack_records += (last - first) as u64;
            diagnostic_stats.diag_selected_unlocated_records +=
                (last - first - objects.len()) as u64;
            pool_packs.insert(index, pack);
            pool_offsets.insert(index, group_offset);
            for object in objects {
                locators.push((pack, group_offset, *object));
            }
        }
        let pack_rows = sql_rows(transaction, 2, 6)?;
        let blob_limit = usize::try_from(transaction.limit(Limit::SQLITE_LIMIT_LENGTH)?)
            .map_err(|_| StoreError::Integrity("SQLite BLOB limit"))?;
        let mut start = 0;
        while start < packs.len() {
            let mut end = start;
            let mut bytes = 0usize;
            while end < packs.len() && end - start < pack_rows {
                let length = packs[end].1.len();
                if length > blob_limit || length > ADMISSION_BATCH_BYTES + 41 {
                    return Err(StoreError::Integrity("pack INSERT byte limit"));
                }
                // Ordinary statements bind at most 1 MiB of BLOBs. The format's
                // oversized RAW singleton is one separately bounded parameter.
                if end > start && bytes + length > 1024 * 1024 {
                    break;
                }
                bytes += length;
                end += 1;
            }
            let page = &packs[start..end];
            let sql = format!(
                "INSERT INTO object_packs(pack_id,data) VALUES {}",
                vec!["(?,?)"; page.len()].join(",")
            );
            let values = page.iter().flat_map(|(id, bytes)| {
                [id as &dyn rusqlite::ToSql, bytes as &dyn rusqlite::ToSql]
            });
            *statement_number += 1;
            crate::schema::fail_transaction_statement(*statement_number)?;
            if transaction
                .prepare_cached(&sql)?
                .execute(params_from_iter(values))?
                != page.len()
            {
                return Err(StoreError::Integrity("pack insertion cardinality"));
            }
            start = end;
        }
        let mut next_ordinal: i64 = if keep_pools {
            super::metadata::next_ordinal(transaction)? as i64
        } else {
            1
        };
        for group in self
            .pool_groups
            .iter()
            .filter(|group| keep_pools && pool_packs.contains_key(&group.pack))
        {
            if group.first as i64 != next_ordinal {
                return Err(StoreError::Integrity(
                    "metadata pool publication epoch moved",
                ));
            }
            *statement_number += 1;
            crate::schema::fail_transaction_statement(*statement_number)?;
            transaction
                .prepare_cached("INSERT INTO metadata_value_groups(first_ordinal,count,pack_id,group_number,digest) VALUES (?1,?2,?3,?4,?5)")?
                .execute(
                    rusqlite::params![
                        group.first as i64,
                        group.count as i64,
                        pool_packs[&group.pack],
                        (group.group + pool_offsets.get(&group.pack).copied().unwrap_or(0)) as i64,
                        group.digest.as_bytes().as_slice()
                    ],
                )?;
            next_ordinal += group.count as i64;
            diagnostic_stats.metadata_pool_admitted_groups += 1;
            diagnostic_stats.metadata_pool_admitted_values += group.count as u64;
        }
        // Preserve pack bytes/order; only the SQL primary-key insertion order changes.
        let sort_started = Instant::now();
        locators.sort_unstable_by_key(|(_, _, object)| object.id);
        crate::telemetry::note_workspace_admission_sort(super::elapsed_ns(sort_started));
        let locator_rows = sql_rows(transaction, 5, 12)?;
        for page in locators.chunks(locator_rows) {
            let sql = format!(
                "INSERT INTO objects(object_id,canonical_length,pack_id,group_number,record_number) VALUES {}",
                vec!["(?,?,?,?,?)"; page.len()].join(",")
            );
            let values = page.iter().flat_map(|(pack, offset, object)| {
                [
                    Value::Blob(object.id.as_bytes().to_vec()),
                    Value::Integer(object.length as i64),
                    Value::Integer(*pack),
                    Value::Integer((offset + object.group) as i64),
                    Value::Integer(object.record as i64),
                ]
            });
            *statement_number += 1;
            crate::schema::fail_transaction_statement(*statement_number)?;
            if transaction
                .prepare_cached(&sql)?
                .execute(params_from_iter(values))?
                != page.len()
            {
                return Err(StoreError::Integrity("locator insertion cardinality"));
            }
        }
        Ok(diagnostic_stats)
    }
}

struct NativePrepared {
    canonical: super::CanonicalObject,
    record: Vec<u8>,
    delta: bool,
    terminal: u8,
}

#[derive(Clone, Copy)]
enum NativeFallback {
    NoHint,
    Unavailable,
    LegacyDelta,
    Role,
    Depth,
    Budget,
    FullWins,
}
impl NativeFallback {
    fn note(self, bytes: usize, stats: &mut crate::PhysicalStorageReceipt) {
        macro_rules! count {
            ($n:ident,$b:ident) => {{
                stats.$n += 1;
                stats.$b += bytes as u64;
            }};
        }
        match self {
            Self::NoHint => count!(native_fallback_no_hint_count, native_fallback_no_hint_bytes),
            Self::Unavailable => count!(
                native_fallback_unavailable_count,
                native_fallback_unavailable_bytes
            ),
            Self::LegacyDelta => count!(
                native_fallback_legacy_delta_count,
                native_fallback_legacy_delta_bytes
            ),
            Self::Role => count!(native_fallback_role_count, native_fallback_role_bytes),
            Self::Depth => count!(native_fallback_depth_count, native_fallback_depth_bytes),
            Self::Budget => count!(native_fallback_budget_count, native_fallback_budget_bytes),
            Self::FullWins => count!(
                native_fallback_full_wins_count,
                native_fallback_full_wins_bytes
            ),
        }
    }
}

/// Optional search is batch-local; admitted immutable locations are the only
/// source of bases. No record in this prepared batch can become an anchor.
struct DeltaSearch {
    reads: read::HintReadBudget,
    trials: usize,
    remaining: usize,
    input_associations: usize,
}

impl Default for DeltaSearch {
    fn default() -> Self {
        Self {
            reads: read::HintReadBudget::default(),
            trials: 0,
            remaining: 16 * 1024 * 1024,
            input_associations: 0,
        }
    }
}

impl DeltaSearch {
    fn candidate(
        &mut self,
        db: &StoreDb,
        object: &AuthenticatedCanonicalObject,
        pooled_target: Option<&[u8]>,
        stats: &mut crate::PhysicalStorageReceipt,
    ) -> Result<Option<Vec<u8>>> {
        // Payload span hints retain their policy. S1 additionally accepts only
        // exact inode leaves carrying the tree editor's immutable origin.
        let value = layerfs_content::decode_bytes_object(&object.bytes);
        let Ok(value) = value else {
            return Ok(None);
        };
        let inode_leaf = super::is_inode_table_leaf(&object.bytes)?;
        if !inode_leaf {
            if !value.starts_with(layerfs_content::file::extent_codec::CHUNK_MAGIC) {
                return Ok(None);
            }
            layerfs_content::file::extent_codec::decode_chunk_payload(value)?;
        }
        if object.bytes.len() + 9 > pack::GROUP_LIMIT {
            return Ok(None);
        }
        stats.eligible_targets += 1;
        let hints = object.prior_ids();
        if !object.1.has_predecessor {
            stats.absent_predecessors += 1;
        }
        if hints.iter().all(Option::is_none) {
            stats.targets_without_hints += 1;
            return Ok(None);
        }
        let mut seen_hints = BTreeSet::new();
        let mut anchors = BTreeSet::new();
        let mut best: Option<(ObjectId, Vec<u8>)> = None;
        self.reads.begin_target();
        for id in hints.iter().flatten().copied() {
            if !seen_hints.insert(id) || anchors.contains(&id) {
                continue;
            }
            if self.trials == 512 || self.remaining == 0 {
                stats.budget_skips += 1;
                stats.match_budget_skips += 1;
                break;
            }
            stats.predecessor_hints += 1;
            let prior = if db.compact_namespace() && read::metadata_leaf(&object.bytes) {
                match db.metadata_predecessor(id, &mut self.reads)? {
                    Some(base)
                        if base.pooled
                            && base.depth < read::METADATA_EDGES
                            && base.canonical_closure + object.bytes.len()
                                <= read::METADATA_CLOSURE =>
                    {
                        Some(read::HintRecord::Full(super::CanonicalObject {
                            id: base.canonical.id,
                            bytes: base.physical,
                        }))
                    }
                    _ => None,
                }
            } else {
                db.read_hint(id, false, &mut self.reads)?
            };
            let base = match prior {
                Some(read::HintRecord::Full(base)) => base,
                Some(read::HintRecord::Anchor(_)) if inode_leaf => continue,
                Some(read::HintRecord::Anchor(id)) => {
                    if anchors.contains(&id) {
                        continue;
                    }
                    match db.read_hint(id, true, &mut self.reads)? {
                        Some(read::HintRecord::Full(base)) => base,
                        Some(read::HintRecord::Anchor(_)) => {
                            return Err(StoreError::Integrity("delta anchor is not FULL"));
                        }
                        None if self.reads.exhausted => {
                            stats.budget_skips += 1;
                            stats.fetch_budget_skips += 1;
                            break;
                        }
                        None => continue,
                    }
                }
                None if self.reads.exhausted => {
                    stats.budget_skips += 1;
                    stats.fetch_budget_skips += 1;
                    break;
                }
                None => continue,
            };
            if !anchors.insert(base.id) {
                continue;
            }
            if inode_leaf {
                if pooled_target.is_none() && !super::is_inode_table_leaf(&base.bytes)? {
                    continue;
                }
            } else {
                let base_value = layerfs_content::decode_bytes_object(&base.bytes)?;
                if !base_value.starts_with(layerfs_content::file::extent_codec::CHUNK_MAGIC) {
                    continue;
                }
                layerfs_content::file::extent_codec::decode_chunk_payload(base_value)?;
            }
            stats.usable_bases += 1;
            stats.candidate_trials += 1;
            self.trials += 1;
            let started = Instant::now();
            let result = pack::delta_record(
                base.id,
                &base.bytes,
                pooled_target.unwrap_or(&object.bytes),
                &mut self.remaining,
                stats,
            );
            stats.matching_ns += super::elapsed_ns(started);
            let candidate = result?;
            if let Some(candidate) = candidate {
                if best
                    .as_ref()
                    .is_none_or(|(id, bytes)| (candidate.len(), base.id) < (bytes.len(), *id))
                {
                    best = Some((base.id, candidate));
                }
            }
            if self.remaining == 0 {
                stats.budget_skips += 1;
                break;
            }
        }
        Ok(best.map(|(_, bytes)| bytes))
    }
}

fn sql_rows(
    connection: &rusqlite::Connection,
    parameters: usize,
    row_bytes: usize,
) -> Result<usize> {
    let parameters_limit =
        usize::try_from(connection.limit(Limit::SQLITE_LIMIT_VARIABLE_NUMBER)?).unwrap_or(0);
    let sql_limit = usize::try_from(connection.limit(Limit::SQLITE_LIMIT_SQL_LENGTH)?).unwrap_or(0);
    let count = OBJECT_PAGE_COUNT
        .min(parameters_limit / parameters)
        .min(sql_limit.saturating_sub(256) / row_bytes);
    if count == 0 {
        return Err(StoreError::Integrity("SQLite bulk statement limit"));
    }
    Ok(count)
}

fn is_content(canonical: &[u8]) -> Result<bool> {
    if canonical.get(4) == Some(&(layerfs_content::ObjectKind::Directory as u8)) {
        return Ok(false);
    }
    let value = layerfs_content::decode_bytes_object(canonical)?;
    Ok(!matches!(
        value.get(..8),
        Some(
            b"LFS4FSR\0"
                | b"LFS6FSR\0"
                | b"LFS6INT\0"
                | b"LFS6NSP\0"
                | b"LFS4INT\0"
                | b"LFS4INO\0"
                | b"LFS4DIR\0"
                | b"LFS4NSP\0"
                | b"LFS4MET\0"
                | b"LFS4MAP\0"
                | b"LFS4LNK\0"
        )
    ))
}

pub(super) fn compare(
    db: &StoreDb,
    known: &BTreeMap<ObjectId, read::Location>,
    supplied: &mut Vec<(ObjectId, &[u8])>,
    metrics: &mut ObjectInsertMetrics,
    retained_physical: usize,
) -> Result<()> {
    let started = Instant::now();
    if supplied.len() > super::PHYSICAL_ADMISSION_BATCH_COUNT || known.len() > supplied.len() {
        return Err(StoreError::Integrity("comparison ownership count"));
    }
    supplied.sort_unstable_by_key(|(id, _)| *id);
    if supplied.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(StoreError::Integrity("duplicate comparison operand"));
    }
    let canonical_for = |id: ObjectId| {
        supplied
            .binary_search_by_key(&id, |(id, _)| *id)
            .ok()
            .map(|index| supplied[index].1)
    };
    let mut ordinary = Vec::with_capacity(known.len());
    for (id, location) in known {
        let canonical =
            canonical_for(*id).ok_or(StoreError::Integrity("unexpected membership result"))?;
        if canonical.len() != location.canonical_length {
            return Err(StoreError::Integrity("object length collision"));
        }
        if location.canonical_length > pack::GROUP_LIMIT {
            db.compare_singleton(*id, *location, canonical)?;
        } else {
            ordinary.push((*id, *location));
        }
        metrics.skipped_ids += 1;
        metrics.skipped_bytes += canonical.len() as u64;
    }
    if !ordinary.is_empty() {
        // Retained locator/map/slot ownership is charged for the whole lookup;
        // each active wave retains its own conservative association charge too.
        // The other 1 MiB remains reserved for active decoding/reconstruction.
        let retained = retained_physical
            .checked_add(supplied.capacity() * std::mem::size_of::<(ObjectId, &[u8])>())
            .and_then(|n| n.checked_add(known.len() * 512))
            .ok_or(StoreError::Integrity("comparison ownership overflow"))?;
        let reserve = read::VALIDATION_RESERVE
            .checked_sub(retained)
            .ok_or(StoreError::Integrity("comparison physical reservation"))?;
        db.visit_locations_with_reserve(&mut ordinary, reserve, |object| {
            if canonical_for(object.id) != Some(object.bytes.as_slice()) {
                return Err(StoreError::Integrity("object collision"));
            }
            Ok(())
        })?;
    }
    metrics.collision_checks += known.len() as u64;
    metrics.conflict_read_rows += known.len() as u64;
    metrics.conflict_read_bytes += known
        .values()
        .map(|location| location.canonical_length as u64)
        .sum::<u64>();
    metrics.conflict_read_ns += super::elapsed_ns(started);
    Ok(())
}

#[cfg(test)]
mod native_tests;

#[path = "admission/metadata_values.rs"]
mod metadata_values;
use metadata_values::{prepare_values, PreparedValueGroup};
