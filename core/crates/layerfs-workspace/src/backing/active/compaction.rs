//! Relocate surviving slots from mixed sealed pages into pooled new pages.
use super::{
    extents::{Extent, ExtentKind},
    generation::{locator_key, locator_value, parse_locator},
    index::Index,
    pack::{PackRecord, PackRecords, TinyPack},
    page::{Kind, PageRef, BODY_BYTES, PAGE_BYTES},
    pages::PageStore,
};
use crate::{backing::budget::Charge, WorkspaceError};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

struct LiveRef {
    key: Vec<u8>,
    ordinal: u16,
    inode: u64,
    start: u64,
}

struct Destination {
    logical: u64,
    body: Vec<u8>,
    records: u16,
    _charge: Charge,
}

impl Destination {
    fn new(pack: &TinyPack, store: &PageStore) -> Result<Self, WorkspaceError> {
        let charge = store.budget().reserve(BODY_BYTES)?;
        Ok(Self {
            logical: pack.reserve_logical()?,
            body: Vec::with_capacity(BODY_BYTES),
            records: 0,
            _charge: charge,
        })
    }

    fn append(&mut self, record: &PackRecord) -> Result<u16, WorkspaceError> {
        let length = record.bytes.len();
        if self.body.len() + 48 + length > BODY_BYTES || length != record.slot.length as usize {
            return Err(WorkspaceError::Capacity);
        }
        let ordinal = self.records;
        self.records = ordinal.checked_add(1).ok_or(WorkspaceError::Capacity)?;
        for value in [
            record.slot.inode,
            record.slot.generation,
            record.slot.revision,
            record.slot.offset,
            self.logical,
        ] {
            self.body.extend_from_slice(&value.to_be_bytes());
        }
        self.body
            .extend_from_slice(&record.slot.length.to_be_bytes());
        self.body.extend_from_slice(&ordinal.to_be_bytes());
        self.body.extend_from_slice(&[0; 4]);
        self.body.extend_from_slice(&record.bytes);
        Ok(ordinal)
    }
}

pub(super) struct CompactionPlan {
    store: Arc<PageStore>,
    new_pages: Vec<PageRef>,
    source_pages: Vec<(u64, PageRef)>,
    charge: Charge,
    updates_charge: Charge,
    updates_bytes: usize,
    attempted_pages: usize,
    finished: bool,
}

impl Drop for CompactionPlan {
    fn drop(&mut self) {
        if !self.finished {
            self.store.stop();
        }
    }
}

impl CompactionPlan {
    pub(super) fn retired_len(&self) -> usize {
        self.source_pages.len()
    }
    fn new(store: Arc<PageStore>) -> Result<Self, WorkspaceError> {
        let charge = store.budget().reserve(0)?;
        let updates_charge = store.budget().reserve(0)?;
        Ok(Self {
            store,
            new_pages: Vec::new(),
            source_pages: Vec::new(),
            charge,
            updates_charge,
            updates_bytes: 0,
            attempted_pages: 0,
            finished: false,
        })
    }

    fn reserve_updates(&mut self, bytes: usize) -> Result<(), WorkspaceError> {
        self.updates_bytes = self
            .updates_bytes
            .checked_add(bytes)
            .ok_or(WorkspaceError::Capacity)?;
        self.updates_charge.resize(self.updates_bytes)
    }

    pub(super) fn abort(mut self) -> Result<(), WorkspaceError> {
        for page in self.new_pages.iter().rev() {
            if let Err(error) = self.store.release(*page) {
                self.store.stop();
                return Err(error);
            }
        }
        self.finished = true;
        Ok(())
    }

    pub(super) fn finish(mut self) -> Vec<(u64, PageRef)> {
        self.finished = true;
        std::mem::take(&mut self.source_pages)
    }

    fn source(&mut self, logical: u64, page: PageRef) -> Result<(), WorkspaceError> {
        self.charge
            .resize((self.new_pages.len() + self.source_pages.len() + 1) * 32)?;
        self.source_pages
            .try_reserve_exact(1)
            .map_err(|_| WorkspaceError::Capacity)?;
        self.source_pages.push((logical, page));
        Ok(())
    }

