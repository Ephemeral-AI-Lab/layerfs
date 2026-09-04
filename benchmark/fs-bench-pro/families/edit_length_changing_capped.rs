// Versioned capped-result replacements; released length-changing IDs stay intact.
use super::edit_length_changing;
use super::sdk_edit_common::{self, ReplacementKind, Scenario};
use super::workspace_common::{self as common, Case, Content, Entry, EntryKind, Receipt};
use super::Result;

pub(crate) const FAMILY: &str = "edit_length_changing_capped";
pub(crate) const FIXTURE_PROFILE: &str = "sdk-edit-standard-content-capped-prefix-v1";
pub(crate) const GENERATOR_SEED: u64 = 0x4c41_5945_5246_5331;
pub(crate) const INPUT_BYTES: [u64; 2] = [524_283_904, 524_285_952];
pub(crate) const INPUT_SHA256: [&str; 2] = [
    "f1b6c61d9c126beba89dd2a310f727fd63cbbf131b793a78fe21247238c98c1f",
    "0e2cc5b14abf95553ba633a11b395c80de3f0a1642cdef0adf753cb5984fbe55",
];
pub(crate) const DEFINITION_MANIFEST_SHA256: &str =
    "a1dab17cf1a4a01b19322c8651ade025f64621f67848b59bcbc7e6c9fa2e3c2e";
pub(crate) const REPETITION_ORDER: [[usize; 5]; 5] = [
    [0, 1, 2, 3, 4],
    [2, 3, 4, 0, 1],
    [4, 0, 1, 2, 3],
    [1, 2, 3, 4, 0],
    [4, 0, 1, 2, 3],
];
const ORIGINAL_OPERATIONS: [usize; 5] = [0, 2, 3, 4, 7];
const PLAN_SHA256: [&str; 5] = [
    "a4a7307366a8fde698f3cc087d82ec74dc00a544303875c4ce6d45093b5f556a",
    "da89c48f4e577f352c4430fdb4e59b6d434c9f778c1f6647199307a0ad848cd0",
    "1cabc3f6cec36534d7cdb4056355537853afe721f976f9955c32540eeae521fd",
    "0e5ac651d919ead4c771692681c27f5babc6652475bd5cba12ac7c729891363f",
    "c97d217bf643d589ce25822eaa2b59aaafc2d238fe95af8bdee784ef9ec69ebe",
];

pub(crate) fn registry() -> Vec<Scenario> {
    ORIGINAL_OPERATIONS
        .into_iter()
        .map(|index| {
            let operation = edit_length_changing::OPERATIONS[index];
            let fixture_bytes = INPUT_BYTES[usize::from(index == 4)];
            let (start, delete_len) = (operation.locate)(fixture_bytes);
            let id = format!(
                "{}-input-{fixture_bytes}b-result-500mib-ops-1-capped-v1",
                operation.key
            );
            let plan_sha256 = sdk_edit_common::plan_sha256(
                FAMILY,
                &id,
                fixture_bytes,
                start,
                delete_len,
                operation.replacement_kind,
                operation.replacement_len,
                operation.payload_sha256,
            );
            Scenario {
                family_id: FAMILY,
                id,
                operation_key: operation.key,
                fixture_bytes,
                start,
                delete_len,
                replacement_kind: operation.replacement_kind,
                replacement_len: operation.replacement_len,
                payload_seed: operation.payload_seed,
                payload_sha256: operation.payload_sha256,
                final_bytes: fixture_bytes - delete_len + operation.replacement_len,
                plan_sha256,
            }
        })
        .collect()
}

pub(crate) fn cases() -> Vec<Case> {
    registry()
        .into_iter()
        .map(|scenario| Case {
            id: scenario.id,
            family: FAMILY,
            tier: 500,
            kind: scenario.operation_key,
        })
        .collect()
}

pub(crate) fn sdk_scenario(case: &Case) -> Result<Scenario> {
    if case.family != FAMILY || case.tier != 500 {
        return Err("capped SDK family/tier identity".into());
    }
    registry()
        .into_iter()
        .find(|scenario| scenario.id == case.id && scenario.operation_key == case.kind)
        .ok_or_else(|| "unknown capped SDK scenario".into())
}

