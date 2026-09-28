use super::{
    compaction,
    extents::{Extent, ExtentKind, ExtentPlan},
    hot_cursor::Seed,
    index::{Index, IndexSnapshot, ScanPage},
    pack::{PackedSlot, TinyPack},
    page::PageRef,
    pages::{PageStore, StoreStatus},
    reclaim,
    records::{dirty_key, inode_key, HotInode},
    retirement::Retirement,
};
use crate::{
    backing::{
        budget::Charge,
        directory::Directory,
        metadata::MetadataHost,
        payload::{OwnedPayload, PayloadHost},
    },
    NodeKind, PortableAttributes, WorkspaceError, MAX_READ_BYTES,
};
use layerfs_bridge::contract::Root;
use std::{
    collections::BTreeMap,
    mem::size_of,
    sync::{Arc, Mutex},
};

pub(super) struct State {
    pub(super) retired: Retirement,
    pub(super) large: BTreeMap<u64, LargeOwner>,
    pub(super) retired_large: BTreeMap<u64, u64>,
    pub(super) retired_large_charge: Charge,
    pub(super) stopped: bool,
    pub(super) closed: bool,
}

pub(super) struct LargeOwner {
    payload: OwnedPayload,
    pub(super) birth: u64,
    _charge: Charge,
}
type KeyUpdates = Vec<(Vec<u8>, Option<Vec<u8>>)>;

/// Coordinates one Workspace's pack tail and pooled index. The index root is
/// the publication point; a prepared pack page is verified before it is named.
pub struct ActiveBacking {
    pub(super) store: Arc<PageStore>,
    pub(super) payloads: Arc<PayloadHost>,
    pub(super) pack: Arc<TinyPack>,
    pub(super) index: Arc<Index>,
    pub(super) state: Mutex<State>,
    _charge: Charge,
}

pub struct ActiveSnapshot {
    active: Arc<ActiveBacking>,
    index: Option<IndexSnapshot>,
    pub generation: u64,
    pub tail: Option<(u64, PageRef)>,
    read_view: bool,
}

#[derive(Clone, Debug)]
pub struct ActiveWrite {
    pub slot: PackedSlot,
    pub length: u64,
    pub revision: u64,
    /// Publication succeeded; cleanup failure remains owned and stops further
    /// admission until the caller handles it. It cannot change accepted bytes.
    pub cleanup_error: Option<WorkspaceError>,
    pub inode: Option<HotInode>,
}

#[derive(Clone, Debug)]
pub struct ActivePublication {
    pub length: u64,
    pub revision: u64,
    pub cleanup_error: Option<WorkspaceError>,
    pub inode: Option<HotInode>,
}

#[derive(Clone, Copy, Debug)]
pub struct ActiveStatus {
    pub store: StoreStatus,
    pub retired_pack_pages: usize,
    pub retired_payloads: usize,
    pub stopped: bool,
    pub closed: bool,
    pub hot_nodes: usize,
    pub hot_cursors: usize,
    pub hot_reserved_bytes: usize,
    pub retired_index_pages: usize,
}

pub(super) fn locator_key(logical: u64) -> Vec<u8> {
    [vec![b'P'], logical.to_be_bytes().to_vec()].concat()
}

pub(super) fn locator_value(physical: PageRef) -> Vec<u8> {
    [physical.id.to_be_bytes(), physical.epoch.to_be_bytes()].concat()
}

pub(super) fn parse_locator(bytes: &[u8]) -> Result<PageRef, WorkspaceError> {
    if bytes.len() != 16 {
        return Err(WorkspaceError::Io);
    }
    let id = u64::from_be_bytes(bytes[..8].try_into().map_err(|_| WorkspaceError::Io)?);
    let epoch = u64::from_be_bytes(bytes[8..].try_into().map_err(|_| WorkspaceError::Io)?);
    if id == 0 || epoch == 0 {
        return Err(WorkspaceError::Io);
    }
    Ok(PageRef { id, epoch })
}

impl ActiveBacking {
    pub fn new(
        directory: Arc<Directory>,
        host: Arc<MetadataHost>,
    ) -> Result<Arc<Self>, WorkspaceError> {
        let payloads = host.payloads.clone();
        let store = PageStore::new(directory, host)?;
        let budget = store.budget();
        let pack = TinyPack::new(store.clone())?;
        let index = Index::new(store.clone())?;
        Ok(Arc::new(Self {
            store,
            payloads,
            pack,
            index,
            state: Mutex::new(State {
                retired: Retirement::new(&budget)?,
                large: BTreeMap::new(),
                retired_large: BTreeMap::new(),
                retired_large_charge: budget.reserve(0)?,
                stopped: false,
                closed: false,
            }),
            _charge: budget.reserve(1024)?,
        }))
    }

