//! Host-private payload arenas. Disk lease/count intervals retain bytes even
//! when every referring inode page has left memory. No per-block Arc registry.
use crate::overlay_index::{ExternalOwner, Index, Limits as IndexLimits, Root};
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read};
use std::ops::Bound;
use std::os::unix::fs::{FileExt, MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

const BLOCK: u64 = 4096;
const MOVE: u64 = 64 * 1024;
const TOKEN: u8 = 1;
const COVER: u8 = 2;
const COVER_REVERSE: u8 = 3;
const LOCATION: u8 = 4;
const PHYSICAL: u8 = 5;
const RELEASE: u8 = 6;
const SOURCE: u8 = 7;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    pub physical_bytes: u64,
    pub owners: usize,
    pub readers: usize,
    pub writes: usize,
    pub index: IndexLimits,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Stats {
    pub physical_bytes: u64,
    pub reserved_bytes: u64,
    pub index_physical_bytes: u64,
    pub live_block_bytes: u64,
    pub owners: usize,
    pub readers: usize,
    pub pending_releases: usize,
    pub pending_writes: usize,
    pub interval_visits: u64,
    pub relocated_bytes: u64,
    pub reclaimed_bytes: u64,
    pub cleanup_pending: bool,
    pub recovery_index_pages: u64,
}

#[derive(Clone)]
pub(crate) struct Payload(Arc<Inner>);
struct Inner {
    index: Index,
    files: [File; 2],
    limits: Limits,
    state: Mutex<State>,
    available: Condvar,
    released: Mutex<VecDeque<u64>>,
    maintenance: Mutex<()>,
    neighbor_visits: AtomicU64,
}
struct State {
    root: Root,
    next: u64,
    tails: [u64; 2],
    active: usize,
    evacuating: Option<usize>,
    moving: bool,
    truncate: Option<(usize, u64)>,
    sweep: bool,
    owners: usize,
    release_jobs: usize,
    writes: Vec<Reservation>,
    readers: Vec<Option<(usize, u64)>>,
    live_bytes: u64,
    visits: u64,
    relocated: u64,
    reclaimed: u64,
    #[cfg(test)]
    fail_after: Option<u64>,
}
#[derive(Clone, Copy)]
struct Reservation {
    id: u64,
    arena: usize,
    start: u64,
    len: u64,
    failed: bool,
}
#[derive(Clone, Copy)]
struct Token {
    source: u64,
    start: u64,
    len: u64,
    refs: u64,
    cursor: u64,
}
#[derive(Clone, Copy)]
struct Location {
    source: u64,
    start: u64,
    len: u64,
    arena: usize,
    offset: u64,
}

/// One admitted in-memory owner. Persisted metadata must independently acquire
/// `token()` through Index::set_owned; storing a numeric ID alone owns nothing.
pub(crate) struct OwnedRange {
    payload: Payload,
    token: u64,
    range: Token,
}
impl OwnedRange {
    pub(crate) fn token(&self) -> u64 {
        self.token
    }
    pub(crate) fn len(&self) -> u64 {
        self.range.len
    }
    pub(crate) fn source(&self) -> u64 {
        self.range.source
    }
    pub(crate) fn offset(&self) -> u64 {
        self.range.start
    }
    pub(crate) fn subrange(&self, offset: u64, len: u64) -> io::Result<Self> {
        self.payload.subrange(self.token, offset, len)
    }
    pub(crate) fn read_exact_at(&self, output: &mut [u8], offset: u64) -> io::Result<()> {
        self.payload.read_exact_at(self.token, output, offset)
    }
}
impl Drop for OwnedRange {
    fn drop(&mut self) {
        // The owner slot was reserved when the token was allocated. The last
        // external page may also retain the token; this releases only our ref.
        self.payload
            .0
            .released
            .lock()
            .unwrap()
            .push_back(self.token);
    }
}
struct PhysicalLease {
    payload: Payload,
    slot: usize,
    arena: usize,
    offset: u64,
    len: usize,
}
impl Drop for PhysicalLease {
    fn drop(&mut self) {
        self.payload.0.state.lock().unwrap().readers[self.slot] = None;
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, message)
}
fn corrupt(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}
fn quota(message: &str) -> io::Error {
    io::Error::new(ErrorKind::StorageFull, message)
}
fn aligned(value: u64) -> io::Result<u64> {
    value
        .checked_add(BLOCK - 1)
        .map(|v| v / BLOCK * BLOCK)
        .ok_or_else(|| invalid("payload range overflow"))
}
fn key(kind: u8, first: u64, second: u64) -> Vec<u8> {
    [
        vec![kind],
        first.to_be_bytes().to_vec(),
        second.to_be_bytes().to_vec(),
    ]
    .concat()
}
fn words(values: &[u64]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}
fn word(bytes: &[u8], at: usize) -> io::Result<u64> {
    Ok(u64::from_be_bytes(
        bytes
            .get(at * 8..at * 8 + 8)
            .ok_or_else(|| corrupt("payload record"))?
            .try_into()
            .unwrap(),
    ))
}
impl Token {
    fn encode(self) -> Vec<u8> {
        words(&[self.source, self.start, self.len, self.refs, self.cursor])
    }
    fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() != 40 {
            return Err(corrupt("payload token record"));
        }
        Ok(Self {
            source: word(bytes, 0)?,
            start: word(bytes, 1)?,
            len: word(bytes, 2)?,
            refs: word(bytes, 3)?,
            cursor: word(bytes, 4)?,
        })
    }
}

impl Payload {
    pub(crate) fn temporary(directory: &Path, limits: Limits) -> io::Result<Self> {
        if limits.physical_bytes < 2 * MOVE
            || limits.owners == 0
            || limits.readers == 0
            || limits.writes == 0
            || limits.index.max_pages <= catalog_pages(limits.index.max_pages, 48)
        {
            return Err(invalid("payload limits"));
        }
        let mut writes = Vec::new();
        writes
            .try_reserve_exact(limits.writes)
            .map_err(|_| quota("payload writer slots"))?;
        let mut readers = Vec::new();
        readers
            .try_reserve_exact(limits.readers)
            .map_err(|_| quota("payload reader slots"))?;
        readers.resize(limits.readers, None);
        let mut released = VecDeque::new();
        released
            .try_reserve_exact(limits.owners)
            .map_err(|_| quota("payload release slots"))?;
        let index = Index::temporary(directory, limits.index)?;
        let root = index.empty_root()?;
        let files = [arena(directory)?, arena(directory)?];
        Ok(Self(Arc::new(Inner {
            index,
            files,
            limits,
            state: Mutex::new(State {
                root,
                next: 1,
                tails: [0, 0],
                active: 0,
                evacuating: None,
                moving: false,
                truncate: None,
                sweep: false,
                owners: 0,
                release_jobs: 0,
                writes,
                readers,
                live_bytes: 0,
                visits: 0,
                relocated: 0,
                reclaimed: 0,
                #[cfg(test)]
                fail_after: None,
            }),
            available: Condvar::new(),
            released: Mutex::new(released),
            maintenance: Mutex::new(()),
            neighbor_visits: AtomicU64::new(0),
        })))
    }

