//! The frozen band table, the ratio normalisation and the aggregation order.
//!
//! `gates.rs` is the module that decides every published status, and it decides it
//! from a table of literals. A drifted bound, a rotation of the band table or an
//! inverted severity order changes what the harness publishes without changing any
//! measurement, so each of those is pinned here against the frozen specification
//! rather than against the code that implements it.
//!
//! Two corrections from `gates_and_oracles.md` section 6 are load-bearing and are
//! tested directly:
//!
//! * the byte ladder `{1, 10, 100, 500}` MiB contains **no doublings**, so a raw
//!   ratio is not a per-doubling figure and must be normalised before it meets a
//!   band;
//! * `Metric::Time` can never produce `FAIL`, because the same-binary wall spread
//!   of +17.6 % makes the O(n) time band overlap the O(n log n) one.

use fs_bench_storage_content::gates::{
    aggregate, attribution_gate, band, device_attestation, doublings, pack_accounting,
    per_doubling, require, residency_gate, scaling, slope, swap_gate, timing_purity, Attribution,
    GateClass, Growth, Metric, Status,
};

/// The frozen `(Growth, Metric)` band table, transcribed from the specification.
///
/// Every pair is listed, including the pairs whose bounds are equal to another
/// pair's: a lookup that returned the wrong row for one metric would otherwise be
/// invisible because the neighbouring row happens to agree.
const FROZEN_BANDS: [(Growth, Metric, Option<f64>, Option<f64>); 16] = [
    (Growth::Constant, Metric::Counter, Some(0.90), Some(1.11)),
    (Growth::Constant, Metric::Heap, Some(0.90), Some(1.11)),
    (Growth::Constant, Metric::Disk, Some(0.90), Some(1.11)),
    (Growth::Constant, Metric::Time, Some(0.60), Some(1.67)),
    (Growth::Linear, Metric::Counter, Some(1.90), Some(2.10)),
    (Growth::Linear, Metric::Heap, Some(1.80), Some(2.22)),
    (Growth::Linear, Metric::Disk, Some(1.80), Some(2.22)),
    (Growth::Linear, Metric::Time, Some(1.60), Some(2.40)),
    (
        Growth::Linearithmic,
        Metric::Counter,
        Some(1.95),
        Some(2.55),
    ),
    (Growth::Linearithmic, Metric::Heap, Some(1.90), Some(2.60)),
    (Growth::Linearithmic, Metric::Disk, Some(1.90), Some(2.60)),
    (Growth::Linearithmic, Metric::Time, Some(1.60), Some(2.90)),
    (Growth::Quadratic, Metric::Counter, Some(3.6), None),
    (Growth::Quadratic, Metric::Heap, Some(3.4), None),
    (Growth::Quadratic, Metric::Disk, Some(3.4), None),
    (Growth::Quadratic, Metric::Time, Some(2.6), None),
];

/// The four ladders the specification freezes, in bytes.
const LADDER_MIB: [u64; 4] = [1, 10, 100, 500];

const MIB: f64 = 1024.0 * 1024.0;

fn close(left: f64, right: f64) -> bool {
    (left - right).abs() < 1e-9
}

#[test]
fn every_growth_and_metric_pair_has_its_frozen_band() {
    for (growth, metric, low, high) in FROZEN_BANDS {
        let actual = band(growth, metric);
        assert_eq!(
            actual.low, low,
            "{growth:?}/{metric:?}: lower bound drifted from the frozen table"
        );
        assert_eq!(
            actual.high, high,
            "{growth:?}/{metric:?}: upper bound drifted from the frozen table"
        );
    }
}

#[test]
fn a_band_contains_its_bounds_inclusively_and_is_one_sided_at_the_top() {
    let closed = band(Growth::Linear, Metric::Counter);
    assert!(closed.contains(1.90), "the lower bound is inclusive");
    assert!(closed.contains(2.10), "the upper bound is inclusive");
    assert!(!closed.contains(1.89));
    assert!(!closed.contains(2.11));
    let one_sided = band(Growth::Quadratic, Metric::Counter);
    assert!(one_sided.contains(3.6));
    assert!(
        one_sided.contains(1.0e9),
        "an open upper bound accepts any large ratio"
    );
    assert!(!one_sided.contains(3.59));
}

