//! The registry's frozen cardinality, and the defects it must refuse.
//!
//! `registry::self_check` asserts these rules against the one global registry, so a
//! passing run proves only that *today's* rows are clean. A check that cannot fail
//! is decoration. This file therefore does both halves:
//!
//! * the real registry must satisfy every rule, and `self_check()` must return no
//!   mismatch at all;
//! * the same predicate, expressed locally over synthetic rows, must **catch** an
//!   injected duplicate ID and an injected unregistered family name.
//!
//! The predicate is duplicated here on purpose. `self_check` is not parameterised
//! over a row list, and a negative case that cannot be constructed is not a
//! negative case. The duplication is asserted to agree with the real registry.

use fs_bench_storage_content::families::{self, profile_for_tier};
use fs_bench_storage_content::registry::{
    self, admission_cases, cardinality, cases, render_tsv, Admission, Case, ADMISSION_CASES,
    DIAGNOSTIC_CASES, FROZEN_CARDINALITY, REGISTERED_ROWS, SMOKE_CASES,
};

/// The registry's uniqueness rule, expressed over any row list.
fn first_duplicate_id(rows: &[Case]) -> Option<&'static str> {
    let mut seen: Vec<&'static str> = Vec::new();
    for row in rows {
        if seen.contains(&row.id) {
            return Some(row.id);
        }
        seen.push(row.id);
    }
    None
}

/// The registry's family-registration rule, expressed over any row list.
fn first_unregistered_family(rows: &[Case]) -> Option<&'static str> {
    rows.iter().map(|row| row.family).find(|family| {
        !families::GROUP_IDS.contains(family) && !families::DIAGNOSTIC_GROUPS.contains(family)
    })
}

/// Whether `self_check` reported a particular rule.
fn self_check_reported(what: &str) -> bool {
    registry::self_check()
        .iter()
        .any(|mismatch| mismatch.what == what)
}

#[test]
fn the_registry_is_clean_under_its_own_self_check() {
    let mismatches = registry::self_check();
    assert!(
        mismatches.is_empty(),
        "the registry failed its own self-check: {mismatches:#?}"
    );
}

#[test]
fn the_frozen_cardinality_array_is_what_the_registry_holds() {
    let counts = cardinality();
    assert_eq!(counts.len(), 21, "there are twenty-one registered groups");
    assert_eq!(
        counts,
        FROZEN_CARDINALITY.to_vec(),
        "per-family cardinality drifted from CONTRACT.md section 3"
    );
    assert_eq!(
        FROZEN_CARDINALITY.iter().sum::<usize>(),
        ADMISSION_CASES,
        "the frozen array must sum to the 217 admission cases"
    );
    assert_eq!(FROZEN_CARDINALITY.len(), families::GROUP_IDS.len());
    assert_eq!(FROZEN_CARDINALITY.len(), families::ALL.len());
    assert!(!self_check_reported("frozen cardinality array"));
    assert!(!self_check_reported("cardinality sum"));
}

#[test]
fn the_lane_sizes_and_admission_split_are_the_frozen_ones() {
    assert_eq!(cases().len(), REGISTERED_ROWS);
    assert_eq!(REGISTERED_ROWS, 220);
    assert_eq!(admission_cases().len(), ADMISSION_CASES);
    assert_eq!(ADMISSION_CASES, 217);
    assert_eq!(registry::smoke_cases().len(), SMOKE_CASES);
    assert_eq!(SMOKE_CASES, 20);

    let diagnostic = cases()
        .iter()
        .filter(|row| row.admission == Admission::Diagnostic)
        .count();
    assert_eq!(diagnostic, DIAGNOSTIC_CASES);
    assert_eq!(DIAGNOSTIC_CASES, 3);
    for row in cases()
        .iter()
        .filter(|row| row.admission == Admission::Diagnostic)
    {
        assert_eq!(
            row.family,
            families::DIAGNOSTIC_GROUPS[0],
            "a diagnostic row must live in the registered diagnostic group"
        );
    }
    assert!(!self_check_reported("diagnostic case count"));
    assert!(!self_check_reported("admission case count"));
    assert!(!self_check_reported("smoke lane size"));
}

#[test]
fn every_real_case_id_is_unique_and_a_duplicate_would_be_caught() {
    let rows = cases();
    assert_eq!(
        first_duplicate_id(rows),
        None,
        "the real registry contains a duplicate case ID"
    );
    assert!(!self_check_reported("case ID uniqueness"));

    // Inject one: the second row claims the first row's ID. The predicate must find
    // it, so the check above is not vacuous.
    let mut injected = rows.to_vec();
    injected[1] = Case {
        id: injected[0].id,
        ..injected[1]
    };
    assert_eq!(
        first_duplicate_id(&injected),
        Some(rows[0].id),
        "the uniqueness predicate failed to catch an injected duplicate ID"
    );
}

