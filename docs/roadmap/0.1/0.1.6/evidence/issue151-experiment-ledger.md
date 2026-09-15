# #151 experiment ledger: sandbox-local snapshots from v0.1.6

Append-only evidence ledger for the combined #149 + #150 experiment executed
under the [#151 pipeline](../experimental-implementation-pipeline.md).
Every entry records stage, source identity, command, result, metrics versus
limits, raw receipt paths and next action. Failed and invalid attempts are
retained. A missing required metric is INCOMPLETE; a valid numerical miss is
FAIL.

- Profile/revision: `v016-local-snapshot-experiment-v1`
- Implementation branch/worktree: `codex/v016-sandbox-local-experiment` at
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-v016`
- Source base: peeled `v0.1.5^{commit}` = `6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13` (verified)
- Control: sealed v0.1.5 product built from the same commit in this worktree
- One performance sample per case per arm; separate correctness verification.

## Frozen gates (from #149, recorded before any measurement)

For each control time T0: candidate <= T0 + max(0.15 * T0, 3 ms) — applied
independently to exec, complete Commit, End/required cleanup, complete
public-call workflow, and each B3 C1/C2/C3 cycle. For control total workflow
CPU C0: candidate <= C0 + max(0.15 * C0, 1 ms).

Resource gates: 64 MiB aggregate accounted algorithm allocations (capacity
charged); 8 MiB combined transfer/staging buffers within that total (preserve
the existing 8 MiB final-delta allowance); process-memory sum <= control +
max(15%, 8 MiB); B1/B2 temporary physical backing <= control + max(15%, 1 MiB);
B3 temporary physical backing <= 32 MiB peak. B2's legitimate 500 MiB new
payload is exempt from the 32 MiB backing ceiling only. Sandbox envelope:
2 CPU / 2 GiB / no swap / 256 PIDs. 300 s product / 310 s outer timeouts.

Cases, in order, one sample per arm each: `tiny-create-500-mixed-v4` (B1) →
`tiny-bulk-create-500-mixed-v3` (B2) → `local-snapshot-create-25000-onebyte-v1`
(B3, three distinct Commits C1/C2/C3 in one lifecycle sample).

---

## Ledger

### L0 — 2026-09-15: handoff read, environment verified, worktree created (I0 start)

- Read the ordered handoff documents (pipeline, spec, connection architecture,
  review record, research review, v0.1.5 release contract, benchmark AGENTS.md
  exception, benchmark_rules.md exception, QUICKSTART, tiny_file_churn family).
- Verified `git rev-parse 'v0.1.5^{commit}'` = `6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13`.
- Main checkout at `7b73c4b33` preserved untouched (uncommitted scoped-exception
  docs remain there).
- Created worktree `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-v016` on new branch
  `codex/v016-sandbox-local-experiment` at the exact v0.1.5 commit.
- Environment: Docker 29.5.2 server running; rustc 1.96.0 (MSRV 1.85); Python
  3.14.3; 332 GiB free disk.
- Next: freeze docs in an attributable commit; read v0.1.5 source areas before
  editing.

### L1 — 2026-09-15: planning documents frozen (I0)

- Copied the five 0.1.6 experimental planning documents and the research review
  from the main checkout into the worktree; applied the scoped benchmark
  hosting/sampling exception patch to `benchmark/AGENTS.md` and
  `docs/general/benchmark_rules.md` (verified it applies cleanly on v0.1.5).
- Added this worktree's 0.1.6 README and the evidence ledger.
- Commit: `25002b8e5` — docs only; no product/benchmark code changes;
  dependency tree untouched.

### L2 — 2026-09-15: source-reading phase complete; design frozen (I0 exit)

- Four parallel read-only source maps completed over the v0.1.5 worktree:
  host backing/wire/transport/registry; Commit lifecycle +
  capture/reconcile/worker/checkpoint; SDK/daemon protocol + FUSE dispatch;
  canonical builder + Store publication contract. Full reports retained in
  session blobs; key structures verified by direct reading of
  `live_owner.rs`, `file_edit.rs`, `lib.rs` (workspace-core), `live_wire.rs`,
  `changes.rs` (prepare_page/produce_file/FrozenFile) by the implementing
  agent.
- Implementation design recorded in
  [issue151-implementation-design.md](issue151-implementation-design.md):
  local packed payload segments under `/snapshots/<id>/`; frozen-frontier
  capture with protect-on-mutation COW (moved dirty set + retained-node
  copies, fixed-size capture); new snapshot service lane for records/payload
  pull; host materialized frozen input into the unchanged single-worker
  builder; generation-guarded completion records replacing checkpoint
  install; volatile fsync; shared `LAYERFS_CONSTRUCTION_WORKERS=1` knob and
  B3 harness adapter applied identically to both arms.
- Safety finding verified in source: `FrozenFile::build`/
  `mutate_existing_file` guard piece-base vs before-root mismatches
  (fallback to `build_complete_with_predecessor`), so completion that leaves
  re-mutated nodes on their pre-capture piece base is safe (correct, with
  CDC-level predecessor locality).
- Commit: design doc committed with I1 start (see L3).
- Next: I1 implementation (local payload ownership + mount layout).

### L3 — 2026-09-15: I1 implementation started

- Local payload backing (`crates/layerfs-fuse/src/local_spool.rs`), backing
  dir plumbing, volatile fsync, host BackingOwner payload/facts removal,
  mount layout `/workspaces` + `/snapshots/<id>/`.

### L4 — 2026-09-15: I1–I3 daemon route complete (commit `ab2a6f9cb`)

- Sandbox-owned mutable state: LiveOwner writes payloads into `LocalSpool`
  under `/snapshots/<id>/` (write-before-apply, positioned writes, no
  durability flush); reads serve local segments; volatile fsync contract
  (validate + known errors, zero wire traffic).
- Stable owned snapshots: protect-on-mutation COW frozen frontier
  (`capture` fixes a generation; live writes continue; retained-node copies
  keep the snapshot readable while the live tree mutates).
- Bounded frozen transfer: new snapshot service lane — `SNAP_RECORDS` pages
  node records with bounded frames; `SNAP_READ` serves payload ranges with a
  bounded host-side window; commands/control/base reads keep progressing
  during transfer (no lifecycle lock, no queue monopolization).
- Host payload/facts wire opcodes and the checkpoint
  install/freeze/resume machinery removed from the route.

### L5 — 2026-09-15: I3–I5 host route complete (commit `55b531bc5`)

- `pull_frozen_input` materializes the frozen generation on the host;
  sandbox piece backings fetch through the bounded remote window
  (`read_backing_exact` dispatch — host segment direct read, remote bounded
  fetch), so the host payload spool is never used for remote payloads.
- `build_remote_candidate` feeds the unchanged single-worker canonical
  builder (CAS/dedup/CDC/FULL-DELTA selection, compression, packing,
  dependency checks) and publishes through the existing
  `commit_workspace_candidate` transaction.
- `complete_generation` applies completion records with revision guards;
  repeated Commits and Workspace isolation verified; End clean-validation
  observes sandbox dirty state through the daemon.
- Physical spool observations restored into commit diagnostics; remote
  verification state reports the materialized host spool only.
- Full workspace native suite green via the canonical runner:
  `RUSTUP_TOOLCHAIN=1.85.1 tools/test-fast.sh` →
  `PASS full workspace native tests in 111s with 4 bounded jobs`
  (raw log: `/tmp/v016_testfast2.log`; includes the rewritten
  LiveOwner full-route integration tests, the k100 point-lookup case, and
  the immutable-base opcode-rejection proof).
- Next: I6 focused checks (fastcdc/extent/small-chain suite subset), then
  B1 → B2 → B3 gates with freshly collected v0.1.5 controls.

### L6 — 2026-09-15: execution handed over at the measurement boundary

- Candidate state: HEAD `adfe869a3` (product commits `141244a3f`, `ab2a6f9cb`,
  `55b531bc5`; harness-only B3 adapter `adfe869a3`; ledger L4/L5
  `164efede7`). Working tree clean; `benchmark-results/` ignored.
- Correctness evidence: `tools/test-fast.sh` PASS (111 s, 4 jobs);
  focused `fastcdc_shifted_stream`, `extent_model`, `small_candidate`,
  `small_chain`; 96 harness tests; workload/host self-checks.
- Not started: candidate host/image build (one attempt interrupted after ~1 min
  of dependency compilation; `benchmark-results/host-store/builds/`
  `incremental-1.85.1-release` resumable), control worktree and build, smoke,
  gates B1–B3, final report.
- Continuation instructions for the receiving agent:
  [issue151-execution-continuation.md](issue151-execution-continuation.md)
  (locations, build/control recipes, gate flags, hard rules, carried
  limitations).
- Commit: `b124bbdd5` — docs only.

### L7 — 2026-09-15: Phase A builds — both arms built; first repair (candidate)

- Candidate arm (`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-v016`,
  `codex/v016-sandbox-local-experiment`), sealed identity after the repairs in
  L8–L10:
  - product commits `141244a3f`, `ab2a6f9cb`, `55b531bc5`; harness-only B3
    adapter `adfe869a3`; measurement repairs `1939f7781`, `e52aeaa98`,
    `fa642c5ea`, `e1a317a4d` (HEAD)
  - source tree `440e3aae4870b325f10dfffe58e635833915579d`, dirty=false
  - source seal `5447aeecbe1d713545e1635e091e0846e7ccd4905da7d389fde5621133e25536`
  - product seal `e03dae88c3226febdd5538c0d412c7e90de0d65018ede2e3fd3936bf7ff67dc2`
  - host binary sha256 `6099eaaafdf33ab7314be0383451d601e0037bc280a460d3bdee834c19bdbda8`
  - image `sha256:97be8c92b743d16444282907051a353e564757aa43d9c6a1ccb83ba65c172029`
    (tag `layerfs-bench-infra:5447aeecbe1d7135`), source-dirty=false
  - in-image binaries: daemon `cedd25a1351545312ad00e96f9e4d88e83339a455d6b49b8eda04e43c0cef454`,
    fuse `47345303fe57aad522e9c213bd7555091fe9835526116018c810e6ebb8f200b9`,
    workload `f2cf9b9f6e803638f6d29163182916ef10e761c3ea641ce6519f1639568f64ae`
  - first-party packages recompiled for the sealed host binary:
    `layerfs-fuse`, `layerfs-workspace`, `layerfs-monitor`, `layerfs-sdk`,
    `fs-benchmark-pro`
- Control arm (`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-v016-control`, detached
  at `6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13` plus the harness patch applied
  exactly as handed over):
  - source tree `739550b79159f4120cb60ddc8310b7cb7168c352`, dirty=true
    (harness patch uncommitted by design in the control worktree)
  - source seal `e5efac1de312f7441d064ec2a6eac4af221beba42a768788154d87ea3a839d93`
  - product seal `276c5970aabf485594d90ae30920b3cdb310134a7572589c558063a9d52ce093`
  - host binary sha256 `39d3c3ef92dd58cd6211863651dec2b5780f20f91fea92a01e8efd701ba56039`
  - image `sha256:179ee8a2b812af4674c26dd722e59de60cebc09b434bc0d1fe0bae8364c1bef5`
    (tag `layerfs-bench-infra:e5efac1de312f744`)
  - in-image binaries: daemon `8e3807fc4e74745c62366df265ddcb2d2318f20234f4ab6b34554a4d02e3e5f6`,
    fuse `ef64c44e10828a53186a20a8736f9f1c45fbbc1d4ad7601862f9c5c99868cb20`,
    workload `f2cf9b9f6e803638f6d29163182916ef10e761c3ea641ce6519f1639568f64ae`
  - first-party packages recompiled: `layerfs-content`,
    `layerfs-workspace-core`, `layerfs-materialization`, `layerfs-daemon`,
    `layerfs-fuse`, `layerfs-layerstack-store`, `layerfs-workspace`,
    `layerfs-monitor`, `layerfs-sdk`, `fs-benchmark-pro`
- Harness identity (the runner hashes `runner.py`, `runtime.py`, `cold.py`,
  `verify-selected.py`): `daa74be0c3a0b40a824617a6403c70ce4e7be115e7c47f4f1faf26029c55b044`
  in **both** arms. Harness files verified byte-identical across the worktrees
  (`runner.py dcca7391a01d9765499b1f33fdb05169c1a74bcf09147235ab68b1aeaf00c971`,
  `runtime.py 36fd4a6c2bb3df8a263ef827e24364105ff97b8390a21c65c91ed4a3669cbc7a`,
  `cold.py fef5391ed771155230d5f8920f8d4c3ada4d726eebac9ad4251b2e9e55a445cc`,
  `verify-selected.py 8c0f387861f93830ab4bcdd28f4ecdee97f7d7c5653021fac0af2a22b4daa84a`),
  and the workload binary hash is identical in both images. The product source
  is the intended, measured difference.
- Repair recorded here: the candidate image build failed outright —
  `crates/layerfs-daemon/src/main.rs` used `protocol::SNAPSHOT_ROOT`, which was
  never defined. macOS builds cfg the entire `mod linux` out, so neither the host
  build nor the native suite could see it and **no candidate image could be
  built**, i.e. no gate was reachable. Fixed in `1939f7781` by defining the
  owner-selected daemon-private root `SNAPSHOT_ROOT = "/snapshots"` and by giving
  the standalone `layerfs-fuse` helper (docker/legacy mount route, not the
  measured daemon route) the backing directory `LiveOwner::connect` now
  requires. Fast cycle after the change:
  `PASS full workspace native tests in 104s with 4 bounded jobs`.
- Next: Phase C smoke before spending any gate sample.

### L8 — 2026-09-15: smoke #1 FAIL (candidate, tiny-create-1-compact-v2) — spool metric unobtainable

- Command: `bash benchmark/fs-bench-pro/families/tiny_file_churn/perf.sh
  --case tiny-create-1-compact-v2 --seed 1 --setup clone
  --image layerfs-bench-infra:61d5952b9509a63c --perf-fast --collection-mode
  --output benchmark-results/issue151/smoke-candidate`
- Receipt: `benchmark-results/issue151/smoke-candidate/` (`failure.log`,
  `perf.jsonl`); FAIL, `error = "fs-benchmark-pro: physical spool current
  allocation unavailable"`, phase `product-command`.
- Cause: `Workspace::physical_spool_snapshot` returned `(None, None, 0, 0)` for a
  sandbox-owned Workspace, but the frozen harness reads
  `verification_workspace_state` before and after Commit and rejects an
  unobservable physical-spool allocation. The sandbox already maintained the
  numbers (`LocalSpool::physical_bytes`/`physical_peak`); the host never asked
  for them.
- Repair (`e52aeaa98`): `OBSERVE` additionally returns
  `LocalSpool::physical_peak_bytes()`; `RemoteWorkspace::observe` decodes the
  whole observation into a named `RemoteObservation` (the physical numbers were
  being dropped and the returned tuple order contradicted its own documented
  order); `verification_workspace_state` reports the sandbox's maintained
  current/peak counters (observation count 1, zero errors) instead of `None`,
  observing before taking the workspace lock. `is_dirty`, `summary`/`diff` and
  `session` keep their existing semantics. Fast cycle after the change:
  PASS 104s / 4 bounded jobs.
- Next: re-run the smoke.

### L9 — 2026-09-15: smoke #2 and #3 FAIL (candidate) — Commit never entered the new route

- Commands: the same smoke with images `layerfs-bench-infra:3fef887fdc562987`
  → `benchmark-results/issue151/smoke-candidate-2` and
  `layerfs-bench-infra:1def237c480c86d4` → `.../smoke-candidate-3`.
- Receipts: FAIL with `Workspace(InvalidExecution)` 267 µs into the Commit
  phase; create and exec had succeeded.
- Cause (two defects, one symptom):
  1. `commit_remote` had **no caller**: the public
     `commit_workspace_session_with_status` always took the materialized path,
     whose first step is `projection::pause(&worker)` → `FREEZE` — an opcode the
     rewritten sandbox owner no longer implements → immediate InvalidExecution.
     The route integration test in `live_backing.rs` drives
     `build_remote_candidate` / `commit_workspace_candidate` /
     `complete_generation` by hand, so the public entry point was never covered.
  2. Nothing could Commit, and End(Clean) would have hit the same FREEZE.
- Repair (`fa642c5ea`), following the owner's directive that freeze/pause/quiesce
  must be removed because the new architecture lets the Workspace keep running
  commands during Commit:
  - the public Commit dispatches a sandbox-owned Workspace to `commit_remote`
    before any projection machinery: capture → pull → build → publish →
    complete, with no pause fence, no wait-for-writers and no quiesce;
  - one shared process-wide construction gate admits one canonical build at a
    time across Workspaces (the experiment's single construction worker);
  - `projection::pause` deleted; `resume` no longer contacts the owner and keeps
    its recorded-failure and injected-fault role; the End(Clean),
    presentation-recovery, SDK-edit and reconciliation call sites drop the
    removed pause step. The materialized route keeps its writer/admission gates,
    and the FUSE-proxy protocol keeps its own pause/resume control.
  Fast cycle after the change: PASS 98s / 4 bounded jobs.
- Next: rebuild both artifacts and re-run the smoke.

### L10 — 2026-09-15: smoke #4 perf PASS, verification FAIL — host `READ_BASE` arm missing

- Perf: `benchmark-results/issue151/smoke-candidate-4` PASS
  (`pure_call_sum_ns=29655377`), full lifecycle create → exec → Commit(Created)
  → visibility → End, host and sandbox cleanup observations back to 0.
- Verification: `benchmark-results/issue151/verify-smoke-candidate/` FAIL with
  `fs-benchmark-workload: Invalid argument (os error 22)` inside the frozen
  `sampled-native-verification` step (a second Workspace reopened over the
  published root, reading a fixture witness file through FUSE).
- Attribution: added the env-gated `LAYERFS_BACKING_FAILURE_DIAGNOSTIC` hook on
  the host backing handler (mirroring the existing daemon-side
  `LAYERFS_EDIT_FAILURE_DIAGNOSTIC`) and reproduced the failure with the same
  identity-pinned verification selection. Host output:
  `{"kind":"backing-failure","opcode":6,"request_bytes":45,"error":"InvalidInput(\"backing request\")"}`.
  Opcode 6 is `READ_BASE`: the host answered every immutable-base content read
  with its unknown-opcode error, which the wire can only report as
  `PortError::Invalid` → EINVAL.
- Cause: the rewritten `BackingOwner::request` kept SEED,
  LOOKUP/LOOKUP_METADATA and DIRECTORY_PAGE but lost the `READ_BASE` arm, so no
  unchanged file could be read through FUSE. The create-only perf workload never
  read base bytes; the sampled verification reads a fixture witness and hit it
  immediately. B1/B2 perf would have failed the same way (their workloads read
  and rewrite fixture files).
- Repair (`e1a317a4d`): restored the released v0.1.5 `READ_BASE` arm verbatim —
  authenticated `read_range` over the Store reader, the same 1 MiB bound, the
  same `note_rope_read` accounting, no policy change. Fast cycle after the
  change: PASS 104s / 4 bounded jobs.
- Next: re-prepare the input for the new product seal, then re-run smoke and its
  separate verification.

### L11 — 2026-09-15: smoke PASS and separate verification PASS (candidate)

- Smoke command: `bash
  benchmark/fs-bench-pro/families/tiny_file_churn/perf.sh --case
  tiny-create-1-compact-v2 --seed 1 --setup clone --image
  layerfs-bench-infra:5447aeecbe1d7135 --perf-fast --collection-mode --output
  benchmark-results/issue151/smoke-candidate-5`
  → PASS, `pure_call_sum_ns=22505667`, cleanup PASS. Identities: source
  `5447aeecbe1d713545e1635e091e0846e7ccd4905da7d389fde5621133e25536`, input
  `afd55b3b52e2e44528cff407861b3a182e553922b02f991e6c6bf3c79977a1f9`, image
  `sha256:4c0ea352bcf9d57f123363a8806f1f04a10b3bc5f87cf6fa1e1ef1c09173283c`
  (pre-commit build of the same source seal and the same in-image binaries and
  product seal `e03dae88…`; the sealed gate image is
  `sha256:97be8c92b743d16444282907051a353e564757aa43d9c6a1ccb83ba65c172029`).
- Verification (separate run with exactly those identities):
  `benchmark-results/issue151/verify-smoke-candidate-2/verification.json` →
  **PASS**. Both `sampled-canonical-verification` (through the Store) and
  `sampled-native-verification` (fresh FUSE reopened over the published root,
  `benchmark_reopen_count=1`, `fresh_fuse_reopened=true`) pass; sampled paths
  `.`, `wide/s000-f000.dat`, `tiny`, `tiny/p0/f116.dat`, range
  `wide/s000-f000.dat:0..2097`.
- A smoke is not a gate sample. Every attempt is preserved:
  `smoke-candidate/`, `smoke-candidate-2/`, `smoke-candidate-3/`,
  `smoke-candidate-4/`, `smoke-candidate-5/`, `verify-smoke-candidate/`,
  `verify-smoke-candidate-2/`.
- Next: Phase D gate B1, control first, against the sealed identities above.

### L12 — 2026-09-15: gate B1 `tiny-create-500-mixed-v4` — PASS does not reproduce; commit gate FAILS

Both arms, seed 1, `--setup clone`, `--perf-fast --collection-mode
--product-timeout 600 --timeout 630 --setup-timeout 600`,
`LAYERFS_CONSTRUCTION_WORKERS=1` exported for both arms (inert in the control —
see L14). Every sample below is preserved.

| sample | receipt (under that arm's `benchmark-results/issue151/`) | workflow ns | exec ns | commit ns | container CPU ns | host CPU ns | CPU sum ns |
|---|---|---|---|---|---|---|---|
| control c1 | `perf-control-tiny-create-500-mixed-v4` | 212,317,917 | 156,949,500 | 40,037,458 | 108,536,000 | 88,442,082 | 196,978,082 |
| control c2 (fresh) | `perf-control2-tiny-create-500-mixed-v4` | 242,483,458 | 176,370,625 | 53,037,416 | 112,224,000 | 106,418,876 | 218,642,876 |
| candidate r1 (rev A) | `perf-candidate2-tiny-create-500-mixed-v4` | 191,581,542 | 136,831,042 | 42,342,583 | 121,660,000 | 83,658,292 | 205,318,292 |
| candidate r2 (rev B) | `perf-candidate3-tiny-create-500-mixed-v4` | 244,905,375 | 166,733,750 | 64,953,250 | 131,794,000 | 95,056,876 | 226,850,876 |
| candidate r3 (rev B, clean host) | `perf-candidate4-tiny-create-500-mixed-v4` | 269,016,874 | 191,666,916 | 65,461,500 | 123,361,000 | 121,792,708 | 245,153,708 |
| candidate probe (rev A, default workers) | `perf-candidate-defaultworkers-tiny-create-500-mixed-v4` | 310,707,125 | 158,965,583 | 140,759,625 | — | — | — |

Revision A = `3a1c35eb4` (segment read-ahead); revision B = `58a54b446`
(A + the frozen-records page-exhaustion fix). The B fix touches the
`SNAP_RECORDS` path B1 exercises, so B1's revision-A sample was invalidated and
re-collected (L14 records the environment problem found while doing that).

Gate arithmetic on the contemporaneous pair (control c2 09:26, candidate r3
09:27; `time_limit(T0) = T0 + max(0.15*T0, 3 ms)`,
`cpu_limit(C0) = C0 + max(0.15*C0, 1 ms)`):

| phase | control | candidate | limit | verdict |
|---|---|---|---|---|
| create | 8,835,083 | 8,819,375 | 11,835,083 | PASS |
| exec | 176,370,625 | 191,666,916 | 202,826,219 | PASS |
| complete Commit | 53,037,416 | 65,461,500 | 60,993,028 | **FAIL** (+4,468,472 ns, +7.3%) |
| visibility | 85,209 | 66,083 | 3,085,209 | PASS |
| End/cleanup | 4,155,125 | 3,003,000 | 7,155,125 | PASS |
| whole workflow | 242,483,458 | 269,016,874 | 278,855,977 | PASS |
| container CPU | 112,224,000 | 123,361,000 | 129,057,600 | PASS |
| host-process CPU | 106,418,876 | 121,792,708 | 122,381,707 | PASS |
| CPU sum (declared metric) | 218,642,876 | 245,153,708 | 251,439,307 | PASS |
| peak-sum memory | 40,706,048 | 40,300,544 | 49,094,656 | PASS |
| temporary backing | 827,392 | 824,450 | 1,851,392 | PASS |

Canonical Store growth: control +888,832 B (page_count 129,478→129,707),
candidate +897,024 B (129,500→129,719); both keep their temporary backing
peak (827,392 / 824,450 B) separate from that growth.

Commit sub-phases (ns; `publication_ns` in the candidate also contains
`object_admission_ns`, which it does not in the control — an attribution
difference, not extra time):

| phase | control c2 | cand r1 (A) | cand r2 (B) | cand r3 (B) |
|---|---|---|---|---|
| total | 53,026,958 | 42,336,250 | 64,943,500 | 65,451,666 |
| capture | 338,666 | 270,208 | 424,833 | 377,750 |
| candidate plan (frozen pull) | 5,084 | 1,822,458 | 3,055,791 | 3,367,750 |
| content | 22,744,834 | 18,551,875 | 26,367,209 | 34,709,125 |
| consumer idle | 3,732,500 | 7,133,333 | 8,993,709 | 9,029,251 |
| namespace | 8,519,625 | 6,276,084 | 10,648,916 | 7,957,792 |
| object admission | 11,704,958 | 10,942,666 | 17,815,417 | 12,254,125 |
| publication (incl. admission) | 108,167 | 11,228,501 | 18,170,166 | 12,549,333 |
| checkpoint | 5,206,334 | 2,852,375 | 4,354,541 | 2,679,125 |

Diagnosis of the FAIL: the candidate's complete Commit carries work the control
does not do — the frozen-records pull (`candidate_plan` 3.4 ms vs 0.005 ms) and
the payload transfer inside construction (`content` +4 to +12 ms). Both are
required by the design (the sandbox owns the mutable state and its bytes), and
both are inside the measured Complete-Commit phase, so they are measured, not
hidden. The same candidate revision measured 2 minutes apart reproduces
(64.95 ms and 65.46 ms), so the failure is stable on this host, but the earlier
revision-A pair measured +5.8% and passed; the 15% allowance is not resolvable
to better than the host's run-to-run spread (L14). No product fix was applied
for this gate: no counter identifies a defect, and the remaining difference is
the declared transfer plus machine noise. B1 is recorded as **FAIL**.

Separate verifications (exact identities from each arm's receipt), all PASS:
- control: `verify-control-tiny-create-500-mixed-v4` (source `e5efac1d…`,
  input `bbc74746…`, image `sha256:179ee8a2…`), `sampled-canonical-` and
  `sampled-native-verification`, wall 4.1-6.1 s.
- candidate r1 identities: `verify-candidate-tiny-create-500-mixed-v4` PASS.
- candidate r3 identities: `verify-candidate2-tiny-create-500-mixed-v4`
  (source `002eb534a9cb2585d59dd6437c08fb7664ab103eb068b882eb920ecc4ffb3d0e`,
  input `18641680b47bf881cec7ee060c96e814af5332ef7d91ed61002ba7be540c4a08`,
  image `sha256:87e09334e057957d9b18ba14c137e86d1185bdfd2f4a7c6a46f48203e121351a`)
  PASS, wall 4.22 s.
- Next: B1 is not a PASS; B2 was reached before this failure was understood and
  is recorded below; the strict order therefore stops here pending an owner
  ruling (L14).

### L13 — 2026-09-15: gate B2 `tiny-bulk-create-500-mixed-v3` — time/CPU/storage PASS, memory metric unresolved

Same flags, extended allowances, seed 1, `--setup clone`, one worker per arm.

| metric | control | candidate | limit | verdict |
|---|---|---|---|---|
| create | 7,211,625 | 9,096,709 | 10,211,625 | PASS |
| exec | 2,758,683,417 | 2,127,819,333 | 3,172,485,930 | PASS |
| complete Commit | 1,573,941,917 | 1,801,882,000 | 1,810,033,205 | PASS (+14.5%) |
| visibility | 87,208 | 75,375 | 3,087,208 | PASS |
| End/cleanup | 14,994,250 | 5,081,375 | 17,994,250 | PASS |
| whole workflow | 4,354,918,417 | 3,943,954,792 | 5,008,156,180 | PASS |
| container CPU | 1,403,415,000 | 1,879,520,000 | 1,613,927,250 | FAIL |
| host-process CPU | 2,720,046,499 | 2,323,990,625 | 3,128,053,474 | PASS |
| CPU sum (declared metric) | 4,123,461,499 | 4,203,510,625 | 4,741,980,724 | PASS |
| temporary backing | 524,529,664 | 524,288,000 | 603,209,114 | PASS |
| canonical Store growth | +530,878,464 | +530,894,848 | — | comparable |
| peak-sum memory (host peak + container lifetime peak) | 111,472,640 | 658,649,088 | 128,193,536 | FAIL as measured |

- The declared CPU metric is the sum of host and sandbox CPU over the
  corresponding window, and it PASSES; the container-only window FAILS because
  the candidate's sandbox now does the payload ownership work the control did on
  the host (this is the measured design difference, not a hidden worker).
- The memory line is the only non-passing metric and its *definition* decides
  the verdict: the declared metric is "host-process + sandbox-process peak
  resident bytes", but the frozen harness emits no sandbox *process* peak for
  this route — only the container lifetime cgroup peak, which includes page
  cache and kernel memory. The candidate's extra 563 MB is ~500 MiB of page
  cache for the payload it legitimately holds in `/snapshots/<id>/` (the same
  bytes the control holds in its host spool, where they appear in no process
  RSS and in no container cgroup). Under the cgroup-proxy reading B2 memory is
  FAIL; under the declared process reading the required input is missing, which
  is INCOMPLETE, never PASS. Escalated in L14.
- First attempt (revision B before the paging fix):
  `perf-candidate-tiny-bulk-create-500-mixed-v3` FAIL
  `Workspace(Storage(Integrity("frontier node")))`; repaired in `58a54b446`
  (a byte-bounded frozen-records page was reported as an exhausted frontier, so
  the host pulled one page and the namespace walk lost nodes). Preserved.
- Separate verifications, both PASS: control
  `verify-control-tiny-bulk-create-500-mixed-v3` (input `40c5fcbf…`, wall 6.1 s)
  and candidate `verify-candidate-tiny-bulk-create-500-mixed-v3`
  (source `002eb534…`, input `3a4e3b4c…`, image `sha256:87e09334…`, wall 6.7 s,
  policy `selected-verification-v1` 45 s work / 59 s hard).
- Next: B2 is not a clean PASS (memory metric), so B3 was not started under the
  pipeline's strict order.

### L14 — 2026-09-15: measurement environment, worker count and open rulings

- **Environment contamination found and removed.** For 34 minutes
  (≈08:53–09:26) an unrelated `grep -rn LAYERFS_LIVE_FUSE .` (PID 43307,
  cwd `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, 30m27s CPU time, ~54% CPU) was
  running in the *main checkout* and was active during control c1, candidate r1
  and candidate r2. It was killed before control c2 and candidate r3, which is
  why those two are treated as the contemporaneous pair. Receipts from both
  regimes are preserved; the desktop session also keeps the host loaded
  (load average ≈5 with browser and editor processes), so run-to-run spread on
  this workstation is ±15–30% at the few-tens-of-milliseconds scale B1 measures.
- **Worker count.** `LAYERFS_CONSTRUCTION_WORKERS` exists only in the candidate
  (`55b531bc5`); v0.1.5 has no such knob and uses
  `available_parallelism().min(8)`, further capped to 4 for the small-content
  format (`SMALL_CONTENT_WORKERS`). The L2 note that the knob is "shared" is
  wrong. Both arms were run with the same exported value (`1`); it binds the
  candidate to one construction worker and is inert in the control, which is
  the conservative direction (the candidate is measured under the stricter
  contract). The probe sample
  `perf-candidate-defaultworkers-tiny-create-500-mixed-v4` (knob unset, four
  workers) is retained for information only and is not used in any gate.
- **Rulings needed from the owner** (both are about the measurement contract,
  not about product behaviour):
  1. B1's Complete-Commit gate fails by 7.3% on a stable, contemporaneous pair,
     while the same candidate behaviour measured 5.8% *under* the control an
     hour earlier under a different host load. Should the experiment accept a
     recorded B1 FAIL for the commit phase (with the transfer-inside-Commit
     cause), or should the pair be re-collected on a quiet host / with a
     declared repeat count?
  2. B2's memory metric: should the gate use the harness's only symmetric
     sandbox number (container cgroup lifetime peak, which includes the page
     cache of the 500 MiB payload the B2 clause explicitly permits the sandbox
     to hold → FAIL), or a sandbox *process* peak that the frozen harness does
     not emit (→ INCOMPLETE)? A third option is a minimal symmetric harness
     addition, which would change the harness identity and invalidate the
     comparability of every sample collected so far.
- Next: with B1 FAIL recorded and B2's memory metric unresolved, B3
  (`local-snapshot-create-25000-onebyte-v1`) has not been started; the pipeline
  forbids advancing past an unpassed gate. All receipts and raw event streams
  above are retained with their exact identities.

### L15 — 2026-09-15: owner directive — promote this implementation to `main` (local and origin)

- Owner decision: the measured sandbox-local implementation is the v0.1.6
  direction. `main` (local `7b73c4b33c950c3ce3cd192ea7b571398bda3f8f`, equal to
  `origin/main`) is replaced by this branch's tip; the earlier host-overlay
  v0.1.6 work on `main` is discarded from `main` **but preserved**:
  - local + remote branch `archive/v016-overlay-7b73c4b33` at the old tip;
  - the uncommitted documents that lived only in the `main` worktree were
    stashed (`stash@{0}`, pushed as a ref) and copied verbatim to
    `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-main-uncommitted-20260915T0930Z/`;
    nine of the ten are byte-identical to files already committed on this
    branch; two documents are *not* on this branch and therefore leave `main`
    with this promotion: `docs/roadmap/0.1/0.1.6/README.md` (the previous
    broader version) and `docs/roadmap/0.1/0.1.6/overlay-snapshot-rule.md`
    (untracked-relative to this branch). Both remain recoverable from the
    archive branch and the stash.
- What the promotion does and does not claim:
  - it selects the implementation direction; it does **not** close #149/#150,
    cut a release tag, or assert release readiness;
  - the recorded gate state travels with it: B1 complete Commit FAIL
    (+7.3 % on the contemporaneous pair), B2 memory metric unresolved (the
    declared sandbox-process input is not emitted; the container-cgroup proxy is
    658.6 MB against a 128.2 MB limit because it includes the payload page
    cache), B3 not run;
  - the four carried limitations remain: retention amplification (measured 0 in
    B1/B2 but unexercised under hostile write-during-transfer), no
    cross-snapshot diffing, unsolved mmap dirty visibility, and no
    durability/recovery by design;
  - the control arm, the sealed v0.1.5 worktree
    (`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-v016-control`) and all receipts
    under `benchmark-results/issue151/` are unaffected by this promotion.
- Product identity of the promoted revision is unchanged by this entry: it is
  documentation only, so the product seal of the measured samples
  (`c2e6dc0a…`) still describes it; the source seal changes because the tree
  changed.

### L16 — 2026-09-15: `main` promoted, CI repaired, repository reduced to `main` only

- Promotion (L15) landed: `main` = `be5beef21`, then `feec2defd` after the CI
  repair below. The image built for the promotion is the direction of record.
- CI repair (`feec2defd`): the promoted branch descends from v0.1.5, which
  predates the repository's rustfmt-1.96 and clippy passes, so
  `cargo +1.96.0 fmt --all --check` failed on the first push and every later
  step was skipped. Ran `cargo +1.96.0 fmt --all` (8 files) and fixed
  `clippy --workspace -D warnings` — an entry-match in `frozen.rs`; the dead
  batch-transport items and the eight-argument `serve` marked where only the
  transport tests exercise them; the snapshot-lane task now torn down with the
  observer task in `cancel`/`finish_shutdown`/`Drop`; redundant casts and an
  `Ok(..?)` wrapper removed; the vestigial host-spool leftovers
  (`BackingOwner.directory` and its constructor argument at four call sites,
  `CaptureSummary.base_root`, `FrozenRemoteInput.canonical_nodes`) and the
  never-read observation fields removed; the B3 adapter's receipt writer uses
  `writeln!`. CI run
  [34919465788](https://github.com/Ephemeral-AI-Lab/layerfs/actions/runs/34919465788)
  is green (fmt, tools unit tests, `tools/test-fast.sh`, clippy).
- Branch cleanup (owner directive): every branch except `main` was deleted,
  locally and on `origin` (9 branches: the two `archive/*` rescue branches,
  `codex/v016-sandbox-local-experiment`, `codex/issue130-m2-control`, four
  `codex/issue144-*` branches, `codex/hybrid-transition-report`). Following the
  repository's existing convention (see
  `archive/main-branch-cleanup-20260905-063146/heads/*` tags), each removed
  branch is preserved as the tag
  `archive/main-branch-cleanup-20260915-1005/heads/<branch>`, pushed to
  `origin`: `v016-overlay-7b73c4b33` = `7b73c4b33` (the discarded host-overlay
  history), `v016-sandbox-local-experiment` = `be5beef21` (the promoted tip),
  `main-uncommitted-20260915T0930Z` = `c5f85d546b` (the `main` worktree's
  uncommitted documents), plus the four `issue144` heads, `issue130-m2-control`
  and `hybrid-transition-report`.
- Measurement consequence: the `writeln!` change to the local_snapshot adapter
  is semantics-free but alters the workload-source hash, so any further sample
  needs a rebuilt image **and** a matching control; the harness identity of the
  samples recorded in L7–L14 (`daa74be0…`) therefore no longer describes the
  tree on `main`.
- Working state: `main` and the `layerfs` checkout are at `feec2defd`; the
  measurement worktree is detached at the same commit; the sealed v0.1.5 control
  worktree is untouched at `6ee1ec94c`. The next measurement work starts from
  `main` with a rebuilt pair.

### L17 — 2026-09-15: local worktrees and `layerfs*` directories reduced to one checkout

- Owner directive: remove every `layerfs` worktree under
  `/Users/yifanxu/Ephemeral-AI-Lab` and keep a single `layerfs*` directory.
- Removed (≈13.6 GB): the two registered worktrees
  `layerfs-v016` (candidate build tree + receipts, 7.8 GB) and
  `layerfs-v016-control` (sealed v0.1.5 control build, 1.1 GB), both
  deregistered with `git worktree remove --force` and `git worktree prune`;
  plus the plain directories `layerfs-checkpoint` (3.5 GB),
  `layerfs-fix-nested-init` (2.3 GB), `layerfs-init-experiment-20260907-221713`
  (3.5 GB), `layerfs-workspace-admission` (3.5 GB),
  `layerfs-issue68-corrected-baseline` (530 MB),
  `layerfs-handoff-archive-20260914T053631Z` (361 MB),
  `layerfs-deepseek-history` (206 MB),
  `layerfs-main-uncommitted-20260915T0930Z` (240 KB) and the stale
  `layerfs-cleanup-active.txt` marker (it pointed at a directory that no longer
  exists).
- Preserved before removal, inside the surviving checkout at
  `benchmark-results/handoff-20260915/` (1.8 MB, git-ignored like all
  `benchmark-results` content):
  - `issue151-candidate/` — all 17 candidate receipt directories (B1/B2 samples,
    the four smoke attempts and their verifications);
  - `issue151-control/` — all 5 control receipt directories.
  - `identities/` — the archived binary and image identity JSONs plus the host
    binary sidecar for both arms.
  - `control-harness.patch` — the exact 314-line harness patch the sealed
    control worktree carried uncommitted, and its `git status` output.
  - `removed-dirs/` — top-level manifests, SHA256SUMS and reports of the deleted
    directories (80 KB).
- Recoverability after this cleanup: the removed branches are tag-preserved
  (`archive/main-branch-cleanup-20260915-1005/heads/*`, L16), including the
  control-era harness patch source; `v0.1.5` remains tagged, so the control arm
  can be recreated with the documented recipe (`git worktree add` at
  `6ee1ec94`, apply the harness diff, rebuild host and image). Build outputs,
  prepared fixtures and image layers are regenerable; the raw receipts are not,
  which is why they were copied first.
- Consequence for the remaining work: measurement can no longer resume from the
  old trees. A fresh pair must be created from `main` (`ef1c2aa0a`) and from the
  `v0.1.5` tag, built with the current harness (whose workload-source hash
  changed with the L16 `writeln!` fix), before B3 or the breadth pass can run.

### L18 — 2026-09-16: owner rulings (B1 accepted, memory amplification forbidden, no warm-cache credit) and the bounded-resident-cache repair

#### Owner rulings that reset the remaining work

1. **B1 is not an issue.** The promoted direction is one construction worker, and a
   single worker is expected to be slower than the sealed control's released
   default (four workers for small content). The B1 commit-phase FAIL recorded in
   L12 is therefore recorded as *accepted*: the candidate's extra time there is the
   frozen-records pull plus the payload transfer inside the measured Commit, work
   the host-authority control does not do. Amended on #151
   (issuecomment-5673868383).
2. **Memory amplification must not be allowed.** The sandbox may not hold the
   workspace payload resident just because it owns it.
3. **A warm cache must not be used to flatter a measured phase.** Prepared data in
   setup is legitimate benchmark infrastructure; letting a measured phase be
   credited by page-cache warmth left by an earlier phase of the same run is not.
   This directly invalidated the basis on which the pre-fix B2 Commit number was
   read: its ~500 MiB transfer was served from the sandbox's page cache at
   19 GB/s (measured: 500 MiB in 0.026 s) because the workload's own writes had
   left it resident.

#### Mechanism measurements (containers, gate shape 2 CPU / 2 GiB / no swap)

- Writing the B2 payload at B2's pacing with no hygiene reproduces B2 exactly:
  container lifetime peak 522.2 MiB, of which `file` 500.3 MiB and `anon` 4.6 MB.
  The sandbox's accounted memory *was* the payload, not process memory.
- `POSIX_FADV_DONTNEED` evicts **only pages that are already clean at call time**:
  on a dirty range the first call starts writeback and evicts nothing (dirty→0,
  `file` unchanged); a second call on the now-clean range evicts it (64 MiB → 0).
  Read-only and writable descriptors behave the same; the discriminator is
  cleanliness.
- A single offer per sealed segment is not enough (peak 499.9 MiB with a window of
  one), because the offer arrives while the range is still dirty. Re-offering the
  window on each seal keeps 500 MiB of payload to a **13.0 MiB** lifetime peak
  (5.1 MiB max resident spool cache, 1 MiB dirty); a window of eight gives
  14.1 MiB. The bound is a constant, not a fraction of the payload.
- Cold re-read of that payload from storage: 500 MiB in 0.23 s (2.1 GiB/s), i.e.
  the honest cost of a transfer that is not served from cache.

#### Repair — `c0ebedd2d` (`layerfs-fuse: bound the sandbox spool's resident cache`)

`LocalSpool` now keeps a bounded resident window: sealing a segment pushes the
previous target into a fixed-size window and offers the whole window back with
`POSIX_FADV_DONTNEED`; a segment is offered again on every later seal until it
slides out. Serving a frozen range to the host drops that range after it has been
copied out, so a bulk transfer cannot re-inflate the charge with the payload it
hands over. The hint is not a durability barrier (no flush is waited on, the
backing stays volatile, and the kernel never discards dirty, in-writeback or
mapped pages, so an eviction can only drop bytes already on storage). It comes
from the crate's existing Linux-only `nix` dependency; other targets keep the
default behaviour. CI-exact `cargo +1.96.0 fmt --all --check` and
`cargo +1.96.0 clippy --workspace --locked -- -D warnings` pass; 44 lib + 6
integration tests pass, including a new test that the window stays bounded and
that eviction never disturbs readable bytes.

New candidate arm identities (harness and workload source unchanged, so the
recorded control samples stay valid): source commit `c0ebedd2d`,
source seal `94d1cd234584ae92b961035df5446bc1cf4c48a7fcfecad68314113275cc0458`,
source tree `827667b539314ea91dc20155d51ddbdddd5c507d`, product seal
`31a42c95197a21c5acd54cb12e7398bd8cb5308fab916e62b9439ca0a17bf01d`,
compilation seal `0f397b536f74c4ba3eaf8e869bc42ae828cbba3aa9dd619aa72bfe65fa7385db`,
host binary `98e63fdde58b46d82597686eea8c38c28fb688d00a13ce3e8fe49e88c78c7226`,
image `layerfs-bench-infra:94d1cd234584ae92` =
`sha256:1435035ffedb783b34cf2f1e7784b3b58ee632f836be378b3c3cac10d844385f`,
harness identity `daa74be0c3a0b40a824617a6403c70ce4e7be115e7c47f4f1faf26029c55b044`,
`WORKLOAD_SOURCE_SHA256 821b240458fe968cec8ec61bf09f1db09e109bf1a3f64fc62e94c449e3c302b8`.
Command shape for both gates (one sample, seed 1, `--setup clone`, one worker):

    LAYERFS_CONSTRUCTION_WORKERS=1 bash benchmark/fs-bench-pro/families/tiny_file_churn/perf.sh \
      --case <case> --seed 1 --setup clone --image layerfs-bench-infra:94d1cd234584ae92 \
      --perf-fast --collection-mode --product-timeout 600 --timeout 630 --setup-timeout 600 \
      --output benchmark-results/issue151/<receipt>

#### B1 re-measured (`tiny-create-500-mixed-v4`)

Receipt `perf-candidate5-tiny-create-500-mixed-v4`, against the contemporaneous
control `perf-control2-tiny-create-500-mixed-v4` of L12 (unchanged identities).

| phase | control | candidate | limit | verdict |
|---|---|---|---|---|
| create | 8,835,083 | 9,083,791 | 10,160,345 | PASS |
| exec | 176,370,625 | 174,008,250 | 202,826,219 | PASS |
| complete Commit | 53,037,416 | 57,568,333 | 60,993,028 | **PASS** (was FAIL at 65,461,500) |
| visibility | 85,209 | 73,583 | 3,085,209 | PASS |
| End/cleanup | 4,155,125 | 3,618,167 | 7,155,125 | PASS |
| whole workflow | 242,483,458 | 244,352,124 | 278,855,977 | PASS |
| container CPU | 112,224,000 | 135,386,000 | 129,057,600 | **FAIL** (+4.9%) |
| host-process CPU | 106,418,876 | 106,165,542 | 122,381,707 | PASS |
| CPU sum (declared) | 218,642,876 | 241,551,542 | 251,439,307 | PASS |
| peak-sum memory | 40,706,048 | 42,352,640 | 49,094,656 | PASS |
| temporary backing | 827,392 | 824,450 | 1,851,392 | PASS |

The commit phase now passes its gate (5.6% under the limit) even before the
owner's ruling that B1 is not a blocker; the only non-passing line is the
container-only CPU sub-gate at +4.9%, a 23 ms difference on a 112 ms control.

#### B2 re-measured (`tiny-bulk-create-500-mixed-v3`)

Receipt `perf-candidate3-tiny-bulk-create-500-mixed-v3`, against the L13 control
`perf-control-tiny-bulk-create-500-mixed-v3`.

| phase | control | candidate | limit | verdict |
|---|---|---|---|---|
| create | 7,211,625 | 8,804,666 | 10,211,625 | PASS |
| exec | 2,758,683,417 | 2,541,289,250 | 3,172,485,930 | PASS |
| complete Commit | 1,573,941,917 | 2,198,331,166 | 1,810,033,205 | **FAIL** (+21.4%) |
| visibility | 87,208 | 81,750 | 3,087,208 | PASS |
| End/cleanup | 14,994,250 | 6,183,500 | 17,994,250 | PASS |
| whole workflow | 4,354,918,417 | 4,754,690,332 | 5,008,156,180 | PASS (5.1% under) |
| container CPU | 1,403,415,000 | 2,254,290,000 | 1,613,927,250 | FAIL (+39.7%) |
| host-process CPU | 2,720,046,499 | 2,802,701,583 | 3,128,053,474 | PASS |
| CPU sum (declared) | 4,123,461,499 | 5,056,991,583 | 4,741,980,724 | **FAIL** (+6.6%; was PASS pre-fix) |
| temporary backing | 524,529,664 | 524,288,000 | 602,931,200 | PASS |
| peak-sum memory | 111,472,640 | 156,405,760 | 128,193,536 | FAIL as measured (was FAIL at 658,649,088) |

Consequences, stated plainly:

- The bounded-cache policy costs **~0.41 s of exec** (2.128 → 2.541 s; the
  writeback it forces is work the pre-fix path deferred and then skipped when the
  spool files were deleted) and **~0.40 s of Commit** (1.802 → 2.198 s; the
  transfer now reads the payload from storage instead of from cache). Both are
  inside the *workflow* allowance (5.1% under), which is the gate the owner ruled
  matters, but the Commit sub-gate and the CPU sum now fail.
- The Commit limit is derived from a control whose equivalent read is served from
  the host page cache, so the candidate is measured on the strictly harder
  footing: the pre-fix Commit number that "passed" was itself cache-credited and
  is exactly what ruling 3 forbids counting.
- Removed amplification, measured on the product: the sandbox's resident spool
  cache during the whole B2 window is **≤ 2.6 MiB** (`file` 1.3–2.6 MiB, `shmem`
  0, `file_dirty` ≤ 1 MiB, `file_writeback` 0) while 500 MiB of payload is written
  and 516 spool files exist — versus ~500 MiB of resident payload pre-fix.

#### The harness memory number is not a product-memory measurement

Six identical post-fix B2 runs (same image, case, seed, flags, one worker; the
first is the declared gate sample, the rest are instrumentation runs) gave
container *lifetime* peaks of 24,633,344 / 25,530,368 / 25,829,376 / 26,185,728 /
**57,462,784 (declared sample)** / 189,788,160 bytes while the product's own
residency was ~2 MB in every run that was instrumented. Live timelines
(`issue151-postfix/mem_timeline*.log`) show the payload's pages being evicted as
the segments seal, the spool reaching 500 MiB and 516 files, and the cgroup's
`file` cache returning to 0 when the spool is destroyed.

So the frozen harness's only symmetric sandbox number — the container cgroup
lifetime peak, which includes image layers, harness `docker exec` helpers and any
other page cache charged to that cgroup — varies by ~8× on identical inputs and
cannot decide a memory gate. The declared metric ("sandbox-process peak resident
bytes") is still not emitted by the frozen harness. The ruling requested in L14
#2 is therefore now backed by direct evidence rather than by an argument, and it
blocks B2's memory verdict: with the product-attributable residency (~2 MB) the
gate is comfortably PASS; with the declared sample's container peak it is FAIL
(+22%); with two of the six runs it would be PASS.

#### Separate verification (exact identities from each receipt)

- `verify-candidate5-tiny-create-500-mixed-v4` → **PASS**, source
  `94d1cd23…`, input `ca3ad683…`, image source `94d1cd23…`, wall 0.86 s.
- `verify-candidate3-tiny-bulk-create-500-mixed-v3` → **PASS**, source
  `94d1cd23…`, input `1295ab34…`, image source `94d1cd23…`, wall 5.39 s.
Both runs also produced smoke `smoke-candidate-6` (PASS) on the same identities.

#### State after L18

- The memory-amplification defect is repaired in the product and verified by
  direct measurement; the cost of the repair is time and CPU, and B2's Commit and
  CPU-sum gates now fail against a limit derived from a cache-warm control.
- Open owner decisions: (a) the sandbox memory metric for B2 (L14 #2, now with
  variance evidence), and (b) whether the control arm must be re-measured under a
  cold-cache stance — which requires recreating the sealed v0.1.5 control worktree
  and changing the harness, i.e. re-collecting both arms.
- B3 (`local-snapshot-create-25000-onebyte-v1`) remains unstarted; the pipeline's
  strict order still blocks it behind B2's unpassed memory line.
- Evidence for this entry: `benchmark-results/issue151/` and its copy
  `benchmark-results/handoff-20260915/issue151-postfix/` (perf, verify and smoke
  receipts, the five post-fix B2 instrumentation runs, and the raw memory
  timelines), plus `candidate-host-binary.identity.json`.
