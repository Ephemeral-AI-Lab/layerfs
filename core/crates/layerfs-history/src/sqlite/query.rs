//! Opaque continuation cursors and byte-bounded page assembly.
//!
//! A cursor is a fixed 160-byte value. It binds the catalog identity and
//! incarnation, the range it belongs to, the immutable anchor that range was
//! opened at, and the position to resume after; a sixteen-byte digest covers
//! everything before it. The server recomputes that digest from the request it
//! is serving, so a cursor from another catalog, another incarnation, another
//! query or with a tampered body fails validation instead of being honoured.
//!
//! Growth of a live Branch never invalidates a cursor: the anchor is an
//! immutable Commit or Layer, and the walk from it is the parent chain, which
//! cannot change. A cursor is not a snapshot and makes no such claim.

use crate::error::{HistoryError, HistoryResult};
use crate::identity::CatalogId;
use crate::records::{
    PageResult, MAXIMUM_CURSOR_BYTES, MAXIMUM_PAGE_RECORDS, MAXIMUM_RESULT_BYTES,
};

/// Frozen cursor format version.
pub(crate) const CURSOR_VERSION: u8 = 1;
/// Fixed encoded width of one cursor.
pub(crate) const CURSOR_BYTES: usize = MAXIMUM_CURSOR_BYTES;
/// Bytes each variable field occupies, zero padded.
const FIELD_BYTES: usize = 33;
/// Offset of the subject field.
const SUBJECT_AT: usize = 42;
/// Offset of the anchor field.
const ANCHOR_AT: usize = SUBJECT_AT + FIELD_BYTES + 1;
/// Offset of the position field.
const POSITION_AT: usize = ANCHOR_AT + FIELD_BYTES + 1;
/// Offset of the digest.
const DIGEST_AT: usize = POSITION_AT + FIELD_BYTES + 1;

/// The bounded range a cursor resumes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Range {
    /// LayerStacks in name order.
    StackNames = 1,
    /// Branches of one stack in name order.
    BranchNames = 2,
    /// Commits of one Branch in ancestry order.
    CommitAncestry = 3,
    /// Layers of one stack in publication order.
    LayerChain = 4,
    /// Stages of one Branch in token order.
    StageTokens = 5,
}

/// The request context a cursor is only valid inside.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Context<'a> {
    /// Catalog the cursor was issued by.
    pub(crate) catalog: CatalogId,
    /// Incarnation the cursor was issued in.
    pub(crate) incarnation: u64,
    /// Range being resumed.
    pub(crate) range: Range,
    /// Subject identity bytes, empty when the range has none.
    pub(crate) subject: &'a [u8],
}

/// A decoded continuation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Cursor {
    /// Immutable starting identity of the range.
    pub(crate) anchor: Vec<u8>,
    /// Last record already delivered.
    pub(crate) position: Vec<u8>,
}

/// Encodes one continuation for a range that has more records.
pub(crate) fn encode(context: Context<'_>, cursor: &Cursor) -> HistoryResult<Vec<u8>> {
    let mut bytes = [0u8; CURSOR_BYTES];
    bytes[0] = CURSOR_VERSION;
    bytes[1] = context.range as u8;
    bytes[2..34].copy_from_slice(context.catalog.as_slice());
    bytes[34..42].copy_from_slice(&context.incarnation.to_be_bytes());
    place(&mut bytes, SUBJECT_AT, context.subject)?;
    place(&mut bytes, ANCHOR_AT, &cursor.anchor)?;
    place(&mut bytes, POSITION_AT, &cursor.position)?;
    let digest = digest(&bytes[..DIGEST_AT]);
    bytes[DIGEST_AT..].copy_from_slice(&digest);
    Ok(bytes.to_vec())
}

