// v0.1.6 mixed workload (schedule M1) operation plan.
//
// One implementation of the five-stage schedule that both the Linux workload
// helper and the host orchestrator read, so the exact operation counts in
// `docs/roadmap/0.1/0.1.6/workloads.md` have a single executable counterpart.
// The product-free functions here choose paths and content recipes only; they
// never touch a Store, a Client or a FUSE mount.
use super::workspace_common::Content;
use super::v016_common::{self as v016, LoadFixture, RefreshClass};
use super::{dedup_workloads as d, Result};
use std::collections::BTreeMap;

pub(crate) const STAGES: usize = 5;

/// The five declared M1 stage ordinals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Stage {
    Refresh = 1,
    Edits = 2,
    Directories = 3,
    AttrsLinks = 4,
    InodeSubtree = 5,
}

impl Stage {
    pub(crate) fn from_ordinal(ordinal: usize) -> Result<Self> {
        Ok(match ordinal {
            1 => Self::Refresh,
            2 => Self::Edits,
            3 => Self::Directories,
            4 => Self::AttrsLinks,
            5 => Self::InodeSubtree,
            other => return Err(format!("v0.1.6 M1 stage ordinal {other} outside 1..=5").into()),
        })
    }
}

/// Stage-2 main edit operations, one per target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Stage2Op {
    TinyPwrite { ordinal: usize },
    MediumMidpoint,
    MediumNetZero,
    MediumReplaceTail,
    SdkInsert,
    SdkDelete,
    AnchorOverwrite,
    BoundaryShrink,
    BoundaryGrow,
}

pub(crate) const STAGE2_POSIX_TARGETS: usize = 14;
pub(crate) const STAGE2_SDK_CALLS: usize = 2;
pub(crate) const STAGE2_MAIN_TARGETS: usize = 16;
pub(crate) const ALIAS_PROBE_WRITES: usize = 4;
pub(crate) const ALIAS_PROBE_BYTES: u64 = 64;
pub(crate) const SUBTREE_RECURRENT: usize = 16;

/// The tiny-file pwrite positions rotate 0, 1920, 3840 by cycle.
pub(crate) fn tiny_pwrite_offset(cycle: usize) -> u64 {
    ((cycle - 1) % 3) as u64 * 1_920
}

/// The anchor overwrite offset rotates 0, aligned midpoint, len-4096.
pub(crate) fn anchor_offset(cycle: usize, len: u64) -> u64 {
    match (cycle - 1) % 3 {
        0 => 0,
        1 => ((len / 2) / 8) * 8,
        _ => len - 4_096,
    }
}

/// The two length-changing SDK calls: odd cycles insert on the first 1 MiB file
/// and delete on the second, even cycles invert. Aggregate committed growth is
/// zero because both move 256 bytes.
pub(crate) fn sdk_pair(cycle: usize) -> (&'static str, &'static str) {
    if cycle % 2 == 1 {
        ("insert", "delete")
    } else {
        ("delete", "insert")
    }
}

/// Declared boundary lengths for one cycle. Odd cycles shrink below/above to
/// the exact 131,072 B control length; even cycles restore 131,071/131,073.
pub(crate) fn boundary_lengths(cycle: usize) -> (u64, u64) {
    if cycle % 2 == 1 {
        (v016::BOUNDARY_EXACT, v016::BOUNDARY_EXACT)
    } else {
        (v016::BOUNDARY_BELOW, v016::BOUNDARY_ABOVE)
    }
}

/// The fixed 256-byte region a local refresh variant changes.
pub(crate) const LOCAL_VARIANT_OFFSET: u64 = 1_024;
pub(crate) const LOCAL_VARIANT_LEN: u64 = 256;

/// Stage-1 slot classes: indices 0/1 of every group of four are recurring, 2 is
/// a local variant and 3 is unique.
pub(crate) fn stage1_slot_role(slot: usize) -> RefreshClass {
    v016::refresh_class(slot)
}

/// Base content of one refresh slot, before the cycle's value is applied.
pub(crate) fn refresh_base(fixture: &LoadFixture, cohort: usize, slot: usize) -> Result<Content> {
    if cohort >= v016::REFRESH_COHORTS || slot >= v016::REFRESH_FILES_PER_COHORT {
        return Err("v0.1.6 refresh cohort/slot outside the pool".into());
    }
    let index = refresh_pool_index(cohort, slot);
    d::content(
        v016::FAMILY_MIXED,
        "v016-refresh",
        fixture.seed,
        index,
        &format!("base-c{cohort}"),
        v016::TINY,
    )
}

