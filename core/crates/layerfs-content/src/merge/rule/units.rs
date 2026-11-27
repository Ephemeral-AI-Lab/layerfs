//! Merge units (#352 F-057): groups of paths a Branch merge takes whole
//! from one side.
//!
//! A unit is a list of patterns in the `/.lww` syntax; it covers every path
//! one of them matches and everything below those paths, and a path two
//! units match belongs to the first one listed. Per unit, each side's
//! changes from the base are classified ([`unit_changes`]): how many paths
//! the side changed in it and its latest write there. A unit both sides
//! changed is taken whole from the side with the later write, ours on a tie
//! ([`decide_units`]); the losing side's changes in it are reverted to the
//! base before the ordinary merge runs ([`super::revert::revert_units`]),
//! so inside a decided unit no text merge, conflict or `/.lww` rule applies.
//! The state is a count and a time per unit and side, never a path list.

use super::super::Side;
use super::lww::Rules;
use crate::compare::bytes::same_content;
use crate::compare::changes::{ChangeWalk, Pair, Plan};
use crate::compare::tree::{same_metadata, same_value, EntryFacts, TreeTable};
use crate::compare::BindingHints;
use crate::error::ContentResult;
use crate::filesystem::tree::attribute::{read_portable, AttributeReadWork};
use crate::filesystem::FilesystemRootId;
use crate::object::inode_leaf::InodeKind;
use crate::object::AuthenticatedObjects;
use layerfs_telemetry::timer::Timing;

/// One unit as requested: its name and patterns.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergeUnit {
    /// The unit's name; a bare pattern names itself.
    pub name: String,
    /// Patterns in the `/.lww` syntax, in order.
    pub patterns: Vec<String>,
}

/// The admitted units of one merge.
#[derive(Clone, Debug, Default)]
pub struct Units {
    units: Vec<(String, Rules)>,
}

/// Why a pattern cannot be a unit pattern; `None` when it can.
fn malformed(pattern: &str) -> Option<&'static str> {
    let body = pattern.strip_prefix('!').unwrap_or(pattern);
    let body = body.strip_suffix('/').unwrap_or(body);
    let body = body.strip_prefix('/').unwrap_or(body);
    if body.is_empty() {
        Some("an empty pattern")
    } else if pattern.starts_with('#') {
        Some("a comment, not a pattern")
    } else if body.contains("//") {
        Some("an empty path component")
    } else if pattern.chars().any(char::is_control) {
        Some("a control character")
    } else {
        None
    }
}

impl Units {
    /// Admits the units; the error names the refused unit or pattern.
    pub fn new(units: &[MergeUnit]) -> Result<Self, String> {
        let mut admitted: Vec<(String, Rules)> = Vec::with_capacity(units.len());
        for unit in units {
            if unit.name.is_empty() {
                return Err("a unit name is empty".to_owned());
            }
            if admitted.iter().any(|(name, _)| *name == unit.name) {
                return Err(format!("unit {:?} is listed twice", unit.name));
            }
            if !unit
                .patterns
                .iter()
                .any(|pattern| !pattern.starts_with('!'))
            {
                return Err(format!("unit {:?} has no pattern to match", unit.name));
            }
            for pattern in &unit.patterns {
                if let Some(why) = malformed(pattern) {
                    return Err(format!(
                        "unit {:?} pattern {pattern:?} is malformed: {why}",
                        unit.name
                    ));
                }
            }
            admitted.push((unit.name.clone(), Rules::of(&unit.patterns)));
        }
        Ok(Self { units: admitted })
    }

    /// True when no unit is listed.
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// Number of units.
    pub fn len(&self) -> usize {
        self.units.len()
    }

    /// A unit's name.
    pub fn name(&self, unit: usize) -> &str {
        &self.units[unit].0
    }

    /// The first listed unit covering `path`: one of its patterns matches
    /// the path or one of its parent directories. The root is in none.
    pub fn unit_of(&self, path: &str) -> Option<usize> {
        if path == "/" {
            return None;
        }
        self.units
            .iter()
            .position(|(_, rules)| rules.matching(path).is_some())
    }
}

/// One side's changes in one unit.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UnitChange {
    /// Paths the side changed in the unit.
    pub changed: u64,
    /// The latest modification time among them, `(seconds, nanoseconds)`.
    pub latest: Option<(i64, u32)>,
}

/// One unit both sides changed, and how it was decided.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnitReport {
    /// The unit's index in the request.
    pub unit: usize,
    /// The unit's name.
    pub name: String,
    /// The side the unit was taken from.
    pub kept: Side,
    /// The kept side's latest write in the unit.
    pub kept_time: (i64, u32),
    /// Paths ours changed in the unit.
    pub changed_ours: u64,
    /// Paths theirs changed in the unit.
    pub changed_theirs: u64,
    /// The losing side's changed paths, discarded.
    pub dropped: u64,
    /// Restored paths whose serial the losing side also binds elsewhere.
    pub cross_links: u64,
}

