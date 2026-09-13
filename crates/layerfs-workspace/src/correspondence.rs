//! Metadata-only C1 predecessor correspondence. Logical lineage survives physical
//! backing substitution; this plan is never installed into live file contents.
use crate::overlay_index::{Index, Root};
use layerfs_content::ObjectId;
use layerfs_workspace_core::NodeId;
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, ErrorKind, Read, Seek, SeekFrom, Write};
use std::ops::Bound;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const BATCH: usize = 64;
const JOURNAL_BUFFER: usize = 16 * 1024;
const DESCRIPTOR_BYTES: usize = 49;
const PLAN_BYTES: usize = 25;
const ANCHOR_BYTES: usize = 24;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}
fn quota(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::StorageFull, message)
}
fn end(offset: u64, length: u64) -> io::Result<u64> {
    offset
        .checked_add(length)
        .ok_or_else(|| invalid("correspondence coordinate overflow"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct OriginId {
    workspace: [u8; 16],
    serial: u64,
}
impl OriginId {
    pub(crate) fn to_bytes(self) -> [u8; 24] {
        let mut bytes = [0; 24];
        bytes[..16].copy_from_slice(&self.workspace);
        bytes[16..].copy_from_slice(&self.serial.to_be_bytes());
        bytes
    }
    pub(crate) fn from_bytes(bytes: [u8; 24]) -> io::Result<Self> {
        let serial = u64::from_be_bytes(bytes[16..].try_into().unwrap());
        if serial == 0 {
            return Err(invalid("reserved origin identity"));
        }
        Ok(Self {
            workspace: bytes[..16].try_into().unwrap(),
            serial,
        })
    }
}
/// Exactly one allocator belongs to each live Workspace. It is not reconstructed
/// per file or Commit; a failed mutation burns its allocated identities.
pub(crate) struct OriginSequence {
    workspace: [u8; 16],
    next: AtomicU64,
}
impl OriginSequence {
    pub(crate) fn new(workspace: [u8; 16]) -> Self {
        Self {
            workspace,
            next: AtomicU64::new(1),
        }
    }
    pub(crate) fn allocate(&self) -> io::Result<OriginId> {
        let serial = self
            .next
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| quota("origin identity exhausted"))?;
        Ok(OriginId {
            workspace: self.workspace,
            serial,
        })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    Base,
    Temporary,
    Inline,
    Zero,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Descriptor {
    pub offset: u64,
    pub length: u64,
    pub origin: Option<OriginId>,
    pub origin_offset: u64,
    pub kind: Kind,
}
impl Descriptor {
    fn validate(self) -> io::Result<()> {
        if self.length == 0
            || (self.kind == Kind::Zero) != self.origin.is_none()
            || (self.kind == Kind::Zero && self.origin_offset != 0)
            || self.origin.is_some_and(|id| id.serial == 0)
        {
            return Err(invalid("invalid correspondence descriptor"));
        }
        end(self.offset, self.length)?;
        end(self.origin_offset, self.length)?;
        Ok(())
    }
    fn encode(self) -> [u8; DESCRIPTOR_BYTES] {
        let mut bytes = [0; DESCRIPTOR_BYTES];
        bytes[..8].copy_from_slice(&self.offset.to_be_bytes());
        bytes[8..16].copy_from_slice(&self.length.to_be_bytes());
        if let Some(origin) = self.origin {
            bytes[16..40].copy_from_slice(&origin.to_bytes());
        }
        bytes[40..48].copy_from_slice(&self.origin_offset.to_be_bytes());
        bytes[48] = match self.kind {
            Kind::Base => 0,
            Kind::Temporary => 1,
            Kind::Inline => 2,
            Kind::Zero => 3,
        };
        bytes
    }
    fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() != DESCRIPTOR_BYTES {
            return Err(invalid("correspondence descriptor length"));
        }
        let kind = match bytes[48] {
            0 => Kind::Base,
            1 => Kind::Temporary,
            2 => Kind::Inline,
            3 => Kind::Zero,
            _ => return Err(invalid("correspondence descriptor kind")),
        };
        let descriptor = Self {
            offset: u64::from_be_bytes(bytes[..8].try_into().unwrap()),
            length: u64::from_be_bytes(bytes[8..16].try_into().unwrap()),
            origin: if kind == Kind::Zero {
                None
            } else {
                Some(OriginId::from_bytes(bytes[16..40].try_into().unwrap())?)
            },
            origin_offset: u64::from_be_bytes(bytes[40..48].try_into().unwrap()),
            kind,
        };
        descriptor.validate()?;
        if descriptor.encode().as_slice() != bytes {
            return Err(invalid("noncanonical correspondence descriptor"));
        }
        Ok(descriptor)
    }
    fn coalesces(self, next: Self) -> bool {
        self.offset + self.length == next.offset
            && self.kind == next.kind
            && self.origin == next.origin
            && (self.kind == Kind::Zero || self.origin_offset + self.length == next.origin_offset)
    }
}
fn origin_key(origin: OriginId, offset: u64) -> [u8; 32] {
    let mut key = [0; 32];
    key[..16].copy_from_slice(&origin.workspace);
    key[16..24].copy_from_slice(&origin.serial.to_be_bytes());
    key[24..].copy_from_slice(&offset.to_be_bytes());
    key
}

/// The caller stores only its latest published header per inode and shares roots
/// for unchanged files. Active snapshots may independently retain these roots.
#[derive(Clone)]
pub(crate) struct Description {
    index: Index,
    offsets: Root,
    origins: Root,
    pub inode: NodeId,
    pub canonical_root: Option<ObjectId>,
    pub length: u64,
    pub records: u64,
    pub build_index_operations: u64,
}
impl Description {
    pub(crate) fn build(
        index: &Index,
        inode: NodeId,
        canonical_root: Option<ObjectId>,
        length: u64,
        max_records: u64,
        descriptors: impl IntoIterator<Item = io::Result<Descriptor>>,
    ) -> io::Result<Self> {
        if inode.0 == 0 || max_records == 0 {
            return Err(invalid("invalid correspondence header limits"));
        }
        let mut result = Self {
            index: index.clone(),
            offsets: index.empty_root()?,
            origins: index.empty_root()?,
            inode,
            canonical_root,
            length,
            records: 0,
            build_index_operations: 0,
        };
        let mut pending: Option<Descriptor> = None;
        let mut position = 0;
        for item in descriptors {
            let descriptor = item?;
            descriptor.validate()?;
            if descriptor.offset != position {
                return Err(invalid("noncontiguous descriptor stream"));
            }
            position = end(position, descriptor.length)?;
            if position > length {
                return Err(invalid("descriptor exceeds file length"));
            }
            if let Some(previous) = pending.as_mut() {
                if previous.coalesces(descriptor) {
                    previous.length = end(previous.length, descriptor.length)?;
                    continue;
                }
            }
            if let Some(previous) = pending.replace(descriptor) {
                result.insert(previous, max_records)?;
            }
        }
        if position != length {
            return Err(invalid("descriptor stream length mismatch"));
        }
        if let Some(previous) = pending {
            result.insert(previous, max_records)?;
        }
        Ok(result)
    }
    fn insert(&mut self, descriptor: Descriptor, max_records: u64) -> io::Result<()> {
        if self.records == max_records {
            return Err(quota("correspondence descriptor limit"));
        }
        if let Some(origin) = descriptor.origin {
            self.build_index_operations += 3; // floor, right neighbor, and origin insertion
            let key = origin_key(origin, descriptor.origin_offset);
            let stop = end(descriptor.origin_offset, descriptor.length)?;
            if let Some((_, value)) = self.index.floor(&self.origins, &key)? {
                let previous = Descriptor::decode(&value)?;
                if previous.origin == Some(origin)
                    && end(previous.origin_offset, previous.length)? > descriptor.origin_offset
                {
                    return Err(invalid("duplicated nonzero origin coordinates"));
                }
            }
            if let Some((_, value)) = self
                .index
                .scan(
                    &self.origins,
                    Bound::Included(key.as_slice()),
                    Bound::Unbounded,
                    1,
                )?
                .first()
            {
                let next = Descriptor::decode(value)?;
                if next.origin == Some(origin) && next.origin_offset < stop {
                    return Err(invalid("overlapping nonzero origin coordinates"));
                }
            }
            self.origins = self.index.set(&self.origins, &key, &descriptor.encode())?;
        }
        self.build_index_operations += 1;
        self.offsets = self.index.set(
            &self.offsets,
            &descriptor.offset.to_be_bytes(),
            &descriptor.encode(),
        )?;
        self.records += 1;
        Ok(())
    }
    fn cursor(&self) -> DescriptorCursor<'_> {
        DescriptorCursor {
            description: self,
            after: None,
            rows: VecDeque::new(),
            done: false,
            seeks: 0,
        }
    }
}
struct DescriptorCursor<'a> {
    description: &'a Description,
    after: Option<Vec<u8>>,
    rows: VecDeque<(Vec<u8>, Vec<u8>)>,
    done: bool,
    seeks: u64,
}
impl DescriptorCursor<'_> {
    fn next(&mut self, visits: &mut u64) -> io::Result<Option<Descriptor>> {
        if self.rows.is_empty() && !self.done {
            self.seeks += 1;
            let start = self
                .after
                .as_deref()
                .map_or(Bound::Unbounded, Bound::Excluded);
            let rows = self.description.index.scan(
                &self.description.offsets,
                start,
                Bound::Unbounded,
                BATCH,
            )?;
            self.done = rows.len() < BATCH;
            self.after = rows.last().map(|(key, _)| key.clone());
            self.rows = rows.into();
        }
        if let Some((key, bytes)) = self.rows.pop_front() {
            let descriptor = Descriptor::decode(&bytes)?;
            if key.as_slice() != descriptor.offset.to_be_bytes() {
                return Err(invalid("descriptor index coordinate mismatch"));
            }
            *visits += 1;
            Ok(Some(descriptor))
        } else {
            Ok(None)
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Counters {
    pub origin_seeks: u64,
    pub offset_seeks: u64,
    pub origin_visits: u64,
    pub old_descriptor_visits: u64,
    pub new_descriptor_visits: u64,
    pub anchors: u64,
    pub fragments: u64,
    pub backward_bytes: u64,
    pub base_bytes: u64,
    pub replacement_bytes: u64,
    pub journal_bytes: u64,
    /// Actual allocated bytes while both unlinked journals remain owned.
    pub journal_physical_bytes: u64,
    pub retained_plan_physical_bytes: u64,
    /// Component buffer bound, excluding the independently budgeted Index pages.
    pub buffer_bound_bytes: usize,
    pub payload_bytes_read: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Span {
    Base {
        old_offset: u64,
        new_offset: u64,
        length: u64,
    },
    Replacement {
        new_offset: u64,
        length: u64,
        zero: bool,
    },
}
impl Span {
    fn position(self) -> (u64, u64) {
        match self {
            Self::Base {
                new_offset, length, ..
            }
            | Self::Replacement {
                new_offset, length, ..
            } => (new_offset, length),
        }
    }
    fn encode(self) -> [u8; PLAN_BYTES] {
        let mut bytes = [0; PLAN_BYTES];
        let (new, length) = self.position();
        bytes[8..16].copy_from_slice(&new.to_be_bytes());
        bytes[16..24].copy_from_slice(&length.to_be_bytes());
        match self {
            Self::Base { old_offset, .. } => {
                bytes[..8].copy_from_slice(&old_offset.to_be_bytes());
                bytes[24] = 0;
            }
            Self::Replacement { zero, .. } => bytes[24] = if zero { 2 } else { 1 },
        }
        bytes
    }
    fn decode(bytes: [u8; PLAN_BYTES]) -> io::Result<Self> {
        let old_offset = u64::from_be_bytes(bytes[..8].try_into().unwrap());
        let new_offset = u64::from_be_bytes(bytes[8..16].try_into().unwrap());
        let length = u64::from_be_bytes(bytes[16..24].try_into().unwrap());
        match bytes[24] {
            0 => Ok(Self::Base {
                old_offset,
                new_offset,
                length,
            }),
            1 | 2 if old_offset == 0 => Ok(Self::Replacement {
                new_offset,
                length,
                zero: bytes[24] == 2,
            }),
            _ => Err(invalid("invalid predecessor plan record")),
        }
    }
}
#[derive(Clone, Copy)]
struct Anchor {
    old: u64,
    new: u64,
    length: u64,
}
impl Anchor {
    fn encode(self) -> [u8; ANCHOR_BYTES] {
        let mut bytes = [0; ANCHOR_BYTES];
        bytes[..8].copy_from_slice(&self.old.to_be_bytes());
        bytes[8..16].copy_from_slice(&self.new.to_be_bytes());
        bytes[16..].copy_from_slice(&self.length.to_be_bytes());
        bytes
    }
    fn decode(bytes: [u8; ANCHOR_BYTES]) -> Self {
        Self {
            old: u64::from_be_bytes(bytes[..8].try_into().unwrap()),
            new: u64::from_be_bytes(bytes[8..16].try_into().unwrap()),
            length: u64::from_be_bytes(bytes[16..].try_into().unwrap()),
        }
    }
}
fn temporary(directory: &Path) -> io::Result<File> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    loop {
        let path = directory.join(format!(
            ".correspondence-{}-{}",
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
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}
fn charge(counters: &mut Counters, limit: u64, bytes: usize) -> io::Result<()> {
    let next = end(counters.journal_bytes, bytes as u64)?;
    if next > limit {
        return Err(quota("correspondence journal limit"));
    }
    counters.journal_bytes = next;
    Ok(())
}
fn read_record<const N: usize>(reader: &mut impl Read) -> io::Result<Option<[u8; N]>> {
    let mut bytes = [0; N];
    if reader.read(&mut bytes[..1])? == 0 {
        return Ok(None);
    }
    reader.read_exact(&mut bytes[1..])?;
    Ok(Some(bytes))
}

#[derive(Debug)]
pub(crate) struct Plan {
    file: File,
    pub counters: Counters,
    pub old_root: ObjectId,
    pub new_length: u64,
    old_length: u64,
}
impl Plan {
    pub(crate) fn into_iter(mut self) -> io::Result<PlanCursor> {
        self.file.seek(SeekFrom::Start(0))?;
        Ok(PlanCursor {
            reader: BufReader::with_capacity(JOURNAL_BUFFER, self.file),
            position: 0,
            old_end: 0,
            length: self.new_length,
            old_length: self.old_length,
            done: false,
        })
    }
}
pub(crate) struct PlanCursor {
    reader: BufReader<File>,
    position: u64,
    old_end: u64,
    length: u64,
    old_length: u64,
    done: bool,
}
impl Iterator for PlanCursor {
    type Item = io::Result<Span>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let result = (|| {
            let Some(bytes) = read_record(&mut self.reader)? else {
                if self.position != self.length {
                    return Err(invalid("incomplete predecessor plan"));
                }
                return Ok(None);
            };
            let span = Span::decode(bytes)?;
            let (offset, length) = span.position();
            if offset != self.position || length == 0 {
                return Err(invalid("noncontiguous predecessor plan"));
            }
            self.position = end(offset, length)?;
            if self.position > self.length {
                return Err(invalid("predecessor plan beyond EOF"));
            }
            if let Span::Base { old_offset, .. } = span {
                if old_offset < self.old_end {
                    return Err(invalid("backward predecessor span"));
                }
                self.old_end = end(old_offset, length)?;
                if self.old_end > self.old_length {
                    return Err(invalid("predecessor plan beyond old EOF"));
                }
            }
            Ok(Some(span))
        })();
        match result {
            Ok(Some(span)) => Some(Ok(span)),
            Ok(None) => {
                self.done = true;
                None
            }
            Err(e) => {
                self.done = true;
                Some(Err(e))
            }
        }
    }
}

/// Two metadata passes: nonzero anchors first, then zeros inside disjoint anchor
/// gaps. The result owns two bounded journals during construction and one after.
pub(crate) fn plan(
    old: &Description,
    current: &Description,
    directory: &Path,
    journal_limit: u64,
) -> io::Result<Plan> {
    if old.inode != current.inode {
        return Err(invalid("correspondence inode mismatch"));
    }
    let old_root = old
        .canonical_root
        .ok_or_else(|| invalid("missing canonical predecessor root"))?;
    let mut counters = Counters {
        buffer_bound_bytes: 2 * JOURNAL_BUFFER
            + 3 * BATCH * (DESCRIPTOR_BYTES + 32 + 2 * std::mem::size_of::<Vec<u8>>())
            + 4 * DESCRIPTOR_BYTES,
        ..Counters::default()
    };
    let mut anchors = BufWriter::with_capacity(JOURNAL_BUFFER, temporary(directory)?);
    let mut cursor = current.cursor();
    let mut old_end = 0_u64;
    while let Some(descriptor) = cursor.next(&mut counters.new_descriptor_visits)? {
        let Some(origin) = descriptor.origin else {
            continue;
        };
        let query_end = end(descriptor.origin_offset, descriptor.length)?;
        let key = origin_key(origin, descriptor.origin_offset);
        counters.origin_seeks += 1;
        let floor = old.index.floor(&old.origins, &key)?;
        let start = if let Some((key, value)) = floor {
            counters.origin_visits += 1;
            let prior = Descriptor::decode(&value)?;
            if prior.origin == Some(origin)
                && end(prior.origin_offset, prior.length)? > descriptor.origin_offset
            {
                key
            } else {
                origin_key(origin, descriptor.origin_offset).to_vec()
            }
        } else {
            key.to_vec()
        };
        let mut after: Option<Vec<u8>> = None;
        let stop = origin_key(origin, query_end);
        loop {
            counters.origin_seeks += 1;
            let lower = after
                .as_deref()
                .map_or(Bound::Included(start.as_slice()), Bound::Excluded);
            let rows =
                old.index
                    .scan(&old.origins, lower, Bound::Excluded(stop.as_slice()), BATCH)?;
            let done = rows.len() < BATCH;
            after = rows.last().map(|(key, _)| key.clone());
            for (_, value) in rows {
                counters.origin_visits += 1;
                let prior = Descriptor::decode(&value)?;
                if prior.origin != Some(origin) {
                    return Err(invalid("origin interval query crossed identity"));
                }
                let left = descriptor.origin_offset.max(prior.origin_offset);
                let right = query_end.min(end(prior.origin_offset, prior.length)?);
                if left >= right {
                    continue;
                }
                let mut anchor = Anchor {
                    old: end(prior.offset, left - prior.origin_offset)?,
                    new: end(descriptor.offset, left - descriptor.origin_offset)?,
                    length: right - left,
                };
                let trim = old_end.saturating_sub(anchor.old).min(anchor.length);
                counters.backward_bytes = end(counters.backward_bytes, trim)?;
                anchor.old += trim;
                anchor.new += trim;
                anchor.length -= trim;
                if anchor.length == 0 {
                    continue;
                }
                old_end = end(anchor.old, anchor.length)?;
                charge(&mut counters, journal_limit, ANCHOR_BYTES)?;
                anchors.write_all(&anchor.encode())?;
                counters.anchors += 1;
            }
            if done {
                break;
            }
        }
    }
    counters.offset_seeks += cursor.seeks;
    drop(cursor);
    let mut anchor_file = anchors.into_inner().map_err(|e| e.into_error())?;
    anchor_file.seek(SeekFrom::Start(0))?;
    let mut anchors = BufReader::with_capacity(JOURNAL_BUFFER, anchor_file);
    let mut output = Emitter {
        writer: BufWriter::with_capacity(JOURNAL_BUFFER, temporary(directory)?),
        pending: None,
        position: 0,
        old_end: 0,
        old_length: old.length,
        counters: &mut counters,
        limit: journal_limit,
    };
    let mut old_window = Window {
        cursor: old.cursor(),
        current: None,
    };
    let mut new_window = Window {
        cursor: current.cursor(),
        current: None,
    };
    let (mut previous_old, mut previous_new) = (0, 0);
    loop {
        let record = read_record(&mut anchors)?;
        let anchor = record.map(Anchor::decode).unwrap_or(Anchor {
            old: old.length,
            new: current.length,
            length: 0,
        });
        if anchor.old < previous_old
            || anchor.new < previous_new
            || end(anchor.old, anchor.length)? > old.length
            || end(anchor.new, anchor.length)? > current.length
        {
            return Err(invalid("nonmonotonic correspondence anchors"));
        }
        gap(
            &mut old_window,
            &mut new_window,
            previous_old,
            anchor.old,
            previous_new,
            anchor.new,
            &mut output,
        )?;
        if anchor.length != 0 {
            output.push(Span::Base {
                old_offset: anchor.old,
                new_offset: anchor.new,
                length: anchor.length,
            })?;
        }
        previous_old = end(anchor.old, anchor.length)?;
        previous_new = end(anchor.new, anchor.length)?;
        if record.is_none() {
            break;
        }
    }
    output.flush()?;
    output.counters.offset_seeks += old_window.cursor.seeks + new_window.cursor.seeks;
    let file = output.writer.into_inner().map_err(|e| e.into_error())?;
    counters.retained_plan_physical_bytes = file.metadata()?.blocks().saturating_mul(512);
    counters.journal_physical_bytes = end(
        counters.retained_plan_physical_bytes,
        anchors.get_ref().metadata()?.blocks().saturating_mul(512),
    )?;
    Ok(Plan {
        file,
        counters,
        old_root,
        new_length: current.length,
        old_length: old.length,
    })
}
struct Window<'a> {
    cursor: DescriptorCursor<'a>,
    current: Option<Descriptor>,
}
impl Window<'_> {
    fn piece(
        &mut self,
        position: &mut u64,
        stop: u64,
        visits: &mut u64,
    ) -> io::Result<Option<(u64, u64, Kind)>> {
        if *position >= stop {
            return Ok(None);
        }
        loop {
            if let Some(descriptor) = self.current {
                let finish = end(descriptor.offset, descriptor.length)?;
                if finish > *position {
                    if descriptor.offset > *position {
                        return Err(invalid("descriptor window hole"));
                    }
                    let start = *position;
                    *position = finish.min(stop);
                    return Ok(Some((start, *position - start, descriptor.kind)));
                }
            }
            self.current = self.cursor.next(visits)?;
            if self.current.is_none() {
                return Err(invalid("descriptor window beyond EOF"));
            }
        }
    }
}
struct Emitter<'a> {
    writer: BufWriter<File>,
    pending: Option<Span>,
    position: u64,
    old_end: u64,
    old_length: u64,
    counters: &'a mut Counters,
    limit: u64,
}
impl Emitter<'_> {
    fn push(&mut self, span: Span) -> io::Result<()> {
        let (offset, length) = span.position();
        if length == 0 || offset != self.position {
            return Err(invalid("invalid emitted plan position"));
        }
        self.position = end(offset, length)?;
        match span {
            Span::Base { old_offset, .. } => {
                let stop = end(old_offset, length)?;
                if old_offset < self.old_end || stop > self.old_length {
                    return Err(invalid("invalid emitted predecessor span"));
                }
                self.old_end = stop;
                self.counters.base_bytes = end(self.counters.base_bytes, length)?;
            }
            Span::Replacement { .. } => {
                self.counters.replacement_bytes = end(self.counters.replacement_bytes, length)?
            }
        }
        if let Some(previous) = self.pending.as_mut() {
            match (previous, span) {
                (
                    Span::Base {
                        old_offset: old,
                        length: len,
                        ..
                    },
                    Span::Base { old_offset, .. },
                ) if *old + *len == old_offset => {
                    *len = end(*len, length)?;
                    return Ok(());
                }
                (
                    Span::Replacement {
                        length: len,
                        zero: a,
                        ..
                    },
                    Span::Replacement { zero: b, .. },
                ) if *a == b => {
                    *len = end(*len, length)?;
                    return Ok(());
                }
                _ => {}
            }
        }
        self.flush()?;
        self.pending = Some(span);
        Ok(())
    }
    fn flush(&mut self) -> io::Result<()> {
        if let Some(span) = self.pending.take() {
            charge(self.counters, self.limit, PLAN_BYTES)?;
            self.writer.write_all(&span.encode())?;
            self.counters.fragments += 1;
        }
        Ok(())
    }
}
fn gap(
    old: &mut Window<'_>,
    current: &mut Window<'_>,
    mut old_position: u64,
    old_stop: u64,
    mut new_position: u64,
    new_stop: u64,
    output: &mut Emitter<'_>,
) -> io::Result<()> {
    let mut zero: Option<(u64, u64)> = None;
    while let Some((start, length, kind)) = current.piece(
        &mut new_position,
        new_stop,
        &mut output.counters.new_descriptor_visits,
    )? {
        if kind != Kind::Zero {
            output.push(Span::Replacement {
                new_offset: start,
                length,
                zero: false,
            })?;
            continue;
        }
        let (mut position, mut remaining) = (start, length);
        while remaining > 0 {
            while zero.is_none() {
                match old.piece(
                    &mut old_position,
                    old_stop,
                    &mut output.counters.old_descriptor_visits,
                )? {
                    Some((start, length, Kind::Zero)) => zero = Some((start, length)),
                    Some(_) => continue,
                    None => break,
                }
            }
            let Some((old_start, old_length)) = zero else {
                output.push(Span::Replacement {
                    new_offset: position,
                    length: remaining,
                    zero: true,
                })?;
                break;
            };
            let length = remaining.min(old_length);
            output.push(Span::Base {
                old_offset: old_start,
                new_offset: position,
                length,
            })?;
            position += length;
            remaining -= length;
            zero = (length < old_length).then_some((old_start + length, old_length - length));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay_index::Limits;

    struct Fixture {
        directory: std::path::PathBuf,
        index: Index,
        origins: OriginSequence,
    }
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let directory = std::env::temp_dir().join(format!(
                "layerfs-correspondence-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).unwrap();
            let index = Index::temporary(
                &directory,
                Limits {
                    max_pages: 2048,
                    max_roots: 1024,
                    ..Limits::default()
                },
            )
            .unwrap();
            Self {
                directory,
                index,
                origins: OriginSequence::new([1; 16]),
            }
        }
        fn origin(&self) -> OriginId {
            self.origins.allocate().unwrap()
        }
        fn description(&self, descriptors: &[Descriptor], canonical: bool) -> Description {
            let length = descriptors.last().map_or(0, |d| d.offset + d.length);
            Description::build(
                &self.index,
                NodeId(7),
                canonical.then(|| ObjectId::for_bytes(b"canonical predecessor")),
                length,
                4096,
                descriptors.iter().copied().map(Ok),
            )
            .unwrap()
        }
        fn check(&self, old: &[Descriptor], current: &[Descriptor], expected: &[Span]) -> Counters {
            let old = self.description(old, true);
            let current = self.description(current, false);
            let plan = plan(&old, &current, &self.directory, 1024 * 1024).unwrap();
            assert_eq!(plan.old_root, old.canonical_root.unwrap());
            let counters = plan.counters;
            assert_eq!(
                plan.into_iter()
                    .unwrap()
                    .collect::<io::Result<Vec<_>>>()
                    .unwrap(),
                expected
            );
            assert_eq!(
                counters.base_bytes + counters.replacement_bytes,
                current.length
            );
            assert_eq!(counters.payload_bytes_read, 0);
            counters
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.directory).unwrap();
        }
    }
    fn data(offset: u64, length: u64, origin: OriginId, origin_offset: u64) -> Descriptor {
        Descriptor {
            offset,
            length,
            origin: Some(origin),
            origin_offset,
            kind: Kind::Base,
        }
    }
    fn zero(offset: u64, length: u64) -> Descriptor {
        Descriptor {
            offset,
            length,
            origin: None,
            origin_offset: 0,
            kind: Kind::Zero,
        }
    }
    fn base(old_offset: u64, new_offset: u64, length: u64) -> Span {
        Span::Base {
            old_offset,
            new_offset,
            length,
        }
    }
    fn replacement(new_offset: u64, length: u64, zero: bool) -> Span {
        Span::Replacement {
            new_offset,
            length,
            zero,
        }
    }

    #[test]
    fn shifted_partial_ranges_and_fresh_occurrence_lineage() {
        let f = Fixture::new();
        let (a, x, y) = (f.origin(), f.origin(), f.origin());
        let counters = f.check(
            &[
                data(0, 100, a, 0),
                data(100, 10, x, 0),
                data(110, 100, a, 100),
            ],
            &[
                data(0, 50, a, 0),
                data(50, 3, y, 0),
                data(53, 50, a, 50),
                data(103, 10, x, 0),
                data(113, 100, a, 100),
            ],
            &[base(0, 0, 50), replacement(50, 3, false), base(50, 53, 160)],
        );
        assert_eq!(counters.replacement_bytes, 3);
        f.check(
            &[data(0, 100, a, 10)],
            &[data(0, 7, a, 33)],
            &[base(23, 0, 7)],
        );
        // Identical bytes/backing with fresh occurrence identity cannot steal an
        // existing anchor. Equality requires lineage, not a matching content hash.
        let mut fresh = data(0, 100, y, 0);
        fresh.kind = Kind::Inline;
        f.check(
            &[data(0, 100, a, 0)],
            &[fresh],
            &[replacement(0, 100, false)],
        );
        // Equivalent physical substitution changes kind, not logical lineage.
        let mut relocated = data(0, 100, a, 0);
        relocated.kind = Kind::Temporary;
        f.check(&[data(0, 100, a, 0)], &[relocated], &[base(0, 0, 100)]);
        let reordered = f.check(
            &[data(0, 4, a, 0), data(4, 4, x, 0)],
            &[data(0, 4, x, 0), data(4, 4, a, 0)],
            &[base(4, 0, 4), replacement(4, 4, false)],
        );
        assert_eq!(reordered.backward_bytes, 4);
    }

    #[test]
    fn zero_insertion_and_deletion_are_bounded_by_nonzero_anchors() {
        let f = Fixture::new();
        let (a, b, y) = (f.origin(), f.origin(), f.origin());
        f.check(
            &[data(0, 100, a, 0), zero(100, 1)],
            &[zero(0, 1), data(1, 100, a, 0), zero(101, 1)],
            &[replacement(0, 1, true), base(0, 1, 101)],
        );
        let huge = 1_u64 << 50;
        let counters = f.check(
            &[data(0, 1, a, 0), zero(1, huge), data(huge + 1, 1, b, 0)],
            &[zero(0, huge), data(huge, 1, b, 0)],
            &[base(1, 0, huge + 1)],
        );
        assert_eq!(counters.anchors, 1);
        assert_eq!(counters.replacement_bytes, 0);
        assert_eq!(counters.journal_bytes, (ANCHOR_BYTES + PLAN_BYTES) as u64);
        assert!(counters.new_descriptor_visits <= 4);
        assert!(counters.old_descriptor_visits <= 3);
        assert!(counters.origin_visits <= 2);
        assert!(counters.buffer_bound_bytes < 64 * 1024);
        f.check(
            &[zero(0, 3), data(3, 2, a, 0), zero(5, 4)],
            &[zero(0, 2), data(2, 1, y, 0), zero(3, 6)],
            &[
                base(0, 0, 2),
                replacement(2, 1, false),
                base(2, 3, 1),
                base(5, 4, 4),
                replacement(8, 1, true),
            ],
        );
        f.check(&[], &[zero(0, huge)], &[replacement(0, huge, true)]);
        f.check(&[zero(0, huge)], &[], &[]);
    }

    #[test]
    fn descriptor_validation_quota_and_origin_exhaustion_preserve_old_inputs() {
        let f = Fixture::new();
        let a = f.origin();
        let original = f.description(&[data(0, 10, a, 0)], true);
        let invalid_sets = [
            vec![data(0, 5, a, 0), data(5, 5, a, 4)],
            vec![data(0, 5, a, 5), data(5, 5, a, 4)],
            vec![data(1, 4, a, 0)],
            vec![data(0, 2, a, u64::MAX)],
        ];
        for descriptors in invalid_sets {
            let length = descriptors.last().unwrap().offset + descriptors.last().unwrap().length;
            assert!(Description::build(
                &f.index,
                NodeId(7),
                None,
                length,
                10,
                descriptors.into_iter().map(Ok)
            )
            .is_err());
        }
        assert!(Description::build(
            &f.index,
            NodeId(7),
            None,
            2,
            1,
            [Ok(data(0, 1, a, 0)), Ok(zero(1, 1))]
        )
        .is_err());
        let current = f.description(&[data(0, 10, a, 0)], false);
        assert!(matches!(
            plan(&original, &current, &f.directory, ANCHOR_BYTES as u64)
                .unwrap_err()
                .kind(),
            ErrorKind::StorageFull
        ));
        assert_eq!(
            plan(&original, &current, &f.directory, 1024)
                .unwrap()
                .into_iter()
                .unwrap()
                .collect::<io::Result<Vec<_>>>()
                .unwrap(),
            [base(0, 0, 10)]
        );
        assert_eq!(
            fs::read_dir(&f.directory).unwrap().count(),
            0,
            "journals and indexes must be unlinked"
        );
        f.origins.next.store(u64::MAX, Ordering::Relaxed);
        assert_eq!(
            f.origins.allocate().unwrap_err().kind(),
            ErrorKind::StorageFull
        );
        assert_eq!(f.origins.next.load(Ordering::Relaxed), u64::MAX);
    }

    #[test]
    fn indexed_overlap_seek_and_streamed_fragments_do_not_expand_payload() {
        let f = Fixture::new();
        let origin = f.origin();
        let old = (0..180)
            .map(|i| data(i * 3, 3, origin, i * 7))
            .collect::<Vec<_>>();
        let current = (0..180)
            .map(|i| data(i * 2, 2, origin, i * 7 + 1))
            .collect::<Vec<_>>();
        let expected = (0..180)
            .map(|i| base(i * 3 + 1, i * 2, 2))
            .collect::<Vec<_>>();
        let counters = f.check(&old, &current, &expected);
        assert_eq!(counters.anchors, 180);
        assert_eq!(counters.fragments, 180);
        assert_eq!(counters.replacement_bytes, 0);
        assert!(counters.origin_visits <= 360);
        assert!(counters.offset_seeks <= 6);
        assert_eq!(counters.new_descriptor_visits, 180);
        let description = f.description(&old, true);
        assert_eq!(description.records, 180);
        assert_eq!(description.build_index_operations, 720);
        let cloned = description.clone();
        drop(description);
        assert_eq!(cloned.cursor().next(&mut 0).unwrap().unwrap(), old[0]);
    }
}
