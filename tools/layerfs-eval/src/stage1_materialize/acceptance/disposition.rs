use super::super::contract::EvalResult;
use super::contract::AcceptanceSample;

pub(in crate::stage1_materialize) struct AcceptanceStats {
    pub(in crate::stage1_materialize) raw: Vec<u128>,
    pub(in crate::stage1_materialize) minimum: u128,
    pub(in crate::stage1_materialize) p50: u128,
    pub(in crate::stage1_materialize) p95: u128,
    pub(in crate::stage1_materialize) maximum: u128,
}

pub(in crate::stage1_materialize) fn acceptance_stats(
    values: &[u128],
) -> EvalResult<AcceptanceStats> {
    if values.len() != 4 {
        return Err("acceptance n=4 statistic requires four values".to_owned());
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    Ok(AcceptanceStats {
        raw: values.to_vec(),
        minimum: sorted[0],
        p50: sorted[1]
            .checked_add(sorted[2])
            .ok_or_else(|| "acceptance p50 overflow".to_owned())?
            / 2,
        p95: sorted[3],
        maximum: sorted[3],
    })
}

pub(in crate::stage1_materialize) struct AbsoluteClass {
    pub(in crate::stage1_materialize) name: &'static str,
    pub(in crate::stage1_materialize) p50_24: u128,
    pub(in crate::stage1_materialize) p95_24: u128,
    pub(in crate::stage1_materialize) p50_96: u128,
    pub(in crate::stage1_materialize) p95_96: u128,
}

// Section 16.1's displayed millisecond gates converted exactly to nanoseconds.
pub(in crate::stage1_materialize) const ABSOLUTE_CLASSES: [AbsoluteClass; 5] = [
    AbsoluteClass {
        name: "375",
        p50_24: 64_000_000,
        p95_24: 70_400_000,
        p50_96: 256_000_000,
        p95_96: 281_600_000,
    },
    AbsoluteClass {
        name: "400",
        p50_24: 60_000_000,
        p95_24: 66_000_000,
        p50_96: 240_000_000,
        p95_96: 264_000_000,
    },
    AbsoluteClass {
        name: "450",
        p50_24: 53_333_000,
        p95_24: 58_667_000,
        p50_96: 213_333_000,
        p95_96: 234_667_000,
    },
    AbsoluteClass {
        name: "500",
        p50_24: 48_000_000,
        p95_24: 52_800_000,
        p50_96: 192_000_000,
        p95_96: 211_200_000,
    },
    AbsoluteClass {
        name: "800",
        p50_24: 30_000_000,
        p95_24: 33_000_000,
        p50_96: 120_000_000,
        p95_96: 132_000_000,
    },
];

pub(in crate::stage1_materialize) struct AcceptanceDisposition {
    pub(in crate::stage1_materialize) status: &'static str,
    pub(in crate::stage1_materialize) populations: Vec<(u64, AcceptanceStats, AcceptanceStats)>,
    pub(in crate::stage1_materialize) wins24: u64,
    pub(in crate::stage1_materialize) wins96: u64,
    pub(in crate::stage1_materialize) fixed_cost_pass: bool,
    pub(in crate::stage1_materialize) p95_relative_pass: bool,
    pub(in crate::stage1_materialize) higher_absolute_class: bool,
    pub(in crate::stage1_materialize) absolute_classes_json: String,
    pub(in crate::stage1_materialize) primary_class_pass: bool,
    pub(in crate::stage1_materialize) primary_nonmaterial_microvariance: bool,
    pub(in crate::stage1_materialize) fitted_fixed_ns: f64,
    pub(in crate::stage1_materialize) fitted_bandwidth_mib_s: f64,
    pub(in crate::stage1_materialize) model_valid: bool,
    pub(in crate::stage1_materialize) cpu_scaling_pass: bool,
    pub(in crate::stage1_materialize) cpu_regression_pass: bool,
    pub(in crate::stage1_materialize) no_resource_regression: bool,
    pub(in crate::stage1_materialize) no_sync_regression: bool,
    pub(in crate::stage1_materialize) no_residue_regression: bool,
}

pub(in crate::stage1_materialize) fn acceptance_disposition(
    samples: &[AcceptanceSample],
) -> EvalResult<AcceptanceDisposition> {
    let mut populations = Vec::new();
    for size in [0_u64, 24, 96] {
        let operand = |wanted| {
            samples
                .iter()
                .filter(|sample| sample.size_mib == size && sample.operand == wanted)
                .map(|sample| sample.wall_ns)
                .collect::<Vec<_>>()
        };
        populations.push((
            size,
            acceptance_stats(&operand('A'))?,
            acceptance_stats(&operand('B'))?,
        ));
    }
    let stats = |size: u64, candidate: bool| {
        populations
            .iter()
            .find(|(candidate_size, _, _)| *candidate_size == size)
            .map(
                |(_, control, candidate_stats)| {
                    if candidate {
                        candidate_stats
                    } else {
                        control
                    }
                },
            )
            .ok_or_else(|| format!("missing acceptance size {size}"))
    };
    let wins = |size| {
        (1..=4)
            .filter(|pair| {
                let wall = |operand| {
                    samples
                        .iter()
                        .find(|sample| {
                            sample.size_mib == size
                                && sample.pair == *pair
                                && sample.operand == operand
                        })
                        .map(|sample| sample.wall_ns)
                };
                matches!((wall('A'), wall('B')), (Some(control), Some(candidate)) if candidate < control)
            })
            .count() as u64
    };
    let wins24 = wins(24);
    let wins96 = wins(96);
    let class_pass = |class: &AbsoluteClass, candidate: bool| -> EvalResult<bool> {
        let s24 = stats(24, candidate)?;
        let s96 = stats(96, candidate)?;
        Ok(s24.p50 <= class.p50_24
            && s24.p95 <= class.p95_24
            && s96.p50 <= class.p50_96
            && s96.p95 <= class.p95_96)
    };
    let mut control_highest = None;
    let mut candidate_highest = None;
    let mut classes = Vec::new();
    for (index, class) in ABSOLUTE_CLASSES.iter().enumerate() {
        let control_pass = class_pass(class, false)?;
        let candidate_pass = class_pass(class, true)?;
        if control_pass {
            control_highest = Some(index);
        }
        if candidate_pass {
            candidate_highest = Some(index);
        }
        classes.push(format!(
            "{{\"class_mib_s\":{},\"control_pass\":{},\"candidate_pass\":{}}}",
            class.name, control_pass, candidate_pass
        ));
    }
    let higher_absolute_class = candidate_highest
        .is_some_and(|candidate| control_highest.is_none_or(|control| candidate > control));
    let fixed_cost_pass = stats(0, true)?.p50 <= stats(0, false)?.p50 + 1_000_000;
    let mut p95_relative_pass = true;
    for size in [0_u64, 24, 96] {
        p95_relative_pass &=
            stats(size, true)?.p95 <= stats(size, false)?.p95 + 1_000_000 || higher_absolute_class;
    }
    let t0 = stats(0, true)?.p50 as f64;
    let t24 = stats(24, true)?.p50 as f64;
    let t96 = stats(96, true)?.p50 as f64;
    let slope = (t96 - t24) / 72.0;
    let residual24 = t24 - (t0 + 24.0 * slope);
    let residual96 = t96 - (t0 + 96.0 * slope);
    let model_valid = slope > 0.0
        && residual24.abs() <= 2_000_000_f64.max(t24 * 0.05)
        && residual96.abs() <= 2_000_000_f64.max(t96 * 0.05);
    let fitted_bandwidth_mib_s = if slope > 0.0 {
        1_000_000_000.0 / slope
    } else {
        0.0
    };
    let cpu = |size| {
        acceptance_stats(
            &samples
                .iter()
                .filter(|sample| sample.size_mib == size && sample.operand == 'B')
                .map(|sample| sample.cpu_ns)
                .collect::<Vec<_>>(),
        )
    };
    let cpu0 = cpu(0)?.p50;
    let cpu24 = cpu(24)?.p50;
    let cpu96 = cpu(96)?.p50;
    let cpu_scaling_pass = cpu24 > cpu0
        && cpu96 > cpu0
        && (cpu96 - cpu0) as f64 / 96.0 <= 1.25 * (cpu24 - cpu0) as f64 / 24.0;
    let mut cpu_regression_pass = true;
    let mut no_resource_regression = true;
    let mut no_sync_regression = true;
    let mut no_residue_regression = true;
    for candidate in samples.iter().filter(|sample| sample.operand == 'B') {
        let control = samples
            .iter()
            .find(|sample| {
                sample.operand == 'A'
                    && sample.pair == candidate.pair
                    && sample.size_mib == candidate.size_mib
            })
            .ok_or_else(|| "candidate has no adjacent control".to_owned())?;
        cpu_regression_pass &= candidate.cpu_ns <= control.cpu_ns;
        no_resource_regression &= candidate.rss_bytes <= control.rss_bytes
            && candidate.q_bytes <= control.q_bytes
            && candidate.fd_peak <= control.fd_peak
            && candidate.primary_connections <= control.primary_connections
            && candidate.scratch_connections <= control.scratch_connections
            && candidate.total_connections <= control.total_connections;
        no_sync_regression &= candidate.sync_calls <= control.sync_calls;
        no_residue_regression &= candidate.residue == 0 && control.residue == 0;
    }
    let primary_class_pass = class_pass(&ABSOLUTE_CLASSES[2], true)?;
    let primary = &ABSOLUTE_CLASSES[2];
    let primary_miss_ns = [
        stats(24, true)?.p50.saturating_sub(primary.p50_24),
        stats(24, true)?.p95.saturating_sub(primary.p95_24),
        stats(96, true)?.p50.saturating_sub(primary.p50_96),
        stats(96, true)?.p95.saturating_sub(primary.p95_96),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    let pass = wins24 >= 3
        && wins96 >= 3
        && fixed_cost_pass
        && p95_relative_pass
        && primary_class_pass
        && model_valid
        && t0 < 20_000_000.0
        && fitted_bandwidth_mib_s >= 500.0
        && cpu_scaling_pass
        && cpu_regression_pass
        && no_resource_regression
        && no_sync_regression
        && no_residue_regression;
    Ok(AcceptanceDisposition {
        status: if pass { "PASS" } else { "REVISE" },
        populations,
        wins24,
        wins96,
        fixed_cost_pass,
        p95_relative_pass,
        higher_absolute_class,
        absolute_classes_json: format!("[{}]", classes.join(",")),
        primary_class_pass,
        primary_nonmaterial_microvariance: !primary_class_pass && primary_miss_ns < 1_000_000,
        fitted_fixed_ns: t0,
        fitted_bandwidth_mib_s,
        model_valid,
        cpu_scaling_pass,
        cpu_regression_pass,
        no_resource_regression,
        no_sync_regression,
        no_residue_regression,
    })
}
