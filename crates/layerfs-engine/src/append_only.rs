//! Phase 4B candidate: one append-only carrier with an authenticated commit marker.
//!
//! The index is deliberately smaller than the proposal's COW B-tree. Object IDs
//! are immutable, so a fixed bucket root plus immutable collision pages gives a
//! disk-backed lookup without rewriting old pages or retaining the full catalog.

#![allow(clippy::too_many_arguments)]

use crate::{DeltaRecord, EngineError, EngineResult, ObjectRecord, PutOutcome, RootId, RootRecord};
use fs2::FileExt;
use layerfs_core::{decode_object, validate_object_from, Object, ObjectId, ObjectKind};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Cursor, Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::os::unix::fs::FileExt as UnixFileExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

const MAGIC: [u8; 4] = *b"L4AO";
const VERSION: u8 = 1;
const ALIGNMENT: u64 = 8;
const HEADER_PREFIX_LEN: usize = 40;
const HEADER_LEN: usize = 72;
const CHECKSUM_LEN: usize = 32;
const BUCKETS: usize = 256;
const INDEX_PAGE_PAYLOAD_LEN: usize = 72;
const MARKER_DIGEST_LEN: usize = 32;
const MAX_INDEX_VISITS: u64 = 1_000_000;
const MAX_MARKER_VISITS: u64 = 1_000_000;
const MAX_CLOSURE_VISITS: u64 = 1_000_000;
const MAX_FRAME_PAYLOAD: u64 = 64 * 1024 * 1024;
const CARRIER_BUFFER_BYTES: usize = 64 * 1024;
const MARKER_FORMAT_ID: [u8; 16] = *b"LFS4B-CARRIER-V1";
const MARKER_PAYLOAD_LEN: usize = 241;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum FrameKind {
    Object = 1,
    IndexPage = 2,
    IndexRoot = 3,
    Delta = 4,
    Root = 5,
    CommitMarker = 6,
}

impl TryFrom<u8> for FrameKind {
    type Error = EngineError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Object),
            2 => Ok(Self::IndexPage),
            3 => Ok(Self::IndexRoot),
            4 => Ok(Self::Delta),
            5 => Ok(Self::Root),
            6 => Ok(Self::CommitMarker),
            _ => Err(EngineError::InvalidRecord("carrier frame kind")),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct FrameHeader {
    kind: FrameKind,
    payload_len: u64,
    generation: u64,
    previous_offset: u64,
    offset: u64,
    checksum: [u8; CHECKSUM_LEN],
}

impl FrameHeader {
    fn decode(bytes: &[u8; HEADER_LEN], offset: u64) -> EngineResult<Self> {
        if bytes[..4] != MAGIC || bytes[4] != VERSION || bytes[6] != 0 || bytes[7] != 0 {
            return Err(EngineError::InvalidRecord("carrier frame header"));
        }
        let kind = FrameKind::try_from(bytes[5])?;
        let payload_len = read_u64(&bytes[8..16])?;
        let generation = read_u64(&bytes[16..24])?;
        let previous_offset = read_u64(&bytes[24..32])?;
        let encoded_offset = read_u64(&bytes[32..40])?;
        if encoded_offset != offset || offset % ALIGNMENT != 0 {
            return Err(EngineError::InvalidRecord("carrier frame offset"));
        }
        if payload_len > MAX_FRAME_PAYLOAD {
            return Err(EngineError::InvalidRecord("carrier frame length"));
        }
        let mut checksum = [0_u8; CHECKSUM_LEN];
        checksum.copy_from_slice(&bytes[HEADER_PREFIX_LEN..]);
        Ok(Self {
            kind,
            payload_len,
            generation,
            previous_offset,
            offset,
            checksum,
        })
    }

    fn prefix(self) -> [u8; HEADER_PREFIX_LEN] {
        let mut bytes = [0_u8; HEADER_PREFIX_LEN];
        bytes[..4].copy_from_slice(&MAGIC);
        bytes[4] = VERSION;
        bytes[5] = self.kind as u8;
        bytes[8..16].copy_from_slice(&self.payload_len.to_be_bytes());
        bytes[16..24].copy_from_slice(&self.generation.to_be_bytes());
        bytes[24..32].copy_from_slice(&self.previous_offset.to_be_bytes());
        bytes[32..40].copy_from_slice(&self.offset.to_be_bytes());
        bytes
    }

    fn encode(self) -> [u8; HEADER_LEN] {
        let mut bytes = [0_u8; HEADER_LEN];
        bytes[..HEADER_PREFIX_LEN].copy_from_slice(&self.prefix());
        bytes[HEADER_PREFIX_LEN..].copy_from_slice(&self.checksum);
        bytes
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AppendOnlyCounters {
    pub captures_started: u64,
    pub captures_committed: u64,
    pub captures_abandoned: u64,
    pub frames_appended: u64,
    pub frames_scanned: u64,
    pub frames_recovered: u64,
    pub frame_bytes_appended: u64,
    pub carrier_read_calls: u64,
    pub carrier_write_calls: u64,
    pub carrier_bytes_read: u64,
    pub carrier_bytes_written: u64,
    pub carrier_append_ns: u64,
    pub carrier_flush_calls: u64,
    pub carrier_flush_failures: u64,
    pub carrier_flush_ns: u64,
    pub object_validated: u64,
    pub objects_created: u64,
    pub objects_reused: u64,
    pub object_bytes_read: u64,
    pub object_bytes_written: u64,
    pub object_frame_bytes_written: u64,
    pub index_frame_bytes_written: u64,
    pub root_frame_bytes_written: u64,
    pub delta_frame_bytes_written: u64,
    pub marker_frame_bytes_written: u64,
    pub object_auth_ns: u64,
    pub object_hash_ns: u64,
    pub object_hash_bytes: u64,
    pub object_validation_ns: u64,
    pub object_validation_bytes: u64,
    pub object_compare_ns: u64,
    pub object_compare_bytes: u64,
    pub range_bytes_requested: u64,
    pub range_bytes_returned: u64,
    pub index_lookups: u64,
    pub index_page_reads: u64,
    pub index_cache_hits: u64,
    pub index_cache_misses: u64,
    pub index_cache_evictions: u64,
    pub index_lookup_ns: u64,
    pub marker_attempts: u64,
    pub marker_sync_attempts: u64,
    pub marker_sync_successes: u64,
    pub marker_sync_failures: u64,
    pub marker_sync_ns: u64,
    pub markers_recovered: u64,
    pub reopen_scans: u64,
    pub residue_bytes: u64,
    pub logical_object_bytes: u64,
    pub logical_root_bytes: u64,
    pub logical_delta_bytes: u64,
    pub index_root_reads: u64,
    pub root_reads: u64,
    pub delta_reads: u64,
    pub marker_reads: u64,
    pub marker_capture_digest_ns: u64,
    pub marker_capture_digest_bytes: u64,
    pub recovery_torn_bytes: u64,
    pub recovery_malformed_bytes: u64,
    pub recovery_integrity_bytes: u64,
    pub writer_lock_wait_ns: u64,
    pub writer_lock_hold_ns: u64,
    pub closure_objects: u64,
    pub closure_references: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AppendOnlyObservation {
    pub carrier_bytes: u64,
    pub visible_end: u64,
    pub residue_bytes: u64,
    pub logical_object_bytes: u64,
    pub logical_root_bytes: u64,
    pub logical_delta_bytes: u64,
}

#[derive(Clone, Copy, Debug)]
struct VisibleMarker {
    offset: u64,
    end: u64,
    generation: u64,
    previous_marker: u64,
    parent: Option<ObjectId>,
    child: ObjectId,
    delta_id: ObjectId,
    delta_offset: u64,
    index_root_offset: u64,
    root_offset: u64,
    capture_start: u64,
    capture_bytes: u64,
    capture_digest: [u8; MARKER_DIGEST_LEN],
}

#[derive(Clone, Copy, Debug)]
struct IndexPage {
    bucket: u8,
    next: u64,
    id: ObjectId,
    object_offset: u64,
    kind: ObjectKind,
    canonical_len: u64,
    end: u64,
}

#[derive(Clone, Copy, Debug)]
struct ObjectLocator {
    offset: u64,
    kind: ObjectKind,
    canonical_len: u64,
}

struct PageCache {
    capacity: usize,
    pages: VecDeque<(u64, IndexPage)>,
}

impl PageCache {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            pages: VecDeque::new(),
        }
    }

    fn get(&mut self, offset: u64) -> Option<IndexPage> {
        let position = self.pages.iter().position(|(key, _)| *key == offset)?;
        let entry = self.pages.remove(position)?;
        let page = entry.1;
        self.pages.push_back((offset, page));
        Some(page)
    }

    fn insert(&mut self, offset: u64, page: IndexPage) -> bool {
        if self.capacity == 0 {
            return false;
        }
        if let Some(position) = self.pages.iter().position(|(key, _)| *key == offset) {
            let _ = self.pages.remove(position);
        }
        let evicted = self.pages.len() == self.capacity;
        if evicted {
            let _ = self.pages.pop_front();
        }
        self.pages.push_back((offset, page));
        evicted
    }

    fn would_evict_unpersisted(&self, persisted_end: u64) -> bool {
        self.capacity != 0
            && self.pages.len() == self.capacity
            && self
                .pages
                .front()
                .is_some_and(|(_, page)| page.end > persisted_end)
    }
}

struct AppendState {
    file: BufWriter<File>,
    visible: Option<VisibleMarker>,
    last_valid_offset: Option<u64>,
    physical_end: u64,
    persisted_end: u64,
    counters: AppendOnlyCounters,
    cache: PageCache,
    poisoned: bool,
    capture_digest: Option<blake3::Hasher>,
}

pub struct AppendOnlyEngine {
    path: PathBuf,
    state: Mutex<AppendState>,
}

impl AppendOnlyEngine {
    pub fn open(path: impl AsRef<Path>) -> EngineResult<Self> {
        let path = path.as_ref().to_owned();
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(carrier_io)?;
        let lock_start = Instant::now();
        file.try_lock_exclusive().map_err(carrier_lock)?;
        let lock_wait_ns = u64::try_from(lock_start.elapsed().as_nanos())
            .map_err(|_| EngineError::CounterOverflow)?;
        let scan = scan_log(&file)?;
        file.seek(SeekFrom::Start(scan.physical_end))
            .map_err(carrier_io)?;
        let file = BufWriter::with_capacity(CARRIER_BUFFER_BYTES, file);
        let mut counters = scan.counters;
        bump(&mut counters.writer_lock_wait_ns, lock_wait_ns)?;
        bump(&mut counters.reopen_scans, 1)?;
        counters.residue_bytes = scan.residue_bytes;
        Ok(Self {
            path,
            state: Mutex::new(AppendState {
                file,
                visible: scan.visible,
                last_valid_offset: scan.last_valid_offset,
                physical_end: scan.physical_end,
                persisted_end: scan.physical_end,
                counters,
                cache: PageCache::new(32),
                poisoned: scan.recovery_blocked,
                capture_digest: None,
            }),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn counters(&self) -> EngineResult<AppendOnlyCounters> {
        let state = self.lock_state()?;
        Ok(state.counters)
    }

    pub fn reset_counters(&self) -> EngineResult<()> {
        self.lock_state()?.counters = AppendOnlyCounters::default();
        Ok(())
    }

    pub fn observations(&self) -> EngineResult<AppendOnlyObservation> {
        let state = self.lock_state()?;
        let visible_end = state.visible.map_or(0, |marker| marker.end);
        Ok(AppendOnlyObservation {
            carrier_bytes: state.physical_end,
            visible_end,
            residue_bytes: state.physical_end.saturating_sub(visible_end),
            logical_object_bytes: state.counters.logical_object_bytes,
            logical_root_bytes: state.counters.logical_root_bytes,
            logical_delta_bytes: state.counters.logical_delta_bytes,
        })
    }

    pub fn load_visible_root(&self) -> EngineResult<Option<RootId>> {
        let mut state = self.lock_state()?;
        let Some(marker) = state.visible else {
            return Ok(None);
        };
        validate_root_and_directory_state(
            &mut state,
            marker.root_offset,
            marker.child,
            marker.index_root_offset,
            marker.offset,
        )?;
        Ok(Some(marker.child))
    }

    pub fn load_root(&self, id: RootId) -> EngineResult<RootRecord> {
        let mut state = self.lock_state()?;
        let file = state.file.get_ref().try_clone().map_err(carrier_io)?;
        let marker = find_marker_for_root(&file, state.visible, id, &mut state.counters)?;
        let root = read_root(
            &file,
            marker.root_offset,
            marker.offset,
            &mut state.counters,
        )?;
        validate_root_and_directory_state(
            &mut state,
            marker.root_offset,
            marker.child,
            marker.index_root_offset,
            marker.offset,
        )?;
        Ok(root)
    }

    pub fn load_delta(&self, id: ObjectId) -> EngineResult<DeltaRecord> {
        let mut state = self.lock_state()?;
        let file = state.file.get_ref().try_clone().map_err(carrier_io)?;
        let marker = find_marker_for_delta(&file, state.visible, id, &mut state.counters)?;
        read_delta(
            &file,
            marker.delta_offset,
            marker.offset,
            &mut state.counters,
        )
    }

    pub fn object_length(&self, id: ObjectId) -> EngineResult<u64> {
        let mut state = self.lock_state()?;
        let locator =
            lookup_index_for_visible(&mut state, id)?.ok_or(EngineError::MissingObject(id))?;
        let visible_end = state
            .visible
            .map(|marker| marker.offset)
            .ok_or(EngineError::MissingObject(id))?;
        authenticate_object_state(&mut state, locator, id, None, visible_end)?;
        Ok(locator.canonical_len)
    }

    pub fn load_object(&self, id: ObjectId) -> EngineResult<ObjectRecord> {
        let length = self.object_length(id)?;
        let bytes = self.read_object_range(id, 0..length)?;
        ObjectRecord::new(id, bytes)
    }

    pub fn read_object_range(&self, id: ObjectId, range: Range<u64>) -> EngineResult<Vec<u8>> {
        let mut state = self.lock_state()?;
        let locator =
            lookup_index_for_visible(&mut state, id)?.ok_or(EngineError::MissingObject(id))?;
        if range.start > range.end || range.end > locator.canonical_len {
            return Err(EngineError::InvalidRange {
                start: range.start,
                end: range.end,
                length: locator.canonical_len,
            });
        }
        let visible_end = state
            .visible
            .map(|marker| marker.offset)
            .ok_or(EngineError::MissingObject(id))?;
        authenticate_object_state(&mut state, locator, id, None, visible_end)?;
        let length = range
            .end
            .checked_sub(range.start)
            .ok_or(EngineError::CounterOverflow)?;
        let output_len = usize::try_from(length).map_err(|_| EngineError::CounterOverflow)?;
        let object_start = object_payload_start(locator.offset)?;
        let start = object_start
            .checked_add(range.start)
            .ok_or(EngineError::CounterOverflow)?;
        let mut output = vec![0_u8; output_len];
        read_exact_state(&mut state, start, &mut output)?;
        bump(&mut state.counters.range_bytes_requested, length)?;
        bump(&mut state.counters.range_bytes_returned, length)?;
        Ok(output)
    }

    pub fn begin_capture(&self, parent: Option<RootId>) -> EngineResult<AppendOnlyCapture<'_>> {
        let mut state = self.lock_state()?;
        if state.poisoned {
            return Err(EngineError::EnginePoisoned);
        }
        let current = state.visible.map(|marker| marker.child);
        if current != parent {
            return Err(EngineError::ParentMismatch {
                expected: parent,
                actual: current,
            });
        }
        let index_heads = match state.visible {
            Some(marker) => read_index_root_state(&mut state, marker.index_root_offset)?,
            None => [0_u64; BUCKETS],
        };
        if let Some(marker) = state.visible {
            validate_root_and_directory_state(
                &mut state,
                marker.root_offset,
                marker.child,
                marker.index_root_offset,
                marker.offset,
            )?;
        }
        bump(&mut state.counters.captures_started, 1)?;
        let generation = match state.visible {
            Some(marker) => marker
                .generation
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?,
            None => 0,
        };
        let capture_start = state.physical_end;
        let previous_marker = state.visible.map_or(0, |marker| marker.offset);
        let mut capture_digest = blake3::Hasher::new();
        capture_digest.update(b"layerfs/phase4b-capture-v1\0");
        state.capture_digest = Some(capture_digest);
        Ok(AppendOnlyCapture {
            engine: self,
            state,
            parent,
            generation,
            capture_start,
            previous_marker,
            index_heads,
            delta: None,
            delta_offset: None,
            active: true,
            hold_start: Instant::now(),
            #[cfg(test)]
            fault: None,
        })
    }

    fn lock_state(&self) -> EngineResult<MutexGuard<'_, AppendState>> {
        let wait_start = Instant::now();
        let mut state = self
            .state
            .lock()
            .map_err(|_| EngineError::CarrierIo("carrier mutex poisoned".to_owned()))?;
        add_elapsed(&mut state.counters.writer_lock_wait_ns, wait_start)?;
        Ok(state)
    }
}

pub struct AppendOnlyCapture<'a> {
    engine: &'a AppendOnlyEngine,
    state: MutexGuard<'a, AppendState>,
    parent: Option<RootId>,
    generation: u64,
    capture_start: u64,
    previous_marker: u64,
    index_heads: [u64; BUCKETS],
    delta: Option<DeltaRecord>,
    delta_offset: Option<u64>,
    active: bool,
    hold_start: Instant,
    #[cfg(test)]
    fault: Option<AppendFaultPoint>,
}

impl AppendOnlyCapture<'_> {
    pub fn counters(&self) -> AppendOnlyCounters {
        self.state.counters
    }

