//! The two real page formats of the filesystem tree and their exact sizes.
//!
//! The sorted engine is one bounded B+tree mutation algorithm over two concrete
//! formats: compact directory pages (`LFS6NSP`) and compact inline inode pages
//! (`LFS6INT`). This module owns their grammar, their node-header fields, their
//! per-row widths and their fill rules, so the partition decisions the engine
//! makes are computed from the same numbers the encoder writes.
//!
//! Sizes are exact arithmetic (`44 + sum(row widths)`), never trial encodings: a
//! candidate page is never cloned and encoded just to discover whether it fits.

use crate::error::{ContentError, ContentResult};
use crate::filesystem::limits::{
    MAXIMUM_INODE_BRANCH_CHILDREN, MAXIMUM_INODE_LEAF_ROWS, MAXIMUM_PAGE_BYTES, MAXIMUM_TREE_LEVEL,
    MINIMUM_FILLED_PAGE_BYTES, MINIMUM_INODE_BRANCH_CHILDREN, MINIMUM_INODE_LEAF_ROWS,
};
use crate::filesystem::path::PathName;
use crate::object::inode_leaf::{InodeValue, LEAF_ROW_BYTES};
use crate::object::{codec, ObjectId};

/// Canonical bytes of an empty page: object envelope plus node header.
pub const EMPTY_PAGE_BYTES: usize = 44;
/// Width of the shared node header.
pub const NODE_HEADER_BYTES: usize = 31;
/// Grammar version of both page formats.
pub const NODE_VERSION: u16 = 1;
/// Directory page magic.
pub const DIRECTORY_MAGIC: [u8; 8] = *b"LFS6NSP\0";
/// Inode page magic.
pub const INODE_MAGIC: [u8; 8] = *b"LFS6INT\0";
/// Directory leaf role.
pub const DIRECTORY_LEAF_ROLE: u8 = 1;
/// Directory branch role.
pub const DIRECTORY_BRANCH_ROLE: u8 = 2;
/// Inode leaf role.
pub const INODE_LEAF_ROLE: u8 = 7;
/// Inode branch role.
pub const INODE_BRANCH_ROLE: u8 = 8;
/// Serial width inside a directory leaf row.
pub const DIRECTORY_LEAF_SERIAL_BYTES: usize = 8;
/// Child identity width inside a branch row.
pub const BRANCH_CHILD_BYTES: usize = 32;
/// Length-prefix width of a stored name.
pub const NAME_LENGTH_BYTES: usize = 2;
/// Inode branch row width.
pub const INODE_BRANCH_ROW_BYTES: usize = 40;
/// Row cells a page reserves before its rows are appended.
pub const DEFAULT_PAGE_ITEMS: usize = 234;

/// One decoded page before the engine turns it into entries.
pub(crate) struct Wire<K, V> {
    /// Page level; zero is a leaf.
    pub level: u8,
    /// Recorded subtree entry count.
    pub count: u64,
    /// Recorded subtree byte total.
    pub bytes: u64,
    /// Rows in key order: key, child identity for a branch, typed value for a leaf.
    pub entries: Vec<WireEntry<K, V>>,
    /// Canonical length of the page these rows came from.
    pub size: usize,
}

/// One decoded row of a page.
pub(crate) struct WireEntry<K, V> {
    /// Row key.
    pub key: K,
    /// Child page identity; present only in a branch row.
    pub child: Option<ObjectId>,
    /// Typed leaf value; present only in a leaf row.
    pub value: Option<V>,
}

/// The page grammar the engine mutates.
pub(crate) trait Format {
    /// Row key type.
    type Key: Ord + Clone;
    /// Typed leaf value type.
    type Value: Clone;

