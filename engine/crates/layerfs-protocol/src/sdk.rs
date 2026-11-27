//! The SDK call channel: one named method with a self-describing argument
//! value, answered by a value or a typed refusal.
//!
//! The JavaScript SDK is the only public SDK and its arguments and results are
//! plain objects, so its calls travel as a small value tree instead of one
//! record type per method. The tree is bounded like every control record: at
//! most [`MAXIMUM_VALUE_DEPTH`] levels, and the whole record within the
//! control limit. A refusal carries a stable code (for example `NoSuchPath`),
//! a message for people and a detail value; the code alone decides.
use crate::{provision::install::ControlError, wire::{Reader, Writer}};

/// Deepest nesting of lists and maps.
pub const MAXIMUM_VALUE_DEPTH: usize = 16;
/// Longest method name or refusal code.
pub const MAXIMUM_NAME_BYTES: usize = 64;

/// One self-describing value.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum SdkValue {
    /// No value.
    #[default]
    Null,
    /// A boolean.
    Bool(bool),
    /// A signed integer.
    Int(i64),
    /// UTF-8 text.
    Text(String),
    /// Bytes.
    Bytes(Vec<u8>),
    /// An ordered list.
    List(Vec<SdkValue>),
    /// Named fields in order; names are unique.
    Map(Vec<(String, SdkValue)>),
}

/// One SDK call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SdkCall {
    /// Method, for example `branch.merge`.
    pub method: String,
    /// Arguments, normally a map.
    pub args: SdkValue,
}

/// A refused SDK call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SdkRefusal {
    /// Stable code, for example `NoSuchPath`.
    pub code: String,
    /// Message for people.
    pub message: String,
    /// Structured detail.
    pub detail: SdkValue,
}

impl SdkValue {
    /// A map from fields.
    pub fn map<const N: usize>(fields: [(&str, SdkValue); N]) -> Self {
        Self::Map(
            fields
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
        )
    }

    /// Text.
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    /// Optional text, `Null` when absent.
    pub fn opt_text(value: Option<impl Into<String>>) -> Self {
        value.map_or(Self::Null, |value| Self::Text(value.into()))
    }

    /// One field of a map.
    pub fn get(&self, name: &str) -> Option<&SdkValue> {
        match self {
            Self::Map(fields) => fields
                .iter()
                .find(|(field, _)| field == name)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// The text of a field.
    pub fn get_text(&self, name: &str) -> Option<&str> {
        match self.get(name) {
            Some(Self::Text(text)) => Some(text),
            _ => None,
        }
    }

    /// The boolean of a field; absent or null is false.
    pub fn get_bool(&self, name: &str) -> bool {
        matches!(self.get(name), Some(Self::Bool(true)))
    }

    /// The integer of a field.
    pub fn get_int(&self, name: &str) -> Option<i64> {
        match self.get(name) {
            Some(Self::Int(value)) => Some(*value),
            _ => None,
        }
    }
}

pub(crate) fn put_value(
    out: &mut Writer,
    value: &SdkValue,
    depth: usize,
) -> Result<(), ControlError> {
    if depth > MAXIMUM_VALUE_DEPTH {
        return Err(ControlError("SDK value depth"));
    }
    match value {
        SdkValue::Null => out.byte(0),
        SdkValue::Bool(value) => out.byte(if *value { 2 } else { 1 }),
        SdkValue::Int(value) => {
            out.byte(3)?;
            out.put(&value.to_be_bytes())
        }
        SdkValue::Text(value) => {
            out.byte(4)?;
            put_long(out, value.as_bytes())
        }
        SdkValue::Bytes(value) => {
            out.byte(5)?;
            put_long(out, value)
        }
        SdkValue::List(items) => {
            out.byte(6)?;
            out.put(&(items.len() as u32).to_be_bytes())?;
            items
                .iter()
                .try_for_each(|item| put_value(out, item, depth + 1))
        }
        SdkValue::Map(fields) => {
            out.byte(7)?;
            out.put(&(fields.len() as u32).to_be_bytes())?;
            for (name, value) in fields {
                if name.len() > MAXIMUM_NAME_BYTES {
                    return Err(ControlError("SDK field name"));
                }
                out.blob(name.as_bytes())?;
                put_value(out, value, depth + 1)?;
            }
            Ok(())
        }
    }
}

/// A length-prefixed run that may exceed one `u16` blob.
fn put_long(out: &mut Writer, bytes: &[u8]) -> Result<(), ControlError> {
    let size = u32::try_from(bytes.len()).map_err(|_| ControlError("SDK field limit"))?;
    out.put(&size.to_be_bytes())?;
    out.put(bytes)
}

fn long<'a>(input: &mut Reader<'a>) -> Result<&'a [u8], ControlError> {
    let size = u32::from_be_bytes(input.array()?) as usize;
    input.take(size)
}

