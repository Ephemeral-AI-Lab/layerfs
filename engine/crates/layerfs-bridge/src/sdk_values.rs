//! Values the SDK shows for history records and merge results, shared by the
//! daemon's SDK calls and the internal API so both encode them one way.
//!
//! Commits and Layers travel as the lowercase hex of their tagged identity.
use crate::control_sdk::SdkValue;
use layerfs_history::{CommitRecord, CommitVia, LayerRecord, NodeId};

/// Lowercase hex of bytes.
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                DIGITS[usize::from(byte >> 4)],
                DIGITS[usize::from(byte & 15)],
            ]
        })
        .map(char::from)
        .collect()
}

/// Bytes of lowercase or uppercase hex.
pub fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(text.get(at..at + 2)?, 16).ok())
        .collect()
}

/// A node's identity as text.
pub fn node_text(node: NodeId) -> String {
    hex(node.as_slice())
}

/// A short label for marker lines: the first six bytes after the tag.
pub fn short(node: NodeId) -> String {
    hex(&node.as_slice()[1..7])
}

/// How a Commit was made, as the SDK names it.
pub fn via(via: CommitVia) -> &'static str {
    match via {
        CommitVia::Workspace => "exec",
        CommitVia::Files => "fs",
        CommitVia::Merge => "merge",
    }
}

/// A Commit as the SDK shows it.
pub fn commit_value(record: &CommitRecord, branch: Option<&str>) -> SdkValue {
    let meta = &record.meta;
    let mut bytes = 0usize;
    let count = meta
        .lww
        .iter()
        .take_while(|fact| {
            bytes += fact.path.len() + fact.rule.len() + 96;
            bytes <= 4096
        })
        .count();
    let mut value = SdkValue::map([
        ("id", SdkValue::text(hex(record.id.as_slice()))),
        ("branch", SdkValue::opt_text(branch)),
        (
            "parent",
            SdkValue::opt_text(record.parent.map(|id| hex(id.as_slice()))),
        ),
        (
            "secondParent",
            SdkValue::opt_text(record.second_parent.map(node_text)),
        ),
        ("base", SdkValue::text(hex(record.base_layer.as_slice()))),
        ("root", SdkValue::text(hex(record.root.as_bytes()))),
        ("via", SdkValue::text(via(meta.via))),
        ("message", SdkValue::opt_text(meta.message.clone())),
        (
            "mergedFrom",
            SdkValue::opt_text(meta.merged_from.map(|id| hex(id.as_slice()))),
        ),
        ("time", SdkValue::Int(meta.time_ms)),
        ("logical", SdkValue::Int(meta.statistics.logical as i64)),
        (
            "newCanonical",
            SdkValue::Int(meta.statistics.new_canonical as i64),
        ),
        (
            "newStored",
            SdkValue::Int(meta.statistics.new_stored as i64),
        ),
        (
            "newMetadata",
            SdkValue::Int(meta.statistics.new_metadata as i64),
        ),
        (
            "displaced",
            SdkValue::opt_text(meta.displaced.map(|id| hex(id.as_slice()))),
        ),
        ("lww", lww(&meta.lww[..count])),
        (
            "lwwTruncated",
            SdkValue::Bool(meta.lww_truncated || meta.lww.len() > count),
        ),
    ]);
    // The merge units a Branch merge ran with (#352 F-057), only when given.
    if let (false, SdkValue::Map(fields)) = (meta.units.is_empty(), &mut value) {
        let units = meta.units.iter().map(|unit| {
            SdkValue::map([
                ("name", SdkValue::text(&unit.name)),
                (
                    "patterns",
                    SdkValue::List(unit.patterns.iter().map(SdkValue::text).collect()),
                ),
            ])
        });
        fields.push(("units".to_owned(), SdkValue::List(units.collect())));
    }
    value
}

/// A Layer as the SDK shows it.
pub fn layer_value(record: &LayerRecord, source_branch: Option<&str>) -> SdkValue {
    SdkValue::map([
        (
            "overwroteCount",
            SdkValue::Int(record.overwrote_count as i64),
        ),
        ("logical", SdkValue::Int(record.logical as i64)),
        ("id", SdkValue::text(hex(record.id.as_slice()))),
        (
            "parent",
            SdkValue::opt_text(record.parent.map(|id| hex(id.as_slice()))),
        ),
        ("root", SdkValue::text(hex(record.root.as_bytes()))),
        ("sourceBranch", SdkValue::opt_text(source_branch)),
        (
            "sourceCommit",
            SdkValue::opt_text(record.source_commit.map(|id| hex(id.as_slice()))),
        ),
    ])
}

pub use crate::sdk_merge_values::{
    conflicts, conflicts_truncated, lww, lww_truncated, units, units_truncated,
};
