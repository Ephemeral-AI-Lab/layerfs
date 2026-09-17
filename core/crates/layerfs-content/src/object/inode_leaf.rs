//! Checked compact inode-value grammar: the physical-pooling input format.
//!
//! This module owns one canonical grammar and nothing else: a 73-byte inode value
//! and a leaf page that carries `(serial, value)` rows. It implements no directory
//! traversal, no inode allocation, no hardlink update and no filesystem-tree
//! construction; those remain later stages. What it does provide is the exact
//! checked byte layout C2 pools, including the derived physical layout of a pooled
//! leaf, so both components agree on one grammar instead of re-deriving it.
//!
//! The node header's `subtree_bytes` field carries the *encoded row width*, not
//! the value width: one leaf row is an 8-byte serial plus the 73-byte value, so a
//! leaf of `n` rows records `n * 81`. The pinned reference encoder writes exactly
//! that (`encode_inode`), and the sealed reference fixture for a two-row leaf
//! (`tests/fixtures/filesystem/codec-inode-leaf.bin`) confirms it. Writing
//! `n * 73` here - as an earlier revision of this module did - produced different
//! canonical bytes and therefore different object identities for the same logical
//! inode page. The field is validated directly on decode; there is no canonical
//! re-encoding step.

use crate::error::{ContentError, ContentResult};
use crate::object::ObjectId;

/// Width of one inode value.
pub const INODE_VALUE_BYTES: usize = 73;
/// Magic of an inode leaf page value.
pub const INODE_LEAF_MAGIC: [u8; 8] = *b"LFS6INT\0";
/// Version of the compact inode grammar.
pub const INODE_LEAF_VERSION: u16 = 1;
/// Width of the shared node header.
pub const NODE_HEADER_BYTES: usize = 31;
/// Width of one leaf row: an 8-byte serial followed by the value.
pub const LEAF_ROW_BYTES: usize = 81;
/// Largest row count of one inode leaf page.
pub const MAXIMUM_LEAF_ROWS: usize = 100;
/// Smallest row count of a non-root inode leaf page.
pub const MINIMUM_LEAF_ROWS: usize = 50;
/// Largest canonical bytes of one tree page, including the object envelope.
pub const MAXIMUM_NODE_OBJECT_BYTES: usize = 8_192;
/// Canonical bytes before the first row: envelope plus node header.
pub const POOLED_PREFIX_BYTES: usize = 44;
/// Physical bytes of one pooled row: the serial and a 4-byte ordinal.
pub const POOLED_ROW_BYTES: usize = 12;
/// Magic of one pooled value object's value.
pub const POOLED_VALUE_MAGIC: [u8; 8] = *b"LFSIVL1\0";
/// Value width of one pooled value object.
pub const POOLED_VALUE_BYTES: usize = 81;
/// Canonical width of one pooled value object.
pub const POOLED_VALUE_CANONICAL_BYTES: usize = 94;

/// Inode kind, exactly as the compact grammar records it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InodeKind {
    /// Regular file.
    RegularFile,
    /// Directory.
    Directory,
    /// Symbolic link.
    Symlink,
}

impl InodeKind {
    /// Persisted code.
    pub const fn code(self) -> u8 {
        match self {
            Self::RegularFile => 1,
            Self::Directory => 2,
            Self::Symlink => 3,
        }
    }

    /// Rebuilds a kind from its persisted code.
    pub const fn from_code(code: u8) -> ContentResult<Self> {
        match code {
            1 => Ok(Self::RegularFile),
            2 => Ok(Self::Directory),
            3 => Ok(Self::Symlink),
            _ => Err(ContentError::InvalidRecord("inode kind")),
        }
    }
}

/// One decoded inode value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InodeValue {
    /// File kind.
    pub kind: InodeKind,
    /// Namespace reference count.
    pub namespace_ref_count: u64,
    /// Logical content root of the file.
    pub content_root: ObjectId,
    /// Logical metadata root of the file.
    pub metadata_root: ObjectId,
}

