pub(crate) const FAMILY_ID: &str = "edit_count_changing";
pub(crate) const FIXTURE_BYTES: u64 = 256 * 1024;
pub(crate) const FIXTURE_PROFILE: &str = "edit-throughput-256k-v1";
pub(crate) const PERFORMANCE_SCHEMA: &str = "fs-bench-pro-edit-performance-v3";
pub(crate) const VERIFICATION_SCHEMA: &str = "fs-bench-pro-edit-verification-v3";
pub(crate) const SEEDS: [u8; 3] = [1, 2, 3];
pub(crate) const VERIFIERS: [&str; 4] = [
    "insert-middle-4k-on-8m-proof",
    "delete-middle-4k-on-8m-proof",
    "rewrite-full-grow-8m-to-12m-proof",
    "rewrite-full-shrink-8m-to-4m-proof",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    FrozenPrepend,
    Prepend,
    Append,
    Insert,
    Delete,
    Truncate,
    Sparse,
    Grow,
    Shrink,
}

impl Kind {
    pub(crate) const fn operation(self) -> &'static str {
        match self {
            Self::FrozenPrepend | Self::Prepend => "prepend",
            Self::Append => "append",
            Self::Insert => "insert",
            Self::Delete => "delete",
            Self::Truncate => "truncate",
            Self::Sparse => "sparse-write",
            Self::Grow | Self::Shrink => "replace",
        }
    }

    pub(crate) const fn position(self) -> &'static str {
        match self {
            Self::FrozenPrepend | Self::Prepend => "head",
            Self::Append | Self::Truncate | Self::Sparse => "tail",
            Self::Insert | Self::Delete | Self::Grow | Self::Shrink => "middle",
        }
    }

    pub(crate) const fn temp_copy(self) -> bool {
        matches!(
            self,
            Self::FrozenPrepend | Self::Prepend | Self::Insert | Self::Delete | Self::Grow | Self::Shrink
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Scenario {
    pub(crate) id: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) operations: usize,
    pub(crate) kind: Kind,
    pub(crate) paired_same_count_control_id: &'static str,
    pub(crate) frozen: bool,
    pub(crate) fixture_bytes: u64,
    pub(crate) cohort: &'static str,
}

macro_rules! scenario {
    ($id:literal, $display:literal, $operations:literal, $kind:ident, $control:literal) => {
        Scenario {
            id: $id,
            display_name: $display,
            operations: $operations,
            kind: Kind::$kind,
            paired_same_count_control_id: $control,
            frozen: false,
            fixture_bytes: FIXTURE_BYTES,
            cohort: "primary",
        }
    };
}

pub(crate) const SCENARIOS: [Scenario; 25] = [
    Scenario {
        id: "prepend-temp-copy-rename",
        display_name: "legacy-prepend-head-10b-on-32m-temp-copy-rename",
        operations: 1,
        kind: Kind::FrozenPrepend,
        paired_same_count_control_id: "not-applicable-frozen-anchor",
        frozen: true,
        fixture_bytes: 32 * 1024 * 1024,
        cohort: "primary",
    },
    scenario!("prepend-head-4k-ops-1", "Prepend 4 KiB at head, 1 operation", 1, Prepend, "overwrite-head-4k-ops-1"),
    scenario!("prepend-head-4k-ops-10", "Prepend 4 KiB at head, 10 operations", 10, Prepend, "overwrite-head-4k-ops-10"),
    scenario!("prepend-head-4k-ops-100", "Prepend 4 KiB at head, 100 operations", 100, Prepend, "overwrite-head-4k-ops-100"),
    scenario!("append-tail-4k-ops-1", "Append 4 KiB at tail, 1 operation", 1, Append, "overwrite-tail-4k-ops-1"),
    scenario!("append-tail-4k-ops-10", "Append 4 KiB at tail, 10 operations", 10, Append, "overwrite-tail-4k-ops-10"),
    scenario!("append-tail-4k-ops-100", "Append 4 KiB at tail, 100 operations", 100, Append, "overwrite-tail-4k-ops-100"),
    scenario!("insert-middle-4k-ops-1", "Insert 4 KiB at middle, 1 operation", 1, Insert, "overwrite-middle-4k-ops-1"),
    scenario!("insert-middle-4k-ops-10", "Insert 4 KiB at middle, 10 operations", 10, Insert, "overwrite-middle-4k-ops-10"),
    scenario!("insert-middle-4k-ops-100", "Insert 4 KiB at middle, 100 operations", 100, Insert, "overwrite-middle-4k-ops-100"),
    scenario!("delete-middle-2k-ops-1", "Delete 2 KiB at middle, 1 operation", 1, Delete, "overwrite-middle-2k-ops-1"),
    scenario!("delete-middle-2k-ops-10", "Delete 2 KiB at middle, 10 operations", 10, Delete, "overwrite-middle-2k-ops-10"),
    scenario!("delete-middle-2k-ops-100", "Delete 2 KiB at middle, 100 operations", 100, Delete, "overwrite-middle-2k-ops-100"),
    scenario!("truncate-tail-2k-ops-1", "Truncate 2 KiB at tail, 1 operation", 1, Truncate, "overwrite-tail-2k-ops-1"),
    scenario!("truncate-tail-2k-ops-10", "Truncate 2 KiB at tail, 10 operations", 10, Truncate, "overwrite-tail-2k-ops-10"),
    scenario!("truncate-tail-2k-ops-100", "Truncate 2 KiB at tail, 100 operations", 100, Truncate, "overwrite-tail-2k-ops-100"),
    scenario!("sparse-write-past-eof-gap-60k-payload-4k-ops-1", "Sparse write 4 KiB after 60 KiB EOF gap, 1 operation", 1, Sparse, "overwrite-tail-4k-ops-1"),
    scenario!("sparse-write-past-eof-gap-60k-payload-4k-ops-10", "Sparse write 4 KiB after 60 KiB EOF gap, 10 operations", 10, Sparse, "overwrite-tail-4k-ops-10"),
    scenario!("sparse-write-past-eof-gap-60k-payload-4k-ops-100", "Sparse write 4 KiB after 60 KiB EOF gap, 100 operations", 100, Sparse, "overwrite-tail-4k-ops-100"),
    scenario!("replace-middle-grow-2k-to-4k-ops-1", "Replace middle 2 KiB with 4 KiB, 1 operation", 1, Grow, "overwrite-middle-4k-ops-1"),
    scenario!("replace-middle-grow-2k-to-4k-ops-10", "Replace middle 2 KiB with 4 KiB, 10 operations", 10, Grow, "overwrite-middle-4k-ops-10"),
    scenario!("replace-middle-grow-2k-to-4k-ops-100", "Replace middle 2 KiB with 4 KiB, 100 operations", 100, Grow, "overwrite-middle-4k-ops-100"),
    scenario!("replace-middle-shrink-4k-to-2k-ops-1", "Replace middle 4 KiB with 2 KiB, 1 operation", 1, Shrink, "overwrite-middle-2k-ops-1"),
    scenario!("replace-middle-shrink-4k-to-2k-ops-10", "Replace middle 4 KiB with 2 KiB, 10 operations", 10, Shrink, "overwrite-middle-2k-ops-10"),
    scenario!("replace-middle-shrink-4k-to-2k-ops-100", "Replace middle 4 KiB with 2 KiB, 100 operations", 100, Shrink, "overwrite-middle-2k-ops-100"),
];

macro_rules! scaling_scenario {
    ($id:literal, $display:literal, $bytes:expr, $kind:ident) => {
        Scenario {
            id: $id,
            display_name: $display,
            operations: 1,
            kind: Kind::$kind,
            paired_same_count_control_id: "not-applicable-scaling-file-size-cohort",
            frozen: false,
            fixture_bytes: $bytes,
            cohort: "delete-shrink-scaling",
        }
    };
}

pub(crate) const SCALING_SCENARIOS: [Scenario; 6] = [
    scaling_scenario!("delete-middle-2k-on-1mib-ops-1-scale", "Delete middle 2 KiB on 1 MiB", 1024 * 1024, Delete),
    scaling_scenario!("delete-middle-2k-on-10mib-ops-1-scale", "Delete middle 2 KiB on 10 MiB", 10 * 1024 * 1024, Delete),
    scaling_scenario!("delete-middle-2k-on-100mib-ops-1-scale", "Delete middle 2 KiB on 100 MiB", 100 * 1024 * 1024, Delete),
    scaling_scenario!("replace-middle-shrink-4k-to-2k-on-1mib-ops-1-scale", "Replace middle 4 KiB with 2 KiB on 1 MiB", 1024 * 1024, Shrink),
    scaling_scenario!("replace-middle-shrink-4k-to-2k-on-10mib-ops-1-scale", "Replace middle 4 KiB with 2 KiB on 10 MiB", 10 * 1024 * 1024, Shrink),
    scaling_scenario!("replace-middle-shrink-4k-to-2k-on-100mib-ops-1-scale", "Replace middle 4 KiB with 2 KiB on 100 MiB", 100 * 1024 * 1024, Shrink),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Edit {
    pub(crate) offset: u64,
    pub(crate) deleted: usize,
    pub(crate) inserted: usize,
    pub(crate) logical_zero: usize,
    pub(crate) prior_len: u64,
    pub(crate) final_len: u64,
}

pub(crate) fn scenario(id: &str) -> Result<Scenario, String> {
    SCENARIOS
        .into_iter()
        .chain(SCALING_SCENARIOS)
        .find(|scenario| scenario.id == id)
        .ok_or_else(|| format!("unknown count-changing scenario: {id}"))
}

pub(crate) fn fixture_bytes() -> Vec<u8> {
    (0..FIXTURE_BYTES)
        .map(|index| ((index * 29 + index / 7) % 251) as u8)
        .collect()
}

pub(crate) fn replacement_bytes(seed: u8, operation: usize, edit: Edit) -> Vec<u8> {
    (0..edit.inserted)
        .map(|index| {
            let value = edit
                .offset
                .wrapping_add(index as u64)
                .wrapping_mul(43)
                .wrapping_add((operation as u64 + 1).wrapping_mul(109))
                .wrapping_add(u64::from(seed).wrapping_mul(59));
            (value % 251) as u8 ^ 0xa5
        })
        .collect()
}

pub(crate) fn schedule(scenario: Scenario, seed: u8) -> Result<Vec<Edit>, String> {
    if scenario.frozen || !SEEDS.contains(&seed) {
        return Err("count-changing schedule identity".into());
    }
    let mut length = scenario.fixture_bytes;
    let mut edits = Vec::with_capacity(scenario.operations);
    for _ in 0..scenario.operations {
        let (offset, deleted, inserted, logical_zero) = match scenario.kind {
            Kind::Prepend => (0, 0, 4096, 0),
            Kind::Append => (length, 0, 4096, 0),
            Kind::Insert => (length / 2, 0, 4096, 0),
            // The registered IDs stay stable; see the prospective contract correction.
            Kind::Delete => (length / 2 - 1024, 2048, 0, 0),
            Kind::Truncate => (length - 2048, 2048, 0, 0),
            Kind::Sparse => (length + 60 * 1024, 0, 4096, 60 * 1024),
            Kind::Grow => (length / 2 - 1024, 2048, 4096, 0),
            Kind::Shrink => (length / 2 - 2048, 4096, 2048, 0),
            Kind::FrozenPrepend => return Err("frozen schedule".into()),
        };
        let final_len = length
            .checked_sub(deleted as u64)
            .and_then(|value| value.checked_add(inserted as u64))
            .and_then(|value| value.checked_add(logical_zero as u64))
            .ok_or("count-changing length")?;
        let in_bounds = if scenario.kind == Kind::Sparse {
            offset == length + logical_zero as u64
        } else {
            offset <= length && offset + deleted as u64 <= length
        };
        if !in_bounds || final_len == length {
            return Err("count-changing edit bounds".into());
        }
        edits.push(Edit {
            offset,
            deleted,
            inserted,
            logical_zero,
            prior_len: length,
            final_len,
        });
        length = final_len;
    }
    Ok(edits)
}

pub(crate) fn self_check() -> Result<(), String> {
    if FAMILY_ID != "edit_count_changing"
        || SCENARIOS.len() != 25
        || SCENARIOS.iter().filter(|row| row.frozen).count() != 1
        || SCALING_SCENARIOS.len() != 6
        || VERIFIERS.len() != 4
    {
        return Err("count-changing family identity".into());
    }
    for (index, scenario) in SCALING_SCENARIOS.iter().enumerate() {
        if SCENARIOS.iter().any(|row| row.id == scenario.id)
            || SCALING_SCENARIOS[..index]
                .iter()
                .any(|prior| prior.id == scenario.id)
            || scenario.operations != 1
            || !matches!(scenario.fixture_bytes, 1_048_576 | 10_485_760 | 104_857_600)
            || scenario.cohort != "delete-shrink-scaling"
            || scenario.paired_same_count_control_id
                != "not-applicable-scaling-file-size-cohort"
        {
            return Err("count-changing scaling registry".into());
        }
        for seed in SEEDS {
            let edits = schedule(*scenario, seed)?;
            let [edit] = edits.as_slice() else {
                return Err("count-changing scaling schedule".into());
            };
            let (offset, deleted, inserted) = match scenario.kind {
                Kind::Delete => (scenario.fixture_bytes / 2 - 1024, 2048, 0),
                Kind::Shrink => (scenario.fixture_bytes / 2 - 2048, 4096, 2048),
                _ => return Err("count-changing scaling kind".into()),
            };
            if edit.offset != offset
                || edit.deleted != deleted
                || edit.inserted != inserted
                || edit.prior_len != scenario.fixture_bytes
                || edit.final_len != scenario.fixture_bytes - 2048
            {
                return Err("count-changing scaling algebra".into());
            }
        }
    }
    for (index, scenario) in SCENARIOS.iter().enumerate() {
        if SCENARIOS[..index].iter().any(|prior| prior.id == scenario.id)
            || scenario.paired_same_count_control_id.is_empty()
        {
            return Err("count-changing registry".into());
        }
    }
    for seed in SEEDS {
        for start in (1..SCENARIOS.len()).step_by(3) {
            let one = schedule(SCENARIOS[start], seed)?;
            let ten = schedule(SCENARIOS[start + 1], seed)?;
            let hundred = schedule(SCENARIOS[start + 2], seed)?;
            if one != hundred[..1]
                || ten != hundred[..10]
                || hundred.iter().any(|edit| {
                    edit.deleted == edit.inserted
                        || edit.prior_len == edit.final_len
                        || replacement_bytes(seed, 0, *edit).len() != edit.inserted
                })
            {
                return Err("count-changing prefix or length contract".into());
            }
        }
    }
    Ok(())
}
