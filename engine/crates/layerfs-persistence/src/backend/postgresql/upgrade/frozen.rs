//! Step 3 -> 4's frozen texts (#804 rule 5): the schema-3 source every
//! schema-3 build wrote after the `25c261b205` revert (through dcd0fa2c30),
//! with or without Init's acquisition tables, its live column definition,
//! and the schema-4 source and column definition the step produces
//! (`attempt` appended after `history_serial`, where `ADD COLUMN` puts it).
//! Shipped texts copied once and never edited (rule 9).
use sha2::Digest;

const V3_SCHEMA: &str = include_str!("../../../../sql/upgrade/postgresql/v3/schema.sql");
const V3_GC: &str = include_str!("../../../../sql/upgrade/postgresql/v3/gc.sql");
const V3_ACQUISITION: &str = include_str!("../../../../sql/upgrade/postgresql/v3/acquisition.sql");
const V3_COLUMNS: &str = include_str!("../../../../sql/upgrade/postgresql/v3/columns.txt");
/// Schema 4's Store tables; its GC and acquisition scripts are schema 3's.
const V4_SCHEMA: &str = include_str!("../../../../sql/upgrade/postgresql/v4/schema.sql");
const V4_COLUMNS: &str = include_str!("../../../../sql/upgrade/postgresql/v4/columns.txt");

/// The recorded-source variant step 3 -> 4 accepts.
pub(crate) const VARIANT: &str = "25c261b205";

/// The version CHECKs the step drops and re-adds, by their generated names.
pub(crate) const VERSION_CHECKS: [(&str, &str); 2] = [
    ("history_meta", "history_meta_schema_version_check"),
    ("store_policy", "store_policy_schema_version_check"),
];

/// The schema-3 source, with or without the acquisition tables.
pub(crate) fn source_v3(acquisition: bool) -> String {
    match acquisition {
        true => format!("{V3_SCHEMA}{V3_GC}{V3_ACQUISITION}"),
        false => format!("{V3_SCHEMA}{V3_GC}"),
    }
}

/// The schema-4 source the step records: what a fresh schema-4 Store records.
pub(crate) fn source_v4(acquisition: bool) -> String {
    match acquisition {
        true => format!("{V4_SCHEMA}{V3_GC}{V3_ACQUISITION}"),
        false => format!("{V4_SCHEMA}{V3_GC}"),
    }
}

fn columns(text: &str, acquisition: bool) -> String {
    match acquisition {
        true => text.to_owned(),
        false => text
            .split_inclusive('\n')
            .filter(|line| !line.starts_with("init_"))
            .collect(),
    }
}

/// The live column definition of a schema-3 Store (`schema.rs` `definition`).
pub(crate) fn columns_v3(acquisition: bool) -> String {
    columns(V3_COLUMNS, acquisition)
}

/// The exact column definition step 3 -> 4 leaves.
pub(crate) fn columns_v4(acquisition: bool) -> String {
    columns(V4_COLUMNS, acquisition)
}

/// Lowercase hex sha256 of `text`, as `encode(sha256(convert_to(t,'UTF8')),'hex')`.
pub(crate) fn sha256_hex(text: &str) -> String {
    let digest = sha2::Sha256::digest(text.as_bytes());
    let mut out = String::with_capacity(64);
    for byte in digest.iter() {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}
