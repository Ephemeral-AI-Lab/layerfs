//! Generic attribute key grammar: structural checks and total ordering.
//!
//! Every attribute key is a `domain` plus opaque `key` bytes. The grammar is
//! structural and domain-generic: a nonempty UTF-8 domain of at most 64 bytes and
//! key bytes of at most 255, neither containing NUL. The reserved `portable`
//! domain is limited to its two typed keys; every other domain is accepted as
//! data. There is no operating-system whitelist, no platform interpretation and
//! no permission enforcement anywhere in this component.

use std::cmp::Ordering;

use crate::error::{ContentError, ContentResult};
use crate::filesystem::limits::{
    MAXIMUM_ATTRIBUTE_DOMAIN_BYTES, MAXIMUM_ATTRIBUTE_KEY_BYTES, PORTABLE_ATTRIBUTE_DOMAIN,
};

/// The two typed keys of the reserved portable domain.
pub const PORTABLE_KEYS: [&[u8]; 2] = [b"mode", b"mtime"];

/// One checked attribute key.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AttributeKey {
    domain: String,
    key: Vec<u8>,
}

impl AttributeKey {
    /// Checks and owns one key.
    pub fn new(domain: String, key: Vec<u8>) -> ContentResult<Self> {
        if domain.is_empty() {
            return Err(ContentError::InvalidRecord("attribute domain"));
        }
        if domain.len() > MAXIMUM_ATTRIBUTE_DOMAIN_BYTES {
            return Err(ContentError::ObjectLimitExceeded {
                limit: MAXIMUM_ATTRIBUTE_DOMAIN_BYTES,
                actual: domain.len(),
            });
        }
        if domain.as_bytes().contains(&0) {
            return Err(ContentError::InvalidRecord("attribute domain"));
        }
        if std::str::from_utf8(domain.as_bytes()).is_err() {
            return Err(ContentError::InvalidUtf8);
        }
        if key.len() > MAXIMUM_ATTRIBUTE_KEY_BYTES {
            return Err(ContentError::ObjectLimitExceeded {
                limit: MAXIMUM_ATTRIBUTE_KEY_BYTES,
                actual: key.len(),
            });
        }
        if key.contains(&0) {
            return Err(ContentError::InvalidRecord("attribute key"));
        }
        if domain == PORTABLE_ATTRIBUTE_DOMAIN && !PORTABLE_KEYS.contains(&key.as_slice()) {
            return Err(ContentError::InvalidRecord("portable attribute key"));
        }
        Ok(Self { domain, key })
    }

    /// The domain these bytes belong to.
    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// The opaque key bytes.
    pub fn key(&self) -> &[u8] {
        &self.key
    }

    /// True when this is one of the two typed portable keys.
    pub fn is_portable(&self) -> bool {
        self.domain == PORTABLE_ATTRIBUTE_DOMAIN
    }

    /// True when this key names the typed mode value.
    pub fn is_mode(&self) -> bool {
        self.is_portable() && self.key == b"mode"
    }

    /// True when this key names the typed mtime value.
    pub fn is_mtime(&self) -> bool {
        self.is_portable() && self.key == b"mtime"
    }
}

impl Ord for AttributeKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.domain
            .as_bytes()
            .cmp(other.domain.as_bytes())
            .then_with(|| self.key.cmp(&other.key))
    }
}

impl PartialOrd for AttributeKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Checks that a request list is strictly sorted and duplicate-free.
pub fn check_patch_order(patches: &[(AttributeKey, Option<Vec<u8>>)]) -> ContentResult<()> {
    if patches.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
        return Err(ContentError::NonCanonicalOrdering);
    }
    Ok(())
}
