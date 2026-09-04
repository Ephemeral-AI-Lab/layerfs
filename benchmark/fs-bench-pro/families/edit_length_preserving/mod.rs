use super::sdk_edit_common::{self, Operation, ReplacementKind, Scenario};

pub(crate) const FAMILY_ID: &str = "edit_length_preserving";
pub(crate) const DEFINITION_MANIFEST_SHA256: &str =
    "daa3bcb8ba94da6dc28f7ca87dc2b27612c9988cf42fe5398cdddb3a5b386324";
pub(crate) const ROTATIONS: [usize; 5] = [0, 5, 10, 3, 8];

fn head(_: u64) -> (u64, u64) {
    (0, 4096)
}

fn middle(len: u64) -> (u64, u64) {
    (len / 2 - 2048, 4096)
}

fn tail(len: u64) -> (u64, u64) {
    (len - 4096, 4096)
}

pub(crate) const OPERATIONS: [Operation; 3] = [
    Operation {
        key: "overwrite-head-4k",
        replacement_kind: ReplacementKind::Inline,
        replacement_len: 4096,
        payload_seed: 3_686_764_519_212_284_394,
        payload_sha256: "faca857b3e7f8b1b8f46c19b1625a1b9995248ae9ac85eae672b85fbc9932375",
        locate: head,
    },
    Operation {
        key: "overwrite-middle-4k",
        replacement_kind: ReplacementKind::Inline,
        replacement_len: 4096,
        payload_seed: 10_800_757_348_883_211_881,
        payload_sha256: "f2ebe7d18fdbdd17c8aaff760ad8eeb0dfd82dadf874c41dad175ff916b2a6c5",
        locate: middle,
    },
    Operation {
        key: "overwrite-tail-4k",
        replacement_kind: ReplacementKind::Inline,
        replacement_len: 4096,
        payload_seed: 3_866_307_116_232_060_780,
        payload_sha256: "37cf5837649769db2eccb504f2af96c69b9e514304374ed7d74c3b1299c2f385",
        locate: tail,
    },
];

pub(crate) fn registry() -> Vec<Scenario> {
    sdk_edit_common::scenarios(FAMILY_ID, &OPERATIONS)
}

pub(crate) fn self_check() -> Result<(), String> {
    let rows = registry();
    sdk_edit_common::validate_registry(&rows, 12)?;
    if sdk_edit_common::sha256_hex(sdk_edit_common::registry_tsv(&rows).as_bytes())
        != DEFINITION_MANIFEST_SHA256
    {
        return Err("length-preserving registry manifest".into());
    }
    if rows.iter().any(|row| {
        row.final_bytes != row.fixture_bytes
            || sdk_edit_common::sha256_hex(&sdk_edit_common::replacement_bytes(row))
                != row.payload_sha256
    }) {
        return Err("length-preserving definition".into());
    }
    Ok(())
}