    pub(crate) fn write_from(&self, reader: &mut impl Read, len: u64) -> io::Result<OwnedRange> {
        if len == 0 {
            return Err(invalid("empty payload extent"));
        }
        let allocated = aligned(len)?;
        let reservation = {
            let mut state = self.0.state.lock().unwrap();
            while state.moving {
                state = self.0.available.wait(state).unwrap();
            }
            if state
                .truncate
                .is_some_and(|(arena, _)| arena == state.active)
            {
                return Err(quota("payload cleanup pending"));
            }
            if state.owners + state.writes.len() >= self.0.limits.owners
                || state.writes.len() >= self.0.limits.writes
            {
                return Err(quota("payload owner/write admission"));
            }
            self.reserve(&state, allocated, false)?;
            self.reserve_catalog(&state, 6, false)?;
            let id = state.next;
            state.next = id
                .checked_add(1)
                .ok_or_else(|| quota("payload identities"))?;
            let arena = state.active;
            let reservation = Reservation {
                id,
                arena,
                start: state.tails[arena],
                len: allocated,
                failed: false,
            };
            state.tails[arena] += allocated;
            state.writes.push(reservation);
            reservation
        };
        let write = self.copy_input(
            reservation.arena,
            reservation.start,
            reader,
            len,
            allocated - len,
        );
        let mut state = self.0.state.lock().unwrap();
        let published = write.and_then(|()| {
            self.reserve(&state, 0, false)?;
            self.reserve_catalog(&state, 0, false)?;
            let token = Token {
                source: reservation.id,
                start: 0,
                len,
                refs: 1,
                cursor: 0,
            };
            let location = Location {
                source: reservation.id,
                start: 0,
                len: allocated,
                arena: reservation.arena,
                offset: reservation.start,
            };
            let root = self.put_location(&state.root, location)?;
            let root = self.put_cover(&root, token.source, 0, allocated, 1)?;
            let root = self
                .0
                .index
                .set(&root, &key(SOURCE, token.source, 0), &words(&[len, 1]))?;
            let root = self
                .0
                .index
                .set(&root, &key(TOKEN, reservation.id, 0), &token.encode())?;
            state.root = root;
            state.live_bytes += allocated;
            state.owners += 1;
            Ok(OwnedRange {
                payload: self.clone(),
                token: reservation.id,
                range: token,
            })
        });
        self.finish_write(&mut state, reservation, &published)?;
        published
    }

    fn copy_input(
        &self,
        arena: usize,
        start: u64,
        reader: &mut impl Read,
        len: u64,
        padding: u64,
    ) -> io::Result<()> {
        let file = &self.0.files[arena];
        let mut buffer = [0_u8; MOVE as usize];
        let mut offset = 0;
        while offset < len {
            let count = (len - offset).min(MOVE) as usize;
            reader.read_exact(&mut buffer[..count])?;
            #[cfg(test)]
            {
                let fail = self.0.state.lock().unwrap().fail_after;
                if let Some(at) = fail.filter(|at| *at < offset + count as u64) {
                    let partial = at.saturating_sub(offset) as usize;
                    file.write_all_at(&buffer[..partial], start + offset)?;
                    self.0.state.lock().unwrap().fail_after = None;
                    return Err(io::Error::other("injected short payload write"));
                }
            }
            file.write_all_at(&buffer[..count], start + offset)?;
            offset += count as u64;
        }
        // Explicitly write only the final allocation block's padding. Do not
        // truncate another concurrent reservation with set_len here.
        if padding > 0 {
            file.write_all_at(&vec![0; padding as usize], start + len)?;
        }
        Ok(())
    }
    fn finish_write<T>(
        &self,
        state: &mut State,
        reservation: Reservation,
        published: &io::Result<T>,
    ) -> io::Result<()> {
        if published.is_ok() {
            state.writes.retain(|item| item.id != reservation.id);
        } else {
            state
                .writes
                .iter_mut()
                .find(|item| item.id == reservation.id)
                .unwrap()
                .failed = true;
            // A failed non-tail reservation stays charged and admitted until a
            // bounded evacuation reaches it; it cannot grow an unbounded list.
            state.sweep = true;
            if reservation.len == 0 {
                state.writes.retain(|item| item.id != reservation.id);
            }
            self.trim_failed(state)?;
        }
        Ok(())
    }

    pub(crate) fn append_to(
        &self,
        owner: &OwnedRange,
        reader: &mut impl Read,
        len: u64,
    ) -> io::Result<Option<OwnedRange>> {
        if !Arc::ptr_eq(&self.0, &owner.payload.0) {
            return Err(invalid("foreign payload owner"));
        }
        self.append(owner.token, reader, len)
    }