    fn finish_page(
        &mut self,
        dest: Destination,
        generation: u64,
        revision: u64,
        updates: &mut BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    ) -> Result<(), WorkspaceError> {
        self.charge
            .resize((self.new_pages.len() + self.source_pages.len() + 1) * 32)?;
        self.new_pages
            .try_reserve_exact(1)
            .map_err(|_| WorkspaceError::Capacity)?;
        self.attempted_pages = self
            .attempted_pages
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        let physical =
            self.store
                .create(Kind::Pack, generation, revision, dest.records, &dest.body)?;
        self.new_pages.push(physical);
        self.reserve_updates(256)?;
        if updates
            .insert(locator_key(dest.logical), Some(locator_value(physical)))
            .is_some()
        {
            return Err(WorkspaceError::Io);
        }
        Ok(())
    }
}

fn live_refs(
    index: &Index,
    updates: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    logical: u64,
) -> Result<(Vec<LiveRef>, Charge), WorkspaceError> {
    let mut lower = [vec![b'R'], logical.to_be_bytes().to_vec()].concat();
    let upper = if logical == u64::MAX {
        vec![b'S']
    } else {
        [vec![b'R'], (logical + 1).to_be_bytes().to_vec()].concat()
    };
    let mut refs = Vec::new();
    let mut charge = index.budget().reserve(0)?;
    loop {
        let page = index.scan(&lower, &upper, 128)?;
        for (key, value) in page.entries() {
            if key.len() != 27 || value != &[1] {
                return Err(WorkspaceError::Io);
            }
            if updates.get(key).is_some_and(Option::is_none) {
                continue;
            }
            charge.resize((refs.len() + 1) * 128)?;
            refs.try_reserve_exact(1)
                .map_err(|_| WorkspaceError::Capacity)?;
            refs.push(LiveRef {
                key: key.clone(),
                ordinal: u16::from_be_bytes(key[9..11].try_into().map_err(|_| WorkspaceError::Io)?),
                inode: u64::from_be_bytes(key[11..19].try_into().map_err(|_| WorkspaceError::Io)?),
                start: u64::from_be_bytes(key[19..27].try_into().map_err(|_| WorkspaceError::Io)?),
            });
        }
        let Some((last, _)) = page.entries().last() else {
            break;
        };
        lower = last.clone();
        lower.push(0);
    }
    refs.sort_unstable_by_key(|reference| reference.ordinal);
    Ok((refs, charge))
}

fn live_bytes(records: &PackRecords, refs: &[LiveRef]) -> usize {
    records
        .records
        .iter()
        .filter(|record| {
            refs.binary_search_by_key(&record.slot.ordinal, |reference| reference.ordinal)
                .is_ok()
        })
        .map(|record| 48 + record.bytes.len())
        .sum()
}

fn relocate_reference(
    index: &Index,
    updates: &mut BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    reference: &LiveRef,
    old_logical: u64,
    new_logical: u64,
    new_ordinal: u16,
) -> Result<(), WorkspaceError> {
    let key = Extent::key(reference.inode, reference.start).to_vec();
    let value = match updates.get(&key) {
        Some(Some(value)) => value.clone(),
        Some(None) => return Err(WorkspaceError::Io),
        None => index.get(&key)?.ok_or(WorkspaceError::Io)?,
    };
    let mut extent = Extent::parse(&key, &value, reference.inode)?;
    if extent.kind != ExtentKind::Packed
        || extent.logical_page != old_logical
        || extent.ordinal != reference.ordinal
    {
        return Err(WorkspaceError::Io);
    }
    extent.logical_page = new_logical;
    extent.ordinal = new_ordinal;
    updates.insert(key, Some(extent.value()?.to_vec()));
    updates.insert(reference.key.clone(), None);
    let inverse = extent
        .inverse_key(reference.inode)
        .ok_or(WorkspaceError::Io)?;
    if updates.insert(inverse, Some(vec![1])).is_some() {
        return Err(WorkspaceError::Io);
    }
    Ok(())
}