#[test]
fn band_describe_renders_closed_and_one_sided_forms() {
    assert_eq!(
        band(Growth::Linear, Metric::Counter).describe(),
        "[1.90, 2.10]"
    );
    assert_eq!(
        band(Growth::Quadratic, Metric::Counter).describe(),
        ">= 3.60"
    );
}

#[test]
fn the_byte_ladder_has_no_doublings() {
    // The whole reason `per_doubling` exists. If any of these were 1.0 the
    // normalisation would be a no-op and the raw ratio would have been fine.
    //
    // `log2(10)` is between 3 and 3.4 because 2^3 = 8 < 10 < 2^3.4; `log2(5)` is
    // between 2 and 2.4 because 4 < 5 < 2^2.4. The sanity band is asserted on the
    // runtime value rather than on the constant, so it is a statement about the
    // ladder and not a foldable tautology.
    let expected = [
        (0usize, 1usize, std::f64::consts::LOG2_10, 3.0_f64, 3.4_f64),
        (1, 2, std::f64::consts::LOG2_10, 3.0, 3.4),
        (2, 3, 5.0_f64.log2(), 2.0, 2.4),
    ];
    for (from, to, doublings_expected, sanity_low, sanity_high) in expected {
        let from_bytes = LADDER_MIB[from] as f64 * MIB;
        let to_bytes = LADDER_MIB[to] as f64 * MIB;
        let actual = doublings(from_bytes, to_bytes).expect("a forward ladder step has doublings");
        assert!(
            close(actual, doublings_expected),
            "doublings({} -> {} MiB) was {actual}, expected {doublings_expected}",
            LADDER_MIB[from],
            LADDER_MIB[to]
        );
        assert!(
            actual > sanity_low && actual < sanity_high,
            "doublings({} -> {} MiB) was {actual}, outside ({sanity_low}, {sanity_high})",
            LADDER_MIB[from],
            LADDER_MIB[to]
        );
        assert_ne!(
            actual, 1.0,
            "a ladder step must not be reported as one doubling"
        );
    }
}

#[test]
fn doublings_refuses_degenerate_steps() {
    assert_eq!(
        doublings(1.0, 1.0),
        None,
        "a zero-width step has no doublings"
    );
    assert_eq!(doublings(0.0, 10.0), None);
    assert_eq!(doublings(10.0, 0.0), None);
    assert_eq!(doublings(-1.0, 10.0), None);
    assert!(doublings(1.0, 2.0).is_some_and(|value| close(value, 1.0)));
}

#[test]
fn per_doubling_normalises_the_raw_ratio_on_the_real_ladders() {
    // A doubling-sized growth: the ratio is the *size* ratio, and the per-doubling
    // figure is 2.0 on every rung however far apart the tiers are.
    for (from, to) in [(0usize, 1usize), (1, 2), (2, 3)] {
        let from_bytes = LADDER_MIB[from] as f64 * MIB;
        let to_bytes = LADDER_MIB[to] as f64 * MIB;
        let raw_ratio = to_bytes / from_bytes;
        let normalised = per_doubling(raw_ratio, from_bytes, to_bytes).expect("a forward step");
        assert!(
            close(normalised, 2.0),
            "{} -> {} MiB: a doubling-sized growth normalised to {normalised}, not 2.0",
            LADDER_MIB[from],
            LADDER_MIB[to]
        );
        assert!(
            !close(normalised, raw_ratio),
            "{} -> {} MiB: the raw ratio {raw_ratio} was returned un-normalised",
            LADDER_MIB[from],
            LADDER_MIB[to]
        );
    }
}

#[test]
fn per_doubling_is_identity_for_a_constant_and_refuses_non_positive() {
    let from = 1.0 * MIB;
    let to = 10.0 * MIB;
    assert!(per_doubling(1.0, from, to).is_some_and(|value| close(value, 1.0)));
    assert_eq!(
        per_doubling(0.0, from, to),
        None,
        "a zero ratio has no root"
    );
    assert_eq!(
        per_doubling(-2.0, from, to),
        None,
        "a negative ratio has no root"
    );
    assert_eq!(
        per_doubling(2.0, from, from),
        None,
        "a zero-width step is undefined"
    );
}