impl InodeValue {
    /// Checks the root/non-root invariants of the compact profile.
    ///
    /// The page itself does not say whether it is the tree root, so this check
    /// belongs to the caller that knows the position - it is not applied by the
    /// page grammar.
    pub fn validate(self, is_root: bool) -> ContentResult<()> {
        let valid = if is_root {
            self.kind == InodeKind::Directory && self.namespace_ref_count == 0
        } else {
            self.namespace_ref_count >= 1
                && (self.kind == InodeKind::RegularFile || self.namespace_ref_count == 1)
        };
        if valid {
            Ok(())
        } else {
            Err(ContentError::InvalidRecord("inode value invariant"))
        }
    }
}

/// Encodes one inode value into its fixed 73-byte form.
pub fn encode_inode_value(value: InodeValue) -> [u8; INODE_VALUE_BYTES] {
    let mut bytes = [0_u8; INODE_VALUE_BYTES];
    bytes[0] = value.kind.code();
    bytes[1..9].copy_from_slice(&value.namespace_ref_count.to_be_bytes());
    bytes[9..41].copy_from_slice(value.content_root.as_bytes());
    bytes[41..].copy_from_slice(value.metadata_root.as_bytes());
    bytes
}

/// Decodes one inode value, rejecting a wrong width or kind.
pub fn decode_inode_value(bytes: &[u8]) -> ContentResult<InodeValue> {
    if bytes.len() != INODE_VALUE_BYTES {
        return Err(ContentError::InvalidRecord("inode value width"));
    }
    Ok(InodeValue {
        kind: InodeKind::from_code(bytes[0])?,
        namespace_ref_count: u64::from_be_bytes(
            bytes[1..9]
                .try_into()
                .map_err(|_| ContentError::UnexpectedEof)?,
        ),
        content_root: ObjectId::from_bytes(&bytes[9..41])?,
        metadata_root: ObjectId::from_bytes(&bytes[41..])?,
    })
}

/// Wraps one inode value as its canonical pooled value object.
pub fn encode_pooled_value(value: &[u8; INODE_VALUE_BYTES]) -> ContentResult<Vec<u8>> {
    // The value is validated while it is wrapped, so a pooled group can only ever
    // hold values this grammar accepts.
    decode_inode_value(value)?;
    let mut payload = Vec::with_capacity(POOLED_VALUE_BYTES);
    payload.extend_from_slice(&POOLED_VALUE_MAGIC);
    payload.extend_from_slice(value);
    crate::object::codec::encode_bytes_object(&payload)
}

/// Borrows the inode value out of a canonical pooled value object.
pub fn decode_pooled_value(canonical: &[u8]) -> ContentResult<[u8; INODE_VALUE_BYTES]> {
    if canonical.len() != POOLED_VALUE_CANONICAL_BYTES {
        return Err(ContentError::InvalidRecord("pooled value length"));
    }
    let value = crate::object::codec::decode_bytes_object(canonical)?;
    if value.len() != POOLED_VALUE_BYTES || !value.starts_with(&POOLED_VALUE_MAGIC) {
        return Err(ContentError::InvalidRecord("pooled value framing"));
    }
    let mut bytes = [0_u8; INODE_VALUE_BYTES];
    bytes.copy_from_slice(&value[POOLED_VALUE_MAGIC.len()..]);
    decode_inode_value(&bytes)?;
    Ok(bytes)
}

/// One `(serial, value)` row of an inode leaf page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InodeLeafRow {
    /// Inode serial; strictly increasing within a page.
    pub serial: u64,
    /// The inode value.
    pub value: [u8; INODE_VALUE_BYTES],
}

/// One decoded inode leaf page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InodeLeaf {
    /// Serialized bytes covered by this page.
    pub subtree_bytes: u64,
    /// Rows in key order.
    pub rows: Vec<InodeLeafRow>,
}

