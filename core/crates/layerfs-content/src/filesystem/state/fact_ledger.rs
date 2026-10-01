//! Ordered independent transcript/count/last-key folds for closed fact tables.
use super::{
    BaseFact, FactScope, FactSeal, ParentFact, ParentSeal, StateTable, BASE_FACT_BYTES,
    PARENT_ELIGIBILITY_BYTES,
};
use crate::{ContentError, ContentResult};
/// Incremental selected ordered transcript, never a complete retained population.
pub struct FactLedger {
    scope: FactScope,
    digest: blake3::Hasher,
    records: u64,
    bytes: u64,
    last: Option<u64>,
    bound: u64,
}
impl FactLedger {
    /// Selects one closed implemented table and full immutable source context.
    pub fn new(scope: FactScope) -> ContentResult<Self> {
        if !matches!(
            scope.state().table(),
            StateTable::BaseFacts | StateTable::ParentEligibility
        ) {
            return Err(ContentError::InvalidOrderingRecord("fact ledger table"));
        }
        let mut digest = blake3::Hasher::new();
        digest.update(b"layerfs/namespace-facts/v1\0");
        digest.update(&scope.encode());
        Ok(Self {
            scope,
            digest,
            records: 0,
            bytes: 0,
            last: None,
            bound: 0,
        })
    }
    fn row(&mut self, serial: u64, value: &[u8], width: u64) -> ContentResult<()> {
        if self.last.is_some_and(|last| last >= serial) {
            return Err(ContentError::InvalidOrderingRecord("fact cursor order"));
        }
        let key = self.scope.key(serial)?;
        self.digest.update(&25u16.to_be_bytes());
        self.digest.update(&key);
        self.digest.update(&(value.len() as u32).to_be_bytes());
        self.digest.update(value);
        self.records = self
            .records
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
        self.bytes = self
            .bytes
            .checked_add(width)
            .ok_or(ContentError::LengthOverflow)?;
        self.last = Some(serial);
        Ok(())
    }
    /// Acknowledges one bounded ordered BaseFact window.
    pub fn base(&mut self, records: &[BaseFact]) -> ContentResult<()> {
        if self.scope.state().table() != StateTable::BaseFacts || records.len() > 128 {
            return Err(ContentError::InvalidOrderingRecord("base fact window"));
        }
        for fact in records {
            self.row(fact.serial, &fact.encode_value()?, BASE_FACT_BYTES)?;
        }
        Ok(())
    }
    /// Acknowledges one bounded ordered ParentEligibility window.
    pub fn parents(&mut self, records: &[ParentFact]) -> ContentResult<()> {
        if self.scope.state().table() != StateTable::ParentEligibility || records.len() > 128 {
            return Err(ContentError::InvalidOrderingRecord("parent fact window"));
        }
        for fact in records {
            self.row(
                fact.serial,
                &[u8::from(fact.bound)],
                PARENT_ELIGIBILITY_BYTES,
            )?;
            self.bound += u64::from(fact.bound);
        }
        Ok(())
    }
    /// Exact acknowledged count.
    pub const fn records(&self) -> u64 {
        self.records
    }
    /// Exact acknowledged serial continuation.
    pub const fn last(&self) -> Option<u64> {
        self.last
    }
    /// Exact final immutable transcript.
    pub fn seal(&self) -> FactSeal {
        let mut digest = self.digest.clone();
        digest.update(&self.records.to_be_bytes());
        digest.update(&self.bytes.to_be_bytes());
        FactSeal {
            scope: self.scope.clone(),
            records: self.records,
            bytes: self.bytes,
            maximum: self.last,
            digest: *digest.finalize().as_bytes(),
        }
    }
    /// Exact final parent transcript plus independently folded bound population.
    pub fn parent_seal(&self) -> ContentResult<ParentSeal> {
        if self.scope.state().table() != StateTable::ParentEligibility {
            return Err(ContentError::InvalidOrderingRecord("parent seal table"));
        }
        Ok(ParentSeal {
            facts: self.seal(),
            bound: self.bound,
        })
    }
}