#[test]
fn the_normalised_ratio_is_what_the_band_judges() {
    // The two halves of the ladder rule, joined: the raw ratio of the 1 -> 10 MiB
    // rung is 10.0, which is far outside the O(n) counter band, and the normalised
    // figure is 2.0, which is inside it. A harness that fed the raw ratio to the
    // band would refute a correct linear family on every rung.
    let from = 1.0 * MIB;
    let to = 10.0 * MIB;
    let raw = scaling(
        GateClass::Scaling,
        "g3.raw",
        Metric::Counter,
        Growth::Linear,
        Some(10.0),
        "raw ratio",
    );
    assert_eq!(
        raw.status,
        Status::Fail,
        "a raw ratio must not satisfy an O(n) band"
    );

    let normalised_value = per_doubling(10.0, from, to).expect("a forward step");
    let normalised = scaling(
        GateClass::Scaling,
        "g3.per-doubling",
        Metric::Counter,
        Growth::Linear,
        Some(normalised_value),
        "per doubling",
    );
    assert_eq!(
        normalised.status,
        Status::Pass,
        "the normalised per-doubling ratio must satisfy the O(n) band"
    );
}

#[test]
fn slope_recovers_a_known_exponent() {
    let linear = [(1.0, 1.0), (2.0, 2.0), (4.0, 4.0), (8.0, 8.0)];
    assert!(slope(&linear).is_some_and(|value| close(value, 1.0)));

    let quadratic = [(1.0, 1.0), (2.0, 4.0), (4.0, 16.0), (8.0, 64.0)];
    assert!(slope(&quadratic).is_some_and(|value| close(value, 2.0)));

    let constant = [(1.0, 7.0), (2.0, 7.0), (4.0, 7.0), (8.0, 7.0)];
    assert!(slope(&constant).is_some_and(|value| close(value, 0.0)));

    // An n log2 n ladder is not a power law, so a four-point fit recovers an
    // exponent that is super-linear but falls towards 1.0 as the sizes grow. The
    // two ladders are asserted separately because one tolerance cannot honestly
    // cover both: on a small ladder the log factor is still large.
    let small: Vec<(f64, f64)> = [1.0_f64, 2.0, 4.0, 8.0]
        .iter()
        .map(|size| (*size, size * size.log2()))
        .collect();
    // The (1, 0) point is dropped, because a non-positive value cannot be fitted.
    let small_fit = slope(&small).expect("three usable points");
    assert!(
        small_fit > 1.0 && small_fit < 2.0,
        "a small n log n ladder fitted to {small_fit}, which is not super-linear and sub-quadratic"
    );

    let large: Vec<(f64, f64)> = [1024.0_f64, 2048.0, 4096.0, 8192.0]
        .iter()
        .map(|size| (*size, size * size.log2()))
        .collect();
    let large_fit = slope(&large).expect("four positive points");
    assert!(
        large_fit > 1.0 && large_fit < 1.25,
        "a large n log n ladder fitted to {large_fit}, which is not approaching linear"
    );
}

#[test]
fn slope_on_the_real_byte_ladder_recovers_a_linear_growth() {
    let points: Vec<(f64, f64)> = LADDER_MIB
        .iter()
        .map(|mib| (*mib as f64 * MIB, *mib as f64 * MIB))
        .collect();
    assert!(slope(&points).is_some_and(|value| close(value, 1.0)));
}

#[test]
fn slope_refuses_what_it_cannot_fit() {
    assert_eq!(slope(&[]), None, "no points cannot be fitted");
    assert_eq!(slope(&[(1.0, 1.0)]), None, "one point cannot be fitted");
    assert_eq!(
        slope(&[(1.0, 1.0), (1.0, 2.0)]),
        None,
        "a vertical fit has zero variance in log2(size)"
    );
    // Non-positive sizes and values are dropped, not substituted.
    assert_eq!(slope(&[(0.0, 1.0), (-1.0, 2.0)]), None);
    assert_eq!(slope(&[(1.0, 1.0), (2.0, -4.0)]), None);
    let with_zero = slope(&[(0.0, 99.0), (1.0, 1.0), (2.0, 2.0)]).expect("two usable points");
    assert!(
        close(with_zero, 1.0),
        "a dropped zero must not bias the fit"
    );
}

