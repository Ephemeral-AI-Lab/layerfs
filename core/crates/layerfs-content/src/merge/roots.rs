//! The merge entry point: the merge by serial ([`super::serial`]) and its
//! merged root.

use layerfs_telemetry::timer::{Active, TimingScope};

use super::plan::spill::{Resident, Spill};
use super::{
    decide::entries, plan::build, serial::walk, MergeInput, MergeOutcome, Rules, RulesFrom, Side,
};
use crate::compare::DifferenceWork;
use crate::error::{ContentError, ContentResult};
use crate::filesystem::FilesystemObjects;
use crate::object::{AuthenticatedObjects, FinalizedConsumer};
use crate::IndexedConstructionBacking;

/// Merges ours into theirs against the base.
///
/// Merged and marker files are constructed into `consumer`, followed by the
/// merged root's own objects; `reader` must read what `consumer` accepted.
/// `fresh` returns one never-exposed serial of the roots' scope each call.
/// `records` holds the merge's working state: its decisions, corrections,
/// plan and the update's validation and reference state (#355). The caller owns their
/// scope and releases it after this call returns. Without records the same
/// state is kept in memory; a caller passes none only with a `bound`.
pub fn merge_roots(
    reader: &dyn AuthenticatedObjects,
    consumer: &mut dyn FinalizedConsumer,
    input: &MergeInput<'_>,
    fresh: &mut dyn FnMut() -> ContentResult<u64>,
    records: Option<&mut dyn IndexedConstructionBacking>,
    timing: &TimingScope<'_, Active>,
) -> ContentResult<MergeOutcome> {
    let mut outcome = MergeOutcome {
        root: input.theirs,
        conflicts: Vec::new(),
        lww: Vec::new(),
        decided: 0,
        work: DifferenceWork::default(),
        hinted: false,
        naming: Default::default(),
    };
    if input.ours == input.base || input.ours == input.theirs {
        return Ok(outcome);
    }
    if input.theirs == input.base {
        outcome.root = input.ours;
        return Ok(outcome);
    }
    let mut resolver = entries::Resolver::new(reader, [input.base, input.ours, input.theirs])?;
    if resolver.trees[entries::OURS].root.scope() != resolver.trees[entries::THEIRS].root.scope() {
        return Err(ContentError::InvalidRecord("merge scope"));
    }
    let (from, into) = match input.decide_from {
        Side::Ours => (entries::OURS, entries::THEIRS),
        Side::Theirs => (entries::THEIRS, entries::OURS),
    };
    let mut resident = Resident::default();
    let spill = Spill::new(match records {
        Some(records) => records,
        None => &mut resident,
    });
    let rules_side = match input.rules_from {
        RulesFrom::Ours => entries::OURS,
        RulesFrom::Theirs => entries::THEIRS,
    };
    let mut decider = entries::Decider {
        labels: input.labels,
        rules: &Rules::default(),
        timing,
        conflicts: Vec::new(),
        lww: Vec::new(),
        // A bounded merge is the lane's (#355): its hold must not grow with
        // a file's size, so larger files are compared by root there (F-073).
        compared: match input.bound {
            Some(_) => crate::compare::MAXIMUM_TEXT_BYTES,
            None => u64::MAX,
        },
    };
    let rules = Rules::load(&mut |path| {
        let entry = resolver.entry(rules_side, path)?;
        decider.text(reader, entry)
    })?;
    decider.rules = &rules;
    let mut objects = FilesystemObjects::new_with_accepted(reader, consumer, reader);
    let mut ctx = walk::Ctx {
        reader,
        resolver,
        from,
        into,
        spill: &spill,
        decider,
        hints: input.hints,
        naming: Default::default(),
        hinted: true,
        work: DifferenceWork::default(),
        plan: build::Plan::new(&spill),
        writer: build::Writer {
            policy: input.policy,
            capacities: input.capacities,
            fresh,
            timing,
        },
        objects: &mut objects,
        paths: Default::default(),
        decided: 0,
        bound: input.bound,
    };
    ctx.run(timing)?;
    let walk::Ctx {
        resolver,
        mut decider,
        plan,
        writer,
        objects,
        naming,
        hinted,
        work,
        decided,
        ..
    } = ctx;
    outcome.root = build::finish(objects, &resolver, into, plan, &writer)?;
    decider.conflicts.sort_by(|x, y| x.path.cmp(&y.path));
    decider.lww.sort_by(|x, y| x.path.cmp(&y.path));
    outcome.conflicts = decider.conflicts;
    outcome.lww = decider.lww;
    outcome.decided = decided;
    outcome.work = work;
    outcome.naming = naming;
    outcome.hinted = hinted && input.hints.is_some();
    Ok(outcome)
}