/// Stage-1 content for one slot in one cycle. `prior_visits` is the cohort's
/// visit count before this cycle; a historical fork inherits it from the trunk
/// so a K10 run stays the exact prefix of K100.
pub(crate) fn stage1_content(
    fixture: &LoadFixture,
    cycle: usize,
    cohort: usize,
    slot: usize,
    prior_visits: usize,
    branch_salt: &str,
) -> Result<Content> {
    let index = refresh_pool_index(cohort, slot);
    Ok(match stage1_slot_role(slot) {
        RefreshClass::Recurring => {
            // Initial A, first visit B, second A. The recurrent bytes omit the
            // cycle and the branch salt, so the pair is exact across branches.
            let value = v016::recurrent_value(prior_visits);
            d::content(
                v016::FAMILY_MIXED,
                "v016-refresh",
                fixture.seed,
                index,
                if value == 0 { "recurring-a" } else { "recurring-b" },
                v016::TINY,
            )?
        }
        RefreshClass::Local => {
            let value = v016::recurrent_value(prior_visits);
            refresh_base(fixture, cohort, slot)?.splice(
                LOCAL_VARIANT_OFFSET,
                LOCAL_VARIANT_LEN,
                d::content(
                    v016::FAMILY_MIXED,
                    "v016-refresh",
                    fixture.seed,
                    index,
                    &format!("local-{}-c{cycle}", if value == 0 { "a" } else { "b" }),
                    LOCAL_VARIANT_LEN,
                )?,
            )?
        }
        RefreshClass::Unique => d::content(
            v016::FAMILY_MIXED,
            "v016-refresh",
            fixture.seed,
            index,
            &format!("unique-c{cycle}-{branch_salt}"),
            v016::TINY,
        )?,
    })
}

/// Cycle `c` addresses a cohort; cohorts are disjoint 64-file pool segments.
pub(crate) fn refresh_pool_index(cohort: usize, slot: usize) -> usize {
    cohort * v016::REFRESH_FILES_PER_COHORT + slot
}

/// Cohorts' visit counts before `cycle`, so a child branch inherits the trunk's
/// recurrence parity through its first executed cycle.
pub(crate) fn prior_visits(cycle: usize, cohort: usize) -> Result<usize> {
    let visits = v016::cohort_visit_counts(cycle - 1)?;
    visits
        .get(cohort)
        .copied()
        .ok_or_else(|| "v0.1.6 cohort ordinal".into())
}

/// The four alias-probe paths: the first four canonical hardlink targets.
pub(crate) fn alias_probe_paths(fixture: &LoadFixture) -> Vec<String> {
    fixture.roles.links.alias_writes.clone()
}

/// The four atomic-save destinations: the second half of the hardlink targets,
/// so the stage-5 replacement exercises different inodes than the stage-4
/// aliases.
pub(crate) fn save_destinations(fixture: &LoadFixture) -> Vec<String> {
    fixture.roles.link_targets.iter().skip(4).cloned().collect()
}

/// Content of one recreated deletion-subtree entry.
pub(crate) fn subtree_content(
    fixture: &LoadFixture,
    cycle: usize,
    index: usize,
    branch_salt: &str,
) -> Result<Content> {
    if index >= v016::DELETION_SUBTREE_FILES {
        return Err("v0.1.6 deletion subtree index".into());
    }
    if index < SUBTREE_RECURRENT {
        // Initial A, first visit B, second A, by subtree ordinal parity.
        let value = v016::recurrent_value(index % 2);
        d::content(
            v016::FAMILY_MIXED,
            "v016-subtree",
            fixture.seed,
            index,
            if value == 0 { "recurring-a" } else { "recurring-b" },
            v016::TINY,
        )
    } else {
        // Generation-specific contents include the cycle and the branch tag, so
        // a delete/recreate-identical no-op cannot collapse history.
        d::content(
            v016::FAMILY_MIXED,
            "v016-subtree",
            fixture.seed,
            index,
            &format!("generation-c{cycle}-{branch_salt}"),
            v016::TINY,
        )
    }
}

/// The four chmod'd directories of stage 4. The two movable subtree roots are
/// named at their post-move locations, but only while their parent still holds
/// them: the even cycles name them at their original parents again.
/// The four chmod'd directories of stage 4, in declared order. Every target is
/// a directory this cycle's stage 3 created or recreated, plus the two
/// destination parents that hold the moved subtrees and the deletion-subtree
/// root. The registered/alternate scratch naming means the concrete names
/// follow the cycle.
pub(crate) fn attribute_directories(fixture: &LoadFixture, cycle: usize) -> Vec<String> {
    let (_, create) = scratch_names(fixture, cycle);
    let scratch = |index: usize| -> String {
        create
            .get(index)
            .cloned()
            .unwrap_or_else(|| fixture.roles.scratch[index].clone())
    };
    vec![
        fixture.roles.deletion_root.clone(),
        scratch(0),
        fixture.roles.move_dest[0].clone(),
        fixture.roles.move_dest[1].clone(),
    ]
}