    /// Decodes one authenticated canonical page.
    fn decode(canonical: &[u8]) -> ContentResult<Wire<Self::Key, Self::Value>>;
    /// Encodes one final page.
    fn encode(page: &PageView<'_, Self::Key, Self::Value>) -> ContentResult<Vec<u8>>;
    /// Exact encoded width of one row at `level`.
    fn width(key: &Self::Key, level: u8) -> usize;
    /// Heap bytes one key retains while the page is held decoded.
    fn heap_bytes(key: &Self::Key) -> usize;
    /// Scratch the decoder needs for a page of `bytes` canonical length.
    fn decode_scratch(bytes: usize) -> usize;
    /// Upper bound on rows one page may hold at `level`.
    fn page_items(level: u8) -> usize;
    /// True when a non-root page of this size and row count satisfies the fill rule.
    fn filled(size: usize, count: usize, level: u8) -> bool;
    /// True when a page of this exact size and row count fits the page ceiling.
    fn fits(size: usize, count: usize, level: u8) -> bool;
    /// True when an empty page is a legal final representation.
    fn empty_allowed() -> bool;
    /// Logical role code for the page at `level`.
    fn role(level: u8) -> u8;
    /// Page magic for the format.
    fn magic() -> &'static [u8; 8];
}

/// One row handed to a format encoder.
pub(crate) struct RowView<'a, K, V> {
    /// Row key.
    pub key: &'a K,
    /// Child page identity; a branch row only.
    pub child: Option<ObjectId>,
    /// Typed leaf value; a leaf row only.
    pub value: Option<&'a V>,
}

/// A page handed to a format encoder.
///
/// For a branch the recorded subtree totals are the page's own summary and are
/// taken from `count`/`bytes`; a leaf derives both from its rows.
pub(crate) struct PageView<'a, K, V> {
    /// Page level.
    pub level: u8,
    /// Entries in this subtree; branches only.
    pub count: u64,
    /// Encoded row bytes in this subtree; branches only.
    pub bytes: u64,
    /// Rows in key order.
    pub rows: &'a [RowView<'a, K, V>],
}

/// The compact directory format: `name -> inode serial`, 8-byte children.
pub(crate) struct CompactDirectory;

impl Format for CompactDirectory {
    type Key = PathName;
    type Value = u64;

    fn decode(canonical: &[u8]) -> ContentResult<Wire<Self::Key, Self::Value>> {
        let value = decode_node_value(canonical, &DIRECTORY_MAGIC)?;
        let role = value[10];
        let level = value[11];
        let count = node_count(value);
        let subtree_count = node_subtree_count(value);
        let subtree_bytes = node_subtree_bytes(value);
        let mut cursor = NODE_HEADER_BYTES;
        let mut entries = Vec::with_capacity(count);
        match role {
            DIRECTORY_LEAF_ROLE if level == 0 && subtree_count == count as u64 => {
                for _ in 0..count {
                    let key = PathName::from_bytes(take_name(value, &mut cursor)?)?;
                    let serial = u64::from_be_bytes(
                        take(value, &mut cursor, DIRECTORY_LEAF_SERIAL_BYTES)?
                            .try_into()
                            .map_err(|_| ContentError::UnexpectedEof)?,
                    );
                    if serial == 0 {
                        return Err(ContentError::InvalidRecord("directory serial"));
                    }
                    entries.push(WireEntry {
                        key,
                        child: None,
                        value: Some(serial),
                    });
                }
                if cursor != value.len() {
                    return Err(ContentError::TrailingBytes);
                }
                let expected = entries.iter().try_fold(0_u64, |sum, entry| {
                    sum.checked_add(
                        (NAME_LENGTH_BYTES
                            + DIRECTORY_LEAF_SERIAL_BYTES
                            + entry.key.as_bytes().len()) as u64,
                    )
                    .ok_or(ContentError::LengthOverflow)
                })?;
                if expected != subtree_bytes {
                    return Err(ContentError::LengthMismatch {
                        expected,
                        actual: subtree_bytes,
                    });
                }
                reject_unordered(&entries)?;
                Ok(Wire {
                    level,
                    count: count as u64,
                    bytes: subtree_bytes,
                    size: canonical.len(),
                    entries,
                })
            }
            DIRECTORY_BRANCH_ROLE if level > 0 && level <= MAXIMUM_TREE_LEVEL && count >= 2 => {
                for _ in 0..count {
                    let key = PathName::from_bytes(take_name(value, &mut cursor)?)?;
                    let child =
                        ObjectId::from_bytes(take(value, &mut cursor, BRANCH_CHILD_BYTES)?)?;
                    entries.push(WireEntry {
                        key,
                        child: Some(child),
                        value: None,
                    });
                }
                if cursor != value.len() {
                    return Err(ContentError::TrailingBytes);
                }
                reject_unordered(&entries)?;
                Ok(Wire {
                    level,
                    count: subtree_count,
                    bytes: subtree_bytes,
                    size: canonical.len(),
                    entries,
                })
            }
            _ => Err(ContentError::InvalidRecord("directory role/level")),
        }
    }

