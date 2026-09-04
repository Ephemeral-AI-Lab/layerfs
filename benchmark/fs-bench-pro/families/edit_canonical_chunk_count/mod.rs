use super::sdk_edit_common::{self, Operation, ReplacementKind, Scenario, SIZES};

pub(crate) const FAMILY_ID: &str = "edit_canonical_chunk_count";
pub(crate) const DEFINITION_MANIFEST_SHA256: &str =
    "e76f9b08f7312abf0f30447765e9ff734cecd6c41210788bd4917286059158bf";
pub(crate) const ROTATIONS: [usize; 5] = [0, 4, 8, 1, 5];
pub(crate) const START: u64 = 147_456;
pub(crate) const LEN: u64 = 65_536;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Expected {
    pub(crate) initial_count: u64,
    pub(crate) final_count: u64,
    pub(crate) final_sha256: &'static str,
    pub(crate) file_root: &'static str,
    pub(crate) map_sha256: &'static str,
}

fn fixed(_: u64) -> (u64, u64) {
    (START, LEN)
}

pub(crate) const OPERATIONS: [Operation; 3] = [
    Operation {
        key: "overwrite-fixed-64k-chunk-count-preserve",
        replacement_kind: ReplacementKind::Inline,
        replacement_len: LEN,
        payload_seed: 4,
        payload_sha256: "6403e9f46c8e5034759add37d4d64ecffbeee1f26b719809d4f04a1e02864978",
        locate: fixed,
    },
    Operation {
        key: "overwrite-fixed-64k-chunk-count-increase",
        replacement_kind: ReplacementKind::Inline,
        replacement_len: LEN,
        payload_seed: 2,
        payload_sha256: "ba71e3adc4ce9f1645d8f622f6c2600ca8236146f1d6817fce183f80170dade0",
        locate: fixed,
    },
    Operation {
        key: "overwrite-fixed-64k-chunk-count-decrease",
        replacement_kind: ReplacementKind::Inline,
        replacement_len: LEN,
        payload_seed: 0,
        payload_sha256: "de2f256064a0af797747c2b97505dc0b9f3df0de4f489eac731c23ae9ca9cc31",
        locate: fixed,
    },
];

const PLAN_SHA256: [&str; 12] = [
    "98670b9838650ba214b4d03a0a17c07bee03aaaa0cf98620653e1ac26ad498dc",
    "c711f36b9c0007beb924f7281baddc20e6c638f65ed921ce8f89a31643d69ed8",
    "8ae7d9acf7a126c6ced5b50f472590b12186a9c86f4c1f78d94c7ee02d16cbb3",
    "38b01ee197b5d7ebda259af756e91717b5e60fa81f3c72ecd6a354d1b456a4b0",
    "79d0ec79d07f8a0d3734e1a666c6bc6f3d8e7495b60380a4adafd49b0aac4008",
    "8313dc5ebc5ee365a2d4446f0b62d4a24ba7285afcbcb294729a88eae3677b01",
    "62fd7352d7faf0829ce7794fda7f263da51f9051cd5692059ec29e5a2b108382",
    "331528a1ee71328f87cab410eb5302a0f083bdeee9986ddce61cc3d2db0bc2fd",
    "e5446d891dde9d5ae6db3e2b577e2932b1facbe78c8c1d9c6cfe068dbb5b05c3",
    "029a74fa27a49b21982fb61387da2917a52d0a71957034e9c4263b6ef7f4c63d",
    "8fc6cf7a01c72f01878cb4a89e8d001005f7c2ce41ed8fa8c9150efc709faee8",
    "a3ac2301c04c3a040b448e05d078df1bbc499cfb38bc71f32ec6e2ccdf57bb1d",
];