    /// The caller retains the metadata root owning `owner` throughout this call.
    /// None leaves input unread: another source owns the physical append tail.
    pub(crate) fn append(
        &self,
        owner: u64,
        reader: &mut impl Read,
        len: u64,
    ) -> io::Result<Option<OwnedRange>> {
        if len == 0 {
            return Err(invalid("empty payload append"));
        }
        let (reservation, location, high, end) = {
            let mut state = self.0.state.lock().unwrap();
            while state.moving {
                state = self.0.available.wait(state).unwrap();
            }
            let owner = self.token(&state.root, owner)?;
            let (high, _) = self.source(&state.root, owner.source)?;
            if owner.start.checked_add(owner.len) != Some(high) {
                // A truncated/older view cannot extend the source occurrence.
                // Decide before reading: the caller must use a fresh lineage.
                return Ok(None);
            }
            let end = high
                .checked_add(len)
                .ok_or_else(|| invalid("payload source overflow"))?;
            let Some((key, bytes)) = self.floor(&state.root, LOCATION, owner.source, high - 1)?
            else {
                return Ok(None);
            };
            let location = decode_location(owner.source, !word(&key[1..], 1)?, &bytes)?;
            if location.start + location.len != aligned(high)?
                || location.arena != state.active
                || location.offset + location.len != state.tails[location.arena]
                || state
                    .truncate
                    .is_some_and(|(arena, _)| arena == state.active)
                || state.writes.iter().any(|r| !r.failed)
            {
                return Ok(None);
            }
            if state.owners + state.writes.len() >= self.0.limits.owners
                || state.writes.len() >= self.0.limits.writes
            {
                return Err(quota("payload append admission"));
            }
            let delta = aligned(end)? - aligned(high)?;
            self.reserve(&state, delta, false)?;
            self.reserve_catalog(&state, 32, false)?;
            let id = state.next;
            state.next = id
                .checked_add(1)
                .ok_or_else(|| quota("payload identities"))?;
            let reservation = Reservation {
                id,
                arena: location.arena,
                start: state.tails[location.arena],
                len: delta,
                failed: false,
            };
            state.tails[location.arena] += delta;
            state.writes.push(reservation);
            (reservation, location, high, end)
        };
        let write = self.copy_input(
            location.arena,
            location.offset + high - location.start,
            reader,
            len,
            aligned(end)? - end,
        );
        let mut state = self.0.state.lock().unwrap();
        let published = write.and_then(|()| {
            self.reserve(&state, 0, false)?;
            self.reserve_catalog(&state, 0, false)?;
            let source = self.source(&state.root, location.source)?;
            if source.0 != high {
                return Err(corrupt("payload append source changed"));
            }
            let token = Token {
                source: location.source,
                start: high,
                len,
                refs: 1,
                cursor: 0,
            };
            let mut root = self.remove_location(&state.root, location)?;
            root = self.put_location(
                &root,
                Location {
                    len: location.len + reservation.len,
                    ..location
                },
            )?;
            let begin = high / BLOCK * BLOCK;
            let boundary = aligned(high)?;
            let mut added_live = aligned(end)? - boundary;
            if begin < boundary {
                if let Some((start, stop, refs)) =
                    self.cover_at(&root, location.source, begin, &mut state.visits)?
                {
                    root = self.remove_cover(&root, location.source, start)?;
                    if start < begin {
                        root = self.put_cover(&root, location.source, start, begin, refs)?;
                    }
                    root = self.put_cover(
                        &root,
                        location.source,
                        begin,
                        boundary,
                        refs.checked_add(1)
                            .ok_or_else(|| quota("payload interval references"))?,
                    )?;
                    if boundary < stop {
                        root = self.put_cover(&root, location.source, boundary, stop, refs)?;
                    }
                } else {
                    root = self.put_cover(&root, location.source, begin, boundary, 1)?;
                    added_live += BLOCK;
                }
            }
            if boundary < aligned(end)? {
                root = self.put_cover(&root, location.source, boundary, aligned(end)?, 1)?;
            }
            root = self.0.index.set(
                &root,
                &key(SOURCE, location.source, 0),
                &words(&[
                    end,
                    source
                        .1
                        .checked_add(1)
                        .ok_or_else(|| quota("payload source owners"))?,
                ]),
            )?;
            root = self
                .0
                .index
                .set(&root, &key(TOKEN, reservation.id, 0), &token.encode())?;
            state.root = root;
            state.owners += 1;
            state.live_bytes += added_live;
            Ok(OwnedRange {
                payload: self.clone(),
                token: reservation.id,
                range: token,
            })
        });
        self.finish_write(&mut state, reservation, &published)?;
        published.map(Some)
    }

    pub(crate) fn subrange(&self, owner: u64, offset: u64, len: u64) -> io::Result<OwnedRange> {
        let mut state = self.0.state.lock().unwrap();
        let original = self.token(&state.root, owner)?;
        check_range(original, offset, len)?;
        self.own_interval(
            &mut state,
            Token {
                source: original.source,
                start: original.start + offset,
                len,
                refs: 1,
                cursor: 0,
            },
        )
    }

    /// Join only adjacent occurrences from the same never-rewound source.
    /// Old owners remain independent until their metadata pages/readers retire.
    pub(crate) fn join(&self, left: &OwnedRange, right: &OwnedRange) -> io::Result<OwnedRange> {
        if !Arc::ptr_eq(&self.0, &left.payload.0) || !Arc::ptr_eq(&self.0, &right.payload.0) {
            return Err(invalid("foreign payload owner"));
        }
        self.join_tokens(left.token, right.token)
    }
    /// Both tokens must be retained by the caller's acquired metadata root.
    pub(crate) fn join_tokens(&self, left: u64, right: u64) -> io::Result<OwnedRange> {
        let mut state = self.0.state.lock().unwrap();
        let left = self.token(&state.root, left)?;
        let right = self.token(&state.root, right)?;
        if left.source != right.source || left.start.checked_add(left.len) != Some(right.start) {
            return Err(invalid("noncontiguous payload owners"));
        }
        self.own_interval(
            &mut state,
            Token {
                source: left.source,
                start: left.start,
                len: left
                    .len
                    .checked_add(right.len)
                    .ok_or_else(|| invalid("payload length overflow"))?,
                refs: 1,
                cursor: 0,
            },
        )
    }

    fn own_interval(&self, state: &mut State, token: Token) -> io::Result<OwnedRange> {
        let len = token.len;
        if len == 0 {
            return Err(invalid("empty payload lease"));
        }
        if state.owners + state.writes.len() >= self.0.limits.owners {
            return Err(quota("payload owner admission"));
        }
        let id = state.next;
        let next = id
            .checked_add(1)
            .ok_or_else(|| quota("payload identities"))?;
        let source = self.source(&state.root, token.source)?;
        if token
            .start
            .checked_add(len)
            .is_none_or(|end| end > source.0)
        {
            return Err(invalid("payload source range"));
        }
        let begin = token.start / BLOCK * BLOCK;
        let end = aligned(token.start + len)?;
        let mut root = state.root.clone();
        let mut fragments = 0;
        let mut scan = begin;
        while scan < end {
            let (_, stop, _) = self
                .cover_at(&root, token.source, scan, &mut state.visits)?
                .ok_or_else(|| corrupt("payload lease lost coverage"))?;
            scan = stop.min(end);
            fragments += 1;
        }
        self.reserve_catalog(state, fragments * 20 + 2, false)?;
        let mut cursor = begin;
        while cursor < end {
            let (start, stop, refs) = self
                .cover_at(&root, token.source, cursor, &mut state.visits)?
                .ok_or_else(|| corrupt("payload lease lost coverage"))?;
            let stop_here = stop.min(end);
            root = self.remove_cover(&root, token.source, start)?;
            if start < cursor {
                root = self.put_cover(&root, token.source, start, cursor, refs)?;
            }
            root = self.put_cover(
                &root,
                token.source,
                cursor,
                stop_here,
                refs.checked_add(1)
                    .ok_or_else(|| quota("payload interval references"))?,
            )?;
            if stop_here < stop {
                root = self.put_cover(&root, token.source, stop_here, stop, refs)?;
            }
            cursor = stop_here;
        }
        root = self
            .0
            .index
            .set(&root, &key(TOKEN, id, 0), &token.encode())?;
        root = self.0.index.set(
            &root,
            &key(SOURCE, token.source, 0),
            &words(&[
                source.0,
                source
                    .1
                    .checked_add(1)
                    .ok_or_else(|| quota("payload source owners"))?,
            ]),
        )?;
        state.root = root;
        state.next = next;
        state.owners += 1;
        Ok(OwnedRange {
            payload: self.clone(),
            token: id,
            range: token,
        })
    }