/// A mutation relocates at most one touched sealed source page; Commit pays the
/// remaining touched pages in the same publication as its frontier removal.
pub(super) fn plan(
    store: Arc<PageStore>,
    pack: &TinyPack,
    index: &Index,
    updates: &mut BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    generation: u64,
    revision: u64,
    max_sources: usize,
) -> Result<CompactionPlan, WorkspaceError> {
    let mut planned = CompactionPlan::new(store.clone())?;
    // One bounded per-plan causal record: the pressure scan is not a physical
    // reservation. Do not confuse these pack reads with SaveFile source loads.
    let mut branch = "unreached";
    let mut pressure_estimate = 0u64;
    let mut remaining = None;
    let mut touched_logicals = 0usize;
    let mut all_logicals = 0usize;
    let mut scan_calls = 0usize;
    let mut scanned_locators = 0usize;
    let mut pressure_pack_reads = 0usize;
    let mut source_pack_reads = 0usize;
    let mut partial_count = 0usize;
    let mut paired_count = 0usize;
    let budget_before = store.budget().used();
    let initial_updates = updates.len();
    let result = (|| -> Result<(), WorkspaceError> {
        let mut logicals = BTreeSet::new();
        let mut logicals_charge = store.budget().reserve(
            updates
                .len()
                .checked_mul(96)
                .ok_or(WorkspaceError::Capacity)?,
        )?;
        for (key, value) in updates.iter() {
            if key.len() == 27 && key[0] == b'R' && value.is_none() {
                logicals.insert(u64::from_be_bytes(
                    key[1..9].try_into().map_err(|_| WorkspaceError::Io)?,
                ));
            }
        }
        logicals_charge.resize(logicals.len() * 96)?;
        touched_logicals = logicals.len();
        // A Commit with little admission headroom can pool previously mixed
        // sealed pages as well as pages touched by this generation.
        let reserve = updates
            .len()
            .checked_add(32)
            .and_then(|pages| pages.checked_mul(PAGE_BYTES))
            .ok_or(WorkspaceError::Capacity)? as u64;
        pressure_estimate = reserve;
        remaining = if max_sources > 1 {
            Some(store.remaining_quota()?)
        } else {
            None
        };
        let pressure = remaining.is_some_and(|headroom| headroom < reserve);
        branch = if pressure { "pressure" } else { "touched_only" };
        let mut partial = Vec::new();
        let mut partial_charge = store.budget().reserve(0)?;
        if pressure {
            let mut lower = vec![b'P'];
            loop {
                scan_calls += 1;
                let page = index.scan(&lower, b"Q", 128)?;
                scanned_locators += page.entries().len();
                for (key, value) in page.entries() {
                    if key.len() != 9 {
                        return Err(WorkspaceError::Io);
                    }
                    let logical =
                        u64::from_be_bytes(key[1..9].try_into().map_err(|_| WorkspaceError::Io)?);
                    if !logicals.contains(&logical) {
                        logicals_charge.resize(
                            logicals
                                .len()
                                .checked_add(1)
                                .and_then(|count| count.checked_mul(96))
                                .ok_or(WorkspaceError::Capacity)?,
                        )?;
                        logicals.insert(logical);
                    }
                    if updates.get(key).is_some_and(Option::is_none) {
                        continue;
                    }
                    let physical = parse_locator(value)?;
                    if !pack.sealed(logical, physical)? {
                        continue;
                    }
                    pressure_pack_reads += 1;
                    let records = pack.records(logical, physical)?;
                    let (refs, _charge) = live_refs(index, updates, logical)?;
                    if refs.is_empty() {
                        continue;
                    }
                    let live = live_bytes(&records, &refs);
                    let dead = records.used.checked_sub(live).ok_or(WorkspaceError::Io)?;
                    if dead > 0 && dead <= BODY_BYTES / 2 {
                        partial_charge.resize(
                            partial
                                .len()
                                .checked_add(1)
                                .and_then(|count| count.checked_mul(112))
                                .ok_or(WorkspaceError::Capacity)?,
                        )?;
                        partial.push((logical, live));
                    }
                }
                let Some((last, _)) = page.entries().last() else {
                    break;
                };
                lower = last.clone();
                lower.push(0);
            }
        }
        partial_count = partial.len();
        partial.sort_unstable_by_key(|(_, bytes)| *bytes);
        let mut paired = BTreeSet::new();
        let _paired_charge = store.budget().reserve(
            partial
                .len()
                .checked_mul(96)
                .ok_or(WorkspaceError::Capacity)?,
        )?;
        let mut at = 0;
        while at + 1 < partial.len() {
            if partial[at].1 + partial[at + 1].1 > BODY_BYTES {
                break;
            }
            paired.insert(partial[at].0);
            paired.insert(partial[at + 1].0);
            at += 2;
        }
        paired_count = paired.len();
        all_logicals = logicals.len();
        let mut destination: Option<Destination> = None;
        for logical in logicals {
            if planned.source_pages.len() == max_sources {
                break;
            }
            let locator = locator_key(logical);
            if updates.get(&locator).is_some_and(Option::is_none) {
                continue;
            }
            let Some(value) = index.get(&locator)? else {
                continue;
            };
            let physical = parse_locator(&value)?;
            if !pack.sealed(logical, physical)? {
                continue;
            }
            source_pack_reads += 1;
            let records = pack.records(logical, physical)?;
            let (refs, _refs_charge) = live_refs(index, updates, logical)?;
            if refs.is_empty() {
                continue;
            }
            let live_bytes = live_bytes(&records, &refs);
            let dead_bytes = records
                .used
                .checked_sub(live_bytes)
                .ok_or(WorkspaceError::Io)?;
            if dead_bytes == 0 || (dead_bytes <= BODY_BYTES / 2 && !paired.contains(&logical)) {
                continue;
            }
            let mut at = 0;
            for record in &records.records {
                if refs
                    .binary_search_by_key(&record.slot.ordinal, |reference| reference.ordinal)
                    .is_err()
                {
                    continue;
                }
                if destination
                    .as_ref()
                    .is_none_or(|dest| dest.body.len() + 48 + record.bytes.len() > BODY_BYTES)
                {
                    if let Some(dest) = destination.take() {
                        planned.finish_page(dest, generation, revision, updates)?;
                    }
                    destination = Some(Destination::new(pack, &store)?);
                }
                let dest = destination.as_mut().ok_or(WorkspaceError::Io)?;
                let ordinal = dest.append(record)?;
                while at < refs.len() && refs[at].ordinal == record.slot.ordinal {
                    planned.reserve_updates(512)?;
                    relocate_reference(index, updates, &refs[at], logical, dest.logical, ordinal)?;
                    at += 1;
                }
            }
            if at != refs.len() {
                return Err(WorkspaceError::Io);
            }
            planned.reserve_updates(256)?;
            updates.insert(locator, None);
            planned.source(logical, physical)?;
        }
        if let Some(dest) = destination {
            planned.finish_page(dest, generation, revision, updates)?;
        }
        Ok(())
    })();
    if max_sources > 1
        && std::env::var_os("LFS_CAPACITY_DIAGNOSTIC").as_deref() == Some(std::ffi::OsStr::new("1"))
    {
        eprintln!(
            "LFS_C5_COMPACTION v=1 status={} branch={} initial_updates={} final_updates={} estimate={} remaining={} touched_logicals={} all_logicals={} scan_calls={} scanned_locators={} pressure_pack_reads={} source_pack_reads={} partial={} paired={} physical_pack_attempts={} physical_pack_created={} sources_selected={} budget_before={} budget_after={}",
            if result.is_ok() { "planned" } else { "aborted" }, branch,
            initial_updates, updates.len(), pressure_estimate,
            remaining.map_or("NA".to_owned(), |n| n.to_string()),
            touched_logicals, all_logicals, scan_calls, scanned_locators,
            pressure_pack_reads, source_pack_reads, partial_count, paired_count,
            planned.attempted_pages, planned.new_pages.len(), planned.source_pages.len(),
            budget_before, store.budget().used(),
        );
    }
    match result {
        Ok(()) => Ok(planned),
        Err(error) => Err(planned.abort().err().unwrap_or(error)),
    }
}
