//! Shared saved-file facts and independent byte, metadata and identity changes.
use super::{compare, snapshot};
use crate::history::lookup as args;
use crate::store::StorePorts;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::compare::{tree::same_metadata, EntryFacts};
use layerfs_content::filesystem::attributes::{read_portable, AttributeReadWork};

/// Metadata for a present file; a missing side remains a literal null.
pub fn facts(reader: &StorePorts, entry: Option<EntryFacts>) -> Result<SdkValue, SdkRefusal> {
    let Some(entry) = entry else {
        return Ok(SdkValue::Null);
    };
    let metadata = read_portable(
        reader,
        entry.value.metadata_root,
        entry.value.kind,
        &mut AttributeReadWork::default(),
    )
    .map_err(args::content)?;
    Ok(SdkValue::map([
        (
            "type",
            SdkValue::text(snapshot::kind_name(entry.value.kind)),
        ),
        ("inode", SdkValue::Int(entry.serial as i64)),
        (
            "size",
            SdkValue::Int(snapshot::size(reader, &entry)? as i64),
        ),
        ("mode", SdkValue::Int(i64::from(metadata.mode))),
    ]))
}

/// Absent sides are explained by Added/Removed; pairwise flags stay false.
pub fn changes(
    reader: &StorePorts,
    left: Option<EntryFacts>,
    right: Option<EntryFacts>,
) -> Result<(bool, bool, bool), SdkRefusal> {
    let (Some(left), Some(right)) = (left, right) else {
        return Ok((false, false, false));
    };
    let metadata = !same_metadata(
        reader,
        left.value.kind,
        left.value.metadata_root,
        right.value.metadata_root,
        &mut AttributeReadWork::default(),
    )
    .map_err(args::content)?;
    Ok((
        !compare::same_content(reader, &left, &right)?,
        metadata,
        left.serial != right.serial,
    ))
}

/// Potential logical payload admitted by a new comparison/history query.
/// Equal roots and unequal lengths settle byte equality without payload replay.
pub const CONTENT_QUERY_BYTES: u64 = 64 * 1024 * 1024;

/// Reserve payload work before an exact different-root byte comparison. Capacity
/// is unknown/refused work, never a false content change as a root-only fallback.
pub fn changes_bounded(
    reader: &StorePorts,
    left: Option<EntryFacts>,
    right: Option<EntryFacts>,
    remaining: &mut u64,
) -> Result<(bool, bool, bool), SdkRefusal> {
    if let (Some(left), Some(right)) = (left, right) {
        if left.value.kind == layerfs_content::object::inode_leaf::InodeKind::RegularFile
            && right.value.kind == left.value.kind
            && left.value.content_root != right.value.content_root
        {
            let a = snapshot::size(reader, &left)?;
            let b = snapshot::size(reader, &right)?;
            if a == b {
                let required = a.saturating_add(b);
                if required > *remaining {
                    return Err(super::query::capacity(
                        "content comparison payload budget reached; narrow the saved comparison",
                    ));
                }
                *remaining -= required;
            }
        }
    }
    changes(reader, left, right)
}

pub fn flags(content: bool, metadata: bool, identity: bool, attribution: Option<bool>) -> SdkValue {
    SdkValue::map([
        ("content", SdkValue::Bool(content)),
        ("metadata", SdkValue::Bool(metadata)),
        ("identity", SdkValue::Bool(identity)),
        (
            "attribution",
            attribution.map_or(SdkValue::Null, SdkValue::Bool),
        ),
    ])
}

pub fn coverage(status: &str, comparison: Option<&str>, reasons: &[&str]) -> SdkValue {
    SdkValue::map([
        ("status", SdkValue::text(status)),
        (
            "comparison",
            comparison.map_or(SdkValue::Null, SdkValue::text),
        ),
        (
            "reasons",
            SdkValue::List(reasons.iter().copied().map(SdkValue::text).collect()),
        ),
    ])
}

/// Conservative maximum of binary wire and JSON value sizes, including escaping.
pub fn value_bytes(value: &SdkValue) -> usize {
    match value {
        SdkValue::Null => 4,
        SdkValue::Bool(_) => 5,
        SdkValue::Int(n) => 9usize.max(n.to_string().len()),
        SdkValue::Text(s) => (5 + s.len()).max(string_bytes(s)),
        SdkValue::Bytes(b) => 5 + b.len(),
        SdkValue::List(items) => 5 + items.iter().map(|v| value_bytes(v) + 1).sum::<usize>(),
        SdkValue::Map(fields) => {
            5 + fields
                .iter()
                .map(|(key, v)| (2 + key.len()).max(string_bytes(key) + 2) + value_bytes(v))
                .sum::<usize>()
        }
    }
}

fn string_bytes(text: &str) -> usize {
    2 + text
        .chars()
        .map(|c| match c {
            '\"' | '\\' | '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' => 2,
            c if c < ' ' => 6,
            c => c.len_utf8(),
        })
        .sum::<usize>()
}