pub(crate) fn value(input: &mut Reader<'_>, depth: usize) -> Result<SdkValue, ControlError> {
    if depth > MAXIMUM_VALUE_DEPTH {
        return Err(ControlError("SDK value depth"));
    }
    Ok(match input.byte()? {
        0 => SdkValue::Null,
        1 => SdkValue::Bool(false),
        2 => SdkValue::Bool(true),
        3 => SdkValue::Int(i64::from_be_bytes(input.array()?)),
        4 => SdkValue::Text(
            std::str::from_utf8(long(input)?)
                .map_err(|_| ControlError("control UTF-8"))?
                .to_owned(),
        ),
        5 => SdkValue::Bytes(long(input)?.to_vec()),
        6 => {
            let count = u32::from_be_bytes(input.array()?);
            let mut items = Vec::new();
            for _ in 0..count {
                items.push(value(input, depth + 1)?);
            }
            SdkValue::List(items)
        }
        7 => {
            let count = u32::from_be_bytes(input.array()?);
            let mut fields: Vec<(String, SdkValue)> = Vec::new();
            for _ in 0..count {
                let name = input.text(MAXIMUM_NAME_BYTES)?;
                if fields.iter().any(|(field, _)| *field == name) {
                    return Err(ControlError("SDK field repeated"));
                }
                let item = value(input, depth + 1)?;
                fields.push((name, item));
            }
            SdkValue::Map(fields)
        }
        _ => return Err(ControlError("SDK value kind")),
    })
}

pub(crate) fn put_call(out: &mut Writer, call: &SdkCall) -> Result<(), ControlError> {
    if call.method.is_empty() || call.method.len() > MAXIMUM_NAME_BYTES {
        return Err(ControlError("SDK method"));
    }
    out.blob(call.method.as_bytes())?;
    put_value(out, &call.args, 0)
}

pub(crate) fn call(input: &mut Reader<'_>) -> Result<SdkCall, ControlError> {
    let method = input.text(MAXIMUM_NAME_BYTES)?;
    if method.is_empty() {
        return Err(ControlError("SDK method"));
    }
    Ok(SdkCall {
        method,
        args: value(input, 0)?,
    })
}

pub(crate) fn put_refusal(out: &mut Writer, refusal: &SdkRefusal) -> Result<(), ControlError> {
    if refusal.code.is_empty() || refusal.code.len() > MAXIMUM_NAME_BYTES {
        return Err(ControlError("SDK refusal code"));
    }
    out.blob(refusal.code.as_bytes())?;
    put_long(out, refusal.message.as_bytes())?;
    put_value(out, &refusal.detail, 0)
}

pub(crate) fn refusal(input: &mut Reader<'_>) -> Result<SdkRefusal, ControlError> {
    let code = input.text(MAXIMUM_NAME_BYTES)?;
    if code.is_empty() {
        return Err(ControlError("SDK refusal code"));
    }
    let message = std::str::from_utf8(long(input)?)
        .map_err(|_| ControlError("control UTF-8"))?
        .to_owned();
    Ok(SdkRefusal {
        code,
        message,
        detail: value(input, 0)?,
    })
}

// Byte-bounded merge diagnostics; omitted details never change merge semantics.
use layerfs_content::merge::{Conflict, LwwDecision, Side, UnitReport};

// Separate windows leave room for source identities, outcome and observation.
const WINDOW: usize = 16 * 1024;

fn spend(remaining: &mut usize, cost: usize) -> bool {
    match remaining.checked_sub(cost) {
        Some(next) => {
            *remaining = next;
            true
        }
        None => false,
    }
}

fn conflict_window(values: &[Conflict]) -> Vec<usize> {
    let mut remaining = WINDOW;
    let mut hunks = Vec::new();
    for conflict in values {
        if !spend(&mut remaining, conflict.path.len().saturating_add(256)) {
            break;
        }
        let mut count = 0;
        for hunk in &conflict.hunks {
            let cost = hunk
                .ours
                .iter()
                .chain(&hunk.theirs)
                .fold(96usize, |sum, line| {
                    sum.saturating_add(line.len().saturating_add(8))
                });
            if !spend(&mut remaining, cost) {
                break;
            }
            count += 1;
        }
        hunks.push(count);
    }
    hunks
}

/// Whether the byte window omitted any conflict paths. Individual records also
/// report `hunksOmitted` when their full marker text did not fit.
pub fn conflicts_truncated(values: &[Conflict]) -> bool {
    conflict_window(values).len() != values.len()
}

