# Prospective G4 materialization acceptance v9

Status: frozen before any v9 preparation or measured row.

V9 is an acceptance-estimator repair only. It changes no production source, executable, operand, schedule, cache profile, workload, or durability rule from v8. V8 remains an immutable complete 30-record / 50-arm terminal REVISE campaign. Its range, edit, and reopen cells each contain one observation per arm; their raw adjacent ratios are retained but cannot resolve a 5% relative effect without prohibited reruns. V9 therefore preregisters hard absolute engineering caps for those micro cells and exact semantic/work parity. These caps were selected after reviewing the v8 repair history and are prospective v9 acceptance thresholds, not statistical confidence claims:

- protected range candidate: `wall_ns <= 3,000,000`;
- protected edit candidate: `durable_capture_total_wall_ns <= 10,000,000`;
- protected reopen candidate: `fresh_reopen_head_wall_ns <= 5,000,000`;
- protected full create remains subject to the unchanged exact adjacent 5% relative noninferiority gate.

For range, edit, and reopen, both analyzers also require exact control/candidate equality for every one of these fields: `root_id`, `transition_id`, `source_fingerprint`, `actual_cdc_references`, `expected_cdc_references`, `expected_cdc_sequence_fingerprint`, `ordered_closure_digest`, `publication_status`, `error`, `q_current`, `edit_reference_count_before`, `edit_reference_count_after`, `edit_count_classification`, `sql_query_calls`, `sql_rows_returned`, `row_blob_reads`, `borrowed_row_blob_reads`, `borrowed_row_blob_bytes`, `objects_authenticated`, `canonical_bytes_authenticated`, `leaf_batch_queries`, `leaf_batch_references`, `leaf_batch_references_max`, `source_bytes_read`, `source_cdc_bytes_read`, `canonical_stage_source_bytes_read`, `w_bytes`, and `d_bytes`. `q_high_water` is intentionally excluded because serialized arm-size differences affect the reporting peak; terminal `q_current` and all underlying work counters remain exact gates.

The raw control/candidate values and ratios remain in `ARM-RAW-v1.jsonl` and both normalized ledgers. The ledger labels micro relative noninferiority `Unavailable`; it does not relabel a reversed one-shot ratio as a statistical pass.

Frozen custody:

- Repository / branch / HEAD: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`, `codex/empty-worktree`, `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`.
- Candidate executable: `c60a19cb3cecb83bb801ba9c36835297e6fc503d736171213ec78e69bd5d6d76`.
- G3 control: `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`.
- Protected control: `5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5`.
- Round-1 handoff: `8ca584b9e7958ac57e28e994e1e9bd5638b7d1c703ace1693b1b58706da07d00`.
- Rust sources: benchmark `eb00674125d18da66253b31949ecba2f874b64ec6a93ad68fe251d4f0649d169`; G3/G4 module `32c8185c3cbc5b444ba0a533ea5f1bd9332b16eb358b9c5540c0ab534ac3f8d9`; unchanged Canonical-v2 `8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc`; Cargo.lock `70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8`.
- Frozen v8 runner / primary / independent sources: `22e924e37ddba807917818acefeffe1c7feeec290b1ab64847c2d9e3dfa14de4`, `cb67568c6d95b3da5ac98623cbd54fa9220527bc5cf7d4c02bc2e294f8e6ab25`, `539a7cf73fbd809cc0c9e4e3633e05c2a1c8349e34d240d4141de515cb451de8`.
- V9 schedule / runner / primary / independent sources: `09f07061cf6c14e791e1f91156881a3a775555bd395c8b108e50b28e5879daec`, `871f945464ba395b3d25a5ec740295ffd4f079f4ca51c9c9c366ad213f241ca5`, `b853c590eabee363e93b2e11408c81a7b01d0ed70762b2033b2bda95dad7b75b`, `76f69a6c7a9b22a58c14e7c70482cb10bbe4dcd2c48c0cfd937d6ec187c9c1d5`.
- V8 terminal / verification / wall / payload manifest / lock release: `6119e1cd8086c6ba80c0f05ba371a3e98827265bd6ff4ac4952fe1f5dcd88930`, `5d53a8679f335324e9a80d4cf61073cfe88eff8fed1207b07f63338a1522cd2f`, `62f5fb02c2d8d0a4e99398f5b513b7f273aa46a4560fbaf6a2c9a4004d5c8bc9`, `e8bca8ce811bf4e6cb0e135621261f21169fd7883d47a5feb2c3621c93a4ad40`, `ede49838bad911fcabf32b03c1c556516f6d19056ca6feb481203e768b2dee46`.
- V8 methodology manifest: `aca5dfdfafa013f5b961fd81bcb1c06a6c31a7c0cac8972308bdd22846cfdaec`.
- V9 pre-exec history: `34951b775af676bdcf4b28a5ebc9ef937c3f51de454480a722b22d8b83a16427`.
- V9 result root: `target/phase4-g4-materialization-acceptance-20260822-v9/`, required absent before atomic `target/BENCHMARK_LOCK` acquisition and never reusable.

All unchanged v8 gates remain exact: M0 absent-prior old-or-new reconciliation and focused fault proof; checked writer and direct durability counters; descriptor-only verification; source-sequence binding; identity-bound cleanup and scanned residue; seed cache class; exact G4 S1-100 SQL/row/BLOB/authentication/write shape; R1 <=333 ms and >=5% direct improvement; fresh/M0 <=400 ms; seed no-digest <=50 ms; G3 semantic/Q/residue and 10/10/20-ms targets with relative inference Unavailable at n=1; every whole child RSS <=20 MiB; buffers <=1 MiB; operation-local sum <=20 seconds; zero terminal Q/residue; two cold/physical-I/O Unavailable cells; independent ledger equality; cleanup root `work-v9`; `BENCHMARK_LOCK` held through fsynced terminal verification and released with a separate attestation; 5/65/15/5/5/5/10-second buckets; and complete wall <=120 seconds.

The run is one new complete campaign only. V5, v6, v7, and v8 are not rerun; no earlier row is imported. After measured PASS, one final workspace test closure and fresh read-only correctness/evidence audits are required before terminal G4 closure. G5 remains blocked, out of scope, and not started. No commit is authorized.
