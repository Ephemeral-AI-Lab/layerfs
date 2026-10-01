//! Exact immutable source-order membership acknowledgement and primary maximum.
use super::{SiteBirthSeal, SiteKey, SiteScope};
use crate::error::{ContentError, ContentResult};
/// birthseal138/max-present1/max-key25.
pub const SITE_MEMBERSHIP_BYTES: usize = 164;
/// Immutable approved population; mutable flags are not sealed by this type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SiteMembership {
    birth: SiteBirthSeal,
    maximum: Option<SiteKey>,
}
impl SiteMembership {
    /// Checks exact empty/nonempty maximum and its selected key association.
    pub fn new(birth: SiteBirthSeal, maximum: Option<SiteKey>) -> ContentResult<Self> {
        if (birth.records() == 0) != maximum.is_none() {
            return Err(ContentError::InvalidOrderingRecord(
                "site membership maximum",
            ));
        }
        if let Some(key) = maximum {
            SiteKey::decode(birth.scope(), key.as_bytes())?;
        }
        Ok(Self { birth, maximum })
    }
    /// Exact acknowledged birth transcript/count/source association.
    pub fn birth(&self) -> &SiteBirthSeal {
        &self.birth
    }
    /// Exact indexed primary maximum, absent only for an empty population.
    pub const fn maximum(&self) -> Option<SiteKey> {
        self.maximum
    }
    /// Complete immutable acknowledgement.
    pub fn encode(&self) -> [u8; SITE_MEMBERSHIP_BYTES] {
        let mut bytes = [0; SITE_MEMBERSHIP_BYTES];
        bytes[..138].copy_from_slice(&self.birth.encode());
        if let Some(key) = self.maximum {
            bytes[138] = 1;
            bytes[139..].copy_from_slice(key.as_bytes());
        }
        bytes
    }
    /// Requires actual issued scope/source, framing and zero absent maximum.
    pub fn decode(scope: &SiteScope, bytes: &[u8]) -> ContentResult<Self> {
        if bytes.len() != SITE_MEMBERSHIP_BYTES || bytes[138] > 1 {
            return Err(ContentError::InvalidOrderingRecord(
                "site membership framing",
            ));
        }
        let maximum = if bytes[138] == 1 {
            Some(SiteKey::decode(scope, &bytes[139..])?)
        } else {
            if bytes[139..] != [0; 25] {
                return Err(ContentError::InvalidOrderingRecord("site absent maximum"));
            }
            None
        };
        Self::new(SiteBirthSeal::decode(scope, &bytes[..138])?, maximum)
    }
}