/// Symlink `(path, target, kind)`, in declared order.
pub(crate) fn symlink_rows(fixture: &LoadFixture) -> Vec<(String, String, v016::SymlinkKind)> {
    fixture.roles.links.symlinks.clone()
}

/// The exact `workloads.md` per-cycle operation totals.
pub(crate) fn declared_cycle_counters() -> BTreeMap<&'static str, usize> {
    BTreeMap::from([
        ("created_commits", v016::CYCLE.created_commits),
        ("regular_unlinks", v016::CYCLE.regular_unlinks),
        ("ordinary_regular_creates", v016::CYCLE.ordinary_regular_creates),
        ("temporary_regular_creates", v016::CYCLE.temporary_regular_creates),
        ("rename_overwrites", v016::CYCLE.rename_overwrites),
        ("main_edit_targets", v016::CYCLE.main_edit_targets),
        ("sdk_edit_calls", v016::CYCLE.sdk_edit_calls),
        ("sdk_edit_members", v016::CYCLE.sdk_edit_members),
        ("hardlink_creates", v016::CYCLE.hardlink_creates),
        ("hardlink_unlinks", v016::CYCLE.hardlink_unlinks),
        ("symlink_creates", v016::CYCLE.symlink_creates),
        ("symlink_unlinks", v016::CYCLE.symlink_unlinks),
        ("alias_probe_writes", v016::CYCLE.alias_probe_writes),
        ("unlinked_descriptor_writes", v016::CYCLE.unlinked_descriptor_writes),
        ("populated_directory_moves", v016::CYCLE.populated_directory_moves),
        ("directory_creates", v016::CYCLE.directory_creates),
        ("directory_removes", v016::CYCLE.directory_removes),
        ("file_chmod", v016::CYCLE.file_chmod),
        ("directory_chmod", v016::CYCLE.directory_chmod),
        ("explicit_mtime_calls", v016::CYCLE.explicit_mtime_calls),
        ("expected_eexist_probes", v016::CYCLE.expected_eexist_probes),
        ("posix_helper_executions", v016::CYCLE.posix_helper_executions),
    ])
}

/// Whether a stage's counter is published by the host rather than the POSIX
/// helper. The two SDK range-edit calls of stage 2 are host calls.
pub(crate) fn is_host_counter(stage: Stage, name: &str) -> bool {
    matches!(stage, Stage::Edits) && matches!(name, "sdk_edit_calls" | "sdk_edit_members")
}

/// Per-stage split of the cycle counters. The host asserts this table for every
/// stage instead of only the cycle aggregate.
pub(crate) fn stage_counters(stage: Stage) -> BTreeMap<&'static str, usize> {
    match stage {
        Stage::Refresh => BTreeMap::from([
            ("regular_unlinks", 64),
            ("ordinary_regular_creates", 64),
            ("expected_eexist_probes", 1),
        ]),
        Stage::Edits => BTreeMap::from([
            ("main_edit_targets", STAGE2_MAIN_TARGETS),
            ("sdk_edit_calls", STAGE2_SDK_CALLS),
            ("sdk_edit_members", STAGE2_SDK_CALLS),
        ]),

        Stage::Directories => BTreeMap::from([
            ("populated_directory_moves", 2),
            ("directory_removes", 8),
            ("directory_creates", 8),
        ]),
        Stage::AttrsLinks => BTreeMap::from([
            ("hardlink_creates", 8),
            ("symlink_creates", 4),
            ("alias_probe_writes", ALIAS_PROBE_WRITES),
            ("file_chmod", 16),
            ("directory_chmod", 4),
            ("explicit_mtime_calls", 16),
        ]),
        Stage::InodeSubtree => BTreeMap::from([
            // The subtree members plus the eight hardlink aliases and the four
            // symlinks that stage 5 removes.
            ("regular_unlinks", 44),
            ("ordinary_regular_creates", 32),
            ("temporary_regular_creates", 4),
            ("rename_overwrites", 4),
            ("hardlink_unlinks", 8),
            ("symlink_unlinks", 4),
            ("unlinked_descriptor_writes", 1),
            ("directory_removes", 1),
            ("directory_creates", 1),
        ]),
    }
}

/// The stage-5 subtree members, in declared order.
pub(crate) fn subtree_paths(fixture: &LoadFixture) -> Vec<String> {
    fixture.roles.deletion.clone()
}

/// The one bounded negative probe of stage 1.
pub(crate) fn eexist_probe_index(cohort: usize) -> usize {
    refresh_pool_index(cohort, 0)
}

