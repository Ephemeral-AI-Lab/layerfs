# Prospective G4 materialization acceptance v10

Status: frozen before any v10 preparation or measured child.

V9 remains immutable `MEASURED_PROTOCOL_REVISE`. Its 30-record / 50-arm evidence is internally strong, but its analyzer deleted the original protected <=5% failures and substituted 3/10/5-ms caps after v8. V9 sequence 8 measured 2,359,166 ns candidate versus 2,188,083 ns control, or +7.8189%, so it fails the unchanged engineering guard. V9 is not reanalyzed, rerun, or promoted.

## One v10 source repair

V10 changes the candidate source once before measurement:

- requested-visible M0 reconciliation retries and requires parent-directory `fsync` acknowledgement before success, matching the accepted G3 state machine;
- the original rename/directory-sync syscall failure remains typed `CoreError::Io`, while reconciliation and retry failure remain separate provenance;
- clone-temp custody is armed immediately after successful `fclonefileat` and before subsequent fallible binding work;
- cleanup atomically moves the current public name to a cryptographically random exclusive/no-follow quarantine name, validates the quarantined identity against retained descriptor/device/inode custody, and unlinks only that validated quarantined identity; a substitute is restored or preserved, never blindly unlinked;
- operation, verification, Q, cleanup, residue, and reconciliation failures retain execution-order first-error provenance inside `G4NativePublicationFailure`;
- authority increments and derived SQL/row counters use checked arithmetic; remaining reporting `saturating_sub` operations are removed;
- closure-on/off now rejects both identity-invalid canonical bytes and an identity-valid authenticated malformed mapping with the exact same `LengthMismatch` payload and terminal Q=0;
- the predecessor-aligned bounded-rejoin range is capped at exactly 1,048,576 bytes, removing the v9 1,085,490-byte old/scan buffers without narrowing the buffer contract;
- the allocation choke point records `max_single_buffer_bytes`; current candidate payloads also report `buffer_evidence_complete=true` and `full_file_buffer_bytes=0`.

Focused G4, clone fallback, predecessor-aligned edit-buffer, full workspace, clippy-all-targets, rustfmt, and diff gates pass before this freeze: 161 tests passed, 1 intentionally ignored, 0 failed.

## Original protected gate, fixed tiny estimator

Every protected G3 and fast route retains the original exact rule:

```text
sum(candidate samples in the frozen route) * 100
    <= sum(control samples in the frozen route) * 105
```

There is no absolute-cap alternative, issue deletion, tolerance band, outlier removal, adaptive sample count, early stop, replacement, or retry.

The following route set is frozen from sealed v9 history because both v9 operation-local values were below 10 ms:

```text
8,
16, 17, 18, 19, 20,
22,
24, 25, 26, 27,
29, 30
```

Each listed role runs exactly two fresh isolated observations. Its decision uses the equal-weight arithmetic mean, implemented without rounding by the exact two-sample sum inequality above. Both raw values, commands, external observations, stdout paths, and hashes are retained. The top-level logical arm stores the ceiling of the mean only for compatibility; the exact sum is normative. No confidence, significance, or population-mean claim is made.

G3 sequences 21 and 23 and full-create sequence 28 retain one fresh adjacent control/candidate pair because their sealed v9 intervals were not sub-10-ms. They still receive the same exact <=5% rule. Thus all 16 protected routes are gated.

The logical schedule remains 30 records and 50 arm envelopes. The fixed estimator adds 26 children, for exactly 76 measured payload observations. No earlier payload is imported. The operation-local <=20-second gate includes all 76 observations, not merely the 50 aggregate envelopes. Every estimator pair must preserve exact within-role work and exact control/candidate semantic/work parity. Fast routes gate the enumerated 28-field parity list frozen in v9; G3 gates every shared non-timing field.

## Explicit buffer evidence gate

