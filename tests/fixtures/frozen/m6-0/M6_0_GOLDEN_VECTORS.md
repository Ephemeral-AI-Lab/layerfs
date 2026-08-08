# M6.0 identity, ChangeReceiptV1, and physical-storage golden vectors

```text
DOCUMENT_ROLE: NORMATIVE_M6_0_EXECUTABLE_GOLDEN_VECTORS
VECTOR_REVISION: 2026-08-06.2
VECTOR_STATUS: FROZEN_M6_0_COMPLETE
M6_0_STATUS: COMPLETE
M6_1_STATUS: NOT_STARTED_AUTHORIZED_TO_BEGIN_LATER_EXPLICIT_EXECUTION_TASK
```

> **DOCUMENTATION-ONLY VERIFICATION TOOLING — NOT PRODUCT IMPLEMENTATION.**

These vectors are the executable byte-level examples for
[ADR-003](../../design/decisions/ADR-003-structural-version-identity.md) and
[ChangeReceiptV1](M6_0_CHANGE_RECEIPT_V1.md), plus the exact direct-to-pack,
local-path, lifecycle-lease, recovery, and journal-retirement decisions in
[SPEC.md](SPEC.md#od-06--physical-object-pack-catalog-selector-journal-and-compatibility-formats)
and the [physical-pack algorithm package](../../algorithm/07-physical-pack-storage/README.md).
The independent docs-local
[generator](m6-vectors/README.md) imports no product code. It constructs each
preimage directly from the normative grammar, computes BLAKE3-256, performs a
strict bounded reference parse, checks every expected outcome, and compares
the generated block below byte-for-byte.

Regenerate and compare without writing into the product checkout:

```sh
cd m6-vectors
CARGO_TARGET_DIR="$(mktemp -d)" cargo run --locked -- --write
CARGO_TARGET_DIR="$(mktemp -d)" cargo run --locked -- --check
```

The compact boundary rows seal their complete encoded byte streams by exact
length and BLAKE3-256 digest. The small valid vectors include every byte so an
independent implementation can reproduce the coverage and authentication
preimages directly. Hostile rows seal the complete mutated input by length and
digest and name the exact expected typed outcome. `S_*` denotes structural
identity admission; `E_*` denotes receipt rejection; `FULL_ENUM_*` denotes an
authenticated but unusable incremental receipt whose typed pre-effect result
requires caller-selected full enumeration or refusal;
`FULL_CHANGED_FILE_REQUIRED` requires a separately reserved changed-file
scan; and `RECEIPT_RESOURCE_REFUSED_*` proves unavailable, cancelled, and
deadline-expired reservations start no parse, authentication, source, or
durable effect.

The occupied-ID matrix uses an explicit forced-digest test oracle: the empty
implicit root and composite implicit root are independently valid canonical
byte streams, while the collision case reports the empty-root claimed ID for
both. Separate rows prove exact equality, recomputed-ID mismatch, cross-type
failure, malformed-object precedence, and same-ID/different-byte refusal.
This exercises the otherwise computationally infeasible collision branch
without treating malformed bytes or an ordinary digest mismatch as a
collision.

The structural oracle parses every identity domain, validates exact lengths,
UTF-8/NUL/name rules, modes, counts, order, exact EOF, logical-length
reconstruction, and registry-backed typed edges. The occupied-object oracle
validates both objects before classifying remembered inequality and compares
through 65,536-byte windows to simultaneous EOF. The receipt oracle performs
checked offset arithmetic, streams coverage and authentication state during
the same forward fact parse, and executes authority, key, epoch, custody,
immediate-revalidation, replay, resource, full-enumeration, and
full-changed-file disposition contexts.

The physical-pack oracle constructs complete `ELSOBJ01` Chunk objects, writes
records once in discovery order, separately sorts fixed 80-byte index
metadata, seals the exact trailer, and validates the record/index bijection by
offset order. It checks the prefix-origin `absolute_offset`, typed IDs,
checksums, lengths, padding, strict key uniqueness, nonoverlap, exact EOF, and
the frozen sorter arithmetic without staging payload. The path/state oracle
checks the exact journal and quarantine basenames, collision outcomes,
no-wrap law, no-I/O-under-gate lease partition, old/new/unknown recovery, and
the only durable journal-retirement crash states.

<!-- BEGIN GENERATED M6.0 VECTORS -->
## Generated structural identity vectors

| Vector | Preimage bytes | Exact preimage hex | BLAKE3-256 ID |
|---|---:|---|---|
| `logical_chunk_abc` | 25 | `455356322d4c4348554e4b0001000300000000000000616263` | `1174c050f4ebe0866002fcd0a52001f0418159dc0c1d2d98e85c14e16a13c164` |
| `logical_file_abc` | 65 | `455356322d4c46494c450001000300000000000000010000001174c050f4ebe0866002fcd0a52001f0418159dc0c1d2d98e85c14e16a13c1640300000000000000` | `c54ded3a17e29e554f21791a488787aadca8241b23e727fd5459ae42e7013d32` |
| `file_node_0644_abc` | 55 | `455356322d464e4f4445000100a401c54ded3a17e29e554f21791a488787aadca8241b23e727fd5459ae42e7013d320300000000000000` | `82204b82869a1532b0c2bddfadcfcc3fd15c2ae78dbde925367d9d49d25e56ee` |
| `symlink_node_file_txt` | 25 | `455356322d534e4f44450001000800000066696c652e747874` | `b09cb3ee0185d96abb9200d1731e74e65a3a97ffe113b14350ad88114ec15236` |
| `directory_explicit_0755_nested_file` | 60 | `455356322d444e4f4445000100ed010100000004000000646174610182204b82869a1532b0c2bddfadcfcc3fd15c2ae78dbde925367d9d49d25e56ee` | `00768e2a70807c641a519cee8544ad1038cd6c769ae4cab05e2c24bfb5b3f466` |
| `directory_implicit_empty_root_1000` | 19 | `455356322d444e4f4445000100001000000000` | `b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09` |
| `version_empty_root` | 45 | `455356322d56524f4f54000100b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09` | `44b0eb7c80a93ffc3cb98e4ff16c90d4a8549b0c7c0e86e0d3ee2a857b300963` |
| `directory_implicit_composite_root_1000` | 148 | `455356322d444e4f44450001000010030000000800000066696c652e7478740182204b82869a1532b0c2bddfadcfcc3fd15c2ae78dbde925367d9d49d25e56ee040000006c696e6b03b09cb3ee0185d96abb9200d1731e74e65a3a97ffe113b14350ad88114ec15236060000006e65737465640200768e2a70807c641a519cee8544ad1038cd6c769ae4cab05e2c24bfb5b3f466` | `70ef59cbf243c0a9c44e26001c6d3deaa01946ea2cd06a2e4b0c87ade1cadd26` |
| `version_composite` | 45 | `455356322d56524f4f5400010070ef59cbf243c0a9c44e26001c6d3deaa01946ea2cd06a2e4b0c87ade1cadd26` | `f2dfceb5f1618031b99634897ddc5c760421fcd92b53f8715b776a127e40effa` |

## Generated valid receipt vectors

### `receipt_minimal_no_change`

- Encoded bytes: `225`
- Exact receipt BLAKE3-256: `ad126d741729b0595c95a9e6a238210f0533921663a6f36e0622b68f02181cbe`
- Coverage digest: `504b8a39a95dee68a838bcb00b8c7e191a768abdeff3d98b3d37268a22f1c98b`
- Issuer authentication: `dc5c3dc1a04f6f1db4eb8b218d2357b612d0fb262933c875697f7f23c47f5a31`
- Exact bytes: `455356322d434847524543000100e1000000000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f0100000000000000202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f01000000000000000100000000000000010000000000000001000000000000000000504b8a39a95dee68a838bcb00b8c7e191a768abdeff3d98b3d37268a22f1c98b0107000000dc5c3dc1a04f6f1db4eb8b218d2357b612d0fb262933c875697f7f23c47f5a31`

### `receipt_nontrivial`

- Encoded bytes: `418`
- Exact receipt BLAKE3-256: `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603`
- Coverage digest: `a66451cb13d119609da4a72d5c9d641b2ae5ee50216f5ff9978e3c865121711d`
- Issuer authentication: `4cbd184297045f51762bd66c0e9bf3e96158800e8658b4ab82603cfe3daa5c0b`
- Exact bytes: `455356322d434847524543000100a2010000000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f0100000000000000202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f090000000000000064000000000000006600000000000000010002000000c100000059000000040303270c000000646f63732f63757272656e740b000000646f63732f6c617465737408000000000000000900000000000000ef3c75a2cb73d585144ea5540380d662d3da01327fd976b5d175ed94d1acb71b0000680000000201013e0a0000007372632f6c69622e727329000000000000002a0000000000000003000000000000000500000000000000ec4bf328d8929ba3cbbefe4fc579531993a7cc46dcdf8e762db4fb2ebc44f61a010001000000000000000200000000000000a66451cb13d119609da4a72d5c9d641b2ae5ee50216f5ff9978e3c865121711d01070000004cbd184297045f51762bd66c0e9bf3e96158800e8658b4ab82603cfe3daa5c0b`

## Generated accepted law and boundary vectors

| Vector | Encoded bytes | BLAKE3-256 | Expected |
|---|---:|---|---|
| `boundary_1048576_total_bytes` | 1048576 | `687037be41f87cbeb636d1a06278ca0a8defa4d4278b69b591f83c771c78cd14` | `ACCEPTED` |
| `boundary_16384_total_and_64_per_fact_ranges` | 283617 | `37981a21dfc05b775f2c6c782c863a1c94648f44913fa6449a0207c80090ed3a` | `ACCEPTED` |
| `boundary_4096_facts` | 241889 | `de1f94b773671ac9c76d826ea6b3676eafe52dd64c7afcb6078b9c6425bd929a` | `ACCEPTED` |
| `boundary_4096_path_bytes` | 4375 | `3766a9d308d63c0433def8c2ac4ef1cfae446c097b2ca7593376a18ca6cfa1da` | `ACCEPTED` |
| `boundary_9298_maximum_reachable_legal_fact_bytes` | 9523 | `e3a16ff4ab5496aa3e85fb01ae06631a3a0946768cfdbfe2aaab72ae2a34d2c8` | `ACCEPTED` |
| `valid_all_fact_kinds` | 878 | `dac46ca82add9b722efc11f8d5c94874b667ece2e4ea3611dba76e0e77bc407c` | `ACCEPTED` |
| `valid_closed_rename_cycle` | 361 | `0e99a48c98c981bda171a9fb09bb70d08b634f9ae3ab3798b3796221d95a31eb` | `ACCEPTED` |
| `valid_exact_idempotent_replay` | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `IDEMPOTENT_REPLAY` |
| `valid_first_receipt_in_new_epoch_sequence_one` | 225 | `ad126d741729b0595c95a9e6a238210f0533921663a6f36e0622b68f02181cbe` | `ACCEPTED` |

## Generated structural hostile vectors

| Vector | Mutated bytes | BLAKE3-256 | Expected |
|---|---:|---|---|
| `structural_chunk_unknown_schema` | 25 | `8a2ab8ed23169a11f72fb49efadb41e830f84102eb372722c3cd451687170193` | `S_SCHEMA` |
| `structural_chunk_length_mismatch` | 25 | `69ffe5b1ee666e0ae9ccf57779e73e43844baf729ae28f6628e15a2ad5f5dddb` | `S_TRUNCATED` |
| `structural_chunk_trailing_byte` | 26 | `6312c617417482ed05bfda97f920537e3ff6b04ad69e9c49865c7af35ae32709` | `S_EXACT_EOF` |
| `structural_logical_file_length_mismatch` | 65 | `488432ba01dc428e723d627cd4987eff563ab9b380b805de7f7eb0316a2ee24e` | `S_LOGICAL_LENGTH` |
| `structural_logical_file_chunk_declared_length_mismatch` | 65 | `fd24fdb654780af00b8c0ed9fdd86caac4974a379101473a0164e6ea3d1bb4f4` | `S_TYPED_EDGE` |
| `structural_logical_file_wrong_typed_edge` | 65 | `cccd3eca9af2fd6f6e37246f4c631293951242bb5451d12060b918867803a5de` | `S_TYPED_EDGE` |
| `structural_logical_file_trailing_byte` | 66 | `1d90d2a34c111699c432973652a39be11d982ff563f215ceb08e585b5863a43c` | `S_EXACT_EOF` |
| `structural_file_mode_sentinel` | 55 | `020071778a1053300019ed505cf70d6171b0cc5c318502bfe5901218a2928110` | `S_FILE_MODE` |
| `structural_file_wrong_typed_edge` | 55 | `3756d1dfe55fc8eea8cb690feed76d986930b857baee3071fb6afdb0372a1335` | `S_TYPED_EDGE` |
| `structural_file_length_mismatch` | 55 | `cc91fb6a09987d7286485cf23c9b5c4358bb9ea512c35280f6e91469ece5ec3f` | `S_TYPED_EDGE` |
| `structural_file_trailing_byte` | 56 | `db9e005099fa528b343e9fffd130abe6a0140257c9b1e29f1a9277316943a252` | `S_EXACT_EOF` |
| `structural_symlink_invalid_utf8` | 25 | `57b6268cf9f477be5c105da1fa45e6c9fa68fed87d77fc78c9cba49b912fa3fd` | `S_TARGET` |
| `structural_symlink_embedded_nul` | 25 | `eff62ece524763e77205c8d7f4a7a496a8bc1c099cacd6573596c7f703a89d53` | `S_TARGET` |
| `structural_symlink_empty` | 17 | `1eadb041192333490653b7ab5ce1c025ffa8daf7d63a9177d9b660b2a971da34` | `S_TARGET` |
| `structural_symlink_trailing_byte` | 26 | `d7c024b922cbd55563d05da582ca6c9df57874a3fad376785dc2571c9fc42cd2` | `S_EXACT_EOF` |
| `structural_root_without_sentinel` | 19 | `95cab21319778eec97e80771d12d544a3a8aadc66f238ffe3c76e4df616ccffe` | `S_ROOT_SENTINEL` |
| `structural_child_with_sentinel` | 60 | `51b398f5b6f8ac8e889b7b2905e6998b48c092f69236dd182b502348c77567c6` | `S_CHILD_MODE` |
| `structural_directory_invalid_utf8` | 60 | `5fdaf33b6100df6aa841090f0181a4a4feb7b8891e818a9721af0da1fc8c43bf` | `S_NAME` |
| `structural_directory_embedded_nul` | 60 | `4978282024c3adbbd88dbe1011648ef7d5ea87afeca079f8a801e19194e20719` | `S_NAME` |
| `structural_unknown_child_kind` | 60 | `8ed7d4862ba2eb91309da8871e76a187dab686a96ab52ca31559e8b273a19ad2` | `S_UNKNOWN_KIND` |
| `structural_directory_wrong_typed_edge` | 60 | `7fed0a64609570e3e44ce50e8bd4b8dc560b353a63b555bfb1650d80b81e0412` | `S_TYPED_EDGE` |
| `structural_implicit_root_rejected_as_explicit_child` | 60 | `d0e312f3823fc62b90f73aad896ac188e6c0fed0f39d7429c88af65076b91f1f` | `S_TYPED_EDGE` |
| `structural_directory_duplicate_name` | 95 | `386cab13ef4883b0412083d62905cefa67642f5f5792d1fb72c72a3df2a7bd46` | `S_ORDER_DUPLICATE` |
| `structural_directory_descending_name` | 95 | `eb9b1cfe591ce505bb9ffa09e9f78aa3c9c5a458417e2e2f623efe1b18ebc5e5` | `S_ORDER_DUPLICATE` |
| `structural_directory_dot_name` | 57 | `2f27eca5c83a35ed6ca050e6182dbd45dbf745cd0d19a58392213a835eb39373` | `S_NAME` |
| `structural_directory_trailing_byte` | 20 | `fa99d3ef548e3aca5ad3d5b7622024eccb623b449fb2f8bdd961e37ff8b12ffd` | `S_EXACT_EOF` |
| `structural_truncated_child_id` | 59 | `4db004e7b31425d2f8d2b5729c480fa61cfdc7b4e57578995e7677109b26ad63` | `S_TRUNCATED` |
| `structural_version_wrong_typed_edge` | 45 | `f1bc2adacbd21af50eb48b55ce4308a5f6e09fc153c7d0b6e741f191ea172aa2` | `S_TYPED_EDGE` |
| `structural_explicit_directory_rejected_as_version_root` | 45 | `c0c949f8a1b9288bf76769e467cedd576ea69d8d302f5c8df2ba0ad5b7acd722` | `S_TYPED_EDGE` |
| `structural_version_trailing_byte` | 46 | `a03747d595b69856359d898e9e533e17f374b2a1fa64a1b1f193212d7947ec90` | `S_EXACT_EOF` |
| `structural_version_wrong_domain` | 45 | `1f44bcb4c779d272b5e4787217fc5d963e8514d46ed83958477b0a9ed103568e` | `S_TYPE_DOMAIN` |

## Generated occupied-ID comparison vectors

| Vector | Oracle | Claimed ID | Recomputed expected ID | Recomputed stored ID | Expected |
|---|---|---|---|---|---|
| `occupied_exact_identical` | `BLAKE3_RECOMPUTED` | `b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09` | `b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09` | `b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09` | `ACCEPTED_IDENTICAL` |
| `occupied_same_id_different_bytes` | `FORCED_COLLISION_AFTER_RECOMPUTE` | `b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09` | `b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09` | `70ef59cbf243c0a9c44e26001c6d3deaa01946ea2cd06a2e4b0c87ade1cadd26` | `S_OCCUPIED_SAME_ID_DIFFERENT_BYTES` |
| `occupied_malformed_outranks_inequality` | `BLAKE3_RECOMPUTED` | `b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09` | `b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09` | `fa99d3ef548e3aca5ad3d5b7622024eccb623b449fb2f8bdd961e37ff8b12ffd` | `S_EXACT_EOF` |
| `occupied_cross_type_outranks_inequality` | `BLAKE3_RECOMPUTED` | `b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09` | `b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09` | `44b0eb7c80a93ffc3cb98e4ff16c90d4a8549b0c7c0e86e0d3ee2a857b300963` | `S_TYPE_DOMAIN` |
| `occupied_recomputed_id_mismatch` | `BLAKE3_RECOMPUTED` | `b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09` | `b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09` | `70ef59cbf243c0a9c44e26001c6d3deaa01946ea2cd06a2e4b0c87ade1cadd26` | `S_ID_MISMATCH` |

## Generated receipt hostile vectors

| Vector | Base | Exact mutation | Mutated bytes | BLAKE3-256 | Expected |
|---|---|---|---:|---|---|
| `receipt_total_length` | `receipt_nontrivial` | total_encoded_bytes -= 1 | 418 | `78c09277fa85ea0fe18bad15d528ea3b1a2c6e302d512423016e8b637cf27a6d` | `E_TOTAL_LENGTH` |
| `receipt_fact_count` | `receipt_nontrivial` | facts_count 2 -> 3; reseal | 418 | `4e8636403697f2935adac87fd37bcee31c04046234fa23d1fd5b00b12bd14226` | `E_FACT_COUNT` |
| `receipt_fact_count_cap` | `receipt_nontrivial` | facts_count 2 -> 4097; reseal | 418 | `fdcb5276421bb512d2bdd8a870639a2aa799f9345bda843fc057c8fa951d9dcc` | `E_FACT_COUNT_CAP` |
| `receipt_reserved_presence` | `receipt_nontrivial` | first fact presence |= 0x40; reseal | 418 | `8a5a41622c22cf18abd7477d6650c1d5365463416134130737a64a2235207252` | `E_PRESENCE` |
| `receipt_missing_required_presence` | `receipt_nontrivial` | first fact metadata presence bit cleared; reseal | 418 | `d7bf2d6899492bd1349342135af54248b280a3567e32fe8da515c2c1a8c9a991` | `E_PRESENCE` |
| `receipt_metadata_arbitrary_digest` | `receipt_nontrivial` | first CHGMETA digest byte toggled; receipt coverage/authentication resealed while immutable-source context remains unchanged | 418 | `e7cda9c2550dec4f7d6e5a9179c86af02c5465f0c52e7d411e4c37a422e21a04` | `E_METADATA` |
| `receipt_metadata_wrong_source_kind` | `receipt_nontrivial` | immutable-source metadata says DIRECTORY for the after-SYMLINK fact | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_METADATA` |
| `receipt_metadata_wrong_source_mode` | `receipt_nontrivial` | immutable-source regular-file mode is 0600 while receipt CHGMETA commits 0644 | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_METADATA` |
| `receipt_metadata_root_sentinel_mode` | `receipt_nontrivial` | immutable-source regular-file metadata attempts reserved structural root sentinel 0x1000 | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_METADATA` |
| `receipt_path_cap` | `one_create_fact` | primary path is 4097 bytes with valid framing/authentication | 4376 | `671f4e1ba694584f147eea65cd726578a28a634e61228aa72ebb2147da9a787b` | `E_PATH_CAP` |
| `receipt_path_dot_component` | `one_create_fact` | encode the named non-canonical OD-01 primary path | 280 | `08f45302ac75231e11baf81b3671ff5aa025a20d9833bba6aa2b955968bba755` | `E_PATH` |
| `receipt_path_dot_dot_component` | `one_create_fact` | encode the named non-canonical OD-01 primary path | 281 | `eee72b89dcd4ff8d8ab26f496e3cb792d43970dee920726b0405a488e72cb12d` | `E_PATH` |
| `receipt_path_leading_slash` | `one_create_fact` | encode the named non-canonical OD-01 primary path | 281 | `ef1007a89a8fc178749246022ad7ef427b669d93a6b6ef80e66857572a312aac` | `E_PATH` |
| `receipt_path_repeated_slash` | `one_create_fact` | encode the named non-canonical OD-01 primary path | 283 | `f2933e3312d8d6791b08ee5e923385affca62cc43274a57fd60f02a66bee287c` | `E_PATH` |
| `receipt_path_trailing_slash` | `one_create_fact` | encode the named non-canonical OD-01 primary path | 281 | `de098f62d1c7aafe72666b90985793cd2f99b9590ae51709875536a78130732f` | `E_PATH` |
| `receipt_path_component_length_cap` | `one_create_fact` | single primary-path component is 256 bytes | 535 | `e3e3224236022a2f6a2505e8e03dbfe49a24a5b5d51b78efae108a779477e97b` | `E_PATH` |
| `receipt_path_component_count_cap` | `one_create_fact` | primary path contains 257 components | 792 | `f39fd9deb45b4aa6854600ed44222b31b906f0ea03c213146a91a0021dcce9c3` | `E_PATH` |
| `receipt_path_invalid_utf8` | `one_create_fact` | one-byte primary path a -> ff; reseal | 280 | `5dfdf17cd204feb69e550ee86f31cbbae669b961740dae38601f9da7e8ec7302` | `E_PATH` |
| `receipt_path_embedded_nul` | `one_create_fact` | one-byte primary path a -> 00; reseal | 280 | `63440e1cbb3451e90cb001c740310bd2be00304f0e0510cb7d1234e44f310f23` | `E_PATH` |
| `receipt_fact_order` | `receipt_nontrivial` | swap the two complete fact records; reseal | 418 | `b81260e9aa65da535452a440046a169513189fa6659c15f767a24a72c0377842` | `E_FACT_ORDER` |
| `receipt_duplicate_path` | `two_create_facts` | second primary path same2 -> same1; reseal | 343 | `06935b49ea9195c8b937ba30b0d5865174455a670c5c756b9afe748fe3911046` | `E_DUPLICATE` |
| `receipt_uncoalesced_rename_chain` | `two_rename_facts` | encode a->b followed by b->c instead of the coalesced a->c fact | 361 | `608611e2400829959712853e216a6bb26f2493de9367058bc066d5048b81291a` | `E_RENAME_CHAIN` |
| `receipt_ambiguous_rename_source` | `create_and_rename_facts` | encode non-rename primary path a and rename source a | 348 | `e587ddf139e0f758af5c6ba29f447d1210bbb649d4fe8bae6d928c3608e6f529` | `E_RENAME_AMBIGUOUS` |
| `receipt_equal_entry_generation` | `one_rename_fact` | before_generation == after_generation == 1 | 297 | `79c201f2f7bb947eb8f8a1dc852240e0ad3d24719afc0f18bbfc7cb59c82dd06` | `E_ENTRY_GENERATION` |
| `receipt_rename_source_fan_out` | `two_rename_facts` | the same prior path a is the source for destinations b and c | 361 | `d42e98f5c2284274269b4598711f9d3be14df5b45c6cff9a96c379878b03fa7d` | `E_RENAME_DUPLICATE` |
| `receipt_range_overlap` | `one_modify_fact` | encode named hostile range set with valid authentication | 340 | `e2da0d14e662b69d79318dded9380a61281d082487acbf0aa918238a4814e62f` | `E_RANGE_OVERLAP` |
| `receipt_range_adjacency` | `one_modify_fact` | encode named hostile range set with valid authentication | 340 | `c0adce9f61860d9735ee62511392339080ffa97e062ddf0fb94e57e090f82a97` | `E_RANGE_ADJACENT` |
| `receipt_range_overflow` | `one_modify_fact` | encode named hostile range set with valid authentication | 324 | `9124012ed9eeb161bf0e0124969c49f3c31661b1defdf50fe96440c3e5d52de3` | `E_RANGE_OVERFLOW` |
| `receipt_range_zero` | `one_modify_fact` | encode named hostile range set with valid authentication | 324 | `cc27e030e3a2256e700ac358d42c91930a0cd4c6821033315cd81364038acdba` | `E_RANGE_ZERO` |
| `receipt_range_after_size` | `one_modify_fact` | encode named hostile range set with valid authentication | 324 | `7dd58171b54b80d95c60b2d1b17b6cd78046f08bdb53cf5d9cad34a647e2f95b` | `E_RANGE_SIZE` |
| `receipt_range_count_cap` | `one_modify_fact` | range_count=65 with valid framing/authentication | 1352 | `550b723380d5faa4574097b914f187e8c5768e2ad0ef143ec2bd63c52148e3d2` | `E_RANGE_COUNT_CAP` |
| `receipt_total_range_cap` | `257_modify_facts` | total range count=16385 with per-fact count <=64 | 286286 | `90108a9b5c09377053b20bae64abe3702f68d1273873940cefbb9c4ad1af0c08` | `E_TOTAL_RANGE_CAP` |
| `receipt_file_size_cap` | `one_modify_fact` | before_size is 8589934593 | 311 | `50af9235433d9ed63c5bde0e8ce575d62fcababd9814146fab896cfc6053f2a4` | `E_SIZE_CAP` |
| `receipt_forbidden_range_combination` | `one_metadata_fact` | directory METADATA fact carries one change range | 306 | `ca9678155e5202cb9fc45bfbf67e230e5db158e0cb03383e2bed3a481c744790` | `E_RANGE_COMBINATION` |
| `receipt_unknown_fact_kind` | `receipt_nontrivial` | first fact_kind -> 0xff; reseal | 418 | `10d1b88c17c663cad0b0d1000fdb7ac969a466fe53b46767e787ef7133771abf` | `E_FACT_KIND` |
| `receipt_unknown_entry_kind` | `receipt_nontrivial` | first before_entry_kind -> 0xff; reseal | 418 | `95c92bae70630269876274af36bab14e1581965fff95b1e249c221da057404f4` | `E_ENTRY_KIND` |
| `receipt_missing_rename_prior_path` | `receipt_nontrivial` | first RENAME fact prior_path presence cleared; reseal | 418 | `77b25e39209046d39d72f97ad74ff5d919b2f9922a303fbed3d6a41feba84c91` | `E_PRESENCE` |
| `receipt_fact_below_defensive_minimum` | `receipt_nontrivial` | first fact_encoded_bytes -> 46; reseal | 418 | `349f3ed8b5cf891a08b4fead6cc24c0fbab5393a7668f896e2b58b0ed34922d3` | `E_FACT_SIZE_CAP` |
| `receipt_fact_size_cap` | `receipt_nontrivial` | first fact_encoded_bytes -> 16385; reseal | 418 | `35b5cd9b637ba9794b1c47b3e078e1d349e47bfba782cdab6364564c1be9a779` | `E_FACT_SIZE_CAP` |
| `receipt_fact_extends_beyond_facts_block` | `receipt_nontrivial` | first fact length exceeds the complete facts block by one; reseal | 418 | `dd78b0dab3a0b42226c5e7022be2addfc236a9b2cfb2cc42a7ae55f8f8dfa743` | `E_FACT_LENGTH` |
| `receipt_fact_local_trailing_byte` | `receipt_nontrivial` | insert one byte at first fact EOF and extend its/block/total lengths; reseal | 419 | `4955fa569fdd9390a4bdf9020f9566a8fa5a06b8eec1e0c205f2d18ff5e72bd3` | `E_FACT_EOF` |
| `receipt_facts_length` | `receipt_nontrivial` | facts_encoded_bytes -= 1 without changing total bytes | 418 | `35d88cfd147b7fd744c85050ae3c80839a3ee706c59d2fecbdbec1711ffefca1` | `E_FACTS_LENGTH` |
| `receipt_zero_binding_token` | `receipt_nontrivial` | base AcceptedBinding token set to all-zero; reseal | 418 | `7ce0424e7e91328839489c6221a3b4d145d4677c3b6278f86aebe7604f53ee9b` | `E_BINDING_AUTHORITY` |
| `receipt_zero_epoch` | `receipt_nontrivial` | issuer_instance_epoch and registered context epoch both set to zero; reseal | 418 | `6b80334fab2626eee44feb5666ddd9817005697298ee2c55b97489948bdd1362` | `E_EPOCH` |
| `receipt_foreign_sandbox` | `receipt_nontrivial` | store-held authority is for a different sandbox_id | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_ISSUER_AUTHORITY` |
| `receipt_issuer_mismatch` | `receipt_nontrivial` | registered issuer_id differs | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_ISSUER_AUTHORITY` |
| `receipt_unknown_nonzero_key_id` | `receipt_nontrivial` | registry exposes key id 8 while receipt names key id 7 | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_AUTH_KEY` |
| `receipt_sequence_first_zero` | `receipt_nontrivial` | sequence_first -> 0; reseal | 418 | `e69bb9929669886d7f87371ae63cf43e47ba5e9423973ee627547e8e213069d6` | `E_SEQUENCE` |
| `receipt_sequence_final_before_first` | `receipt_nontrivial` | sequence_final 102 -> 99 while sequence_first is 100; reseal | 418 | `36a7f33c3c23b1628a0fa1c959030c93636dd5af4f4a2a9a0a2f40151157048b` | `E_SEQUENCE` |
| `receipt_sequence_gap` | `receipt_nontrivial` | validate against last_sequence_final=98, expected first=99 | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `FULL_ENUM_SEQUENCE_GAP` |
| `receipt_sequence_overflow` | `receipt_nontrivial` | validate after last_sequence_final=u64::MAX | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_SEQUENCE_OVERFLOW` |
| `receipt_stale_generation` | `receipt_nontrivial` | validate against last_generation=9 | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_SOURCE_GENERATION` |
| `receipt_custody_mismatch` | `receipt_nontrivial` | registered custody_id differs | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_CUSTODY` |
| `receipt_replay_divergence` | `receipt_nontrivial` | same generation/sequence tuple, different validly authenticated bytes | 418 | `1b6c994a6923e7b14df150fb05bd9b072d7c6375ca3582087cf9cdda6eb9dea5` | `E_REPLAY_DIVERGENCE` |
| `receipt_authentication` | `receipt_nontrivial` | issuer_authentication final byte ^= 1 | 418 | `a6e1657383d3d0b7b56ee756e228633e088d691e9fb36633c49389192de4827e` | `E_AUTH` |
| `receipt_coverage_digest` | `receipt_nontrivial` | coverage_digest byte 0 ^= 1; recompute only MAC | 418 | `d8840a4e21dc5c2e329980dc5201dbd3aa1e402b0c5af25436857c472d8431bf` | `E_COVERAGE` |
| `receipt_non_final` | `receipt_nontrivial` | finality FINAL -> NON_FINAL; reseal | 418 | `d0d1b0a2f0bb3978ecf6d44e50b340c1bd438691642612c6c77fb950c4434d81` | `FULL_ENUM_NON_FINAL` |
| `receipt_producer_overflow` | `receipt_nontrivial` | overflowed COMPLETE -> PRODUCER_OVERFLOW; reseal | 418 | `1b6c994a6923e7b14df150fb05bd9b072d7c6375ca3582087cf9cdda6eb9dea5` | `FULL_ENUM_PRODUCER_OVERFLOW` |
| `receipt_unknown_finality` | `receipt_nontrivial` | finality -> 2; reseal | 418 | `8b78a6be4b315fa95a0552e084721399e3089ea1df32052f29d32856c4a07ead` | `E_FINALITY_TAG` |
| `receipt_unknown_overflow_tag` | `receipt_nontrivial` | overflowed -> 2; reseal | 418 | `e1a561cd53701d6a63699ad4e0dd64d836214b85a18ec47ebefe872bc1b4e828` | `E_OVERFLOW_TAG` |
| `receipt_unknown_auth_scheme` | `receipt_nontrivial` | authentication_scheme -> 2 | 418 | `658e3a79d7befc956ffd9b19f7efb9a18dc0b9b4b234a3c5388d151e2d8e12cf` | `E_AUTH_SCHEME` |
| `receipt_zero_key_id` | `receipt_nontrivial` | issuer_key_id set to zero; reseal | 418 | `4379f56cbca9ebc9c4883dba64f0d800fa4bab5b2ea69e8cec093148a09ad783` | `E_AUTH_KEY` |
| `receipt_trailing_byte` | `receipt_nontrivial` | append 00 without changing total_encoded_bytes | 419 | `34d664100f3e368b5a5e0ce87de65ee32a5cdb6ef586a8da17ec75a57aabf53e` | `E_TOTAL_LENGTH` |
| `receipt_total_cap` | `synthetic` | borrowed input length = 1048577 | 1048577 | `c9b3e89559bb623b5e2dc19daebf3933c1afe5ee5dca08428522e60a40fcb998` | `E_TOTAL_CAP` |

## Generated typed pre-effect disposition vectors

| Vector | Base | Exact context/mutation | Bytes | BLAKE3-256 | Expected |
|---|---|---|---:|---|---|
| `disposition_full_changed_file_required` | `receipt_nontrivial` | valid changed-path coverage; no authoritative resync witness | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `FULL_CHANGED_FILE_REQUIRED` |
| `disposition_resource_unavailable` | `receipt_nontrivial` | shared B_s reservation unavailable before parse/authentication | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `RECEIPT_RESOURCE_REFUSED_UNAVAILABLE` |
| `disposition_resource_deadline` | `receipt_nontrivial` | reservation deadline already expired before parse/authentication | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `RECEIPT_RESOURCE_REFUSED_DEADLINE` |
| `disposition_resource_cancelled` | `receipt_nontrivial` | reservation cancelled before parse/authentication | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `RECEIPT_RESOURCE_REFUSED_CANCELLED` |
| `disposition_resource_short_precharge` | `receipt_nontrivial` | reservation is one byte below exact combined precharge | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `RECEIPT_RESOURCE_REFUSED_UNAVAILABLE` |
| `decoder_owned_capacity_cap` | `receipt_nontrivial` | decoder-owned capacity request is 2097153 bytes | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_DECODER_OWNED_CAP` |
| `error_disclosure_cap` | `receipt_nontrivial` | error disclosure request is 4097 bytes | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_ERROR_DISCLOSURE_CAP` |
| `authority_raw_token_cannot_mint` | `receipt_nontrivial` | matching raw token lacks a store-held accepted capability | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_BINDING_AUTHORITY` |
| `authority_revoked_binding` | `receipt_nontrivial` | store-held binding is revoked | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_BINDING_AUTHORITY` |
| `authority_stale_locator` | `receipt_nontrivial` | store-held binding locator proof is stale | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_BINDING_AUTHORITY` |
| `authority_unauthenticated_closure` | `receipt_nontrivial` | accepted-binding closure authentication is absent | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_BINDING_AUTHORITY` |
| `authority_disabled_issuer_key` | `receipt_nontrivial` | registered issuer key is disabled | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_AUTH_KEY` |
| `authority_epoch_reuse_or_rollback` | `receipt_nontrivial` | epoch history marks the current numeric epoch reused or rolled back | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_EPOCH` |
| `custody_transferred_or_mutable_snapshot` | `receipt_nontrivial` | snapshot custody was transferred or is no longer immutable | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_CUSTODY` |
| `custody_immediate_revalidation_failure` | `receipt_nontrivial` | authority/custody revalidation fails immediately before effects | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_IMMEDIATE_REVALIDATION` |
| `replay_unknown_outcome_requires_recovery` | `receipt_nontrivial` | exact replay exists but prior outcome is unknown | 418 | `e98c752d54e170dd8f896dae4f852ece54edc8fd99c272f3a9d205b3529fd603` | `E_REPLAY_RECOVERY_REQUIRED` |

## Generated valid physical-pack vectors

### `pack_minimal_one_chunk`

- Encoded bytes: `288`
- Exact pack BLAKE3-256: `8beece1d4a7e0da6b226d1d0c69814a8768aa1513ffb3930e72970a606e5554c`
- PackId / authenticated trailer checksum: `38129fd7c46de8098d765e1648ea40d799ff6ee8f6a83101f813e072c3e4e940`
- Physical discovery order: `05:d42cbe6f90ec1871ef9ffd38aab94a9fb7d226eb1c565cd1a01936c994f9c634`
- Embedded key order: `05:d42cbe6f90ec1871ef9ffd38aab94a9fb7d226eb1c565cd1a01936c994f9c634`
- Exact bytes: `454c535041434b31000100400000000044444444444444444444444444444444444444444444444444444444444444440000000100500000000000000000008000000035454c534f424a30310001050044444444444444444444444444444444444444444444444444444444444444440000000000000001000000000000000005000000d42cbe6f90ec1871ef9ffd38aab94a9fb7d226eb1c565cd1a01936c994f9c6340000000000000040000000359b43567793cb5b3e73c5cf4e7796aa4b9dfcc36590db7775a9cc84a059d3078c454c5350454e44310001005000000000000000000000012000000000000000800000000000000050000000010000000038129fd7c46de8098d765e1648ea40d799ff6ee8f6a83101f813e072c3e4e940`

### `pack_discovery_order_differs_from_index_order`

- Encoded bytes: `432`
- Exact pack BLAKE3-256: `142b5522be827dffc8bd390220cee15e243ad256e5dc59beec65cd9843f8f578`
- PackId / authenticated trailer checksum: `ba54add8a2137fde07cb013c1fe6e04c8b6ac9af0558021be1dcc5e9b21a2591`
- Physical discovery order: `05:eaccc310a94b3e2e50ffdce579f88a00fdb892fd7c2e829ed7c4da502d28acb7, 05:d42cbe6f90ec1871ef9ffd38aab94a9fb7d226eb1c565cd1a01936c994f9c634`
- Embedded key order: `05:d42cbe6f90ec1871ef9ffd38aab94a9fb7d226eb1c565cd1a01936c994f9c634, 05:eaccc310a94b3e2e50ffdce579f88a00fdb892fd7c2e829ed7c4da502d28acb7`
- Exact bytes: `454c535041434b3100010040000000004444444444444444444444444444444444444444444444444444444444444444000000020050000000000000000000c000000035454c534f424a30310001050044444444444444444444444444444444444444444444444444444444444444440000000000000001010000000000000000000035454c534f424a30310001050044444444444444444444444444444444444444444444444444444444444444440000000000000001000000000000000005000000d42cbe6f90ec1871ef9ffd38aab94a9fb7d226eb1c565cd1a01936c994f9c6340000000000000080000000359b43567793cb5b3e73c5cf4e7796aa4b9dfcc36590db7775a9cc84a059d3078c05000000eaccc310a94b3e2e50ffdce579f88a00fdb892fd7c2e829ed7c4da502d28acb70000000000000040000000357bae149370c09a81f82cf0a001a47db51b25941f639fdebcc0ea2d5ed8bf36f7454c5350454e4431000100500000000000000000000001b000000000000000c000000000000000a00000000200000000ba54add8a2137fde07cb013c1fe6e04c8b6ac9af0558021be1dcc5e9b21a2591`

## Generated physical-pack hostile vectors

| Vector | Base | Exact mutation | Mutated bytes | BLAKE3-256 | Expected |
|---|---|---|---:|---|---|
| `pack_index_object_start_offset` | `pack_discovery_order_differs_from_index_order` | first index absolute_offset += 4 (object start instead of record-length prefix); reseal | 432 | `e82e1c95cc48ded0f008a3142bb42332933f72fa125a1cd1165a0db181b7b3e9` | `P_RECORD_BIJECTION` |
| `pack_duplicate_typed_index_key` | `pack_discovery_order_differs_from_index_order` | second typed index key = first typed index key; reseal | 432 | `02f3229e96c1078afaf44db45b57dea78eb2ebc0441cc19c8001564b1d21da29` | `P_INDEX_ORDER_DUPLICATE` |
| `pack_descending_index` | `pack_discovery_order_differs_from_index_order` | swap the two complete sorted index entries; reseal | 432 | `819978e247ca77b2423a8147917af486479ee3a6867f831ac59de8ed392bd38e` | `P_INDEX_ORDER_DUPLICATE` |
| `pack_overlapping_record_offsets` | `pack_discovery_order_differs_from_index_order` | later physical record absolute_offset -> 64; reseal | 432 | `af416563400bd40cfeddfa0960f1149e849643dd3834c4a2500818ff540f0156` | `P_RECORD_BIJECTION` |
| `pack_record_length_prefix_mismatch` | `pack_discovery_order_differs_from_index_order` | first physical object_len prefix += 1; reseal | 432 | `145d66ac6d60ffbe6966cb33aa8112a4a835423ca0e6fcaef93035586955ee11` | `P_RECORD_LENGTH` |
| `pack_unknown_physical_object_kind` | `pack_discovery_order_differs_from_index_order` | first physical ELSOBJ01 kind -> ff; reseal | 432 | `723712b0b7229bde6ff5ca72c996d04c76d6718db8a900e540325bab85e77047` | `P_OBJECT_KIND` |
| `pack_nonzero_minimum_padding` | `pack_discovery_order_differs_from_index_order` | first minimum-padding byte 00 -> 01; reseal | 432 | `606fb1f45bfec5b9fdc8c132ba8ee217e12c67c98f5e6dfdb1ad53b0dfe6a8c5` | `P_PADDING` |
| `pack_object_checksum_mismatch` | `pack_discovery_order_differs_from_index_order` | first index object_checksum byte ^= 1; reseal pack | 432 | `fd7304e16164ffaf49cbda07728a0f2b7f0a25448c50477c8d2c8738d6498d76` | `P_OBJECT_CHECKSUM` |
| `pack_authentication_mismatch` | `pack_discovery_order_differs_from_index_order` | final pack_checksum byte ^= 1 | 432 | `4b759c318829c4a815ca4e0c46f9af5ffb37f7a7f447825e3d83b1efa69687f3` | `P_PACK_CHECKSUM` |
| `pack_trailing_byte` | `pack_discovery_order_differs_from_index_order` | append 00 after declared pack EOF | 433 | `ed8872c2c41b1f06f9c033fa1b7e7767277969b394105ab02c4dc22928a736c5` | `P_EXACT_EOF` |
| `pack_physical_record_omitted_from_index` | `pack_discovery_order_differs_from_index_order` | remove one index entry; set header/trailer count=1, index_len=80, pack_len exact; reseal | 352 | `e6ee4286f4531f98dbfe481d4b53108b0d57cc42e4a295585e8933cb2a26e6cb` | `P_RECORD_BIJECTION` |
| `pack_extra_index_entry_without_physical_record` | `pack_minimal_one_chunk` | insert sorted kind-04 index entry sharing the physical offset; set count/index_len/pack_len; reseal | 368 | `c08d299600f056f4f8373379fd31a69cffad8ae863e1c46ab201bc3e9a74fb4c` | `P_RECORD_BIJECTION` |
| `pack_trailer_record_count_disagrees` | `pack_discovery_order_differs_from_index_order` | trailer record_count 2 -> 3; reseal | 432 | `3ddbe2b5cde4a5d1ec14da788a63619eae05968b024a4f0bccc398f0518b8dcc` | `P_TRAILER_FIELDS` |
| `pack_trailer_index_offset_disagrees` | `pack_discovery_order_differs_from_index_order` | trailer index_offset += 8; reseal | 432 | `951960c9086e725a9c212f5abd32fe7585919293897c32bede394558f94ab28c` | `P_TRAILER_FIELDS` |
| `pack_trailer_index_length_disagrees` | `pack_discovery_order_differs_from_index_order` | trailer index_len 160 -> 240; reseal | 432 | `a24224d2e695db36414716cd0d57f4e5d3a95d886cae236a1613427b9700f382` | `P_TRAILER_FIELDS` |
| `pack_index_position_overlaps_physical_records` | `pack_discovery_order_differs_from_index_order` | header index_offset -> 64 without relocating bytes; reseal | 432 | `00dff68fb2945e58577a9ef11b562a5ad4ae51b5a4ff83376c233c86f41af305` | `P_EXACT_EOF` |
| `pack_trailer_position_has_unindexed_gap` | `pack_discovery_order_differs_from_index_order` | insert 00 between index and trailer | 433 | `efcc00a1eba493b1944a9a142ca3a94d0fdf985cd2ddc370b55dd8845d960772` | `P_EXACT_EOF` |
| `pack_offset_relative_to_record_region` | `pack_discovery_order_differs_from_index_order` | later absolute_offset encoded relative to byte 64; reseal | 432 | `af416563400bd40cfeddfa0960f1149e849643dd3834c4a2500818ff540f0156` | `P_RECORD_BIJECTION` |
| `pack_offset_relative_to_index` | `pack_discovery_order_differs_from_index_order` | later absolute_offset incorrectly adds index origin; reseal | 432 | `cbf4e5fcea4869df2da63f4399929ae9a2267eca0e005bb97a4c93cdeb061e5d` | `P_RECORD_BIJECTION` |
| `pack_offset_one_based` | `pack_discovery_order_differs_from_index_order` | later absolute_offset += 1; reseal | 432 | `3bcba0e0979a5632160c81f57e17a4a53ba49bf11235ab6dc5c2365660e8d76f` | `P_RECORD_BIJECTION` |

## Generated exact journal, quarantine-record, and catalog-descriptor vectors

| Vector | Base | Exact mutation | Bytes | BLAKE3-256 | Expected | Exact accepted bytes |
|---|---|---|---:|---|---|---|
| `journal_valid_intent` | `exact ELSJRN01 intent` | none | 188 | `164d0a139b09f2f6997b0b776c62a17e6d6972644378d8dbc2b865b472ee74eb` | `J_INTENT_ACCEPTED` | `454c534a524e303100010101000000004444444444444444444444444444444444444444444444444444444444444444111111111111111111111111111111110000000000000000000000500000000000000007222222222222222222222222222222222222222222222222222222222222222200000000000000083333333333333333333333333333333333333333333333333333333333333333c1c74cbc2dddfe6f036e9d4bfa730d655c27ed03de64c18ff61d424d65da154d` |
| `journal_valid_committed_outcome` | `exact ELSJRN01 outcome` | none | 151 | `f8ceab9897576276b1d599270dfb4385c7a48d98c8ffa0e84e8a93fd69640ec9` | `J_OUTCOME_ACCEPTED` | `454c534a524e3031000102010000000044444444444444444444444444444444444444444444444444444444444444441111111111111111111111111111111100000000000000010000002b010000000000000008333333333333333333333333333333333333333333333333333333333333333300003b6d2ef30cb9a65675aeafba175f9199d91a3840e409a9290501cd4ae61dfeac` |
| `journal_bad_magic` | `journal_valid_intent` | magic[0] ^= 1 | 188 | `b3304149ff135e32b973dd247e2ffad2dbe0617b947c68a0d5f4ae1032ef98c3` | `J_MAGIC` | — |
| `journal_bad_schema` | `journal_valid_intent` | schema -> 2; reseal | 188 | `844ac38195e782d21d5be249d9068d7639ef2f6407fb5b0be3072622576050d7` | `J_SCHEMA` | — |
| `journal_unknown_type` | `journal_valid_intent` | frame_type -> 3; reseal | 188 | `7aaf8c208a7506279354049a1d35a1c250681a116078b047291ead4a9406a1ec` | `J_FRAME_TYPE` | — |
| `journal_unknown_operation` | `journal_valid_intent` | operation -> 2; reseal | 188 | `93abe017112dcd4357864a14e6be10d81104858d79d5a0f6a9ae51b11047acdc` | `J_OPERATION` | — |
| `journal_nonzero_flags` | `journal_valid_intent` | flags -> 1; reseal | 188 | `919f588b101e17cefb26b5032d986a12142ccb73f1b30f59c208d5a89f754685` | `J_FLAGS` | — |
| `journal_wrong_sequence` | `journal_valid_intent` | intent sequence -> 1; reseal | 188 | `cb5295fdf37d7762fb08d7afac0a856df3085fabc980234cce2322a1a27ce4bc` | `J_SEQUENCE` | — |
| `journal_payload_zero_shape` | `journal_valid_intent` | replace declared payload and reseal | 108 | `591bb1c78d5cf7e54d99fbdf830e828f8f52b835c8726e91aafadb9dd67965f4` | `J_INTENT_PAYLOAD_SHAPE` | — |
| `journal_payload_4096_shape` | `journal_valid_intent` | replace declared payload and reseal | 4204 | `ab19a774b9065108a0b74a65081954985aa22f076af623401f688fa0f764d6db` | `J_INTENT_PAYLOAD_SHAPE` | — |
| `journal_payload_4097_refused` | `journal_valid_intent` | replace declared payload and reseal | 4205 | `f76c9303e1a5d15574c66ec046aa1259571177125dd1bbe778455a28492b3768` | `J_PAYLOAD_CAP` | — |
| `journal_checksum_mismatch` | `journal_valid_intent` | checksum[-1] ^= 1 | 188 | `a2b37c51d4a3fbff0664e32fc3947ddc82f57f9573251761bd2a4dec29e7b19c` | `J_CHECKSUM` | — |
| `journal_truncation` | `journal_valid_intent` | remove final checksum byte | 187 | `33d0927b67df186e5d13653eb8289db2f2e43c65844741619f4adfe6ecfb98f8` | `J_TRUNCATED` | — |
| `journal_trailing_byte` | `journal_valid_intent` | append 00 after exact EOF | 189 | `69938780a444ba4f5c9d1705faf21c84ad016f05078f9f136eef90bbf967ac8c` | `J_TRAILING` | — |
| `journal_unknown_result` | `journal_valid_committed_outcome` | result -> 0; reseal | 151 | `3e4551f8bb3127da05ca3e6d095572b4b2ea572583059f15a49a7369a4af4102` | `J_RESULT` | — |
| `journal_payload_zero_envelope_boundary` | `journal_valid_intent` | replace declared payload and reseal; envelope-only boundary oracle | 108 | `591bb1c78d5cf7e54d99fbdf830e828f8f52b835c8726e91aafadb9dd67965f4` | `J_ENVELOPE_ACCEPTED` | — |
| `journal_payload_4096_envelope_boundary` | `journal_valid_intent` | replace declared payload and reseal; envelope-only boundary oracle | 4204 | `ab19a774b9065108a0b74a65081954985aa22f076af623401f688fa0f764d6db` | `J_ENVELOPE_ACCEPTED` | — |
| `qrec_valid_reason2_private_pack_object` | `exact ELSQRN01 reason-2 private-pack record` | none | 284 | `3531c650db87e4c5c966be22819d051f3d27a0c61568faab1173db6bd147a396` | `QREC_ACCEPTED` | `454c5351524e30310001011c000200004444444444444444444444444444444444444444444444444444444444444444000000000000000833333333333333333333333333333333333333333333333333333333333333330205000038129fd7c46de8098d765e1648ea40d799ff6ee8f6a83101f813e072c3e4e9408888888888888888888888888888888888888888888888888888888888888888000000000000002a0000012000000000999999999999999999999999999999999999999999999999999999999999999938129fd7c46de8098d765e1648ea40d799ff6ee8f6a83101f813e072c3e4e940111111111111111111111111111111115827ef99f8dca330956f38f7fcecb76b4eb953a7148b87af54d2aeb2c7a81379` |
| `qrec_bad_magic` | `qrec_valid_reason2_private_pack_object` | magic[0] ^= 1; reseal | 284 | `d963b3973512e95995eb3714d982dc0c0ed6cc8c42964d5b7378ed8d47682fca` | `Q_MAGIC` | — |
| `qrec_bad_schema` | `qrec_valid_reason2_private_pack_object` | schema -> 2; reseal | 284 | `05718f3f0be9ff7c29c6cff4cc0cb0a22934c754e3b92ecd03879efe0ca3b982` | `Q_SCHEMA` | — |
| `qrec_bad_record_len` | `qrec_valid_reason2_private_pack_object` | record_len -> 283; reseal | 284 | `499d709588a90e6fe5a314c066eac1dbbcbb3a2158b8ec0a88663711bcfbc379` | `Q_RECORD_LENGTH` | — |
| `qrec_unknown_reason` | `qrec_valid_reason2_private_pack_object` | reason -> 6; reseal | 284 | `0aa32444f9b2e872efe9afcae8b9f7dcbae18ba1f451da4b306fea0bdbf7d208` | `Q_REASON` | — |
| `qrec_nonzero_flags` | `qrec_valid_reason2_private_pack_object` | flags -> 1; reseal | 284 | `a4a658f2632d5962fddf8ea8a63a5ccdf7d1fafeefbe4c33cfa8b1c6e9d76e18` | `Q_FLAGS` | — |
| `qrec_unknown_carrier` | `qrec_valid_reason2_private_pack_object` | carrier_kind -> 4; reseal | 284 | `fda8f51102fcab1be89f469dbbf0745df01c5b286b9d807ba740af60b9bb48f1` | `Q_CARRIER_KIND` | — |
| `qrec_unknown_object_kind` | `qrec_valid_reason2_private_pack_object` | object_kind -> 6; reseal | 284 | `7423ce867d082bf7b8917e4f6790ed07561b266bfeaea2670f04ee49a81dc7a1` | `Q_OBJECT_KIND` | — |
| `qrec_nonzero_reserved` | `qrec_valid_reason2_private_pack_object` | reserved -> 1; reseal | 284 | `e5fcf132152e37cb0bb17696760269c9f58f3e26cb481976d1e139f27dd66528` | `Q_RESERVED` | — |
| `qrec_nonzero_reserved2` | `qrec_valid_reason2_private_pack_object` | reserved2 -> 1; reseal | 284 | `ab736354bfdf45d27ba40adc83a4824767cb88776b4c7f9f5bb657170a581cfc` | `Q_RESERVED` | — |
| `qrec_absent_pack_field_nonzero` | `qrec_valid_reason2_private_pack_object` | Metadata carrier with nonzero PackId_or_zero | 284 | `98e48bfbac9daf81fbd6664170933300968ea0106719925899c7235185bb1904` | `Q_PRESENCE` | — |
| `qrec_carrier_wide_object_field_present` | `qrec_valid_reason2_private_pack_object` | object_kind -> 0 but ObjectId remains present | 284 | `f9fd9552d642a65a9ec400a0c6b077223f92c269f8243568f9c7f71ac0f1841d` | `Q_PRESENCE` | — |
| `qrec_private_ordinal_overflow` | `qrec_valid_reason2_private_pack_object` | private ordinal -> u32::MAX+1 | 284 | `bc52a0ea795ebcdc85264110a7eaa5162778f25021600b1fe6255ce6da7f856e` | `Q_PRIVATE_ORDINAL` | — |
| `qrec_private_complete_length_mismatch` | `qrec_valid_reason2_private_pack_object` | encoded_len 287 != retained carrier 288 | 284 | `9a38a84834edb790862fe682c3c13499d3afef1e989412da8ff25e95d397de1e` | `Q_PRIVATE_LENGTH` | — |
| `qrec_final_object_offset_before_elsobj` | `qrec_valid_reason2_private_pack_object` | object_offset -> 67 | 284 | `eb4909b703d13b1f6027365f965828929233576d8e9ed0b427d723cb0bdd59b6` | `Q_OBJECT_EXTENT` | — |
| `qrec_final_object_extent_past_eof` | `qrec_valid_reason2_private_pack_object` | offset 68 + len 221 > carrier 288 | 284 | `b59321c37554491abcccee91c4dc4c5a23c708d80ae9282a7df4f974fc7affdf` | `Q_OBJECT_EXTENT` | — |
| `qrec_checksum_mismatch` | `qrec_valid_reason2_private_pack_object` | record_checksum[-1] ^= 1 | 284 | `29081148ac30bd981e20f3994805efc80bea82f0077d0ecbe8b23a15c004afab` | `Q_CHECKSUM` | — |
| `qrec_truncation` | `qrec_valid_reason2_private_pack_object` | remove final checksum byte | 283 | `ec9e820ad5366e6c0582558a73d9afcb27a3d9beb857c1e0e156a6d22f5a76e4` | `Q_TRUNCATED` | — |
| `qrec_trailing_byte` | `qrec_valid_reason2_private_pack_object` | append 00 after exact EOF | 285 | `3b68bda7c3ff55f2e612b4252fe673e74d1db96d8324bf948234597ecafcd1ca` | `Q_TRAILING` | — |
| `catalog_valid_exact_168_byte_descriptor` | `authenticated pack descriptor` | none | 168 | `2cfe27bcf4b2c0ff1d335f929aebec95a4b5dba041cb0872bfb8e12216f88fa4` | `C_DESCRIPTOR_ACCEPTED` | `38129fd7c46de8098d765e1648ea40d799ff6ee8f6a83101f813e072c3e4e940000000000000012000000001000000000000000000000080000000000000005005000000d42cbe6f90ec1871ef9ffd38aab94a9fb7d226eb1c565cd1a01936c994f9c63405000000d42cbe6f90ec1871ef9ffd38aab94a9fb7d226eb1c565cd1a01936c994f9c63438129fd7c46de8098d765e1648ea40d799ff6ee8f6a83101f813e072c3e4e940` |
| `catalog_descriptor_nonzero_flags` | `catalog_valid_exact_168_byte_descriptor` | flags -> 1 | 168 | `3fd1922d8343939cfabbac2fe0f873205637d42fc2a2551e686fa27537a4f862` | `C_DESCRIPTOR_FLAGS` | — |
| `catalog_descriptor_nonzero_key_reserved` | `catalog_valid_exact_168_byte_descriptor` | min_key.reserved[0] -> 1 | 168 | `422a7adf8b2584aa84bbdd6601805fb437a9a7f479459fe0f808eb3bbb4a1420` | `C_KEY_RESERVED` | — |

## Generated bounded-pack arithmetic vectors

| Law | Exact value | Expected |
|---|---:|---|
| maximum pack bytes | 67108864 | `PACK_CAP_ACCEPTED` |
| maximum record metadata entries | 466032 | `PACK_RECORD_CAP_ACCEPTED` |
| maximum embedded-index bytes (`466032 * 80`) | 37282560 | `PACK_INDEX_CAP_ACCEPTED` |
| fixed metadata entries per initial run | 46604 | `RUN_ENTRY_CAP_ACCEPTED` |
| initial run byte window (`46604 * 80 + 64`) | 3728384 | `A7_BYTE_WINDOW_ACCEPTED` |
| initial runs / first-pass input+output files | `10/15` | `R28_MIN_16_ACCEPTED` |
| physical spill maximum | 74687985 | `BELOW_75000000` |

## Generated path, collision, lifecycle, crash, catalog, identity, and bounded-resource model vectors

| Vector | Exact input | Input BLAKE3-256 | Expected |
|---|---|---|---|
| `journal_intent_final_path` | `journal/v1/4444444444444444444444444444444444444444444444444444444444444444/11111111111111111111111111111111-0000000000000000.frame` | `fb8546eca76a6c9b832fe094d6108a14453922dffc8896afec5f3da06e176beb` | `JOURNAL_PATH_ACCEPTED` |
| `journal_outcome_private_path` | `journal/v1/4444444444444444444444444444444444444444444444444444444444444444/11111111111111111111111111111111-0000000000000001.frame.tmp` | `3c9eb688b51a10ee6fad2d724b574e0672b0796e898994c50495ff402d21179d` | `JOURNAL_PATH_ACCEPTED` |
| `journal_uppercase_sequence` | `journal/v1/4444444444444444444444444444444444444444444444444444444444444444/11111111111111111111111111111111-000000000000000A.frame` | `2bba5cd726bf6c33ed93cfb4556fca4b960486025a6e851a6457197170113b44` | `PATH_SEQUENCE_LOWERHEX` |
| `journal_sequence_wrong_width` | `journal/v1/4444444444444444444444444444444444444444444444444444444444444444/11111111111111111111111111111111-000000000000000.frame` | `51a64730b991e73880426ddf049a4280e978fdcbeaf7383825a1281deebec07d` | `PATH_SEQUENCE_LOWERHEX` |
| `journal_sequence_outside_v1_pair` | `journal/v1/4444444444444444444444444444444444444444444444444444444444444444/11111111111111111111111111111111-0000000000000002.frame` | `88c4c1cd59c8981a1478f18d521adf471004d4a9ee2614621ddce6c6f414e1c4` | `JOURNAL_SEQUENCE` |
| `journal_sequence_name_frame_mismatch` | `journal/v1/4444444444444444444444444444444444444444444444444444444444444444/11111111111111111111111111111111-0000000000000000.frame` | `fb8546eca76a6c9b832fe094d6108a14453922dffc8896afec5f3da06e176beb` | `JOURNAL_SEQUENCE_NAME_MISMATCH` |
| `journal_profile_name_authenticated_frame_mismatch` | `journal/v1/4545454545454545454545454545454545454545454545454545454545454545/11111111111111111111111111111111-0000000000000000.frame` | `789bf4d62dab62950cee7bafe43f1b97b8d0e3e2e3c8edf1e7edf0c3589741e6` | `JOURNAL_PROFILE_NAME_MISMATCH` |
| `journal_transaction_name_authenticated_frame_mismatch` | `journal/v1/4444444444444444444444444444444444444444444444444444444444444444/12121212121212121212121212121212-0000000000000001.frame.tmp` | `6a9a835b0a0cb54c93837e36333581bbbfb2fd06d113af134d9317cb6c6e8f50` | `JOURNAL_TRANSACTION_NAME_MISMATCH` |
| `journal_sequence_no_wrap` | `checked_next(u64::MAX)` | `34556f3751b1ee84ae559a7165d4fa03a6a48d545f905a2ec5845e0bf4817c36` | `JOURNAL_SEQUENCE_OVERFLOW_REFUSED_PRE_EFFECT` |
| `journal_private_occupied_exact_authenticated_bytes_and_eof` | `path=journal/v1/4444444444444444444444444444444444444444444444444444444444444444/11111111111111111111111111111111-0000000000000001.frame.tmp; occupied_bytes=454c534a524e3031000102010000000044444444444444444444444444444444444444444444444444444444444444441111111111111111111111111111111100000000000000010000002b010000000000000008333333333333333333333333333333333333333333333333333333333333333300003b6d2ef30cb9a65675aeafba175f9199d91a3840e409a9290501cd4ae61dfeac; EOF=151` | `b4b354189fa373b4ee8a5738915d917999e6cdbb99c12fc0a3f642493909ac24` | `JOURNAL_PRIVATE_RESUME` |
| `journal_final_occupied_exact_authenticated_bytes_and_eof` | `path=journal/v1/4444444444444444444444444444444444444444444444444444444444444444/11111111111111111111111111111111-0000000000000000.frame; occupied_bytes=454c534a524e303100010101000000004444444444444444444444444444444444444444444444444444444444444444111111111111111111111111111111110000000000000000000000500000000000000007222222222222222222222222222222222222222222222222222222222222222200000000000000083333333333333333333333333333333333333333333333333333333333333333c1c74cbc2dddfe6f036e9d4bfa730d655c27ed03de64c18ff61d424d65da154d; EOF=188` | `3cc213c8891affaf37dc861efd7e60264a892b1399531cda947435e06bc60749` | `JOURNAL_FINAL_IDEMPOTENT_REUSE` |
| `journal_private_path_cannot_be_claimed_final` | `path=journal/v1/4444444444444444444444444444444444444444444444444444444444444444/11111111111111111111111111111111-0000000000000001.frame.tmp; claimed_final=true; occupied_bytes=454c534a524e3031000102010000000044444444444444444444444444444444444444444444444444444444444444441111111111111111111111111111111100000000000000010000002b010000000000000008333333333333333333333333333333333333333333333333333333333333333300003b6d2ef30cb9a65675aeafba175f9199d91a3840e409a9290501cd4ae61dfeac; EOF=151` | `068b5e635c92924a95aaa621a71f096954980d804c2bbf37baddb64d1b0525a3` | `JOURNAL_PATH_CLASS_MISMATCH` |
| `journal_final_path_cannot_be_claimed_private` | `path=journal/v1/4444444444444444444444444444444444444444444444444444444444444444/11111111111111111111111111111111-0000000000000000.frame; claimed_final=false; occupied_bytes=454c534a524e303100010101000000004444444444444444444444444444444444444444444444444444444444444444111111111111111111111111111111110000000000000000000000500000000000000007222222222222222222222222222222222222222222222222222222222222222200000000000000083333333333333333333333333333333333333333333333333333333333333333c1c74cbc2dddfe6f036e9d4bfa730d655c27ed03de64c18ff61d424d65da154d; EOF=188` | `cc69d0da8d02c4f7ec79fc05725b129aab81ca455db77848069ee6a8ce4f374f` | `JOURNAL_PATH_CLASS_MISMATCH` |
| `journal_private_occupied_different_authenticated_frame` | `occupied path has a valid but byte-different authenticated frame` | `b3532dc589da5cfc3b3288827b5846f2bc49454a29d40bd72d32060b53aa1e54` | `JournalPathCollision` |
| `journal_final_occupied_invalid_checksum` | `occupied final path has exact length but invalid authentication` | `484b2e287426191d3ffb39341c21e317fefb7df70d9815382dff50bf62e52a78` | `JournalPathCollision` |
| `journal_final_occupied_trailing_bytes_after_frame` | `occupied final path has authenticated prefix plus a byte after declared EOF` | `8d5abb9ec7b805eb28a63116affc04df5125aa2e7b210979284a40f9cbeb9298` | `JournalPathCollision` |
| `journal_collision_never_uses_alternate_suffix` | `journal/v1/4444444444444444444444444444444444444444444444444444444444444444/11111111111111111111111111111111-0000000000000001.frame.tmp.1` | `b7de84c0867b45727c90564c9608d32c02a358166707503d5cf0c4c8cc539ecf` | `PATH_SUFFIX` |
| `quarantine_private_final_pair` | `quarantine/v1/55555555555555555555555555555555/.0000002a.qrec.tmp -> quarantine/v1/55555555555555555555555555555555/0000002a.qrec` | `566880a4f7810fdd208b2a0f188d1721bbdc4d5bc8af5dd4a8e4b14425e16a79` | `QUARANTINE_PATH_PAIR_ACCEPTED` |
| `quarantine_uppercase_ordinal` | `quarantine/v1/55555555555555555555555555555555/.0000002A.qrec.tmp -> quarantine/v1/55555555555555555555555555555555/0000002A.qrec` | `46b1cf25a2e0e34822612c26298d277a26340c55a4fa5eb5b13612c1ab6f11b0` | `QUARANTINE_ORDINAL_LOWERHEX` |
| `quarantine_alternative_private_suffix` | `quarantine/v1/55555555555555555555555555555555/.0000002a.qrec.tmp.1 -> quarantine/v1/55555555555555555555555555555555/0000002a.qrec` | `d9a5f28ca0fe38a021519668ac07fc5e5801eb9519f87cdc590cac81a73de404` | `QUARANTINE_PRIVATE_GRAMMAR` |
| `quarantine_ordinal_relation_mismatch` | `quarantine/v1/55555555555555555555555555555555/.0000002a.qrec.tmp -> quarantine/v1/55555555555555555555555555555555/0000002b.qrec` | `502ac289a089fa297b84827b5eed81e46f15ac35b2fbbd8f41ee7870a9573a23` | `QUARANTINE_FINAL_RELATION` |
| `quarantine_group_relation_mismatch` | `quarantine/v1/55555555555555555555555555555555/.0000002a.qrec.tmp -> quarantine/v1/66666666666666666666666666666666/0000002a.qrec` | `43247fd2b52a832d4e968001b0a570490ed57944f18fe9e60e2be85ad107d60d` | `QUARANTINE_GROUP_RELATION` |
| `quarantine_private_occupied_identical` | `private occupied; authenticated exact 284 bytes+EOF; identical` | `55e98ac5e5884f148f443237ea88265448dffca0c3713edc9a03a145e81ed6bb` | `QUARANTINE_PRIVATE_RESUME` |
| `quarantine_private_occupied_mismatch` | `private occupied; authenticated exact 284 bytes+EOF; different` | `30b6e2eecbb97f7fa2832e4ffbab7bda4f46e7dc6656e623abbb472f961ff668` | `QuarantinePathCollision` |
| `quarantine_final_occupied_identical` | `final occupied; authenticated exact 284 bytes+EOF; identical` | `9f9cd00fceda68a61390be0a40e28dfb1e69276bfecb3fc6b628cba254db6130` | `QUARANTINE_FINAL_IDEMPOTENT_REUSE` |
| `quarantine_final_occupied_invalid` | `final occupied; invalid length/checksum/EOF` | `49ce5b3b71fae5ac5bbeeeef8ba6b83d7e0daa8e1d946fd61d8e8d7c5e706075` | `QuarantinePathCollision` |
| `catalog_switch_valid_gate_partition` | `I/O outside; fixed old tuple/epoch compare, fixed lease install/validate, and final atomic clear-select of only fixed old/new/authority-unavailable under catalog gate` | `ca0afbc15b6871dd4b406353947506fcf5ef8bcc8d85c3e00190971e422b940b` | `LIFECYCLE_GATE_TRACE_ACCEPTED` |
| `catalog_switch_inconclusive_readback_valid_gate_partition` | `authenticated readback=inconclusive outside gate; custody transfer to bounded RecoveryHold outside gate; fixed lease validate plus atomic clear-select fixed authority-unavailable under CatalogSwitch; caller-supplied tuple=none` | `a2fd741bd6b2d36804575a8f83e1445f61bf22ae94721110ace882887af2f4f6` | `LIFECYCLE_GATE_TRACE_ACCEPTED` |
| `catalog_switch_inconclusive_readback_selects_fixed_authority_unavailable` | `authenticated readback=inconclusive; caller-supplied tuple=none; result source=lease-fixed authority-unavailable marker; lease cleared atomically` | `93999011752b74968a9b727c1f5bbb37a6c98ea4fe6e62d5d31cc16b330f08d3` | `FIXED_AUTHORITY_UNAVAILABLE_SELECTED_LEASE_CLEARED` |
| `catalog_switch_clear_rejects_caller_supplied_tuple` | `authenticated readback=exact_new; caller-supplied tuple=injected_new; fixed lease tuple remains sole selection authority` | `2775e1681e897c990267ad4dd929e997fe560750dac4049b9e04c48c59e403cf` | `CALLER_SUPPLIED_CATALOG_TUPLE_FORBIDDEN` |
| `read_pin_valid_gate_partition` | `precharge/I/O outside; fixed binding+locator comparison and counter install/release under read gate` | `caef3295f8da4f7e11bcdd187b886d2ec109af53922d7d3e28b5d7d1de895472` | `LIFECYCLE_GATE_TRACE_ACCEPTED` |
| `reclamation_valid_gate_partition` | `fixed snapshot pin plus generation/epoch comparison under the separately typed reclamation gate; enumeration/mark outside` | `145a334678696e9bd2d010519a2f76dc7b62c2fcc58bcb076695302afcfa403c` | `LIFECYCLE_GATE_TRACE_ACCEPTED` |
| `reclamation_gate_rejects_catalog_switch_lease_install` | `attempt CatalogSwitchLeaseV1 install while ProtectedAuthoritySnapshotPinV1 reclamation gate is held` | `d09546aeecba3b3c847718ab0bd3436d943fca09ae0668779638cfcbbad4013a` | `LIFECYCLE_GATE_EFFECT_FORBIDDEN` |
| `reclamation_to_catalog_switch_continuous_common_gate_purpose_transition` | `one continuous common short-gate hold: Reclamation performs exact G0/E0 recheck and releases its fixed snapshot pin; without release/reacquire, purpose changes to separately typed CatalogSwitch, which compares the fixed old tuple/epoch and installs its fixed lease; release common gate only after install; snapshot pin grants/shares no lease authority` | `456eb284c717bdce6ad9ce7b399e8981c031dedc4a6c7ffb7f6e2fa0175bc707` | `CONTINUOUS_COMMON_GATE_PURPOSE_TRANSITION_ACCEPTED` |
| `catalog_switch_stale_expected_old` | `gate compare stale before intent` | `08da4af70a42a373ab6c29430cbc3af6b2058b980a85117be1d035cd3637179f` | `CatalogSwitchConflict_NO_JOURNAL_NO_SELECTOR_EFFECT` |
| `catalog_switch_contender_while_lease_held` | `bounded waiter unavailable before I/O` | `bb1d29af537fe58725c6a298616bfafd3f58c2d50f43824ee813ee6e3237136a` | `CatalogSwitchBusy_PRE_IO` |
| `catalog_recovery_selector_exact_old` | `intent exists; selector=authenticated exact old tuple` | `1003739c01072d541e529e70dd1410f1aada290fca6e90626ab4499a5bf284bd` | `ROLL_BACK_AND_WRITE_CONCLUSIVE_OUTCOME` |
| `catalog_recovery_selector_exact_new` | `intent exists; selector=authenticated exact new tuple` | `b521ff314ac216435430ba467e7c0c55ad64fa18cbd508fe9ed6cf6ecdd3672f` | `COMMIT_AND_WRITE_CONCLUSIVE_OUTCOME` |
| `journal_retirement_crash_after_outcome_unlink_fence` | `unlink outcome; directory fence; crash` | `ef0036b7db3486ae5ee2476d49d6e21ffd9ecf2456d5752c884bb3a3cf7522ba` | `INTENT_ONLY_RECOVERABLE` |
| `journal_retirement_complete` | `unlink outcome+fence; unlink intent+fence` | `87bb02394db0d9a9210749be93eaa3cce2811afc5998ad33bded8781b833e596` | `ABSENT_RELEASE_CHARGES_AFTER_SECOND_FENCE` |
| `journal_retirement_intent_unlinked_before_outcome_fence` | `unlink outcome; unlink intent before outcome directory fence` | `5ea800dc68d038b25f129e0f194cd89c19327cc1c81ebffc018c0fa0099ad772` | `JOURNAL_RETIREMENT_ORDER_OR_FENCE_VIOLATION` |
| `reason2_retained_transaction_private_pack_path` | `authenticated ProfileHash=4444444444444444444444444444444444444444444444444444444444444444 + JournalTxId=11111111111111111111111111111111 + u32 ordinal=42 derive pre-existing path=packs/v1/4444444444444444444444444444444444444444444444444444444444444444/.tmp/11111111111111111111111111111111-0000002a.pack; created_carrier_paths=0` | `654830cbdc7d3916da81a857d3ee510ae5825f243f87be1b9968d4e3102bc9d6` | `Q_REASON2_PRIVATE_PATH_ACCEPTED` |
| `reason2_qrec_mints_no_second_carrier_path` | `qrec publication records evidence only; existing transaction-private pack remains the sole retained carrier; no copy/rename/new carrier path` | `d03d07bfd29543406adb71902b8787495026bf6c7e4d138ef2a90bc6b70d6a91` | `Q_REASON2_ZERO_NEW_CARRIER_PATHS` |
| `reason2_existing_private_carrier_exact_authentication` | `derived_path=packs/v1/4444444444444444444444444444444444444444444444444444444444444444/.tmp/11111111111111111111111111111111-0000002a.pack; encoded_len=288; PackId=38129fd7c46de8098d765e1648ea40d799ff6ee8f6a83101f813e072c3e4e940; observed_checksum=38129fd7c46de8098d765e1648ea40d799ff6ee8f6a83101f813e072c3e4e940; custody_owners=1` | `3e895d3dbf5a1e8849b5f031dc8eaae250ede0bb9ad131aade094c59c4b6b189` | `Q_REASON2_EXISTING_PRIVATE_CARRIER_AUTHENTICATED` |
| `reason2_existing_private_carrier_missing` | `derived transaction-private path is absent` | `c9da828fe77f129ae4f22f6456eb1db9850306867762b66721e6d0dced31ae5e` | `Q_REASON2_QUARANTINED_AUTHORITY_UNAVAILABLE` |
| `reason2_existing_private_carrier_mismatch` | `derived path contains bytes that fail qrec PackId/checksum authentication` | `0738dc795d57be6964d0a8ed48a65ed8221278ab744b03f7d855f268b4d3bafc` | `Q_REASON2_QUARANTINED_AUTHORITY_UNAVAILABLE` |
| `reason2_existing_private_carrier_multiply_owned` | `authenticated retained path has two claimed custody owners` | `d693af27ada4e19c2388945970dc32ef3d224ac1827c0858fb1ab04dc4df514a` | `Q_REASON2_QUARANTINED_AUTHORITY_UNAVAILABLE` |
| `reason2_qrec_identifies_occupied_final_pack` | `path=packs/v1/4444444444444444444444444444444444444444444444444444444444444444/38129fd7c46de8098d765e1648ea40d799ff6ee8f6a83101f813e072c3e4e940.pack; object_kind=5; ObjectId=8888888888888888888888888888888888888888888888888888888888888888; private_encoded_len=288` | `c82bab49028d2e964663947ed2cab6357208a75c4662f54bd170aa771371b50f` | `Q_REASON2_OCCUPIED_FINAL_PACK_IDENTIFIED` |
| `reason2_private_pack_wrong_txid` | `packs/v1/4444444444444444444444444444444444444444444444444444444444444444/.tmp/12121212121212121212121212121212-0000002a.pack` | `4793f0f4cc3026dbca375814a2c70086bb4d55cfea9981eb98bb74c259c7ab98` | `Q_REASON2_PRIVATE_PATH_RELATION` |
| `reason2_obsolete_incoming_pack_path_rejected` | `quarantine/v1/55555555555555555555555555555555/0000002a.incoming.pack` | `ecebba9d4b623265112e5b278a0d06b290a21e9f82ea9a83e7bb89bdff41fc5f` | `Q_REASON2_PRIVATE_PATH_RELATION` |
| `journal_valid_intent_outcome_pair` | `authenticated sequence-0 intent plus sequence-1 committed outcome` | `7823f18c48d96bc8a15ffa9ea3edfcea8db47ad4ad74ab0a53c0d0916feb0815` | `J_TRANSACTION_ACCEPTED` |
| `journal_intent_only_reconstruction` | `authenticated sequence-0 intent; outcome absent` | `140e118eb983b0cfb966c1d06d3be127689721eb0de7e0d5ae37b40792fafe29` | `J_INTENT_ONLY_RECOVERABLE` |
| `journal_duplicate_sequence` | `two authenticated sequence-0 frames` | `5f64382af34508c25c31a7afb18d4deba887b35c6d44987df29b5fed1f05a993` | `J_DUPLICATE_SEQUENCE` |
| `journal_outcome_without_intent` | `authenticated sequence-1 outcome only` | `38beda11545205ea4d356d425afd83fa86e08a897f3938e34ce7a0a70963c7ea` | `J_OUTCOME_WITHOUT_INTENT` |
| `journal_profile_mismatch` | `outcome ProfileId differs from intent; both resealed` | `b2bea7f853c07b9a431340059d6b9273be85515600fad468a85ffa868186009a` | `J_PROFILE_MISMATCH` |
| `journal_transaction_mismatch` | `outcome transaction_id differs from intent; both resealed` | `9d0f1466e2b4253889344f1184801cce78566a2cf624acf0ff3449663b52b7e0` | `J_TRANSACTION_MISMATCH` |
| `journal_contradicting_committed_outcome` | `committed result selects authenticated old tuple; resealed` | `0decf58afb9de03a6b255b083894b518b3dadc18bf9d786c60e9ff984b2720d9` | `J_OUTCOME_CONTRADICTION` |
| `catalog_gate_forbids_io` | `io under purpose-scoped gate` | `fd3a2d35e86789d591ea6397fe53a94749301b44a01d01cd47d28fd877a1753f` | `LIFECYCLE_GATE_EFFECT_FORBIDDEN` |
| `catalog_gate_forbids_allocation` | `allocation under purpose-scoped gate` | `5b8b6e4e36a8f67873750969e66035263254aecc2c24d3b734c02a2096217239` | `LIFECYCLE_GATE_EFFECT_FORBIDDEN` |
| `catalog_gate_forbids_wait` | `wait under purpose-scoped gate` | `189ffa8837ebc0a2b7f92b85dbd273114c96f75b8a262fee208d2ee7602bd4bc` | `LIFECYCLE_GATE_EFFECT_FORBIDDEN` |
| `catalog_gate_forbids_enumeration` | `enumeration under purpose-scoped gate` | `38b63bb36a150b97e6097be0d16d0ee54b962d5c3cfbc400a1f9eadcd62052af` | `LIFECYCLE_GATE_EFFECT_FORBIDDEN` |
| `catalog_gate_forbids_mark` | `mark under purpose-scoped gate` | `35b2a77d3da5cd23dae84f1cb7d368d02f022224bd63fb4cf82cbd5943e90536` | `LIFECYCLE_GATE_EFFECT_FORBIDDEN` |
| `read_gate_forbids_io` | `io under purpose-scoped gate` | `fd3a2d35e86789d591ea6397fe53a94749301b44a01d01cd47d28fd877a1753f` | `LIFECYCLE_GATE_EFFECT_FORBIDDEN` |
| `read_gate_forbids_allocation` | `allocation under purpose-scoped gate` | `5b8b6e4e36a8f67873750969e66035263254aecc2c24d3b734c02a2096217239` | `LIFECYCLE_GATE_EFFECT_FORBIDDEN` |
| `read_gate_forbids_wait` | `wait under purpose-scoped gate` | `189ffa8837ebc0a2b7f92b85dbd273114c96f75b8a262fee208d2ee7602bd4bc` | `LIFECYCLE_GATE_EFFECT_FORBIDDEN` |
| `read_gate_forbids_enumeration` | `enumeration under purpose-scoped gate` | `38b63bb36a150b97e6097be0d16d0ee54b962d5c3cfbc400a1f9eadcd62052af` | `LIFECYCLE_GATE_EFFECT_FORBIDDEN` |
| `read_gate_forbids_mark` | `mark under purpose-scoped gate` | `35b2a77d3da5cd23dae84f1cb7d368d02f022224bd63fb4cf82cbd5943e90536` | `LIFECYCLE_GATE_EFFECT_FORBIDDEN` |
| `reclamation_gate_forbids_io` | `io under purpose-scoped gate` | `fd3a2d35e86789d591ea6397fe53a94749301b44a01d01cd47d28fd877a1753f` | `LIFECYCLE_GATE_EFFECT_FORBIDDEN` |
| `reclamation_gate_forbids_allocation` | `allocation under purpose-scoped gate` | `5b8b6e4e36a8f67873750969e66035263254aecc2c24d3b734c02a2096217239` | `LIFECYCLE_GATE_EFFECT_FORBIDDEN` |
| `reclamation_gate_forbids_wait` | `wait under purpose-scoped gate` | `189ffa8837ebc0a2b7f92b85dbd273114c96f75b8a262fee208d2ee7602bd4bc` | `LIFECYCLE_GATE_EFFECT_FORBIDDEN` |
| `reclamation_gate_forbids_enumeration` | `enumeration under purpose-scoped gate` | `38b63bb36a150b97e6097be0d16d0ee54b962d5c3cfbc400a1f9eadcd62052af` | `LIFECYCLE_GATE_EFFECT_FORBIDDEN` |
| `reclamation_gate_forbids_mark` | `mark under purpose-scoped gate` | `35b2a77d3da5cd23dae84f1cb7d368d02f022224bd63fb4cf82cbd5943e90536` | `LIFECYCLE_GATE_EFFECT_FORBIDDEN` |
| `selector_bootstrap_absent` | `intent-only recovery observes selector=absent; bootstrap_old=true` | `bc15ce969cfff4f602d04627889db8e00a86130472f269e21d01534ac8f90626` | `BOOTSTRAP_ABSENT_IS_EXACT_OLD_ROLLBACK` |
| `selector_nonbootstrap_absent` | `intent-only recovery observes selector=absent; bootstrap_old=false` | `c3a0c70b4e5b0282923907671b49d9c4a7919d44754e14f416985c6aeffa9967` | `NONBOOTSTRAP_ABSENT_QUARANTINE_BLOCK_READINESS` |
| `selector_invalid` | `intent-only recovery observes selector=invalid; bootstrap_old=false` | `2a44b725ec9f5efaa0b3ee6202419fcddfa6ad5107be22754095ae06c3fad91d` | `OutcomeUnknown_QUARANTINE_BLOCK_READINESS` |
| `selector_truncated` | `intent-only recovery observes selector=truncated; bootstrap_old=false` | `4465bea51f3f82ef4d4811e066c631622047ce40c77ca915420c6c76510a20ea` | `OutcomeUnknown_QUARANTINE_BLOCK_READINESS` |
| `selector_unrelated` | `intent-only recovery observes selector=unrelated; bootstrap_old=false` | `6d258341ba6376e7a32bc731d16beb975bd520b532b5993896aa1281bde3881a` | `OutcomeUnknown_QUARANTINE_BLOCK_READINESS` |
| `selector_contradictory` | `intent-only recovery observes selector=contradictory; bootstrap_old=false` | `9e58b53e90d14b1e823c0f1bc535108e876ea6a55f557daa182181da8c33f715` | `OutcomeUnknown_QUARANTINE_BLOCK_READINESS` |
| `crash_before_private_pack_file_fence` | `last possibly durable admission cut 1` | `3b0b41e93c202f0b07595d696a4209af153cfe8c6b0a6da59586aa80d4d3255d` | `OLD_PRIVATE_UNFENCED_RECOVERY_CUSTODY` |
| `crash_private_pack_fenced_before_final_install_fences` | `last possibly durable admission cut 2` | `9f57d896e455893132ef990bd7fc872d3621c808ce6a78eb355e807de640397f` | `OLD_PRIVATE_FENCED_RESUME_OR_QUARANTINE` |
| `crash_final_pack_installed_before_catalog_installed` | `last possibly durable admission cut 3` | `c5e0b7508802ef16ce717b2a7266216c2eef34f2b384dea9955c7daba01c5a43` | `OLD_FINAL_PACK_INSTALLED_ORPHAN` |
| `crash_final_catalog_installed_before_intent_fence` | `last possibly durable admission cut 4` | `40fdb9ad98db70a85def335cb23f2cef617e617bed4a18aa4e801d481f7f64e3` | `OLD_UNSELECTED_CLOSURE_ORPHAN` |
| `crash_intent_fenced_before_selector_rename` | `last possibly durable admission cut 5` | `76bc7186756a055625f877335e6060c65a29364f3777ced0a580c2f0fd63aa11` | `OLD_INTENT_ONLY_RECONCILE_SELECTOR` |
| `crash_selector_rename_entered_directory_fence_inconclusive` | `last possibly durable admission cut 6` | `06f21c3cd89bf40591e4f77451cdfe45e7e7eb2861d4e30c5ddd7434aa9e546a` | `UNKNOWN_OUTCOME_UNKNOWN_RECOVERY_HOLD` |
| `crash_selector_directory_fenced_before_outcome_fence` | `last possibly durable admission cut 7` | `088f69479a0c8061da5145d58d5d7b9c6758bb8fee6c92c6c90a0e44c2b439aa` | `NEW_RECONSTRUCT_COMMITTED_OUTCOME` |
| `crash_outcome_fenced` | `last possibly durable admission cut 8` | `26e25eac1a0f6c902f606468d5e7cbde8591f811b8fa4a36c735b1715ffda320` | `EXACT_RECORDED_RESULT_COMMITTED_NEW_ONLY_RECEIPT` |
| `quarantine_ambiguity_blocks_readiness` | `reason-4 ambiguity retained under recovery hold` | `365e5d52bc7f372996715140738ca7a93e72ff9e84c366ae1b506b6d10b318fe` | `OutcomeUnknown_QUARANTINE_BLOCK_READINESS` |
| `journal_capacity_refusal_preserves_unresolved_evidence` | `1024 unresolved frame items plus request for reserved pair` | `a46f8d34fcaf3c85a137148e9efab0fc7fed98276118fc6ee415f638f072e2ba` | `JournalCapacityExhausted_EVIDENCE_PRESERVED_PRE_EFFECT` |
| `pack_len_cap_n_minus_1` | `pack_len=67108863` | `9d04439e03d37d33c53052f6472fde5915d4435a7bee877099710ba13e528b5c` | `PACK_CAP_ACCEPTED` |
| `pack_len_cap_n` | `pack_len=67108864` | `e77ce012854f2b6973e7cd9eb3bbacaad989373b06df8126de3ed901aa4791ab` | `PACK_CAP_ACCEPTED` |
| `pack_len_cap_n_plus_1` | `pack_len=67108865` | `56f779b02aca33a04f8a5785127dede81a8f110b6ea8877b158fbfccb889452b` | `PACK_CAP_REFUSED_PREALLOCATION` |
| `record_count_cap_n_minus_1` | `record_count=466031` | `c89b3ff5953e40e95e87aedbcc23efa5dde9f47aaaa1cb689c074255c4b76e14` | `PACK_RECORD_CAP_ACCEPTED` |
| `record_count_cap_n` | `record_count=466032` | `8e2b6c589025a8304429789f9e1c58b410fee452813ca30284e3bded7e8757d9` | `PACK_RECORD_CAP_ACCEPTED` |
| `record_count_cap_n_plus_1` | `record_count=466033` | `b610b59e6290f19ad316fdcb978f9c48bbf832a9397590d2bc78b9d5b0e8d72f` | `PACK_RECORD_CAP_REFUSED_PREALLOCATION` |
| `index_len_cap_n_minus_1_entry` | `index_len=37282480` | `7db1e292b6204452187b12ad50254797c375920eae9b85c226b2e963806856da` | `PACK_INDEX_CAP_ACCEPTED` |
| `index_len_cap_n` | `index_len=37282560` | `ab8ed748ae7068c3dacac371d0f94cb46944f7b43e61485c3a086af7b1d04653` | `PACK_INDEX_CAP_ACCEPTED` |
| `index_len_cap_n_plus_1_entry` | `index_len=37282640` | `0bd559d7acddabc73e5ebf44dd72457e3e8ef48de3aff6506a302aadb8488e13` | `PACK_INDEX_CAP_REFUSED_PREALLOCATION` |
| `index_len_literal_n_minus_1` | `index_len=37282559; exact N±1 byte boundary` | `ba60fa6ce2b3a4651838fb89f8fa670f02dfa836ca889c654b026309d8146c9d` | `PACK_INDEX_CAP_REFUSED_PREALLOCATION` |
| `index_len_literal_n_plus_1` | `index_len=37282561; exact N±1 byte boundary` | `a32a570c58bf8fa995a826f01ae5e14c410edcb63beb79194093ca41ebcf4a12` | `PACK_INDEX_CAP_REFUSED_PREALLOCATION` |
| `group_rollover_unique_record_valid` | `descriptor_count=7; distinct typed key; one prior-pack byte window and one prior-pack FD live` | `c5ad131fd0f332dabd1230c79b282c60da44c0d54b311634d88f77dfd5a51f54` | `GROUP_UNIQUE_RECORD_ACCEPTED_ONE_PRIOR_WINDOW_FD` |
| `group_rollover_unique_record_at_last_appendable_boundary` | `descriptor_count=1023; distinct typed key; the 1024th and final representable pack may be opened only after the full precharge; one prior-pack byte window and one prior-pack FD live` | `708f046ca2f600cf40caa6cc2e2a43dc24b60b27e9eb6234edc7c4e61c12be6f` | `GROUP_UNIQUE_RECORD_ACCEPTED_ONE_PRIOR_WINDOW_FD` |
| `group_rollover_exact_equal_reuse_at_descriptor_boundary` | `descriptor_count=1024; same typed key; authenticated candidate bytes/length/EOF exactly equal prior record; one prior-pack byte window and one prior-pack FD live` | `96198277b99735eeca460ef53be4d27e4f46bae7295a27b0c6db3d0084e69ad6` | `GROUP_EXACT_EQUAL_REUSED_NO_SECOND_RECORD` |
| `group_rollover_same_key_different_bytes_hostile` | `descriptor_count=1024; same typed key; authenticated candidate differs from prior record before exact EOF; one prior-pack byte window and one prior-pack FD live` | `ff1a87886977a3900e1af52b53dd7144d8d3338b00f0a5e19cf6b741ffffa454` | `GROUP_SAME_KEY_DIFFERENT_BYTES_FAIL_CLOSED` |
| `group_rollover_distinct_record_at_pack_ceiling_hostile` | `descriptor_count=1024; distinct typed key after the group already contains the maximum 1024 pack descriptors; refuse before allocation, open, or append` | `f2cfcc6b9b3a005ade51065d68dc815a97db8e5aee07dea858d85428cd6dfac5` | `GROUP_DISTINCT_AT_PACK_CEILING_REFUSED_PRE_EFFECT` |
| `group_rollover_descriptor_owned_exact_boundary` | `owned(1024,168)=172096; one prior-pack byte window and one prior-pack FD remain the probe maximum` | `56ae6f611447a479fb49f04cc4f629ad2c217bdefab5c608cf4a5a6a3f13d4d5` | `GROUP_DESCRIPTOR_OWNED_172096` |
| `group_rollover_descriptor_n_plus_1_hostile_pre_effect_refusal` | `descriptor_count=1025; existing prior-pack probe stays at one byte window and one FD; no extra open, allocation, or append begins` | `4d67e9895dcd3aa0e6a6b84fd7938ad820fdaddd53e0e3b624ef967a39b3c933` | `GROUP_DESCRIPTOR_N_PLUS_1_REFUSED_PRE_EFFECT` |
| `pack_validator_fixed_state_minimum_receipt` | `byte_window(4096)=4160` | `c2d4670f560596f7b1fc24c0a2f538f2e50d606eb5973929dc7d00a9d4460acd` | `PACK_VALIDATOR_FIXED_RECEIPT_4160` |
| `standalone_validator_minimum_recovery_gc_receipt` | `2*byte_window(65536)+byte_window(3728320)+2*byte_window(65536)+byte_window(4096)=3994944` | `eb8b211dbc9505881cc8cc8e2dc31c0da4f1dcd70ef4c1c17c1c85e55f9aecf7` | `STANDALONE_VALIDATOR_RECEIPT_3994944` |
| `standalone_validator_minimum_recovery_gc_slack` | `4194304-3994944=199360 recovery/GC bytes` | `740a89b22ea9a6c8688c9f9c19900fd4ece460d68ad6ad8c93846f243ced35dd` | `RECOVERY_GC_SLACK_199360` |
| `writer_validator_descriptors_minimum_foreground_receipt` | `byte_window(65536)+byte_window(65536)+byte_window(131072)+byte_window(262144)+3994944+owned(1024,168)=4691584` | `6e6e9adc7cb96224e67f912e6314ff56261d7c298b712c0164e93291581d37ba` | `WRITER_VALIDATOR_DESCRIPTORS_RECEIPT_4691584` |
| `writer_validator_descriptors_minimum_foreground_slack` | `10485760-4691584=5794176 foreground bytes` | `67f0193292b64dbe534f67dbfb19ec5eccb088f013c4be56f876e5ba8d753f9f` | `FOREGROUND_SLACK_5794176` |
| `writer_immediate_validator_indivisible_foreground_owner` | `mode=writer_immediate; foreground=4691584; recovery_gc=0` | `f3146d0ff83ab87b012cf40c6d17fcde5cbff3e9844d946625a331326c17bfc4` | `COMPLETE_WRITER_VALIDATOR_DESCRIPTOR_SET_FOREGROUND_OWNED` |
| `standalone_validator_indivisible_recovery_gc_owner` | `mode=standalone; foreground=0; recovery_gc=3994944` | `06898159f1082d09b52c78e7fa55d33d6a3b2ae8a663c1164b704a3a1db24a82` | `COMPLETE_STANDALONE_VALIDATOR_SET_RECOVERY_GC_OWNED` |
| `validator_split_bucket_owner_hostile` | `mode=standalone; foreground=1; recovery_gc=3994943 (same complete validator set split across owners)` | `77c2ae61f96b9ed37635381aaf22824c5e67e60c7c81d8fac69035d9921e85c7` | `SPLIT_BUCKET_VALIDATOR_OWNERSHIP_FORBIDDEN` |
| `minimal_pack_at_record_cap` | `checked_minimal_pack_len(466032)=67108752` | `4067ff3d529a0738bf53590566ebbc4b9620f136c99e9200126c53c09dcf3fee` | `MINIMAL_PACK_67108752_ACCEPTED` |
| `minimal_pack_above_record_cap` | `checked_minimal_pack_len(466033)=67108896` | `a711f5b2349f07549b114bde018b6c33ed44f5ec2ed2e6926254fa8d6c1cec27` | `MINIMAL_PACK_67108896_REFUSED` |
| `pack_checked_arithmetic_overflow` | `checked_minimal_pack_len(usize::MAX)` | `484136d258f8b15f582ae89654a8c157a6d28aa66ecb2af058cf8c37867f9ad4` | `PACK_ARITHMETIC_OVERFLOW` |
| `run_flush_46604` | `46604 fixed 80-byte entries` | `f4572e3e1c7cd0848ffc9a5c240b11fda9213acf1a05d65e5f147c6d6b89e673` | `ONE_INITIAL_RUN` |
| `run_flush_46605` | `46605 fixed 80-byte entries` | `b7f0aaed3ec2967788a8a581910c1ab95b41b13efe01d88538af01b44baea465` | `TWO_INITIAL_RUNS` |
| `ten_run_two_way_first_merge_file_bound` | `10 input runs plus ceil(10/2)=5 output runs` | `b4b07b346e9cf6982488edf08fdf2b93f406bcf2dc5beb6ef4da06b897a12e2b` | `FIFTEEN_FILES_WITHIN_R28_MIN_16` |
| `spill_exact_boundary` | `physical spill charge=74687985` | `aead90635cdc415099bd39d3f016d92392d459224a05511c719a3a2a702c8c52` | `SPILL_BOUNDARY_ACCEPTED` |
| `spill_subcap_refusal` | `physical spill charge=75000001` | `ef1db332fa4219ce4a945fd0fd6c48357925e192cac6c59771ff28e05f8fcf90` | `SPILL_REFUSED_PRE_EFFECT` |
| `spill_subcap_exact_boundary` | `physical spill charge=75000000` | `e9c58172dfaec001cc9bb23799170c39e094d421e1634f11020bc334e76598fd` | `SPILL_SUBCAP_EXACT_ACCEPTED` |
| `bounded_offset_order_validation_at_record_cap` | `466032 synthetic 64-byte record locators iterated without a max-size pack allocation` | `8faf4f00776738de8de5f0abbc27f78e52913483d9d1816b59f378307c753acd` | `BOUNDED_STREAMING_VALIDATION_ACCEPTED` |
| `streaming_construction_residency_counters` | `payload_staging=0; whole_pack_resident=0; whole_index_resident=0; max_run_entries=46604` | `519865676d72fcaf8a51ef1f33094eaaf127b6298f695e2afd3fb279fb5e5d40` | `BOUNDED_RESIDENCY_COUNTERS_ACCEPTED` |
| `changed_source_checksum_and_independent_validation_read_counters` | `changed_source_reads_max=1; checksum_construction_carrier_reads=1; completed_carrier_validation_reads=1; completed_validation_independent=true; second_changed_source_ordering_reads=0; second_selected_old_carrier_ordering_reads=0; pack_rewrite_passes=0; whole_pack_resident=0; whole_index_resident=0` | `dc7e95e2004c19a79d37c2f37485cf95f45883dc6242fb0e12fd924515e34bf7` | `EXACT_ONE_CHECKSUM_READ_ONE_INDEPENDENT_VALIDATION_READ_NO_REREAD_REWRITE_RESIDENCY` |
| `canonical_object_closure_two_carrier_orders_and_partitions` | `closure=chunk,file,symlink,nested-tree,root-tree,version-record; closure_keys=6; VersionId=f2dfceb5f1618031b99634897ddc5c760421fcd92b53f8715b776a127e40effa; carrierA_packids=d43f0f9dea3adb23ffb8ae1176824e5b27757d76aa5c10131b280f419d19e277; carrierB_packids=0064c297c2560981496559149babfe1e57c776be526a80784bcaf70629e9dafd,a5bfd1fded95bc7d74eab6c32d38688de1c55b390d6a6f2e4ce5b2de7ce752c6` | `ad553b09e060b138953be2f5316b1275b911c6c117e379bab269897dcc6b85cc` | `OBJECT_IDS_AND_VERSION_ID_INVARIANT_PACK_IDS_MAY_DIFFER` |
| `catalog_overlapping_descriptor_ranges_without_duplicate` | `two exact descriptors have overlapping min/max ranges; global typed keys are disjoint` | `fa4464438aa9c16d9d56a2ed85a5de70468c96ec2b1c7651aa90af2a67edfef1` | `CATALOG_ACCEPTED` |
| `catalog_cross_pack_duplicate_typed_key` | `two authenticated packs contain the same typed (kind,id)` | `b83ef715bd4d03e5406f0cc590d02f04b1286e707157da3fde3f668b167061c1` | `C_CROSS_PACK_DUPLICATE_KEY` |
| `catalog_descriptor_min_key_crosscheck` | `descriptor min_key replaced by its max_key while pack bytes remain authenticated` | `3748154bb95cc18ecd04041906cfa32d4e84da2fd50ed76f027c718ef7fb8dfe` | `C_DESCRIPTOR_PACK_MISMATCH` |

<!-- END GENERATED M6.0 VECTORS -->


## Cost and scope assertion

The structural sentinel reuses the existing `u16` mode field and therefore
adds zero bytes, traversal, allocation, and payload I/O. Receipt processing is
one sequential bounded parse/authentication pass over at most 1 MiB of
borrowed receipt input plus at most 2 MiB of precharged decoder-owned capacity
and 4 KiB of bounded error disclosure. It hashes no Workspace payload and
opens no unchanged file. Queue descriptors retain only bounded scalar IDs,
generation/sequence values, and receipt digest—not receipt bytes or fact
lists. Direct-pack discovery never spills payload, rereads a changed source,
retains or rewrites a whole pack, or changes object/Version IDs; only fixed
80-byte index metadata enters the charged bounded sorter. These properties are
contract requirements; the vectors establish grammar, state transitions, and
bounds, not a performance guarantee.