/// Every stage-2 target with its operation, in declared order.
pub(crate) fn stage2_plan(fixture: &LoadFixture, cycle: usize) -> Result<Vec<(String, Stage2Op)>> {
    let _ = cycle;
    let mut rows: Vec<(String, Stage2Op)> = fixture
        .roles
        .edit
        .tiny_pwrite
        .iter()
        .enumerate()
        .map(|(ordinal, path)| (path.clone(), Stage2Op::TinyPwrite { ordinal }))
        .collect();
    rows.push((
        fixture.roles.edit.medium_overwrite.clone(),
        Stage2Op::MediumMidpoint,
    ));
    rows.push((
        fixture.roles.edit.medium_net_zero.clone(),
        Stage2Op::MediumNetZero,
    ));
    rows.push((
        fixture.roles.edit.medium_replace_tail.clone(),
        Stage2Op::MediumReplaceTail,
    ));
    rows.push((fixture.roles.edit.sdk_insert.clone(), Stage2Op::SdkInsert));
    rows.push((fixture.roles.edit.sdk_delete.clone(), Stage2Op::SdkDelete));
    rows.push((
        fixture.roles.edit.anchor_overwrite.clone(),
        Stage2Op::AnchorOverwrite,
    ));
    rows.push((
        fixture.roles.boundary_below.clone(),
        Stage2Op::BoundaryShrink,
    ));
    rows.push((fixture.roles.boundary_above.clone(), Stage2Op::BoundaryGrow));
    if rows.len() != STAGE2_MAIN_TARGETS {
        return Err("v0.1.6 stage-2 target cardinality".into());
    }
    Ok(rows)
}

/// Which stage-2 rows the host performs through the public SDK after the POSIX
/// helper has exited. The pair is two ordered single-file calls, never one
/// atomic cross-file batch.
pub(crate) fn is_sdk_row(operation: &Stage2Op) -> bool {
    matches!(operation, Stage2Op::SdkInsert | Stage2Op::SdkDelete)
}

/// The SDK insertion/deletion of one cycle: `(path, kind, offset)`. The length
/// comes from the sealed fixture, which earlier stages of the cycle never
/// modify.
pub(crate) fn sdk_edit_target(
    fixture: &LoadFixture,
    cycle: usize,
    operation: &Stage2Op,
) -> Result<(String, &'static str, u64)> {
    let (insert, delete) = sdk_pair(cycle);
    let (path, kind) = match operation {
        Stage2Op::SdkInsert => (&fixture.roles.edit.sdk_insert, insert),
        Stage2Op::SdkDelete => (&fixture.roles.edit.sdk_delete, delete),
        _ => return Err("v0.1.6 SDK edit target".into()),
    };
    let len = fixture.len(path)?;
    let middle = ((len / 2) / 8) * 8;
    let offset = (middle + ((cycle - 1) % 2) as u64 * 8) % len;
    Ok((path.clone(), kind, offset))
}

/// Stage-3 scratch directory names for one cycle: `(remove, create)`. The eight
/// initial scratch directories are removed before their eight replacement names
/// are created, so every cycle ends with exactly eight scratch directories and
/// the registered/alternate names alternate.
pub(crate) fn scratch_names(fixture: &LoadFixture, cycle: usize) -> (Vec<String>, Vec<String>) {
    v016::scratch_for_cycle(fixture, cycle)
}

/// Stage-3 populated directory moves: `(source, destination)` pairs. The two
/// subtrees alternate between their registered roots and the destination
/// parents, starting from the registered root on cycle 1. The direction is a
/// pure function of the fixture and the cycle number, so the POSIX helper, the
/// host orchestrator and the independent oracle all agree without observing the
/// live filesystem.
pub(crate) fn move_rows(fixture: &LoadFixture, cycle: usize) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for (index, root) in [&fixture.roles.move_a_root, &fixture.roles.move_b_root]
        .into_iter()
        .enumerate()
    {
        let destination = format!("{}/{}", fixture.roles.move_dest[index], root);
        if cycle % 2 == 1 {
            rows.push((root.clone(), destination));
        } else {
            rows.push((destination, root.clone()));
        }
    }
    rows
}

/// The M1 live high-water path/byte envelope, recomputed from the fixture so a
/// stage that exceeds it fails instead of silently overrunning the cap.
pub(crate) fn live_envelope(fixture: &LoadFixture) -> Result<(usize, u64)> {
    let tier = fixture.tier;
    let paths = tier.initial_paths() + v016::MAX_EXTRA_PATHS;
    let mut bytes = tier.initial_bytes()
        + v016::HARDLINK_TARGETS as u64 * v016::TINY
        + v016::TINY;
    for (_, target, _) in &fixture.roles.links.symlinks {
        bytes += target.len() as u64;
    }
    Ok((paths, bytes))
}

/// Number of stage helper executions a case performs.
pub(crate) fn helper_executions(commits: usize) -> Result<usize> {
    if commits == 0 || commits % STAGES != 0 {
        return Err("v0.1.6 M1 commit count must be a whole number of cycles".into());
    }
    Ok(commits)
}

// ------------------------------------------------------------ case registry

