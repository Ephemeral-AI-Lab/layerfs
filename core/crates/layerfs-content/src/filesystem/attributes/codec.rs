//! Attribute-tree pages: grammar, exact sizing and the fill rule.
//!
//! One page is `LFS4MET`, role 9 for a leaf and 10 for a branch. A leaf row is
//! `domain + key + required-flag + value root`; a branch row is
//! `domain + key + child`. Sizes are computed exactly (`44 + sum(row widths)`),
//! which is the same number the encoder writes, so a partition decision never
//! needs a trial encoding.

use crate::error::{ContentError, ContentResult};
use crate::filesystem::attributes::keys::AttributeKey;
use crate::filesystem::limits::{MAXIMUM_PAGE_BYTES, MINIMUM_FILLED_PAGE_BYTES};
use crate::object::{codec, ObjectId};

/// Magic of an attribute page.
pub const ATTRIBUTE_MAGIC: [u8; 8] = *b"LFS4MET\0";
/// Grammar version.
pub const ATTRIBUTE_VERSION: u16 = 1;
/// Leaf role.
pub const ATTRIBUTE_LEAF_ROLE: u8 = 9;
/// Branch role.
pub const ATTRIBUTE_BRANCH_ROLE: u8 = 10;
/// Canonical bytes of an empty page: envelope plus node header.
pub const EMPTY_PAGE_BYTES: usize = 44;
/// Header width of one attribute page.
pub const NODE_HEADER_BYTES: usize = 31;
/// Fixed bytes of one leaf row beyond its domain and key.
pub const LEAF_ROW_OVERHEAD: usize = 37;
/// Fixed bytes of one branch row beyond its domain and key.
pub const BRANCH_ROW_OVERHEAD: usize = 36;

/// One attribute row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttributeEntry {
    /// Checked key.
    pub key: AttributeKey,
    /// Extent-only value root.
    pub value_root: ObjectId,
}

/// One decoded attribute page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AttributePage {
    /// Entries of a leaf page.
    Leaf {
        /// Encoded row bytes of this subtree.
        subtree_bytes: u64,
        /// Entries in key order.
        entries: Vec<AttributeEntry>,
    },
    /// Child summaries of a branch page.
    Branch {
        /// Page level; children are one level lower.
        level: u8,
        /// Entries in this subtree.
        subtree_count: u64,
        /// Encoded row bytes in this subtree.
        subtree_bytes: u64,
        /// Children in key order.
        children: Vec<(AttributeKey, ObjectId)>,
    },
}

impl AttributePage {
    /// Page level; a leaf is level zero.
    pub const fn level(&self) -> u8 {
        match self {
            Self::Leaf { .. } => 0,
            Self::Branch { level, .. } => *level,
        }
    }

    /// Exact canonical bytes this page needs.
    pub fn bytes(&self) -> ContentResult<usize> {
        match self {
            Self::Leaf { entries, .. } => {
                entries.iter().try_fold(EMPTY_PAGE_BYTES, |total, entry| {
                    total
                        .checked_add(row_bytes(&entry.key, LEAF_ROW_OVERHEAD))
                        .ok_or(ContentError::LengthOverflow)
                })
            }
            Self::Branch { children, .. } => {
                children
                    .iter()
                    .try_fold(EMPTY_PAGE_BYTES, |total, (key, _)| {
                        total
                            .checked_add(row_bytes(key, BRANCH_ROW_OVERHEAD))
                            .ok_or(ContentError::LengthOverflow)
                    })
            }
        }
    }

    /// True when this page fits the attribute page ceiling.
    pub fn fits(&self) -> ContentResult<bool> {
        Ok(self.bytes()? <= MAXIMUM_PAGE_BYTES)
    }

    /// True when this page satisfies the non-root 2/5 fill rule.
    pub fn filled(&self) -> ContentResult<bool> {
        Ok(self.bytes()? >= MINIMUM_FILLED_PAGE_BYTES)
    }

    /// Entries in this page's subtree.
    pub fn subtree_count(&self) -> u64 {
        match self {
            Self::Leaf { entries, .. } => entries.len() as u64,
            Self::Branch { subtree_count, .. } => *subtree_count,
        }
    }
}

/// Exact bytes of one row: overhead, domain length, key length and prefixes.
pub fn row_bytes(key: &AttributeKey, overhead: usize) -> usize {
    overhead + key.domain().len() + key.key().len()
}

