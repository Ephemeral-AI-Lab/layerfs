#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::stage1_materialize) struct AcceptanceBlock {
    pub(in crate::stage1_materialize) pair: u64,
    pub(in crate::stage1_materialize) size_mib: u64,
    pub(in crate::stage1_materialize) order: [char; 2],
}

pub(in crate::stage1_materialize) const fn acceptance_block(
    pair: u64,
    size_mib: u64,
    order: [char; 2],
) -> AcceptanceBlock {
    AcceptanceBlock {
        pair,
        size_mib,
        order,
    }
}

pub(in crate::stage1_materialize) const ACCEPTANCE_SCHEDULE: [AcceptanceBlock; 12] = [
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
];

pub(in crate::stage1_materialize) fn acceptance_schedule_json() -> String {
    let blocks = ACCEPTANCE_SCHEDULE
        .iter()
        .enumerate()
        .map(|(index, block)| {
            format!(
                "{{\"block\":{},\"pair\":{},\"size_mib\":{},\"order\":[\"{}\",\"{}\"]}}",
                index + 1,
                block.pair,
                block.size_mib,
                block.order[0],
                block.order[1],
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"schema\":\"layerfs-stage1m-acceptance-schedule-v1\",\"blocks\":[{blocks}],\"paired_warmups\":24,\"measured\":24,\"rows\":48}}\n"
    )
}

#[derive(Clone)]
pub(in crate::stage1_materialize) struct AcceptanceSample {
    pub(in crate::stage1_materialize) pair: u64,
    pub(in crate::stage1_materialize) size_mib: u64,
    pub(in crate::stage1_materialize) operand: char,
    pub(in crate::stage1_materialize) wall_ns: u128,
    pub(in crate::stage1_materialize) cpu_ns: u128,
    pub(in crate::stage1_materialize) rss_bytes: u128,
    pub(in crate::stage1_materialize) q_bytes: u128,
    pub(in crate::stage1_materialize) fd_peak: u128,
    pub(in crate::stage1_materialize) primary_connections: u128,
    pub(in crate::stage1_materialize) scratch_connections: u128,
    pub(in crate::stage1_materialize) total_connections: u128,
    pub(in crate::stage1_materialize) sync_calls: u128,
    pub(in crate::stage1_materialize) residue: u128,
}