/// The declared M1 topologies with their exact case IDs.
pub(crate) const MIXED_IDS: [&str; 4] = [
    "v016-mixed-development-100mb-5000-k10-v1",
    "v016-mixed-development-100mb-5000-k100-v1",
    "v016-mixed-development-500mb-30000-k10-v1",
    "v016-mixed-development-500mb-30000-k100-v1",
];
pub(crate) const WORKSPACE_IDS: [&str; 4] = [
    "v016-workspace-mixed-100mb-5000-k10-v1",
    "v016-workspace-mixed-100mb-5000-k100-v1",
    "v016-workspace-mixed-500mb-30000-k10-v1",
    "v016-workspace-mixed-500mb-30000-k100-v1",
];
pub(crate) const BRANCH_IDS: [&str; 4] = [
    "v016-branch-mixed-100mb-5000-k10-v1",
    "v016-branch-mixed-100mb-5000-k100-v1",
    "v016-branch-mixed-500mb-30000-k10-v1",
    "v016-branch-mixed-500mb-30000-k100-v1",
];
pub(crate) const EXTENDED_IDS: [&str; 3] = [
    "v016-mixed-exhaustive-100mb-5000-k100-v1",
    "v016-mixed-exhaustive-500mb-30000-k100-v1",
    "v016-workspace-four-100mb-5000-k100-v1",
];

// ------------------------------------------------- two compact branch controls

/// The two compact `branch_development` controls of `benchmark-families.md`
/// §"Two compact branch controls". They are fixture-S graph controls, not M1
/// load-bearing schedules: every local commit is one 256 B SDK overwrite on the
/// first medium file, at the ancestral ordinal's declared offset and value.
pub(crate) const COMPACT_CONVERGENT: &str = "v016-branch-convergent-content-v1";
pub(crate) const COMPACT_DESCENDANT: &str = "v016-branch-fork-descendant-v1";
pub(crate) const COMPACT_IDS: [&str; 2] = [COMPACT_CONVERGENT, COMPACT_DESCENDANT];
/// Trunk depth, child depth, the trunk commit both children fork from, and the
/// local commit of A that the descendant forks from.
pub(crate) const COMPACT_TRUNK_COMMITS: usize = 10;
pub(crate) const COMPACT_CHILD_COMMITS: usize = 10;
pub(crate) const COMPACT_FORK_COMMIT: usize = 5;
pub(crate) const COMPACT_DESCENDANT_FORK: usize = 5;
/// The declared content profile of the two controls.
pub(crate) const COMPACT_PROFILE: &str = "v016-branch-compact";

/// One parsed compact control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CompactCase {
    pub(crate) descendant: bool,
}

/// The four branches of a compact control, in declared order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum CompactRole {
    Trunk = 0,
    A = 1,
    B = 2,
    C = 3,
}

impl CompactRole {
    pub(crate) fn of(self) -> &'static str {
        match self {
            Self::Trunk => "trunk",
            Self::A => "a",
            Self::B => "b",
            Self::C => "c",
        }
    }
    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

/// One declared compact commit: the branch, its local 1-based ordinal and the
/// ancestral ordinal `j` the offset and payload value follow from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CompactStep {
    pub(crate) role: CompactRole,
    pub(crate) local: usize,
    pub(crate) ancestral: usize,
}

impl CompactCase {
    pub(crate) fn roles(self) -> Vec<CompactRole> {
        if self.descendant {
            vec![
                CompactRole::Trunk,
                CompactRole::A,
                CompactRole::B,
                CompactRole::C,
            ]
        } else {
            vec![CompactRole::Trunk, CompactRole::A, CompactRole::B]
        }
    }
    pub(crate) fn total_commits(self) -> usize {
        COMPACT_TRUNK_COMMITS + (self.roles().len() - 1) * COMPACT_CHILD_COMMITS
    }
    pub(crate) fn roots(self) -> usize {
        1 + self.total_commits()
    }
    pub(crate) fn longest_ancestry(self) -> usize {
        if self.descendant {
            COMPACT_FORK_COMMIT + COMPACT_DESCENDANT_FORK + COMPACT_CHILD_COMMITS
        } else {
            COMPACT_FORK_COMMIT + COMPACT_CHILD_COMMITS
        }
    }
    pub(crate) fn branches(self) -> usize {
        self.roles().len()
    }
    /// `cases.json` declares every mode inside the unchanged 15 s budget.
    pub(crate) fn watchdog_seconds(self) -> u64 {
        15
    }
    /// The inherited ancestry of one branch, as an ancestral-ordinal prefix.
    pub(crate) fn fork_ancestry(self, role: CompactRole) -> usize {
        match role {
            CompactRole::Trunk => 0,
            CompactRole::A | CompactRole::B => COMPACT_FORK_COMMIT,
            CompactRole::C => COMPACT_FORK_COMMIT + COMPACT_DESCENDANT_FORK,
        }
    }
}

