//! Merge units of a Branch merge (#352 F-057).
//!
//! Each side's changes from the base are classified per unit, then a side
//! that lost a unit both sides changed has its changes there reverted to
//! the base, in the merge's Save and its own merge records. The ordinary
//! merge then runs on the reverted roots, so a decided unit comes whole from
//! the side with the later write and nothing inside it conflicts.
use super::lookup::{self as args, history};
use crate::store::{CatalogHints, Store};
use layerfs_bridge::control::SdkRefusal;
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_content::merge::{
    decide_units, revert_units, unit_changes, RevertInput, Side, UnitReport, Units,
};
use layerfs_content::{ConstructionCapacities, ConstructionPolicy, ContentResult};
use layerfs_storage::Save;
use layerfs_telemetry::timer::{Active, TimingScope};

/// The merge's sides with lost units reverted, and the decided units.
pub(crate) struct Decided {
    pub(crate) ours: FilesystemRootId,
    pub(crate) theirs: FilesystemRootId,
    pub(crate) reports: Vec<UnitReport>,
}

/// One merge's inputs to its units.
pub(crate) struct Sides<'a> {
    pub(crate) base: FilesystemRootId,
    pub(crate) ours: FilesystemRootId,
    pub(crate) theirs: FilesystemRootId,
    pub(crate) policy: ConstructionPolicy,
    pub(crate) capacities: &'a ConstructionCapacities,
}

/// Classifies and decides the units, then reverts each losing side.
pub(crate) fn decide(
    store: &Store,
    save: &Save<'_>,
    hints: &CatalogHints<'_>,
    units: &Units,
    sides: &Sides<'_>,
    fresh: &mut dyn FnMut() -> ContentResult<u64>,
    timing: &TimingScope<'_, Active>,
) -> Result<Decided, SdkRefusal> {
    let content = |error| match hints.failure.take() {
        Some(failure) => history(failure),
        None => args::content(error),
    };
    // Ours first: when ours changed no unit, no unit can be two-sided, so
    // theirs is not classified at all.
    let (ours, theirs) = timing.child("units.classify").run(|_| {
        let ours =
            unit_changes(save, Some(hints), sides.base, sides.ours, units).map_err(content)?;
        let theirs = if ours.iter().any(|unit| unit.changed > 0) {
            unit_changes(save, Some(hints), sides.base, sides.theirs, units).map_err(content)?
        } else {
            vec![Default::default(); units.len()]
        };
        Ok::<_, SdkRefusal>((ours, theirs))
    })?;
    let mut reports = decide_units(units, &ours, &theirs);
    let mut roots = [sides.ours, sides.theirs];
    for (index, this) in [Side::Ours, Side::Theirs].into_iter().enumerate() {
        let mut lost = vec![false; units.len()];
        for report in &reports {
            lost[report.unit] = report.kept != this;
        }
        if !lost.contains(&true) {
            continue;
        }
        let input = RevertInput {
            base: sides.base,
            side: roots[index],
            units,
            lost: &lost,
            hints: Some(hints),
            policy: sides.policy,
            capacities: sides.capacities,
        };
        let (reverted, scratch) = timing
            .child("units.revert")
            .run(|timing| {
                store.merge_records(|records| {
                    revert_units(save, &mut save.sink(), &input, fresh, Some(records), timing)
                })
            })
            .map_err(args::failed)?;
        let reverted = reverted.map_err(|error| match (hints.failure.take(), scratch) {
            (Some(failure), _) => history(failure),
            (None, Some(failure)) => args::failed(failure),
            (None, None) => args::content(error),
        })?;
        for report in &mut reports {
            report.cross_links += reverted.cross_links[report.unit];
        }
        roots[index] = reverted.root;
    }
    Ok(Decided {
        ours: roots[0],
        theirs: roots[1],
        reports,
    })
}