    pub(crate) fn read_exact_at(
        &self,
        owner: u64,
        mut output: &mut [u8],
        mut offset: u64,
    ) -> io::Result<()> {
        while !output.is_empty() {
            let lease = self.physical_lease(owner, offset, output.len().min(MOVE as usize))?;
            self.0.files[lease.arena].read_exact_at(&mut output[..lease.len], lease.offset)?;
            offset += lease.len as u64;
            output = &mut output[lease.len..];
        }
        Ok(())
    }
    fn physical_lease(&self, owner: u64, offset: u64, len: usize) -> io::Result<PhysicalLease> {
        let mut state = self.0.state.lock().unwrap();
        let token = self.token(&state.root, owner)?;
        check_range(token, offset, len as u64)?;
        let logical = token.start + offset;
        // Locations use reverse offsets too, providing an indexed floor seek.
        let found = self
            .floor(&state.root, LOCATION, token.source, logical)?
            .ok_or_else(|| corrupt("payload location absent"))?;
        let location = decode_location(token.source, !word(&found.0[1..], 1)?, &found.1)?;
        if logical >= location.start + location.len {
            return Err(corrupt("payload location gap"));
        }
        let len = len.min((location.start + location.len - logical) as usize);
        let physical = location.offset + logical - location.start;
        let slot = state
            .readers
            .iter()
            .position(Option::is_none)
            .ok_or_else(|| quota("payload read leases"))?;
        state.readers[slot] = Some((location.arena, physical + len as u64));
        Ok(PhysicalLease {
            payload: self.clone(),
            slot,
            arena: location.arena,
            offset: physical,
            len,
        })
    }

    /// One bounded ownership/physical maintenance step. Large source release
    /// advances by interval; evacuation moves at most 64 KiB and then truncates.
    pub(crate) fn reclaim_step(&self) -> io::Result<bool> {
        let Ok(_maintenance) = self.0.maintenance.try_lock() else {
            return Ok(false);
        };
        let released = self.0.released.lock().unwrap().front().copied();
        if let Some(id) = released {
            self.release(id)?;
            self.0.released.lock().unwrap().pop_front();
            return Ok(true);
        }
        let mut state = self.0.state.lock().unwrap();
        if state.moving {
            return Ok(false);
        }
        if self.finish_truncate(&mut state)? {
            return Ok(true);
        }
        if state.truncate.is_some() {
            return Ok(false);
        }
        if self.trim_failed(&mut state)? {
            return Ok(true);
        }
        if self.release_interval(&mut state)? {
            return Ok(true);
        }
        if state.evacuating.is_none() {
            if !state.sweep {
                return Ok(false);
            }
            let source = state.active;
            if state.tails[1 - source] != 0 {
                return Err(corrupt("payload evacuation destination not empty"));
            }
            state.evacuating = Some(source);
            state.active = 1 - source;
            state.sweep = false;
        }
        let source_arena = state.evacuating.unwrap();
        if state.tails[source_arena] == 0 {
            state.evacuating = None;
            return Ok(true);
        }
        // Pending writers own their reservations independently. Foreground
        // writes proceed in the other arena while this source waits for them.
        if state
            .writes
            .iter()
            .any(|r| r.arena == source_arena && !r.failed)
        {
            return Ok(false);
        }
        let first = key(PHYSICAL, source_arena as u64, 0);
        let last = key(PHYSICAL, source_arena as u64 + 1, 0);
        let rows = self.0.index.scan(
            &state.root,
            Bound::Included(&first),
            Bound::Excluded(&last),
            1,
        )?;
        let Some((physical_key, value)) = rows.first() else {
            return Err(corrupt("payload tail location absent"));
        };
        let physical = !word(&physical_key[1..], 1)?;
        let location = Location {
            source: word(value, 0)?,
            start: word(value, 1)?,
            len: word(value, 2)?,
            arena: source_arena,
            offset: physical,
        };
        if physical + location.len != state.tails[source_arena] {
            return Err(corrupt("payload noncontiguous tail"));
        }
        let len = location.len.min(MOVE);
        let start = location.start + location.len - len;
        let physical_start = physical + location.len - len;
        let mut live = Vec::with_capacity((MOVE / BLOCK) as usize);
        let mut cursor = start;
        while cursor < start + len {
            state.visits += 1;
            let found = self.cover_at(&state.root, location.source, cursor, &mut 0)?;
            if let Some((_, end, _)) = found {
                let stop = end.min(start + len);
                live.push((cursor, stop - cursor));
                cursor = stop;
            } else {
                let first = key(COVER, location.source, cursor);
                let last = key(COVER, location.source + 1, 0);
                let next = self.0.index.scan(
                    &state.root,
                    Bound::Included(&first),
                    Bound::Excluded(&last),
                    1,
                )?;
                cursor = next
                    .first()
                    .map(|(key, _)| word(&key[1..], 1))
                    .transpose()?
                    .unwrap_or(start + len)
                    .min(start + len);
            }
        }
        let move_bytes: u64 = live.iter().map(|(_, len)| *len).sum();
        self.reserve(&state, move_bytes, true)?;
        self.reserve_catalog(&state, 36, true)?;
        // Keep destination rollback a tail operation. Existing reservations
        // finish first, and later writers wait only for this bounded move.
        if state.writes.iter().any(|reservation| !reservation.failed) {
            return Ok(false);
        }
        let destination = state.active;
        let destination_start = state.tails[destination];
        state.tails[destination] += move_bytes;
        state.moving = true;
        drop(state);

        let copied = (|| {
            let mut bytes = [0_u8; MOVE as usize];
            let mut verify = [0_u8; MOVE as usize];
            let mut dest = destination_start;
            for (offset, count) in &live {
                let count = *count as usize;
                self.0.files[source_arena]
                    .read_exact_at(&mut bytes[..count], physical + offset - location.start)?;
                self.0.files[destination].write_all_at(&bytes[..count], dest)?;
                self.0.files[destination].read_exact_at(&mut verify[..count], dest)?;
                if bytes[..count] != verify[..count] {
                    return Err(corrupt("payload relocation verification"));
                }
                dest += count as u64;
            }
            Ok(())
        })();
        let mut state = self.0.state.lock().unwrap();
        let installed = copied.and_then(|()| {
            self.reserve(&state, 0, true)?;
            let mut root = self.remove_location(&state.root, location)?;
            if location.len > len {
                root = self.put_location(
                    &root,
                    Location {
                        len: location.len - len,
                        ..location
                    },
                )?;
            }
            let mut dest = destination_start;
            for (offset, count) in &live {
                root = self.put_location(
                    &root,
                    Location {
                        source: location.source,
                        start: *offset,
                        len: *count,
                        arena: destination,
                        offset: dest,
                    },
                )?;
                dest += count;
            }
            state.root = root;
            state.relocated += move_bytes;
            state.truncate = Some((source_arena, physical_start));
            Ok(())
        });
        if installed.is_err() {
            state.truncate = Some((destination, destination_start));
        }
        state.moving = false;
        self.0.available.notify_all();
        self.finish_truncate(&mut state)?;
        installed.map(|()| true)
    }

