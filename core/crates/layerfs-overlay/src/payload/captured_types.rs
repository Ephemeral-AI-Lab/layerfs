//! Fixed retained-input cursor; no backing rows or ownership acquisition.
use crate::{CapturedReader, LocalRead, OverlayError, OverlayResult, CELL_BYTES};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CellMetadata {
    pub offset: u64,
    pub length: u64,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Probe {
    pub after: u64,
    pub ready: bool,
    pub cell: Option<CellMetadata>,
}
/// Forward-only scan of one exact captured regular file. The caller supplies
/// its already checked logical size, including authenticated base facts when
/// there is no local inode row. This value acquires/releases no reader owner.
#[derive(Clone, Copy, Debug)]
pub struct CapturedRunCursor {
    pub(crate) reader: CapturedReader,
    pub(crate) serial: u64,
    pub(crate) at: u64,
    pub(crate) size: u64,
    pub(crate) probes: [Probe; 4],
    pub(crate) generations: [i64; 4],
    pub(crate) layer_count: Option<usize>,
}
impl CapturedRunCursor {
    /// Start a new stable read pass at a requested position. SQLite's existing
    /// signed offset format applies; there is no smaller total file/run limit.
    pub fn new(
        reader: CapturedReader,
        serial: u64,
        offset: u64,
        logical_size: u64,
    ) -> OverlayResult<Self> {
        crate::db::integer(serial)?;
        crate::db::integer(logical_size)?;
        if serial == 0 || offset > logical_size {
            return Err(OverlayError::Invalid("captured run input"));
        }
        let after = offset - offset % CELL_BYTES as u64;
        Ok(Self {
            reader,
            serial,
            at: offset,
            size: logical_size,
            probes: [Probe {
                after,
                ..Probe::default()
            }; 4],
            generations: [0; 4],
            layer_count: None,
        })
    }
    pub const fn reader(self) -> CapturedReader {
        self.reader
    }
    pub const fn serial(self) -> u64 {
        self.serial
    }
    pub const fn offset(self) -> u64 {
        self.at
    }
    pub const fn logical_size(self) -> u64 {
        self.size
    }
    /// Skip an already-owned input interval while preserving every forward
    /// metadata seek/lookahead. A backwards pass requires a new cursor.
    pub fn seek_forward(self, to: u64) -> OverlayResult<Self> {
        if to < self.at || to > self.size {
            return Err(OverlayError::Invalid("captured run seek"));
        }
        Ok(self.advance(to))
    }
    pub(crate) fn advance(mut self, to: u64) -> Self {
        self.at = to;
        let floor = to - to % CELL_BYTES as u64;
        for probe in &mut self.probes {
            if probe
                .cell
                .is_some_and(|cell| cell.offset + cell.length <= to)
            {
                probe.cell = None;
                probe.ready = false;
            }
            probe.after = probe.after.max(floor);
        }
        self
    }
}
/// A gap is inherited unless actual retained layer EOF/cutoff establishes zero.
/// Neither kind interprets missing physical cells as immutable zero evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapturedGap {
    Inherited,
    Zero,
}
/// One bounded SQL-owner unit. Continue is normal metadata cursor progress,
/// never a replay after failure. Window retains the existing decided-byte mask.
#[derive(Clone, Debug)]
pub enum CapturedRunStep {
    Continue(CapturedRunCursor),
    Gap {
        kind: CapturedGap,
        offset: u64,
        length: u64,
        next: CapturedRunCursor,
    },
    Window {
        read: Box<LocalRead>,
        next: CapturedRunCursor,
    },
    End,
}
/// Exact metadata row visits for this unit. SQL VM/value/BLOB work and byte
/// composition remain separately observable through their existing counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CapturedRunWork {
    pub metadata_rows: u64,
    pub stale_rows: u64,
}
#[derive(Clone, Debug)]
pub struct CapturedRunReply {
    pub step: CapturedRunStep,
    pub work: CapturedRunWork,
}