    pub(crate) fn source_counts(&self) -> (u64, u64) {
        self.store.source_counts()
    }

    pub fn status(&self) -> Result<ActiveStatus, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let (hot_nodes, hot_cursors, hot_reserved_bytes, retired_index_pages) =
            self.index.hot_status()?;
        Ok(ActiveStatus {
            store: self.store.status()?,
            retired_pack_pages: state.retired.len(),
            retired_payloads: state.retired_large.len(),
            stopped: state.stopped,
            closed: state.closed,
            hot_nodes,
            hot_cursors,
            hot_reserved_bytes,
            retired_index_pages,
        })
    }

    pub fn generation_revision(&self) -> Result<(u64, u64), WorkspaceError> {
        self.index.generation_revision()
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, WorkspaceError> {
        let _state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        self.index.get(key)
    }

    pub fn scan(
        &self,
        lower: &[u8],
        upper: &[u8],
        limit: usize,
    ) -> Result<ScanPage, WorkspaceError> {
        let _state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        self.index.scan(lower, upper, limit)
    }

    /// Publishes namespace, inode and dirty records without a pack append.
    /// The caller supplies one complete mutation's sorted, unique key changes.
    pub fn publish_records(
        &self,
        updates: &[(Vec<u8>, Option<Vec<u8>>)],
    ) -> Result<u64, WorkspaceError> {
        if updates.is_empty() {
            return Err(WorkspaceError::InvalidInput);
        }
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.stopped || state.closed {
            return Err(WorkspaceError::Busy);
        }
        self.index.prepare(updates)?.publish()
    }

    /// Reads one slot from the selected generation's locator root.
    pub fn read(
        &self,
        slot: PackedSlot,
        snapshot: Option<&ActiveSnapshot>,
    ) -> Result<Vec<u8>, WorkspaceError> {
        let _state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let locator = match snapshot {
            Some(snapshot) => snapshot
                .index
                .as_ref()
                .ok_or(WorkspaceError::Closed)?
                .get(&locator_key(slot.logical_page))?,
            None => self.index.get(&locator_key(slot.logical_page))?,
        }
        .ok_or(WorkspaceError::NotFound)?;
        self.pack.read(slot, parse_locator(&locator)?)
    }

    /// Reads one indexed file revision. Packed bytes are copied while the
    /// current root is locked; Base and owned-payload reads run afterward.
    pub fn read_file(
        &self,
        inode: u64,
        offset: u64,
        output: &mut [u8],
        snapshot: Option<&ActiveSnapshot>,
        mut read_base: impl FnMut(Root, u64, &mut [u8]) -> Result<(), WorkspaceError>,
        mut read_payload: impl FnMut(&OwnedPayload, u64, &mut [u8]) -> Result<(), WorkspaceError>,
    ) -> Result<usize, WorkspaceError> {
        if output.len() > MAX_READ_BYTES
            || snapshot.is_some_and(|view| !std::ptr::eq(Arc::as_ptr(&view.active), self))
        {
            return Err(WorkspaceError::InvalidInput);
        }
        let mut spans = Vec::<(usize, u64, usize, Option<OwnedPayload>)>::new();
        let mut spans_charge = self.store.budget().reserve(0)?;
        let _slot_charge = self.store.budget().reserve(128)?;
        let (selected, length) = {
            let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            let get = |key: &[u8]| match snapshot {
                Some(view) => view.get(key),
                None => self.index.get(key),
            };
            let selected =
                HotInode::parse(&get(&inode_key(inode))?.ok_or(WorkspaceError::NotFound)?)?;
            if !matches!(selected.kind, NodeKind::File | NodeKind::Symlink) {
                return Err(WorkspaceError::WrongKind);
            }
            let length = selected
                .length
                .saturating_sub(offset)
                .min(output.len() as u64) as usize;
            let mut completed = 0;
            while completed < length {
                let position = offset + completed as u64;
                let extent = if selected.storage == 1 {
                    selected
                        .inline
                        .into_iter()
                        .flatten()
                        .find(|extent| extent.start <= position && position < extent.end)
                } else if selected.storage == 2 {
                    let key = Extent::key(inode, position);
                    let found = match snapshot {
                        Some(view) => view
                            .index
                            .as_ref()
                            .ok_or(WorkspaceError::Closed)?
                            .floor(&key)?,
                        None => self.index.floor(&key)?,
                    };
                    found
                        .filter(|(key, _)| key.starts_with(&key[..9]))
                        .map(|(key, value)| Extent::parse(&key, &value, inode))
                        .transpose()?
                } else {
                    None
                }
                .filter(|extent| extent.start <= position && position < extent.end)
                .ok_or(WorkspaceError::Io)?;
                let count = (extent.end - position).min((length - completed) as u64) as usize;
                let source = extent.source_offset + (position - extent.start);
                match extent.kind {
                    ExtentKind::Zero => output[completed..completed + count].fill(0),
                    ExtentKind::Base | ExtentKind::Payload => {
                        let owner = if extent.kind == ExtentKind::Payload {
                            let payload = &state
                                .large
                                .get(&extent.logical_page)
                                .ok_or(WorkspaceError::Io)?
                                .payload;
                            if payload.len() != extent.generation {
                                return Err(WorkspaceError::Io);
                            }
                            Some(payload.clone())
                        } else {
                            None
                        };
                        if spans.len() == spans.capacity() {
                            let next = spans.capacity().max(4).saturating_mul(2);
                            spans_charge.resize(
                                next * size_of::<(usize, u64, usize, Option<OwnedPayload>)>(),
                            )?;
                            spans
                                .try_reserve_exact(next - spans.capacity())
                                .map_err(|_| WorkspaceError::Capacity)?;
                            spans_charge.resize(
                                spans.capacity()
                                    * size_of::<(usize, u64, usize, Option<OwnedPayload>)>(),
                            )?;
                        }
                        spans.push((completed, source, count, owner));
                    }
                    ExtentKind::Packed => {
                        let slot = PackedSlot {
                            logical_page: extent.logical_page,
                            ordinal: extent.ordinal,
                            inode,
                            generation: extent.generation,
                            revision: extent.revision,
                            offset: extent
                                .start
                                .checked_sub(extent.source_offset)
                                .ok_or(WorkspaceError::Io)?,
                            length: extent.slot_length,
                        };
                        let locator =
                            get(&locator_key(slot.logical_page))?.ok_or(WorkspaceError::Io)?;
                        let data = self.pack.read(slot, parse_locator(&locator)?)?;
                        let at = source as usize;
                        output[completed..completed + count]
                            .copy_from_slice(data.get(at..at + count).ok_or(WorkspaceError::Io)?);
                    }
                }
                completed += count;
            }
            (selected, length)
        };
        for (start, source, count, owner) in spans {
            if let Some(payload) = owner {
                read_payload(&payload, source, &mut output[start..start + count])?;
            } else {
                read_base(selected.base, source, &mut output[start..start + count])?;
            }
        }
        Ok(length)
    }

    /// One tiny data and index publication. Extra keyed updates supply inode,
    /// attribute, namespace and dirty records from the Workspace mutation.
    pub fn write_tiny(
        &self,
        inode: u64,
        old_length: u64,
        offset: u64,
        data: &[u8],
        extra: &[(Vec<u8>, Option<Vec<u8>>)],
    ) -> Result<ActiveWrite, WorkspaceError> {
        self.write_with(
            inode,
            old_length,
            offset,
            data,
            |_, _, _| Ok(extra.to_vec()),
        )
    }

    /// Publishes a tiny file WRITE with its inode attributes and dirty key in
    /// the same index revision as the extent and pack locator.
    pub fn write_tiny_file(
        &self,
        inode: u64,
        original: HotInode,
        offset: u64,
        data: &[u8],
    ) -> Result<ActiveWrite, WorkspaceError> {
        if let Some(write) = self.try_hot_file(inode, original, offset, data)? {
            return Ok(write);
        }
        self.write_with(
            inode,
            original.length,
            offset,
            data,
            |generation, revision, length| {
                self.inode_updates(inode, original, generation, revision, length)
            },
        )
    }

    fn inode_updates(
        &self,
        inode: u64,
        original: HotInode,
        generation: u64,
        revision: u64,
        length: u64,
    ) -> Result<KeyUpdates, WorkspaceError> {
        let key = inode_key(inode);
        let selected = self.index.file_facts(inode)?.0.unwrap_or(original);
        if !matches!(selected.kind, NodeKind::File | NodeKind::Symlink)
            || selected.length != original.length
        {
            return Err(WorkspaceError::InvalidInput);
        }
        let selected = super::hot_path::updated(selected, generation, revision, length)?;
        Ok(vec![
            (key.to_vec(), Some(selected.value()?.to_vec())),
            (dirty_key(generation, inode).to_vec(), Some(vec![1])),
        ])
    }

    /// Publishes one large owned payload through the same active inode and
    /// dirty index used by tiny writes. The `p-*` owner is retained with the
    /// active view, without constructing a legacy keyed root.
    pub fn write_payload_file(
        &self,
        inode: u64,
        original: HotInode,
        offset: u64,
        payload: &OwnedPayload,
    ) -> Result<ActivePublication, WorkspaceError> {
        self.write_payload_with(inode, original, offset, payload, &[])
    }

    /// Publishes a symlink target with its namespace records as one revision.
    pub fn create_symlink(
        &self,
        inode: u64,
        original: HotInode,
        payload: &OwnedPayload,
        extra: &[(Vec<u8>, Option<Vec<u8>>)],
    ) -> Result<ActivePublication, WorkspaceError> {
        if original.kind != NodeKind::Symlink || original.length != 0 {
            return Err(WorkspaceError::InvalidInput);
        }
        self.write_payload_with(inode, original, 0, payload, extra)
    }

    fn write_payload_with(
        &self,
        inode: u64,
        original: HotInode,
        offset: u64,
        payload: &OwnedPayload,
        extra_updates: &[(Vec<u8>, Option<Vec<u8>>)],
    ) -> Result<ActivePublication, WorkspaceError> {
        if !Arc::ptr_eq(&payload.host, &self.payloads)
            || payload.record.directory.incarnation != self.store.incarnation()
            || payload.is_empty()
            || payload.len() > 8 * 1024 * 1024
        {
            return Err(WorkspaceError::InvalidInput);
        }
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.stopped || state.closed {
            return Err(WorkspaceError::Busy);
        }
        if state
            .large
            .get(&payload.record.id)
            .is_some_and(|owner| !Arc::ptr_eq(&owner.payload.record, &payload.record))
        {
            return Err(WorkspaceError::InvalidInput);
        }
        let _working = self.store.budget().reserve(256 * 1024)?;
        self.index.maintain()?;
        let (generation, prior_revision) = self.index.generation_revision()?;
        let revision = prior_revision
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        let planned = ExtentPlan::payload(
            &self.index,
            inode,
            original.length,
            offset,
            payload.record.id,
            payload.len(),
        )?;
        let mut extra =
            self.inode_updates(inode, original, generation, revision, planned.length)?;
        extra.extend_from_slice(extra_updates);
        let new_owner = (!state.large.contains_key(&payload.record.id))
            .then(|| {
                Ok::<_, WorkspaceError>(LargeOwner {
                    payload: payload.clone(),
                    birth: revision,
                    _charge: self.store.budget().reserve(128)?,
                })
            })
            .transpose()?;
        self.publish_no_pack(
            &mut state,
            planned,
            extra,
            new_owner,
            Some(payload.record.id),
        )
    }

    /// Publishes a length change and its final Zero/retained extent view.
    pub fn resize_file(
        &self,
        inode: u64,
        original: HotInode,
        new_length: u64,
    ) -> Result<ActivePublication, WorkspaceError> {
        self.set_attributes_file(
            inode,
            original,
            PortableAttributes {
                size: Some(new_length),
                ..PortableAttributes::default()
            },
        )
    }

    /// Publishes one portable metadata change, materializing a single inline
    /// Base extent for an inherited file first touched only by attributes.
    pub fn set_attributes_file(
        &self,
        inode: u64,
        original: HotInode,
        request: PortableAttributes,
    ) -> Result<ActivePublication, WorkspaceError> {
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.stopped || state.closed {
            return Err(WorkspaceError::Busy);
        }
        let _working = self.store.budget().reserve(256 * 1024)?;
        self.index.maintain()?;
        let (generation, prior_revision) = self.index.generation_revision()?;
        let revision = prior_revision
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        let key = inode_key(inode);
        let stored = self.index.get(&key)?;
        let mut selected = match &stored {
            Some(value) => HotInode::parse(value)?,
            None => original,
        };
        if selected.kind != original.kind || selected.length != original.length {
            return Err(WorkspaceError::InvalidInput);
        }
        request.check(selected.kind, selected.length)?;
        let length = request.size.unwrap_or(selected.length);
        let planned = ExtentPlan::resize(&self.index, inode, selected.length, length)?;
        if length != selected.length {
            selected.storage = 2;
            selected.inline = [None; 4];
        } else if stored.is_none() && selected.kind == NodeKind::File && length > 0 {
            selected.storage = 1;
            selected.inline = [Some(Extent::base(0, length)), None, None, None];
        }
        selected.length = length;
        selected.generation = generation;
        selected.revision = revision;
        selected.mode = request.mode.unwrap_or(selected.mode);
        (selected.seconds, selected.nanos) =
            request.mtime.unwrap_or((selected.seconds, selected.nanos));
        let extra = vec![
            (key.to_vec(), Some(selected.value()?.to_vec())),
            (dirty_key(generation, inode).to_vec(), Some(vec![1])),
        ];
        self.publish_no_pack(&mut state, planned, extra, None, None)
    }

    fn publish_no_pack(
        &self,
        state: &mut State,
        planned: ExtentPlan,
        extra: KeyUpdates,
        new_owner: Option<LargeOwner>,
        live_payload: Option<u64>,
    ) -> Result<ActivePublication, WorkspaceError> {
        let length = planned.length;
        let inode = extra
            .iter()
            .find(|(key, _)| key.first() == Some(&b'I'))
            .and_then(|(_, value)| value.as_deref())
            .map(HotInode::parse)
            .transpose()?;
        let mut updates: BTreeMap<Vec<u8>, Option<Vec<u8>>> = planned.updates.into_iter().collect();
        for (key, value) in extra {
            if updates.insert(key, value).is_some() {
                return Err(WorkspaceError::InvalidInput);
            }
        }
        let tail = self.pack.tail()?;
        let retained_tail = match tail {
            Some((logical, physical)) if !self.pack.sealed(logical, physical)? => Some(logical),
            _ => None,
        };
        let (dead, _dead_charge) = reclaim::prune_dead(&self.index, &mut updates, retained_tail)?;
        let (dead_large, _dead_large_charge) = reclaim::prune_dead_payloads(&self.index, &updates)?;
        self.reserve_retired_large(state, dead_large.len())?;
        let (generation, prior_revision) = self.index.generation_revision()?;
        let revision = prior_revision
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        let plan = compaction::plan(
            self.store.clone(),
            &self.pack,
            &self.index,
            &mut updates,
            generation,
            revision,
            1,
        )?;
        let update_vec: Vec<_> = updates
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        if let Err(error) = state.retired.reserve(dead.len() + plan.retired_len()) {
            return Err(plan.abort().err().unwrap_or(error));
        }
        let candidate = match self.index.prepare(&update_vec) {
            Ok(candidate) => candidate,
            Err(error) => return Err(plan.abort().err().unwrap_or(error)),
        };
        let published_revision = match candidate.publish() {
            Ok(revision) => revision,
            Err(error) => return Err(plan.abort().err().unwrap_or(error)),
        };
        let compacted = plan.finish();
        if let Some(owner) = new_owner {
            state.large.insert(owner.payload.record.id, owner);
        }
        Self::record_retired_large(state, live_payload, dead_large, published_revision);
        let mut cleanup_error = self.index.maintain().err();
        for (logical, page) in dead
            .into_iter()
            .map(|page| {
                (
                    tail.map_or(
                        0,
                        |(logical, current)| if current == page { logical } else { 0 },
                    ),
                    page,
                )
            })
            .chain(compacted)
        {
            if tail.is_some_and(|(_, current)| current == page) {
                if let Err(error) = self.pack.forget_if(logical, page) {
                    cleanup_error.get_or_insert(error);
                }
            }
            if let Err(error) =
                state
                    .retired
                    .retire(&self.store, page, published_revision, |birth, retire| {
                        self.index.selecting_revision(birth, retire)
                    })
            {
                cleanup_error.get_or_insert(error);
            }
        }
        if cleanup_error.is_some() {
            state.stopped = true;
        }
        Ok(ActivePublication {
            length,
            revision: published_revision,
            cleanup_error,
            inode,
        })
    }

    fn write_with(
        &self,
        inode: u64,
        old_length: u64,
        offset: u64,
        data: &[u8],
        extra: impl FnOnce(u64, u64, u64) -> Result<KeyUpdates, WorkspaceError>,
    ) -> Result<ActiveWrite, WorkspaceError> {
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.stopped || state.closed {
            return Err(WorkspaceError::Busy);
        }
        let _working = self.store.budget().reserve(256 * 1024)?;
        self.index.maintain()?;
        let (generation, prior_revision) = self.index.generation_revision()?;
        let revision = prior_revision
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        let (slot, rewritten) =
            self.pack
                .preview(inode, generation, revision, offset, data.len())?;
        let planned = ExtentPlan::tiny(&self.index, inode, old_length, slot)?;
        let length = planned.length;
        let extra = extra(generation, revision, length)?;
        let mut updates: BTreeMap<Vec<u8>, Option<Vec<u8>>> = planned.updates.into_iter().collect();
        let logical = slot.logical_page;
        updates.insert(
            locator_key(logical),
            Some(locator_value(PageRef { id: 1, epoch: 1 })),
        );
        for (key, value) in extra {
            if updates.insert(key, value).is_some() {
                return Err(WorkspaceError::InvalidInput);
            }
        }
        let ordered: Vec<_> = updates
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let mut reservation = self.index.boundary_reservation(&ordered)?;
        state.retired.reserve(usize::from(rewritten.is_some()))?;
        let prepared = self.pack.prepare_from(
            inode,
            generation,
            revision,
            offset,
            data,
            reservation.as_mut(),
        )?;
        if prepared.slot() != slot {
            return Err(prepared.abort().err().unwrap_or(WorkspaceError::Io));
        }
        let (_, physical) = prepared.locator();
        updates.insert(locator_key(logical), Some(locator_value(physical)));
        let (dead, _dead_charge) = match reclaim::prune_dead(&self.index, &mut updates, None) {
            Ok(dead) => dead,
            Err(error) => return Err(prepared.abort().err().unwrap_or(error)),
        };
        let (dead_large, _dead_large_charge) =
            match reclaim::prune_dead_payloads(&self.index, &updates) {
                Ok(dead) => dead,
                Err(error) => return Err(prepared.abort().err().unwrap_or(error)),
            };
        if let Err(error) = self.reserve_retired_large(&mut state, dead_large.len()) {
            return Err(prepared.abort().err().unwrap_or(error));
        }
        let plan = match compaction::plan(
            self.store.clone(),
            &self.pack,
            &self.index,
            &mut updates,
            generation,
            revision,
            1,
        ) {
            Ok(plan) => plan,
            Err(error) => return Err(prepared.abort().err().unwrap_or(error)),
        };
        let updates: Vec<_> = updates.into_iter().collect();
        let published_inode = updates
            .iter()
            .find(|(key, _)| key.as_slice() == inode_key(inode))
            .and_then(|(_, value)| value.as_deref())
            .map(HotInode::parse)
            .transpose()?;
        let seed = published_inode.map(|selected| {
            let frontier = offset + data.len() as u64;
            let source = updates
                .iter()
                .filter_map(|(key, value)| {
                    value
                        .as_ref()
                        .and_then(|value| Extent::parse(key, value, inode).ok())
                })
                .find(|extent| {
                    extent.start == frontier
                        && matches!(extent.kind, ExtentKind::Base | ExtentKind::Zero)
                });
            Seed {
                inode,
                selected,
                frontier,
                source,
                locator: locator_key(logical),
                inverse: Extent::packed(offset, slot)
                    .expect("validated packed slot")
                    .inverse_key(inode)
                    .expect("packed inverse"),
            }
        });
        if let Err(error) = state.retired.reserve(
            dead.len() + plan.retired_len() + usize::from(prepared.retired_physical().is_some()),
        ) {
            let plan_error = plan.abort().err();
            let slot_error = prepared.abort().err();
            return Err(plan_error.or(slot_error).unwrap_or(error));
        }
        let candidate = match self.index.prepare_file(&updates, seed, None, reservation) {
            Ok(candidate) => candidate,
            Err(error) => {
                let plan_error = plan.abort().err();
                let slot_error = prepared.abort().err();
                return Err(plan_error.or(slot_error).unwrap_or(error));
            }
        };
        let published_revision = match candidate.publish() {
            Ok(revision) => revision,
            Err(error) => {
                let plan_error = plan.abort().err();
                let slot_error = prepared.abort().err();
                return Err(plan_error.or(slot_error).unwrap_or(error));
            }
        };
        let compacted = plan.finish();
        let rewritten = prepared.retired_physical();
        let mut cleanup_error = prepared.publish().err();
        if let Err(error) = self.index.maintain() {
            cleanup_error.get_or_insert(error);
        }
        Self::record_retired_large(&mut state, None, dead_large, published_revision);
        for page in rewritten
            .into_iter()
            .chain(dead.into_iter())
            .chain(compacted.into_iter().map(|(_, page)| page))
        {
            if let Err(error) =
                state
                    .retired
                    .retire(&self.store, page, published_revision, |birth, retire| {
                        self.index.selecting_revision(birth, retire)
                    })
            {
                cleanup_error.get_or_insert(error);
            }
        }
        if cleanup_error.is_some() {
            state.stopped = true;
        }
        Ok(ActiveWrite {
            slot,
            length,
            revision: published_revision,
            cleanup_error,
            inode: published_inode,
        })
    }

    pub fn capture(self: &Arc<Self>) -> Result<ActiveSnapshot, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.stopped || state.closed {
            return Err(WorkspaceError::Busy);
        }
        let generation = self.index.generation_revision()?.0;
        let tail = self.pack.seal()?;
        let snapshot = self.index.capture()?;
        Ok(ActiveSnapshot {
            active: self.clone(),
            index: Some(snapshot),
            generation,
            tail,
            read_view: false,
        })
    }

    /// Pins a current read view without advancing the mutation generation.
    /// Directory handles can retain it across later namespace revisions.
    pub fn pin_view(self: &Arc<Self>) -> Result<ActiveSnapshot, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.closed {
            return Err(WorkspaceError::Busy);
        }
        let generation = self.index.generation_revision()?.0;
        let tail = self.pack.tail()?;
        let snapshot = self.index.pin_current()?;
        Ok(ActiveSnapshot {
            active: self.clone(),
            index: Some(snapshot),
            generation,
            tail,
            read_view: true,
        })
    }

    pub fn close_clean(&self) -> Result<(), WorkspaceError> {
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.closed {
            return Ok(());
        }
        if self.index.frozen_count()? > 0 {
            return Err(WorkspaceError::Busy);
        }
        if state.stopped {
            return Err(WorkspaceError::Io);
        }
        self.index.maintain()?;
        if state.retired.len() != 0 {
            return Err(WorkspaceError::Busy);
        }
        self.store.close()?;
        state.large.clear();
        state.retired_large.clear();
        state.retired_large_charge.resize(0)?;
        state.retired.clear()?;
        state.closed = true;
        Ok(())
    }
}

