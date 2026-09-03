use super::{hex, Sha256};
use std::collections::BTreeSet;

pub(crate) const SIZES: [u64; 4] = [1_048_576, 10_485_760, 104_857_600, 524_288_000];
pub(crate) const SIZE_LABELS: [&str; 4] = ["1", "10", "100", "500"];
pub(crate) const FIXTURE_PROFILE: &str = "sdk-edit-standard-content-v1";
pub(crate) const PERFORMANCE_SCHEMA: &str = "fs-bench-pro-sdk-edit-performance-v1";
pub(crate) const VERIFICATION_SCHEMA: &str = "fs-bench-pro-sdk-edit-verification-v1";
pub(crate) const SUMMARY_SCHEMA: &str = "fs-bench-pro-sdk-edit-summary-v1";
pub(crate) const STATUS_SCHEMA: &str = "fs-bench-pro-sdk-edit-status-v1";
pub(crate) const FIXTURE_SCHEMA: &str = "fs-bench-pro-sdk-edit-fixture-v1";
pub(crate) const CGROUP_SCHEMA: &str = "fs-bench-pro-sdk-edit-cgroup-v1";
pub(crate) const PROCESS_RSS_SCHEMA: &str = "fs-bench-pro-sdk-edit-process-rss-v1";
pub(crate) const COMBINED_REGISTRY_SHA256: &str =
    "1773c7b82f739eaf1c2b8a2877f56baaa7e72b26ac8980802bdb82c80e270af6";
pub(crate) const FIXTURE_SHA256: [&str; 4] = [
    "d7dfe3d2828aceb85177e6efbeb600f23672a326c902e525e401c1545bb05bdc",
    "29c89128c748e4404f31b0147d447bd524d7b75afc98d56ac4debac762ee4b79",
    "1bb2d79d54f72ae15eb0bb76ad715b9aafeba8ff8f9aa4f47bad3e3f101885bd",
    "bd782f202ec4c40a2070a1d08b78f5135a0ac604b871e4907846740bde906157",
];
pub(crate) const FIXTURE_FILE_ROOT: [&str; 4] = [
    "8fafdf06fac9dbdffb7ccb6b1bde3b2460c387ef1abc55717dee8be401ff6078",
    "dd79a6666e83927d787c8a7679b06f4c98ca5f80b6abd48d94b5e8f84aad1c85",
    "bbee7155df021324495d88954be4db125eca49442b50aadc16439f61f6c32efe",
    "e4ab3cdbf81fe421e6bd2df0b34e57639845dcf244d127507cf15d6ebe01e9a3",
];
pub(crate) const FIXTURE_MAP_ROOT: [&str; 4] = [
    "dcea0efdabc05e8cc5634505601cf682ee362c7e16e6a6c228b4b699b16b3eea",
    "85cc86ea39b444756034d586424a0575b325aabea5beaf7c249a20b4aadb1638",
    "113604cbe8daefa95427d079c28e82bc6c1a78fd353bc7839b5e72c78cbd84b2",
    "9fe7687518d436f9b88c2e983566dd4110fd118b749becf9b087fb60fcc33ee3",
];
pub(crate) const FIXTURE_EXTENTS: [u64; 4] = [54, 544, 5_394, 26_995];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReplacementKind {
    Inline,
    Zero,
}

