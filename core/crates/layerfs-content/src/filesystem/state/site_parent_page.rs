//! Bounded immutable existing-site projection for one positive parent.
use super::{
    SiteMembership, SiteRecord, SITE_RECORD_BYTES, STATE_MAX_PAGE_BYTES, STATE_MAX_PAGE_RECORDS,
};
use crate::error::{ContentError, ContentResult};
/// membership164/parent8/last-present1/last4/max-present1/max4/count2/bytes4/EOF1.
pub const SITE_PARENT_PAGE_HEADER_BYTES: usize = 189;
/// Header-inclusive parent projection limits, distinct from primary page limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SiteParentPageLimit {
    records: usize,
    bytes: usize,
}
impl Default for SiteParentPageLimit {
    fn default() -> Self {
        Self {
            records: STATE_MAX_PAGE_RECORDS,
            bytes: STATE_MAX_PAGE_BYTES,
        }
    }
}
impl SiteParentPageLimit {
    /// Two simultaneous bounded dimensions.
    pub fn new(records: usize, bytes: usize) -> ContentResult<Self> {
        if records == 0
            || records > STATE_MAX_PAGE_RECORDS
            || !(SITE_PARENT_PAGE_HEADER_BYTES..=STATE_MAX_PAGE_BYTES).contains(&bytes)
        {
            return Err(ContentError::InvalidOrderingRecord(
                "site parent page limit",
            ));
        }
        Ok(Self { records, bytes })
    }
    /// Maximum returned/native-capacity records.
    pub const fn records(self) -> usize {
        self.records
    }
    /// Maximum complete encoded bytes including header.
    pub const fn bytes(self) -> usize {
        self.bytes
    }
    /// Rows fitting both dimensions.
    pub fn fitting_records(self) -> usize {
        self.records
            .min((self.bytes - SITE_PARENT_PAGE_HEADER_BYTES) / SITE_RECORD_BYTES)
    }
    /// Checks exact encoding before native effects/output.
    pub fn check_records(self, records: usize) -> ContentResult<()> {
        if records > self.records {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_sites.parent_records",
                limit: self.records as u64,
                actual: records as u64,
            });
        }
        let bytes = records
            .checked_mul(SITE_RECORD_BYTES)
            .and_then(|n| n.checked_add(SITE_PARENT_PAGE_HEADER_BYTES))
            .ok_or(ContentError::LengthOverflow)?;
        if bytes > self.bytes {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_sites.parent_bytes",
                limit: self.bytes as u64,
                actual: bytes as u64,
            });
        }
        Ok(())
    }
}
/// Immutable existing birth records, in actual binding ordinal order.
#[derive(Debug)]
pub struct SiteParentPage {
    members: SiteMembership,
    parent: u64,
    last: Option<u32>,
    maximum: Option<u32>,
    records: Vec<SiteRecord>,
    eof: bool,
}
impl SiteParentPage {
    /// First selected parent projection page.
    pub fn new(
        members: SiteMembership,
        parent: u64,
        maximum: Option<u32>,
        records: Vec<SiteRecord>,
        eof: bool,
    ) -> ContentResult<Self> {
        Self::after(members, parent, None, maximum, records, eof)
    }
    /// Actual prior/max ordinals include Some(0); no absent sentinel aliases zero.
    pub fn after(
        members: SiteMembership,
        parent: u64,
        after: Option<u32>,
        maximum: Option<u32>,
        records: Vec<SiteRecord>,
        eof: bool,
    ) -> ContentResult<Self> {
        if parent == 0 || parent > i64::MAX as u64 {
            return Err(ContentError::InvalidOrderingRecord("site parent"));
        }
        SiteParentPageLimit::default().check_records(records.len())?;
        if records.capacity() > STATE_MAX_PAGE_RECORDS {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_sites.parent_capacity",
                limit: STATE_MAX_PAGE_RECORDS as u64,
                actual: records.capacity() as u64,
            });
        }
        if after.is_some_and(|prior| maximum.is_none_or(|max| prior > max)) {
            return Err(ContentError::InvalidOrderingRecord(
                "site parent continuation",
            ));
        }
        let mut last = after;
        for record in &records {
            SiteRecord::decode(members.birth().scope(), &record.encode())?;
            let ordinal = record.point().binding_ordinal();
            if record.flags() != 1
                || record.point().parent() != parent
                || last.is_some_and(|prior| prior >= ordinal)
                || maximum.is_none_or(|max| ordinal > max)
            {
                return Err(ContentError::InvalidOrderingRecord(
                    "site parent page order",
                ));
            }
            last = Some(ordinal);
        }
        if records.is_empty() && !eof || eof != (last == maximum) {
            return Err(ContentError::InvalidOrderingRecord("site parent EOF"));
        }
        Ok(Self {
            members,
            parent,
            last,
            maximum,
            records,
            eof,
        })
    }
    /// Exact approved immutable population.
    pub fn members(&self) -> &SiteMembership {
        &self.members
    }
    /// Positive selected parent.
    pub const fn parent(&self) -> u64 {
        self.parent
    }
    /// Acknowledged actual indexed maximum, absent for empty projection only.
    pub const fn maximum(&self) -> Option<u32> {
        self.maximum
    }
    /// Immutable birth records, never current mutable facts.
    pub fn records(&self) -> &[SiteRecord] {
        &self.records
    }
    /// Transfers this one page's record owners.
    pub fn into_records(self) -> Vec<SiteRecord> {
        self.records
    }
    /// Actual last emitted or prior ordinal on empty terminal page.
    pub const fn last(&self) -> Option<u32> {
        self.last
    }
    /// Exact maximum assertion; independently checked by the cursor/provider.
    pub const fn eof(&self) -> bool {
        self.eof
    }
    /// Complete header-inclusive byte count.
    pub fn encoded_len(&self) -> usize {
        SITE_PARENT_PAGE_HEADER_BYTES + self.records.len() * SITE_RECORD_BYTES
    }
    /// Includes actual native Vec capacity, independently of byte framing.
    pub fn check_limit(&self, limit: SiteParentPageLimit) -> ContentResult<()> {
        limit.check_records(self.records.len())?;
        if self.records.capacity() > limit.records() {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_sites.parent_capacity",
                limit: limit.records() as u64,
                actual: self.records.capacity() as u64,
            });
        }
        Ok(())
    }
    /// membership164/parent8/presence+last/presence+MAX/count/bytes/EOF.
    pub fn encode_header(&self) -> [u8; SITE_PARENT_PAGE_HEADER_BYTES] {
        let mut bytes = [0; SITE_PARENT_PAGE_HEADER_BYTES];
        bytes[..164].copy_from_slice(&self.members.encode());
        bytes[164..172].copy_from_slice(&self.parent.to_be_bytes());
        if let Some(last) = self.last {
            bytes[172] = 1;
            bytes[173..177].copy_from_slice(&last.to_be_bytes());
        }
        if let Some(max) = self.maximum {
            bytes[177] = 1;
            bytes[178..182].copy_from_slice(&max.to_be_bytes());
        }
        bytes[182..184].copy_from_slice(&(self.records.len() as u16).to_be_bytes());
        bytes[184..188].copy_from_slice(&((self.records.len() * 60) as u32).to_be_bytes());
        bytes[188] = u8::from(self.eof);
        bytes
    }
}