#[test]
fn time_never_produces_fail_whatever_the_value() {
    for growth in [
        Growth::Constant,
        Growth::Linear,
        Growth::Linearithmic,
        Growth::Quadratic,
    ] {
        for value in [0.0_f64, 0.5, 1.0, 2.0, 3.0, 100.0, 1.0e9] {
            let gate = scaling(
                GateClass::Scaling,
                "g3.time",
                Metric::Time,
                growth,
                Some(value),
                "wall",
            );
            assert_ne!(
                gate.status,
                Status::Fail,
                "{growth:?}: elapsed_ns produced FAIL at {value}, which decision D1 forbids"
            );
            assert_eq!(
                gate.status,
                Status::TargetMiss,
                "{growth:?}: a diagnostic time ratio must be reported as a diagnostic"
            );
            assert!(
                gate.limit.contains("DIAGNOSTIC"),
                "the limit text must say the band never gate-decides"
            );
        }
    }
}

#[test]
fn a_gating_metric_can_produce_fail_and_a_missing_tier_is_incomplete() {
    for (growth, metric) in [
        (Growth::Constant, Metric::Counter),
        (Growth::Linear, Metric::Heap),
        (Growth::Linearithmic, Metric::Disk),
        (Growth::Quadratic, Metric::Counter),
    ] {
        // A quadratic band is one-sided at the *bottom*, so "outside" means below
        // the lower bound for it and above the upper bound for a closed band. A
        // single large probe would sit inside `>= 3.60` and pass.
        let frozen = band(growth, metric);
        let outside = match frozen.high {
            Some(high) => high + 1.0,
            None => frozen.low.expect("every frozen band has a lower bound") - 1.0,
        };
        let gate = scaling(
            GateClass::Scaling,
            "g3.out-of-band",
            metric,
            growth,
            Some(outside),
            "just outside",
        );
        assert_eq!(
            gate.status,
            Status::Fail,
            "{growth:?}/{metric:?} must be able to fail at {outside} against {}",
            frozen.describe()
        );
    }
    // A missing tier is INCOMPLETE on every metric, including the diagnostic one:
    // an absent measurement is never a pass and never a zero.
    for metric in [Metric::Counter, Metric::Heap, Metric::Disk, Metric::Time] {
        let gate = scaling(
            GateClass::Scaling,
            "g3.missing",
            metric,
            Growth::Linear,
            None,
            "tier absent",
        );
        assert_eq!(
            gate.status,
            Status::Incomplete,
            "{metric:?}: a missing tier is not a pass"
        );
    }
}

#[test]
fn only_time_is_stripped_of_the_power_to_decide() {
    assert!(!Metric::Time.gate_decides());
    assert!(Metric::Counter.gate_decides());
    assert!(Metric::Heap.gate_decides());
    assert!(Metric::Disk.gate_decides());
    assert_eq!(Metric::Counter.token(), "counter");
    assert_eq!(Metric::Heap.token(), "heap");
    assert_eq!(Metric::Disk.token(), "disk");
    assert_eq!(Metric::Time.token(), "time");
}

#[test]
fn severity_orders_not_run_above_target_miss() {
    // A row with no evidence is weaker than a row with valid evidence and a missed
    // number, so `NOT_RUN` must outrank `TARGET_MISS`.
    assert!(Status::TargetMiss.severity() < Status::NotRun.severity());
    let order = [
        Status::Pass,
        Status::TargetMiss,
        Status::NotRun,
        Status::Ineligible,
        Status::Incomplete,
        Status::Fail,
    ];
    for pair in order.windows(2) {
        assert!(
            pair[0].severity() < pair[1].severity(),
            "{:?} must be less severe than {:?}",
            pair[0],
            pair[1]
        );
    }
    let tokens = [
        "PASS",
        "TARGET_MISS",
        "NOT_RUN",
        "INELIGIBLE",
        "INCOMPLETE",
        "FAIL",
    ];
    for (status, token) in order.iter().zip(tokens) {
        assert_eq!(status.token(), token);
    }
}

#[test]
fn aggregate_returns_the_worst_gate() {
    let pass = require(GateClass::Correctness, "g1", true, "ok", "ok");
    let miss = scaling(
        GateClass::Scaling,
        "g3",
        Metric::Time,
        Growth::Linear,
        Some(9.0),
        "wall",
    );
    let not_run = fs_bench_storage_content::gates::Gate::not_run(
        GateClass::Mechanism,
        "g2",
        "not attempted",
        "declared",
    );
    let fail = require(GateClass::Correctness, "g1.bad", false, "no", "yes");

    assert_eq!(aggregate(&[]), Status::Pass, "no gates is vacuously a pass");
    assert_eq!(aggregate(&[pass.clone()]), Status::Pass);
    assert_eq!(aggregate(&[pass.clone(), miss.clone()]), Status::TargetMiss);
    assert_eq!(
        aggregate(&[miss.clone(), not_run.clone()]),
        Status::NotRun,
        "NOT_RUN must outrank TARGET_MISS"
    );
    assert_eq!(aggregate(&[not_run.clone(), pass.clone()]), Status::NotRun);
    assert_eq!(aggregate(&[not_run, pass, fail.clone()]), Status::Fail);
    assert_eq!(
        aggregate(&[fail.clone()]),
        Status::Fail,
        "FAIL is the worst outcome and must dominate"
    );
}