    pub fn put_object_if_absent(
        &mut self,
        id: ObjectId,
        canonical_bytes: &[u8],
    ) -> EngineResult<PutOutcome> {
        let result = self.put_object_if_absent_inner(id, canonical_bytes, false);
        if result.is_err() {
            self.state.poisoned = true;
            self.active = false;
        }
        result
    }

    pub fn put_canonical_object_if_absent(
        &mut self,
        canonical_bytes: &[u8],
    ) -> EngineResult<(ObjectId, PutOutcome)> {
        let result = self.put_canonical_object_if_absent_inner(canonical_bytes);
        if result.is_err() {
            self.state.poisoned = true;
            self.active = false;
        }
        result
    }

    fn put_canonical_object_if_absent_inner(
        &mut self,
        canonical_bytes: &[u8],
    ) -> EngineResult<(ObjectId, PutOutcome)> {
        self.ensure_active()?;
        let hash_start = Instant::now();
        let id = ObjectId::for_bytes(canonical_bytes);
        bump(
            &mut self.state.counters.object_hash_bytes,
            canonical_bytes.len() as u64,
        )?;
        add_elapsed(&mut self.state.counters.object_hash_ns, hash_start)?;
        let outcome = self.put_object_if_absent_inner(id, canonical_bytes, true)?;
        Ok((id, outcome))
    }

    fn put_object_if_absent_inner(
        &mut self,
        id: ObjectId,
        canonical_bytes: &[u8],
        identity_verified: bool,
    ) -> EngineResult<PutOutcome> {
        self.ensure_active()?;
        if !identity_verified {
            let hash_start = Instant::now();
            let actual = ObjectId::for_bytes(canonical_bytes);
            bump(
                &mut self.state.counters.object_hash_bytes,
                canonical_bytes.len() as u64,
            )?;
            add_elapsed(&mut self.state.counters.object_hash_ns, hash_start)?;
            if actual != id {
                return Err(EngineError::IdentityMismatch {
                    expected: id,
                    actual,
                });
            }
        }
        let validation_start = Instant::now();
        let summary = validate_object_from(Cursor::new(canonical_bytes))
            .map_err(|cause| EngineError::MalformedObject { id, cause })?;
        bump(
            &mut self.state.counters.object_validation_bytes,
            canonical_bytes.len() as u64,
        )?;
        add_elapsed(
            &mut self.state.counters.object_validation_ns,
            validation_start,
        )?;
        bump(&mut self.state.counters.object_validated, 1)?;
        let bucket = usize::from(id.as_bytes()[0]);
        let visible_end = self.state.physical_end;
        if let Some(locator) = lookup_index(&mut self.state, &self.index_heads, id, visible_end)? {
            ensure_object_persisted(&mut self.state, locator)?;
            let max_end = self.state.physical_end;
            authenticate_object_state(
                &mut self.state,
                locator,
                id,
                Some(canonical_bytes),
                max_end,
            )?;
            bump(&mut self.state.counters.objects_reused, 1)?;
            return Ok(PutOutcome::Reused);
        }

        let canonical_len =
            u64::try_from(canonical_bytes.len()).map_err(|_| EngineError::CounterOverflow)?;
        let mut prefix = [0_u8; 41];
        prefix[..32].copy_from_slice(id.as_bytes());
        prefix[32] = summary.kind as u8;
        prefix[33..41].copy_from_slice(&canonical_len.to_be_bytes());
        let (object_offset, _) = append_frame_parts(
            &mut self.state,
            FrameKind::Object,
            self.generation,
            &[&prefix, canonical_bytes],
        )?;
        bump(&mut self.state.counters.objects_created, 1)?;
        bump(&mut self.state.counters.object_bytes_written, canonical_len)?;
        bump(&mut self.state.counters.logical_object_bytes, canonical_len)?;
        #[cfg(test)]
        self.fail_if(AppendFaultPoint::AfterObjectAppend)?;

        if self
            .state
            .cache
            .would_evict_unpersisted(self.state.persisted_end)
        {
            flush_state(&mut self.state)?;
        }

        let mut page = [0_u8; INDEX_PAGE_PAYLOAD_LEN];
        page[0] = id.as_bytes()[0];
        page[8..16].copy_from_slice(&self.index_heads[bucket].to_be_bytes());
        page[16..48].copy_from_slice(id.as_bytes());
        page[48..56].copy_from_slice(&object_offset.to_be_bytes());
        page[56] = summary.kind as u8;
        page[64..72].copy_from_slice(&canonical_len.to_be_bytes());
        let (page_offset, page_end) = append_frame_parts(
            &mut self.state,
            FrameKind::IndexPage,
            self.generation,
            &[&page],
        )?;
        let page_record = IndexPage {
            bucket: id.as_bytes()[0],
            next: self.index_heads[bucket],
            id,
            object_offset,
            kind: summary.kind,
            canonical_len,
            end: page_end,
        };
        if self.state.cache.insert(page_offset, page_record) {
            bump(&mut self.state.counters.index_cache_evictions, 1)?;
        }
        self.index_heads[bucket] = page_offset;
        #[cfg(test)]
        self.fail_if(AppendFaultPoint::AfterIndexAppend)?;
        Ok(PutOutcome::Created)
    }

    pub fn write_delta(&mut self, delta: &DeltaRecord) -> EngineResult<()> {
        self.ensure_active()?;
        delta.validate()?;
        if delta.parent != self.parent {
            return Err(EngineError::ParentMismatch {
                expected: self.parent,
                actual: delta.parent,
            });
        }
        if let Some(previous) = self.state.visible {
            if previous.delta_id == delta.id {
                let state = &mut *self.state;
                let stored = read_delta(
                    state.file.get_ref(),
                    previous.delta_offset,
                    previous.offset,
                    &mut state.counters,
                )?;
                if stored != *delta {
                    return Err(EngineError::ImmutableConflict("delta", delta.id));
                }
                self.delta = Some(delta.clone());
                self.delta_offset = Some(previous.delta_offset);
                return Ok(());
            }
        }
        self.delta = Some(delta.clone());
        self.delta_offset = None;
        Ok(())
    }

    pub fn commit_root(mut self, root: RootRecord) -> EngineResult<()> {
        let result = self.commit_root_inner(root);
        if result.is_err() {
            self.state.poisoned = true;
            self.active = false;
        }
        result
    }

    fn commit_root_inner(&mut self, root: RootRecord) -> EngineResult<()> {
        self.ensure_active()?;
        if root.parent != self.parent {
            return Err(EngineError::ParentMismatch {
                expected: self.parent,
                actual: root.parent,
            });
        }
        let delta = self
            .delta
            .as_ref()
            .ok_or(EngineError::InvalidRecord("delta"))?;
        if delta.child != root.id {
            return Err(EngineError::InvalidRecord("root/delta linkage"));
        }
        flush_state(&mut self.state)?;
        let visible_bound = self.state.physical_end;
        let directory = lookup_index(
            &mut self.state,
            &self.index_heads,
            root.directory_object,
            visible_bound,
        )?
        .ok_or(EngineError::MissingObject(root.directory_object))?;
        authenticate_object_state(
            &mut self.state,
            directory,
            root.directory_object,
            None,
            visible_bound,
        )?;
        if directory.kind != ObjectKind::Directory {
            return Err(EngineError::InvalidRecord("root directory object"));
        }

        let index_root_offset =
            append_index_root(&mut self.state, self.generation, &self.index_heads)?;
        #[cfg(test)]
        self.fail_if(AppendFaultPoint::AfterIndexRootAppend)?;

        let delta_offset = match self.delta_offset {
            Some(offset) => offset,
            None => {
                let (offset, _) = append_delta(&mut self.state, self.generation, delta)?;
                bump(
                    &mut self.state.counters.logical_delta_bytes,
                    delta_record_len(delta)?,
                )?;
                self.delta_offset = Some(offset);
                #[cfg(test)]
                self.fail_if(AppendFaultPoint::AfterDeltaAppend)?;
                offset
            }
        };
        let root_payload = encode_root(&root);
        let (root_offset, _) = append_frame_parts(
            &mut self.state,
            FrameKind::Root,
            self.generation,
            &[&root_payload],
        )?;
        bump(
            &mut self.state.counters.logical_root_bytes,
            root_record_len(&root)?,
        )?;
        #[cfg(test)]
        self.fail_if(AppendFaultPoint::AfterRootAppend)?;

        let (marker_offset, capture_bytes, capture_digest) = {
            let state = &mut *self.state;
            flush_state(state)?;
            let persisted_root = read_root(
                state.file.get_ref(),
                root_offset,
                state.physical_end,
                &mut state.counters,
            )?;
            if persisted_root != root {
                return Err(EngineError::CarrierIntegrity("root publication"));
            }
            let persisted_delta = read_delta(
                state.file.get_ref(),
                delta_offset,
                state.physical_end,
                &mut state.counters,
            )?;
            if persisted_delta != *delta {
                return Err(EngineError::CarrierIntegrity("delta publication"));
            }
            let persisted_heads = read_index_root(
                state.file.get_ref(),
                index_root_offset,
                state.physical_end,
                &mut state.counters,
            )?;
            if persisted_heads != self.index_heads {
                return Err(EngineError::CarrierIntegrity("index root publication"));
            }
            validate_object_closure(
                state.file.get_ref(),
                &persisted_heads,
                root.directory_object,
                state.physical_end,
                &mut state.counters,
            )?;
            let marker_offset = align_up(state.physical_end)?;
            let capture_bytes = marker_offset
                .checked_sub(self.capture_start)
                .ok_or(EngineError::CounterOverflow)?;
            if marker_offset > state.physical_end {
                capture_digest_update_zeros(state, marker_offset - state.physical_end)?;
            }
            let capture_digest = finalize_capture_digest(state)?;
            (marker_offset, capture_bytes, capture_digest)
        };
        let marker_len = MARKER_PAYLOAD_LEN as u64;
        let marker_end = frame_end(marker_offset, marker_len)?;
        let mut marker_payload = encode_marker(
            self.generation,
            self.previous_marker,
            self.parent,
            root.id,
            delta.id,
            delta_offset,
            index_root_offset,
            root_offset,
            marker_end,
            self.capture_start,
            capture_bytes,
            capture_digest,
        );
        let digest_start = marker_payload
            .len()
            .checked_sub(MARKER_DIGEST_LEN)
            .ok_or(EngineError::CounterOverflow)?;
        let digest = marker_digest(&marker_payload[..digest_start]);
        marker_payload[digest_start..].copy_from_slice(&digest);
        bump(&mut self.state.counters.marker_attempts, 1)?;
        let (actual_marker_offset, actual_marker_end) = append_frame_parts(
            &mut self.state,
            FrameKind::CommitMarker,
            self.generation,
            &[&marker_payload],
        )?;
        if actual_marker_offset != marker_offset || actual_marker_end != marker_end {
            return Err(EngineError::InvalidRecord("marker extent"));
        }
        #[cfg(test)]
        self.fail_if(AppendFaultPoint::BeforeMarkerSync)?;
        if let Err(error) = flush_state(&mut self.state) {
            self.state.poisoned = true;
            self.active = false;
            return Err(error);
        }
        bump(&mut self.state.counters.marker_sync_attempts, 1)?;
        let sync_start = Instant::now();
        let sync_result = self.state.file.get_ref().sync_all();
        add_elapsed(&mut self.state.counters.marker_sync_ns, sync_start)?;
        #[cfg(test)]
        if self.fault == Some(AppendFaultPoint::Sync) {
            bump(&mut self.state.counters.marker_sync_failures, 1)?;
            self.state.poisoned = true;
            self.active = false;
            return Err(EngineError::DurabilityAmbiguous);
        }
        if sync_result.is_err() {
            bump(&mut self.state.counters.marker_sync_failures, 1)?;
            self.state.poisoned = true;
            self.active = false;
            return Err(EngineError::DurabilityAmbiguous);
        }
        bump(&mut self.state.counters.marker_sync_successes, 1)?;
        bump(&mut self.state.counters.captures_committed, 1)?;
        self.state.visible = Some(VisibleMarker {
            offset: marker_offset,
            end: marker_end,
            generation: self.generation,
            previous_marker: self.previous_marker,
            parent: self.parent,
            child: root.id,
            delta_id: delta.id,
            delta_offset,
            index_root_offset,
            root_offset,
            capture_start: self.capture_start,
            capture_bytes,
            capture_digest,
        });
        self.state.counters.residue_bytes = self.state.physical_end.saturating_sub(marker_end);
        self.active = false;
        let _ = self.engine;
        Ok(())
    }

