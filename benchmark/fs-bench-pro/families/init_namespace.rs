pub(crate) const FAMILY_ID: &str = "init_namespace";
pub(crate) const PERFORMANCE_SCHEMA: &str = "fs-bench-pro-edit-performance-v1";
pub(crate) const VERIFICATION_SCHEMA: &str = "fs-bench-pro-edit-verification-v1";
pub(crate) const NAMESPACE_SCHEMA: &str = "fs-bench-pro-namespace-v3";
pub(crate) const NAMESPACE_FIXTURE_SCHEMA: &str = "fs-bench-pro-namespace-fixture-v2";
pub(crate) const NAMESPACE_FAILURE_SCHEMA: &str = "fs-bench-pro-namespace-failure-v3";
pub(crate) const NAMESPACE_FIXTURE_PROFILE: &str = "synthetic-small-heavy-v2";
pub(crate) const NAMESPACE_DIGEST_PROFILE: &str = "namespace-file-digest-tree-v2";
pub(crate) const NAMESPACE_EDIT_CONTRACT: &str = "content-only-normalized-mtime-v1";
pub(crate) const NAMESPACE_LIFECYCLE_PROFILE: &str = "commit-head-exact-reopen-v2";
pub(crate) const NAMESPACE_INIT_DIAGNOSTIC_PROFILE: &str = "initialization-only-diagnostic-v1";
pub(crate) const NAMESPACE_FILES_PER_DIRECTORY: u64 = 100;
pub(crate) const NAMESPACE_ANCHOR_BYTES: u64 = 100_000_000;
pub(crate) const NAMESPACE_EDIT_MARKER: &[u8] = b"E000000001";
pub(crate) const NAMESPACE_FILE_MODE: u32 = 0o640;
pub(crate) const NAMESPACE_DIRECTORY_MODE: u32 = 0o750;
pub(crate) const NAMESPACE_MTIME_SECONDS: i64 = 1_700_000_000;
pub(crate) const NAMESPACE_MTIME_NANOSECONDS: u32 = 0;
pub(crate) const SEEDS: [u8; 3] = [1, 2, 3];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NamespaceScenario {
    pub(crate) id: &'static str,
    pub(crate) alias: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) regular_files: u64,
    pub(crate) data_directories: u64,
    pub(crate) logical_bytes: u64,
    pub(crate) anchor_files: u64,
    pub(crate) empty_files: u64,
    pub(crate) tiny_files: u64,
    pub(crate) small_files: u64,
    pub(crate) medium_files: u64,
}

pub(crate) const NAMESPACE_SCENARIOS: [NamespaceScenario; 4] = [
    NamespaceScenario {
        id: "namespace-100",
        alias: "namespace-100-files-125mb",
        display_name: "Initialize 100 files / 125 MB",
        regular_files: 100,
        data_directories: 1,
        logical_bytes: 125_000_000,
        anchor_files: 1,
        empty_files: 1,
        tiny_files: 78,
        small_files: 15,
        medium_files: 5,
    },
    NamespaceScenario {
        id: "namespace-1000",
        alias: "namespace-1000-files-200mb",
        display_name: "Initialize 1,000 files / 200 MB",
        regular_files: 1_000,
        data_directories: 10,
        logical_bytes: 200_000_000,
        anchor_files: 1,
        empty_files: 10,
        tiny_files: 789,
        small_files: 150,
        medium_files: 50,
    },
    NamespaceScenario {
        id: "namespace-10000",
        alias: "namespace-10000-files-300mb",
        display_name: "Initialize 10,000 files / 300 MB",
        regular_files: 10_000,
        data_directories: 100,
        logical_bytes: 300_000_000,
        anchor_files: 1,
        empty_files: 100,
        tiny_files: 7_899,
        small_files: 1_500,
        medium_files: 500,
    },
    NamespaceScenario {
        id: "namespace-100000",
        alias: "namespace-100000-files-500mb",
        display_name: "Initialize 100,000 files / 500 MB",
        regular_files: 100_000,
        data_directories: 1_000,
        logical_bytes: 500_000_000,
        anchor_files: 2,
        empty_files: 1_000,
        tiny_files: 78_998,
        small_files: 15_000,
        medium_files: 5_000,
    },
];

pub(crate) fn namespace_scenario(id_or_alias: &str) -> Result<NamespaceScenario, String> {
    NAMESPACE_SCENARIOS
        .into_iter()
        .find(|scenario| scenario.id == id_or_alias || scenario.alias == id_or_alias)
        .ok_or_else(|| format!("unknown namespace scenario: {id_or_alias}"))
}

pub(crate) fn self_check() -> Result<(), String> {
    let expected = [
        (
            "namespace-100",
            "namespace-100-files-125mb",
            100,
            125_000_000,
        ),
        (
            "namespace-1000",
            "namespace-1000-files-200mb",
            1_000,
            200_000_000,
        ),
        (
            "namespace-10000",
            "namespace-10000-files-300mb",
            10_000,
            300_000_000,
        ),
        (
            "namespace-100000",
            "namespace-100000-files-500mb",
            100_000,
            500_000_000,
        ),
    ];
    for (index, (id, alias, files, bytes)) in expected.into_iter().enumerate() {
        let scenario = NAMESPACE_SCENARIOS[index];
        if scenario.id != id
            || scenario.alias != alias
            || scenario.regular_files != files
            || scenario.logical_bytes != bytes
            || namespace_scenario(id)? != scenario
            || namespace_scenario(alias)? != scenario
        {
            return Err("namespace registry order, identity, or alias".into());
        }
    }
    for (index, scenario) in NAMESPACE_SCENARIOS.iter().enumerate() {
        if NAMESPACE_SCENARIOS[..index]
            .iter()
            .any(|earlier| earlier.id == scenario.id || earlier.alias == scenario.alias)
        {
            return Err("duplicate namespace ID or alias".into());
        }
    }
    if FAMILY_ID != "init_namespace"
        || PERFORMANCE_SCHEMA != "fs-bench-pro-edit-performance-v1"
        || VERIFICATION_SCHEMA != "fs-bench-pro-edit-verification-v1"
        || SEEDS != [1, 2, 3]
    {
        return Err("namespace family identity, schema, or seeds".into());
    }
    Ok(())
}