#[test]
fn require_maps_a_boolean_onto_a_named_limit() {
    let held = require(GateClass::Cleanup, "g5", true, "1 cleanup", "exactly 1");
    assert_eq!(held.status, Status::Pass);
    assert_eq!(held.class, GateClass::Cleanup);
    assert_eq!(held.limit, "exactly 1");
    let missed = require(GateClass::Cleanup, "g5", false, "0 cleanups", "exactly 1");
    assert_eq!(missed.status, Status::Fail);
    assert_eq!(missed.measured, "0 cleanups");
}

#[test]
fn device_attestation_demands_ninety_percent_from_the_device() {
    assert_eq!(device_attestation(1000, None).status, Status::Incomplete);
    assert_eq!(device_attestation(1000, Some(900)).status, Status::Pass);
    assert_eq!(device_attestation(1000, Some(1000)).status, Status::Pass);
    assert_eq!(
        device_attestation(1000, Some(899)).status,
        Status::Ineligible,
        "a cache-served read must be INELIGIBLE, not quietly fast"
    );
    // Zero requested bytes needs nothing from the device.
    assert_eq!(device_attestation(0, Some(0)).status, Status::Pass);
}

#[test]
fn residency_and_swap_gates_fail_closed_on_a_missing_reading() {
    assert_eq!(residency_gate(None).status, Status::Incomplete);
    assert_eq!(residency_gate(Some(0)).status, Status::Pass);
    assert_eq!(
        residency_gate(Some(1)).status,
        Status::Ineligible,
        "a resident page makes a cold claim INELIGIBLE"
    );

    assert_eq!(swap_gate(None).status, Status::Incomplete);
    assert_eq!(swap_gate(Some(0)).status, Status::Pass);
    assert_eq!(
        swap_gate(Some(1)).status,
        Status::Fail,
        "a non-zero swap count is a hard failure"
    );
}

#[test]
fn attribution_refuses_a_shared_with_master_row() {
    assert_eq!(
        attribution_gate(Attribution::Exclusive).status,
        Status::Pass
    );
    assert_eq!(
        attribution_gate(Attribution::SharedWithMaster).status,
        Status::Ineligible
    );
}

#[test]
fn pack_accounting_refuses_a_fabricated_zero() {
    // A non-empty store cannot hold zero pack bytes: that is the lifted-SQL
    // `COALESCE` hazard, and it must be INCOMPLETE rather than a passing 0 <= n.
    assert_eq!(pack_accounting(None, 4096, 3).status, Status::Incomplete);
    assert_eq!(
        pack_accounting(Some(0), 4096, 3).status,
        Status::Incomplete,
        "0 pack bytes over a non-empty store must not pass"
    );
    // An empty store is legitimately zero.
    assert_eq!(pack_accounting(Some(0), 4096, 0).status, Status::Pass);
    assert_eq!(pack_accounting(Some(4096), 4096, 3).status, Status::Pass);
    assert_eq!(pack_accounting(Some(4097), 4096, 3).status, Status::Fail);
}

#[test]
fn timing_purity_turns_a_clipped_tree_into_a_hard_failure() {
    assert_eq!(timing_purity(false, 1024, 32).status, Status::Pass);
    assert_eq!(
        timing_purity(true, 1024, 32).status,
        Status::Fail,
        "clipping is silent by design, so the harness must convert it"
    );
}

#[test]
fn gate_class_tokens_are_the_frozen_seven() {
    let classes = [
        (GateClass::Correctness, "G1"),
        (GateClass::Mechanism, "G2"),
        (GateClass::Scaling, "G3"),
        (GateClass::Resource, "G4"),
        (GateClass::Cleanup, "G5"),
        (GateClass::Custody, "G6"),
        (GateClass::TimingPurity, "G7"),
    ];
    for (class, token) in classes {
        assert_eq!(class.token(), token);
    }
}
