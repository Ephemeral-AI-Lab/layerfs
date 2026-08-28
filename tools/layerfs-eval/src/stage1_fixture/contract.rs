use layerfs_core::ObjectId;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub type EvalResult<T> = Result<T, String>;
pub type RootId = ObjectId;

pub const FILE_BYTES: u64 = 104_857_600;
pub const BUFFER_BYTES: usize = 1_048_576;
pub const RANDOM_RANGE_BYTES: u64 = 65_536;
pub const EXPECTED_RAW_DIGEST: &str =
    "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7";
pub const EXPECTED_CDC_REFERENCES: u64 = 5_284;
pub const EXPECTED_CDC_SEQUENCE: &str =
    "5bb376c3c54d8724973a7b160acab599f2f5cee4b4a56e855ff0cbe987425994";
pub const FIXTURE_VERSION: &str = "single-100m-v1";
pub const FILE_PATH: &str = "S1-100.bin";
pub(super) const RETAINED_SEED: u64 = 0x4c41_5945_5253_4653;
pub(super) const LABEL: &str = "S1-100";
pub(super) const BASES: &[&str] = &[
    "read-reconstruct",
    "import-genesis",
    "replace-existing",
    "overwrite",
    "insert",
    "delete",
    "append",
    "truncate",
    "refresh-a-b",
    "history",
];

#[derive(Clone, Debug)]
pub struct BaseManifest {
    pub name: String,
    pub root: RootId,
    pub root_a: Option<RootId>,
    pub root_b: Option<RootId>,
    pub generation: u64,
    pub selector_generation: u64,
    pub store_id: String,
    pub profile_id: String,
    pub store_database_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct Master {
    pub raw_digest: String,
    pub replacement_digest: String,
    pub inventory_digest: String,
    pub new_file_aggregate_rope_references: u64,
    pub bases: BTreeMap<String, BaseManifest>,
}

#[derive(Clone, Debug, Default)]
pub struct CloneReceipt {
    /// Complete reset admission, including clone custody and selector checks.
    pub wall_ns: u128,
    /// `/bin/cp -cR` return wall inside the complete reset.
    pub clone_wall_ns: u128,
    pub source_logical_bytes: u64,
    pub destination_logical_bytes: u64,
    pub source_allocated_bytes: u64,
    pub destination_allocated_bytes: u64,
    pub distinct_regular_inodes: u64,
    pub clone_id: u64,
}

#[derive(Debug)]
pub struct Attempt {
    pub(super) root: PathBuf,
    pub(super) store: PathBuf,
    pub(super) marker: String,
    pub clone: CloneReceipt,
}

#[derive(Clone, Debug)]
pub struct Selector {
    pub generation: u64,
    pub store_id: String,
    pub profile_id: String,
}