    fn release_interval(&self, state: &mut State) -> io::Result<bool> {
        let rows = self.0.index.scan(
            &state.root,
            Bound::Included(&[RELEASE]),
            Bound::Excluded(&[RELEASE + 1]),
            1,
        )?;
        let Some((job, _)) = rows.first() else {
            return Ok(false);
        };
        let id = word(&job[1..], 0)?;
        self.reserve_catalog(state, 24, true)?;
        let bytes = self
            .0
            .index
            .get(&state.root, &key(TOKEN, id, 0))?
            .ok_or_else(|| corrupt("payload release token absent"))?;
        let mut token = Token::decode(&bytes)?;
        if token.refs != 0 {
            return Err(corrupt("payload live release token"));
        }
        let end = aligned(token.start + token.len)?;
        let (start, stop, refs) = self
            .cover_at(&state.root, token.source, token.cursor, &mut state.visits)?
            .ok_or_else(|| corrupt("payload release coverage absent"))?;
        let stop_here = stop.min(end);
        let released_start = token.cursor;
        let mut root = self.remove_cover(&state.root, token.source, start)?;
        if start < token.cursor {
            root = self.put_cover(&root, token.source, start, token.cursor, refs)?;
        }
        if refs > 1 {
            root = self.put_cover(&root, token.source, token.cursor, stop_here, refs - 1)?;
        }
        if stop_here < stop {
            root = self.put_cover(&root, token.source, stop_here, stop, refs)?;
        }
        token.cursor = stop_here;
        if stop_here == end {
            root = self.0.index.remove(&root, job)?;
            root = self.0.index.remove(&root, &key(TOKEN, id, 0))?;
            let source = self.source(&root, token.source)?;
            root = if source.1 == 1 {
                self.0.index.remove(&root, &key(SOURCE, token.source, 0))?
            } else {
                self.0.index.set(
                    &root,
                    &key(SOURCE, token.source, 0),
                    &words(&[source.0, source.1 - 1]),
                )?
            };
        } else {
            root = self
                .0
                .index
                .set(&root, &key(TOKEN, id, 0), &token.encode())?;
        }
        state.root = root;
        if refs == 1 {
            state.live_bytes -= stop_here - released_start;
            state.sweep = true;
        }
        if stop_here == end {
            state.owners -= 1;
            state.release_jobs -= 1;
        }
        Ok(true)
    }
    fn finish_truncate(&self, state: &mut State) -> io::Result<bool> {
        let Some((arena, len)) = state.truncate else {
            return Ok(false);
        };
        if state
            .readers
            .iter()
            .flatten()
            .any(|(at, end)| *at == arena && *end > len)
        {
            return Ok(false);
        }
        self.0.files[arena].set_len(len)?;
        state.reclaimed += state.tails[arena] - len;
        state.tails[arena] = len;
        state.truncate = None;
        Ok(true)
    }
    fn trim_failed(&self, state: &mut State) -> io::Result<bool> {
        let found = state
            .writes
            .iter()
            .find(|r| r.failed && r.start + r.len == state.tails[r.arena])
            .copied();
        let Some(reservation) = found else {
            return Ok(false);
        };
        self.0.files[reservation.arena].set_len(reservation.start)?;
        state.tails[reservation.arena] = reservation.start;
        state.writes.retain(|r| r.id != reservation.id);
        state.reclaimed += reservation.len;
        Ok(true)
    }
    fn reserve(&self, state: &State, bytes: u64, recovery: bool) -> io::Result<()> {
        let physical = physical(&self.0.files)?;
        let used = physical.max(state.tails[0].saturating_add(state.tails[1]));
        let total = used
            .checked_add(bytes)
            .and_then(|v| v.checked_add(if recovery { 0 } else { MOVE }));
        if total.is_none_or(|v| v > self.0.limits.physical_bytes) {
            return Err(quota("payload physical quota/recovery reserve"));
        }
        Ok(())
    }
    fn reserve_catalog(&self, state: &State, operations: u64, recovery: bool) -> io::Result<()> {
        let limits = self.0.limits.index;
        let pending = state.writes.iter().filter(|r| !r.failed).count() as u64 * 32;
        let needed = catalog_pages(
            limits.max_pages,
            operations + pending + if recovery { 0 } else { 40 },
        );
        if self
            .0
            .index
            .stats()?
            .live_pages
            .checked_add(needed)
            .is_none_or(|pages| pages > limits.max_pages)
        {
            return Err(quota("payload catalog recovery reserve"));
        }
        Ok(())
    }
    fn source(&self, root: &Root, id: u64) -> io::Result<(u64, u64)> {
        let bytes = self
            .0
            .index
            .get(root, &key(SOURCE, id, 0))?
            .ok_or_else(|| corrupt("payload source absent"))?;
        if bytes.len() != 16 || word(&bytes, 1)? == 0 {
            return Err(corrupt("payload source record"));
        }
        Ok((word(&bytes, 0)?, word(&bytes, 1)?))
    }
    fn token(&self, root: &Root, id: u64) -> io::Result<Token> {
        let token = self
            .0
            .index
            .get(root, &key(TOKEN, id, 0))?
            .ok_or_else(|| invalid("stale payload owner"))?;
        let token = Token::decode(&token)?;
        if token.refs == 0 {
            return Err(invalid("released payload owner"));
        }
        Ok(token)
    }
    fn floor(
        &self,
        root: &Root,
        kind: u8,
        source: u64,
        at: u64,
    ) -> io::Result<Option<(Vec<u8>, Vec<u8>)>> {
        let first = key(kind, source, !at);
        let last = key(kind, source + 1, 0);
        Ok(self
            .0
            .index
            .scan(root, Bound::Included(&first), Bound::Excluded(&last), 1)?
            .pop())
    }
    fn cover_at(
        &self,
        root: &Root,
        source: u64,
        at: u64,
        visits: &mut u64,
    ) -> io::Result<Option<(u64, u64, u64)>> {
        *visits += 1;
        let Some((key, value)) = self.floor(root, COVER_REVERSE, source, at)? else {
            return Ok(None);
        };
        let start = !word(&key[1..], 1)?;
        let end = word(&value, 0)?;
        let refs = word(&value, 1)?;
        Ok((start <= at && at < end && refs > 0).then_some((start, end, refs)))
    }
    fn put_cover(
        &self,
        root: &Root,
        source: u64,
        mut start: u64,
        mut end: u64,
        refs: u64,
    ) -> io::Result<Root> {
        let mut root = root.clone();
        if start > 0 {
            self.0.neighbor_visits.fetch_add(1, Ordering::Relaxed);
            if let Some((previous, stop, count)) =
                self.cover_at(&root, source, start - 1, &mut 0)?
            {
                if stop == start && count == refs {
                    root = self.remove_cover(&root, source, previous)?;
                    start = previous;
                }
            }
        }
        self.0.neighbor_visits.fetch_add(1, Ordering::Relaxed);
        if let Some(next) = self.0.index.get(&root, &key(COVER, source, end))? {
            if word(&next, 1)? == refs {
                root = self.remove_cover(&root, source, end)?;
                end = word(&next, 0)?;
            }
        }
        let value = words(&[end, refs]);
        let root = self
            .0
            .index
            .set(&root, &key(COVER, source, start), &value)?;
        self.0
            .index
            .set(&root, &key(COVER_REVERSE, source, !start), &value)
    }
    fn remove_cover(&self, root: &Root, source: u64, start: u64) -> io::Result<Root> {
        let root = self.0.index.remove(root, &key(COVER, source, start))?;
        self.0
            .index
            .remove(&root, &key(COVER_REVERSE, source, !start))
    }
    fn put_location(&self, root: &Root, location: Location) -> io::Result<Root> {
        let root = self.0.index.set(
            root,
            &key(LOCATION, location.source, !location.start),
            &words(&[location.len, location.arena as u64, location.offset]),
        )?;
        self.0.index.set(
            &root,
            &key(PHYSICAL, location.arena as u64, !location.offset),
            &words(&[location.source, location.start, location.len]),
        )
    }
    fn remove_location(&self, root: &Root, location: Location) -> io::Result<Root> {
        let root = self
            .0
            .index
            .remove(root, &key(LOCATION, location.source, !location.start))?;
        self.0.index.remove(
            &root,
            &key(PHYSICAL, location.arena as u64, !location.offset),
        )
    }
    pub(crate) fn stats(&self) -> io::Result<Stats> {
        let state = self.0.state.lock().unwrap();
        let pending_releases = state.release_jobs + self.0.released.lock().unwrap().len();
        Ok(Stats {
            physical_bytes: physical(&self.0.files)?,
            reserved_bytes: state.tails.iter().sum(),
            index_physical_bytes: self.0.index.stats()?.physical_bytes,
            live_block_bytes: state.live_bytes,
            owners: state.owners,
            readers: state.readers.iter().flatten().count(),
            pending_releases,
            pending_writes: state.writes.len(),
            interval_visits: state.visits + self.0.neighbor_visits.load(Ordering::Relaxed),
            relocated_bytes: state.relocated,
            reclaimed_bytes: state.reclaimed,
            cleanup_pending: pending_releases != 0
                || state.writes.iter().any(|reservation| reservation.failed)
                || state.sweep
                || state.evacuating.is_some()
                || state.truncate.is_some(),
            recovery_index_pages: catalog_pages(self.0.limits.index.max_pages, 40),
        })
    }
}