const EXPECTED: [[Expected; 4]; 3] = [
    [
        Expected { initial_count: 54, final_count: 54, final_sha256: "a3374b0be7c654cf87f2b8d411d657e170c821837dcce8661759aa5fe1fc7070", file_root: "644ab1b651adc897f95da15461e32e587565b7e2789b377524c8e88aaf03e4a6", map_sha256: "4c3514314a7daf61074e6b3a3093fb3beb07699a296d87b30e5e7ec316dea714" },
        Expected { initial_count: 544, final_count: 544, final_sha256: "231b62f873d1c1b498809d40bd92235a5cdf08150abaf802422e109fee490fcc", file_root: "feef5c7528ffb92220caca77fdd89d2d1cce257e977cc204cef1e676ade6e493", map_sha256: "d38ce6c671b657acee99b8a8848efd653a91dc5bc67005e6e38e9f2e42b96ec1" },
        Expected { initial_count: 5_394, final_count: 5_394, final_sha256: "f8e906873405662688d8c8add82abef06155c084f84957f0043103fc55d909f3", file_root: "edfd0588251dddcb7fbbd4993a18d9d10e96d43c2ba07f3fa9c11389cd2e88cc", map_sha256: "6a6ffdee08de34fd91ff016ec43309913930612585695f6ffc7ca40667a5c82b" },
        Expected { initial_count: 26_995, final_count: 26_995, final_sha256: "5d8205919a2abe3f7c51f1592ceed0977b39fa2c7e6a4568b541e6fa9ed51437", file_root: "728adcbeb98753983afe97b0c2fde4d92251e044e6330d7919012879bc9a1c39", map_sha256: "1fbf938ddc01cf94487c897c1e001268412f42cafec46e16488554d1178f6afc" },
    ],
    [
        Expected { initial_count: 54, final_count: 55, final_sha256: "b320ec162166c71532c93f1013e42b6d0beb9f194c467e043c07057f55101055", file_root: "3756ef696b53388b234cc2c5877240c82a2ed660e9a462e86168bf4ebd9c4fcd", map_sha256: "98eb01c06366048bf9e53da461bd01da68f7f2af182fc7420f6b1befab5b3c22" },
        Expected { initial_count: 544, final_count: 545, final_sha256: "a5afa52cc6527313281971d1bb816d593cb173abf024a09689406a9b7afe01b5", file_root: "e24afbe82dcb11d8d6084b77d519cf342127d29c4cad7db74fd275ae6ca3fc4d", map_sha256: "2a5fc94a51cbeef53d196210d5118735f3a8d016f5c0bec198afdf3703965ad6" },
        Expected { initial_count: 5_394, final_count: 5_395, final_sha256: "b95945fead470121e03fa4d1e640582a5d2a2ae63d66ff66b537ce407e408c7d", file_root: "1b5c0ee5c643aaea649b7a76f1b325093f4887ce0e6fddfce1e221f2d5826567", map_sha256: "1edb462942b53fbf3fb8353ec6f9bced7f23d7ba7628b067a03655a657e3b450" },
        Expected { initial_count: 26_995, final_count: 26_996, final_sha256: "3accd704e6596ce90622d134a35591efe89f6ccb6e84a3d88b5e4aea6378bfd4", file_root: "95fc3c70f8ea88cb3bee08ed9aa9c0fc4aae28eeac480f9d323c0665ddd9dd67", map_sha256: "db2bebd5ac72048df698a4f6d4b42de4e07c66275397695fa5ee117639fe13b0" },
    ],
    [
        Expected { initial_count: 54, final_count: 53, final_sha256: "dab7df0938e609ac80dddfc7fb6c0ed0a3e2643d5eb9181197cdd0e185920ed6", file_root: "d0b2112fb3ce304634515ec0126759d4d54ce41ddd6ec6772646286445af35dc", map_sha256: "5d10305e079c3d9c458a25bcfdf159df9d6a2aeb459262d16d852b3170370b85" },
        Expected { initial_count: 544, final_count: 543, final_sha256: "94d1d712c610775c38ae1be46233ff351e9104ce6af67543d4be3b6c5b8f0d4d", file_root: "3a36a0494a2ad58411826e27d75bfaaa1970594efc06bc165496323256f71552", map_sha256: "4610f5bfef022392b9edabd15b66bf70bf6f03f35ed0480e3e29d2378b28e238" },
        Expected { initial_count: 5_394, final_count: 5_393, final_sha256: "0865e1e1cdef049bfb49b5888808fdf87b1add29c45c7da55ff0c5d1f43db961", file_root: "e7405b58a6e108d5cb4f7949766cb43a25b755a71233fb39e5694331950d41f3", map_sha256: "6acf193d7fe65d835a09dbe2576015c1d14d8a2814cb599a31666727390e520f" },
        Expected { initial_count: 26_995, final_count: 26_994, final_sha256: "af536c25d5ee02671afa6eb0194534973d7f1595b6a84cf82acbf5305e7a145d", file_root: "b3c43de62a637318f417a091cc82fe3b9c5bccf9d61d33127207182614e14e2e", map_sha256: "3d81624ed5a969b832d45fd97fc39a8acad32863a8fead9a6dbc62ad6239be15" },
    ],
];

pub(crate) fn registry() -> Vec<Scenario> {
    sdk_edit_common::scenarios(FAMILY_ID, &OPERATIONS)
}

pub(crate) fn expected(row: &Scenario) -> Expected {
    let operation = OPERATIONS
        .iter()
        .position(|operation| operation.key == row.operation_key)
        .expect("canonical operation");
    let size = SIZES
        .iter()
        .position(|size| *size == row.fixture_bytes)
        .expect("canonical size");
    EXPECTED[operation][size]
}

pub(crate) fn self_check() -> Result<(), String> {
    let rows = registry();
    sdk_edit_common::validate_registry(&rows, 12)?;
    if sdk_edit_common::sha256_hex(sdk_edit_common::registry_tsv(&rows).as_bytes())
        != DEFINITION_MANIFEST_SHA256
    {
        return Err("canonical registry manifest".into());
    }
    for (index, row) in rows.iter().enumerate() {
        let result = expected(row);
        let delta = result.final_count as i64 - result.initial_count as i64;
        if row.plan_sha256 != PLAN_SHA256[index]
            || row.final_bytes != row.fixture_bytes
            || sdk_edit_common::sha256_hex(&sdk_edit_common::replacement_bytes(row))
                != row.payload_sha256
            || delta.signum() != [0, 1, -1][index / 4]
        {
            return Err(format!("canonical definition: {}", row.id));
        }
    }
    Ok(())
}