/// Validates one continuation against the request being served.
pub(crate) fn decode(context: Context<'_>, bytes: &[u8]) -> HistoryResult<Cursor> {
    if bytes.len() != CURSOR_BYTES {
        return Err(HistoryError::InvalidInput("page cursor width"));
    }
    let mut expected = [0u8; CURSOR_BYTES];
    expected.copy_from_slice(bytes);
    if expected[0] != CURSOR_VERSION || expected[1] != context.range as u8 {
        return Err(HistoryError::InvalidInput("page cursor range"));
    }
    if expected[2..34] != *context.catalog.as_slice()
        || expected[34..42] != context.incarnation.to_be_bytes()
    {
        return Err(HistoryError::InvalidInput("page cursor catalog"));
    }
    if expected[DIGEST_AT..] != digest(&expected[..DIGEST_AT]) {
        return Err(HistoryError::Integrity("page cursor digest"));
    }
    let subject = field(&expected, SUBJECT_AT)?;
    if subject != context.subject {
        return Err(HistoryError::InvalidInput("page cursor subject"));
    }
    Ok(Cursor {
        anchor: field(&expected, ANCHOR_AT)?.to_vec(),
        position: field(&expected, POSITION_AT)?.to_vec(),
    })
}

/// How many records of one kind fit a byte-bounded page.
///
/// The caller's `limit` is an upper bound; the encoded-result budget is the
/// other. A page always carries at least one record when rows remain, because
/// the widest record is far smaller than the budget.
pub(crate) fn capacity(limit: u16, per_record: usize) -> HistoryResult<usize> {
    if limit == 0 || limit > MAXIMUM_PAGE_RECORDS {
        return Err(HistoryError::InvalidInput("page limit"));
    }
    let available = MAXIMUM_RESULT_BYTES.saturating_sub(MAXIMUM_CURSOR_BYTES + 16);
    Ok(usize::from(limit).min((available / per_record.max(1)).max(1)))
}

/// Assembles one page from rows already in query order.
///
/// The caller collects one row beyond the page when more remain; that extra row
/// only proves the range continues. The continuation names the **last delivered**
/// record, and every range resumes strictly after it: a name or token range with
/// an exclusive comparison, and an ancestry walk by stepping to that record's
/// parent. One convention, so no page repeats a record and none skips one.
pub(crate) fn page<T>(
    mut records: Vec<T>,
    capacity: usize,
    more: bool,
    position: impl FnOnce(&T) -> HistoryResult<Option<Vec<u8>>>,
) -> HistoryResult<PageResult<T>> {
    let continuation = if more && records.len() > capacity {
        records.truncate(capacity);
        match records.last() {
            Some(last) => position(last)?,
            None => None,
        }
    } else {
        None
    };
    Ok(PageResult {
        records,
        continuation,
    })
}

fn place(target: &mut [u8; CURSOR_BYTES], at: usize, value: &[u8]) -> HistoryResult<()> {
    if value.len() > FIELD_BYTES {
        return Err(HistoryError::InvalidInput("page cursor field"));
    }
    target[at] = value.len() as u8;
    target[at + 1..at + 1 + value.len()].copy_from_slice(value);
    Ok(())
}

fn field(bytes: &[u8; CURSOR_BYTES], at: usize) -> HistoryResult<&[u8]> {
    let length = usize::from(bytes[at]);
    if length > FIELD_BYTES {
        return Err(HistoryError::Integrity("page cursor field"));
    }
    let value = &bytes[at + 1..at + 1 + length];
    if bytes[at + 1 + length..at + 1 + FIELD_BYTES]
        .iter()
        .any(|byte| *byte != 0)
    {
        return Err(HistoryError::Integrity("page cursor padding"));
    }
    Ok(value)
}

fn digest(bytes: &[u8]) -> [u8; 16] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/history/cursor/v1\0");
    hasher.update(bytes);
    let full = hasher.finalize();
    let mut out = [0u8; 16];
    out.copy_from_slice(&full.as_bytes()[..16]);
    out
}