    fn encode(page: &PageView<'_, Self::Key, Self::Value>) -> ContentResult<Vec<u8>> {
        let count = page.rows.len();
        validate_count(
            page.level,
            count,
            <Self as Format>::page_items(page.level),
            <Self as Format>::empty_allowed(),
        )?;
        let rows = page.rows;
        let mut previous: Option<&[u8]> = None;
        for row in rows {
            if previous.is_some_and(|prior| prior >= row.key.as_bytes()) {
                return Err(ContentError::NonCanonicalOrdering);
            }
            previous = Some(row.key.as_bytes());
        }
        let (total, bytes) = if page.level == 0 {
            let bytes = rows.iter().try_fold(0_u64, |sum, row| {
                sum.checked_add(
                    (NAME_LENGTH_BYTES + DIRECTORY_LEAF_SERIAL_BYTES + row.key.as_bytes().len())
                        as u64,
                )
                .ok_or(ContentError::LengthOverflow)
            })?;
            (count as u64, bytes)
        } else {
            (page.count, page.bytes)
        };
        if page.level > 0 && total < count as u64 {
            return Err(ContentError::InvalidRecord("directory subtree summary"));
        }
        let mut value = node_header(
            &DIRECTORY_MAGIC,
            Self::role(page.level),
            page.level,
            count,
            total,
            bytes,
        )?;
        for row in rows {
            put_name(&mut value, row.key.as_bytes())?;
            match page.level {
                0 => value.extend_from_slice(
                    &row.value
                        .ok_or(ContentError::InvalidRecord("directory leaf value"))?
                        .to_be_bytes(),
                ),
                _ => value.extend_from_slice(
                    row.child
                        .ok_or(ContentError::InvalidRecord("directory child"))?
                        .as_bytes(),
                ),
            }
        }
        finish_node(value)
    }

    fn width(key: &Self::Key, level: u8) -> usize {
        NAME_LENGTH_BYTES
            + key.as_bytes().len()
            + if level == 0 {
                DIRECTORY_LEAF_SERIAL_BYTES
            } else {
                BRANCH_CHILD_BYTES
            }
    }

    fn heap_bytes(key: &Self::Key) -> usize {
        key.as_bytes().len()
    }

    fn decode_scratch(bytes: usize) -> usize {
        bytes * 3 + bytes.saturating_sub(EMPTY_PAGE_BYTES) / 11 * 48
    }

    fn page_items(level: u8) -> usize {
        if level == 0 {
            (MAXIMUM_PAGE_BYTES - EMPTY_PAGE_BYTES) / 11 + 1
        } else {
            DEFAULT_PAGE_ITEMS
        }
    }

    fn filled(size: usize, _count: usize, _level: u8) -> bool {
        size >= MINIMUM_FILLED_PAGE_BYTES
    }

    fn fits(size: usize, _count: usize, _level: u8) -> bool {
        size <= MAXIMUM_PAGE_BYTES
    }

    fn empty_allowed() -> bool {
        true
    }

    fn role(level: u8) -> u8 {
        if level == 0 {
            DIRECTORY_LEAF_ROLE
        } else {
            DIRECTORY_BRANCH_ROLE
        }
    }

    fn magic() -> &'static [u8; 8] {
        &DIRECTORY_MAGIC
    }
}

/// The compact inline inode format: `serial -> 73-byte value`.
pub(crate) struct CompactInodes;

impl Format for CompactInodes {
    type Key = u64;
    type Value = InodeValue;