/// Encodes one final attribute page.
pub fn encode_attribute_page(page: &AttributePage) -> ContentResult<Vec<u8>> {
    let (role, level, count, subtree_count, subtree_bytes) = match page {
        AttributePage::Leaf {
            entries,
            subtree_bytes,
        } => {
            let count = entries.len();
            let expected = entries.iter().try_fold(0_u64, |total, entry| {
                total
                    .checked_add(row_bytes(&entry.key, LEAF_ROW_OVERHEAD) as u64)
                    .ok_or(ContentError::LengthOverflow)
            })?;
            if expected != *subtree_bytes {
                return Err(ContentError::LengthMismatch {
                    expected,
                    actual: *subtree_bytes,
                });
            }
            (
                ATTRIBUTE_LEAF_ROLE,
                0_u8,
                count,
                count as u64,
                *subtree_bytes,
            )
        }
        AttributePage::Branch {
            level,
            subtree_count,
            subtree_bytes,
            children,
        } => (
            ATTRIBUTE_BRANCH_ROLE,
            *level,
            children.len(),
            *subtree_count,
            *subtree_bytes,
        ),
    };
    if level > crate::filesystem::limits::MAXIMUM_TREE_LEVEL {
        return Err(ContentError::MappingDepthExceeded);
    }
    if role == ATTRIBUTE_BRANCH_ROLE && (level == 0 || count < 2) {
        return Err(ContentError::NonCanonicalPagePartition);
    }
    let mut value = header(role, level, count, subtree_count, subtree_bytes)?;
    match page {
        AttributePage::Leaf { entries, .. } => {
            check_order(entries.iter().map(|entry| &entry.key))?;
            for entry in entries {
                put_key(&mut value, &entry.key)?;
                value.push(1);
                value.extend_from_slice(entry.value_root.as_bytes());
            }
        }
        AttributePage::Branch { children, .. } => {
            check_order(children.iter().map(|(key, _)| key))?;
            for (key, child) in children {
                put_key(&mut value, key)?;
                value.extend_from_slice(child.as_bytes());
            }
        }
    }
    finish(value)
}

/// Decodes and validates one authenticated attribute page.
pub fn decode_attribute_page(canonical: &[u8]) -> ContentResult<AttributePage> {
    if canonical.len() > MAXIMUM_PAGE_BYTES {
        return Err(ContentError::ObjectLimitExceeded {
            limit: MAXIMUM_PAGE_BYTES,
            actual: canonical.len(),
        });
    }
    let value = codec::decode_bytes_object(canonical)?;
    if value.len() < NODE_HEADER_BYTES {
        return Err(ContentError::UnexpectedEof);
    }
    if !value.starts_with(&ATTRIBUTE_MAGIC) {
        return Err(ContentError::UnsupportedFraming);
    }
    let version = u16::from_be_bytes([value[8], value[9]]);
    if version != ATTRIBUTE_VERSION {
        return Err(ContentError::UnsupportedMappingVersion { version });
    }
    if value[12] != 0 {
        return Err(ContentError::InvalidRecord("attribute flags"));
    }
    let role = value[10];
    let level = value[11];
    if level > crate::filesystem::limits::MAXIMUM_TREE_LEVEL {
        return Err(ContentError::MappingDepthExceeded);
    }
    let count = usize::from(u16::from_be_bytes([value[13], value[14]]));
    let subtree_count = u64::from_be_bytes(
        value[15..23]
            .try_into()
            .map_err(|_| ContentError::UnexpectedEof)?,
    );
    let subtree_bytes = u64::from_be_bytes(
        value[23..31]
            .try_into()
            .map_err(|_| ContentError::UnexpectedEof)?,
    );
    if count > (value.len() - NODE_HEADER_BYTES) / (LEAF_ROW_OVERHEAD - 1) {
        return Err(ContentError::UnexpectedEof);
    }
    let mut cursor = NODE_HEADER_BYTES;
    let page = match (role, level) {
        (ATTRIBUTE_LEAF_ROLE, 0) if subtree_count == count as u64 => {
            let mut entries = Vec::with_capacity(count);
            let mut previous: Option<AttributeKey> = None;
            for _ in 0..count {
                let key = take_key(value, &mut cursor)?;
                if previous.as_ref().is_some_and(|prior| prior >= &key) {
                    return Err(ContentError::NonCanonicalOrdering);
                }
                previous = Some(key.clone());
                let flag = take(value, &mut cursor, 1)?;
                if flag != [1] {
                    return Err(ContentError::InvalidRecord("attribute required flag"));
                }
                let value_root = ObjectId::from_bytes(take(value, &mut cursor, 32)?)?;
                entries.push(AttributeEntry { key, value_root });
            }
            let expected = entries.iter().try_fold(0_u64, |total, entry| {
                total
                    .checked_add(row_bytes(&entry.key, LEAF_ROW_OVERHEAD) as u64)
                    .ok_or(ContentError::LengthOverflow)
            })?;
            if expected != subtree_bytes {
                return Err(ContentError::LengthMismatch {
                    expected,
                    actual: subtree_bytes,
                });
            }
            AttributePage::Leaf {
                subtree_bytes,
                entries,
            }
        }
        (ATTRIBUTE_BRANCH_ROLE, level) if level > 0 && count >= 2 => {
            let mut children = Vec::with_capacity(count);
            let mut previous: Option<AttributeKey> = None;
            for _ in 0..count {
                let key = take_key(value, &mut cursor)?;
                if previous.as_ref().is_some_and(|prior| prior >= &key) {
                    return Err(ContentError::NonCanonicalOrdering);
                }
                previous = Some(key.clone());
                children.push((key, ObjectId::from_bytes(take(value, &mut cursor, 32)?)?));
            }
            AttributePage::Branch {
                level,
                subtree_count,
                subtree_bytes,
                children,
            }
        }
        _ => return Err(ContentError::InvalidRecord("attribute role/level")),
    };
    if cursor != value.len() {
        return Err(ContentError::TrailingBytes);
    }
    Ok(page)
}

