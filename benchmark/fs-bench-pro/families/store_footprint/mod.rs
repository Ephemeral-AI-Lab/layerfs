pub(crate) const FAMILY_ID: &str = "store_footprint";
pub(crate) const FIXTURE_SCHEMA: &str = "fs-bench-pro-store-footprint-fixture-v1";
pub(crate) const PERFORMANCE_SCHEMA: &str = "fs-bench-pro-store-footprint-performance-v5";
pub(crate) const VERIFICATION_SCHEMA: &str = "fs-bench-pro-store-footprint-verification-v4";
pub(crate) const LOGICAL_BYTES: u64 = 500_000_000;
pub(crate) const PRIMARY_CONTROL_ID: &str = "store-footprint-unique-100000";
pub(crate) const SEEDS: [u8; 3] = [1, 2, 3];
pub(crate) const DIAGNOSTIC_TIERS: [u64; 3] = [100, 1_000, 10_000];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    Unique,
    MetadataCardinality,
    LargeObject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Control {
    pub(crate) id: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) kind: Kind,
    pub(crate) infra_tier: u64,
    pub(crate) regular_files: u64,
    pub(crate) logical_bytes: u64,
}

pub(crate) const CONTROLS: [Control; 6] = [
    Control {
        id: "store-footprint-unique-100000",
        display_name: "100,000-file unique-content Store footprint",
        kind: Kind::Unique,
        infra_tier: 100_000,
        regular_files: 100_000,
        logical_bytes: LOGICAL_BYTES,
    },
    Control {
        id: "store-footprint-metadata-cardinality-100000",
        display_name: "100,000-file metadata-cardinality Store footprint",
        kind: Kind::MetadataCardinality,
        infra_tier: 100_000,
        regular_files: 100_000,
        logical_bytes: LOGICAL_BYTES,
    },
    Control {
        id: "store-footprint-large-object-500m",
        display_name: "500 MB large-object Store footprint",
        kind: Kind::LargeObject,
        infra_tier: 100_000,
        regular_files: 100,
        logical_bytes: LOGICAL_BYTES,
    },
    Control {
        id: "store-footprint-unique-100-low-v1",
        display_name: "100-file / 5 MB unique-content Store footprint",
        kind: Kind::Unique,
        infra_tier: 100,
        regular_files: 100,
        logical_bytes: 5_000_000,
    },
    Control {
        id: "store-footprint-metadata-cardinality-100-low-v1",
        display_name: "100-file / 5 MB metadata-cardinality Store footprint",
        kind: Kind::MetadataCardinality,
        infra_tier: 100,
        regular_files: 100,
        logical_bytes: 5_000_000,
    },
    Control {
        id: "store-footprint-large-object-10m-low-v1",
        display_name: "10-file / 10 MB large-object Store footprint",
        kind: Kind::LargeObject,
        infra_tier: 10,
        regular_files: 10,
        logical_bytes: 10_000_000,
    },
];

pub(crate) fn control(id: &str) -> Result<Control, String> {
    CONTROLS
        .into_iter()
        .find(|control| control.id == id)
        .ok_or_else(|| format!("unknown Store-footprint control: {id}"))
}

pub(crate) fn self_check() -> Result<(), String> {
    if FAMILY_ID != "store_footprint"
        || FIXTURE_SCHEMA != "fs-bench-pro-store-footprint-fixture-v1"
        || PERFORMANCE_SCHEMA != "fs-bench-pro-store-footprint-performance-v5"
        || VERIFICATION_SCHEMA != "fs-bench-pro-store-footprint-verification-v4"
        || CONTROLS.len() != 6
        || CONTROLS.iter().enumerate().any(|(index, control)| {
            control.id.is_empty()
                || control.display_name.is_empty()
                || CONTROLS[..index].iter().any(|prior| prior.id == control.id)
        })
        || LOGICAL_BYTES != 500_000_000
        || CONTROLS[0].id != PRIMARY_CONTROL_ID
        || SEEDS != [1, 2, 3]
        || DIAGNOSTIC_TIERS != [100, 1_000, 10_000]
        || CONTROLS.iter().any(|control| {
            control.regular_files == 0 || control.logical_bytes == 0 || control.infra_tier == 0
        })
        || CONTROLS[3..]
            .iter()
            .any(|control| control.regular_files > 1_000 || control.logical_bytes > 50_000_000)
        || CONTROLS[..3]
            .iter()
            .any(|control| control.infra_tier != 100_000 || control.logical_bytes != LOGICAL_BYTES)
    {
        return Err("Store-footprint family identity".into());
    }
    Ok(())
}
