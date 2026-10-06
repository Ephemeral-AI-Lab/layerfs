//! Length-prefixed records in caller-owned append-only ordering runs.
//!
//! A run is written once through a [`Writer`] and then read sequentially through
//! any number of [`Cursor`]s; it is never read while it is still being written.
//! [`Sorter`] orders an unbounded record stream with one fixed chunk and merges
//! of bounded fan-in, so resident state is fixed buffers plus a run-handle count
//! that grows with the logarithm of the stream, never with its length.
use crate::error::{content, ProjectError as Failure};
use layerfs_content::filesystem::references::{OrderingBacking, OrderingRun};
use std::{io, time::Instant};

/// One finished run the acquisition scratch owns until it is dropped.
pub(crate) type Run = Box<dyn OrderingRun>;
/// Bytes one writer or reader buffers.
const BUFFER_BYTES: usize = 64 * 1024;
/// Record bytes one resident sort chunk holds.
const SORT_BYTES: usize = 1 << 20;
/// Records one resident sort chunk holds.
const SORT_ROWS: usize = 4096;
/// Runs one merge reads, and runs one level holds before it is merged.
const FAN_IN: usize = 16;
/// Merged records between deadline observations.
const DEADLINE_STRIDE: u64 = 4096;

/// Appends records to one new run through a fixed buffer.
pub(crate) struct Writer {
    run: Run,
    buffer: Vec<u8>,
}
impl Writer {
    pub(crate) fn new(backing: &mut dyn OrderingBacking) -> Result<Self, Failure> {
        Ok(Self {
            run: backing.create_run().map_err(content)?,
            buffer: Vec::with_capacity(BUFFER_BYTES),
        })
    }
    pub(crate) fn push(&mut self, record: &[u8]) -> Result<(), Failure> {
        let len = u32::try_from(record.len()).map_err(|_| Failure::Capacity)?;
        if !self.buffer.is_empty() && self.buffer.len() + 4 + record.len() > BUFFER_BYTES {
            self.drain()?;
        }
        self.buffer.extend_from_slice(&len.to_be_bytes());
        self.buffer.extend_from_slice(record);
        Ok(())
    }
    fn drain(&mut self) -> Result<(), Failure> {
        self.run.append(&self.buffer).map_err(content)?;
        self.buffer.clear();
        Ok(())
    }
    /// Hands the complete run over for reading.
    pub(crate) fn finish(mut self) -> Result<Run, Failure> {
        self.drain()?;
        self.run.flush().map_err(content)?;
        Ok(self.run)
    }
}

/// Sequential record position in one finished run.
struct Reader {
    offset: u64,
    buffer: Vec<u8>,
    start: usize,
    current: Option<usize>,
}
impl Reader {
    fn new() -> Self {
        Self {
            offset: 0,
            buffer: Vec::new(),
            start: 0,
            current: None,
        }
    }
    /// Buffers the record at this position; false at the end of the run.
    fn load(&mut self, run: &dyn OrderingRun) -> Result<bool, Failure> {
        if self.current.is_some() {
            return Ok(true);
        }
        if !self.fill(run, 4)? {
            return Ok(false);
        }
        let mut len = [0; 4];
        len.copy_from_slice(&self.buffer[self.start..self.start + 4]);
        let len = u32::from_be_bytes(len) as usize;
        if !self.fill(run, 4 + len)? {
            return Err(truncated());
        }
        self.current = Some(len);
        Ok(true)
    }
    /// The loaded record.
    fn record(&self) -> Option<&[u8]> {
        let len = self.current?;
        Some(&self.buffer[self.start + 4..self.start + 4 + len])
    }
    /// Moves past the loaded record.
    fn advance(&mut self) {
        if let Some(len) = self.current.take() {
            self.start += 4 + len;
        }
    }
    /// Ensures `need` unread bytes are resident; false when none remain.
    fn fill(&mut self, run: &dyn OrderingRun, need: usize) -> Result<bool, Failure> {
        if self.buffer.len() - self.start >= need {
            return Ok(true);
        }
        if self.buffer.capacity() == 0 {
            self.buffer.reserve_exact(BUFFER_BYTES);
        }
        self.buffer.drain(..self.start);
        self.start = 0;
        let held = self.buffer.len();
        let remaining = run.len().saturating_sub(self.offset);
        let want = (need.max(BUFFER_BYTES) - held) as u64;
        let want = want.min(remaining) as usize;
        self.buffer.resize(held + want, 0);
        run.read_at(self.offset, &mut self.buffer[held..])
            .map_err(content)?;
        self.offset += want as u64;
        match self.buffer.len() {
            0 => Ok(false),
            len if len < need => Err(truncated()),
            _ => Ok(true),
        }
    }
}

fn truncated() -> Failure {
    Failure::Io(io::Error::new(
        io::ErrorKind::UnexpectedEof,
        "acquisition run ended inside a record",
    ))
}

