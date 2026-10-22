//! A side with its changes in lost units reverted to the base (#352 F-057).
//!
//! The walk of base against the side binds, at every path of a lost unit,
//! what the base binds there, with the base's value; every other path keeps
//! the side's entry. The result is one backed update of the side over the
//! merge plan's records, on the merge's own route ([`super::build`]). A
//! base directory the side lacks is rebuilt from its base listing; one the
//! side moved elsewhere is rebuilt under a fresh serial, so no directory is
//! bound twice. A restored file the side binds elsewhere too keeps one
//! serial with the base's value and is counted as a cross link.

use layerfs_telemetry::timer::{Active, TimingScope};

use super::build::{self, Writer};
use super::decide::Resolver;
use super::rows::ChangeWriter;
use super::serial::built;
use super::spill::{Resident, Spill};
use super::units::{below, is_directory, Units};
use crate::compare::changes::{ChangeWalk, Pair, Plan};
use crate::compare::tree::same_value;
use crate::compare::BindingHints;
use crate::error::{ContentError, ContentResult};
use crate::filesystem::path::PathName;
use crate::filesystem::{FilesystemObjects, FilesystemRootId};
use crate::object::{AuthenticatedObjects, FinalizedConsumer};
use crate::policy::{ConstructionCapacities, ConstructionPolicy};
use crate::IndexedConstructionBacking;

/// The reverted side in the resolver, after the base.
const SIDE: usize = 1;

/// One side to revert.
pub struct RevertInput<'a> {
    /// Common ancestor root.
    pub base: FilesystemRootId,
    /// The side's root.
    pub side: FilesystemRootId,
    /// The request's units.
    pub units: &'a Units,
    /// Per unit, true when this side lost it.
    pub lost: &'a [bool],
    /// Recorded name bindings; without them the whole trees are walked.
    pub hints: Option<&'a dyn BindingHints>,
    /// Construction policy of the reverted root.
    pub policy: ConstructionPolicy,
    /// Construction capacities of the reverted root.
    pub capacities: &'a ConstructionCapacities,
}

/// The reverted root and, per unit, its cross links.
pub struct Reverted {
    /// The side's root with its lost units as in the base.
    pub root: FilesystemRootId,
    /// Restored paths whose serial the side also binds at another path.
    pub cross_links: Vec<u64>,
}

/// One directory of the result being written.
struct Frame {
    path: String,
    /// True when the directory's listing starts empty: a rebuilt directory.
    rebuilt: bool,
    out: ChangeWriter,
}