    fn decode(canonical: &[u8]) -> ContentResult<Wire<Self::Key, Self::Value>> {
        let value = decode_node_value(canonical, &INODE_MAGIC)?;
        let role = value[10];
        let level = value[11];
        let count = node_count(value);
        let subtree_count = node_subtree_count(value);
        let subtree_bytes = node_subtree_bytes(value);
        let mut cursor = NODE_HEADER_BYTES;
        let mut entries = Vec::with_capacity(count);
        match role {
            INODE_LEAF_ROLE
                if level == 0
                    && subtree_count == count as u64
                    && count >= 1
                    && count as u64 <= MAXIMUM_INODE_LEAF_ROWS =>
            {
                for _ in 0..count {
                    let key = u64::from_be_bytes(
                        take(value, &mut cursor, 8)?
                            .try_into()
                            .map_err(|_| ContentError::UnexpectedEof)?,
                    );
                    if key == 0 {
                        return Err(ContentError::InvalidRecord("inode serial"));
                    }
                    let record = crate::object::inode_leaf::decode_inode_value(take(
                        value,
                        &mut cursor,
                        LEAF_ROW_BYTES - 8,
                    )?)?;
                    entries.push(WireEntry {
                        key,
                        child: None,
                        value: Some(record),
                    });
                }
                if cursor != value.len() {
                    return Err(ContentError::TrailingBytes);
                }
                let expected = (count as u64)
                    .checked_mul(LEAF_ROW_BYTES as u64)
                    .ok_or(ContentError::LengthOverflow)?;
                if expected != subtree_bytes {
                    return Err(ContentError::LengthMismatch {
                        expected,
                        actual: subtree_bytes,
                    });
                }
                reject_unordered(&entries)?;
                Ok(Wire {
                    level,
                    count: count as u64,
                    bytes: subtree_bytes,
                    size: canonical.len(),
                    entries,
                })
            }
            INODE_BRANCH_ROLE
                if level > 0
                    && level <= MAXIMUM_TREE_LEVEL
                    && count >= 2
                    && count as u64 <= MAXIMUM_INODE_BRANCH_CHILDREN =>
            {
                for _ in 0..count {
                    let key = u64::from_be_bytes(
                        take(value, &mut cursor, 8)?
                            .try_into()
                            .map_err(|_| ContentError::UnexpectedEof)?,
                    );
                    if key == 0 {
                        return Err(ContentError::InvalidRecord("inode serial"));
                    }
                    let child =
                        ObjectId::from_bytes(take(value, &mut cursor, BRANCH_CHILD_BYTES)?)?;
                    entries.push(WireEntry {
                        key,
                        child: Some(child),
                        value: None,
                    });
                }
                if cursor != value.len() {
                    return Err(ContentError::TrailingBytes);
                }
                if subtree_count < count as u64 {
                    return Err(ContentError::InvalidRecord("inode subtree summary"));
                }
                let expected = subtree_count
                    .checked_mul(LEAF_ROW_BYTES as u64)
                    .ok_or(ContentError::LengthOverflow)?;
                if expected != subtree_bytes {
                    return Err(ContentError::LengthMismatch {
                        expected,
                        actual: subtree_bytes,
                    });
                }
                reject_unordered(&entries)?;
                Ok(Wire {
                    level,
                    count: subtree_count,
                    bytes: subtree_bytes,
                    size: canonical.len(),
                    entries,
                })
            }
            _ => Err(ContentError::InvalidRecord("inode role/level")),
        }
    }

