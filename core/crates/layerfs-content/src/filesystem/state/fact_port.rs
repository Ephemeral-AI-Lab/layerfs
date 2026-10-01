//! Narrow exact immutable base facts and selected parent-eligibility phases.
use super::{BaseFact, FactCapacity, FactScope, GraphMemory, GraphMemoryLease, ParentFact};
use crate::{ContentError, ContentResult};
/// Full scope228/digest32/totals16/maximum9/last9/count2/bytes4/EOF1.
pub const FACT_PAGE_HEADER_BYTES: usize = 301;
/// Known immutable final selected table transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactSeal {
    /// Exact issued selected table/base/epoch.
    pub scope: FactScope,
    /// Exact sealed row population.
    pub records: u64,
    /// Complete key/value/framing bytes.
    pub bytes: u64,
    /// Last exact serial, absent only for an empty table.
    pub maximum: Option<u64>,
    /// Full ordered record transcript digest.
    pub digest: [u8; 32],
}
/// Known immutable parent membership and exact bound count.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParentSeal {
    /// Exact selected table transcript.
    pub facts: FactSeal,
    /// Rows with an incoming final binding.
    pub bound: u64,
}
impl ParentSeal {
    /// Exact excluded declared parent count.
    pub fn excluded(&self) -> ContentResult<u64> {
        self.facts
            .records
            .checked_sub(self.bound)
            .ok_or(ContentError::InvalidOrderingRecord(
                "parent sealed bound count",
            ))
    }
}
/// One leased page; result consumers retain its capacity until its Vec is freed.
#[derive(Debug)]
pub struct FactPage<T> {
    /// Exact immutable table seal.
    pub seal: FactSeal,
    records: Vec<T>,
    /// Last emitted or prior serial on terminal empty page.
    pub last: Option<u64>,
    /// Exact sealed population EOF.
    pub eof: bool,
    memory: GraphMemoryLease,
}
impl<T> FactPage<T> {
    /// Construct only after reserving actual page/vector capacity before allocation.
    pub fn new(
        memory: GraphMemoryLease,
        seal: FactSeal,
        records: Vec<T>,
        last: Option<u64>,
        eof: bool,
    ) -> ContentResult<Self> {
        let bytes = std::mem::size_of::<Self>() + records.capacity() * std::mem::size_of::<T>();
        if records.capacity() > 128 || bytes > memory.bytes() || (records.is_empty() && !eof) {
            return Err(ContentError::InvalidOrderingRecord("fact page capacity"));
        }
        Ok(Self {
            seal,
            records,
            last,
            eof,
            memory,
        })
    }
    /// Borrow records without detaching the Vec from its last-owner lease.
    pub fn records(&self) -> &[T] {
        &self.records
    }
    /// Borrow shared admission while this result retains its own credit.
    pub fn memory(&self) -> &GraphMemory {
        self.memory.memory()
    }
}
/// Base facts share the selected native owner; unknown never means absence.
pub trait FactState {
    /// Previously admitted independent record/aggregate class.
    fn fact_capacity(&self, scope: &FactScope) -> ContentResult<FactCapacity>;
    /// Shared scoped owner/window/consumer working admission.
    fn fact_memory(&self, scope: &FactScope) -> ContentResult<GraphMemory>;
    /// Bind the actual authenticated table once before first fact SQL.
    fn fact_bind(&mut self, scope: &FactScope) -> ContentResult<()>;
    /// No row is unknown; Some with absent payload is known absence.
    fn fact_get(&mut self, scope: &FactScope, serial: u64) -> ContentResult<Option<BaseFact>>;
    /// One checked immutable <=128/64KiB insertion window.
    fn fact_insert(&mut self, scope: &FactScope, records: &[BaseFact]) -> ContentResult<()>;
    /// Close further insertion and seal exact rows/bytes/key/digest.
    fn fact_seal(&mut self, scope: &FactScope) -> ContentResult<FactSeal>;
    /// Advancing header-inclusive bounded page with leased actual capacity.
    fn fact_page(
        &mut self,
        seal: &FactSeal,
        after: Option<u64>,
        records: usize,
        bytes: usize,
    ) -> ContentResult<FactPage<BaseFact>>;
    /// Bounded exact retirement after all readers end.
    fn fact_retire(&mut self, seal: &FactSeal) -> ContentResult<()>;
    /// Selected metadata-only terminalization, no cleanup/refund.
    fn fact_abandon(&mut self, scope: &FactScope) -> ContentResult<()>;
}
/// Only new nonroot directory headers participate; point absence is not excluded.
pub trait ParentEligibilityState {
    /// Bind the same immutable selected subject before declaration SQL.
    fn parent_bind(&mut self, scope: &FactScope) -> ContentResult<()>;
    /// Strictly ordered declared-new nonroot directory serials.
    fn parent_insert(&mut self, scope: &FactScope, serials: &[u64]) -> ContentResult<()>;
    /// Acknowledge exact declaration EOF before bound marking.
    fn parent_close_declarations(&mut self, scope: &FactScope) -> ContentResult<()>;
    /// Monotone incoming-binding observations; a missing declared key stays absent.
    fn parent_mark_bound(&mut self, scope: &FactScope, children: &[u64]) -> ContentResult<()>;
    /// Seal ordered rows, exact bound/excluded totals and terminal key.
    fn parent_seal(&mut self, scope: &FactScope) -> ContentResult<ParentSeal>;
    /// Exact selected immutable parent row; no row is not excluded.
    fn parent_get(&mut self, seal: &ParentSeal, serial: u64) -> ContentResult<Option<ParentFact>>;
    /// Advancing header-inclusive bounded page with leased actual capacity.
    fn parent_page(
        &mut self,
        seal: &ParentSeal,
        after: Option<u64>,
        records: usize,
        bytes: usize,
    ) -> ContentResult<FactPage<ParentFact>>;
    /// Bounded exact retirement after canonical consumers end.
    fn parent_retire(&mut self, seal: &ParentSeal) -> ContentResult<()>;
    /// Selected metadata-only terminalization, no cleanup/refund.
    fn parent_abandon(&mut self, scope: &FactScope) -> ContentResult<()>;
}
