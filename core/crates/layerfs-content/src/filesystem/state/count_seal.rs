//! Independent complete ordered count/zero transcripts and immutable epochs.
use super::{BaseFact, CanonicalScope, CountEpoch, CountRecord};
use crate::{ContentError, ContentResult};
/// Fullscope228/epoch1/totals24/maximum9/digest32.
pub const COUNT_SEAL_BYTES: usize = 294;
/// Exact immutable Count table snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountSeal {
    /// Exact authenticated scope, owner and phase.
    pub scope: CanonicalScope,
    /// Closed effect or final epoch.
    pub epoch: CountEpoch,
    /// Complete row population, including declared yet untouched rows.
    pub records: u64,
    /// Actual touched population, without a resident membership set.
    pub touched: u64,
    /// Complete current ordered record bytes.
    pub bytes: u64,
    /// Final full serial, absent only for empty population.
    pub maximum: Option<u64>,
    /// Complete ordered record digest.
    pub digest: [u8; 32],
}
impl CountSeal {
    /// Exact private immutable snapshot framing.
    pub fn encode(&self) -> [u8; 294] {
        let mut b = [0; 294];
        b[..228].copy_from_slice(&self.scope.encode());
        b[228] = self.epoch.code();
        b[229..237].copy_from_slice(&self.records.to_be_bytes());
        b[237..245].copy_from_slice(&self.touched.to_be_bytes());
        b[245..253].copy_from_slice(&self.bytes.to_be_bytes());
        if let Some(last) = self.maximum {
            b[253] = 1;
            b[254..262].copy_from_slice(&last.to_be_bytes());
        }
        b[262..].copy_from_slice(&self.digest);
        b
    }
}
/// Exact independently folded count snapshot without retaining its population.
#[derive(Clone)]
pub struct CountLedger {
    scope: CanonicalScope,
    epoch: CountEpoch,
    digest: blake3::Hasher,
    records: u64,
    touched: u64,
    last: Option<u64>,
}
impl CountLedger {
    /// Begin the full selected epoch transcript.
    pub fn new(scope: CanonicalScope, epoch: CountEpoch) -> ContentResult<Self> {
        if scope.state().table().code() != 19 {
            return Err(ContentError::InvalidOrderingRecord("count ledger table"));
        }
        let mut digest = blake3::Hasher::new();
        digest.update(b"layerfs/reference-counts/v1\0");
        digest.update(&scope.encode());
        digest.update(&[epoch.code()]);
        Ok(Self {
            scope,
            epoch,
            digest,
            records: 0,
            touched: 0,
            last: None,
        })
    }
    /// Strict ordered bounded append, including untouched membership.
    pub fn append(&mut self, rows: &[CountRecord]) -> ContentResult<()> {
        if rows.len() > 128 {
            return Err(ContentError::InvalidOrderingRecord("count ledger window"));
        }
        for row in rows {
            if self.last.is_some_and(|s| s >= row.serial()) {
                return Err(ContentError::InvalidOrderingRecord("count cursor order"));
            }
            let key = self.scope.key(row.serial())?;
            let value = row.encode_value()?;
            CountRecord::decode(&self.scope, &key, &value)?;
            self.digest.update(&25u16.to_be_bytes());
            self.digest.update(&key);
            self.digest.update(&97u32.to_be_bytes());
            self.digest.update(&value);
            self.records = self
                .records
                .checked_add(1)
                .ok_or(ContentError::LengthOverflow)?;
            self.touched = self
                .touched
                .checked_add(u64::from(row.touched))
                .ok_or(ContentError::LengthOverflow)?;
            self.last = Some(row.serial());
        }
        Ok(())
    }
    /// Current full serial continuation.
    pub const fn last(&self) -> Option<u64> {
        self.last
    }
    /// Exact acknowledged count.
    pub const fn records(&self) -> u64 {
        self.records
    }
    /// Exact closed count snapshot.
    pub fn seal(&self) -> ContentResult<CountSeal> {
        let bytes = self
            .records
            .checked_mul(128)
            .ok_or(ContentError::LengthOverflow)?;
        let mut digest = self.digest.clone();
        digest.update(&self.records.to_be_bytes());
        digest.update(&self.touched.to_be_bytes());
        digest.update(&bytes.to_be_bytes());
        Ok(CountSeal {
            scope: self.scope.clone(),
            epoch: self.epoch,
            records: self.records,
            touched: self.touched,
            bytes,
            maximum: self.last,
            digest: *digest.finalize().as_bytes(),
        })
    }
}
/// Full exact Counts snapshot294 plus selected zero scope228/totals16/maximum9/digest32.
pub const ZERO_SEAL_BYTES: usize = 579;
/// Exact ordered zero candidates after a known Counts effect snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZeroSeal {
    /// Original immutable effect snapshot, never a later count epoch.
    pub counts: CountSeal,
    /// Exact table20 phase6 authenticated context.
    pub scope: CanonicalScope,
    /// Exact candidate population.
    pub records: u64,
    /// Complete ordered candidate bytes.
    pub bytes: u64,
    /// Last candidate serial or explicit empty.
    pub maximum: Option<u64>,
    /// Complete exact candidate transcript.
    pub digest: [u8; 32],
}
impl ZeroSeal {
    /// Exact full private seed snapshot framing.
    pub fn encode(&self) -> [u8; 579] {
        let mut b = [0; 579];
        b[..294].copy_from_slice(&self.counts.encode());
        b[294..522].copy_from_slice(&self.scope.encode());
        b[522..530].copy_from_slice(&self.records.to_be_bytes());
        b[530..538].copy_from_slice(&self.bytes.to_be_bytes());
        if let Some(last) = self.maximum {
            b[538] = 1;
            b[539..547].copy_from_slice(&last.to_be_bytes());
        }
        b[547..].copy_from_slice(&self.digest);
        b
    }
}
/// Advancing independent ordered seed fold, with known base values carried in rows.
#[derive(Clone)]
pub struct ZeroLedger {
    counts: CountSeal,
    scope: CanonicalScope,
    digest: blake3::Hasher,
    records: u64,
    last: Option<u64>,
}
impl ZeroLedger {
    /// Bind exactly the closed Effects epoch.
    pub fn new(counts: CountSeal) -> ContentResult<Self> {
        if counts.epoch != CountEpoch::Effects {
            return Err(ContentError::InvalidOrderingRecord("zero count epoch"));
        }
        let scope = counts.scope.zeros()?;
        let mut digest = blake3::Hasher::new();
        digest.update(b"layerfs/reference-zero/v1\0");
        digest.update(&counts.encode());
        digest.update(&scope.encode());
        Ok(Self {
            counts,
            scope,
            digest,
            records: 0,
            last: None,
        })
    }
    /// Check one bounded strict candidate window before retaining only its digest.
    pub fn append(&mut self, rows: &[BaseFact]) -> ContentResult<()> {
        if rows.len() > 128 {
            return Err(ContentError::InvalidOrderingRecord("zero seed window"));
        }
        for row in rows {
            if self.last.is_some_and(|s| s >= row.serial) {
                return Err(ContentError::InvalidOrderingRecord("zero seed order"));
            }
            let key = self.scope.key(row.serial)?;
            let value = row.encode_value()?;
            self.digest.update(&25u16.to_be_bytes());
            self.digest.update(&key);
            self.digest.update(&74u32.to_be_bytes());
            self.digest.update(&value);
            self.records = self
                .records
                .checked_add(1)
                .ok_or(ContentError::LengthOverflow)?;
            self.last = Some(row.serial);
        }
        Ok(())
    }
    /// Exact current candidate continuation.
    pub const fn last(&self) -> Option<u64> {
        self.last
    }
    /// Exact current candidate count.
    pub const fn records(&self) -> u64 {
        self.records
    }
    /// Full immutable seed seal.
    pub fn seal(&self) -> ContentResult<ZeroSeal> {
        let bytes = self
            .records
            .checked_mul(105)
            .ok_or(ContentError::LengthOverflow)?;
        let mut digest = self.digest.clone();
        digest.update(&self.records.to_be_bytes());
        digest.update(&bytes.to_be_bytes());
        Ok(ZeroSeal {
            counts: self.counts.clone(),
            scope: self.scope.clone(),
            records: self.records,
            bytes,
            maximum: self.last,
            digest: *digest.finalize().as_bytes(),
        })
    }
}