/// A side's changes from `base` to `side` per unit, in unit order.
///
/// A path is changed when it was added or deleted, or its kind, content
/// (for a file or symlink) or metadata differs; a new serial with the same
/// kind, content and metadata is not a change, so a relink is none. A
/// deletion's time is its nearest surviving parent's on that side.
pub fn unit_changes(
    reader: &dyn AuthenticatedObjects,
    hints: Option<&dyn BindingHints>,
    base: FilesystemRootId,
    side: FilesystemRootId,
    units: &Units,
) -> ContentResult<Vec<UnitChange>> {
    let mut counts = vec![UnitChange::default(); units.len()];
    if units.is_empty() || base == side {
        return Ok(counts);
    }
    let left = TreeTable::open(reader, base)?;
    let right = TreeTable::open(reader, side)?;
    let plan = match hints {
        Some(hints) => Plan::make(reader, hints, &left, &right)?,
        None => None,
    };
    let mut walk = match &plan {
        Some(plan) => ChangeWalk::planned(reader, &left, &right, plan, None)?,
        None => ChangeWalk::full(reader, &left, &right, None)?,
    };
    let root = right.root_serial();
    let root = right.value(reader, root)?.map(|value| EntryFacts {
        serial: root,
        value,
    });
    // Entered directories on the current path, with the side's entry.
    let mut frames: Vec<(String, Option<EntryFacts>)> = vec![("/".to_owned(), root)];
    let mut work = AttributeReadWork::default();
    while let Some(pair) = walk.next_pair()? {
        let (path, b, s) = &pair;
        while frames.len() > 1 && !below(path, &frames[frames.len() - 1].0) {
            frames.pop();
        }
        if let Some(unit) = units.unit_of(path) {
            if changed(reader, *b, *s, &mut work)? {
                let entry = match s {
                    Some(entry) => Some(*entry),
                    None => frames.iter().rev().find_map(|(_, entry)| *entry),
                };
                let count = &mut counts[unit];
                count.changed += 1;
                if let Some(entry) = entry {
                    let time = mtime(reader, entry, &mut work)?;
                    count.latest = Some(count.latest.map_or(time, |latest| latest.max(time)));
                }
            }
        }
        if is_directory(*b) || is_directory(*s) {
            frames.push((path.clone(), s.filter(|entry| is_directory(Some(*entry)))));
            walk.enter(&pair as &Pair)?;
        }
    }
    Ok(counts)
}

/// The units both sides changed, each kept from the side with the later
/// write, ours on a tie.
pub fn decide_units(units: &Units, ours: &[UnitChange], theirs: &[UnitChange]) -> Vec<UnitReport> {
    let mut reports = Vec::new();
    for (unit, (o, t)) in ours.iter().zip(theirs).enumerate() {
        if o.changed == 0 || t.changed == 0 {
            continue;
        }
        let (ot, tt) = (o.latest.unwrap_or_default(), t.latest.unwrap_or_default());
        let kept_ours = ot >= tt;
        reports.push(UnitReport {
            unit,
            name: units.name(unit).to_owned(),
            kept: if kept_ours { Side::Ours } else { Side::Theirs },
            kept_time: if kept_ours { ot } else { tt },
            changed_ours: o.changed,
            changed_theirs: t.changed,
            dropped: if kept_ours { t.changed } else { o.changed },
            cross_links: 0,
        });
    }
    reports
}

/// True when `path` lies strictly below directory `parent`.
pub(super) fn below(path: &str, parent: &str) -> bool {
    parent == "/"
        || path
            .strip_prefix(parent)
            .is_some_and(|rest| rest.starts_with('/'))
}

pub(super) fn is_directory(entry: Option<EntryFacts>) -> bool {
    entry.is_some_and(|entry| entry.value.kind == InodeKind::Directory)
}

/// True when the entry was added, deleted or changed in kind, content or
/// metadata; a directory's content is its children, which count themselves.
fn changed(
    reader: &dyn AuthenticatedObjects,
    b: Option<EntryFacts>,
    s: Option<EntryFacts>,
    work: &mut AttributeReadWork,
) -> ContentResult<bool> {
    let (b, s) = match (b, s) {
        (None, None) => return Ok(false),
        (Some(b), Some(s)) => (b.value, s.value),
        _ => return Ok(true),
    };
    if same_value(&b, &s) {
        return Ok(false);
    }
    if b.kind != s.kind {
        return Ok(true);
    }
    // A file written away and back keeps its bytes under a new root (#352 F-073).
    if b.kind != InodeKind::Directory
        && !Timing::disabled("merge.unit", |scope| {
            same_content(reader, &b, &s, u64::MAX, scope.child("compare"))
        })
        .0?
    {
        return Ok(true);
    }
    Ok(!same_metadata(
        reader,
        b.kind,
        b.metadata_root,
        s.metadata_root,
        work,
    )?)
}

fn mtime(
    reader: &dyn AuthenticatedObjects,
    entry: EntryFacts,
    work: &mut AttributeReadWork,
) -> ContentResult<(i64, u32)> {
    let metadata = read_portable(reader, entry.value.metadata_root, entry.value.kind, work)?;
    Ok((metadata.mtime_seconds, metadata.mtime_nanoseconds))
}
