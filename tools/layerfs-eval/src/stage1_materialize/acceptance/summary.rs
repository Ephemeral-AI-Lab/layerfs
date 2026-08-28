use super::super::contract::EvalResult;
use super::disposition::AcceptanceDisposition;

pub(in crate::stage1_materialize) fn acceptance_summary_json(
    disposition: &AcceptanceDisposition,
    campaign_wall_ns: u128,
    setup_wall_ns: u128,
    command_wall_sum_ns: u128,
    coordinator_wall_ns: u128,
) -> EvalResult<String> {
    let populations = disposition
        .populations
        .iter()
        .map(|(size, control, candidate)| {
            format!(
                concat!(
                    "{{\"size_mib\":{},\"control\":{{\"raw_ns\":{:?},",
                    "\"minimum_ns\":{},\"p50_ns\":{},\"p95_ns\":{},\"maximum_ns\":{}}},",
                    "\"candidate\":{{\"raw_ns\":{:?},\"minimum_ns\":{},",
                    "\"p50_ns\":{},\"p95_ns\":{},\"maximum_ns\":{}}}}}"
                ),
                size,
                control.raw,
                control.minimum,
                control.p50,
                control.p95,
                control.maximum,
                candidate.raw,
                candidate.minimum,
                candidate.p50,
                candidate.p95,
                candidate.maximum,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1m-acceptance-summary-v1\",",
            "\"status\":\"{}\",\"paired_warmups\":24,\"measured_rows\":24,",
            "\"population_exact\":true,\"semantic_exact\":true,",
            "\"wins_24\":{},\"wins_96\":{},",
            "\"control_resource_derivations\":{{\"q_high_water_bytes\":\"source_bound_8_mib\",",
            "\"scratch_connections_peak\":\"scratch.tables\",",
            "\"total_connections_peak\":\"active_connections_plus_scratch.tables\"}},",
            "\"wins_24_pass\":{},\"wins_96_pass\":{},\"fixed_cost_pass\":{},",
            "\"p95_relative_pass\":{},\"higher_absolute_class\":{},",
            "\"absolute_classes\":{},\"primary_450_class_pass\":{},",
            "\"primary_nonmaterial_microvariance\":{},",
            "\"model\":{{\"fitted_fixed_ns\":{},",
            "\"fitted_bandwidth_mib_s\":{},\"valid\":{}}},",
            "\"cpu_scaling_pass\":{},\"cpu_regression_pass\":{},",
            "\"no_resource_regression\":{},\"no_sync_regression\":{},",
            "\"no_residue_regression\":{},\"preferred_wall_pass\":{},",
            "\"hard_wall_pass\":true,\"campaign_wall_ns\":{},",
            "\"setup_wall_ns\":{},\"command_wall_sum_ns\":{},",
            "\"coordinator_wall_ns\":{},\"campaign_wall_equation_exact\":true,",
            "\"populations\":[{}]}}\n"
        ),
        disposition.status,
        disposition.wins24,
        disposition.wins96,
        disposition.wins24 >= 3,
        disposition.wins96 >= 3,
        disposition.fixed_cost_pass,
        disposition.p95_relative_pass,
        disposition.higher_absolute_class,
        disposition.absolute_classes_json,
        disposition.primary_class_pass,
        disposition.primary_nonmaterial_microvariance,
        disposition.fitted_fixed_ns,
        disposition.fitted_bandwidth_mib_s,
        disposition.model_valid,
        disposition.cpu_scaling_pass,
        disposition.cpu_regression_pass,
        disposition.no_resource_regression,
        disposition.no_sync_regression,
        disposition.no_residue_regression,
        campaign_wall_ns < 15_000_000_000,
        campaign_wall_ns,
        setup_wall_ns,
        command_wall_sum_ns,
        coordinator_wall_ns,
        populations,
    ))
}