    fn encode(page: &PageView<'_, Self::Key, Self::Value>) -> ContentResult<Vec<u8>> {
        let count = page.rows.len();
        validate_count(
            page.level,
            count,
            <Self as Format>::page_items(page.level),
            <Self as Format>::empty_allowed(),
        )?;
        let mut previous: Option<u64> = None;
        for row in page.rows {
            if previous.is_some_and(|prior| prior >= *row.key) {
                return Err(ContentError::NonCanonicalOrdering);
            }
            if *row.key == 0 {
                return Err(ContentError::InvalidRecord("inode serial"));
            }
            previous = Some(*row.key);
        }
        let (total, bytes) = if page.level == 0 {
            let bytes = (count as u64)
                .checked_mul(LEAF_ROW_BYTES as u64)
                .ok_or(ContentError::LengthOverflow)?;
            (count as u64, bytes)
        } else {
            // A branch's recorded totals are its own summary: the engine passes
            // the totals it accumulated from the children it read.
            let total = page.count;
            if total < count as u64 {
                return Err(ContentError::InvalidRecord("inode subtree summary"));
            }
            (
                total,
                total
                    .checked_mul(LEAF_ROW_BYTES as u64)
                    .ok_or(ContentError::LengthOverflow)?,
            )
        };
        let mut value = node_header(
            &INODE_MAGIC,
            Self::role(page.level),
            page.level,
            count,
            total,
            bytes,
        )?;
        for row in page.rows {
            value.extend_from_slice(&row.key.to_be_bytes());
            match page.level {
                0 => {
                    let record = row
                        .value
                        .ok_or(ContentError::InvalidRecord("inode leaf value"))?;
                    value
                        .extend_from_slice(&crate::object::inode_leaf::encode_inode_value(*record));
                }
                _ => value.extend_from_slice(
                    row.child
                        .ok_or(ContentError::InvalidRecord("inode child"))?
                        .as_bytes(),
                ),
            }
        }
        finish_node(value)
    }

    fn width(_key: &Self::Key, level: u8) -> usize {
        if level == 0 {
            LEAF_ROW_BYTES
        } else {
            INODE_BRANCH_ROW_BYTES
        }
    }

    fn heap_bytes(_key: &Self::Key) -> usize {
        0
    }

    fn decode_scratch(bytes: usize) -> usize {
        bytes * 3 + bytes.saturating_sub(EMPTY_PAGE_BYTES) / LEAF_ROW_BYTES * 88
    }

    fn page_items(_level: u8) -> usize {
        // Cells reserved before any row is appended. The row limit is enforced by
        // `fits` after each append, exactly as the reference does: a page that
        // reached its row ceiling is split, never refused.
        DEFAULT_PAGE_ITEMS
    }

    fn filled(_size: usize, count: usize, level: u8) -> bool {
        let minimum = if level == 0 {
            MINIMUM_INODE_LEAF_ROWS
        } else {
            MINIMUM_INODE_BRANCH_CHILDREN
        };
        count as u64 >= minimum
    }

    fn fits(_size: usize, count: usize, level: u8) -> bool {
        let maximum = if level == 0 {
            MAXIMUM_INODE_LEAF_ROWS
        } else {
            MAXIMUM_INODE_BRANCH_CHILDREN
        };
        count as u64 <= maximum
    }

    fn empty_allowed() -> bool {
        false
    }

    fn role(level: u8) -> u8 {
        if level == 0 {
            INODE_LEAF_ROLE
        } else {
            INODE_BRANCH_ROLE
        }
    }

    fn magic() -> &'static [u8; 8] {
        &INODE_MAGIC
    }
}

/// Index one row past the midpoint of `widths`, as the reference does: the
/// smallest prefix whose doubling is closest to the total, breaking ties toward
/// the smaller left page.
pub(crate) fn nearest_half(widths: &[usize]) -> usize {
    let total: usize = widths.iter().sum();
    let mut prefix = 0;
    let mut best = 1;
    let mut distance = usize::MAX;
    for (index, width) in widths
        .iter()
        .enumerate()
        .take(widths.len().saturating_sub(1))
    {
        prefix += width;
        let next = total.abs_diff(prefix * 2);
        if next < distance {
            best = index + 1;
            distance = next;
        }
    }
    best
}

fn validate_count(
    level: u8,
    count: usize,
    maximum: usize,
    empty_allowed: bool,
) -> ContentResult<()> {
    if level > MAXIMUM_TREE_LEVEL {
        return Err(ContentError::MappingDepthExceeded);
    }
    if (count == 0 && !empty_allowed) || count > maximum {
        return Err(ContentError::NonCanonicalPagePartition);
    }
    if u16::try_from(count).is_err() {
        return Err(ContentError::ObjectLimitExceeded {
            limit: u16::MAX as usize,
            actual: count,
        });
    }
    Ok(())
}