/// Conflict paths and bounded whole hunks. `blocks` always reports the original
/// block count, and `hunksOmitted` explicitly identifies incomplete hunk text.
pub fn conflicts(values: &[Conflict]) -> SdkValue {
    let lines = |lines: &[String]| SdkValue::List(lines.iter().map(SdkValue::text).collect());
    SdkValue::List(
        values
            .iter()
            .zip(conflict_window(values))
            .map(|(conflict, count)| {
                SdkValue::map([
                    ("path", SdkValue::text(&conflict.path)),
                    ("kind", SdkValue::text(conflict.kind.as_str())),
                    ("blocks", SdkValue::Int(conflict.hunks.len() as i64)),
                    (
                        "hunksOmitted",
                        SdkValue::Bool(count != conflict.hunks.len()),
                    ),
                    (
                        "hunks",
                        SdkValue::List(
                            conflict.hunks[..count]
                                .iter()
                                .map(|hunk| {
                                    SdkValue::map([
                                        ("line", SdkValue::Int(hunk.line as i64)),
                                        ("ours", lines(&hunk.ours)),
                                        ("theirs", lines(&hunk.theirs)),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                ])
            })
            .collect(),
    )
}

fn lww_count(values: &[LwwDecision]) -> usize {
    let mut remaining = WINDOW;
    values
        .iter()
        .take_while(|decision| {
            spend(
                &mut remaining,
                decision
                    .path
                    .len()
                    .saturating_add(decision.rule.len())
                    .saturating_add(96),
            )
        })
        .count()
}

/// Whether the byte window omitted any generated-path decisions.
pub fn lww_truncated(values: &[LwwDecision]) -> bool {
    lww_count(values) != values.len()
}

/// `.lww` decisions in one byte window. SDK results pair this list with
/// `lwwTruncated`; this bounds diagnostics, never the merge's total work.
pub fn lww(values: &[LwwDecision]) -> SdkValue {
    SdkValue::List(
        values[..lww_count(values)]
            .iter()
            .map(|decision| {
                SdkValue::map([
                    ("path", SdkValue::text(&decision.path)),
                    (
                        "kept",
                        SdkValue::text(if decision.kept_ours { "ours" } else { "theirs" }),
                    ),
                    ("deleted", SdkValue::Bool(decision.deleted)),
                    ("rule", SdkValue::text(&decision.rule)),
                ])
            })
            .collect(),
    )
}

fn units_count(values: &[UnitReport]) -> usize {
    let mut remaining = WINDOW;
    values
        .iter()
        .take_while(|unit| spend(&mut remaining, unit.name.len().saturating_add(192)))
        .count()
}

/// Whether the byte window omitted any decided merge unit (#352 F-057).
pub fn units_truncated(values: &[UnitReport]) -> bool {
    units_count(values) != values.len()
}

/// Merge units both sides changed and how each was decided, in one byte
/// window. SDK results pair this list with `unitsTruncated`.
pub fn units(values: &[UnitReport]) -> SdkValue {
    let count = |n: u64| SdkValue::Int(i64::try_from(n).unwrap_or(i64::MAX));
    SdkValue::List(
        values[..units_count(values)]
            .iter()
            .map(|unit| {
                SdkValue::map([
                    ("unit", SdkValue::text(&unit.name)),
                    (
                        "kept",
                        SdkValue::text(match unit.kept {
                            Side::Ours => "ours",
                            Side::Theirs => "theirs",
                        }),
                    ),
                    ("keptTime", SdkValue::text(utc(unit.kept_time))),
                    ("changedOurs", count(unit.changed_ours)),
                    ("changedTheirs", count(unit.changed_theirs)),
                    ("dropped", count(unit.dropped)),
                    ("crossLinks", count(unit.cross_links)),
                ])
            })
            .collect(),
    )
}

/// RFC 3339 UTC text of a modification time, with nanoseconds.
fn utc((seconds, nanoseconds): (i64, u32)) -> String {
    let (days, second) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (proleptic Gregorian).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{nanoseconds:09}Z",
        second / 3600,
        second / 60 % 60,
        second % 60
    )
}

// Values the SDK shows for history records and merge results, shared by the
// daemon's SDK calls and the internal API so both encode them one way.
//
// Commits and Layers travel as the lowercase hex of their tagged identity.
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

/// A short label for marker lines: the tag and the six bytes after it, the
/// first 14 characters of the identity, so the label is a prefix that
/// `commit.get` and every `--at` accept.
pub fn short(node: NodeId) -> String {
    hex(&node.as_slice()[..7])
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