    fn ensure_active(&self) -> EngineResult<()> {
        if self.active {
            Ok(())
        } else {
            Err(EngineError::InvalidTransaction)
        }
    }

    #[cfg(test)]
    fn fail_if(&self, point: AppendFaultPoint) -> EngineResult<()> {
        if self.fault == Some(point) {
            return Err(EngineError::InjectedFailure("append-only fault"));
        }
        Ok(())
    }

    #[cfg(test)]
    fn fail_at(&mut self, point: AppendFaultPoint) {
        self.fault = Some(point);
    }
}

impl Drop for AppendOnlyCapture<'_> {
    fn drop(&mut self) {
        if self.active {
            self.active = false;
            let _ = bump(&mut self.state.counters.captures_abandoned, 1);
        }
        let _ = add_elapsed(
            &mut self.state.counters.writer_lock_hold_ns,
            self.hold_start,
        );
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppendFaultPoint {
    AfterObjectAppend,
    AfterIndexAppend,
    AfterIndexRootAppend,
    AfterDeltaAppend,
    AfterRootAppend,
    BeforeMarkerSync,
    Sync,
}

fn scan_log(file: &File) -> EngineResult<ScanResult> {
    let physical_end = file.metadata().map_err(carrier_io)?.len();
    let mut counters = AppendOnlyCounters::default();
    let mut offset = 0_u64;
    let mut last_valid_offset = None;
    let mut last_valid_end = 0_u64;
    let mut visible = None;
    let mut recovery_blocked = false;
    while offset < physical_end {
        bump(&mut counters.frames_scanned, 1)?;
        let header_end = offset
            .checked_add(HEADER_LEN as u64)
            .ok_or(EngineError::CounterOverflow)?;
        if header_end > physical_end {
            bump(
                &mut counters.recovery_torn_bytes,
                physical_end.saturating_sub(offset),
            )?;
            if visible.is_none() {
                return Err(EngineError::CarrierRecoveryTornTail);
            }
            recovery_blocked = true;
            break;
        }
        let header = match read_header(file, offset, &mut counters) {
            Ok(header) => header,
            Err(error @ EngineError::CarrierPermissionDenied(_))
            | Err(error @ EngineError::CarrierNoSpace(_))
            | Err(error @ EngineError::CarrierIo(_))
            | Err(error @ EngineError::ShortRead { .. })
            | Err(error @ EngineError::CounterOverflow) => return Err(error),
            Err(error) => {
                if retain_visible_marker_on_recovery_error(
                    visible,
                    physical_end.saturating_sub(offset),
                    false,
                    &mut counters,
                )? {
                    recovery_blocked = true;
                    break;
                }
                return Err(match error {
                    EngineError::CarrierRecoveryMalformed(_) => error,
                    _ => EngineError::CarrierRecoveryMalformed("frame header"),
                });
            }
        };
        let end = frame_end(offset, header.payload_len)?;
        if end > physical_end {
            bump(
                &mut counters.recovery_torn_bytes,
                physical_end.saturating_sub(offset),
            )?;
            if visible.is_none() {
                return Err(EngineError::CarrierRecoveryTornTail);
            }
            recovery_blocked = true;
            break;
        }
        if let Err(error) = verify_frame_checksum(file, header, &mut counters) {
            match error {
                EngineError::CarrierPermissionDenied(_)
                | EngineError::CarrierNoSpace(_)
                | EngineError::CarrierIo(_)
                | EngineError::ShortRead { .. }
                | EngineError::CounterOverflow => return Err(error),
                _ => {
                    if retain_visible_marker_on_recovery_error(
                        visible,
                        end.saturating_sub(offset),
                        true,
                        &mut counters,
                    )? {
                        recovery_blocked = true;
                        break;
                    }
                    return Err(EngineError::CarrierIntegrity("frame checksum"));
                }
            }
        }
        if header.previous_offset != last_valid_offset.unwrap_or(0) {
            if retain_visible_marker_on_recovery_error(
                visible,
                end.saturating_sub(offset),
                false,
                &mut counters,
            )? {
                recovery_blocked = true;
                break;
            }
            return Err(EngineError::CarrierRecoveryMalformed("frame predecessor"));
        }
        if offset > last_valid_end {
            if retain_visible_marker_on_recovery_error(
                visible,
                offset.saturating_sub(last_valid_end),
                false,
                &mut counters,
            )? {
                recovery_blocked = true;
                break;
            }
            return Err(EngineError::CarrierRecoveryMalformed("frame gap"));
        }
        bump(&mut counters.frames_recovered, 1)?;
        last_valid_offset = Some(offset);
        last_valid_end = end;
        offset = end;
        if header.kind == FrameKind::CommitMarker {
            let payload = match read_payload(file, header, &mut counters) {
                Ok(payload) => payload,
                Err(error @ EngineError::CarrierPermissionDenied(_))
                | Err(error @ EngineError::CarrierNoSpace(_))
                | Err(error @ EngineError::CarrierIo(_))
                | Err(error @ EngineError::ShortRead { .. })
                | Err(error @ EngineError::CounterOverflow) => return Err(error),
                Err(error) => {
                    if retain_visible_marker_on_recovery_error(
                        visible,
                        end.saturating_sub(header.offset),
                        false,
                        &mut counters,
                    )? {
                        recovery_blocked = true;
                        break;
                    }
                    return Err(match error {
                        EngineError::CarrierRecoveryMalformed(_) => error,
                        _ => EngineError::CarrierRecoveryMalformed("commit marker"),
                    });
                }
            };
            let marker = match decode_marker(&payload, header.offset, end) {
                Ok(marker) => marker,
                Err(error) => {
                    if retain_visible_marker_on_recovery_error(
                        visible,
                        end.saturating_sub(header.offset),
                        matches!(error, EngineError::CarrierIntegrity(_)),
                        &mut counters,
                    )? {
                        recovery_blocked = true;
                        break;
                    }
                    return Err(error);
                }
            };
            let validated = match validate_marker(file, marker, visible, &mut counters) {
                Ok(validated) => validated,
                Err(error @ EngineError::CarrierPermissionDenied(_))
                | Err(error @ EngineError::CarrierNoSpace(_))
                | Err(error @ EngineError::CarrierIo(_))
                | Err(error @ EngineError::ShortRead { .. })
                | Err(error @ EngineError::CounterOverflow) => return Err(error),
                Err(error) => {
                    if retain_visible_marker_on_recovery_error(
                        visible,
                        end.saturating_sub(header.offset),
                        matches!(error, EngineError::CarrierIntegrity(_)),
                        &mut counters,
                    )? {
                        recovery_blocked = true;
                        break;
                    }
                    return Err(match error {
                        EngineError::CarrierRecoveryMalformed(_) => error,
                        EngineError::CarrierIntegrity(_) => error,
                        _ => EngineError::CarrierRecoveryMalformed("commit marker"),
                    });
                }
            };
            if !validated {
                if retain_visible_marker_on_recovery_error(
                    visible,
                    end.saturating_sub(header.offset),
                    false,
                    &mut counters,
                )? {
                    recovery_blocked = true;
                    break;
                }
                return Err(EngineError::CarrierRecoveryMalformed("commit marker chain"));
            }
            visible = Some(marker);
            bump(&mut counters.markers_recovered, 1)?;
        };
    }
    let visible_end = visible.map_or(0, |marker| marker.end);
    counters.residue_bytes = physical_end
        .checked_sub(visible_end)
        .ok_or(EngineError::CarrierIntegrity("visible end"))?;
    Ok(ScanResult {
        physical_end,
        last_valid_offset,
        visible,
        residue_bytes: counters.residue_bytes,
        recovery_blocked,
        counters,
    })
}

struct ScanResult {
    physical_end: u64,
    last_valid_offset: Option<u64>,
    visible: Option<VisibleMarker>,
    residue_bytes: u64,
    recovery_blocked: bool,
    counters: AppendOnlyCounters,
}

fn retain_visible_marker_on_recovery_error(
    visible: Option<VisibleMarker>,
    bytes: u64,
    integrity: bool,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<bool> {
    if visible.is_none() {
        return Ok(false);
    }
    if integrity {
        bump(&mut counters.recovery_integrity_bytes, bytes)?;
    } else {
        bump(&mut counters.recovery_malformed_bytes, bytes)?;
    }
    Ok(true)
}

fn validate_marker(
    file: &File,
    marker: VisibleMarker,
    previous: Option<VisibleMarker>,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<bool> {
    let previous_end = previous.map_or(0, |marker| marker.end);
    if marker.generation == 0 {
        if marker.previous_marker != 0 || marker.parent.is_some() {
            return Ok(false);
        }
    } else {
        let Some(previous) = previous else {
            return Ok(false);
        };
        if marker.previous_marker != previous.offset
            || marker.generation != previous.generation.saturating_add(1)
            || marker.parent != Some(previous.child)
        {
            return Ok(false);
        }
    }
    if marker.capture_start < previous_end
        || marker.capture_start > marker.offset
        || marker.capture_bytes != marker.offset.saturating_sub(marker.capture_start)
        || marker.capture_start.checked_add(marker.capture_bytes) != Some(marker.offset)
        || marker.delta_offset >= marker.offset
        || marker.index_root_offset >= marker.offset
        || marker.root_offset >= marker.offset
    {
        return Ok(false);
    }
    let capture_digest =
        capture_range_digest(file, marker.capture_start, marker.capture_bytes, counters)?;
    if capture_digest != marker.capture_digest {
        return Err(EngineError::CarrierIntegrity("capture evidence"));
    }
    let root = read_root(file, marker.root_offset, marker.offset, counters)?;
    if root.id != marker.child || root.parent != marker.parent {
        return Ok(false);
    }
    let delta = read_delta(file, marker.delta_offset, marker.offset, counters)?;
    if delta.id != marker.delta_id || delta.child != marker.child || delta.parent != marker.parent {
        return Ok(false);
    }
    let heads = read_index_root(file, marker.index_root_offset, marker.offset, counters)?;
    validate_index(file, &heads, marker.offset, counters)?;
    let Some(directory) =
        lookup_index_file(file, &heads, root.directory_object, marker.offset, counters)?
    else {
        return Ok(false);
    };
    if directory.kind != ObjectKind::Directory {
        return Ok(false);
    }
    authenticate_object(
        file,
        directory,
        root.directory_object,
        None,
        marker.offset,
        counters,
    )?;
    validate_object_closure(file, &heads, root.directory_object, marker.offset, counters)?;
    Ok(true)
}

fn validate_root_and_directory_state(
    state: &mut AppendState,
    root_offset: u64,
    expected_id: ObjectId,
    index_root_offset: u64,
    max_end: u64,
) -> EngineResult<()> {
    validate_root_and_directory(
        state.file.get_ref(),
        root_offset,
        expected_id,
        index_root_offset,
        max_end,
        &mut state.counters,
    )
}

fn authenticate_object_state(
    state: &mut AppendState,
    locator: ObjectLocator,
    expected_id: ObjectId,
    expected_bytes: Option<&[u8]>,
    max_end: u64,
) -> EngineResult<()> {
    authenticate_object(
        state.file.get_ref(),
        locator,
        expected_id,
        expected_bytes,
        max_end,
        &mut state.counters,
    )
}

fn read_exact_state(state: &mut AppendState, offset: u64, output: &mut [u8]) -> EngineResult<()> {
    read_exact_at(state.file.get_ref(), offset, output, &mut state.counters)
}

fn validate_root_and_directory(
    file: &File,
    root_offset: u64,
    expected_id: ObjectId,
    index_root_offset: u64,
    max_end: u64,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<()> {
    let root = read_root(file, root_offset, max_end, counters)?;
    if root.id != expected_id {
        return Err(EngineError::InvalidRecord("root identity"));
    }
    let heads = read_index_root(file, index_root_offset, max_end, counters)?;
    let locator = lookup_index_file(file, &heads, root.directory_object, max_end, counters)?
        .ok_or(EngineError::MissingObject(root.directory_object))?;
    authenticate_object(
        file,
        locator,
        root.directory_object,
        None,
        max_end,
        counters,
    )?;
    if locator.kind != ObjectKind::Directory {
        return Err(EngineError::InvalidRecord("root directory object"));
    }
    validate_index(file, &heads, max_end, counters)?;
    validate_object_closure(file, &heads, root.directory_object, max_end, counters)?;
    Ok(())
}

fn validate_index(
    file: &File,
    heads: &[u64; BUCKETS],
    max_end: u64,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<()> {
    let mut visits = 0_u64;
    for (bucket, &head) in heads.iter().enumerate() {
        let mut offset = head;
        while offset != 0 {
            visits = visits.checked_add(1).ok_or(EngineError::CounterOverflow)?;
            if visits > MAX_INDEX_VISITS {
                return Err(EngineError::CarrierIntegrity("index visit bound"));
            }
            let page = read_index_page(file, offset, max_end, counters)?;
            if page.bucket != bucket as u8 {
                return Err(EngineError::CarrierIntegrity("index bucket"));
            }
            let locator = ObjectLocator {
                offset: page.object_offset,
                kind: page.kind,
                canonical_len: page.canonical_len,
            };
            authenticate_object(file, locator, page.id, None, max_end, counters)?;
            offset = page.next;
        }
    }
    Ok(())
}

fn validate_object_closure(
    file: &File,
    heads: &[u64; BUCKETS],
    root_id: ObjectId,
    max_end: u64,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<()> {
    let mut pending = vec![root_id];
    let mut visits = 0_u64;
    while let Some(id) = pending.pop() {
        visits = visits.checked_add(1).ok_or(EngineError::CounterOverflow)?;
        if visits > MAX_CLOSURE_VISITS {
            return Err(EngineError::CarrierIntegrity("object closure visit bound"));
        }
        let locator = lookup_index_file(file, heads, id, max_end, counters)?
            .ok_or(EngineError::MissingObject(id))?;
        authenticate_object(file, locator, id, None, max_end, counters)?;
        let bytes = read_object_bytes(file, locator, max_end, counters)?;
        let object =
            decode_object(&bytes).map_err(|cause| EngineError::MalformedObject { id, cause })?;
        bump(&mut counters.closure_objects, 1)?;
        if let Object::Directory(entries) = object {
            for entry in entries {
                bump(&mut counters.closure_references, 1)?;
                let reference = entry.reference();
                let child = lookup_index_file(file, heads, reference.id(), max_end, counters)?
                    .ok_or(EngineError::MissingObject(reference.id()))?;
                if child.kind != reference.kind() {
                    return Err(EngineError::CarrierIntegrity("directory reference kind"));
                }
                pending.push(reference.id());
            }
        }
    }
    Ok(())
}

fn read_object_bytes(
    file: &File,
    locator: ObjectLocator,
    max_end: u64,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<Vec<u8>> {
    let (_, _, length, canonical_start) = object_metadata(file, locator.offset, max_end, counters)?;
    let length = usize::try_from(length).map_err(|_| EngineError::CounterOverflow)?;
    let mut bytes = vec![0_u8; length];
    read_exact_at(file, canonical_start, &mut bytes, counters)?;
    Ok(bytes)
}

fn capture_range_digest(
    file: &File,
    start: u64,
    length: u64,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<[u8; MARKER_DIGEST_LEN]> {
    let digest_start = Instant::now();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/phase4b-capture-v1\0");
    let mut offset = start;
    let mut remaining = length;
    let mut buffer = [0_u8; 32 * 1024];
    while remaining != 0 {
        let amount = remaining.min(buffer.len() as u64) as usize;
        read_exact_at(file, offset, &mut buffer[..amount], counters)?;
        hasher.update(&buffer[..amount]);
        offset = offset
            .checked_add(amount as u64)
            .ok_or(EngineError::CounterOverflow)?;
        remaining -= amount as u64;
        bump(&mut counters.marker_capture_digest_bytes, amount as u64)?;
    }
    add_elapsed(&mut counters.marker_capture_digest_ns, digest_start)?;
    Ok(*hasher.finalize().as_bytes())
}

fn capture_digest_update(state: &mut AppendState, bytes: &[u8]) -> EngineResult<()> {
    if let Some(hasher) = state.capture_digest.as_mut() {
        let digest_start = Instant::now();
        hasher.update(bytes);
        add_elapsed(&mut state.counters.marker_capture_digest_ns, digest_start)?;
        bump(
            &mut state.counters.marker_capture_digest_bytes,
            bytes.len() as u64,
        )?;
    }
    Ok(())
}

fn capture_digest_update_zeros(state: &mut AppendState, mut length: u64) -> EngineResult<()> {
    let zeros = [0_u8; 4096];
    while length != 0 {
        let amount = length.min(zeros.len() as u64) as usize;
        capture_digest_update(state, &zeros[..amount])?;
        length -= amount as u64;
    }
    Ok(())
}

fn finalize_capture_digest(state: &mut AppendState) -> EngineResult<[u8; MARKER_DIGEST_LEN]> {
    let hasher = state
        .capture_digest
        .take()
        .ok_or(EngineError::InvalidTransaction)?;
    Ok(*hasher.finalize().as_bytes())
}

fn read_header(
    file: &File,
    offset: u64,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<FrameHeader> {
    let mut bytes = [0_u8; HEADER_LEN];
    read_exact_at(file, offset, &mut bytes, counters)?;
    FrameHeader::decode(&bytes, offset)
}

fn read_payload(
    file: &File,
    header: FrameHeader,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<Vec<u8>> {
    let length = usize::try_from(header.payload_len).map_err(|_| EngineError::CounterOverflow)?;
    let mut payload = vec![0_u8; length];
    read_exact_at(
        file,
        header
            .offset
            .checked_add(HEADER_LEN as u64)
            .ok_or(EngineError::CounterOverflow)?,
        &mut payload,
        counters,
    )?;
    Ok(payload)
}

fn read_frame(
    file: &File,
    offset: u64,
    expected_kind: FrameKind,
    max_end: u64,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<Vec<u8>> {
    let header = read_header(file, offset, counters)?;
    if header.kind != expected_kind {
        return Err(EngineError::InvalidRecord("carrier frame reference"));
    }
    let end = frame_end(offset, header.payload_len)?;
    if end > max_end {
        return Err(EngineError::CarrierIntegrity("frame beyond visible marker"));
    }
    verify_frame_checksum(file, header, counters)?;
    read_payload(file, header, counters)
}

fn verify_frame_checksum(
    file: &File,
    header: FrameHeader,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<()> {
    let reader = PayloadReader::new(
        file,
        header
            .offset
            .checked_add(HEADER_LEN as u64)
            .ok_or(EngineError::CounterOverflow)?,
        header.payload_len,
    );
    let prefix = header.prefix();
    let mut input = prefix.as_slice().chain(reader);
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 32 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(carrier_io)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        bump(&mut counters.carrier_bytes_read, read as u64)?;
    }
    let digest = hasher.finalize();
    if digest.as_bytes() != &header.checksum {
        return Err(EngineError::InvalidRecord("carrier frame checksum"));
    }
    Ok(())
}

fn append_frame_parts(
    state: &mut AppendState,
    kind: FrameKind,
    generation: u64,
    parts: &[&[u8]],
) -> EngineResult<(u64, u64)> {
    let append_start = Instant::now();
    let payload_len = parts.iter().try_fold(0_u64, |sum, part| {
        sum.checked_add(u64::try_from(part.len()).map_err(|_| EngineError::CounterOverflow)?)
            .ok_or(EngineError::CounterOverflow)
    })?;
    if payload_len > MAX_FRAME_PAYLOAD {
        return Err(EngineError::InvalidRecord("carrier frame length"));
    }
    let offset = align_up(state.physical_end)?;
    let mut header = FrameHeader {
        kind,
        payload_len,
        generation,
        previous_offset: state.last_valid_offset.unwrap_or(0),
        offset,
        checksum: [0_u8; CHECKSUM_LEN],
    };
    let mut hasher = blake3::Hasher::new();
    hasher.update(&header.prefix());
    for part in parts {
        hasher.update(part);
    }
    header
        .checksum
        .copy_from_slice(hasher.finalize().as_bytes());
    let encoded = header.encode();
    let gap = offset.saturating_sub(state.physical_end);
    if gap != 0 {
        write_zeros(&mut state.file, gap, &mut state.counters)?;
        capture_digest_update_zeros(state, gap)?;
    }
    write_all_counted(&mut state.file, &encoded, &mut state.counters)?;
    capture_digest_update(state, &encoded)?;
    for part in parts {
        write_all_counted(&mut state.file, part, &mut state.counters)?;
        capture_digest_update(state, part)?;
    }
    let end = frame_end(offset, payload_len)?;
    let padding = end
        .checked_sub(
            offset
                .checked_add(HEADER_LEN as u64)
                .and_then(|value| value.checked_add(payload_len))
                .ok_or(EngineError::CounterOverflow)?,
        )
        .ok_or(EngineError::CounterOverflow)?;
    if padding != 0 {
        write_zeros(&mut state.file, padding, &mut state.counters)?;
        capture_digest_update_zeros(state, padding)?;
    }
    state.physical_end = end;
    state.last_valid_offset = Some(offset);
    bump(&mut state.counters.frames_appended, 1)?;
    bump(&mut state.counters.frame_bytes_appended, end - offset)?;
    bump_frame_bytes(&mut state.counters, kind, end - offset)?;
    add_elapsed(&mut state.counters.carrier_append_ns, append_start)?;
    Ok((offset, end))
}

fn flush_state(state: &mut AppendState) -> EngineResult<()> {
    let flush_start = Instant::now();
    let result = state.file.flush();
    add_elapsed(&mut state.counters.carrier_flush_ns, flush_start)?;
    match result {
        Ok(()) => {
            bump(&mut state.counters.carrier_flush_calls, 1)?;
            state.persisted_end = state.physical_end;
            Ok(())
        }
        Err(error) => {
            bump(&mut state.counters.carrier_flush_failures, 1)?;
            Err(carrier_io(error))
        }
    }
}

fn ensure_object_persisted(state: &mut AppendState, locator: ObjectLocator) -> EngineResult<()> {
    let object_end = frame_end(
        locator.offset,
        41_u64
            .checked_add(locator.canonical_len)
            .ok_or(EngineError::CounterOverflow)?,
    )?;
    if object_end > state.persisted_end {
        flush_state(state)?;
    }
    Ok(())
}

fn append_index_root(
    state: &mut AppendState,
    generation: u64,
    heads: &[u64; BUCKETS],
) -> EngineResult<u64> {
    let mut payload = vec![0_u8; BUCKETS * 8];
    for (index, head) in heads.iter().enumerate() {
        let start = index.checked_mul(8).ok_or(EngineError::CounterOverflow)?;
        payload[start..start + 8].copy_from_slice(&head.to_be_bytes());
    }
    let (offset, _) = append_frame_parts(state, FrameKind::IndexRoot, generation, &[&payload])?;
    Ok(offset)
}

fn append_delta(
    state: &mut AppendState,
    generation: u64,
    delta: &DeltaRecord,
) -> EngineResult<(u64, u64)> {
    let payload_len =
        u64::try_from(delta.payload.len()).map_err(|_| EngineError::CounterOverflow)?;
    let mut prefix = Vec::with_capacity(32 + 33 + 32 + 8);
    prefix.extend_from_slice(delta.id.as_bytes());
    match delta.parent {
        Some(parent) => {
            prefix.push(1);
            prefix.extend_from_slice(parent.as_bytes());
        }
        None => prefix.extend_from_slice(&[0_u8; 33]),
    }
    prefix.extend_from_slice(delta.child.as_bytes());
    prefix.extend_from_slice(&payload_len.to_be_bytes());
    append_frame_parts(
        state,
        FrameKind::Delta,
        generation,
        &[&prefix, &delta.payload],
    )
}

fn frame_end(offset: u64, payload_len: u64) -> EngineResult<u64> {
    let raw = offset
        .checked_add(HEADER_LEN as u64)
        .and_then(|value| value.checked_add(payload_len))
        .ok_or(EngineError::CounterOverflow)?;
    align_up(raw)
}

fn align_up(value: u64) -> EngineResult<u64> {
    let remainder = value % ALIGNMENT;
    if remainder == 0 {
        Ok(value)
    } else {
        value
            .checked_add(ALIGNMENT - remainder)
            .ok_or(EngineError::CounterOverflow)
    }
}

fn lookup_index_for_visible(
    state: &mut AppendState,
    id: ObjectId,
) -> EngineResult<Option<ObjectLocator>> {
    let Some(marker) = state.visible else {
        return Ok(None);
    };
    let heads = read_index_root_state(state, marker.index_root_offset)?;
    lookup_index(state, &heads, id, marker.offset)
}

fn lookup_index(
    state: &mut AppendState,
    heads: &[u64; BUCKETS],
    id: ObjectId,
    max_end: u64,
) -> EngineResult<Option<ObjectLocator>> {
    bump(&mut state.counters.index_lookups, 1)?;
    let lookup_start = Instant::now();
    let result = lookup_index_file_cached(
        state.file.get_ref(),
        &mut state.cache,
        heads,
        id,
        max_end,
        state.persisted_end,
        &mut state.counters,
    );
    add_elapsed(&mut state.counters.index_lookup_ns, lookup_start)?;
    result
}

fn lookup_index_file(
    file: &File,
    heads: &[u64; BUCKETS],
    id: ObjectId,
    max_end: u64,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<Option<ObjectLocator>> {
    lookup_index_file_cached(
        file,
        &mut PageCache::new(0),
        heads,
        id,
        max_end,
        max_end,
        counters,
    )
}

fn lookup_index_file_cached(
    file: &File,
    cache: &mut PageCache,
    heads: &[u64; BUCKETS],
    id: ObjectId,
    max_end: u64,
    persisted_end: u64,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<Option<ObjectLocator>> {
    let mut offset = heads[usize::from(id.as_bytes()[0])];
    let mut visits = 0_u64;
    while offset != 0 {
        visits = visits.checked_add(1).ok_or(EngineError::CounterOverflow)?;
        if visits > MAX_INDEX_VISITS {
            return Err(EngineError::InvalidRecord("index cycle"));
        }
        let page = if let Some(page) = cache.get(offset) {
            if page.end > max_end {
                return Err(EngineError::CarrierIntegrity(
                    "cached index page beyond marker",
                ));
            }
            bump(&mut counters.index_cache_hits, 1)?;
            page
        } else {
            bump(&mut counters.index_cache_misses, 1)?;
            let page = read_index_page(file, offset, max_end, counters)?;
            if !cache.would_evict_unpersisted(persisted_end) && cache.insert(offset, page) {
                bump(&mut counters.index_cache_evictions, 1)?;
            }
            page
        };
        if page.bucket != id.as_bytes()[0] {
            return Err(EngineError::InvalidRecord("index bucket"));
        }
        if page.id == id {
            return Ok(Some(ObjectLocator {
                offset: page.object_offset,
                kind: page.kind,
                canonical_len: page.canonical_len,
            }));
        }
        offset = page.next;
    }
    Ok(None)
}

fn read_index_root(
    file: &File,
    offset: u64,
    max_end: u64,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<[u64; BUCKETS]> {
    bump(&mut counters.index_root_reads, 1)?;
    bump(&mut counters.index_page_reads, 1)?;
    let payload = read_frame(file, offset, FrameKind::IndexRoot, max_end, counters)?;
    if payload.len() != BUCKETS * 8 {
        return Err(EngineError::InvalidRecord("index root length"));
    }
    let mut heads = [0_u64; BUCKETS];
    for (index, head) in heads.iter_mut().enumerate() {
        let start = index.checked_mul(8).ok_or(EngineError::CounterOverflow)?;
        *head = read_u64(&payload[start..start + 8])?;
    }
    Ok(heads)
}

fn read_index_root_state(state: &mut AppendState, offset: u64) -> EngineResult<[u64; BUCKETS]> {
    let max_end = state
        .visible
        .map_or(state.physical_end, |marker| marker.offset);
    read_index_root(state.file.get_ref(), offset, max_end, &mut state.counters)
}

fn read_index_page(
    file: &File,
    offset: u64,
    max_end: u64,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<IndexPage> {
    bump(&mut counters.index_page_reads, 1)?;
    let payload = read_frame(file, offset, FrameKind::IndexPage, max_end, counters)?;
    if payload.len() != INDEX_PAGE_PAYLOAD_LEN {
        return Err(EngineError::InvalidRecord("index page length"));
    }
    Ok(IndexPage {
        bucket: payload[0],
        next: read_u64(&payload[8..16])?,
        id: ObjectId::from_bytes(&payload[16..48]).map_err(EngineError::Core)?,
        object_offset: read_u64(&payload[48..56])?,
        kind: ObjectKind::try_from(payload[56]).map_err(EngineError::Core)?,
        canonical_len: read_u64(&payload[64..72])?,
        end: frame_end(offset, payload.len() as u64)?,
    })
}

fn read_root(
    file: &File,
    offset: u64,
    max_end: u64,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<RootRecord> {
    bump(&mut counters.root_reads, 1)?;
    let payload = read_frame(file, offset, FrameKind::Root, max_end, counters)?;
    decode_root(&payload)
}

fn encode_root(root: &RootRecord) -> Vec<u8> {
    let mut payload = vec![0_u8; 97];
    payload[..32].copy_from_slice(root.id.as_bytes());
    payload[32..64].copy_from_slice(root.directory_object.as_bytes());
    if let Some(parent) = root.parent {
        payload[64] = 1;
        payload[65..97].copy_from_slice(parent.as_bytes());
    }
    payload
}

fn decode_root(payload: &[u8]) -> EngineResult<RootRecord> {
    if payload.len() != 97 {
        return Err(EngineError::InvalidRecord("root length"));
    }
    let id = ObjectId::from_bytes(&payload[..32]).map_err(EngineError::Core)?;
    let directory_object = ObjectId::from_bytes(&payload[32..64]).map_err(EngineError::Core)?;
    let parent = match payload[64] {
        0 => None,
        1 => Some(ObjectId::from_bytes(&payload[65..97]).map_err(EngineError::Core)?),
        _ => return Err(EngineError::InvalidRecord("root parent")),
    };
    Ok(RootRecord {
        id,
        directory_object,
        parent,
    })
}

fn read_delta(
    file: &File,
    offset: u64,
    max_end: u64,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<DeltaRecord> {
    bump(&mut counters.delta_reads, 1)?;
    let payload = read_frame(file, offset, FrameKind::Delta, max_end, counters)?;
    if payload.len() < 105 {
        return Err(EngineError::InvalidRecord("delta length"));
    }
    let id = ObjectId::from_bytes(&payload[..32]).map_err(EngineError::Core)?;
    let parent = match payload[32] {
        0 => None,
        1 => Some(ObjectId::from_bytes(&payload[33..65]).map_err(EngineError::Core)?),
        _ => return Err(EngineError::InvalidRecord("delta parent")),
    };
    let child = ObjectId::from_bytes(&payload[65..97]).map_err(EngineError::Core)?;
    let length =
        usize::try_from(read_u64(&payload[97..105])?).map_err(|_| EngineError::CounterOverflow)?;
    let end = 105usize
        .checked_add(length)
        .ok_or(EngineError::CounterOverflow)?;
    if end != payload.len() {
        return Err(EngineError::InvalidRecord("delta payload length"));
    }
    let delta = DeltaRecord {
        id,
        parent,
        child,
        payload: payload[105..].to_vec(),
    };
    delta.validate()?;
    Ok(delta)
}

fn encode_marker(
    generation: u64,
    previous_marker: u64,
    parent: Option<ObjectId>,
    child: ObjectId,
    delta_id: ObjectId,
    delta_offset: u64,
    index_root_offset: u64,
    root_offset: u64,
    visible_end: u64,
    capture_start: u64,
    capture_bytes: u64,
    capture_digest: [u8; MARKER_DIGEST_LEN],
) -> Vec<u8> {
    let mut payload = vec![0_u8; MARKER_PAYLOAD_LEN];
    let mut cursor = 0_usize;
    payload[..MARKER_FORMAT_ID.len()].copy_from_slice(&MARKER_FORMAT_ID);
    cursor += MARKER_FORMAT_ID.len();
    put_u64(&mut payload, &mut cursor, generation);
    put_u64(&mut payload, &mut cursor, previous_marker);
    match parent {
        Some(parent) => {
            payload[cursor] = 1;
            cursor += 1;
            payload[cursor..cursor + 32].copy_from_slice(parent.as_bytes());
            cursor += 32;
        }
        None => cursor += 33,
    }
    payload[cursor..cursor + 32].copy_from_slice(child.as_bytes());
    cursor += 32;
    payload[cursor..cursor + 32].copy_from_slice(delta_id.as_bytes());
    cursor += 32;
    for value in [
        delta_offset,
        index_root_offset,
        root_offset,
        visible_end,
        capture_start,
        capture_bytes,
    ] {
        put_u64(&mut payload, &mut cursor, value);
    }
    payload[cursor..cursor + MARKER_DIGEST_LEN].copy_from_slice(&capture_digest);
    payload
}

fn decode_marker(payload: &[u8], offset: u64, end: u64) -> EngineResult<VisibleMarker> {
    if payload.len() != MARKER_PAYLOAD_LEN {
        return Err(EngineError::InvalidRecord("marker length"));
    }
    let digest_start = payload
        .len()
        .checked_sub(MARKER_DIGEST_LEN)
        .ok_or(EngineError::CounterOverflow)?;
    let expected = marker_digest(&payload[..digest_start]);
    if payload[digest_start..] != expected {
        return Err(EngineError::CarrierIntegrity("commit marker digest"));
    }
    if payload[..MARKER_FORMAT_ID.len()] != MARKER_FORMAT_ID {
        return Err(EngineError::CarrierRecoveryMalformed("marker format"));
    }
    let mut cursor = 0_usize;
    cursor += MARKER_FORMAT_ID.len();
    let generation = take_u64(payload, &mut cursor)?;
    let previous_marker = take_u64(payload, &mut cursor)?;
    let parent = match payload[cursor] {
        0 => {
            cursor += 33;
            None
        }
        1 => {
            cursor += 1;
            let id =
                ObjectId::from_bytes(&payload[cursor..cursor + 32]).map_err(EngineError::Core)?;
            cursor += 32;
            Some(id)
        }
        _ => return Err(EngineError::InvalidRecord("marker parent")),
    };
    let child = take_id(payload, &mut cursor)?;
    let delta_id = take_id(payload, &mut cursor)?;
    let delta_offset = take_u64(payload, &mut cursor)?;
    let index_root_offset = take_u64(payload, &mut cursor)?;
    let root_offset = take_u64(payload, &mut cursor)?;
    let visible_end = take_u64(payload, &mut cursor)?;
    let capture_start = take_u64(payload, &mut cursor)?;
    let capture_bytes = take_u64(payload, &mut cursor)?;
    let capture_digest_end = cursor
        .checked_add(MARKER_DIGEST_LEN)
        .ok_or(EngineError::CounterOverflow)?;
    let mut capture_digest = [0_u8; MARKER_DIGEST_LEN];
    capture_digest.copy_from_slice(&payload[cursor..capture_digest_end]);
    cursor = capture_digest_end;
    if cursor != digest_start || visible_end != end {
        return Err(EngineError::InvalidRecord("marker extent"));
    }
    Ok(VisibleMarker {
        offset,
        end,
        generation,
        previous_marker,
        parent,
        child,
        delta_id,
        delta_offset,
        index_root_offset,
        root_offset,
        capture_start,
        capture_bytes,
        capture_digest,
    })
}

fn marker_digest(payload_without_digest: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/phase4b-marker-v2\0");
    hasher.update(payload_without_digest);
    *hasher.finalize().as_bytes()
}

fn find_marker_for_root(
    file: &File,
    current: Option<VisibleMarker>,
    id: ObjectId,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<VisibleMarker> {
    let mut marker = current.ok_or(EngineError::MissingRoot(id))?;
    let mut visits = 0_u64;
    loop {
        visits = visits.checked_add(1).ok_or(EngineError::CounterOverflow)?;
        if visits > MAX_MARKER_VISITS {
            return Err(EngineError::CarrierIntegrity("marker chain bound"));
        }
        if marker.child == id {
            return Ok(marker);
        }
        if marker.previous_marker == 0 {
            return Err(EngineError::MissingRoot(id));
        }
        if marker.previous_marker >= marker.offset {
            return Err(EngineError::CarrierIntegrity("marker chain order"));
        }
        marker = read_marker_at(file, marker.previous_marker, counters)?;
    }
}

fn find_marker_for_delta(
    file: &File,
    current: Option<VisibleMarker>,
    id: ObjectId,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<VisibleMarker> {
    let mut marker = current.ok_or(EngineError::MissingDelta(id))?;
    let mut visits = 0_u64;
    loop {
        visits = visits.checked_add(1).ok_or(EngineError::CounterOverflow)?;
        if visits > MAX_MARKER_VISITS {
            return Err(EngineError::CarrierIntegrity("marker chain bound"));
        }
        if marker.delta_id == id {
            return Ok(marker);
        }
        if marker.previous_marker == 0 {
            return Err(EngineError::MissingDelta(id));
        }
        if marker.previous_marker >= marker.offset {
            return Err(EngineError::CarrierIntegrity("marker chain order"));
        }
        marker = read_marker_at(file, marker.previous_marker, counters)?;
    }
}

fn read_marker_at(
    file: &File,
    offset: u64,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<VisibleMarker> {
    bump(&mut counters.marker_reads, 1)?;
    let header = read_header(file, offset, counters)?;
    if header.kind != FrameKind::CommitMarker {
        return Err(EngineError::InvalidRecord("marker chain"));
    }
    verify_frame_checksum(file, header, counters)?;
    let payload = read_payload(file, header, counters)?;
    let end = frame_end(offset, header.payload_len)?;
    decode_marker(&payload, offset, end)
}

fn authenticate_object(
    file: &File,
    locator: ObjectLocator,
    expected_id: ObjectId,
    expected_bytes: Option<&[u8]>,
    max_end: u64,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<()> {
    let auth_start = Instant::now();
    let (stored_id, stored_kind, stored_len, canonical_start) =
        object_metadata(file, locator.offset, max_end, counters)?;
    if stored_id != expected_id
        || stored_kind != locator.kind
        || stored_len != locator.canonical_len
    {
        return Err(EngineError::InvalidRecord("object locator"));
    }
    let hash_start = Instant::now();
    let actual = ObjectId::from_reader(PayloadReader::new(file, canonical_start, stored_len))
        .map_err(carrier_io)?;
    bump(&mut counters.object_hash_bytes, stored_len)?;
    add_elapsed(&mut counters.object_hash_ns, hash_start)?;
    if actual != expected_id {
        return Err(EngineError::IdentityMismatch {
            expected: expected_id,
            actual,
        });
    }
    let validation_start = Instant::now();
    let summary = validate_object_from(PayloadReader::new(file, canonical_start, stored_len))
        .map_err(|cause| EngineError::MalformedObject {
            id: expected_id,
            cause,
        })?;
    bump(&mut counters.object_validation_bytes, stored_len)?;
    add_elapsed(&mut counters.object_validation_ns, validation_start)?;
    if summary.kind != stored_kind || summary.canonical_len != stored_len {
        return Err(EngineError::InvalidRecord("object metadata"));
    }
    if let Some(expected_bytes) = expected_bytes {
        let compare_start = Instant::now();
        let same = expected_bytes.len() as u64 == stored_len
            && same_bytes(file, canonical_start, expected_bytes, counters)?;
        bump(
            &mut counters.object_compare_bytes,
            expected_bytes.len() as u64,
        )?;
        add_elapsed(&mut counters.object_compare_ns, compare_start)?;
        if !same {
            return Err(EngineError::ImmutableConflict("object", expected_id));
        }
    }
    bump(&mut counters.object_validated, 1)?;
    bump(&mut counters.object_bytes_read, stored_len)?;
    add_elapsed(&mut counters.object_auth_ns, auth_start)?;
    Ok(())
}

fn object_metadata(
    file: &File,
    offset: u64,
    max_end: u64,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<(ObjectId, ObjectKind, u64, u64)> {
    let header = read_header(file, offset, counters)?;
    if header.kind != FrameKind::Object {
        return Err(EngineError::InvalidRecord("object frame kind"));
    }
    let end = frame_end(offset, header.payload_len)?;
    if end > max_end {
        return Err(EngineError::CarrierIntegrity(
            "object beyond visible marker",
        ));
    }
    verify_frame_checksum(file, header, counters)?;
    if header.payload_len < 41 {
        return Err(EngineError::InvalidRecord("object frame length"));
    }
    let mut prefix = [0_u8; 41];
    read_exact_at(
        file,
        offset
            .checked_add(HEADER_LEN as u64)
            .ok_or(EngineError::CounterOverflow)?,
        &mut prefix,
        counters,
    )?;
    let id = ObjectId::from_bytes(&prefix[..32]).map_err(EngineError::Core)?;
    let kind = ObjectKind::try_from(prefix[32]).map_err(EngineError::Core)?;
    let canonical_len = read_u64(&prefix[33..41])?;
    if canonical_len != header.payload_len - 41 {
        return Err(EngineError::InvalidRecord("object canonical length"));
    }
    let canonical_start = offset
        .checked_add(HEADER_LEN as u64)
        .and_then(|value| value.checked_add(41))
        .ok_or(EngineError::CounterOverflow)?;
    Ok((id, kind, canonical_len, canonical_start))
}

fn same_bytes(
    file: &File,
    offset: u64,
    expected: &[u8],
    counters: &mut AppendOnlyCounters,
) -> EngineResult<bool> {
    let mut reader = PayloadReader::new(file, offset, expected.len() as u64);
    let mut buffer = [0_u8; 32 * 1024];
    let mut at = 0_usize;
    loop {
        let read = reader.read(&mut buffer).map_err(carrier_io)?;
        if read == 0 {
            break;
        }
        bump(&mut counters.carrier_bytes_read, read as u64)?;
        if buffer[..read] != expected[at..at + read] {
            return Ok(false);
        }
        at += read;
    }
    Ok(at == expected.len())
}

fn object_payload_start(offset: u64) -> EngineResult<u64> {
    offset
        .checked_add(HEADER_LEN as u64)
        .and_then(|value| value.checked_add(41))
        .ok_or(EngineError::CounterOverflow)
}

struct PayloadReader<'a> {
    file: &'a File,
    offset: u64,
    remaining: u64,
}

impl<'a> PayloadReader<'a> {
    fn new(file: &'a File, offset: u64, remaining: u64) -> Self {
        Self {
            file,
            offset,
            remaining,
        }
    }
}

impl Read for PayloadReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 || output.is_empty() {
            return Ok(0);
        }
        let requested = usize::try_from(self.remaining)
            .unwrap_or(usize::MAX)
            .min(output.len());
        let read = self.file.read_at(&mut output[..requested], self.offset)?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "carrier payload",
            ));
        }
        self.offset = self
            .offset
            .checked_add(read as u64)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "carrier offset overflow"))?;
        self.remaining -= read as u64;
        Ok(read)
    }
}

fn read_exact_at(
    file: &File,
    mut offset: u64,
    mut output: &mut [u8],
    counters: &mut AppendOnlyCounters,
) -> EngineResult<()> {
    let expected = output.len() as u64;
    let mut actual = 0_u64;
    while !output.is_empty() {
        let read = file.read_at(output, offset).map_err(carrier_io)?;
        bump(&mut counters.carrier_read_calls, 1)?;
        bump(&mut counters.carrier_bytes_read, read as u64)?;
        if read == 0 {
            return Err(EngineError::ShortRead { expected, actual });
        }
        actual = actual
            .checked_add(read as u64)
            .ok_or(EngineError::CounterOverflow)?;
        offset = offset
            .checked_add(read as u64)
            .ok_or(EngineError::CounterOverflow)?;
        output = &mut output[read..];
    }
    Ok(())
}

fn write_all_counted<W: Write>(
    file: &mut W,
    bytes: &[u8],
    counters: &mut AppendOnlyCounters,
) -> EngineResult<()> {
    file.write_all(bytes).map_err(carrier_io)?;
    bump(&mut counters.carrier_write_calls, 1)?;
    bump(&mut counters.carrier_bytes_written, bytes.len() as u64)?;
    Ok(())
}

fn write_zeros<W: Write>(
    file: &mut W,
    mut length: u64,
    counters: &mut AppendOnlyCounters,
) -> EngineResult<()> {
    let zeros = [0_u8; 4096];
    while length != 0 {
        let amount = length.min(zeros.len() as u64) as usize;
        write_all_counted(file, &zeros[..amount], counters)?;
        length -= amount as u64;
    }
    Ok(())
}

fn read_u64(bytes: &[u8]) -> EngineResult<u64> {
    let array: [u8; 8] = bytes
        .try_into()
        .map_err(|_| EngineError::InvalidRecord("u64"))?;
    Ok(u64::from_be_bytes(array))
}

fn put_u64(payload: &mut [u8], cursor: &mut usize, value: u64) {
    payload[*cursor..*cursor + 8].copy_from_slice(&value.to_be_bytes());
    *cursor += 8;
}

fn take_u64(payload: &[u8], cursor: &mut usize) -> EngineResult<u64> {
    let end = cursor.checked_add(8).ok_or(EngineError::CounterOverflow)?;
    let value = read_u64(&payload[*cursor..end])?;
    *cursor = end;
    Ok(value)
}

fn take_id(payload: &[u8], cursor: &mut usize) -> EngineResult<ObjectId> {
    let end = cursor.checked_add(32).ok_or(EngineError::CounterOverflow)?;
    let id = ObjectId::from_bytes(&payload[*cursor..end]).map_err(EngineError::Core)?;
    *cursor = end;
    Ok(id)
}

fn delta_record_len(delta: &DeltaRecord) -> EngineResult<u64> {
    let payload = u64::try_from(delta.payload.len()).map_err(|_| EngineError::CounterOverflow)?;
    64_u64
        .checked_add(payload)
        .and_then(|value| value.checked_add(if delta.parent.is_some() { 32 } else { 0 }))
        .ok_or(EngineError::CounterOverflow)
}

fn root_record_len(root: &RootRecord) -> EngineResult<u64> {
    64_u64
        .checked_add(if root.parent.is_some() { 32 } else { 0 })
        .ok_or(EngineError::CounterOverflow)
}

fn bump(value: &mut u64, amount: u64) -> EngineResult<()> {
    *value = value
        .checked_add(amount)
        .ok_or(EngineError::CounterOverflow)?;
    Ok(())
}

fn add_elapsed(counter: &mut u64, start: Instant) -> EngineResult<()> {
    let nanos =
        u64::try_from(start.elapsed().as_nanos()).map_err(|_| EngineError::CounterOverflow)?;
    bump(counter, nanos)
}

fn bump_frame_bytes(
    counters: &mut AppendOnlyCounters,
    kind: FrameKind,
    bytes: u64,
) -> EngineResult<()> {
    let counter = match kind {
        FrameKind::Object => &mut counters.object_frame_bytes_written,
        FrameKind::IndexPage | FrameKind::IndexRoot => &mut counters.index_frame_bytes_written,
        FrameKind::Root => &mut counters.root_frame_bytes_written,
        FrameKind::Delta => &mut counters.delta_frame_bytes_written,
        FrameKind::CommitMarker => &mut counters.marker_frame_bytes_written,
    };
    bump(counter, bytes)
}

fn carrier_io(error: io::Error) -> EngineError {
    match error.kind() {
        io::ErrorKind::PermissionDenied => EngineError::CarrierPermissionDenied(error.to_string()),
        io::ErrorKind::StorageFull => EngineError::CarrierNoSpace(error.to_string()),
        io::ErrorKind::UnexpectedEof => EngineError::ShortRead {
            expected: 1,
            actual: 0,
        },
        _ => EngineError::CarrierIo(error.to_string()),
    }
}

fn carrier_lock(error: io::Error) -> EngineError {
    if error.kind() == io::ErrorKind::WouldBlock {
        EngineError::CarrierBusy
    } else {
        carrier_io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_core::{
        cdc::{FastCdc, MAXIMUM_CHUNK_BYTES},
        encode_object, CanonicalName, DirectoryEntry, Object, ObjectReference,
    };
    use std::io::Cursor;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "layerfs-append-{label}-{}-{stamp}.log",
            std::process::id()
        ))
    }

    fn directory_object() -> (ObjectId, Vec<u8>) {
        let canonical = encode_object(&Object::Directory(Vec::new())).expect("encode");
        (ObjectId::for_bytes(&canonical), canonical)
    }

    fn bytes_object(byte: u8) -> (ObjectId, Vec<u8>) {
        let canonical = encode_object(&Object::bytes(vec![byte]).expect("bytes")).expect("encode");
        (ObjectId::for_bytes(&canonical), canonical)
    }

    fn rewrite_frame_payload(path: &Path, offset: u64, mutate: impl FnOnce(&mut [u8])) {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .expect("open for tamper");
        let mut header_bytes = [0_u8; HEADER_LEN];
        file.read_at(&mut header_bytes, offset).expect("header");
        let mut header = FrameHeader::decode(&header_bytes, offset).expect("decode header");
        let length = usize::try_from(header.payload_len).expect("payload length");
        let mut payload = vec![0_u8; length];
        file.read_at(
            &mut payload,
            offset
                .checked_add(HEADER_LEN as u64)
                .expect("payload offset"),
        )
        .expect("payload");
        mutate(&mut payload);
        let mut hasher = blake3::Hasher::new();
        hasher.update(&header.prefix());
        hasher.update(&payload);
        header
            .checksum
            .copy_from_slice(hasher.finalize().as_bytes());
        file.write_at(&header.encode(), offset)
            .expect("header write");
        file.write_at(
            &payload,
            offset
                .checked_add(HEADER_LEN as u64)
                .expect("payload offset"),
        )
        .expect("payload write");
    }

    fn rewrite_marker(path: &Path, mutate: impl FnOnce(&mut [u8])) {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .expect("open marker");
        let mut counters = AppendOnlyCounters::default();
        let marker = scan_log(&file)
            .expect("scan marker")
            .visible
            .expect("marker");
        let mut header = read_header(&file, marker.offset, &mut counters).expect("marker header");
        let mut payload = read_payload(&file, header, &mut counters).expect("marker payload");
        mutate(&mut payload);
        let digest_start = payload.len() - MARKER_DIGEST_LEN;
        let digest = marker_digest(&payload[..digest_start]);
        payload[digest_start..].copy_from_slice(&digest);
        let mut hasher = blake3::Hasher::new();
        hasher.update(&header.prefix());
        hasher.update(&payload);
        header
            .checksum
            .copy_from_slice(hasher.finalize().as_bytes());
        file.write_at(&header.encode(), marker.offset)
            .expect("marker header write");
        file.write_at(&payload, marker.offset + HEADER_LEN as u64)
            .expect("marker payload write");
    }

    fn commit_one(path: &Path) -> (ObjectId, Vec<u8>, RootRecord, DeltaRecord) {
        let (directory_id, directory_bytes) = directory_object();
        let engine = AppendOnlyEngine::open(path).expect("open");
        let mut capture = engine.begin_capture(None).expect("capture");
        assert_eq!(
            capture.put_object_if_absent(directory_id, &directory_bytes),
            Ok(PutOutcome::Created)
        );
        let root = RootRecord {
            id: ObjectId::for_bytes(b"root"),
            directory_object: directory_id,
            parent: None,
        };
        let delta = DeltaRecord::new(None, root.id, b"delta".to_vec());
        capture.write_delta(&delta).expect("delta");
        capture.commit_root(root.clone()).expect("commit");
        (directory_id, directory_bytes, root, delta)
    }

    #[test]
    fn append_only_round_trip_reuses_and_recovers() {
        let path = temp_path("round-trip");
        let (directory_id, directory_bytes, root, delta) = commit_one(&path);
        let engine = AppendOnlyEngine::open(&path).expect("reopen");
        assert_eq!(engine.load_visible_root().expect("root"), Some(root.id));
        assert_eq!(engine.load_root(root.id).expect("load root"), root);
        assert_eq!(engine.load_delta(delta.id).expect("load delta"), delta);
        assert_eq!(
            engine.object_length(directory_id).expect("length"),
            directory_bytes.len() as u64
        );
        assert_eq!(
            engine.read_object_range(directory_id, 0..4).expect("range"),
            directory_bytes[..4]
        );
        assert_eq!(
            engine
                .read_object_range(directory_id, 2..directory_bytes.len() as u64 - 1)
                .expect("middle range"),
            directory_bytes[2..directory_bytes.len() - 1]
        );
        assert_eq!(
            engine
                .read_object_range(
                    directory_id,
                    directory_bytes.len() as u64..directory_bytes.len() as u64
                )
                .expect("empty range"),
            Vec::<u8>::new()
        );
        assert!(matches!(
            engine.read_object_range(directory_id, 4..2),
            Err(EngineError::InvalidRange { .. })
        ));
        assert!(matches!(
            engine.read_object_range(directory_id, 0..directory_bytes.len() as u64 + 1),
            Err(EngineError::InvalidRange { .. })
        ));
        let mut capture = engine.begin_capture(Some(root.id)).expect("capture");
        assert_eq!(
            capture.put_object_if_absent(directory_id, &directory_bytes),
            Ok(PutOutcome::Reused)
        );
        drop(capture);
        std::fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn append_only_writer_authority_is_process_wide() {
        let path = temp_path("writer-conflict");
        let first = AppendOnlyEngine::open(&path).expect("first open");
        assert!(matches!(
            AppendOnlyEngine::open(&path),
            Err(EngineError::CarrierBusy)
        ));
        drop(first);
        let second = AppendOnlyEngine::open(&path).expect("lock released");
        let _lock_wait_ns = second.counters().expect("counters").writer_lock_wait_ns;
        std::fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn append_only_publication_has_one_marker_one_sync_and_exact_residue() {
        let path = temp_path("accounting");
        let (_, _, root, _) = commit_one(&path);
        let engine = AppendOnlyEngine::open(&path).expect("reopen");
        let counters = engine.counters().expect("counters");
        assert_eq!(counters.marker_attempts, 0);
        let (object_id, object_bytes) = bytes_object(17);
        let mut capture = engine.begin_capture(Some(root.id)).expect("capture");
        capture
            .put_object_if_absent(object_id, &object_bytes)
            .expect("object");
        drop(capture);
        drop(engine);

        let reopened = AppendOnlyEngine::open(&path).expect("reopen residue");
        let observation = reopened.observations().expect("observation");
        let recovery = reopened.counters().expect("recovery counters");
        assert!(observation.residue_bytes > 0);
        assert_eq!(
            observation.residue_bytes,
            observation.carrier_bytes - observation.visible_end
        );
        assert_eq!(recovery.residue_bytes, observation.residue_bytes);
        assert_eq!(reopened.load_visible_root().expect("root"), Some(root.id));
        drop(reopened);

        let path2 = temp_path("publication");
        let (directory_id, directory_bytes) = directory_object();
        let engine2 = AppendOnlyEngine::open(&path2).expect("open");
        let mut capture2 = engine2.begin_capture(None).expect("capture");
        capture2
            .put_object_if_absent(directory_id, &directory_bytes)
            .expect("directory");
        let root2 = RootRecord {
            id: ObjectId::for_bytes(b"publication-root"),
            directory_object: directory_id,
            parent: None,
        };
        let delta2 = DeltaRecord::new(None, root2.id, b"publication-delta".to_vec());
        capture2.write_delta(&delta2).expect("delta");
        capture2.commit_root(root2).expect("commit");
        let counters2 = engine2.counters().expect("counters");
        assert_eq!(counters2.marker_attempts, 1);
        assert_eq!(counters2.marker_sync_attempts, 1);
        assert_eq!(counters2.marker_sync_successes, 1);
        assert_eq!(counters2.marker_sync_failures, 0);
        assert_eq!(counters2.carrier_flush_failures, 0);
        drop(engine2);
        let file = OpenOptions::new().read(true).open(&path2).expect("file");
        let scan = scan_log(&file).expect("scan");
        assert_eq!(scan.counters.markers_recovered, 1);
        assert!(scan.visible.is_some());
        assert_eq!(scan.counters.residue_bytes, 0);
        std::fs::remove_file(path).expect("cleanup");
        std::fs::remove_file(path2).expect("cleanup");
    }

    fn assert_append_fault(point: AppendFaultPoint, fail_during_put: bool) {
        let path = temp_path("append-stage");
        let (directory_id, _, old_root, _) = commit_one(&path);
        let engine = AppendOnlyEngine::open(&path).expect("reopen");
        let (object_id, object_bytes) = bytes_object(point as u8 + 1);
        let mut capture = engine.begin_capture(Some(old_root.id)).expect("capture");
        capture.fail_at(point);
        let put = capture.put_object_if_absent(object_id, &object_bytes);
        if fail_during_put {
            assert_eq!(put, Err(EngineError::InjectedFailure("append-only fault")));
            drop(capture);
        } else {
            put.expect("object");
            let new_root = RootRecord {
                id: ObjectId::for_bytes(&[point as u8 + 20]),
                directory_object: directory_id,
                parent: Some(old_root.id),
            };
            let new_delta =
                DeltaRecord::new(Some(old_root.id), new_root.id, vec![point as u8 + 30]);
            capture.write_delta(&new_delta).expect("delta");
            assert_eq!(
                capture.commit_root(new_root),
                Err(EngineError::InjectedFailure("append-only fault"))
            );
        }
        drop(engine);
        let reopened = AppendOnlyEngine::open(&path).expect("reopen after fault");
        assert_eq!(
            reopened.load_visible_root().expect("visible root"),
            Some(old_root.id)
        );
        std::fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn append_only_append_stage_faults_leave_the_old_marker_visible() {
        assert_append_fault(AppendFaultPoint::AfterObjectAppend, true);
        assert_append_fault(AppendFaultPoint::AfterIndexAppend, true);
        assert_append_fault(AppendFaultPoint::AfterIndexRootAppend, false);
        assert_append_fault(AppendFaultPoint::AfterDeltaAppend, false);
        assert_append_fault(AppendFaultPoint::AfterRootAppend, false);
    }

    #[test]
    fn append_only_counter_and_error_mapping_are_typed() {
        assert!(matches!(
            carrier_io(io::Error::new(io::ErrorKind::PermissionDenied, "denied")),
            EngineError::CarrierPermissionDenied(_)
        ));
        assert!(matches!(
            carrier_io(io::Error::new(io::ErrorKind::StorageFull, "full")),
            EngineError::CarrierNoSpace(_)
        ));
        assert!(matches!(
            carrier_io(io::Error::new(io::ErrorKind::UnexpectedEof, "short")),
            EngineError::ShortRead { .. }
        ));
        assert!(matches!(
            carrier_lock(io::Error::new(io::ErrorKind::WouldBlock, "busy")),
            EngineError::CarrierBusy
        ));

        let path = temp_path("short-read");
        std::fs::write(&path, [1_u8]).expect("write");
        let file = File::open(&path).expect("open");
        let mut output = [0_u8; 2];
        assert!(matches!(
            read_exact_at(&file, 0, &mut output, &mut AppendOnlyCounters::default()),
            Err(EngineError::ShortRead {
                expected: 2,
                actual: 1
            })
        ));
        std::fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn append_only_recovery_rejects_header_checksum_and_predecessor_tamper() {
        let cases = ["header", "checksum"];
        for case in cases {
            let path = temp_path(case);
            let _ = commit_one(&path);
            if case == "header" {
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path)
                    .expect("open");
                file.write_at(b"BAD!", 0).expect("header tamper");
            } else {
                rewrite_frame_payload(&path, 0, |payload| payload[0] ^= 1);
            }
            assert!(matches!(
                AppendOnlyEngine::open(&path),
                Err(EngineError::CarrierRecoveryMalformed("frame header"))
                    | Err(EngineError::CarrierIntegrity("frame checksum"))
                    | Err(EngineError::CarrierIntegrity("capture evidence"))
            ));
            std::fs::remove_file(path).expect("cleanup");
        }

        for case in ["predecessor-tail", "header-tail", "checksum-tail"] {
            let path = temp_path(case);
            let (_, _, root, _) = commit_one(&path);
            let engine = AppendOnlyEngine::open(&path).expect("reopen");
            let (id, bytes) = bytes_object(91);
            let mut capture = engine.begin_capture(Some(root.id)).expect("capture");
            capture.put_object_if_absent(id, &bytes).expect("object");
            drop(capture);
            drop(engine);
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .expect("open tail");
            let marker = scan_log(&file).expect("scan").visible.expect("marker");
            let mut counters = AppendOnlyCounters::default();
            let object_offset = marker.end;
            let mut header = read_header(&file, object_offset, &mut counters).expect("tail header");
            match case {
                "predecessor-tail" => {
                    header.previous_offset = 0;
                    let payload = read_payload(&file, header, &mut counters).expect("tail payload");
                    let mut hasher = blake3::Hasher::new();
                    hasher.update(&header.prefix());
                    hasher.update(&payload);
                    header
                        .checksum
                        .copy_from_slice(hasher.finalize().as_bytes());
                    file.write_at(&header.encode(), object_offset)
                        .expect("header");
                }
                "header-tail" => {
                    file.write_at(b"BAD!", object_offset)
                        .expect("header tamper");
                }
                "checksum-tail" => {
                    header.checksum[0] ^= 1;
                    file.write_at(&header.encode(), object_offset)
                        .expect("checksum tamper");
                }
                _ => unreachable!(),
            }
            drop(file);
            let reopened = AppendOnlyEngine::open(&path).expect("retain old marker");
            assert_eq!(
                reopened.load_visible_root().expect("visible root"),
                Some(root.id)
            );
            let observation = reopened.observations().expect("observation");
            assert_eq!(
                observation.residue_bytes,
                observation.carrier_bytes - observation.visible_end
            );
            let counters = reopened.counters().expect("recovery counters");
            if case == "checksum-tail" {
                assert!(counters.recovery_integrity_bytes > 0);
            } else {
                assert!(counters.recovery_malformed_bytes > 0);
            }
            assert!(matches!(
                reopened.begin_capture(Some(root.id)),
                Err(EngineError::EnginePoisoned)
            ));
            drop(reopened);
            std::fs::remove_file(path).expect("cleanup");
        }
    }

    #[test]
    fn append_only_recovery_rejects_index_root_delta_root_and_marker_tamper() {
        for kind in [FrameKind::IndexRoot, FrameKind::Delta, FrameKind::Root] {
            let path = temp_path("frame-tamper");
            let _ = commit_one(&path);
            let file = OpenOptions::new().read(true).open(&path).expect("open");
            let marker = scan_log(&file).expect("scan").visible.expect("marker");
            let offset = match kind {
                FrameKind::IndexRoot => marker.index_root_offset,
                FrameKind::Delta => marker.delta_offset,
                FrameKind::Root => marker.root_offset,
                _ => unreachable!(),
            };
            drop(file);
            rewrite_frame_payload(&path, offset, |payload| payload[0] ^= 1);
            assert!(matches!(
                AppendOnlyEngine::open(&path),
                Err(EngineError::CarrierIntegrity("capture evidence"))
                    | Err(EngineError::CarrierIntegrity("frame checksum"))
            ));
            std::fs::remove_file(path).expect("cleanup");
        }

        let path = temp_path("marker-tamper");
        let _ = commit_one(&path);
        rewrite_marker(&path, |payload| payload[177] ^= 1);
        assert!(matches!(
            AppendOnlyEngine::open(&path),
            Err(EngineError::CarrierIntegrity("capture evidence"))
        ));
        std::fs::remove_file(path).expect("cleanup");

        let path = temp_path("marker-reference");
        let _ = commit_one(&path);
        rewrite_marker(&path, |payload| {
            let root_offset_start = 16 + 8 + 8 + 33 + 32 + 32 + 8 + 8;
            payload[root_offset_start..root_offset_start + 8]
                .copy_from_slice(&u64::MAX.to_be_bytes());
        });
        assert!(matches!(
            AppendOnlyEngine::open(&path),
            Err(EngineError::CarrierRecoveryMalformed("commit marker chain"))
        ));
        std::fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn append_only_authenticates_unequal_index_occupants_and_recursive_closure() {
        let child = Object::bytes(b"child".to_vec()).expect("child");
        let child_bytes = encode_object(&child).expect("child encoding");
        let child_id = ObjectId::for_bytes(&child_bytes);
        let directory = Object::Directory(vec![DirectoryEntry::new(
            CanonicalName::new("child").expect("name"),
            ObjectReference::new(layerfs_core::ObjectKind::Bytes, child_id),
        )]);
        let directory_bytes = encode_object(&directory).expect("directory encoding");
        let directory_id = ObjectId::for_bytes(&directory_bytes);
        let path = temp_path("closure");
        let engine = AppendOnlyEngine::open(&path).expect("open");
        let mut capture = engine.begin_capture(None).expect("capture");
        capture
            .put_object_if_absent(child_id, &child_bytes)
            .expect("child");
        capture
            .put_object_if_absent(directory_id, &directory_bytes)
            .expect("directory");
        let root = RootRecord {
            id: ObjectId::for_bytes(b"closure-root"),
            directory_object: directory_id,
            parent: None,
        };
        let delta = DeltaRecord::new(None, root.id, b"closure-delta".to_vec());
        capture.write_delta(&delta).expect("delta");
        capture.commit_root(root.clone()).expect("commit");
        let marker = engine.state.lock().expect("state").visible.expect("marker");
        let heads = read_index_root_state(
            &mut engine.state.lock().expect("state"),
            marker.index_root_offset,
        )
        .expect("heads");
        let locator = lookup_index_file(
            engine.state.lock().expect("state").file.get_ref(),
            &heads,
            child_id,
            marker.offset,
            &mut AppendOnlyCounters::default(),
        )
        .expect("lookup")
        .expect("child locator");
        let mut unequal = locator;
        unequal.canonical_len += 1;
        assert_eq!(
            authenticate_object(
                engine.state.lock().expect("state").file.get_ref(),
                unequal,
                child_id,
                None,
                marker.offset,
                &mut AppendOnlyCounters::default(),
            ),
            Err(EngineError::InvalidRecord("object locator"))
        );
        drop(engine);
        let reopened = AppendOnlyEngine::open(&path).expect("reopen");
        assert_eq!(reopened.load_visible_root().expect("root"), Some(root.id));
        assert_eq!(reopened.load_delta(delta.id).expect("delta"), delta);
        std::fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn append_only_public_reuse_reauthenticates_persisted_bytes() {
        let child = Object::bytes(b"persisted child".to_vec()).expect("child");
        let child_bytes = encode_object(&child).expect("child encoding");
        let child_id = ObjectId::for_bytes(&child_bytes);
        let directory = Object::Directory(vec![DirectoryEntry::new(
            CanonicalName::new("child").expect("name"),
            ObjectReference::new(ObjectKind::Bytes, child_id),
        )]);
        let directory_bytes = encode_object(&directory).expect("directory encoding");
        let directory_id = ObjectId::for_bytes(&directory_bytes);
        let path = temp_path("public-reuse-tamper");
        let engine = AppendOnlyEngine::open(&path).expect("open");
        let mut capture = engine.begin_capture(None).expect("capture");
        capture
            .put_object_if_absent(child_id, &child_bytes)
            .expect("child");
        capture
            .put_object_if_absent(directory_id, &directory_bytes)
            .expect("directory");
        let root = RootRecord {
            id: ObjectId::for_bytes(b"public-reuse-root"),
            directory_object: directory_id,
            parent: None,
        };
        capture
            .write_delta(&DeltaRecord::new(
                None,
                root.id,
                b"public-reuse-delta".to_vec(),
            ))
            .expect("delta");
        capture.commit_root(root.clone()).expect("commit");
        drop(engine);

        let file = OpenOptions::new().read(true).open(&path).expect("scan");
        let marker = scan_log(&file).expect("scan").visible.expect("marker");
        let heads = read_index_root(
            &file,
            marker.index_root_offset,
            marker.offset,
            &mut AppendOnlyCounters::default(),
        )
        .expect("index root");
        let locator = lookup_index_file(
            &file,
            &heads,
            child_id,
            marker.offset,
            &mut AppendOnlyCounters::default(),
        )
        .expect("lookup")
        .expect("child locator");
        drop(file);

        let engine = AppendOnlyEngine::open(&path).expect("reopen before tamper");
        let mut capture = engine.begin_capture(Some(root.id)).expect("reuse capture");
        rewrite_frame_payload(&path, locator.offset, |payload| payload[41] ^= 1);
        assert!(matches!(
            capture.put_object_if_absent(child_id, &child_bytes),
            Err(EngineError::IdentityMismatch { expected, actual })
                if expected == child_id && actual != child_id
        ));
        assert_eq!(capture.counters().objects_reused, 0);
        drop(capture);
        assert!(matches!(
            engine.begin_capture(Some(root.id)),
            Err(EngineError::EnginePoisoned)
        ));
        drop(engine);
        assert!(matches!(
            AppendOnlyEngine::open(&path),
            Err(EngineError::CarrierIntegrity("capture evidence"))
        ));
        std::fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn append_only_persists_phase2_cdc_and_phase3_parented_delta_reopen() {
        let mut source = Vec::with_capacity(100_000);
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        for _ in 0..100_000 {
            state ^= state.wrapping_shl(7);
            state ^= state.wrapping_shr(9);
            state ^= state.wrapping_shl(8);
            source.push(state as u8);
        }
        let path = temp_path("cdc-cow");
        let (directory_id, directory_bytes) = directory_object();
        let engine = AppendOnlyEngine::open(&path).expect("open");
        let mut capture = engine.begin_capture(None).expect("capture");
        capture
            .put_object_if_absent(directory_id, &directory_bytes)
            .expect("directory");
        let mut chunks = Vec::new();
        let mut chunk_lengths = Vec::new();
        let cdc = FastCdc::new()
            .scan(Cursor::new(&source), |chunk| {
                assert!(!chunk.is_empty() && chunk.len() <= MAXIMUM_CHUNK_BYTES);
                chunk_lengths.push(chunk.len());
                let canonical = encode_object(&Object::bytes(chunk.to_vec())?)?;
                let id = ObjectId::for_bytes(&canonical);
                capture
                    .put_object_if_absent(id, &canonical)
                    .unwrap_or_else(|error| panic!("CDC append failed: {error:?}"));
                chunks.push((id, canonical));
                Ok(())
            })
            .expect("cdc");
        assert_eq!(cdc.bytes_scanned, source.len() as u64);
        assert_eq!(
            chunk_lengths,
            [16_396, 17_093, 16_413, 20_273, 19_016, 10_809]
        );
        assert_eq!(
            chunks
                .iter()
                .map(|(_, bytes)| bytes.len() - 13)
                .sum::<usize>(),
            source.len()
        );
        let first_root = RootRecord {
            id: ObjectId::for_bytes(b"cdc-root-1"),
            directory_object: directory_id,
            parent: None,
        };
        let first_delta = DeltaRecord::new(None, first_root.id, b"delta-1".to_vec());
        capture.write_delta(&first_delta).expect("delta");
        capture.commit_root(first_root.clone()).expect("commit");

        let mut second_capture = engine
            .begin_capture(Some(first_root.id))
            .expect("second capture");
        let second_root = RootRecord {
            id: ObjectId::for_bytes(b"cdc-root-2"),
            directory_object: directory_id,
            parent: Some(first_root.id),
        };
        let second_delta =
            DeltaRecord::new(Some(first_root.id), second_root.id, b"delta-2".to_vec());
        second_capture.write_delta(&second_delta).expect("delta");
        second_capture
            .commit_root(second_root.clone())
            .expect("second commit");
        drop(engine);

        let reopened = AppendOnlyEngine::open(&path).expect("reopen");
        assert_eq!(
            reopened.load_visible_root().expect("root"),
            Some(second_root.id)
        );
        assert_eq!(
            reopened.load_root(first_root.id).expect("first root"),
            first_root
        );
        assert_eq!(
            reopened.load_delta(first_delta.id).expect("first delta"),
            first_delta
        );
        assert_eq!(
            reopened.load_delta(second_delta.id).expect("second delta"),
            second_delta
        );
        for (id, canonical) in chunks {
            assert_eq!(
                reopened
                    .read_object_range(id, 0..canonical.len() as u64)
                    .expect("chunk"),
                canonical
            );
        }
        std::fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn append_only_index_cache_is_bounded_and_observable() {
        let path = temp_path("cache");
        let (directory_id, directory_bytes) = directory_object();
        let engine = AppendOnlyEngine::open(&path).expect("open");
        let mut capture = engine.begin_capture(None).expect("capture");
        capture
            .put_object_if_absent(directory_id, &directory_bytes)
            .expect("directory");
        let mut ids = Vec::new();
        for byte in 0_u8..40 {
            let canonical =
                encode_object(&Object::bytes(vec![byte]).expect("bytes")).expect("encode");
            let id = ObjectId::for_bytes(&canonical);
            capture
                .put_object_if_absent(id, &canonical)
                .expect("object");
            ids.push(id);
        }
        let root = RootRecord {
            id: ObjectId::for_bytes(b"cache-root"),
            directory_object: directory_id,
            parent: None,
        };
        capture
            .write_delta(&DeltaRecord::new(None, root.id, b"cache".to_vec()))
            .expect("delta");
        capture.commit_root(root).expect("commit");
        engine.reset_counters().expect("reset");
        for id in ids {
            assert!(engine.object_length(id).expect("length") > 0);
        }
        let counters = engine.counters().expect("counters");
        assert!(counters.index_page_reads > 0);
        assert!(counters.index_cache_evictions > 0);
        std::fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn append_only_lookup_cache_keeps_unflushed_index_pages() {
        let path = temp_path("dirty-cache-page");
        let engine = AppendOnlyEngine::open(&path).expect("open");
        let mut capture = engine.begin_capture(None).expect("capture");
        let mut objects = Vec::new();
        for byte in 0_u8..64 {
            let canonical =
                encode_object(&Object::bytes(vec![byte]).expect("bytes")).expect("encode");
            let id = ObjectId::for_bytes(&canonical);
            assert_eq!(
                capture.put_object_if_absent(id, &canonical),
                Ok(PutOutcome::Created)
            );
            objects.push((id, canonical));
        }
        for (id, canonical) in objects.iter().take(40) {
            assert_eq!(
                capture.put_object_if_absent(*id, canonical),
                Ok(PutOutcome::Reused)
            );
        }
        let (id, canonical) = objects.last().expect("last object");
        assert_eq!(
            capture.put_object_if_absent(*id, canonical),
            Ok(PutOutcome::Reused)
        );
        drop(capture);
        std::fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn append_only_faults_leave_old_head_and_sync_poison() {
        let path = temp_path("faults");
        let (directory_id, directory_bytes, root, _) = commit_one(&path);
        let engine = AppendOnlyEngine::open(&path).expect("reopen");
        let (new_object_id, new_object_bytes) = bytes_object(203);
        let mut capture = engine.begin_capture(Some(root.id)).expect("capture");
        capture.fail_at(AppendFaultPoint::Sync);
        capture
            .put_object_if_absent(new_object_id, &new_object_bytes)
            .expect("object");
        let new_root = RootRecord {
            id: ObjectId::for_bytes(b"new-root"),
            directory_object: directory_id,
            parent: Some(root.id),
        };
        let new_delta = DeltaRecord::new(Some(root.id), new_root.id, b"new-delta".to_vec());
        capture.write_delta(&new_delta).expect("delta");
        assert_eq!(
            capture.commit_root(new_root),
            Err(EngineError::DurabilityAmbiguous)
        );
        assert_eq!(engine.load_visible_root().expect("old head"), Some(root.id));
        assert!(matches!(
            engine.begin_capture(Some(root.id)),
            Err(EngineError::EnginePoisoned)
        ));
        drop(engine);
        let reopened = AppendOnlyEngine::open(&path).expect("reopen after residue");
        let visible = reopened.load_visible_root().expect("root");
        assert!(visible == Some(root.id) || visible == Some(ObjectId::for_bytes(b"new-root")));
        if visible == Some(ObjectId::for_bytes(b"new-root")) {
            assert_eq!(
                reopened.object_length(new_object_id).expect("new object"),
                new_object_bytes.len() as u64
            );
        }
        assert_eq!(
            reopened
                .read_object_range(directory_id, 0..directory_bytes.len() as u64)
                .expect("old object"),
            directory_bytes
        );
        std::fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn append_only_torn_tail_does_not_publish() {
        let path = temp_path("torn");
        let (_, _, root, _) = commit_one(&path);
        let engine = AppendOnlyEngine::open(&path).expect("reopen");
        let (directory_id, directory_bytes) = directory_object();
        let mut capture = engine.begin_capture(Some(root.id)).expect("capture");
        capture
            .put_object_if_absent(directory_id, &directory_bytes)
            .expect("object");
        let second_root = RootRecord {
            id: ObjectId::for_bytes(b"second-root"),
            directory_object: directory_id,
            parent: Some(root.id),
        };
        let second_delta =
            DeltaRecord::new(Some(root.id), second_root.id, b"second-delta".to_vec());
        capture.write_delta(&second_delta).expect("delta");
        capture.commit_root(second_root).expect("commit");
        drop(engine);
        let length = std::fs::metadata(&path).expect("metadata").len();
        let file = OpenOptions::new().write(true).open(&path).expect("open");
        file.set_len(length - 1).expect("tear");
        drop(file);
        let reopened = AppendOnlyEngine::open(&path).expect("reopen");
        assert_eq!(reopened.load_visible_root().expect("root"), Some(root.id));
        assert!(reopened.observations().expect("observation").residue_bytes > 0);
        std::fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn append_only_first_capture_torn_tail_is_typed() {
        let partial_header = MAGIC[..1].to_vec();
        let partial_payload = FrameHeader {
            kind: FrameKind::Object,
            payload_len: 1,
            generation: 1,
            previous_offset: 0,
            offset: 0,
            checksum: [0_u8; CHECKSUM_LEN],
        }
        .encode()
        .to_vec();

        for (label, bytes) in [
            ("first-partial-header", partial_header),
            ("first-partial-payload", partial_payload),
        ] {
            let path = temp_path(label);
            std::fs::write(&path, bytes).expect("write torn first frame");
            assert!(matches!(
                AppendOnlyEngine::open(&path),
                Err(EngineError::CarrierRecoveryTornTail)
            ));
            std::fs::remove_file(path).expect("cleanup");
        }
    }
}