fn reject_unordered<K, V>(entries: &[WireEntry<K, V>]) -> ContentResult<()>
where
    K: Ord,
{
    if entries.windows(2).any(|pair| pair[0].key >= pair[1].key) {
        return Err(ContentError::NonCanonicalOrdering);
    }
    Ok(())
}

/// Splits one canonical page value out of its envelope and checks the header.
pub(crate) fn decode_node_value<'a>(
    canonical: &'a [u8],
    magic: &[u8; 8],
) -> ContentResult<&'a [u8]> {
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
    if &value[..8] != magic {
        return Err(ContentError::UnsupportedFraming);
    }
    let version = u16::from_be_bytes([value[8], value[9]]);
    if version != NODE_VERSION {
        return Err(ContentError::UnsupportedMappingVersion { version });
    }
    if value[12] != 0 {
        return Err(ContentError::InvalidRecord("node flags"));
    }
    if value[11] > MAXIMUM_TREE_LEVEL {
        return Err(ContentError::MappingDepthExceeded);
    }
    Ok(value)
}

/// Builds one node header.
pub(crate) fn node_header(
    magic: &[u8; 8],
    role: u8,
    level: u8,
    count: usize,
    subtree_count: u64,
    subtree_bytes: u64,
) -> ContentResult<Vec<u8>> {
    let count = u16::try_from(count).map_err(|_| ContentError::LengthOverflow)?;
    let mut value = Vec::with_capacity(NODE_HEADER_BYTES);
    value.extend_from_slice(magic);
    value.extend_from_slice(&NODE_VERSION.to_be_bytes());
    value.extend_from_slice(&[role, level, 0]);
    value.extend_from_slice(&count.to_be_bytes());
    value.extend_from_slice(&subtree_count.to_be_bytes());
    value.extend_from_slice(&subtree_bytes.to_be_bytes());
    Ok(value)
}

/// Wraps one page value that is already known to fit the page ceiling.
pub(crate) fn finish_node(value: Vec<u8>) -> ContentResult<Vec<u8>> {
    let canonical = codec::encode_bytes_object(&value)?;
    if canonical.len() > MAXIMUM_PAGE_BYTES {
        return Err(ContentError::ObjectLimitExceeded {
            limit: MAXIMUM_PAGE_BYTES,
            actual: canonical.len(),
        });
    }
    Ok(canonical)
}

/// Recorded row/entry count of a page value.
pub(crate) fn node_count(value: &[u8]) -> usize {
    usize::from(u16::from_be_bytes([value[13], value[14]]))
}

/// Recorded subtree entry count of a page value.
pub(crate) fn node_subtree_count(value: &[u8]) -> u64 {
    u64::from_be_bytes(value[15..23].try_into().unwrap_or([0; 8]))
}

/// Recorded subtree byte total of a page value.
pub(crate) fn node_subtree_bytes(value: &[u8]) -> u64 {
    u64::from_be_bytes(value[23..31].try_into().unwrap_or([0; 8]))
}

/// Writes one length-prefixed name.
pub(crate) fn put_name(output: &mut Vec<u8>, bytes: &[u8]) -> ContentResult<()> {
    let length = u16::try_from(bytes.len()).map_err(|_| ContentError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

/// Reads one length-prefixed name of at most 255 bytes.
pub(crate) fn take_name<'a>(bytes: &'a [u8], cursor: &mut usize) -> ContentResult<&'a [u8]> {
    let prefix = take(bytes, cursor, NAME_LENGTH_BYTES)?;
    let length = usize::from(u16::from_be_bytes([prefix[0], prefix[1]]));
    if length > 255 {
        return Err(ContentError::PathLimitExceeded);
    }
    take(bytes, cursor, length)
}

/// Bounded slice reader.
pub(crate) fn take<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
    length: usize,
) -> ContentResult<&'a [u8]> {
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