fn header(
    role: u8,
    level: u8,
    count: usize,
    subtree_count: u64,
    subtree_bytes: u64,
) -> ContentResult<Vec<u8>> {
    let count = u16::try_from(count).map_err(|_| ContentError::LengthOverflow)?;
    let mut value = Vec::with_capacity(NODE_HEADER_BYTES);
    value.extend_from_slice(&ATTRIBUTE_MAGIC);
    value.extend_from_slice(&ATTRIBUTE_VERSION.to_be_bytes());
    value.extend_from_slice(&[role, level, 0]);
    value.extend_from_slice(&count.to_be_bytes());
    value.extend_from_slice(&subtree_count.to_be_bytes());
    value.extend_from_slice(&subtree_bytes.to_be_bytes());
    Ok(value)
}

fn finish(value: Vec<u8>) -> ContentResult<Vec<u8>> {
    let canonical = codec::encode_bytes_object(&value)?;
    if canonical.len() > MAXIMUM_PAGE_BYTES {
        return Err(ContentError::ObjectLimitExceeded {
            limit: MAXIMUM_PAGE_BYTES,
            actual: canonical.len(),
        });
    }
    Ok(canonical)
}

fn put_key(output: &mut Vec<u8>, key: &AttributeKey) -> ContentResult<()> {
    let domain = u16::try_from(key.domain().len()).map_err(|_| ContentError::LengthOverflow)?;
    let bytes = u16::try_from(key.key().len()).map_err(|_| ContentError::LengthOverflow)?;
    output.extend_from_slice(&domain.to_be_bytes());
    output.extend_from_slice(key.domain().as_bytes());
    output.extend_from_slice(&bytes.to_be_bytes());
    output.extend_from_slice(key.key());
    Ok(())
}

fn take_key(bytes: &[u8], cursor: &mut usize) -> ContentResult<AttributeKey> {
    let domain_length = usize::from(u16::from_be_bytes(
        take(bytes, cursor, 2)?
            .try_into()
            .map_err(|_| ContentError::UnexpectedEof)?,
    ));
    if domain_length > crate::filesystem::limits::MAXIMUM_ATTRIBUTE_DOMAIN_BYTES {
        return Err(ContentError::ObjectLimitExceeded {
            limit: crate::filesystem::limits::MAXIMUM_ATTRIBUTE_DOMAIN_BYTES,
            actual: domain_length,
        });
    }
    let domain = std::str::from_utf8(take(bytes, cursor, domain_length)?)
        .map_err(|_| ContentError::InvalidUtf8)?
        .to_owned();
    let key_length = usize::from(u16::from_be_bytes(
        take(bytes, cursor, 2)?
            .try_into()
            .map_err(|_| ContentError::UnexpectedEof)?,
    ));
    if key_length > crate::filesystem::limits::MAXIMUM_ATTRIBUTE_KEY_BYTES {
        return Err(ContentError::ObjectLimitExceeded {
            limit: crate::filesystem::limits::MAXIMUM_ATTRIBUTE_KEY_BYTES,
            actual: key_length,
        });
    }
    AttributeKey::new(domain, take(bytes, cursor, key_length)?.to_vec())
}

fn take<'a>(bytes: &'a [u8], cursor: &mut usize, length: usize) -> ContentResult<&'a [u8]> {
    let end = cursor
        .checked_add(length)
        .ok_or(ContentError::LengthOverflow)?;
    if end > bytes.len() {
        return Err(ContentError::UnexpectedEof);
    }
    let value = &bytes[*cursor..end];
    *cursor = end;
    Ok(value)
}

fn check_order<'a>(keys: impl Iterator<Item = &'a AttributeKey>) -> ContentResult<()> {
    let mut previous: Option<&AttributeKey> = None;
    for key in keys {
        if previous.is_some_and(|prior| prior >= key) {
            return Err(ContentError::NonCanonicalOrdering);
        }
        previous = Some(key);
    }
    Ok(())
}