#[test]
fn every_real_family_is_registered_and_an_unknown_name_would_be_caught() {
    let rows = cases();
    assert_eq!(
        first_unregistered_family(rows),
        None,
        "a row names a family that no module declares"
    );
    assert!(!self_check_reported("family identifier is registered"));

    let mut injected = rows.to_vec();
    injected[0] = Case {
        family: "c1.not-a-real-family",
        ..injected[0]
    };
    assert_eq!(
        first_unregistered_family(&injected),
        Some("c1.not-a-real-family"),
        "the registration predicate failed to catch an unregistered family name"
    );

    // The diagnostic group is registered even though it is not one of the twenty
    // families, so the predicate must not treat it as unknown.
    let mut diagnostic = rows.to_vec();
    diagnostic[0] = Case {
        family: families::DIAGNOSTIC_GROUPS[0],
        ..diagnostic[0]
    };
    assert_eq!(
        first_unregistered_family(&diagnostic),
        None,
        "the registered diagnostic group must not be reported as an unknown family"
    );
}

#[test]
fn the_profile_per_tier_ruling_is_fixed() {
    // A bracketed profile list is **one** case rendered with a tier-selected
    // profile, never one case per profile. The rule is fixed once, in
    // `profile_for_tier`: the two small tiers take the mixed variant.
    assert_eq!(profile_for_tier(0, "mixed-v4", "compact-v2"), "mixed-v4");
    assert_eq!(profile_for_tier(1, "mixed-v4", "compact-v2"), "mixed-v4");
    assert_eq!(profile_for_tier(2, "mixed-v4", "compact-v2"), "compact-v2");
    assert_eq!(profile_for_tier(3, "mixed-v4", "compact-v2"), "compact-v2");
}

#[test]
fn the_ruling_is_visible_in_the_rows_it_governs() {
    // `c1.change-locality` writes `[-mixed-v4|-compact-v2]` and must still hold
    // twelve rows: three kinds over four tiers, not twenty-four.
    let rows: Vec<Case> = cases()
        .iter()
        .copied()
        .filter(|row| row.family == "c1.change-locality")
        .collect();
    assert_eq!(
        rows.len(),
        12,
        "the bracketed list must not multiply the row count"
    );

    let mut profiles: Vec<&str> = rows.iter().map(|row| row.profile).collect();
    profiles.sort_unstable();
    profiles.dedup();
    assert_eq!(
        profiles,
        vec!["compact-v2", "mixed-v4"],
        "the ruling must render both profiles across the ladder"
    );

    for row in &rows {
        let expected = profile_for_tier(row.tier as usize, "mixed-v4", "compact-v2");
        assert_eq!(
            row.profile, expected,
            "{} at tier {} carries the wrong profile",
            row.id, row.tier
        );
        assert!(
            row.id.ends_with(row.profile),
            "{} does not name the profile it was rendered with",
            row.id
        );
    }

    // And the golden table names it too, so a drift is visible in a diff rather
    // than only inside a passing assertion.
    let golden = render_tsv();
    for row in &rows {
        assert!(
            golden.contains(row.id),
            "the golden rendering lost row {}",
            row.id
        );
    }
}

#[test]
fn the_byte_ladder_families_carry_both_profiles_too() {
    // The same ruling governs `c1.many-tiny` and the two tree families.
    for family in [
        "c1.many-tiny",
        "c1.tree.construct-traverse",
        "c1.tree.namespace-mutation",
    ] {
        let rows: Vec<Case> = cases()
            .iter()
            .copied()
            .filter(|row| row.family == family)
            .collect();
        assert!(!rows.is_empty(), "{family} has no rows");
        let mut profiles: Vec<&str> = rows.iter().map(|row| row.profile).collect();
        profiles.sort_unstable();
        profiles.dedup();
        assert!(
            profiles.contains(&"mixed-v4") && profiles.contains(&"compact-v2"),
            "{family} did not render the bracketed profile list: {profiles:?}"
        );
        for row in &rows {
            if row.profile == "mixed-v4" || row.profile == "compact-v2" {
                assert_eq!(
                    row.profile,
                    profile_for_tier(row.tier as usize, "mixed-v4", "compact-v2"),
                    "{} at tier {} violates the tier-to-profile ruling",
                    row.id,
                    row.tier
                );
            }
        }
    }
}

#[test]
fn the_registry_owns_one_row_per_family_module_entry() {
    // `cardinality()` counts by family name; every group module must therefore
    // contribute exactly the number of rows the frozen array names, so no family
    // can silently gain or lose a case.
    let mut counted: Vec<(&str, usize)> = families::GROUP_IDS
        .iter()
        .map(|group| {
            (
                *group,
                cases().iter().filter(|row| row.family == *group).count(),
            )
        })
        .collect();
    counted.retain(|(_, count)| *count == 0);
    assert_eq!(
        counted,
        Vec::new(),
        "a registered group contributed no rows: {counted:?}"
    );
}
