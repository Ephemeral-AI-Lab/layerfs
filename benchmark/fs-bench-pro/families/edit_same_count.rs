pub(crate) const FAMILY_ID: &str = "edit_same_count";
pub(crate) const FIXTURE_BYTES: u64 = 256 * 1024;
pub(crate) const FIXTURE_PROFILE: &str = "edit-throughput-256k-v1";
pub(crate) const PERFORMANCE_SCHEMA: &str = "fs-bench-pro-edit-performance-v1";
pub(crate) const VERIFICATION_SCHEMA: &str = "fs-bench-pro-edit-verification-v2";
pub(crate) const VERIFIER_ID: &str = "overwrite-fragmented-10b-ops-1000-proof";
pub(crate) const SEEDS: [u8; 3] = [1, 2, 3];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Position {
    Head,
    Middle,
    Tail,
    Distributed,
}

impl Position {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Head => "head",
            Self::Middle => "middle",
            Self::Tail => "tail",
            Self::Distributed => "distributed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Scenario {
    pub(crate) id: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) operations: usize,
    pub(crate) position: Position,
    pub(crate) frozen: bool,
}

pub(crate) const SCENARIOS: [Scenario; 14] = [
    Scenario {
        id: "small-edit",
        display_name: "legacy-overwrite-distributed-10b-ops-1",
        operations: 1,
        position: Position::Distributed,
        frozen: true,
    },
    Scenario {
        id: "edit16",
        display_name: "legacy-overwrite-distributed-10b-ops-16-commit-each",
        operations: 16,
        position: Position::Distributed,
        frozen: true,
    },
    Scenario {
        id: "overwrite-head-4k-ops-1",
        display_name: "Overwrite 4 KiB at head, 1 operation",
        operations: 1,
        position: Position::Head,
        frozen: false,
    },
    Scenario {
        id: "overwrite-head-4k-ops-10",
        display_name: "Overwrite 4 KiB at head, 10 operations",
        operations: 10,
        position: Position::Head,
        frozen: false,
    },
    Scenario {
        id: "overwrite-head-4k-ops-100",
        display_name: "Overwrite 4 KiB at head, 100 operations",
        operations: 100,
        position: Position::Head,
        frozen: false,
    },
    Scenario {
        id: "overwrite-middle-4k-ops-1",
        display_name: "Overwrite 4 KiB in middle, 1 operation",
        operations: 1,
        position: Position::Middle,
        frozen: false,
    },
    Scenario {
        id: "overwrite-middle-4k-ops-10",
        display_name: "Overwrite 4 KiB in middle, 10 operations",
        operations: 10,
        position: Position::Middle,
        frozen: false,
    },
    Scenario {
        id: "overwrite-middle-4k-ops-100",
        display_name: "Overwrite 4 KiB in middle, 100 operations",
        operations: 100,
        position: Position::Middle,
        frozen: false,
    },
    Scenario {
        id: "overwrite-tail-4k-ops-1",
        display_name: "Overwrite 4 KiB at tail, 1 operation",
        operations: 1,
        position: Position::Tail,
        frozen: false,
    },
    Scenario {
        id: "overwrite-tail-4k-ops-10",
        display_name: "Overwrite 4 KiB at tail, 10 operations",
        operations: 10,
        position: Position::Tail,
        frozen: false,
    },
    Scenario {
        id: "overwrite-tail-4k-ops-100",
        display_name: "Overwrite 4 KiB at tail, 100 operations",
        operations: 100,
        position: Position::Tail,
        frozen: false,
    },
    Scenario {
        id: "overwrite-distributed-1b-to-4k-ops-1",
        display_name: "Distributed 1 B-4 KiB overwrite, 1 operation",
        operations: 1,
        position: Position::Distributed,
        frozen: false,
    },
    Scenario {
        id: "overwrite-distributed-1b-to-4k-ops-10",
        display_name: "Distributed 1 B-4 KiB overwrite, 10 operations",
        operations: 10,
        position: Position::Distributed,
        frozen: false,
    },
    Scenario {
        id: "overwrite-distributed-1b-to-4k-ops-100",
        display_name: "Distributed 1 B-4 KiB overwrite, 100 operations",
        operations: 100,
        position: Position::Distributed,
        frozen: false,
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PairControl {
    pub(crate) id: &'static str,
    pub(crate) operations: usize,
    pub(crate) position: Position,
}

pub(crate) const PAIR_CONTROLS: [PairControl; 6] = [
    PairControl { id: "overwrite-middle-2k-ops-1", operations: 1, position: Position::Middle },
    PairControl { id: "overwrite-middle-2k-ops-10", operations: 10, position: Position::Middle },
    PairControl { id: "overwrite-middle-2k-ops-100", operations: 100, position: Position::Middle },
    PairControl { id: "overwrite-tail-2k-ops-1", operations: 1, position: Position::Tail },
    PairControl { id: "overwrite-tail-2k-ops-10", operations: 10, position: Position::Tail },
    PairControl { id: "overwrite-tail-2k-ops-100", operations: 100, position: Position::Tail },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Edit {
    pub(crate) offset: u64,
    pub(crate) len: usize,
}

pub(crate) fn fixture_bytes() -> Vec<u8> {
    (0..FIXTURE_BYTES)
        .map(|index| ((index * 29 + index / 7) % 251) as u8)
        .collect()
}

pub(crate) fn replacement_bytes(seed: u8, operation: usize, edit: Edit) -> Vec<u8> {
    (0..edit.len)
        .map(|index| {
            let value = edit
                .offset
                .wrapping_add(index as u64)
                .wrapping_mul(37)
                .wrapping_add((operation as u64 + 1).wrapping_mul(101))
                .wrapping_add(u64::from(seed).wrapping_mul(53));
            (value % 251) as u8 ^ 0x5a
        })
        .collect()
}

pub(crate) fn fragmented_schedule(
    cohort: &str,
    count: usize,
    seed: u8,
) -> Result<Vec<Edit>, String> {
    if !matches!(count, 100 | 1_000) || !SEEDS.contains(&seed) {
        return Err("same-count fragmentation identity".into());
    }
    let edits = (0..count)
        .map(|operation| {
            let offset = match cohort {
                "increasing" => (operation * 20) as u64,
                "descending" => ((999 - operation) * 20) as u64,
                "hotspot" => {
                    ((operation as u64 * 7_919 + u64::from(seed) * 101) % (64 * 1024 - 10))
                        + 96 * 1024
                }
                _ => return Err("same-count fragmentation cohort".into()),
            };
            Ok(Edit { offset, len: 10 })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(edits)
}

pub(crate) fn scenario(id: &str) -> Result<Scenario, String> {
    SCENARIOS
        .into_iter()
        .find(|scenario| scenario.id == id)
        .ok_or_else(|| format!("unknown same-count scenario: {id}"))
}

pub(crate) fn pair_control(id: &str) -> Result<PairControl, String> {
    PAIR_CONTROLS
        .into_iter()
        .find(|control| control.id == id)
        .ok_or_else(|| format!("unknown same-count pair control: {id}"))
}

pub(crate) fn pair_control_schedule(control: PairControl, seed: u8) -> Result<Vec<Edit>, String> {
    if !SEEDS.contains(&seed) {
        return Err("same-count pair-control identity".into());
    }
    let offset = match control.position {
        Position::Middle => FIXTURE_BYTES / 2 - 1024,
        Position::Tail => FIXTURE_BYTES - 2048,
        _ => return Err("same-count pair-control position".into()),
    };
    Ok(vec![Edit { offset, len: 2048 }; control.operations])
}

pub(crate) fn schedule(scenario: Scenario, seed: u8) -> Result<Vec<Edit>, String> {
    if scenario.frozen || !SEEDS.contains(&seed) {
        return Err("same-count schedule identity".into());
    }
    let mut state = 0x9e37_79b9_7f4a_7c15_u64 ^ u64::from(seed);
    let mut output = Vec::with_capacity(scenario.operations);
    for operation in 0..scenario.operations {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let (len, offset) = match scenario.position {
            Position::Head => (4096, 0),
            Position::Tail => (4096, FIXTURE_BYTES - 4096),
            Position::Middle => (4096, FIXTURE_BYTES / 4 + state % (FIXTURE_BYTES / 2 - 4096)),
            Position::Distributed => {
                let len = 1 + (state as usize % 4096);
                let mixed = state.wrapping_add((operation as u64 + 1).wrapping_mul(2_654_435_761));
                (len, mixed % (FIXTURE_BYTES - len as u64 + 1))
            }
        };
        output.push(Edit { offset, len });
    }
    Ok(output)
}

pub(crate) fn self_check() -> Result<(), String> {
    for (index, scenario) in SCENARIOS.iter().enumerate() {
        if SCENARIOS[..index]
            .iter()
            .any(|prior| prior.id == scenario.id)
        {
            return Err("duplicate same-count scenario".into());
        }
    }
    for seed in SEEDS {
        for position in [
            Position::Head,
            Position::Middle,
            Position::Tail,
            Position::Distributed,
        ] {
            let rows: Vec<_> = SCENARIOS
                .iter()
                .filter(|row| !row.frozen && row.position == position)
                .copied()
                .collect();
            let one = schedule(rows[0], seed)?;
            let ten = schedule(rows[1], seed)?;
            let hundred = schedule(rows[2], seed)?;
            if one != hundred[..1]
                || ten != hundred[..10]
                || hundred
                    .iter()
                    .any(|edit| edit.len == 0 || edit.offset + edit.len as u64 > FIXTURE_BYTES)
            {
                return Err("same-count prefix, length, or bounds".into());
            }
            if replacement_bytes(seed, 0, one[0]).len() != one[0].len {
                return Err("same-count replacement length".into());
            }
        }
    }
    for seed in SEEDS {
        for start in [0, 3] {
            let one = pair_control_schedule(PAIR_CONTROLS[start], seed)?;
            let ten = pair_control_schedule(PAIR_CONTROLS[start + 1], seed)?;
            let hundred = pair_control_schedule(PAIR_CONTROLS[start + 2], seed)?;
            if one != hundred[..1] || ten != hundred[..10] {
                return Err("same-count pair-control prefix".into());
            }
        }
    }
    if FAMILY_ID != "edit_same_count"
        || SCENARIOS.len() != 14
        || SCENARIOS.iter().filter(|row| row.frozen).count() != 2
    {
        return Err("same-count family identity".into());
    }
    for seed in SEEDS {
        for cohort in ["increasing", "descending", "hotspot"] {
            let hundred = fragmented_schedule(cohort, 100, seed)?;
            let thousand = fragmented_schedule(cohort, 1_000, seed)?;
            if hundred != thousand[..100] {
                return Err("same-count fragmentation prefix".into());
            }
            if thousand
                .iter()
                .any(|edit| edit.offset + edit.len as u64 > FIXTURE_BYTES)
            {
                return Err("same-count fragmentation bounds".into());
            }
        }
    }
    Ok(())
}
