//! One preserved forward metadata cursor and one pending bounded window/gap.
use super::state::Shared;
use crate::{OverlayCapturedRuns, OverlayOperationRecords};
use layerfs_content::{ContentError, ContentResult, FileRun};
use layerfs_overlay::{
    CapturedGap, CapturedReader, CapturedRunCursor, CapturedRunStep, InodeKind, LocalRead,
    CELL_BYTES, MASK_BYTES,
};

enum Pending {
    Gap {
        start: u64,
        end: u64,
        kind: CapturedGap,
    },
    Window(Box<LocalRead>),
}
pub(super) struct Scan {
    reader: CapturedReader,
    serial: u64,
    size: u64,
    base_size: u64,
    cursor: CapturedRunCursor,
    pending: Option<Pending>,
}
#[derive(Clone, Copy)]
pub(super) struct Span {
    pub start: u64,
    pub end: u64,
    pub changed: bool,
}
impl Scan {
    pub fn new(
        reader: CapturedReader,
        serial: u64,
        size: u64,
        base_size: u64,
    ) -> ContentResult<Self> {
        let cursor = CapturedRunCursor::new(reader, serial, 0, size)
            .map_err(|_| ContentError::InvalidRecord("captured file cursor"))?;
        Ok(Self {
            reader,
            serial,
            size,
            base_size,
            cursor,
            pending: None,
        })
    }
    fn bounds(&self) -> Option<(u64, u64)> {
        match self.pending.as_ref()? {
            Pending::Gap { start, end, .. } => Some((*start, *end)),
            Pending::Window(read) => Some((read.offset, read.offset + read.data.len() as u64)),
        }
    }
    fn ensure<P: OverlayCapturedRuns + OverlayOperationRecords + ?Sized>(
        &mut self,
        provider: &P,
        shared: &Shared<'_, P>,
        at: u64,
        normalization: bool,
    ) -> ContentResult<()> {
        shared.borrow().ready()?;
        if at >= self.size {
            return Err(ContentError::InvalidRecord("captured file source EOF"));
        }
        if self
            .bounds()
            .is_some_and(|(start, end)| at >= start && at < end)
        {
            return Ok(());
        }
        self.pending = None;
        if at < self.cursor.offset() {
            return Err(ContentError::InvalidRecord("captured file cursor reversal"));
        }
        self.cursor = self
            .cursor
            .seek_forward(at)
            .map_err(|_| ContentError::InvalidRecord("captured file cursor seek"))?;
        loop {
            shared.borrow().ready()?;
            {
                let mut owner = shared.borrow_mut();
                let calls = if normalization {
                    &mut owner.work.normalization_steps
                } else {
                    &mut owner.work.source_steps
                };
                *calls = calls.checked_add(1).ok_or(ContentError::LengthOverflow)?;
            }
            let reply = match provider.captured_run_step(self.cursor) {
                Ok(reply) => reply,
                Err(original) => return Err(shared.borrow_mut().original(original)),
            };
            {
                let mut owner = shared.borrow_mut();
                let work = &mut owner.work;
                work.metadata_rows = work
                    .metadata_rows
                    .checked_add(reply.work.metadata_rows)
                    .ok_or(ContentError::LengthOverflow)?;
                work.stale_rows = work
                    .stale_rows
                    .checked_add(reply.work.stale_rows)
                    .ok_or(ContentError::LengthOverflow)?;
            }
            let valid = |next: CapturedRunCursor| {
                next.reader() == self.reader
                    && next.serial() == self.serial
                    && next.logical_size() == self.size
            };
            match reply.step {
                CapturedRunStep::Continue(next)
                    if valid(next)
                        && next.offset() == at
                        && reply.work.metadata_rows != 0
                        && next.metadata_advanced_from(self.cursor) =>
                {
                    self.cursor = next
                }
                CapturedRunStep::Gap {
                    kind,
                    offset,
                    length,
                    next,
                } if valid(next)
                    && offset == at
                    && length != 0
                    && offset.checked_add(length) == Some(next.offset())
                    && next.offset() <= self.size =>
                {
                    self.pending = Some(Pending::Gap {
                        start: at,
                        end: next.offset(),
                        kind,
                    });
                    self.cursor = next;
                    return Ok(());
                }
                CapturedRunStep::Window { read, next }
                    if valid(next)
                        && read.offset == at
                        && !read.data.is_empty()
                        && read.data.len() <= CELL_BYTES
                        && read.data.capacity() <= CELL_BYTES
                        && read.inherited.capacity() <= MASK_BYTES
                        && read.kind == InodeKind::File
                        && read.size == self.size
                        && read.base_root == Some(self.reader.root())
                        && (read.inherited.is_empty()
                            || read.inherited.len() == read.data.len().div_ceil(8))
                        && at.checked_add(read.data.len() as u64) == Some(next.offset())
                        && next.offset() <= self.size =>
                {
                    self.pending = Some(Pending::Window(read));
                    self.cursor = next;
                    return Ok(());
                }
                _ => {
                    return Err(shared
                        .borrow_mut()
                        .content(ContentError::InvalidRecord("captured file run shape")))
                }
            }
        }
    }
    pub fn span<P: OverlayCapturedRuns + OverlayOperationRecords + ?Sized>(
        &mut self,
        provider: &P,
        shared: &Shared<'_, P>,
        at: u64,
        normalization: bool,
    ) -> ContentResult<Span> {
        self.ensure(provider, shared, at, normalization)?;
        match self.pending.as_ref().ok_or(ContentError::UnexpectedEof)? {
            Pending::Gap { end, kind, .. } => Ok(Span {
                start: at,
                end: if *kind == CapturedGap::Inherited && at < self.base_size {
                    (*end).min(self.base_size)
                } else {
                    *end
                },
                changed: *kind == CapturedGap::Zero || at >= self.base_size,
            }),
            Pending::Window(read) => {
                let index = (at - read.offset) as usize;
                let inherited = |at: usize| {
                    !read.inherited.is_empty() && read.inherited[at / 8] & (1 << (at % 8)) != 0
                };
                let first = inherited(index);
                let mut end = index + 1;
                while end < read.data.len() && inherited(end) == first {
                    end += 1;
                }
                Ok(Span {
                    start: at,
                    end: if first && at < self.base_size {
                        (read.offset + end as u64).min(self.base_size)
                    } else {
                        read.offset + end as u64
                    },
                    changed: !first || at >= self.base_size,
                })
            }
        }
    }
    pub fn read<P: OverlayCapturedRuns + OverlayOperationRecords + ?Sized>(
        &mut self,
        provider: &P,
        shared: &Shared<'_, P>,
        at: u64,
        limit: u64,
        output: &mut [u8],
    ) -> ContentResult<FileRun> {
        let span = self.span(provider, shared, at, false)?;
        if !span.changed {
            return Err(shared.borrow_mut().content(ContentError::InvalidRecord(
                "inherited captured replacement",
            )));
        }
        let length = (span.end - at).min(limit);
        match self.pending.as_ref().ok_or(ContentError::UnexpectedEof)? {
            Pending::Gap { .. } => Ok(FileRun::Zero(length)),
            Pending::Window(read) => {
                let index = (at - read.offset) as usize;
                if !read.inherited.is_empty() && read.inherited[index / 8] & (1 << (index % 8)) != 0
                {
                    // span() established authenticated base EOF. Bytes behind
                    // inherited bits never carry replacement authority.
                    return Ok(FileRun::Zero(length));
                }
                let count = usize::try_from(length)
                    .unwrap_or(usize::MAX)
                    .min(output.len());
                if count == 0 {
                    return Err(ContentError::InvalidRecord("captured replacement output"));
                }
                let start = (at - read.offset) as usize;
                let bytes = &read.data[start..start + count];
                if bytes.iter().all(|byte| *byte == 0) {
                    Ok(FileRun::Zero(count as u64))
                } else {
                    output[..count].copy_from_slice(bytes);
                    Ok(FileRun::Data(count))
                }
            }
        }
    }
}