/// The declared branch name of one compact control's branch. The host
/// orchestrator publishes it and the `historical_access` route resolves it from
/// the sealed producer Store, so both read the same declaration.
pub(crate) fn compact_branch_name(case_id: &str, role: CompactRole) -> String {
    format!("v0.1.6-compact-{case_id}-{}", role.of())
}

/// Resolve one registered compact control, or `None` when the ID is not one.
pub(crate) fn compact_case(id: &str) -> Result<Option<CompactCase>> {
    Ok(match id {
        COMPACT_CONVERGENT => Some(CompactCase { descendant: false }),
        COMPACT_DESCENDANT => Some(CompactCase { descendant: true }),
        _ => None,
    })
}

/// The declared commit sequence of one compact control, in execution order:
/// the trunk, then each child, and finally the descendant.
pub(crate) fn compact_steps(compact: CompactCase) -> Vec<CompactStep> {
    let mut rows = Vec::with_capacity(compact.total_commits());
    for role in compact.roles() {
        let base = compact.fork_ancestry(role);
        let count = if role == CompactRole::Trunk {
            COMPACT_TRUNK_COMMITS
        } else {
            COMPACT_CHILD_COMMITS
        };
        for local in 1..=count {
            rows.push(CompactStep {
                role,
                local,
                ancestral: base + local,
            });
        }
    }
    rows
}

/// The declared 256 B region offset of one ancestral ordinal: `4096*((j-1) mod 2)`.
pub(crate) fn compact_offset(ancestral: usize) -> Result<u64> {
    if ancestral == 0 {
        return Err("v0.1.6 compact ancestral ordinals are 1-based".into());
    }
    Ok(4_096 * ((ancestral - 1) % 2) as u64)
}

/// The branch salt of one compact commit. The convergent control changes
/// identical content on both children; the descendant control salts B and C by
/// branch while A keeps the shared value.
pub(crate) fn compact_salt(compact: CompactCase, role: CompactRole) -> &'static str {
    match (compact.descendant, role) {
        (true, CompactRole::B) => "branch-b",
        (true, CompactRole::C) => "branch-c",
        _ => "",
    }
}

/// The declared payload of one compact commit: the value alternates A/B by
/// `floor((j-1)/2) mod 2`, and a salted branch carries its salt in the recipe.
pub(crate) fn compact_payload(
    family: &str,
    seed: u8,
    ancestral: usize,
    salt: &str,
) -> Result<Content> {
    if ancestral == 0 {
        return Err("v0.1.6 compact ancestral ordinals are 1-based".into());
    }
    let second = ((ancestral - 1) / 2) % 2 == 1;
    let role = match (second, salt.is_empty()) {
        (false, true) => "A".to_owned(),
        (true, true) => "B".to_owned(),
        (false, false) => format!("A-{salt}"),
        (true, false) => format!("B-{salt}"),
    };
    d::content(
        family,
        COMPACT_PROFILE,
        seed,
        ancestral,
        &role,
        super::dedup_workloads::BRANCH_REGION_LEN,
    )
}

/// The declared per-control operation totals.
pub(crate) fn compact_counters(compact: CompactCase) -> BTreeMap<&'static str, usize> {
    BTreeMap::from([
        ("created_commits", compact.total_commits()),
        ("sdk_edit_calls", compact.total_commits()),
        ("sdk_edit_members", compact.total_commits()),
        ("sdk_batch_calls", 0),
        ("posix_helper_executions", 0),
    ])
}