/// Typed sequential stream over one finished run, one decoded row ahead.
pub(crate) struct Cursor<'r, T> {
    run: &'r dyn OrderingRun,
    reader: Reader,
    head: Option<T>,
    decode: fn(&[u8]) -> Result<T, Failure>,
}
impl<'r, T> Cursor<'r, T> {
    pub(crate) fn new(run: &'r Run, decode: fn(&[u8]) -> Result<T, Failure>) -> Self {
        Self {
            run: &**run,
            reader: Reader::new(),
            head: None,
            decode,
        }
    }
    /// Next row without consuming it.
    pub(crate) fn peek(&mut self) -> Result<Option<&T>, Failure> {
        if self.head.is_none() && self.reader.load(self.run)? {
            let row = self.reader.record().map(self.decode).transpose()?;
            self.reader.advance();
            self.head = row;
        }
        Ok(self.head.as_ref())
    }
    /// Consumes the row the last `peek` returned.
    pub(crate) fn take(&mut self) -> Option<T> {
        self.head.take()
    }
    /// Returns an unconsumed row to the front of the stream.
    pub(crate) fn restore(&mut self, row: T) {
        self.head = Some(row);
    }
    /// Consumes and returns the next row.
    pub(crate) fn next(&mut self) -> Result<Option<T>, Failure> {
        self.peek()?;
        Ok(self.take())
    }
    /// Read-buffer capacity; decoded rows' owned bytes are excluded.
    pub(crate) fn resident_bytes(&self) -> usize {
        self.reader.buffer.capacity()
    }
}

/// Orders records by a byte key that is unique within one stream.
pub(crate) struct Sorter {
    key: fn(&[u8]) -> &[u8],
    deadline: Instant,
    arena: Vec<u8>,
    rows: Vec<(usize, usize)>,
    levels: Vec<Vec<Run>>,
    resident: usize,
}
impl Sorter {
    pub(crate) fn new(key: fn(&[u8]) -> &[u8], deadline: Instant) -> Self {
        Self {
            key,
            deadline,
            arena: Vec::new(),
            rows: Vec::new(),
            levels: Vec::new(),
            resident: 0,
        }
    }
    pub(crate) fn push(
        &mut self,
        backing: &mut dyn OrderingBacking,
        record: &[u8],
    ) -> Result<(), Failure> {
        if self.arena.capacity() == 0 {
            self.arena.reserve_exact(SORT_BYTES);
            self.rows.reserve_exact(SORT_ROWS);
        }
        let full = self.rows.len() == SORT_ROWS || self.arena.len() + record.len() > SORT_BYTES;
        if full && !self.rows.is_empty() {
            self.spill(backing)?;
        }
        self.rows.push((self.arena.len(), record.len()));
        self.arena.extend_from_slice(record);
        Ok(())
    }
    /// Writes the resident chunk as one ordered run of the lowest level.
    fn spill(&mut self, backing: &mut dyn OrderingBacking) -> Result<(), Failure> {
        let (arena, key) = (&self.arena, self.key);
        self.rows
            .sort_unstable_by(|a, b| key(&arena[a.0..a.0 + a.1]).cmp(key(&arena[b.0..b.0 + b.1])));
        let mut writer = Writer::new(backing)?;
        for &(start, len) in &self.rows {
            writer.push(&self.arena[start..start + len])?;
        }
        self.note(BUFFER_BYTES);
        self.arena.clear();
        self.rows.clear();
        let (mut run, mut level) = (writer.finish()?, 0);
        loop {
            if self.levels.len() == level {
                self.levels.push(Vec::new());
            }
            self.levels[level].push(run);
            if self.levels[level].len() < FAN_IN {
                return Ok(());
            }
            let inputs = std::mem::take(&mut self.levels[level]);
            run = self.merge(backing, inputs)?;
            level += 1;
        }
    }
    /// Merges ordered runs into one; the inputs' bytes are returned on drop.
    fn merge(
        &mut self,
        backing: &mut dyn OrderingBacking,
        inputs: Vec<Run>,
    ) -> Result<Run, Failure> {
        let mut readers = Vec::with_capacity(inputs.len());
        for run in &inputs {
            let mut reader = Reader::new();
            reader.load(&**run)?;
            readers.push(reader);
        }
        let mut writer = Writer::new(backing)?;
        let mut merged = 0u64;
        loop {
            let mut least: Option<(usize, &[u8])> = None;
            for (position, reader) in readers.iter().enumerate() {
                let Some(record) = reader.record() else {
                    continue;
                };
                if least.is_none_or(|(_, other)| (self.key)(record) < (self.key)(other)) {
                    least = Some((position, record));
                }
            }
            let Some((position, record)) = least else {
                break;
            };
            writer.push(record)?;
            readers[position].advance();
            readers[position].load(&*inputs[position])?;
            merged += 1;
            if merged % DEADLINE_STRIDE == 0 && Instant::now() >= self.deadline {
                return Err(Failure::Deadline);
            }
        }
        let buffers: usize = readers.iter().map(|reader| reader.buffer.capacity()).sum();
        self.note(buffers + BUFFER_BYTES);
        writer.finish()
    }
    fn note(&mut self, buffers: usize) {
        let rows = self.rows.capacity() * std::mem::size_of::<(usize, usize)>();
        self.resident = self.resident.max(self.arena.capacity() + rows + buffers);
    }
    /// Completes the stream as one ordered run and leaves the sorter reusable.
    pub(crate) fn finish(&mut self, backing: &mut dyn OrderingBacking) -> Result<Run, Failure> {
        if !self.rows.is_empty() {
            self.spill(backing)?;
        }
        let mut carry: Option<Run> = None;
        for mut inputs in std::mem::take(&mut self.levels) {
            inputs.extend(carry.take());
            carry = match inputs.len() {
                0 | 1 => inputs.pop(),
                _ => Some(self.merge(backing, inputs)?),
            };
        }
        match carry {
            Some(run) => Ok(run),
            None => Writer::new(backing)?.finish(),
        }
    }
    /// Largest resident chunk plus merge buffers this sorter held.
    pub(crate) fn resident_bytes(&self) -> usize {
        self.resident
    }
}