pub(crate) fn fixture(case: &Case, repetition: u8) -> Result<Vec<Entry>> {
    if !(1..=5).contains(&repetition) {
        return Err("capped SDK repetition must be 1 through 5".into());
    }
    let scenario = sdk_scenario(case)?;
    Ok(vec![
        Entry::directory("."),
        Entry::file(
            "payload.bin",
            Content::Seed {
                seed: GENERATOR_SEED,
                len: scenario.fixture_bytes,
            },
        ),
    ])
}

pub(crate) fn expected(case: &Case, repetition: u8, step: usize) -> Result<Vec<Entry>> {
    if step > 1 {
        return Err("capped SDK has one edit, only steps 0 and 1 exist".into());
    }
    let mut entries = fixture(case, repetition)?;
    if step == 1 {
        let scenario = sdk_scenario(case)?;
        let EntryKind::File(initial) = &entries[1].kind else {
            return Err("capped SDK initial descriptor".into());
        };
        let replacement = match scenario.replacement_kind {
            ReplacementKind::Inline => {
                Content::Literal(sdk_edit_common::replacement_bytes(&scenario))
            }
            ReplacementKind::Zero => Content::Zero {
                len: scenario.replacement_len,
            },
        };
        entries[1].kind =
            EntryKind::File(initial.splice(scenario.start, scenario.delete_len, replacement)?);
    }
    common::validate_entries(&entries)?;
    Ok(entries)
}

pub(crate) fn fixture_sha256(case: &Case) -> Result<&'static str> {
    let scenario = sdk_scenario(case)?;
    let index = INPUT_BYTES
        .iter()
        .position(|len| *len == scenario.fixture_bytes)
        .ok_or("capped input length")?;
    Ok(INPUT_SHA256[index])
}

pub(crate) fn apply(_case: &Case, _repetition: u8, _step: usize, _verify: bool) -> Result<Receipt> {
    Err("capped SDK edits require the singular public SDK call; no ordinary workload route".into())
}

pub(crate) fn self_check() -> Result<()> {
    let registry = registry();
    if registry.len() != 5
        || sdk_edit_common::sha256_hex(sdk_edit_common::registry_tsv(&registry).as_bytes())
            != DEFINITION_MANIFEST_SHA256
    {
        return Err("capped SDK registry manifest".into());
    }
    for (index, scenario) in registry.iter().enumerate() {
        if scenario.plan_sha256 != PLAN_SHA256[index]
            || scenario.final_bytes != 524_288_000
            || scenario
                .start
                .checked_add(scenario.delete_len)
                .is_none_or(|end| end > scenario.fixture_bytes)
            || scenario.replacement_len != 4096
        {
            return Err("capped SDK plan/byte topology".into());
        }
        let logical_replacement = match scenario.replacement_kind {
            ReplacementKind::Inline => sdk_edit_common::replacement_bytes(scenario),
            ReplacementKind::Zero => vec![0; scenario.replacement_len as usize],
        };
        if sdk_edit_common::sha256_hex(&logical_replacement) != scenario.payload_sha256 {
            return Err("capped SDK original replacement identity".into());
        }
        let case = &cases()[index];
        for repetition in 1..=5 {
            if common::validate_entries(&fixture(case, repetition)?)? != scenario.fixture_bytes
                || common::validate_entries(&expected(case, repetition, 1)?)?
                    != scenario.final_bytes
            {
                return Err("capped SDK fixture/result bounds".into());
            }
        }
        if fixture(case, 0).is_ok()
            || fixture(case, 6).is_ok()
            || expected(case, 1, 2).is_ok()
            || apply(case, 1, 1, false).is_ok()
        {
            return Err("capped SDK selection/route rejection".into());
        }
    }
    for (rotation, expected) in edit_length_changing::ROTATIONS
        .into_iter()
        .zip(REPETITION_ORDER)
    {
        let mut projected =
            (0..32)
                .map(|offset| (offset + rotation) % 32)
                .filter_map(|old_index| {
                    ORIGINAL_OPERATIONS
                        .iter()
                        .position(|operation| operation * 4 + 3 == old_index)
                });
        if !projected.by_ref().eq(expected) {
            return Err("capped inherited sample order".into());
        }
    }
    Ok(())
}