/// Product-free self-check of the two compact controls: the declared
/// cardinalities, the ancestral arithmetic, and the control's own no-op rule
/// (every write must differ from the value it inherits).
pub(crate) fn compact_self_check() -> Result<()> {
    use super::v016_compact as s;
    for (id, descendant, commits, roots, ancestry, branches) in [
        (COMPACT_CONVERGENT, false, 30usize, 31usize, 15usize, 3usize),
        (COMPACT_DESCENDANT, true, 40, 41, 20, 4),
    ] {
        let compact = compact_case(id)?.ok_or("v0.1.6 compact control is not registered")?;
        if compact.descendant != descendant
            || compact.total_commits() != commits
            || compact.roots() != roots
            || compact.longest_ancestry() != ancestry
            || compact.branches() != branches
            || compact_steps(compact).len() != commits
        {
            return Err(format!("v0.1.6 compact cardinality {id}").into());
        }
        let counters = compact_counters(compact);
        if counters.get("created_commits") != Some(&commits)
            || counters.get("sdk_edit_calls") != Some(&commits)
            || counters.get("posix_helper_executions") != Some(&0)
        {
            return Err("v0.1.6 compact counter table".into());
        }
        for seed in 1..=3u8 {
            for role in compact.roles() {
                // Every branch replays the frozen `check_plan.py` control model
                // over its inherited prefix and its own commits; a repeated
                // value in the same region is the no-op the contract forbids.
                let mut state = [None::<u8>; 2];
                let mut apply = |ancestral: usize| -> Result<()> {
                    let offset = compact_offset(ancestral)?;
                    if offset % 4_096 != 0 || offset > 4_096 {
                        return Err("v0.1.6 compact region offset".into());
                    }
                    let value = ((ancestral - 1) / 2) % 2;
                    let slot = (offset / 4_096) as usize;
                    if state[slot] == Some(value as u8) {
                        return Err(format!(
                            "v0.1.6 compact control no-op at ancestral {ancestral}"
                        )
                        .into());
                    }
                    state[slot] = Some(value as u8);
                    Ok(())
                };
                let inherited = compact.fork_ancestry(role);
                for ancestral in 1..=inherited {
                    apply(ancestral)?;
                }
                let salt = compact_salt(compact, role);
                for step in compact_steps(compact)
                    .iter()
                    .filter(|row| row.role == role)
                {
                    let payload =
                        compact_payload("branch_development", seed, step.ancestral, salt)?;
                    if payload.len() != super::dedup_workloads::BRANCH_REGION_LEN {
                        return Err("v0.1.6 compact payload length".into());
                    }
                    apply(step.ancestral)?;
                }
            }
            // Convergent children must be one content function; the descendant
            // control must salt B/C by branch. The salt of a branch is exactly
            // what its declared commits carry.
            let child_a = compact_payload(
                "branch_development",
                seed,
                15,
                compact_salt(compact, CompactRole::A),
            )?
            .digest()?;
            let child_b = compact_payload(
                "branch_development",
                seed,
                15,
                compact_salt(compact, CompactRole::B),
            )?
            .digest()?;
            if compact.descendant == (child_a == child_b) {
                return Err("v0.1.6 compact branch salt declaration".into());
            }
        }
        // The fixture profile declares exactly the two control regions, at Z.
        let regions = s::SProfile::BranchControl.regions();
        if regions.len() != 1
            || regions[0].0 != s::s_medium_path(0)
            || regions[0].1.len() != 2
            || regions[0].1.iter().any(|region| {
                region.label != super::dedup_workloads::BRANCH_Z
                    || region.len != super::dedup_workloads::BRANCH_REGION_LEN
            })
            || regions[0].1[0].offset != 0
            || regions[0].1[1].offset != 4_096
        {
            return Err("v0.1.6 compact fixture regions".into());
        }
    }
    Ok(())
}

/// One parsed M1 case: the load tier name, the local depth and the topology.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MixedCase {
    pub(crate) tier: &'static str,
    pub(crate) k: usize,
    pub(crate) topology: Topology,
    pub(crate) extended: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Topology {
    Sequential,
    Concurrent,
    Branch,
    Four,
}

impl Topology {
    pub(crate) fn branches(self) -> usize {
        match self {
            Self::Sequential => 1,
            Self::Concurrent => 2,
            Self::Branch => 3,
            Self::Four => 4,
        }
    }

    pub(crate) fn trunk_commits(self) -> usize {
        match self {
            Self::Branch => 10,
            _ => 0,
        }
    }

    pub(crate) fn worker_count(self) -> usize {
        match self {
            Self::Sequential | Self::Branch => self.branches(),
            Self::Concurrent | Self::Four => self.branches(),
        }
    }

    pub(crate) fn worker_commits(self, k: usize) -> usize {
        match self {
            Self::Branch => k,
            _ => k,
        }
    }

    pub(crate) fn total_commits(self, k: usize) -> usize {
        match self {
            Self::Branch => 10 + 2 * k,
            Self::Sequential => k,
            Self::Concurrent => 2 * k,
            Self::Four => 4 * k,
        }
    }

    pub(crate) fn roots(self, k: usize) -> usize {
        1 + self.total_commits(k)
    }

    pub(crate) fn longest_ancestry(self, k: usize) -> usize {
        match self {
            Self::Branch => 5 + k,
            _ => k,
        }
    }
}

impl MixedCase {
    /// The two exhaustive replays carry no performance distribution: their
    /// scope is a complete retained-state proof under a separately declared
    /// watchdog, and performance is explicitly N/A rather than zero.
    pub(crate) fn exhaustive_verify_only(self) -> bool {
        self.extended && self.topology == Topology::Sequential
    }
    /// The complete-command watchdog of this case, in seconds.
    pub(crate) fn watchdog_seconds(self) -> u64 {
        if !self.extended {
            return 15;
        }
        if self.topology == Topology::Four {
            return 60;
        }
        if self.tier == "L500" {
            300
        } else {
            120
        }
    }
}

