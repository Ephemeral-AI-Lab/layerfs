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
use std::sync::{Arc, Mutex};

const BLOCK: u64 = 4096;
const MOVE: u64 = 64 * 1024;
const TOKEN: u8 = 1;
const COVER: u8 = 2;
const COVER_REVERSE: u8 = 3;
const LOCATION: u8 = 4;
const PHYSICAL: u8 = 5;
const RELEASE: u8 = 6;
const SOURCE: u8 = 7;
/// Unclaimed physical interval of an arena, keyed `(FREE, arena, offset)` in
/// ascending offset order and valued `[len]`. Adjacent rows are always
/// coalesced and a row never overlaps a LOCATION row, so the family is exactly
/// the complement of the live physical extents below the arena length.
const FREE: u8 = 8;
/// Bounded number of candidate records inspected by one allocation/relocation
/// decision. Each lookup is logarithmic in the catalog; this caps the fan-out so
/// a single step stays bounded with a large unclaimed or live set.
const CANDIDATES: usize = 64;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    pub physical_bytes: u64,
    /// Resident owner cap: live `OwnedRange` handles plus their queued release
    /// tickets. It is deliberately **not** a ceiling on persisted payload
    /// tokens, whose disk cost is charged through `index` and `physical_bytes`.
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
    /// Legacy ordinary-spool charge: live coverage clipped to source highwater.
    /// SDK-inline sources use the same physical backing but are excluded here.
    pub chargeable_bytes: u64,
    /// Resident ownership charge: live `OwnedRange` handles plus their queued
    /// release tickets. Bounded by `Limits::owners`.
    pub owners: usize,
    /// Persisted payload TOKEN rows. Their disk cost is charged through the
    /// independent index/arena quotas, not the resident owner cap.
    pub persisted_tokens: usize,
    /// Payload-index pages still live (records plus unreclaimed old versions).
    pub index_live_pages: u64,
    /// Highest payload-index page slot ever allocated in this arena.
    pub index_allocated_pages: u64,
    /// Payload-index pages actually returned to the free list.
    pub index_reclaimed_pages: u64,
    /// Retired payload-index roots awaiting reference release.
    pub index_pending_roots: usize,
    /// Payload-index reclamation backlog flag.
    pub index_reclamation_pending: bool,
    pub readers: usize,
    pub pending_releases: usize,
    pub pending_writes: usize,
    pub interval_visits: u64,
    pub relocated_bytes: u64,
    pub reclaimed_bytes: u64,
    /// Bytes currently unclaimed inside the arena: freed, not yet returned to
    /// the file system and available for reuse.
    pub unclaimed_bytes: u64,
    /// Cumulative bytes served by reusing an unclaimed interval instead of
    /// extending the arena.
    pub reused_bytes: u64,
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
    released: Mutex<VecDeque<u64>>,
    maintenance: Mutex<()>,
    neighbor_visits: AtomicU64,
    /// Catalog operations performed since the last bounded recycle. Advisory
    /// only: it decides *when* assistance runs, never admission itself.
    catalog_pressure: AtomicU64,
    /// Advisory mirror of `State::owners` for lock-free assistance decisions.
    resident_hint: AtomicU64,
    // Drop after files/state so aggregate backing admission cannot be released
    // while an independent OwnedRange or physical reader still owns this arena.
    _lifetime: Option<Arc<dyn std::any::Any + Send + Sync>>,
}
struct State {
    root: Root,
    next: u64,
    tails: [u64; 2],
    /// The single arena that holds payload. Reclamation is local: an unclaimed
    /// interval is reused in place instead of evacuating a whole arena, so the
    /// second backing file stays empty and is only admitted by the aggregate
    /// physical quota.
    active: usize,
    /// Resident owners: live handles plus queued release tickets/jobs.
    owners: usize,
    /// Live persisted TOKEN rows, charged by the index/arena disk quotas.
    tokens: usize,
    release_jobs: usize,
    writes: Vec<Reservation>,
    readers: Vec<Option<Reader>>,
    live_bytes: u64,
    chargeable_bytes: u64,
    visits: u64,
    relocated: u64,
    reclaimed: u64,
    unclaimed: u64,
    reused: u64,
    #[cfg(test)]
    fail_after: Option<u64>,
}
#[derive(Clone, Copy)]
struct Reservation {
    id: u64,
    arena: usize,
    start: u64,
    len: u64,
}
/// One bounded physical read lease. Its range is protected twice: no allocation
/// may reuse any part of it, and the arena length may not be reduced below its
/// end while the lease lives.
#[derive(Clone, Copy)]
struct Reader {
    arena: usize,
    start: u64,
    end: u64,
}
/// Bounded local reclamation decision for one maintenance step. Every variant
/// is derived from indexed records only, never from a whole-arena walk.
enum Local {
    /// No unclaimed interval is registered.
    None,
    /// The highest unclaimed interval reaches the arena length.
    Truncate { len: u64 },
    /// One bounded live interval can move into a strictly lower unclaimed
    /// interval, shortening the arena by at least the moved byte count.
    Relocate {
        physical: u64,
        source: u64,
        logical: u64,
        len: u64,
        destination: u64,
    },
    /// Unclaimed space exists but a live physical reader still pins it.
    Deferred,
    /// Unclaimed space is retained, charged slack: no bounded step can shrink
    /// the arena and the space is only reusable by a later fitting allocation.
    Retained,
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
        Self::create(directory, limits, None)
    }
    pub(crate) fn temporary_with_lifetime(
        directory: &Path,
        limits: Limits,
        lifetime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> io::Result<Self> {
        Self::create(directory, limits, Some(lifetime))
    }
    fn create(
        directory: &Path,
        limits: Limits,
        lifetime: Option<Arc<dyn std::any::Any + Send + Sync>>,
    ) -> io::Result<Self> {
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
                owners: 0,
                tokens: 0,
                release_jobs: 0,
                writes,
                readers,
                live_bytes: 0,
                chargeable_bytes: 0,
                visits: 0,
                relocated: 0,
                reclaimed: 0,
                unclaimed: 0,
                reused: 0,
                #[cfg(test)]
                fail_after: None,
            }),
            released: Mutex::new(released),
            maintenance: Mutex::new(()),
            neighbor_visits: AtomicU64::new(0),
            catalog_pressure: AtomicU64::new(0),
            resident_hint: AtomicU64::new(0),
            _lifetime: lifetime,
        })))
    }

    /// Mirror the resident charge for lock-free assistance decisions. Advisory
    /// only; `State::owners` remains the admission authority.
    fn note_resident(&self, state: &State) {
        self.0
            .resident_hint
            .store(state.owners as u64, Ordering::Relaxed);
    }
    /// Bounded admission assistance for retired transient handles and for
    /// recycled catalog versions. Called before an owner-producing path takes
    /// the state lock, so it never runs under it and never drains without a
    /// bound. Ordinary callers do not need a separate maintenance loop for
    /// either the release queue or the index backlog to keep up.
    fn assist(&self) -> io::Result<()> {
        // Recycle at most one assistance budget of obsolete catalog versions.
        const CATALOG_ASSIST_PAGES: u64 = 1024;
        if self
            .0
            .catalog_pressure
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |pressure| {
                (pressure >= CATALOG_ASSIST_PAGES).then_some(0)
            })
            .is_ok()
        {
            self.assist_catalog()?;
        }
        // Release retired handles while the resident charge is above half of
        // its cap. The queue pop path re-acquires the state lock per item, so
        // no lock is held across this loop.
        const RELEASE_ASSIST: usize = 64;
        if self.0.resident_hint.load(Ordering::Relaxed) * 2 >= self.0.limits.owners as u64 {
            for _ in 0..RELEASE_ASSIST {
                if self.0.resident_hint.load(Ordering::Relaxed) * 4 < self.0.limits.owners as u64 {
                    break;
                }
                if !self.release_step()? {
                    break;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn write_from(&self, reader: &mut impl Read, len: u64) -> io::Result<OwnedRange> {
        self.assist()?;
        self.write_classified(reader, len, true)
    }
    /// Preserve SDK-inline input classification without retaining its payload
    /// in RAM or creating a second storage/encoding pipeline.
    pub(crate) fn write_inline_from(
        &self,
        reader: &mut impl Read,
        len: u64,
    ) -> io::Result<OwnedRange> {
        self.assist()?;
        self.write_classified(reader, len, false)
    }
    fn write_classified(
        &self,
        reader: &mut impl Read,
        len: u64,
        spool_charge: bool,
    ) -> io::Result<OwnedRange> {
        if len == 0 {
            return Err(invalid("empty payload extent"));
        }
        let allocated = aligned(len)?;
        let reservation = {
            let mut state = self.0.state.lock().unwrap();
            if state.owners + state.writes.len() >= self.0.limits.owners
                || state.writes.len() >= self.0.limits.writes
            {
                return Err(quota("payload owner/write admission"));
            }
            self.reserve(&state, allocated, false)?;
            self.reserve_catalog(&state, 8, false)?;
            let id = state.next;
            state.next = id
                .checked_add(1)
                .ok_or_else(|| quota("payload identities"))?;
            let arena = state.active;
            let start = self.allocate(&mut state, allocated)?;
            let reservation = Reservation {
                id,
                arena,
                start,
                len: allocated,
            };
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
            let root = self.0.index.set(
                &root,
                &key(SOURCE, token.source, 0),
                &words(&[len, 1, u64::from(spool_charge)]),
            )?;
            let root = self
                .0
                .index
                .set(&root, &key(TOKEN, reservation.id, 0), &token.encode())?;
            let chargeable = state
                .chargeable_bytes
                .checked_add(if spool_charge { len } else { 0 })
                .ok_or_else(|| quota("payload charge overflow"))?;
            state.root = root;
            state.chargeable_bytes = chargeable;
            state.live_bytes += allocated;
            state.owners += 1;
            state.tokens += 1;
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
        state.writes.retain(|item| item.id != reservation.id);
        if published.is_err() {
            // Never-published bytes are referenced by no location record.
            // Return them to the unclaimed set in place and give back the top of
            // the arena immediately: a failed write must not leave charged slack
            // and must not grow an unbounded retention list.
            self.reserve_catalog(state, 12, true)?;
            let (root, added) = self.free_interval(
                &state.root,
                reservation.arena,
                reservation.start,
                reservation.len,
            )?;
            state.root = root;
            state.unclaimed += added;
            self.truncate_top(state)?;
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
        self.assist()?;
        let (reservation, location, high, end) = {
            let mut state = self.0.state.lock().unwrap();
            let owner = self.token(&state.root, owner)?;
            let (high, _, spool_charge) = self.source(&state.root, owner.source)?;
            if !spool_charge || owner.start.checked_add(owner.len) != Some(high) {
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
            // Only an occurrence whose location still ends exactly at the
            // active arena tail can extend contiguously. A location reused from
            // an unclaimed interval in the middle of the arena reports None and
            // the caller starts a fresh lineage instead.
            if location.start + location.len != aligned(high)?
                || location.arena != state.active
                || location.offset + location.len != state.tails[location.arena]
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
            let mut added_charge = len;
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
                    added_charge = added_charge
                        .checked_add(high - begin)
                        .ok_or_else(|| quota("payload charge overflow"))?;
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
                    u64::from(source.2),
                ]),
            )?;
            root = self
                .0
                .index
                .set(&root, &key(TOKEN, reservation.id, 0), &token.encode())?;
            let chargeable = state
                .chargeable_bytes
                .checked_add(added_charge)
                .ok_or_else(|| quota("payload charge overflow"))?;
            state.root = root;
            state.owners += 1;
            state.tokens += 1;
            state.live_bytes += added_live;
            state.chargeable_bytes = chargeable;
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
        self.assist()?;
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
        self.assist()?;
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
                u64::from(source.2),
            ]),
        )?;
        state.root = root;
        state.next = next;
        state.owners += 1;
        state.tokens += 1;
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
        state.readers[slot] = Some(Reader {
            arena: location.arena,
            start: physical,
            end: physical + len as u64,
        });
        Ok(PhysicalLease {
            payload: self.clone(),
            slot,
            arena: location.arena,
            offset: physical,
            len,
        })
    }

    /// One bounded local reclamation decision for `arena` (always the active
    /// arena). Every variant is derived from indexed records: the highest
    /// unclaimed interval, the highest live interval, and at most one bounded
    /// candidate probe. No step walks the arena, so a small deletion can never
    /// recopy unrelated live payload.
    fn plan_local(&self, state: &State, arena: usize) -> io::Result<Local> {
        let Some((start, len)) = self.free_top(&state.root, arena)? else {
            return Ok(Local::None);
        };
        if !state.writes.is_empty() {
            // An admitted reservation may still extend the arena length and its
            // bytes are neither live nor unclaimed yet. No bounded step can
            // reduce the arena safely until that write publishes or fails.
            return Ok(Local::Deferred);
        }
        let ceiling = reader_ceiling(state, arena);
        if start + len == state.tails[arena] {
            return Ok(if ceiling <= start {
                Local::Truncate { len: start }
            } else {
                Local::Deferred
            });
        }
        let Some((physical, source, logical, len)) = self.physical_top(&state.root, arena)? else {
            return Err(corrupt("payload unclaimed interval above live top"));
        };
        if len == 0 || len > MOVE {
            // Whole-arena repacking is deliberately not performed. A live
            // interval larger than one bounded move is retained and charged.
            return Ok(Local::Retained);
        }
        let Some(destination) = self.free_below(state, arena, physical, len)? else {
            return Ok(Local::Retained);
        };
        let target = self
            .live_end_below(&state.root, arena, physical)?
            .unwrap_or(0)
            .max(destination + len);
        if target >= physical + len {
            return Ok(Local::Retained);
        }
        if ceiling > target {
            return Ok(Local::Deferred);
        }
        Ok(Local::Relocate {
            physical,
            source,
            logical,
            len,
            destination,
        })
    }
    /// Apply at most one bounded local reclamation step. Returns whether it made
    /// progress. Relocation moves at most `MOVE` bytes and permanently removes
    /// at least the moved byte count from the arena length, so cumulative
    /// relocation work is charged to reclaimed bytes: repeated small deletions
    /// or replacements cannot recopy an unrelated live set.
    fn reclaim_local(&self, state: &mut State) -> io::Result<bool> {
        let arena = state.active;
        match self.plan_local(state, arena)? {
            Local::Truncate { len } => {
                self.truncate_to(state, len)?;
                Ok(true)
            }
            Local::Relocate {
                physical,
                source,
                logical,
                len,
                destination,
            } => {
                self.relocate(state, physical, source, logical, len, destination)?;
                Ok(true)
            }
            Local::None | Local::Deferred | Local::Retained => Ok(false),
        }
    }
    /// Return the top of the arena to the file system when the highest
    /// unclaimed interval reaches it. Bounded and reader-safe.
    fn truncate_top(&self, state: &mut State) -> io::Result<bool> {
        let arena = state.active;
        let Some((start, len)) = self.free_top(&state.root, arena)? else {
            return Ok(false);
        };
        if start + len != state.tails[arena]
            || !state.writes.is_empty()
            || reader_ceiling(state, arena) > start
        {
            return Ok(false);
        }
        self.truncate_to(state, start)?;
        Ok(true)
    }
    /// Reduce the arena length to `len` (the start of the highest unclaimed
    /// interval). The caller established that no live reader covers the removed
    /// bytes and that no reservation is in flight.
    fn truncate_to(&self, state: &mut State, len: u64) -> io::Result<()> {
        let arena = state.active;
        if len >= state.tails[arena] {
            return Err(corrupt("payload truncation growth"));
        }
        self.0.files[arena].set_len(len)?;
        let removed = state.tails[arena] - len;
        state.reclaimed += removed;
        state.unclaimed = state
            .unclaimed
            .checked_sub(removed)
            .ok_or_else(|| corrupt("payload unclaimed underflow"))?;
        state.tails[arena] = len;
        // The record that described the returned space is no longer inside the
        // arena; `remove` is a no-op when the interval was already consumed.
        state.root = self
            .0
            .index
            .remove(&state.root, &key(FREE, arena as u64, len))?;
        Ok(())
    }
    /// Move one bounded live interval into a strictly lower unclaimed interval
    /// and return the vacated top of the arena to the file system. Only the
    /// moved interval's own location record is rewritten through stable
    /// locations; no other live payload is touched.
    fn relocate(
        &self,
        state: &mut State,
        physical: u64,
        source: u64,
        logical: u64,
        len: u64,
        destination: u64,
    ) -> io::Result<()> {
        let arena = state.active;
        self.reserve_catalog(state, 24, true)?;
        let mut bytes = vec![0_u8; len as usize];
        self.0.files[arena].read_exact_at(&mut bytes, physical)?;
        self.0.files[arena].write_all_at(&bytes, destination)?;
        let available = self
            .0
            .index
            .get(&state.root, &key(FREE, arena as u64, destination))?
            .ok_or_else(|| corrupt("payload relocation destination absent"))?;
        let available = word(&available, 0)?;
        if available < len {
            return Err(corrupt("payload relocation destination too small"));
        }
        let mut root = self
            .0
            .index
            .remove(&state.root, &key(FREE, arena as u64, destination))?;
        if available > len {
            root = self.0.index.set(
                &root,
                &key(FREE, arena as u64, destination + len),
                &words(&[available - len]),
            )?;
        }
        let location = Location {
            source,
            start: logical,
            len,
            arena,
            offset: physical,
        };
        root = self.remove_location(&root, location)?;
        root = self.put_location(
            &root,
            Location {
                offset: destination,
                ..location
            },
        )?;
        let (root, added) = self.free_interval(&root, arena, physical, len)?;
        state.root = root;
        state.unclaimed = state
            .unclaimed
            .checked_sub(len)
            .ok_or_else(|| corrupt("payload unclaimed underflow"))?
            .checked_add(added)
            .ok_or_else(|| corrupt("payload unclaimed overflow"))?;
        state.relocated += len;
        let _ = self.truncate_top(state)?;
        Ok(())
    }
    /// Highest unclaimed interval in `arena`. The unclaimed family is stored in
    /// ascending offset order, so its greatest key is that interval.
    fn free_top(&self, root: &Root, arena: usize) -> io::Result<Option<(u64, u64)>> {
        let Some((row, value)) = self.0.index.floor(root, &key(FREE, u64::MAX, u64::MAX))? else {
            return Ok(None);
        };
        if row[0] != FREE || word(&row[1..], 0)? != arena as u64 {
            return Ok(None);
        }
        Ok(Some((word(&row[1..], 1)?, word(&value, 0)?)))
    }
    /// Highest live physical interval of `arena`: `(offset, source, logical
    /// start, length)`. Physical keys are stored in reverse offset order.
    fn physical_top(&self, root: &Root, arena: usize) -> io::Result<Option<(u64, u64, u64, u64)>> {
        // Physical keys are stored in reverse offset order, so the first record
        // of the family is the highest live interval.
        let first = key(PHYSICAL, arena as u64, 0);
        let last = key(PHYSICAL, arena as u64 + 1, 0);
        let rows = self
            .0
            .index
            .scan(root, Bound::Included(&first), Bound::Excluded(&last), 1)?;
        let Some((row, value)) = rows.first() else {
            return Ok(None);
        };
        Ok(Some((
            !word(&row[1..], 1)?,
            word(value, 0)?,
            word(value, 1)?,
            word(value, 2)?,
        )))
    }
    /// End offset of the greatest live interval strictly below `physical`.
    fn live_end_below(&self, root: &Root, arena: usize, physical: u64) -> io::Result<Option<u64>> {
        let first = key(PHYSICAL, arena as u64, !physical);
        let last = key(PHYSICAL, arena as u64 + 1, 0);
        let rows = self
            .0
            .index
            .scan(root, Bound::Excluded(&first), Bound::Excluded(&last), 1)?;
        rows.first()
            .map(|(row, value)| {
                let offset = !word(&row[1..], 1)?;
                Ok(offset + word(value, 2)?)
            })
            .transpose()
    }
    /// Lowest unclaimed interval below `physical` that fits `len` and is not
    /// pinned by a live reader. Bounded to `CANDIDATES` record probes.
    fn free_below(
        &self,
        state: &State,
        arena: usize,
        physical: u64,
        len: u64,
    ) -> io::Result<Option<u64>> {
        if physical == 0 {
            return Ok(None);
        }
        let first = key(FREE, arena as u64, 0);
        let last = key(FREE, arena as u64, physical);
        let rows = self.0.index.scan(
            &state.root,
            Bound::Included(&first),
            Bound::Excluded(&last),
            CANDIDATES,
        )?;
        for (row, value) in rows {
            let offset = word(&row[1..], 1)?;
            if word(&value, 0)? >= len && !reader_pins(state, arena, offset, len) {
                return Ok(Some(offset));
            }
        }
        Ok(None)
    }
    /// Register `[start, start + len)` as unclaimed, coalescing both neighbours.
    /// Bounded: one exact lookup, one neighbour seek and one update. Returns the
    /// root and the number of newly unclaimed bytes.
    fn free_interval(
        &self,
        root: &Root,
        arena: usize,
        start: u64,
        len: u64,
    ) -> io::Result<(Root, u64)> {
        if len == 0 {
            return Ok((root.clone(), 0));
        }
        let added = len;
        let mut root = root.clone();
        let mut start = start;
        let mut len = len;
        if let Some(bytes) = self
            .0
            .index
            .get(&root, &key(FREE, arena as u64, start + len))?
        {
            let following = word(&bytes, 0)?;
            if following == 0 {
                return Err(corrupt("payload unclaimed record"));
            }
            root = self
                .0
                .index
                .remove(&root, &key(FREE, arena as u64, start + len))?;
            len = len
                .checked_add(following)
                .ok_or_else(|| corrupt("payload unclaimed overflow"))?;
        }
        if start > 0 {
            if let Some((row, value)) = self
                .0
                .index
                .floor(&root, &key(FREE, arena as u64, start - 1))?
            {
                let at = word(&row[1..], 1)?;
                let size = word(&value, 0)?;
                if row[0] == FREE
                    && word(&row[1..], 0)? == arena as u64
                    && size != 0
                    && at + size == start
                {
                    root = self.0.index.remove(&root, &key(FREE, arena as u64, at))?;
                    start = at;
                    len = len
                        .checked_add(size)
                        .ok_or_else(|| corrupt("payload unclaimed overflow"))?;
                }
            }
        }
        let root = self
            .0
            .index
            .set(&root, &key(FREE, arena as u64, start), &words(&[len]))?;
        Ok((root, added))
    }
    /// Lowest unclaimed interval that fits `len`, is not pinned by a live reader
    /// and does not overlap an in-flight reservation. Bounded to `CANDIDATES`.
    fn take_free(&self, state: &mut State, arena: usize, len: u64) -> io::Result<Option<u64>> {
        let first = key(FREE, arena as u64, 0);
        let last = key(FREE, arena as u64 + 1, 0);
        let rows = self.0.index.scan(
            &state.root,
            Bound::Included(&first),
            Bound::Excluded(&last),
            CANDIDATES,
        )?;
        for (row, value) in rows {
            let offset = word(&row[1..], 1)?;
            let available = word(&value, 0)?;
            if available < len
                || state
                    .writes
                    .iter()
                    .any(|item| item.arena == arena && overlaps(item.start, item.len, offset, len))
                || reader_pins(state, arena, offset, len)
            {
                continue;
            }
            let mut root = self
                .0
                .index
                .remove(&state.root, &key(FREE, arena as u64, offset))?;
            if available > len {
                root = self.0.index.set(
                    &root,
                    &key(FREE, arena as u64, offset + len),
                    &words(&[available - len]),
                )?;
            }
            state.root = root;
            state.unclaimed = state
                .unclaimed
                .checked_sub(len)
                .ok_or_else(|| corrupt("payload unclaimed underflow"))?;
            state.reused += len;
            return Ok(Some(offset));
        }
        Ok(None)
    }
    /// Reserve `len` bytes of the active arena: reuse the lowest fitting
    /// unclaimed interval first, otherwise extend the append tail. Tail-first
    /// contiguity is preserved for the append path because a fresh write of at
    /// least `MOVE` bytes is the common sequential-stream shape, while tiny
    /// payloads reuse unclaimed space and keep the arena length down.
    fn allocate(&self, state: &mut State, len: u64) -> io::Result<u64> {
        let arena = state.active;
        if len < MOVE {
            if let Some(offset) = self.take_free(state, arena, len)? {
                return Ok(offset);
            }
        }
        let offset = state.tails[arena];
        state.tails[arena] = offset
            .checked_add(len)
            .ok_or_else(|| quota("payload arena length"))?;
        Ok(offset)
    }
    /// One bounded ownership/physical maintenance step. Reclamation is local:
    /// at most one truncation or one bounded relocation per call, never a whole
    /// arena evacuate. Retired payload-index versions are reclaimed here as
    /// well: the arena's index is otherwise never recycled, which would exhaust
    /// its catalog quota long before any ownership or disk bound is reached.
    pub(crate) fn reclaim_step(&self) -> io::Result<bool> {
        let Ok(_maintenance) = self.0.maintenance.try_lock() else {
            return Ok(false);
        };
        // Bounded catalog recycle first and unconditionally: it never touches
        // payload state, and it must not be starved by a busy handle-release
        // queue. Every copied page path retires a whole cascade of now
        // unreferenced pages, and releasing one child reference frees no page
        // at all, so progress is judged by the pending backlog rather than by
        // pages freed. Without this the catalog quota is exhausted long before
        // any ownership or disk bound is reached.
        let recycled = recycle_catalog(&self.0.index)?;
        if self.release_step()? {
            return Ok(true);
        }
        if recycled {
            return Ok(true);
        }
        let mut state = self.0.state.lock().unwrap();
        self.reclaim_local(&mut state)
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
        let source = self.source(&state.root, token.source)?;
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
        let mut unclaimed = 0;
        if refs == 1 {
            // The last reference over `[released_start, stop_here)` is gone, so
            // exactly that physical span becomes unclaimed. The location record
            // is split at the same boundaries, which keeps live extents exact: a
            // file-only reader retains its own bounded allocation unit and never
            // the unrelated payload that shared the arena.
            let (split, added) =
                self.release_physical(&root, token.source, released_start, stop_here)?;
            root = split;
            unclaimed = added;
        }
        token.cursor = stop_here;
        if stop_here == end {
            root = self.0.index.remove(&root, job)?;
            root = self.0.index.remove(&root, &key(TOKEN, id, 0))?;
            root = if source.1 == 1 {
                self.0.index.remove(&root, &key(SOURCE, token.source, 0))?
            } else {
                self.0.index.set(
                    &root,
                    &key(SOURCE, token.source, 0),
                    &words(&[source.0, source.1 - 1, u64::from(source.2)]),
                )?
            };
        } else {
            root = self
                .0
                .index
                .set(&root, &key(TOKEN, id, 0), &token.encode())?;
        }
        let removed_charge = if refs == 1 && source.2 {
            stop_here
                .min(source.0)
                .checked_sub(released_start.min(source.0))
                .ok_or_else(|| corrupt("payload clipped coverage order"))?
        } else {
            0
        };
        let chargeable = state
            .chargeable_bytes
            .checked_sub(removed_charge)
            .ok_or_else(|| corrupt("payload charge underflow"))?;
        state.root = root;
        state.chargeable_bytes = chargeable;
        state.unclaimed += unclaimed;
        if refs == 1 {
            state.live_bytes -= stop_here - released_start;
        }
        if stop_here == end {
            state.owners -= 1;
            state.tokens -= 1;
            state.release_jobs -= 1;
        }
        self.note_resident(state);
        Ok(true)
    }
    /// Split the location record covering `[released_start, stop_here)` and
    /// register the released physical span as unclaimed. `stop_here` is a
    /// coverage boundary, so both remaining parts stay exactly covered and a
    /// later append can still extend the surviving tail occurrence.
    fn release_physical(
        &self,
        root: &Root,
        source: u64,
        released_start: u64,
        stop_here: u64,
    ) -> io::Result<(Root, u64)> {
        let Some((row, value)) = self.floor(root, LOCATION, source, released_start)? else {
            return Err(corrupt("payload location absent"));
        };
        let location = decode_location(source, !word(&row[1..], 1)?, &value)?;
        let location_end = location
            .start
            .checked_add(location.len)
            .ok_or_else(|| corrupt("payload location overflow"))?;
        if stop_here <= released_start
            || released_start < location.start
            || stop_here > location_end
        {
            return Err(corrupt("payload release outside location"));
        }
        let mut root = self.remove_location(root, location)?;
        if location.start < released_start {
            root = self.put_location(
                &root,
                Location {
                    len: released_start - location.start,
                    ..location
                },
            )?;
        }
        if stop_here < location_end {
            root = self.put_location(
                &root,
                Location {
                    start: stop_here,
                    len: location_end - stop_here,
                    offset: location.offset + (stop_here - location.start),
                    ..location
                },
            )?;
        }
        self.free_interval(
            &root,
            location.arena,
            location.offset + (released_start - location.start),
            stop_here - released_start,
        )
    }
    /// Bounded catalog assistance used by admission: one short burst, sized so
    /// an owner-producing call stays within a predictable work bound even when
    /// a large backlog exists.
    fn assist_catalog(&self) -> io::Result<bool> {
        let Ok(_maintenance) = self.0.maintenance.try_lock() else {
            return Ok(false);
        };
        recycle_catalog(&self.0.index)
    }
    /// Consume one queued retired handle. The caller must not hold the state
    /// lock: this re-acquires it, so a bounded assistance loop stays deadlock
    /// free and its per-item cost is bounded by one catalog update.
    fn release_step(&self) -> io::Result<bool> {
        let queued = self.0.released.lock().unwrap().front().copied();
        if let Some(id) = queued {
            self.release_charged(id, true)?;
            self.0.released.lock().unwrap().pop_front();
            return Ok(true);
        }
        // Metadata leaves acquire tokens without a resident handle; their
        // release jobs are queued work too and are counted against the same
        // resident admission until they complete.
        let mut state = self.0.state.lock().unwrap();
        let pending_interval = self.release_interval(&mut state)?;
        drop(state);
        Ok(pending_interval)
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
    /// Deep drain of the payload catalog reclamation for diagnosis: repeats the
    /// bounded step until it stops making progress.
    #[cfg(test)]
    pub(crate) fn deep_reclaim(&self) -> io::Result<usize> {
        let mut total = 0;
        for _ in 0..1_000_000 {
            let freed = self.0.index.reclaim(1024)?;
            if freed == 0 {
                break;
            }
            total += freed;
        }
        Ok(total)
    }
    /// Diagnostic header census of the payload catalog arena.
    #[cfg(test)]
    pub(crate) fn catalog_header_census(&self) -> io::Result<crate::overlay_index::HeaderCensus> {
        self.0.index.header_census()
    }
    /// Diagnostic census of the payload catalog tree structure.
    #[cfg(test)]
    pub(crate) fn catalog_tree_census(&self) -> io::Result<crate::overlay_index::Census> {
        let state = self.0.state.lock().unwrap();
        self.0.index.census(&state.root)
    }
    /// Diagnostic census of the payload catalog: live records by key prefix.
    /// Test-only measurement helper; it never gates product behavior.
    #[cfg(test)]
    pub(crate) fn catalog_census(&self) -> io::Result<Vec<(u8, usize, u64)>> {
        use std::collections::BTreeMap;
        let state = self.0.state.lock().unwrap();
        let mut tally: BTreeMap<u8, (usize, u64)> = BTreeMap::new();
        let mut start: Option<Vec<u8>> = None;
        loop {
            let lower = start
                .as_deref()
                .map(Bound::Excluded)
                .unwrap_or(Bound::Unbounded);
            let rows = self
                .0
                .index
                .scan(&state.root, lower, Bound::Unbounded, 1024)?;
            let Some((last, _)) = rows.last() else {
                break;
            };
            for (key, value) in &rows {
                let entry = tally.entry(key[0]).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += value.len() as u64;
            }
            start = Some(last.clone());
        }
        Ok(tally.into_iter().map(|(k, (n, b))| (k, n, b)).collect())
    }
    /// Release one reference to `id`. `charged` states whether the caller's
    /// resident slot already covers this work (a retired handle ticket) or the
    /// charge must be created for queued metadata-leaf release work.
    fn release_charged(&self, id: u64, charged: bool) -> io::Result<()> {
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
            // A queued release job replaces the retired handle's charge; a
            // metadata-started job adds its own until it completes.
            state.release_jobs += 1;
            if !charged {
                state.owners += 1;
            }
        } else if charged {
            // An independent metadata leaf still owns the token. The retired
            // handle's resident slot is free again; the token itself remains
            // charged to the catalog/arena disk quotas.
            state.owners -= 1;
        }
        state.root = root;
        self.note_resident(&state);
        Ok(())
    }
    fn reserve_catalog(&self, state: &State, operations: u64, recovery: bool) -> io::Result<()> {
        // Advisory pressure for bounded inline assistance. Saturating: the
        // assist resets it, and an overflow only delays the next early assist.
        let _ = self.0.catalog_pressure.fetch_update(
            Ordering::Relaxed,
            Ordering::Relaxed,
            |pressure| Some(pressure.saturating_add(operations)),
        );
        let limits = self.0.limits.index;
        let pending = state.writes.len() as u64 * 32;
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
    fn source(&self, root: &Root, id: u64) -> io::Result<(u64, u64, bool)> {
        let bytes = self
            .0
            .index
            .get(root, &key(SOURCE, id, 0))?
            .ok_or_else(|| corrupt("payload source absent"))?;
        if bytes.len() != 24 || word(&bytes, 1)? == 0 || word(&bytes, 2)? > 1 {
            return Err(corrupt("payload source record"));
        }
        Ok((word(&bytes, 0)?, word(&bytes, 1)?, word(&bytes, 2)? == 1))
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
    /// Flush private data and its indexed ownership for an explicit fsync.
    /// Acknowledged writes are already host-owned; no state/root lock or global
    /// writer drain is held across these calls. This does not publish a Commit.
    pub(crate) fn sync_data(&self) -> io::Result<()> {
        for file in &self.0.files {
            file.sync_data()?;
        }
        self.0.index.sync_data()
    }
    pub(crate) fn stats(&self) -> io::Result<Stats> {
        let state = self.0.state.lock().unwrap();
        let pending_releases = state.release_jobs + self.0.released.lock().unwrap().len();
        let index = self.0.index.stats()?;
        Ok(Stats {
            physical_bytes: physical(&self.0.files)?,
            reserved_bytes: state.tails.iter().sum(),
            index_physical_bytes: index.physical_bytes,
            live_block_bytes: state.live_bytes,
            chargeable_bytes: state.chargeable_bytes,
            owners: state.owners,
            persisted_tokens: state.tokens,
            index_live_pages: index.live_pages,
            index_allocated_pages: index.allocated_pages,
            index_reclaimed_pages: index.reclaimed_pages,
            index_pending_roots: index.pending_roots,
            index_reclamation_pending: index.reclamation_pending,
            readers: state.readers.iter().flatten().count(),
            pending_releases,
            pending_writes: state.writes.len(),
            interval_visits: state.visits + self.0.neighbor_visits.load(Ordering::Relaxed),
            relocated_bytes: state.relocated,
            reclaimed_bytes: state.reclaimed,
            unclaimed_bytes: state.unclaimed,
            reused_bytes: state.reused,
            cleanup_pending: pending_releases != 0
                || index.reclamation_pending
                || matches!(
                    self.plan_local(&state, state.active)?,
                    Local::Truncate { .. } | Local::Relocate { .. } | Local::Deferred
                ),
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
        // Invoked by a metadata leaf retiring its own reference: this is new
        // queued release work without a resident handle.
        self.release_charged(id, false)
    }
}
/// Bounded catalog recycle. Pages are reclaimed in batches; the loop stops as
/// soon as the index reports no further backlog, so a caught-up index costs one
/// cheap probe per call.
fn recycle_catalog(index: &Index) -> io::Result<bool> {
    const CATALOG_STEPS: usize = 8;
    const CATALOG_BATCH: usize = 256;
    let mut recycled = false;
    for _ in 0..CATALOG_STEPS {
        if !index.reclaim_pending() {
            break;
        }
        recycled |= index.reclaim(CATALOG_BATCH)? > 0;
    }
    Ok(recycled)
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
/// Whether a live physical reader still covers part of `[start, start + len)`.
fn reader_pins(state: &State, arena: usize, start: u64, len: u64) -> bool {
    state.readers.iter().flatten().any(|reader| {
        reader.arena == arena && overlaps(reader.start, reader.end - reader.start, start, len)
    })
}
/// Highest end offset covered by a live reader of `arena` (0 when none). The
/// arena length may never be reduced below this bound.
fn reader_ceiling(state: &State, arena: usize) -> u64 {
    state
        .readers
        .iter()
        .flatten()
        .filter(|reader| reader.arena == arena)
        .map(|reader| reader.end)
        .max()
        .unwrap_or(0)
}
fn overlaps(first: u64, first_len: u64, second: u64, second_len: u64) -> bool {
    let first_end = first.saturating_add(first_len);
    let second_end = second.saturating_add(second_len);
    first < second_end && second < first_end
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

        // A live physical lease pins its own bytes and the arena length: local
        // reclamation may not reuse or truncate the range it still reads, and it
        // is charged as deferred work until that bounded reader finishes.
        let discard = payload.write_from(&mut Repeated(0x41), MOVE)?;
        let physical = payload.physical_lease(discard.token(), 0, 1)?;
        drop(discard);
        for _ in 0..64 {
            if !payload.reclaim_step()? {
                break;
            }
        }
        self_read(&physical, &mut byte)?;
        assert_eq!(byte, [0x41]);
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
    fn live_physical_reader_defers_arena_truncation_without_blocking_writes() -> io::Result<()> {
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
        let released = payload.write_from(&mut Repeated(0x5a), MOVE)?;
        // The lease covers bytes of an occurrence that is then released: its
        // range must stay readable, must not be reused, and must be charged as
        // deferred reclamation until the bounded reader retires.
        let physical = payload.physical_lease(released.token(), 0, 1)?;
        drop(released);
        for _ in 0..32 {
            if !payload.reclaim_step()? {
                break;
            }
        }
        let deferred = payload.stats()?;
        assert!(deferred.cleanup_pending, "{deferred:?}");
        assert!(deferred.unclaimed_bytes >= MOVE, "{deferred:?}");
        assert_eq!(deferred.relocated_bytes, 0, "{deferred:?}");
        // A foreground write is admitted while the reader pins that range: it
        // must neither be rejected nor overwrite the leased bytes.
        let written = payload.write_from(&mut &b"B"[..], 1)?;
        let mut byte = [0];
        written.read_exact_at(&mut byte, 0)?;
        assert_eq!(byte, [b'B']);
        self_read(&physical, &mut byte)?;
        assert_eq!(byte, [0x5a]);
        kept.read_exact_at(&mut byte, 0)?;
        assert_eq!(byte, [0x41]);
        println!("payload deferred_source_reader foreground_write=PASS old_reader=PASS");
        drop(physical);
        drain(&payload)?;
        kept.read_exact_at(&mut byte, 0)?;
        assert_eq!(byte, [0x41]);
        written.read_exact_at(&mut byte, 0)?;
        assert_eq!(byte, [b'B']);
        let settled = payload.stats()?;
        assert_eq!(settled.live_block_bytes, 2 * BLOCK, "{settled:?}");
        assert!(settled.physical_bytes <= 2 * BLOCK, "{settled:?}");
        drop(written);
        drop(kept);
        drain(&payload)?;
        drop(payload);
        fs::remove_dir(&directory)?;
        Ok(())
    }

    /// Focused counterexample and repair proof for release-triggered reclamation.
    ///
    /// The former mechanism set a sweep flag on every coverage release and
    /// evacuated the whole arena one bounded step at a time, so `D` small
    /// deletions among `N` live blocks moved `N + (N-1) + ...` live bytes with
    /// completed maintenance between them: quadratic cumulative work. Local
    /// reclamation may only move an interval whose relocation permanently
    /// removes at least the moved byte count from the arena length, so total
    /// relocation is charged to reclaimed bytes and stays linear in freed input.
    #[test]
    fn repeated_small_release_over_large_live_set_moves_only_justified_bytes() -> io::Result<()> {
        const FILES: usize = 64;
        const SIZE: u64 = 8 * 1024;
        let directory =
            std::env::temp_dir().join(format!("layerfs-payload-amortized-{}", std::process::id()));
        fs::create_dir_all(&directory)?;
        let payload = Payload::temporary(
            &directory,
            Limits {
                physical_bytes: 16 * 1024 * 1024,
                owners: FILES as usize + 16,
                readers: 4,
                writes: 4,
                index: IndexLimits {
                    max_pages: 65_536,
                    max_roots: 4096,
                    ..IndexLimits::default()
                },
            },
        )?;
        let mut live = Vec::new();
        for step in 0..FILES {
            live.push(payload.write_from(&mut Repeated(step as u8), SIZE)?);
        }
        let mut freed = 0_u64;
        for step in 0..FILES {
            let retired = live.remove(0);
            drop(retired);
            freed += SIZE;
            // Completed maintenance between deletions, exactly the shape that
            // made the former whole-arena evacuation quadratic.
            while payload.reclaim_step()? {}
            live.push(payload.write_from(&mut Repeated(step as u8), SIZE)?);
        }
        let after = payload.stats()?;
        println!(
            "payload amortized files={FILES} size={SIZE} freed={freed} relocated={} reused={} \
             unclaimed={} physical={} reclaimed={}",
            after.relocated_bytes,
            after.reused_bytes,
            after.unclaimed_bytes,
            after.physical_bytes,
            after.reclaimed_bytes
        );
        assert_eq!(after.live_block_bytes, FILES as u64 * SIZE, "{after:?}");
        assert!(
            after.relocated_bytes <= freed,
            "relocation must be charged to reclaimed bytes: {after:?}"
        );
        assert_eq!(after.unclaimed_bytes, 0, "{after:?}");
        drop(live);
        drain(&payload)?;
        assert_eq!(payload.stats()?.physical_bytes, 0);
        drop(payload);
        fs::remove_dir(&directory)?;
        Ok(())
    }

    /// A bounded top-of-arena live interval makes relocation impossible, so a
    /// small unrelated release must copy nothing at all and the next fitting
    /// write must reuse the unclaimed interval in place instead of growing the
    /// arena.
    #[test]
    fn middle_release_reuses_unclaimed_interval_without_relocating_live_payload() -> io::Result<()>
    {
        const SMALL: u64 = 4 * 1024;
        const LARGE: u64 = 4 * MOVE;
        let directory =
            std::env::temp_dir().join(format!("layerfs-payload-reuse-{}", std::process::id()));
        fs::create_dir_all(&directory)?;
        let payload = Payload::temporary(
            &directory,
            Limits {
                physical_bytes: 16 * 1024 * 1024,
                owners: 64,
                readers: 4,
                writes: 4,
                index: IndexLimits {
                    max_pages: 65_536,
                    max_roots: 4096,
                    ..IndexLimits::default()
                },
            },
        )?;
        let middle = payload.write_from(&mut Repeated(0x44), SMALL)?;
        let top = payload.write_from(&mut Repeated(0x33), LARGE)?;
        let before = payload.stats()?;
        let physical_before = before.physical_bytes;
        drop(middle);
        while payload.reclaim_step()? {}
        let settled = payload.stats()?;
        assert_eq!(
            settled.relocated_bytes, before.relocated_bytes,
            "a release under a large live interval must not move unrelated payload"
        );
        assert!(settled.unclaimed_bytes >= SMALL, "{settled:?}");
        let reuse = payload.write_from(&mut Repeated(0x55), SMALL)?;
        let after = payload.stats()?;
        assert!(
            after.reused_bytes >= SMALL,
            "the next fitting write must reuse the unclaimed interval: {after:?}"
        );
        assert_eq!(
            after.physical_bytes, physical_before,
            "reuse must not extend the arena: {after:?}"
        );
        let mut bytes = vec![0_u8; LARGE as usize];
        top.read_exact_at(&mut bytes, 0)?;
        assert!(bytes.iter().all(|byte| *byte == 0x33));
        let mut byte = [0_u8; SMALL as usize];
        reuse.read_exact_at(&mut byte, 0)?;
        assert!(byte.iter().all(|value| *value == 0x55));
        drop(reuse);
        drop(top);
        drain(&payload)?;
        assert_eq!(payload.stats()?.physical_bytes, 0);
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
    /// Tiny admission-cap oracle. The resident owner cap must bound genuinely
    /// simultaneous handles, and it must **not** become a ceiling on the number
    /// of persisted payload tokens a Workspace retains: those are charged to the
    /// independent catalog/arena disk quotas. Reclamation must also keep the
    /// payload index's live page count proportional to the tokens it holds.
    #[test]
    fn tiny_owner_cap_bounds_resident_handles_not_persisted_tokens() -> io::Result<()> {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-payload-tiny-owner-cap-{}",
            std::process::id()
        ));
        fs::create_dir_all(&directory)?;
        let payload = Payload::temporary(
            &directory,
            Limits {
                physical_bytes: 16 * 1024 * 1024,
                owners: 4,
                readers: 4,
                writes: 2,
                index: IndexLimits {
                    max_pages: 8192,
                    max_roots: 64,
                    ..IndexLimits::default()
                },
            },
        )?;
        // Simultaneous handles are still bounded by the configured cap, and a
        // rejected one leaves the admitted owners usable.
        let held: Vec<OwnedRange> = (0..4)
            .map(|_| payload.write_from(&mut &b"a"[..], 1).unwrap())
            .collect();
        assert_eq!(payload.stats()?.owners, 4);
        assert_eq!(
            payload.write_from(&mut &b"b"[..], 1).err().unwrap().kind(),
            ErrorKind::StorageFull
        );
        let mut byte = [0];
        held[3].read_exact_at(&mut byte, 0)?;
        assert_eq!(byte, [b'a']);
        drop(held);
        drain(&payload)?;
        assert_eq!(payload.stats()?.owners, 0);

        // Persisted tokens outlive their transient handles through an
        // independent metadata root. With a four-slot resident cap and no
        // external maintenance loop, sixty-four retained tokens must be
        // admitted: the bounded admission assistance retires each handle.
        let metadata = Index::temporary_with_owner(
            &directory,
            IndexLimits {
                max_pages: 8192,
                max_roots: 64,
                ..IndexLimits::default()
            },
            Arc::new(payload.clone()),
        )?;
        let mut root = metadata.empty_root()?;
        let mut tokens = Vec::new();
        let mut peak_resident = 0;
        for index in 0..64u64 {
            let owner = payload.write_from(&mut &[index as u8][..], 1)?;
            let token = owner.token();
            root = metadata.set_owned(
                &root,
                &index.to_be_bytes(),
                b"one retained byte",
                Some(token),
            )?;
            drop(owner);
            peak_resident = peak_resident.max(payload.stats()?.owners);
            assert!(
                payload.stats()?.owners <= 4,
                "resident charge escaped the configured cap: {:?}",
                payload.stats()?
            );
            tokens.push((token, index as u8));
        }
        // Bounded assistance retires the dropped handles, so the settled state
        // holds every token with no resident owner and a catalog proportional
        // to those tokens rather than to the write history.
        drain(&payload)?;
        let stats = payload.stats()?;
        assert_eq!(
            stats.persisted_tokens, 64,
            "every retained file keeps exactly one persisted token"
        );
        assert_eq!(stats.owners, 0, "{stats:?}");
        assert_eq!(stats.readers, 0, "{stats:?}");
        assert!(
            stats.index_live_pages <= 4 * 64 + 64,
            "payload catalog pages must stay proportional to live tokens: {stats:?}"
        );
        for (token, expected) in &tokens {
            payload.read_exact_at(*token, &mut byte, 0)?;
            assert_eq!(byte, [*expected]);
        }
        println!(
            "tiny-owner-cap PASS tokens={} peak_resident={peak_resident} live_pages={} allocated_pages={} pending_releases={}",
            stats.persisted_tokens,
            stats.index_live_pages,
            stats.index_allocated_pages,
            stats.pending_releases
        );

        // Dropping the retaining metadata root converges: tokens and catalog
        // pages both retire with it.
        drop(root);
        for _ in 0..1024 {
            metadata.reclaim(64)?;
        }
        drain(&payload)?;
        let settled = payload.stats()?;
        assert_eq!(settled.persisted_tokens, 0, "{settled:?}");
        assert_eq!(settled.owners, 0, "{settled:?}");
        assert_eq!(settled.index_live_pages, 0, "{settled:?}");
        assert!(!settled.cleanup_pending, "{settled:?}");
        drop(metadata);
        drop(payload);
        fs::remove_dir(&directory)?;
        Ok(())
    }

    /// Diagnostic (not a gate): page cost of one isolated one-byte write and of
    /// a run of them, so the per-file metadata footprint is measured rather
    /// than inferred from arithmetic.
    #[test]
    fn one_byte_write_index_page_cost() -> io::Result<()> {
        let directory =
            std::env::temp_dir().join(format!("layerfs-payload-pagecost-{}", std::process::id()));
        fs::create_dir_all(&directory)?;
        let payload = Payload::temporary(
            &directory,
            Limits {
                physical_bytes: 64 * 1024 * 1024,
                owners: 65_536,
                readers: 4,
                writes: 4,
                index: IndexLimits {
                    max_pages: 262_144,
                    max_roots: 16_384,
                    ..IndexLimits::default()
                },
            },
        )?;
        let mut previous = payload.0.index.stats()?;
        for index in 0..200 {
            let owner = payload.write_from(&mut &b"x"[..], 1)?;
            let stats = payload.0.index.stats()?;
            if index < 5 || index % 25 == 24 {
                // Deltas are signed: bounded catalog assistance may recycle more
                // retired pages than this write allocates, so a run of writes is
                // not a monotone live-page count.
                println!(
                    "pagecost index={index} live={} allocated={} page_writes={} delta_live={} \
                     delta_writes={}",
                    stats.live_pages,
                    stats.allocated_pages,
                    stats.page_writes,
                    stats.live_pages as i64 - previous.live_pages as i64,
                    stats.page_writes as i64 - previous.page_writes as i64
                );
            }
            previous = stats;
            drop(owner);
            payload.0.index.reclaim(64)?;
            let after = payload.0.index.stats()?;
            if index < 5 || index % 25 == 24 {
                println!(
                    "pagecost-after index={index} live={} allocated={} reclaimed={} pending={}",
                    after.live_pages,
                    after.allocated_pages,
                    after.reclaimed_pages,
                    after.reclamation_pending
                );
            }
        }
        let settled = payload.0.index.stats()?;
        println!(
            "pagecost files=200 live={} allocated={} page_writes={} physical={}",
            settled.live_pages,
            settled.allocated_pages,
            settled.page_writes,
            settled.physical_bytes
        );
        drop(payload);
        fs::remove_dir(&directory)?;
        Ok(())
    }

    #[test]
    fn chargeable_spool_counts_clipped_shared_coverage_and_excludes_sdk_inline() -> io::Result<()> {
        let payload = Payload::temporary(
            &std::env::temp_dir(),
            Limits {
                physical_bytes: 8 * MOVE,
                owners: 128,
                readers: 4,
                writes: 4,
                index: IndexLimits {
                    max_pages: 8192,
                    max_roots: 256,
                    ..IndexLimits::default()
                },
            },
        )?;
        let original = payload.write_from(&mut &b"ABCD"[..], 4)?;
        assert_eq!(payload.stats()?.chargeable_bytes, 4);
        assert_eq!(payload.stats()?.live_block_bytes, BLOCK);
        let original_token = original.token();
        payload.retain(original_token)?;
        let slice = original.subrange(1, 1)?;
        assert_eq!(
            payload.stats()?.chargeable_bytes,
            4,
            "shared coverage counted twice"
        );
        let tail = payload.append_to(&original, &mut &b"EFGH"[..], 4)?.unwrap();
        let joined = payload.join(&original, &tail)?;
        assert_eq!(payload.stats()?.chargeable_bytes, 8);
        drop(original);
        payload.release(original_token)?;
        drop((slice, tail));
        drain(&payload)?;
        assert_eq!(payload.stats()?.chargeable_bytes, 8);
        payload.0.state.lock().unwrap().fail_after = Some(0);
        assert!(payload.append_to(&joined, &mut &b"FAIL"[..], 4).is_err());
        assert_eq!(
            payload.stats()?.chargeable_bytes,
            8,
            "failed append changed published charge"
        );
        drain(&payload)?;
        drop(joined);
        drain(&payload)?;
        assert_eq!(payload.stats()?.chargeable_bytes, 0);

        let full = payload.write_from(&mut Repeated(0x52), 4 * BLOCK)?;
        let middle = full.subrange(BLOCK + 1, 1)?;
        let duplicate = middle.subrange(0, 1)?;
        drop(full);
        drain(&payload)?;
        assert_eq!(payload.stats()?.chargeable_bytes, BLOCK);
        assert_eq!(payload.stats()?.live_block_bytes, BLOCK);
        drop(middle);
        drain(&payload)?;
        assert_eq!(payload.stats()?.chargeable_bytes, BLOCK);
        let inline = payload.write_inline_from(&mut Repeated(0x61), 128)?;
        assert_eq!(
            payload.stats()?.chargeable_bytes,
            BLOCK,
            "SDK inline changed ordinary-spool charge"
        );
        assert_eq!(payload.stats()?.live_block_bytes, 2 * BLOCK);
        let mut unread = std::io::Cursor::new(b"new");
        assert!(payload.append_to(&inline, &mut unread, 3)?.is_none());
        assert_eq!(
            unread.position(),
            0,
            "ordinary append consumed inline-class input"
        );
        let ordinary = payload.write_from(&mut unread, 3)?;
        assert_eq!(payload.stats()?.chargeable_bytes, BLOCK + 3);
        drop((ordinary, inline));
        drain(&payload)?;
        assert_eq!(payload.stats()?.chargeable_bytes, BLOCK);
        drop(duplicate);
        drain(&payload)?;
        let settled = payload.stats()?;
        assert_eq!(settled.chargeable_bytes, 0);
        assert_eq!(settled.live_block_bytes, 0);
        assert_eq!(settled.physical_bytes, 0);
        Ok(())
    }
    #[test]
    fn explicit_sync_flushes_both_private_arenas_and_returns_io_failures() -> io::Result<()> {
        let mut payload = Payload::temporary(
            &std::env::temp_dir(),
            Limits {
                physical_bytes: 8 * MOVE,
                owners: 128,
                readers: 4,
                writes: 4,
                index: IndexLimits {
                    max_pages: 8192,
                    max_roots: 256,
                    ..IndexLimits::default()
                },
            },
        )?;
        let ordinary = payload.write_from(&mut &b"ordinary"[..], 8)?;
        let inline = payload.write_inline_from(&mut &b"inline"[..], 6)?;
        let before = payload.stats()?;
        payload.sync_data()?;
        assert_eq!(payload.stats()?.chargeable_bytes, before.chargeable_bytes);
        let mut bytes = [0; 8];
        ordinary.read_exact_at(&mut bytes, 0)?;
        assert_eq!(&bytes, b"ordinary");
        drop((ordinary, inline));
        for arena in 0..2 {
            let (socket, _peer) = std::os::unix::net::UnixStream::pair()?;
            let fd: std::os::fd::OwnedFd = socket.into();
            let original = std::mem::replace(
                &mut Arc::get_mut(&mut payload.0).unwrap().files[arena],
                File::from(fd),
            );
            assert!(payload.sync_data().is_err());
            Arc::get_mut(&mut payload.0).unwrap().files[arena] = original;
        }
        payload.sync_data()?;
        drain(&payload)?;
        assert_eq!(payload.stats()?.physical_bytes, 0);
        Ok(())
    }
}
