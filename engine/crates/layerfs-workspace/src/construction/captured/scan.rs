//! One preserved forward metadata cursor and one pending bounded window/gap.
use super::state::Shared;
use crate::{OverlayCapturedRuns, OverlayOperationRecords};
use layerfs_content::{ContentError, ContentResult, FileRun};
use layerfs_overlay::{
    CapturedGap, CapturedReader, CapturedRunCursor, CapturedRunStep, InodeKind, LocalRead,
    READ_REFERENCES, RUN_BYTES,
};

enum Pending {
    Gap {
        start: u64,
        end: u64,
        kind: CapturedGap,
    },
    Window(Box<LocalRead>),
    /// Local bytes reported without payload to a shapes-only pass.
    Local {
        start: u64,
        end: u64,
    },
    /// Committed bytes of `root` from `source`, named by one reference piece.
    Reference {
        start: u64,
        end: u64,
        root: [u8; 32],
        source: u64,
    },
}
pub(super) struct Scan {
    reader: CapturedReader,
    serial: u64,
    size: u64,
    base_size: u64,
    cursor: CapturedRunCursor,
    pending: Option<Pending>,
    referenced: bool,
}
/// Where the bytes of one span of the final file come from.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Origin {
    /// New bytes of the captured file: written cells or zeros.
    Local,
    /// Bytes of the file's own base at the same offset.
    Inherited,
    /// Committed bytes of content root `root` from `source`.
    Reference { root: [u8; 32], source: u64 },
}
#[derive(Clone, Copy)]
pub(super) struct Span {
    pub end: u64,
    pub origin: Origin,
}
/// What the construction source hands out for one span.
pub(super) enum Supply {
    Run(FileRun),
    /// Bytes to read from committed content: the file's own base when `root`
    /// is none.
    Committed {
        root: Option<[u8; 32]>,
        offset: u64,
        length: u64,
    },
}
impl Scan {
    pub fn new(
        reader: CapturedReader,
        serial: u64,
        size: u64,
        base_size: u64,
    ) -> ContentResult<Self> {
        Self::start(reader, serial, size, base_size, false)
    }
    /// A pass that learns only which bytes are local and never reads them.
    pub fn shapes(
        reader: CapturedReader,
        serial: u64,
        size: u64,
        base_size: u64,
    ) -> ContentResult<Self> {
        Self::start(reader, serial, size, base_size, true)
    }
    fn start(
        reader: CapturedReader,
        serial: u64,
        size: u64,
        base_size: u64,
        shapes: bool,
    ) -> ContentResult<Self> {
        let cursor = CapturedRunCursor::new(reader, serial, 0, size)
            .map_err(|_| ContentError::InvalidRecord("captured file cursor"))?;
        let cursor = if shapes { cursor.shapes_only() } else { cursor };
        Ok(Self {
            reader,
            serial,
            size,
            base_size,
            cursor,
            pending: None,
            referenced: false,
        })
    }
    /// Whether a unit of this pass reported that the file holds pieces.
    pub fn referenced(&self) -> bool {
        self.referenced
    }
    fn bounds(&self) -> Option<(u64, u64)> {
        match self.pending.as_ref()? {
            Pending::Gap { start, end, .. }
            | Pending::Local { start, end }
            | Pending::Reference { start, end, .. } => Some((*start, *end)),
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
            self.referenced |= reply.referenced;
            let valid = |next: CapturedRunCursor| {
                next.reader() == self.reader
                    && next.serial() == self.serial
                    && next.logical_size() == self.size
                    && next.is_shapes_only() == self.cursor.is_shapes_only()
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
                        && read.data.len() <= RUN_BYTES
                        && read.data.capacity() <= RUN_BYTES
                        && read.inherited.capacity() <= RUN_BYTES / 8
                        && read.kind == InodeKind::File
                        && read.size == self.size
                        && read.base_root == Some(self.reader.root())
                        && (read.inherited.is_empty()
                            || read.inherited.len() == read.data.len().div_ceil(8))
                        && read.references.len() <= READ_REFERENCES
                        && read.referenced.capacity() <= RUN_BYTES / 8
                        && read.referenced.len()
                            == if read.references.is_empty() {
                                0
                            } else {
                                read.data.len().div_ceil(8)
                            }
                        && at.checked_add(read.data.len() as u64) == Some(next.offset())
                        && next.offset() <= self.size =>
                {
                    self.pending = Some(Pending::Window(read));
                    self.cursor = next;
                    return Ok(());
                }
                CapturedRunStep::Local {
                    offset,
                    length,
                    next,
                } if valid(next)
                    && self.cursor.is_shapes_only()
                    && offset == at
                    && length != 0
                    && offset.checked_add(length) == Some(next.offset())
                    && next.offset() <= self.size =>
                {
                    self.pending = Some(Pending::Local {
                        start: at,
                        end: next.offset(),
                    });
                    self.cursor = next;
                    return Ok(());
                }
                CapturedRunStep::Reference {
                    offset,
                    length,
                    root,
                    source_offset,
                    next,
                } if valid(next)
                    && reply.referenced
                    && offset == at
                    && length != 0
                    && offset.checked_add(length) == Some(next.offset())
                    && source_offset.checked_add(length).is_some()
                    && next.offset() <= self.size =>
                {
                    self.pending = Some(Pending::Reference {
                        start: at,
                        end: next.offset(),
                        root,
                        source: source_offset,
                    });
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
            Pending::Gap { end, kind, .. } => {
                Ok(if *kind == CapturedGap::Inherited && at < self.base_size {
                    Span {
                        end: (*end).min(self.base_size),
                        origin: Origin::Inherited,
                    }
                } else {
                    Span {
                        end: *end,
                        origin: Origin::Local,
                    }
                })
            }
            Pending::Local { end, .. } => Ok(Span {
                end: *end,
                origin: Origin::Local,
            }),
            Pending::Reference {
                start,
                end,
                root,
                source,
            } => Ok(Span {
                end: *end,
                origin: Origin::Reference {
                    root: *root,
                    source: source + (at - start),
                },
            }),
            Pending::Window(read) => {
                let index = (at - read.offset) as usize;
                // A byte is inherited (2), supplied by a reference range (1)
                // or local (0).
                let bit = |mask: &[u8], at: usize| {
                    !mask.is_empty() && mask[at / 8] & (1 << (at % 8)) != 0
                };
                let class = |at: usize| {
                    if bit(&read.inherited, at) {
                        2
                    } else {
                        u8::from(bit(&read.referenced, at))
                    }
                };
                let first = class(index);
                // The one range that holds a referenced byte bounds its span.
                let range = if first == 1 {
                    Some(
                        read.references
                            .iter()
                            .find(|range| range.offset <= at && at - range.offset < range.length)
                            .ok_or(ContentError::InvalidRecord("captured reference range"))?,
                    )
                } else {
                    None
                };
                let limit = range.map_or(read.data.len(), |range| {
                    read.data
                        .len()
                        .min((range.offset + range.length - read.offset) as usize)
                });
                // No mask means every byte of the window is local.
                let mut end = if read.inherited.is_empty() && read.referenced.is_empty() {
                    read.data.len()
                } else {
                    index + 1
                };
                while end < limit && class(end) == first {
                    end += 1;
                }
                let end = read.offset + end as u64;
                Ok(match (first, range) {
                    (2, _) if at < self.base_size => Span {
                        end: end.min(self.base_size),
                        origin: Origin::Inherited,
                    },
                    (_, Some(range)) => Span {
                        end,
                        origin: Origin::Reference {
                            root: range.source,
                            source: range.source_offset + (at - range.offset),
                        },
                    },
                    _ => Span {
                        end,
                        origin: Origin::Local,
                    },
                })
            }
        }
    }
    /// The bytes of a span of new content at `at`, at most `limit` of them.
    /// A span the base or a reference supplies names its committed source;
    /// the caller reads it.
    pub fn read<P: OverlayCapturedRuns + OverlayOperationRecords + ?Sized>(
        &mut self,
        provider: &P,
        shared: &Shared<'_, P>,
        at: u64,
        limit: u64,
        output: &mut [u8],
        normalization: bool,
    ) -> ContentResult<Supply> {
        let span = self.span(provider, shared, at, normalization)?;
        let length = (span.end - at).min(limit);
        match span.origin {
            Origin::Local => {}
            // Only the script of a file with pieces replaces inherited bytes.
            Origin::Inherited if self.referenced => {
                return Ok(Supply::Committed {
                    root: None,
                    offset: at,
                    length,
                })
            }
            Origin::Inherited => {
                return Err(shared.borrow_mut().content(ContentError::InvalidRecord(
                    "inherited captured replacement",
                )))
            }
            Origin::Reference { root, source } => {
                return Ok(Supply::Committed {
                    root: Some(root),
                    offset: source,
                    length,
                })
            }
        }
        match self.pending.as_ref().ok_or(ContentError::UnexpectedEof)? {
            Pending::Gap { .. } => Ok(Supply::Run(FileRun::Zero(length))),
            // A shapes-only pass holds no bytes to hand out.
            Pending::Local { .. } | Pending::Reference { .. } => Err(shared
                .borrow_mut()
                .content(ContentError::InvalidRecord("captured shape read"))),
            Pending::Window(read) => {
                let index = (at - read.offset) as usize;
                if !read.inherited.is_empty() && read.inherited[index / 8] & (1 << (index % 8)) != 0
                {
                    // span() established authenticated base EOF. Bytes behind
                    // inherited bits never carry replacement authority.
                    return Ok(Supply::Run(FileRun::Zero(length)));
                }
                let count = usize::try_from(length)
                    .unwrap_or(usize::MAX)
                    .min(output.len());
                if count == 0 {
                    return Err(ContentError::InvalidRecord("captured replacement output"));
                }
                let bytes = &read.data[index..index + count];
                Ok(Supply::Run(if bytes.iter().all(|byte| *byte == 0) {
                    FileRun::Zero(count as u64)
                } else {
                    output[..count].copy_from_slice(bytes);
                    FileRun::Data(count)
                }))
            }
        }
    }
}