impl ReplacementKind {
    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::Inline => b'I',
            Self::Zero => b'Z',
        }
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Zero => "zero",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Operation {
    pub(crate) key: &'static str,
    pub(crate) replacement_kind: ReplacementKind,
    pub(crate) replacement_len: u64,
    pub(crate) payload_seed: u64,
    pub(crate) payload_sha256: &'static str,
    pub(crate) locate: fn(u64) -> (u64, u64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Scenario {
    pub(crate) family_id: &'static str,
    pub(crate) id: String,
    pub(crate) operation_key: &'static str,
    pub(crate) fixture_bytes: u64,
    pub(crate) start: u64,
    pub(crate) delete_len: u64,
    pub(crate) replacement_kind: ReplacementKind,
    pub(crate) replacement_len: u64,
    pub(crate) payload_seed: u64,
    pub(crate) payload_sha256: &'static str,
    pub(crate) final_bytes: u64,
    pub(crate) plan_sha256: String,
}

pub(crate) fn scenarios(family_id: &'static str, operations: &[Operation]) -> Vec<Scenario> {
    let mut scenarios = Vec::with_capacity(operations.len() * SIZES.len());
    for operation in operations {
        for (fixture_bytes, label) in SIZES.into_iter().zip(SIZE_LABELS) {
            let (start, delete_len) = (operation.locate)(fixture_bytes);
            let id = format!("{}-on-{}mib-ops-1", operation.key, label);
            let final_bytes = fixture_bytes - delete_len + operation.replacement_len;
            scenarios.push(Scenario {
                family_id,
                plan_sha256: plan_sha256(
                    family_id,
                    &id,
                    fixture_bytes,
                    start,
                    delete_len,
                    operation.replacement_kind,
                    operation.replacement_len,
                    operation.payload_sha256,
                ),
                id,
                operation_key: operation.key,
                fixture_bytes,
                start,
                delete_len,
                replacement_kind: operation.replacement_kind,
                replacement_len: operation.replacement_len,
                payload_seed: operation.payload_seed,
                payload_sha256: operation.payload_sha256,
                final_bytes,
            });
        }
    }
    scenarios
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_sha256(
    family_id: &str,
    scenario_id: &str,
    initial_len: u64,
    start: u64,
    delete_len: u64,
    replacement_kind: ReplacementKind,
    replacement_len: u64,
    replacement_sha256: &str,
) -> String {
    let mut hash = Sha256::new();
    hash.update(b"layerfs/fs-bench-pro/sdk-file-edit-plan/v1\0");
    hash.update(family_id.as_bytes());
    hash.update(&[0]);
    hash.update(scenario_id.as_bytes());
    hash.update(&[0]);
    hash.update(&initial_len.to_be_bytes());
    hash.update(&start.to_be_bytes());
    hash.update(&delete_len.to_be_bytes());
    hash.update(&[replacement_kind.code()]);
    hash.update(&replacement_len.to_be_bytes());
    hash.update(&decode_digest(replacement_sha256));
    hex(&hash.finish())
}

pub(crate) fn ordinary_seed(family_id: &str, operation_key: &str) -> u64 {
    let mut hash = Sha256::new();
    hash.update(b"layerfs/fs-bench-pro/sdk-edit-replacement-seed/v1\0");
    hash.update(family_id.as_bytes());
    hash.update(&[0]);
    hash.update(operation_key.as_bytes());
    hash.update(&[0]);
    u64::from_be_bytes(hash.finish()[..8].try_into().expect("seed width"))
}

pub(crate) fn replacement_bytes(scenario: &Scenario) -> Vec<u8> {
    match scenario.replacement_kind {
        ReplacementKind::Zero => Vec::new(),
        ReplacementKind::Inline if scenario.family_id == "edit_canonical_chunk_count" => {
            if scenario.payload_seed == 0 {
                vec![0; scenario.replacement_len as usize]
            } else {
                splitmix_bytes(
                    0x4348_554e_4b43_4e54 ^ scenario.payload_seed,
                    scenario.replacement_len as usize,
                )
            }
        }
        ReplacementKind::Inline => {
            splitmix_bytes(scenario.payload_seed, scenario.replacement_len as usize)
        }
    }
}

pub(crate) fn splitmix_bytes(mut state: u64, len: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(len);
    while output.len() < len {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        let word = (z ^ (z >> 31)).to_le_bytes();
        output.extend_from_slice(&word[..word.len().min(len - output.len())]);
    }
    output
}

pub(crate) fn fixture_block(state: &mut u64, output: &mut [u8]) {
    let mut offset = 0;
    while offset < output.len() {
        *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        let word = (z ^ (z >> 31)).to_le_bytes();
        let take = word.len().min(output.len() - offset);
        output[offset..offset + take].copy_from_slice(&word[..take]);
        offset += take;
    }
}

pub(crate) fn validate_registry(rows: &[Scenario], expected: usize) -> Result<(), String> {
    if rows.len() != expected || rows.iter().map(|row| &row.id).collect::<BTreeSet<_>>().len() != expected {
        return Err("SDK edit registry cardinality".into());
    }
    for row in rows {
        if row.start + row.delete_len > row.fixture_bytes
            || row.final_bytes != row.fixture_bytes - row.delete_len + row.replacement_len
            || ordinary_seed(row.family_id, row.operation_key) != row.payload_seed
                && row.family_id != "edit_canonical_chunk_count"
            || plan_sha256(
                row.family_id,
                &row.id,
                row.fixture_bytes,
                row.start,
                row.delete_len,
                row.replacement_kind,
                row.replacement_len,
                row.payload_sha256,
            ) != row.plan_sha256
        {
            return Err(format!("SDK edit registry row: {}", row.id));
        }
    }
    Ok(())
}

pub(crate) fn registry_tsv(rows: &[Scenario]) -> String {
    let mut output = String::from("family_id\tscenario_id\toperation_key\tfixture_bytes\tstart\tdelete_len\treplacement_kind\treplacement_len\tpayload_seed\tpayload_sha256\tfinal_bytes\tplan_sha256\n");
    for row in rows {
        use std::fmt::Write as _;
        writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            row.family_id,
            row.id,
            row.operation_key,
            row.fixture_bytes,
            row.start,
            row.delete_len,
            row.replacement_kind.name(),
            row.replacement_len,
            row.payload_seed,
            row.payload_sha256,
            row.final_bytes,
            row.plan_sha256,
        )
        .expect("String write");
    }
    output
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn decode_digest(value: &str) -> [u8; 32] {
    assert_eq!(value.len(), 64);
    let mut output = [0; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).expect("digest hex");
    }
    output
}
