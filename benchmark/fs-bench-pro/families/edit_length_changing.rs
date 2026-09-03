use super::sdk_edit_common::{self, Operation, ReplacementKind, Scenario};

pub(crate) const FAMILY_ID: &str = "edit_length_changing";
pub(crate) const DEFINITION_MANIFEST_SHA256: &str =
    "b6e8d0ab87a2ed72234623198994a460484bd950a04bb81a99a9aecda06c4390";
pub(crate) const ROTATIONS: [usize; 5] = [0, 13, 26, 7, 20];

fn insert(len: u64) -> (u64, u64) {
    (len / 2, 0)
}
fn delete(len: u64) -> (u64, u64) {
    (len / 2 - 2048, 4096)
}
fn append(len: u64) -> (u64, u64) {
    (len, 0)
}
fn prepend(_: u64) -> (u64, u64) {
    (0, 0)
}
fn grow(len: u64) -> (u64, u64) {
    (len / 2 - 1024, 2048)
}
fn shrink(len: u64) -> (u64, u64) {
    (len / 2 - 2048, 4096)
}
fn truncate(len: u64) -> (u64, u64) {
    (len - 4096, 4096)
}
fn zero_extend(len: u64) -> (u64, u64) {
    (len, 0)
}

pub(crate) const OPERATIONS: [Operation; 8] = [
    Operation {
        key: "insert-middle-4k",
        replacement_kind: ReplacementKind::Inline,
        replacement_len: 4096,
        payload_seed: 6_313_238_748_831_594_097,
        payload_sha256: "568c5408a3f292d4a593d5ffa43736b790b6a5dac749427b0ad53c765e672616",
        locate: insert,
    },
    Operation {
        key: "delete-middle-4k",
        replacement_kind: ReplacementKind::Inline,
        replacement_len: 0,
        payload_seed: 14_631_710_363_380_426_233,
        payload_sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        locate: delete,
    },
    Operation {
        key: "append-tail-4k",
        replacement_kind: ReplacementKind::Inline,
        replacement_len: 4096,
        payload_seed: 10_524_769_729_031_953_950,
        payload_sha256: "50495bfeedccb8983ead82f1fc3a55b7a45bb5741ecc79970b8a846616f95d22",
        locate: append,
    },
    Operation {
        key: "prepend-head-4k",
        replacement_kind: ReplacementKind::Inline,
        replacement_len: 4096,
        payload_seed: 11_539_886_650_128_519_955,
        payload_sha256: "a675326972c1cb5168b42d324036fab260ccb91b9df982eaace85cd05682cbdb",
        locate: prepend,
    },
    Operation {
        key: "replace-grow-middle-2k-to-4k",
        replacement_kind: ReplacementKind::Inline,
        replacement_len: 4096,
        payload_seed: 6_297_716_278_452_303_078,
        payload_sha256: "5d682316189ab7e945b298632c929e67f90a2b1aa13987181aef3f501421d93e",
        locate: grow,
    },
    Operation {
        key: "replace-shrink-middle-4k-to-2k",
        replacement_kind: ReplacementKind::Inline,
        replacement_len: 2048,
        payload_seed: 1_824_427_086_451_703_536,
        payload_sha256: "5e24d5a26d23669833f39b2c5b3c9f6a3620ba16d3505c710020d31704a8744b",
        locate: shrink,
    },
    Operation {
        key: "truncate-tail-4k",
        replacement_kind: ReplacementKind::Inline,
        replacement_len: 0,
        payload_seed: 9_706_727_036_258_497_900,
        payload_sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        locate: truncate,
    },
    Operation {
        key: "zero-extend-tail-4k",
        replacement_kind: ReplacementKind::Zero,
        replacement_len: 4096,
        payload_seed: 7_852_731_424_507_589_290,
        payload_sha256: "ad7facb2586fc6e966c004d7d1d16b024f5805ff7cb47c7a85dabd8b48892ca7",
        locate: zero_extend,
    },
];

pub(crate) fn registry() -> Vec<Scenario> {
    sdk_edit_common::scenarios(FAMILY_ID, &OPERATIONS)
}

pub(crate) fn self_check() -> Result<(), String> {
    let rows = registry();
    sdk_edit_common::validate_registry(&rows, 32)?;
    if sdk_edit_common::sha256_hex(sdk_edit_common::registry_tsv(&rows).as_bytes())
        != DEFINITION_MANIFEST_SHA256
    {
        return Err("length-changing registry manifest".into());
    }
    if rows.iter().any(|row| {
        row.final_bytes == row.fixture_bytes
            || match row.replacement_kind {
                ReplacementKind::Inline => {
                    sdk_edit_common::sha256_hex(&sdk_edit_common::replacement_bytes(row))
                        != row.payload_sha256
                }
                ReplacementKind::Zero => {
                    sdk_edit_common::sha256_hex(&vec![0; row.replacement_len as usize])
                        != row.payload_sha256
                }
            }
    }) {
        return Err("length-changing definition".into());
    }
    Ok(())
}