impl InodeLeaf {
    /// Encodes the page into its canonical object bytes after full validation.
    ///
    /// The grammar itself is [`encode_leaf_value`]; this route adds the recorded
    /// subtree byte total and the object envelope.
    pub fn encode(&self) -> ContentResult<Vec<u8>> {
        let value = encode_leaf_value(&self.rows)?;
        let subtree_bytes = (self.rows.len() as u64)
            .checked_mul(LEAF_ROW_BYTES as u64)
            .ok_or(ContentError::LengthOverflow)?;
        if self.subtree_bytes != subtree_bytes {
            return Err(ContentError::LengthMismatch {
                expected: subtree_bytes,
                actual: self.subtree_bytes,
            });
        }
        crate::object::codec::encode_bytes_object(&value)
    }

    /// Decodes and validates a canonical leaf page without re-encoding it.
    ///
    /// The grammar itself is [`decode_leaf_value`]; this route adds the object
    /// envelope. A malformed or hand-built page fails here exactly as it fails
    /// the sorted engine's own leaf decoder, with the same error, because both
    /// call the one leaf grammar.
    pub fn decode(canonical: &[u8]) -> ContentResult<Self> {
        if canonical.len() > MAXIMUM_NODE_OBJECT_BYTES {
            return Err(ContentError::ObjectLimitExceeded {
                limit: MAXIMUM_NODE_OBJECT_BYTES,
                actual: canonical.len(),
            });
        }
        let value = crate::object::codec::decode_bytes_object(canonical)?;
        let rows = decode_leaf_value(value)?;
        Ok(Self {
            subtree_bytes: rows.len() as u64 * LEAF_ROW_BYTES as u64,
            rows,
        })
    }

    /// Row count.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Encoded row bytes this page accounts for (`row_count * 81`).
    pub fn row_bytes(&self) -> u64 {
        self.rows.len() as u64 * LEAF_ROW_BYTES as u64
    }
}

/// Encodes one checked leaf page value: the node header and the `(serial, value)`
/// rows. The caller owns the object envelope.
///
/// This is the single leaf-page grammar. [`InodeLeaf::encode`] and the sorted
/// engine's own inode-leaf encoder both call it, so one malformation cannot have
/// two different errors.
pub(crate) fn encode_leaf_value(rows: &[InodeLeafRow]) -> ContentResult<Vec<u8>> {
    let count = rows.len();
    if count == 0 || count > MAXIMUM_LEAF_ROWS {
        return Err(ContentError::NonCanonicalPagePartition);
    }
    if rows.windows(2).any(|pair| pair[0].serial >= pair[1].serial) {
        return Err(ContentError::InvalidRecord("inode key order"));
    }
    if rows.iter().any(|row| row.serial == 0) {
        return Err(ContentError::InvalidRecord("inode serial"));
    }
    let subtree_bytes = (count as u64)
        .checked_mul(LEAF_ROW_BYTES as u64)
        .ok_or(ContentError::LengthOverflow)?;
    let mut value = Vec::with_capacity(NODE_HEADER_BYTES + count * LEAF_ROW_BYTES);
    value.extend_from_slice(&INODE_LEAF_MAGIC);
    value.extend_from_slice(&INODE_LEAF_VERSION.to_be_bytes());
    value.extend_from_slice(&[LEAF_ROLE, 0, 0]);
    value.extend_from_slice(
        &u16::try_from(count)
            .map_err(|_| ContentError::LengthOverflow)?
            .to_be_bytes(),
    );
    value.extend_from_slice(&(count as u64).to_be_bytes());
    value.extend_from_slice(&subtree_bytes.to_be_bytes());
    for row in rows {
        value.extend_from_slice(&row.serial.to_be_bytes());
        value.extend_from_slice(&row.value);
    }
    Ok(value)
}