/// Resolve one registered M1 case ID, or `None` when it is not an M1 case.
pub(crate) fn mixed_case(id: &str) -> Result<Option<MixedCase>> {
    let lookup = |ids: &[&'static str], topology: Topology| -> Option<MixedCase> {
        let index = ids.iter().position(|candidate| *candidate == id)?;
        Some(match_row(ids[index], topology))
    };
    for (ids, topology) in [
        (&MIXED_IDS[..], Topology::Sequential),
        (&WORKSPACE_IDS[..], Topology::Concurrent),
        (&BRANCH_IDS[..], Topology::Branch),
    ] {
        if let Some(found) = lookup(ids, topology) {
            return Ok(Some(found));
        }
    }
    // The three extended cases: two verify-only exhaustive replays of the
    // sequential K100 Store and one explicit four-workspace schedule.
    for id_text in EXTENDED_IDS {
        if id == id_text {
            let topology = if id_text.contains("workspace-four") {
                Topology::Four
            } else {
                Topology::Sequential
            };
            let mut row = match_row(id_text, topology);
            row.tier = if id_text.contains("100mb-5000") {
                "L100"
            } else {
                "L500"
            };
            row.k = 100;
            row.extended = true;
            return Ok(Some(row));
        }
    }
    Ok(None)
}

fn match_row(id: &str, topology: Topology) -> MixedCase {
    MixedCase {
        tier: if id.contains("100mb-5000") {
            "L100"
        } else {
            "L500"
        },
        k: if id.contains("-k100-") { 100 } else { 10 },
        topology,
        extended: false,
    }
}

/// Case IDs of one M1 topology, for the family registries.
pub(crate) fn family_case_ids(topology: Topology) -> Vec<&'static str> {
    match topology {
        Topology::Sequential => MIXED_IDS.to_vec(),
        Topology::Concurrent => WORKSPACE_IDS.to_vec(),
        Topology::Branch => BRANCH_IDS.to_vec(),
        Topology::Four => EXTENDED_IDS.to_vec(),
    }
}

/// Product-free plan self-check: the declared case table, the per-stage counter
/// split, the topology cardinalities and the two compact controls.
pub(crate) fn self_check() -> Result<()> {
    let mut ids = Vec::new();
    for (table, topology) in [
        (&MIXED_IDS[..], Topology::Sequential),
        (&WORKSPACE_IDS[..], Topology::Concurrent),
        (&BRANCH_IDS[..], Topology::Branch),
    ] {
        for id in table {
            let row = mixed_case(id)?.ok_or_else(|| format!("{id} is not a declared M1 case"))?;
            if row.topology != topology {
                return Err(format!("{id} topology mismatch").into());
            }
            if row.extended {
                return Err(format!("{id} is a regular case, not extended").into());
            }
            ids.push(*id);
        }
    }
    for (k, total, roots, ancestry) in [
        (10usize, 40usize, 41usize, 10usize),
        (100, 400, 401, 100),
    ] {
        if Topology::Four.total_commits(k) != total
            || Topology::Four.roots(k) != roots
            || Topology::Four.longest_ancestry(k) != ancestry
        {
            return Err("v0.1.6 four-workspace cardinality".into());
        }
    }
    compact_self_check()?;
    for (topology, expected) in [
        (Topology::Sequential, [(10usize, 10usize, 11usize, 10usize), (100, 100, 101, 100)]),
        (Topology::Concurrent, [(10, 20, 21, 10), (100, 200, 201, 100)]),
        (Topology::Branch, [(10, 30, 31, 15), (100, 210, 211, 105)]),
    ] {
        for (k, total, roots, ancestry) in expected {
            if topology.total_commits(k) != total
                || topology.roots(k) != roots
                || topology.longest_ancestry(k) != ancestry
            {
                return Err(format!("v0.1.6 topology cardinality {topology:?} K{k}").into());
            }
        }
    }
    let declared = declared_cycle_counters();
    let mut totals: BTreeMap<&str, usize> = BTreeMap::new();
    for ordinal in 1..=STAGES {
        for (name, value) in stage_counters(Stage::from_ordinal(ordinal)?) {
            *totals.entry(name).or_default() += value;
        }
    }
    for (name, cycle_value) in &declared {
        if let Some(total) = totals.get(name) {
            if total != cycle_value {
                return Err(format!(
                    "v0.1.6 per-stage sum for {name}: {total} != {cycle_value}"
                )
                .into());
            }
        }
    }
    if declared.get("main_edit_targets") != Some(&STAGE2_MAIN_TARGETS)
        || declared.get("sdk_edit_calls") != Some(&STAGE2_SDK_CALLS)
    {
        return Err("v0.1.6 stage-2 declared totals".into());
    }
    for extended in EXTENDED_IDS {
        let row = mixed_case(extended)?.ok_or_else(|| format!("{extended} is not declared"))?;
        if !row.extended {
            return Err(format!("{extended} must be an explicit extended case").into());
        }
    }
    let _ = ids;
    Ok(())
}