/// Reverts the side's changes in its lost units; objects of the new root go
/// to `consumer`, which `reader` must read back. `fresh` returns one
/// never-exposed serial of the roots' scope each call.
pub fn revert_units(
    reader: &dyn AuthenticatedObjects,
    consumer: &mut dyn FinalizedConsumer,
    input: &RevertInput<'_>,
    fresh: &mut dyn FnMut() -> ContentResult<u64>,
    records: Option<&mut dyn IndexedConstructionBacking>,
    timing: &TimingScope<'_, Active>,
) -> ContentResult<Reverted> {
    let mut cross_links = vec![0; input.units.len()];
    if input.base == input.side || !input.lost.iter().any(|lost| *lost) {
        return Ok(Reverted {
            root: input.side,
            cross_links,
        });
    }
    let resolver = Resolver::new(reader, [input.base, input.side, input.side])?;
    let [base, side, _] = &resolver.trees;
    let plan = match input.hints {
        Some(hints) => Plan::make(reader, hints, base, side)?,
        None => None,
    };
    let mut walk = match &plan {
        Some(plan) => ChangeWalk::planned(reader, base, side, plan, None)?,
        None => ChangeWalk::full(reader, base, side, None)?,
    };
    let mut resident = Resident::default();
    let spill = Spill::new(match records {
        Some(records) => records,
        None => &mut resident,
    });
    let mut changes = build::Plan::new(&spill);
    // Restored serials the side binds at another path: a cross link only
    // when the reverted tree binds them more often than the base does, not
    // when the other binding was a move inside the unit, reverted too.
    let mut moved: Vec<(usize, u64, u64)> = Vec::new();
    let mut frames = vec![Frame {
        path: "/".to_owned(),
        rebuilt: false,
        out: ChangeWriter::new(side.root_serial()),
    }];
    while let Some(pair) = walk.next_pair()? {
        while frames.len() > 1 && !below(&pair.0, &frames[frames.len() - 1].path) {
            close(&spill, &mut changes, frames.pop().expect("a frame"))?;
        }
        let (path, b, s) = &pair;
        let name = PathName::new(path.rsplit('/').next().unwrap_or_default())?;
        let parent = frames.last_mut().expect("the root frame");
        let current = if parent.rebuilt {
            None
        } else {
            s.map(|s| s.serial)
        };
        let lost = input.units.unit_of(path).filter(|unit| input.lost[*unit]);
        let Some(unit) = lost else {
            // Outside a lost unit: the side's entry, below which a lost
            // unit may still lie.
            let wanted = s.map(|s| s.serial);
            if wanted != current {
                parent.out.push(&spill, &name, wanted)?;
            }
            if let Some(s) = s.filter(|s| is_directory(Some(*s))) {
                frames.push(Frame {
                    path: path.clone(),
                    rebuilt: false,
                    out: ChangeWriter::new(s.serial),
                });
                walk.enter(&pair)?;
            }
            continue;
        };
        let Some(b) = b else {
            if current.is_some() {
                parent.out.push(&spill, &name, None)?;
            }
            continue;
        };
        let held = resolver.value(SIDE, b.serial)?;
        let here = s.is_some_and(|s| s.serial == b.serial);
        if !is_directory(Some(*b)) {
            if current != Some(b.serial) {
                parent.out.push(&spill, &name, Some(b.serial))?;
            }
            match held {
                None => {
                    changes.fresh(b.serial)?;
                    changes.value(b.serial, b.value)?;
                }
                Some(value) => {
                    let same = same_value(&value, &b.value);
                    if !same {
                        changes.value(b.serial, b.value)?;
                    }
                    if !here {
                        moved.push((unit, b.serial, b.value.namespace_ref_count));
                    } else if !same && value.namespace_ref_count > 1 {
                        cross_links[unit] += 1;
                    }
                }
            }
            continue;
        }
        let (serial, rebuilt) = match held {
            Some(value) if here => {
                if value.metadata_root != b.value.metadata_root {
                    changes.value(
                        b.serial,
                        crate::object::inode_leaf::InodeValue {
                            content_root: value.content_root,
                            ..b.value
                        },
                    )?;
                }
                (b.serial, false)
            }
            None => {
                changes.fresh(b.serial)?;
                changes.value(b.serial, built(b.value)?)?;
                (b.serial, true)
            }
            Some(_) => {
                // The side moved this directory elsewhere and keeps it there.
                let serial = fresh()?;
                if resolver.value(SIDE, serial)?.is_some() || changes.valued(serial)? {
                    return Err(ContentError::InvalidRecord("merge unit fresh serial"));
                }
                changes.fresh(serial)?;
                changes.value(serial, built(b.value)?)?;
                (serial, true)
            }
        };
        let parent = frames.last_mut().expect("the root frame");
        if current != Some(serial) {
            parent.out.push(&spill, &name, Some(serial))?;
        }
        frames.push(Frame {
            path: path.clone(),
            rebuilt,
            out: ChangeWriter::new(serial),
        });
        walk.enter(&pair as &Pair)?;
    }
    while let Some(frame) = frames.pop() {
        close(&spill, &mut changes, frame)?;
    }
    let mut objects = FilesystemObjects::new_with_accepted(reader, consumer, reader);
    let writer = Writer {
        policy: input.policy,
        capacities: input.capacities,
        fresh,
        timing,
    };
    let root = timing
        .child("merge.units")
        .run(|_| build::finish(&mut objects, &resolver, SIDE, changes, &writer))?;
    let reverted = crate::compare::tree::TreeTable::open(reader, root)?;
    let serials: Vec<u64> = moved.iter().map(|(_, serial, _)| *serial).collect();
    for ((unit, _, before), value) in moved.iter().zip(reverted.values(reader, &serials)?) {
        if value.is_some_and(|value| value.namespace_ref_count > *before) {
            cross_links[*unit] += 1;
        }
    }
    Ok(Reverted { root, cross_links })
}

/// Writes a directory's changed names; a rebuilt directory always has its
/// header, so the update builds its page even when it binds nothing.
fn close(spill: &Spill<'_>, changes: &mut build::Plan<'_, '_>, frame: Frame) -> ContentResult<()> {
    if frame.rebuilt || !frame.out.is_empty() {
        frame.out.finish(spill, &mut changes.counts)?;
    }
    Ok(())
}