/// Decodes and validates one leaf page value already split out of its envelope.
///
/// Every check the leaf encoder enforces is repeated here directly: framing,
/// magic, version, role, level, flags, the row-count bound, the exact row width,
/// the declared byte total, serial ordering and the value grammar of every row.
/// A caller that only needs the rows never re-encodes to discover malformation.
pub(crate) fn decode_leaf_value(value: &[u8]) -> ContentResult<Vec<InodeLeafRow>> {
    if value.len() < NODE_HEADER_BYTES {
        return Err(ContentError::UnexpectedEof);
    }
    if !value.starts_with(&INODE_LEAF_MAGIC) {
        return Err(ContentError::UnsupportedFraming);
    }
    let version = u16::from_be_bytes([value[8], value[9]]);
    if version != INODE_LEAF_VERSION {
        return Err(ContentError::UnsupportedMappingVersion { version });
    }
    if value[10] != LEAF_ROLE || value[11] != 0 {
        return Err(ContentError::InvalidRecord("inode leaf role/level"));
    }
    // The reserved flag byte is named exactly as the shared node-header check
    // names it, so a page that carries it fails the same way on both routes.
    if value[12] != 0 {
        return Err(ContentError::InvalidRecord("node flags"));
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
    if count == 0 || count > MAXIMUM_LEAF_ROWS || subtree_count != count as u64 {
        return Err(ContentError::NonCanonicalPagePartition);
    }
    let rows_bytes = count
        .checked_mul(LEAF_ROW_BYTES)
        .ok_or(ContentError::LengthOverflow)?;
    if value.len()
        != NODE_HEADER_BYTES
            .checked_add(rows_bytes)
            .ok_or(ContentError::LengthOverflow)?
    {
        return Err(ContentError::InvalidRecord("inode leaf length"));
    }
    if subtree_bytes != rows_bytes as u64 {
        return Err(ContentError::LengthMismatch {
            expected: rows_bytes as u64,
            actual: subtree_bytes,
        });
    }
    let mut rows = Vec::with_capacity(count);
    let mut previous = None;
    for row in value[NODE_HEADER_BYTES..].chunks_exact(LEAF_ROW_BYTES) {
        let serial = u64::from_be_bytes(
            row[..8]
                .try_into()
                .map_err(|_| ContentError::UnexpectedEof)?,
        );
        if serial == 0 {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
        if previous.is_some_and(|previous| previous >= serial) {
            return Err(ContentError::InvalidRecord("inode key order"));
        }
        previous = Some(serial);
        let mut bytes = [0_u8; INODE_VALUE_BYTES];
        bytes.copy_from_slice(&row[8..]);
        decode_inode_value(&bytes)?;
        rows.push(InodeLeafRow {
            serial,
            value: bytes,
        });
    }
    Ok(rows)
}

/// Physical width of a pooled leaf for a canonical leaf of `canonical_length`.
pub fn pooled_physical_length(canonical_length: usize) -> ContentResult<usize> {
    let payload = canonical_length
        .checked_sub(POOLED_PREFIX_BYTES)
        .ok_or(ContentError::InvalidRecord("pooled leaf length"))?;
    if payload == 0 || payload % LEAF_ROW_BYTES != 0 || payload / LEAF_ROW_BYTES > MAXIMUM_LEAF_ROWS
    {
        return Err(ContentError::InvalidRecord("pooled leaf count"));
    }
    Ok(POOLED_PREFIX_BYTES + payload / LEAF_ROW_BYTES * POOLED_ROW_BYTES)
}

/// Builds the pooled physical body of a leaf: the prefix and `(serial, ordinal)` rows.
pub fn pooled_body(canonical: &[u8], ordinals: &[u32]) -> ContentResult<Vec<u8>> {
    let leaf = InodeLeaf::decode(canonical)?;
    if ordinals.len() != leaf.rows.len() || ordinals.iter().any(|ordinal| *ordinal == 0) {
        return Err(ContentError::InvalidRecord("pooled ordinal count"));
    }
    let mut body = Vec::with_capacity(pooled_physical_length(canonical.len())?);
    body.extend_from_slice(&canonical[..POOLED_PREFIX_BYTES]);
    for (row, ordinal) in leaf.rows.iter().zip(ordinals) {
        body.extend_from_slice(&row.serial.to_be_bytes());
        body.extend_from_slice(&ordinal.to_be_bytes());
    }
    Ok(body)
}

/// One row of a pooled physical body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PooledRow {
    /// Inode serial.
    pub serial: u64,
    /// Value-group ordinal.
    pub ordinal: u32,
}

/// Splits a pooled physical body into its prefix and rows.
pub fn decode_pooled_body(body: &[u8]) -> ContentResult<(Vec<u8>, Vec<PooledRow>)> {
    if body.len() < POOLED_PREFIX_BYTES {
        return Err(ContentError::UnexpectedEof);
    }
    let payload = body.len() - POOLED_PREFIX_BYTES;
    if payload == 0 || payload % POOLED_ROW_BYTES != 0 {
        return Err(ContentError::InvalidRecord("pooled body rows"));
    }
    let count = payload / POOLED_ROW_BYTES;
    if count > MAXIMUM_LEAF_ROWS {
        return Err(ContentError::NonCanonicalPagePartition);
    }
    let mut rows = Vec::with_capacity(count);
    for row in body[POOLED_PREFIX_BYTES..].chunks_exact(POOLED_ROW_BYTES) {
        let ordinal = u32::from_be_bytes(
            row[8..12]
                .try_into()
                .map_err(|_| ContentError::UnexpectedEof)?,
        );
        if ordinal == 0 {
            return Err(ContentError::InvalidRecord("pooled ordinal"));
        }
        rows.push(PooledRow {
            serial: u64::from_be_bytes(
                row[..8]
                    .try_into()
                    .map_err(|_| ContentError::UnexpectedEof)?,
            ),
            ordinal,
        });
    }
    if rows.windows(2).any(|pair| pair[0].serial >= pair[1].serial) {
        return Err(ContentError::InvalidRecord("pooled key order"));
    }
    let mut prefix = body[..POOLED_PREFIX_BYTES].to_vec();
    // The node header records the row count and the serialized byte total; the
    // pooled body carries `count` rows, so both fields must match it.
    prefix[13..15].copy_from_slice(&(count as u16).to_be_bytes());
    prefix[15..23].copy_from_slice(&(count as u64).to_be_bytes());
    prefix[23..31].copy_from_slice(&(count as u64 * LEAF_ROW_BYTES as u64).to_be_bytes());
    Ok((prefix, rows))
}

/// Rebuilds the canonical leaf from a pooled prefix, rows and resolved values.
pub fn rebuild_leaf(
    prefix: &[u8],
    rows: &[PooledRow],
    values: &[[u8; INODE_VALUE_BYTES]],
) -> ContentResult<Vec<u8>> {
    if rows.len() != values.len() {
        return Err(ContentError::InvalidRecord("pooled value count"));
    }
    let mut leaf = InodeLeaf {
        subtree_bytes: rows.len() as u64 * LEAF_ROW_BYTES as u64,
        rows: rows
            .iter()
            .zip(values)
            .map(|(row, value)| InodeLeafRow {
                serial: row.serial,
                value: *value,
            })
            .collect(),
    };
    if prefix.len() != POOLED_PREFIX_BYTES {
        return Err(ContentError::InvalidRecord("pooled prefix width"));
    }
    // The canonical form is rebuilt from the header constant and the decoded rows,
    // never from the supplied bytes: the prefix was validated above for width and
    // grammar, and its contents are deliberately not carried into the result.
    leaf.subtree_bytes = rows.len() as u64 * LEAF_ROW_BYTES as u64;
    leaf.encode()
}

const LEAF_ROLE: u8 = 7;
