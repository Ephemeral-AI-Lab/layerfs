//! Last-owner leased pages for immutable Counts and ZeroSeeds snapshots.
use super::{BaseFact, CountRecord, CountSeal, GraphMemoryLease, ZeroSeal};
use crate::{ContentError, ContentResult};
/// Exact CountSeal294/last9/count2/bytes4/EOF1.
pub const COUNT_PAGE_HEADER_BYTES: usize = 310;
/// Exact ZeroSeal579/last9/count2/bytes4/EOF1.
pub const ZERO_PAGE_HEADER_BYTES: usize = 595;
/// Bounded Count rows; its Vec cannot detach from its admission lease.
#[derive(Debug)]
pub struct CountPage {
    /// Exact selected immutable count epoch.
    pub seal: CountSeal,
    records: Vec<CountRecord>,
    /// Last emitted or prior terminal serial.
    pub last: Option<u64>,
    /// Exact epoch EOF.
    pub eof: bool,
    memory: GraphMemoryLease,
}
impl CountPage {
    /// Construct only after prospective actual Vec/owner admission.
    pub fn new(
        memory: GraphMemoryLease,
        seal: CountSeal,
        records: Vec<CountRecord>,
        last: Option<u64>,
        eof: bool,
    ) -> ContentResult<Self> {
        if records.capacity() > 128
            || std::mem::size_of::<Self>() + records.capacity() * std::mem::size_of::<CountRecord>()
                > memory.bytes()
            || (records.is_empty() && !eof)
        {
            return Err(ContentError::InvalidOrderingRecord("count page capacity"));
        }
        Ok(Self {
            seal,
            records,
            last,
            eof,
            memory,
        })
    }
    /// Borrow the actual rows while retaining their last-owner credit.
    pub fn records(&self) -> &[CountRecord] {
        &self.records
    }
    /// Actual current admitted result size.
    pub fn working_bytes(&self) -> usize {
        self.memory.bytes()
    }
}
/// Bounded carried base values for zero candidates, with inseparable Vec credit.
#[derive(Debug)]
pub struct ZeroPage {
    /// Exact closed candidate snapshot and original count epoch.
    pub seal: ZeroSeal,
    records: Vec<BaseFact>,
    /// Last emitted or prior terminal serial.
    pub last: Option<u64>,
    /// Exact candidate EOF.
    pub eof: bool,
    memory: GraphMemoryLease,
}
impl ZeroPage {
    /// Construct only after prospective actual Vec/owner admission.
    pub fn new(
        memory: GraphMemoryLease,
        seal: ZeroSeal,
        records: Vec<BaseFact>,
        last: Option<u64>,
        eof: bool,
    ) -> ContentResult<Self> {
        if records.capacity() > 128
            || std::mem::size_of::<Self>() + records.capacity() * std::mem::size_of::<BaseFact>()
                > memory.bytes()
            || (records.is_empty() && !eof)
        {
            return Err(ContentError::InvalidOrderingRecord("zero page capacity"));
        }
        Ok(Self {
            seal,
            records,
            last,
            eof,
            memory,
        })
    }
    /// Borrow candidates without detaching their Vec from its lease.
    pub fn records(&self) -> &[BaseFact] {
        &self.records
    }
    /// Actual current admitted result size.
    pub fn working_bytes(&self) -> usize {
        self.memory.bytes()
    }
}
