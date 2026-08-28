use super::contract::{
    acceptance_block, acceptance_schedule_json, AcceptanceSample, ACCEPTANCE_SCHEDULE,
};
use super::disposition::{acceptance_disposition, acceptance_stats};

#[test]
fn acceptance_schedule_statistics_and_gates_are_frozen() {
    assert_eq!(
        ACCEPTANCE_SCHEDULE,
        [
            acceptance_block(1, 0, ['A', 'B']),
            acceptance_block(1, 24, ['A', 'B']),
            acceptance_block(1, 96, ['A', 'B']),
            acceptance_block(2, 96, ['B', 'A']),
            acceptance_block(2, 24, ['B', 'A']),
            acceptance_block(2, 0, ['B', 'A']),
            acceptance_block(3, 24, ['B', 'A']),
            acceptance_block(3, 0, ['B', 'A']),
            acceptance_block(3, 96, ['B', 'A']),
            acceptance_block(4, 0, ['A', 'B']),
            acceptance_block(4, 96, ['A', 'B']),
            acceptance_block(4, 24, ['A', 'B']),
        ]
    );
    let schedule = acceptance_schedule_json();
    assert!(schedule.contains("\"paired_warmups\":24"));
    assert!(schedule.contains("\"measured\":24"));
    let stats = acceptance_stats(&[40, 10, 30, 20]).unwrap();
    assert_eq!(
        (stats.minimum, stats.p50, stats.p95, stats.maximum),
        (10, 25, 40, 40)
    );
    let mut samples = Vec::new();
    for pair in 1..=4 {
        for (size, control_wall, candidate_wall, candidate_cpu) in [
            (0, 12_000_000, 10_000_000, 1_000_000),
            (24, 70_000_000, 53_200_000, 25_000_000),
            (96, 250_000_000, 182_800_000, 97_000_000),
        ] {
            samples.push(AcceptanceSample {
                pair,
                size_mib: size,
                operand: 'A',
                wall_ns: control_wall,
                cpu_ns: candidate_cpu + 1_000_000,
                rss_bytes: 10_000_000,
                q_bytes: 8 * 1024 * 1024,
                fd_peak: 12,
                primary_connections: 1,
                scratch_connections: 3,
                total_connections: 4,
                sync_calls: 4,
                residue: 0,
            });
            samples.push(AcceptanceSample {
                pair,
                size_mib: size,
                operand: 'B',
                wall_ns: candidate_wall,
                cpu_ns: candidate_cpu,
                rss_bytes: 9_000_000,
                q_bytes: 8 * 1024 * 1024 - 1,
                fd_peak: 10,
                primary_connections: 1,
                scratch_connections: 1,
                total_connections: 2,
                sync_calls: 3,
                residue: 0,
            });
        }
    }
    let disposition = acceptance_disposition(&samples).unwrap();
    assert_eq!(disposition.status, "PASS");
    assert_eq!((disposition.wins24, disposition.wins96), (4, 4));
    assert!(disposition.fixed_cost_pass);
    assert!(disposition.p95_relative_pass);
    assert!(disposition.higher_absolute_class);
    assert!(disposition.primary_class_pass);
    assert!(disposition.model_valid);
    assert!(disposition.fitted_bandwidth_mib_s >= 500.0);
    assert!(disposition.cpu_scaling_pass);
    assert!(disposition.cpu_regression_pass);
    assert!(disposition.no_resource_regression);
    assert!(disposition.no_sync_regression);
    assert!(disposition.no_residue_regression);

    for sample in &mut samples {
        if sample.operand == 'B' && sample.pair <= 2 {
            sample.wall_ns = sample.wall_ns.saturating_add(100_000_000);
        }
    }
    assert_eq!(acceptance_disposition(&samples).unwrap().status, "REVISE");
}
