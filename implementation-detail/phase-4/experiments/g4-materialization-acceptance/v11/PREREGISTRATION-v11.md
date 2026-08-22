# Phase 4 G4 v11 preregistration

Status before execution: `PRE_EXECUTION_FROZEN`. This is a fresh campaign. V9 remains `MEASURED_PROTOCOL_REVISE`; v10 remains `PRE_EXEC_REVISE_ABORTED_INVALID_EXECUTION`. No prior row, arm, child payload, or result is reused.

## Frozen custody

- Branch/HEAD: `codex/empty-worktree` / `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`.
- Candidate executable SHA-256: `789f53068793b8aeea0d03924b1dd985cc6a0d4706c98188f2034b216ca3adcb`.
- Benchmark source SHA-256: `10aa474bf5fe5130fde73703444f5082bfa4a41a3c82dd0d84a025bedd9485a2`.
- G3/native source SHA-256: `326cf27c61915501611d5a1876392ee8b93c10038adeae3d8986c13cfc314f71`.
- Canonical-v2 source SHA-256: `8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc`.
- `Cargo.lock` SHA-256: `70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8`.
- Frozen G3 control SHA-256: `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`.
- Frozen protected control SHA-256: `5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5`.

The runner checks the methodology manifest, all four sources, live candidate, and controls before work. Both analyzers independently recheck the four sources. While the owner-bound lock remains held, terminal verification rechecks the four sources, live candidate, measured candidate snapshot, and both measured controls.

## Source contract frozen before measurement

- Rejoin search retains predecessor start through `edit_end + 1 MiB`, capped only by file length.
- Fragmented authenticated input preserves that full search while every owned byte buffer is at most 1 MiB.
- `RangeSegments.values` is sized from the requested range by checked `rejoin_chunk_capacity(requested)`, including two sliced-boundary slots, never from the file-wide reference count.
- Exact 1 MiB replacement identity is compared with a complete independent rebuild. Boundary reading and EOF exact-complete fallback are typed tests.
- Requested-visible native publication retries and acknowledges directory fsync; the original typed first I/O cause remains separately recorded.
- Successful clone creation immediately acquires no-follow descriptor and inode identity. Unbound post-clone failures return typed unresolved cleanup. Bound cleanup never removes a substitute.
- Authority and durability counters use checked arithmetic.

## Frozen measurement and estimator

The logical schedule is the frozen v1 matrix: 30 records and 50 logical arms. Exactly 13 prospectively selected sub-10-ms/noisy protected routes use two samples per role: sequences `8,16,17,18,19,20,22,24,25,26,27,29,30`. Their execution order alternates:

- even estimator index: control-1, candidate-1, candidate-2, control-2 (`CPPC`, compatibility label `ABBA`);
- odd estimator index: candidate-1, control-1, control-2, candidate-2 (`PCCP`, compatibility label `BAAB`).

Together with the 24 one-shot logical children, this produces exactly 76 timed child executions. Every child command, exact global order, role, sample, executable hash, stdout hash, stderr hash, start/end monotonic time, and parsed external observation is appended to chronology and frozen in `COMMANDS-v1.json`.

Both analyzers independently reopen and hash all 76 stdout/stderr files, parse every `/usr/bin/time -l` stderr into real/user/system seconds, maximum RSS, voluntary switches, and involuntary switches, hash `command[0]`, enforce the exact role-to-binary mapping, bind raw payloads to logical arms/estimator metadata, and require exact 76-entry global order.

The protected relative gate is unchanged and exact:

`sum(candidate_ns) * 100 <= sum(control_ns) * 105`.

There are no micro-caps, adaptive samples, deletions, outlier rules, or alternative tolerances. For the 13 estimated routes only, the frozen v1 rounded-mean adjacent decision is prospectively replaced by the exact raw-sum decision to avoid ceiling-rounding disagreement. Every other frozen issue decision—including one-shot sequences 21, 23, and 28—remains untouched.

## Frozen global gates

- Complete wall: strictly less than 120,000,000,000 ns from global lock attempt through fsynced terminal verification.
- Bucket partition, exact sum 120 seconds: lock/preflight 1s; preparation 80s; row dispatch/operations 31s; exact verification 1s; analysis 1s; cleanup 1s; terminal/verification 5s.
- Maximum RSS: at most 20,971,520 bytes for every independently parsed timed child.
- Measured operation sum: at most 20 seconds.
- Every candidate observation must provide complete direct buffer evidence with `max_single_buffer_bytes <= 1,048,576` and no full-file buffer. Applicable create/edit rows must provide nonnegative segment/slot/query/report maxima at most 1 MiB. Cumulative fragmented-window charges are not misclassified as single buffers.
- Exact record/arm/child counts; equal primary/independent normalized ledgers; Q/residue/manifest/cleanup closure; direct M0 durability counters; seed cache class; frozen cache profile; exact protected work parity.

## Lock and cleanup contract

Custody is retained on the exact `O_CREAT|O_EXCL` descriptor immediately after open. A checked random token is written and synced; the frozen legacy write and unlink are inert. The lock stays held through fsynced terminal verification. Success or failure rewrites the retained inode as a token-bound JSON attestation, fsyncs it, atomically renames the public lock to a unique sealed attestation, fsyncs the affected directories, and verifies descriptor/inode/token/content plus public-name absence. No public pathname is unlinked. Cleanup must name and remove only `work-v11`.

## Scope boundary

`research/phase-4/g5-round-0` is concurrent, premature foreign work already present in the shared tree. V11 does not edit, hash, import, or include it in G4 custody, and authorizes no further G5 activity. G4 completion will be reported with that qualification rather than claiming G5 has never been started.

Exactly one v11 campaign is authorized after the frozen dry-run and custody verification pass. No v9/v10 rerun and no unchanged-source noise attempt are authorized.