impl ExternalOwner for Payload {
    fn retain(&self, id: u64) -> io::Result<()> {
        let mut state = self.0.state.lock().unwrap();
        let mut token = self.token(&state.root, id)?;
        self.reserve_catalog(&state, 1, false)?;
        token.refs = token
            .refs
            .checked_add(1)
            .ok_or_else(|| quota("payload owner references"))?;
        let root = self
            .0
            .index
            .set(&state.root, &key(TOKEN, id, 0), &token.encode())?;
        state.root = root;
        Ok(())
    }
    fn release(&self, id: u64) -> io::Result<()> {
        let mut state = self.0.state.lock().unwrap();
        let mut token = self.token(&state.root, id)?;
        self.reserve_catalog(&state, 2, true)?;
        token.refs -= 1;
        token.cursor = token.start / BLOCK * BLOCK;
        let mut root = self
            .0
            .index
            .set(&state.root, &key(TOKEN, id, 0), &token.encode())?;
        if token.refs == 0 {
            root = self.0.index.set(&root, &key(RELEASE, id, 0), &[])?;
        }
        state.root = root;
        if token.refs == 0 {
            state.release_jobs += 1;
        }
        Ok(())
    }
}
fn catalog_pages(max_pages: u64, operations: u64) -> u64 {
    // Minimum internal fanout is four. Include two boundary levels, split
    // siblings, and a root. Forty updates cover sixteen relocated intervals
    // plus source removal/prefix replacement and a bounded ownership release.
    let height = (64 - max_pages.max(1).leading_zeros() as u64).div_ceil(2) + 2;
    operations.saturating_mul(2 * height + 3)
}
fn check_range(token: Token, offset: u64, len: u64) -> io::Result<()> {
    if offset.checked_add(len).is_none_or(|end| end > token.len) {
        return Err(invalid("payload lease bounds"));
    }
    Ok(())
}
fn decode_location(source: u64, start: u64, bytes: &[u8]) -> io::Result<Location> {
    let arena = word(bytes, 1)?;
    if bytes.len() != 24 || arena > 1 {
        return Err(corrupt("payload location record"));
    }
    Ok(Location {
        source,
        start,
        len: word(bytes, 0)?,
        arena: arena as usize,
        offset: word(bytes, 2)?,
    })
}
fn physical(files: &[File; 2]) -> io::Result<u64> {
    files.iter().try_fold(0_u64, |sum, file| {
        file.metadata()?
            .blocks()
            .checked_mul(512)
            .and_then(|n| sum.checked_add(n))
            .ok_or_else(|| corrupt("payload physical accounting"))
    })
}
fn arena(directory: &Path) -> io::Result<File> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    loop {
        let path = directory.join(format!(
            ".overlay-payload-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => {
                fs::remove_file(path)?;
                return Ok(file);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Repeated(u8);
    impl Read for Repeated {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            output.fill(self.0);
            Ok(output.len())
        }
    }
    fn drain(payload: &Payload) -> io::Result<()> {
        for _ in 0..8192 {
            if !payload.reclaim_step()? {
                let stats = payload.stats()?;
                assert!(
                    !stats.cleanup_pending && stats.pending_releases == 0,
                    "{stats:?}"
                );
                return Ok(());
            }
        }
        panic!("payload cleanup failed to converge: {:?}", payload.stats()?);
    }

    #[test]
    fn partial_retention_disk_owner_reader_quota_and_short_write() -> io::Result<()> {
        let directory =
            std::env::temp_dir().join(format!("layerfs-payload-{}", std::process::id()));
        fs::create_dir_all(&directory)?;
        let payload = Payload::temporary(
            &directory,
            Limits {
                physical_bytes: 80 * 1024 * 1024,
                owners: 64,
                readers: 4,
                writes: 4,
                index: IndexLimits {
                    max_pages: 4096,
                    max_roots: 1024,
                    ..IndexLimits::default()
                },
            },
        )?;
        let full = payload.write_from(&mut Repeated(0x5a), 64 * 1024 * 1024)?;
        assert_eq!(full.len(), 64 * 1024 * 1024);
        let before = payload.stats()?;
        assert!(before.physical_bytes >= full.len());
        let held = full.subrange(32 * 1024 * 1024 + 17, 1)?;
        let after_slice = payload.stats()?;
        assert!(
            after_slice.interval_visits - before.interval_visits <= 8,
            "one-byte retention must split interval boundaries, not visit 16,384 blocks"
        );

        // Temporary slices leave no historical split points after ownership
        // release. This count is an exact ownership oracle, not a timing gate.
        for offset in (0..32).map(|i| (i * 7919) as u64) {
            let temporary = full.subrange(offset, 1)?;
            drop(temporary);
            for _ in 0..8 {
                if payload.stats()?.pending_releases == 0 {
                    break;
                }
                payload.reclaim_step()?;
            }
        }
        {
            let state = payload.0.state.lock().unwrap();
            let first = key(COVER, full.range.source, 0);
            let end = key(COVER, full.range.source + 1, 0);
            let rows = payload.0.index.scan(
                &state.root,
                Bound::Included(&first),
                Bound::Excluded(&end),
                128,
            )?;
            assert_eq!(
                rows.len(),
                3,
                "only the still-held subrange splits the full source"
            );
        }

        // The metadata graph acquires the token. Its leaf remains authoritative
        // after the OwnedRange and the initial metadata root handles disappear.
        let metadata = Index::temporary_with_owner(
            &directory,
            IndexLimits {
                max_pages: 512,
                max_roots: 128,
                ..IndexLimits::default()
            },
            Arc::new(payload.clone()),
        )?;
        let empty = metadata.empty_root()?;
        let root =
            metadata.set_owned(&empty, b"inode", b"one retained byte", Some(held.token()))?;
        let snapshot = root.clone();
        let token = held.token();
        drop(root);
        drop(held);
        drop(full);
        drain(&payload)?;
        let after = payload.stats()?;
        assert_eq!(after.live_block_bytes, BLOCK);
        assert!(after.physical_bytes <= BLOCK, "{after:?}");
        assert_eq!(after.relocated_bytes, BLOCK);
        let mut byte = [0];
        payload.read_exact_at(token, &mut byte, 0)?;
        assert_eq!(byte, [0x5a]);

        // A physical I/O lease survives catalog relocation; truncation is
        // deferred and charged until that bounded reader finishes.
        let physical = payload.physical_lease(token, 0, 1)?;
        let discard = payload.write_from(&mut Repeated(0x41), MOVE)?;
        drop(discard);
        for _ in 0..64 {
            if !payload.reclaim_step()? {
                break;
            }
        }
        self_read(&physical, &mut byte)?;
        assert_eq!(byte, [0x5a]);
        assert!(payload.stats()?.cleanup_pending);
        drop(physical);
        drain(&payload)?;
        payload.read_exact_at(token, &mut byte, 0)?;
        assert_eq!(byte, [0x5a]);

        let before_failure = payload.stats()?;
        assert_eq!(
            payload
                .write_from(&mut Repeated(0), 80 * 1024 * 1024)
                .err()
                .unwrap()
                .kind(),
            ErrorKind::StorageFull
        );
        payload.0.state.lock().unwrap().fail_after = Some(97);
        assert!(payload.write_from(&mut Repeated(0xff), MOVE).is_err());
        assert_eq!(
            payload.stats()?.physical_bytes,
            before_failure.physical_bytes
        );
        payload.read_exact_at(token, &mut byte, 0)?;
        assert_eq!(byte, [0x5a]);

        // Release the retained leaf through the disk graph and reject stale IDs.
        drop(snapshot);
        drop(empty);
        for _ in 0..64 {
            metadata.reclaim(16)?;
        }
        drain(&payload)?;
        assert!(payload.read_exact_at(token, &mut byte, 0).is_err());
        let final_stats = payload.stats()?;
        assert_eq!(final_stats.live_block_bytes, 0);
        assert_eq!(final_stats.physical_bytes, 0);
        assert_eq!(final_stats.owners, 0);
        println!("payload physical before={} retained={} final={} relocated={} interval_visits={} index_physical={}",
            before.physical_bytes, after.physical_bytes, final_stats.physical_bytes,
            after.relocated_bytes, after.interval_visits, after.index_physical_bytes);
        drop(metadata);
        drop(payload);
        fs::remove_dir(&directory)?;
        Ok(())
    }
    fn self_read(lease: &PhysicalLease, bytes: &mut [u8]) -> io::Result<()> {
        lease.payload.0.files[lease.arena].read_exact_at(bytes, lease.offset)
    }

    #[test]
    fn append_frontier_coalesces_without_rewriting_retained_prefix() -> io::Result<()> {
        let directory =
            std::env::temp_dir().join(format!("layerfs-payload-append-{}", std::process::id()));
        fs::create_dir_all(&directory)?;
        let payload = Payload::temporary(
            &directory,
            Limits {
                physical_bytes: 4 * MOVE,
                owners: 64,
                readers: 4,
                writes: 4,
                index: IndexLimits {
                    max_pages: 4096,
                    max_roots: 1024,
                    ..IndexLimits::default()
                },
            },
        )?;
        let original = payload.write_from(&mut &b"old"[..], 3)?;
        let mut current = original.subrange(0, 3)?;
        let mut expected = b"old".to_vec();
        for step in 0..257 {
            let value = (step % 251) as u8;
            let appended = payload
                .append_to(&current, &mut Repeated(value), 17)?
                .unwrap();
            assert_eq!(appended.source(), original.source());
            assert_eq!(appended.offset(), current.len());
            let joined = payload.join(&current, &appended)?;
            drop(current);
            drop(appended);
            current = joined;
            expected.extend([value; 17]);
            while payload.stats()?.pending_releases != 0 {
                assert!(payload.reclaim_step()?);
            }
        }
        let mut bytes = vec![0; expected.len()];
        current.read_exact_at(&mut bytes, 0)?;
        assert_eq!(bytes, expected);
        let mut old = [0; 3];
        original.read_exact_at(&mut old, 0)?;
        assert_eq!(&old, b"old");
        assert!(original.read_exact_at(&mut [0; 4], 0).is_err());
        {
            let state = payload.0.state.lock().unwrap();
            let first = key(LOCATION, original.source(), 0);
            let last = key(LOCATION, original.source() + 1, 0);
            let locations = payload.0.index.scan(
                &state.root,
                Bound::Included(&first),
                Bound::Excluded(&last),
                128,
            )?;
            assert_eq!(
                locations.len(),
                1,
                "sustained append must not retain one location per write"
            );
            assert_eq!(
                payload.source(&state.root, original.source())?.0,
                current.len()
            );
        }
        let before = payload.stats()?;
        payload.0.state.lock().unwrap().fail_after = Some(7);
        assert!(payload
            .append_to(&current, &mut Repeated(0xee), 17)
            .is_err());
        current.read_exact_at(&mut bytes, 0)?;
        assert_eq!(bytes, expected);
        assert_eq!(payload.stats()?.physical_bytes, before.physical_bytes);
        let next = payload
            .append_to(&current, &mut Repeated(0xab), 17)?
            .unwrap();
        assert_eq!(
            next.offset(),
            current.len(),
            "unpublished failed append cannot advance the source frontier"
        );
        let blocker = payload.write_from(&mut Repeated(0), 1)?;
        let mut untouched = &b"new"[..];
        assert!(payload.append_to(&current, &mut untouched, 3)?.is_none());
        assert_eq!(
            untouched, b"new",
            "fallback must not consume the caller's input"
        );
        println!(
            "payload sustained_append writes=257 bytes={} source_locations=1 physical={} owners={}",
            expected.len(),
            before.physical_bytes,
            before.owners
        );
        drop(blocker);
        drop(next);
        drop(current);
        drop(original);
        drain(&payload)?;
        assert_eq!(payload.stats()?.physical_bytes, 0);
        drop(payload);
        fs::remove_dir(&directory)?;
        Ok(())
    }

    #[test]
    fn writes_continue_while_old_source_reader_defers_truncation() -> io::Result<()> {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-payload-reader-write-{}",
            std::process::id()
        ));
        fs::create_dir_all(&directory)?;
        let payload = Payload::temporary(
            &directory,
            Limits {
                physical_bytes: 1024 * 1024,
                owners: 64,
                readers: 4,
                writes: 4,
                index: IndexLimits {
                    max_pages: 4096,
                    max_roots: 1024,
                    ..IndexLimits::default()
                },
            },
        )?;
        let kept = payload.write_from(&mut Repeated(0x41), BLOCK)?;
        let physical = payload.physical_lease(kept.token(), 0, 1)?;
        let discard = payload.write_from(&mut Repeated(0), MOVE)?;
        drop(discard);
        for _ in 0..32 {
            if !payload.reclaim_step()? {
                break;
            }
        }
        {
            let state = payload.0.state.lock().unwrap();
            let (retired, _) = state
                .truncate
                .expect("old physical reader defers source truncation");
            assert_ne!(retired, state.active);
            assert!(
                state.tails.iter().sum::<u64>() + MOVE + BLOCK < payload.0.limits.physical_bytes
            );
        }
        // The reader holds only retired source storage. It must not reject an
        // admitted foreground write into the independent active arena.
        let written = payload.write_from(&mut &b"B"[..], 1)?;
        let mut byte = [0];
        written.read_exact_at(&mut byte, 0)?;
        assert_eq!(byte, [b'B']);
        self_read(&physical, &mut byte)?;
        assert_eq!(byte, [b'A']);
        println!("payload deferred_source_reader foreground_write=PASS old_reader=PASS");
        drop(physical);
        drop(written);
        drop(kept);
        drain(&payload)?;
        drop(payload);
        fs::remove_dir(&directory)?;
        Ok(())
    }

    #[test]
    fn oversized_slot_limits_return_resource_error() {
        let limits = Limits {
            physical_bytes: 1024 * 1024,
            owners: 64,
            readers: 4,
            writes: 4,
            index: IndexLimits {
                max_pages: 4096,
                max_roots: 1024,
                ..IndexLimits::default()
            },
        };
        // Slot reservation precedes filesystem allocation, so no fixture is
        // needed for these rejected constructor inputs.
        for rejected in [
            Limits {
                owners: usize::MAX,
                ..limits
            },
            Limits {
                readers: usize::MAX,
                ..limits
            },
            Limits {
                writes: usize::MAX,
                ..limits
            },
        ] {
            assert_eq!(
                Payload::temporary(Path::new("."), rejected)
                    .err()
                    .unwrap()
                    .kind(),
                ErrorKind::StorageFull
            );
        }
    }
}