The candidate allocation metric is updated at the single checked capacity-allocation choke point. Every current-candidate observation—including every estimator repetition—must report:

```text
buffer_evidence_complete == true
full_file_buffer_bytes == 0
max_single_buffer_bytes <= 1,048,576
```

The analyzer also gates any reported `buffer_bytes`, `q_cdc_old_window_bytes`, `q_cdc_scan_input_bytes`, `q_cdc_old_chunk_slots_bytes`, `leaf_batch_query_bytes_max`, `q_report_output_bytes`, and returned range buffer against 1,048,576 bytes. Cumulative authentication, native-write, BLOB, and Q counters are not mislabeled as buffers.

The frozen G3-v13 and protected-control executables do not emit the new field. Their exact executable hashes remain bound to the previously frozen <=1-MiB source/static buffer proof, and every such control observation is labeled `frozen-*-source-static-bound` in the ledger. Missing candidate evidence or an unknown control hash fails closed. The normalized ledger must contain exactly 76 buffer-evidence entries and a campaign maximum <=1,048,576.

## Frozen custody

- Repository / branch / HEAD: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`, `codex/empty-worktree`, `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`.
- Candidate executable: `770dcfa8db17f1f9e1b90336a26923eb0530073590a9da5578e06339d85813e8`.
- G3 control: `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`.
- Protected control: `5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5`.
- Round-1 handoff: `8ca584b9e7958ac57e28e994e1e9bd5638b7d1c703ace1693b1b58706da07d00`.
- Rust sources: benchmark `54f5a88e79235db31c5c4f26166371d6a0e7bede23265d533fb278ab4455b1f2`; G3/G4 module `efd03e2961e4c762528b0fbbcd7843b069fe953eebbf01d75c0443faec67b381`; unchanged Canonical-v2 `8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc`; Cargo.lock `70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8`.
- Frozen v8 runner: `22e924e37ddba807917818acefeffe1c7feeec290b1ab64847c2d9e3dfa14de4`.
- V10 schedule / runner / primary / independent: `5927c9506c338db117f25486caee8215f3cdc069f4f56b6a0505e77d0a67aabb`, `72215e6d4f18282b6e72a86d6b1887cf955ccc38c986f9e8c8263eec07e21627`, `fee060dbdd083fd04900dadadba8aa623ba4083bcec9726e8dc4ce3b8dfca792`, `0a91f45112c612ff39df890cb9db8190f1c621787946431200ee3caa0f29cf32`.
- V9 pre-exec history: `736a415957ad0e787e72bb51eb16325c4c1af95b3e9f4d15e4410260791e50aa`.
- V10 result root: `target/phase4-g4-materialization-acceptance-20260822-v10/`, required absent before atomic `target/BENCHMARK_LOCK` acquisition and never reusable.

All other v8/v9 substantive gates remain: R1 <=333 ms and >=5% direct closure-fold improvement; fresh/M0 <=400 ms; seed no-digest <=50 ms; exact S1-100 SQL/row/BLOB/authentication/write shape; G3 10/10/20-ms absolute routes; M0 checked writes, direct sync/metadata/rename/dirsync/reconciliation counters, descriptor verification, source-sequence binding, and zero scanned residue; correct seed/cache labels; every whole child RSS <=20 MiB; all terminal Q zero; two controlled-cold and physical-I/O cells honestly unavailable; equal primary/independent ledgers; cleanup root `work-v10`; and `BENCHMARK_LOCK` held through fsynced terminal verification then released with a separate attestation.

The prospective bucket limits are 5/85/40/5/5/5/10 seconds. The private and row buckets are reallocated only for the fixed extra estimator work; the global complete-wall ceiling remains exactly 120 seconds. If the complete run or any individual gate fails, v10 is terminal REVISE with no rerun or gate change.

After measured PASS, final read-only correctness/evidence audits and documentation freeze are required. G5 remains blocked, out of scope, and not started. No commit is authorized.