impl ActiveSnapshot {
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, WorkspaceError> {
        self.index.as_ref().ok_or(WorkspaceError::Closed)?.get(key)
    }

    pub fn scan(
        &self,
        lower: &[u8],
        upper: &[u8],
        limit: usize,
    ) -> Result<ScanPage, WorkspaceError> {
        self.index
            .as_ref()
            .ok_or(WorkspaceError::Closed)?
            .scan(lower, upper, limit)
    }

    /// The pinned revision this snapshot selected.
    pub fn revision(&self) -> Result<u64, WorkspaceError> {
        Ok(self
            .index
            .as_ref()
            .ok_or(WorkspaceError::Closed)?
            .revision())
    }

    fn release_inner(&mut self) -> Result<(), WorkspaceError> {
        let mut state = self.active.state.lock().map_err(|_| WorkspaceError::Io)?;
        let revision = self
            .index
            .as_ref()
            .ok_or(WorkspaceError::Closed)?
            .revision();
        let result = self
            .index
            .take()
            .ok_or(WorkspaceError::Closed)?
            .release()
            .and_then(|()| {
                if !self.active.index.pin_exists(revision)? {
                    state.retired.release_selector(
                        &self.active.store,
                        revision,
                        |birth, retire| self.active.index.selecting_revision(birth, retire),
                    )?;
                }
                Ok(())
            });
        if result.is_err() {
            state.stopped = true;
        }
        result
    }

    pub fn release(mut self) -> Result<(), WorkspaceError> {
        self.release_inner()
    }
}

impl Drop for ActiveSnapshot {
    fn drop(&mut self) {
        if self.index.is_some() {
            if self.read_view {
                let _ = self.release_inner();
            } else if let Ok(mut state) = self.active.state.lock() {
                state.stopped = true;
            }
        }
    }
}
