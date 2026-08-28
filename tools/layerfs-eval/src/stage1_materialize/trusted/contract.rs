pub(in crate::stage1_materialize) const TRUSTED_SCHEDULE: [u64; 3] = [0, 24, 96];

pub(in crate::stage1_materialize) fn trusted_schedule_json() -> String {
    let blocks = TRUSTED_SCHEDULE
        .iter()
        .enumerate()
        .map(|(index, size)| {
            format!(
                "{{\"block\":{},\"integrity_mode\":\"TrustedLocalDev\",\"arm\":\"complete\",\"size_mib\":{},\"warmups\":1,\"measured\":3}}",
                index + 1,
                size
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"schema\":\"layerfs-stage1t-schedule-v1\",\"blocks\":[{blocks}],\"warmups\":3,\"measured\":9,\"rows\":12}}\n"
    )
}
