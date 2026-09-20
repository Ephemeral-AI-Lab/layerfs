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

### L19 — 2026-09-16: gate B3 `local-snapshot-create-25000-onebyte-v1` — collected on a reconstructed control arm, and three harness defects repaired on the way

Owner directive: proceed with B3 (`#151` checklist item: *one-sample 25k three-Commit
control/candidate + separate verification*).

#### Control arm reconstructed (its worktree had been removed in L17)

- `git worktree add --detach /Users/yifanxu/layerfs-v016-control v0.1.5`
  (`6ee1ec94`), then the benchmark tree mirrored from `main`. It lives **outside**
  `Ephemeral-AI-Lab`, so the owner's "one `layerfs*` folder under the lab directory"
  rule still holds. The control tree is by construction `SOURCE_DIRTY=true`, which is
  what the recorded control receipts carry.
- Evidence that this reproduces the original composition rather than inventing one:
  the diff against `v0.1.5` is exactly the five files of `control-harness.patch`
  (`benchmark/AGENTS.md`, `shared/runner.py`, `shared/test_layout.py`,
  `shared/test_runner.py`, `src/workspace_bench.rs`), and `v0.1.5`'s own
  `workload/main.rs` hashes `c6f1e4b1…` while both recorded arms hash `821b2404…`,
  i.e. the sealed control already carried the current benchmark tree with `crates/`
  still at v0.1.5.
- Rebuild reproduced the recorded identities: source commit `6ee1ec94`,
  source tree `739550b79159f4120cb60ddc8310b7cb7168c352`, **product seal
  `276c5970aabf485594d90ae30920b3cdb310134a7572589c558063a9d52ce093`** (exact match),
  workload source `821b2404…`; harness identity stayed
  `daa74be0c3a0b40a824617a6403c70ce4e7be115e7c47f4f1faf26029c55b044`, which is why
  the B1/B2 pairs recorded earlier remain valid. The source and compilation seals
  are new values (`7bfd6855…`/`93f49a88…` at the control's first B3 build) because
  the benchmark tree has since taken the `feec2defd` formatting commit and the B3
  verification repairs below.

#### Three harness defects made B3's verification impossible (none in the product)

B3's *perf* runs passed from the first attempt, but its separate verification
failed identically in **both** arms — the signature of a harness defect, not an arm
defect. All three were on the verification path:

1. **Root metadata expectation.** The comparison expects the fixture's root
   (`0755`, mtime 1700000000), but `ordinary_workloads::fixture` builds `"."` through
   the shared `dir()` helper, which uses `Entry::directory` and therefore `0750`.
   With the error text extended to print both sides, the failure is unambiguous:
   `expected mode [00,00,01,e8] mtime [00,00,00,00,65,53,f1,00,00,00,00,00];
   observed mode [00,00,01,ed] mtime [00,00,00,00,65,53,f1,00,00,00,00,00]` — the
   published root is the correct `0755` and the expectation was wrong. Repaired by
   re-pinning the root entry in the `local_snapshot` expectation.
2. **Evidence directory collision.** `persist_snapshot` refuses to create
   `canonical-verification` twice, so a three-Commit lifecycle could never verify
   ("canonical verifier evidence already exists" on C2). Repaired with
   `verify_step`, which gives each cycle its own
   `canonical-verification-step-{N}` directory instead of overwriting or colliding.
3. **No bounded native recipe.** `workspace_sample` and the `sampled` allow-list in
   `workspace_bench.rs` did not know `local_snapshot`, so verification fell through
   to the *unbounded* native walk of the reopened mount. Over 25,001 entries that
   walk died after 0.21 s in a fresh process with `fs-benchmark-workload: No space
   left on device (os error 28)` — a failure the perf path never exercises (the
   container overlay and `/dev/shm` both had 124 GB / 64 MB free, and no inotify or
   shm is involved). Repaired by adding the bounded witness recipe (root plus seven
   ordinals spanning first/middle/last and the C2-edited range) and adding
   `local_snapshot` to the allow-list, so the case now gets the sampled native proof
   the other families use.

All three live behind verification-mode gates (`expected()` is only called under
`if verification && case.family == "local_snapshot"`), so the perf samples below
measure the same product behaviour as the earlier builds; the product seals were
unchanged throughout (`31a42c95…` candidate, `276c5970…` control). The richer
metadata-mismatch message was kept as a diagnostic improvement.

#### B3 samples collected (ns; the case is one 25k lifecycle with three Commits)

| sample | control workflow | control C1 exec | candidate workflow | candidate C1 exec |
|---|---|---|---|---|
| r1 (first cold run per arm) | 9,061,830,292 | 8,022,200,125 | 9,869,486,458 | 8,691,086,875 |
| r2 (immediately after r1) | **1,912,412,501** | 897,722,917 | 9,663,445,501 | 8,617,677,250 |
| r3 | 9,698,489,543 | 8,429,252,917 | 9,826,580,669 | 8,800,800,375 |
| **r4 (final source, both arms)** | 8,913,636,875 | 7,842,096,125 | 10,570,971,626 | 9,420,617,625 |

**Cache stance, stated because it decides this case.** The only cache-sensitive
phase is C1's create/write: the control measured 8.02 s cold and 0.90 s when the
run followed another run of the same case back-to-back (r2, a 4.7× artifact), while
the candidate stayed at 8.6–9.4 s in every run because its sandbox is a fresh
container each time. The r2 control row is therefore **not poolable** with any cold
candidate row — it is exactly the "warm cache flatters a measured phase" pattern the
owner forbade — and the one-sample rule also forbids back-to-back repeats as gate
input. Gate samples are the first run of the case in each arm's current state,
with the two arms separated by the intervening rebuild.

#### B3 gate arithmetic (final source, r4 pair)

`time_limit(T0) = T0 + max(0.15*T0, 3 ms)`, `cpu_limit(C0) = C0 + max(0.15*C0, 1 ms)`.

| metric | control | candidate | limit | verdict |
|---|---|---|---|---|
| create | 8,175,208 | 10,483,333 | 11,175,208 | PASS |
| C1 create/write exec | 7,842,096,125 | 9,420,617,625 | 9,018,410,543 | FAIL (+4.5% over limit, +20.1% vs control) |
| C1 complete Commit | 394,334,750 | 540,147,709 | 453,484,962 | FAIL (+19.1%, +37.0%) |
| C2 edit exec | 366,879,583 | 364,458,917 | 421,911,520 | PASS |
| C2 complete Commit | 100,429,459 | 71,814,167 | 115,493,877 | PASS |
| C3 edit exec | 82,294,875 | 81,366,250 | 94,639,106 | PASS |
| C3 complete Commit | 82,243,167 | 64,039,542 | 94,579,642 | PASS |
| visibility | 79,250 | 82,083 | 3,079,250 | PASS |
| End/cleanup | 37,104,458 | 17,962,000 | 42,670,126 | PASS |
| whole workflow (3 cycles) | 8,913,636,875 | 10,570,971,626 | 10,250,682,406 | FAIL (+3.1%, +18.6%) |
| container CPU | 4,780,898,000 | 6,351,426,000 | 5,498,032,700 | FAIL (+15.5%, +32.9%) |
| host-process CPU | 845,495,750 | 736,620,042 | 972,320,112 | PASS |
| CPU sum (declared) | 5,626,393,750 | 7,088,046,042 | 6,470,352,812 | FAIL (+9.5%, +26.0%) |
| peak-sum memory | 133,734,400 | 132,997,120 | 153,794,560 | **PASS** |
| temporary backing | 28,672 | 25,000 | 1,077,248 | **PASS** |

The two 25k-specific absolute gates hold: transient physical backing is **25,000 B**
against the **32 MiB** ceiling, and the candidate's peak-sum memory is *below* the
control's. The four non-passing lines are all inside the owner's one-worker
tolerance (every delta ≤ 50%), and they are not stable: the r3 pair, collected
minutes earlier on the same source and the same product seals, passes **every**
line (workflow 9.827 s vs limit 11.153 s, C1 exec 8.801 s vs 9.694 s, C1 commit
0.480 s vs 0.511 s, container CPU 5.783 s vs 6.000 s, CPU sum 6.448 s vs 7.172 s).
The ~16% swing in the candidate/control ratio between r3 and r4 with no product
change is the host-state spread L14 already documented (±15–30%), and it is wider
than the 15% allowance this case is measured against.

#### Separate verification — both arms PASS

| verification | status | wall | source | input | image |
|---|---|---|---|---|---|
| `verify-CTRL-25k-r4` | **PASS** | 10.82 s | `7bfd6855…` | `e7004eff…` | `sha256:dcc26186…` |
| `verify-CAND-25k-r4` | **PASS** | 11.89 s | `a3b8441b…` | `b8d655de…` | `sha256:a40d8c0e…` |

Each receipt carries one sampled-canonical verification, the three per-Commit
canonical verifications (C1/C2/C3 exact bytes, namespace and metadata through
published content — 25,000 files, 25,001 inodes), one sampled native verification
over a fresh FUSE reopen, cleanup PASS, and harness identity `daa74be0…`.

#### Applicability after L19

| result | revision | product seal | receipts | verdict |
|---|---|---|---|---|
| B1 create-500 | `c0ebedd2d` | `31a42c95…` | `perf-candidate5-…`, `verify-candidate5-…` | all gates PASS except the container-only CPU line (+4.9%); owner waived the commit-phase reading |
| B2 bulk-create-500 | `c0ebedd2d` | `31a42c95…` | `perf-candidate3-…`, `verify-candidate3-…` | workflow/exec/End/backing PASS; Commit and CPU-sum FAIL against a cache-warm control's limit; sandbox memory bounded (≤2.6 MiB) with the metric ruling still open |
| B3 25k three-Commit | `2e1cbb4b…` (harness repairs) | `31a42c95…` | `perf-CAND-25k-r4`, `verify-CAND-25k-r4` | **both 25k absolute gates PASS**, verification PASS, four time/CPU lines inside the owner's one-worker tolerance (r3 pair passes them strictly) |

Next: the owner's remaining decisions (B2's sandbox memory metric; whether the
control must be re-measured under an equal cold stance), then the optional breadth
pass and the final adoption recommendation. The control worktree stays at
`/Users/yifanxu/layerfs-v016-control` for paired runs and can be removed on request.

#### L19 addendum — final source configuration (`9e3a4c91…` candidate / `40bb391e…` control)

`cargo +1.96.0 fmt --all` reformatted the three repaired files (CI requires a clean
`fmt --all --check`), which re-seals both arms, so the pair was re-collected on the
final source rather than reported from the pre-format build. Samples `perf-CTRL-25k-r5`
and `perf-CAND-25k-r5`; harness identity still `daa74be0…`, workload `821b2404…`,
product seals unchanged (`31a42c95…` / `276c5970…`), images
`sha256:10b6200e…` (candidate) and `sha256:3b8d8db2…` (control).

| metric | control | candidate | limit | verdict |
|---|---|---|---|---|
| create | 8,350,708 | 9,192,792 | 11,350,708 | PASS |
| C1 create/write exec | 7,819,424,208 | 9,204,423,625 | 8,992,337,839 | FAIL (+2.4%, +17.7%) |
| C1 complete Commit | 383,394,375 | 521,839,917 | 440,903,531 | FAIL (+18.4%, +36.1%) |
| C2 edit exec | 365,328,291 | 402,329,250 | 420,127,534 | PASS |
| C2 complete Commit | 98,736,291 | 76,904,333 | 113,546,734 | PASS |
| C3 edit exec | 86,469,333 | 82,235,208 | 99,439,732 | PASS |
| C3 complete Commit | 78,614,500 | 67,380,500 | 90,406,675 | PASS |
| visibility | 88,042 | 113,709 | 3,088,042 | PASS |
| End/cleanup | 33,510,792 | 16,964,084 | 38,537,410 | PASS |
| whole workflow (3 cycles) | 8,873,916,540 | 10,381,383,418 | 10,205,004,021 | FAIL (+1.7%, +17.0%) |
| container CPU | 4,732,720,000 | 6,242,791,000 | 5,442,628,000 | FAIL (+14.7%, +31.9%) |
| host-process CPU | 840,852,375 | 748,738,417 | 966,980,231 | PASS |
| CPU sum (declared) | 5,573,572,375 | 6,991,529,417 | 6,409,608,231 | FAIL (+9.1%, +25.4%) |
| peak-sum memory | 134,623,232 | 130,150,400 | 154,816,716 | **PASS** |
| temporary backing | 28,672 | 25,000 | 1,077,248 | **PASS** (32 MiB absolute gate also PASS) |

- **Separate verification on this source: both arms PASS** —
  `verify-CTRL-25k-r5` (wall 11.15 s) and `verify-CAND-25k-r5` (wall 11.96 s), each
  carrying one sampled-canonical verification, the three per-Commit canonical
  verifications, one sampled native verification over a fresh FUSE reopen, and
  cleanup PASS.
- The five non-passing lines are the same five as the r4 pair and all sit inside the
  owner's one-worker tolerance (every delta ≤ 50%). They are not stable across the
  session: the r3 pair (same products, pre-sample-recipe harness) passed **all** of
  them (workflow 9.827 s vs limit 11.153 s; C1 exec 8.801 s vs 9.694 s; C1 commit
  0.480 s vs 0.511 s; container CPU 5.783 s vs 6.000 s; CPU sum 6.448 s vs 7.172 s).
  Between r3 and r5 the candidate slowed ~6% and the control sped up ~8% with no
  product change; the plausible cause is environmental and asymmetric: the
  candidate's 25k create/write happens inside the Docker VM (whose disk gained ~10 GB
  of images today) while the control's happens on the host SSD with its page cache
  holding the fixture. The B3 time lines therefore carry a larger uncertainty than
  the 15% allowance this case is measured against, and only the two absolute 25k
  gates (backing, memory) plus the verification are decisive here.

### L20 — 2026-09-17: owner acceptance of the one-worker cost; final applicability, limitations and adoption recommendation

Owner ruling on the B3 numbers ("that is good, not really bad numbers"): the
measured deltas are accepted as the cost of the promoted one-worker direction.
This entry converts that ruling and the L18/L19 evidence into the closing record
required by the pipeline's checklist (*final evidence/applicability table, cleanup
and adoption recommendation recorded*). It does **not** merge, close or release
anything: the pipeline states that passing this experiment does not automatically
merge `main`, close #149/#150 or release v0.1.6.

#### Disposition of the three gates

| gate | case | disposition | decisive evidence |
|---|---|---|---|
| B1 | `tiny-create-500-mixed-v4` | **accepted** | every gate passes on revision `c0ebedd2d` except the container-only CPU sub-gate (+4.9%, a 23 ms difference); the complete-Commit reading that failed in L12 now passes (57,568,333 vs limit 60,993,028). The owner had already ruled the earlier commit-phase reading not a blocker (L18). |
| B2 | `tiny-bulk-create-500-mixed-v3` | **accepted, with a declared metric caveat** | workflow 4,754,690,332 vs limit 5,008,156,180 PASS, exec/End/backing PASS, canonical Store growth comparable; Commit +21.4% and CPU sum +6.6% over limits derived from a control whose equivalent read is cache-served, both inside the owner's one-worker tolerance. The memory-amplification defect is repaired (sandbox residency ~500 MiB → ≤ 2.6 MiB); the harness's container-lifetime proxy cannot decide the memory line (L18), so that line is recorded as *not measurable on this harness*, not as PASS. |
| B3 | `local-snapshot-create-25000-onebyte-v1` | **accepted** | separate verification PASS on both arms (per-Commit C1/C2/C3 canonical checks, sampled native reopen, cleanup PASS); both 25k absolute gates PASS (transient backing 25,000 B vs 32 MiB; peak-sum memory 130.2 MB vs control 134.6 MB, limit 154.8 MB); five time/CPU lines miss the strict +15% allowance by +1.7% to +36.1% while an earlier pair on the same product seals passed all five, i.e. inside the host-state spread and inside the owner's ≤50% tolerance. |

#### Source applicability — which implementation each result validates

| item | revision | evidence | status |
|---|---|---|---|
| I0 frozen inputs | L0–L2 | ledger L0–L2 | recorded |
| I1–I3 ownership, snapshot structure, bounded transfer | `ab2a6f9cb` (daemon route), `fuse`/`workspace` crates as promoted | ledger L4, L5, L11 smoke + verification PASS on the public path | recorded |
| I3–I5 host route, canonical construction, publication | `55b531bc5`, promoted to `main` | ledger L5, L15 | recorded |
| I6 focused checks and sealed candidate | `e03dae88…`/`31a42c95…` product seals | fast cycles and smoke PASS (ledger L7, L10, L11); the section 9 focused proofs (held-builder, deliberate stall, partial-transfer cancel) were part of the implementation phase and were **not re-run in this measurement span** | recorded, not re-run here |
| B1 create-500 | `c0ebedd2d`, product `31a42c95…` | `perf-candidate5-…` + `verify-candidate5-…` | accepted (one informational line) |
| B2 bulk-create-500 | `c0ebedd2d`, product `31a42c95…` | `perf-candidate3-…` + `verify-candidate3-…`, plus the five post-fix B2 instrumentation runs | accepted with metric caveat |
| B3 25k three-Commit | `9104bcb4f`, product `31a42c95…` | `perf-CAND-25k-r5` + `verify-CAND-25k-r5` (control `perf-CTRL-25k-r5` + `verify-CTRL-25k-r5`) | accepted |
| non-pausing execution during Commit | `fa642c5ea` (freeze/pause/quiesce removed) | code path removed and exercised by every B1/B2/B3 Commit; no dedicated proof re-run in this span | implemented, exercised |
| multi-Workspace isolation | — | **not measured in this span** | outstanding limitation |

#### Reported resources (candidate, accepted samples)

| case | workflow | Commit(s) | container CPU | host CPU | peak-sum memory | temporary backing | canonical Store growth |
|---|---|---|---|---|---|---|---|
| B1 | 244,352,124 ns | 57,568,333 ns | 135,386,000 ns | 106,165,542 ns | 42,352,640 B | 824,450 B | +897,024 B |
| B2 | 4,754,690,332 ns | 2,198,331,166 ns | 2,254,290,000 ns | 2,802,701,583 ns | (see caveat) | 524,288,000 B | +530,894,848 B |
| B3 | 10,381,383,418 ns | 521,839,917 / 76,904,333 / 67,380,500 ns | 6,242,791,000 ns | 748,738,417 ns | 130,150,400 B | 25,000 B (32 MiB gate) | n/a (three Commits, reported per step) |

#### Remaining limitations, stated rather than hidden

- **Sandbox memory is not measurable on the frozen harness.** The only symmetric
  sandbox number is a container *lifetime* cgroup peak, which includes image
  layers, harness exec helpers and fixture-preparation high-water; it varied
  24.6–189.8 MB across six identical B2 runs while the product's own residency
  stayed ≤ 2.6 MiB. The spec's declared metric is a *process* peak sum with
  cgroup/kernel reported separately, so the B2 memory line is `INCOMPLETE` here.
- **The time comparison is cache-stance sensitive and host-load sensitive.** The
  B3 control measured 9.06 s cold and 1.91 s when run back-to-back with another
  run of the same case (4.7×); between the r3 and r5 pairs the candidate/control
  ratio moved ~16% with no product change. The 15% allowance this experiment uses
  is narrower than that spread, so individual time lines are not individually
  conclusive; the accepted ruling absorbs this.
- **Dirty shared-mmap visibility remains unsolved** (research, recorded in
  `overlay-snapshot-contract-resolution.md`), and there is no cross-snapshot
  diffing; durability is out of scope by design (volatile contract).
- **The breadth families were not run.** 233 cases exist across 24 families; the
  pipeline scopes this experiment to B1–B3 and forbids the broad matrix. A breadth
  pass remains optional and owner-scoped, and `historical_access` would need the
  sealed full157 Store path supplied by the owner.

#### Adoption recommendation

Adopt the sandbox-local snapshot ownership with host-authoritative canonical
publication as the v0.1.6 direction, at one construction worker, on the strength of:
a functionally complete public path (three gates reaching real Commits, exact
end-to-end verification on both arms), the repaired memory-amplification defect
(payload residency bounded by a constant instead of by payload size), both 25k
absolute gates passing, and a CPU sum that stays within the accepted one-worker
tolerance. Keep the three limitations above attached to the adoption. Do not treat
this as a release decision: the remaining work is the optional breadth pass, a
harness field for sandbox *process* memory if the B2 memory line is to be gated
rather than reported, a declared repeat policy for the +15% lines, multi-Workspace
isolation measurement, and the unresolved dirty-mmap visibility research.

### L21 — 2026-09-17: owner directive — disable CI permanently; local preflight replaces it

- Directive: disable CI, permanently.
- Actions: the in-flight run for `7b9b9df5e` was cancelled, GitHub Actions was
  disabled for the repository (`gh api -X PUT
  repos/Ephemeral-AI-Lab/layerfs/actions/permissions -F enabled=false` →
  `{"enabled":false}`), and `.github/workflows/ci.yml` was removed from the tree so
  the repository no longer defines a workflow.
- Consequence: nothing runs on push. The checks that workflow performed are preserved
  as a local pre-push gate, `tools/preflight.sh` — rustfmt 1.96
  `fmt --all --check`, the `tools` unit tests, the workspace fast suite under
  `RUSTUP_TOOLCHAIN=1.85.1`, clippy `--workspace --locked -- -D warnings`, and (added
  here, because #152 will keep repairing benchmark-harness code) the
  `benchmark/fs-bench-pro/shared` tests. `AGENTS.md` §4 now states that the repository
  runs no CI and that no push may claim "CI green".
- Historical status: the "CI green" statements in L16, L18, L19 and L20 describe the
  runs that existed when they were written; they are not claims about the repository's
  current automation, which is now none.
- Reversal, if ever wanted: re-enable Actions in repository settings and restore the
  workflow from git history (`git show 7b9b9df5e:.github/workflows/ci.yml`).

### L22 — 2026-09-18: #152 full-suite campaign, G1 `init_namespace` — collected, plus a host prepared-input truncation found and repaired

Campaign: issue #152, group 1 of 8. Same one-sample rule, bounded acceptance
(< 50 % worse **or** < 10 ms absolute) and single construction worker as L18–L20.

#### Collection identity

Source seal `0debfccbfe56516fe32efd906d90a98f42cc023c28be7b8861a13267f893eab7`
@ `8b5e0955e` (tree `ec127964b4`, clean); product
`31a42c95197a21c5acd54cb12e7398bd8cb5308fab916e62b9439ca0a17bf01d`; compilation
`79dab102022084ac3926eaa4b1c81585bee3d90eeb25a4940f510dac10083688`; harness
`daa74be0c3a0b40a824617a6403c70ce4e7be115e7c47f4f1faf26029c55b044`; workload
`821b240458fe968cec8ec61bf09f1db09e109bf1a3f64fc62e94c449e3c302b8`; image
`layerfs-bench-infra:0debfccbfe56516f` =
`sha256:f0c86469d31f0841e103626d20aea144866935df90b25075c87b7011009cf852`.

The source seal is three commits past the issue's frozen `9e3a4c91…` because
`4787e7474`/`35980634e` changed `tools/preflight.sh`; `tools/` is inside the
source seal but host-only, outside the Docker context and outside the harness
identity. Product, compilation, harness, workload and image identities all match
the frozen table; only the source seal differs.

Command shape (all four cells):

    LAYERFS_CONSTRUCTION_WORKERS=1 bash benchmark/fs-bench-pro/families/init_namespace/perf.sh \
      --case <case> --seed 1 --setup fresh --image layerfs-bench-infra:0debfccbfe56516f \
      --perf-fast --collection-mode --product-timeout 600 --timeout 630 --setup-timeout 900 \
      --output benchmark-results/issue152/g1/<case>

#### Results (one sample per cell; comparators are the recorded #120 rows)

| case | timer | candidate | v0.1.5 (#120) | ratio | Δ abs | disposition |
|---|---|---|---|---|---|---|
| `namespace-100-compact-v3` | `layerstack_init_ns` | 31,667,167 | 28,703,959 | 1.10× | +2.96 ms | PASS |
| `namespace-1000-compact-v3` | `layerstack_init_ns` | 130,366,708 | 112,732,750 | 1.16× | +17.63 ms | PASS |
| `namespace-10000` | `layerstack_init_ns` | 1,100,711,333 | 1,020,422,292 | 1.08× | +80.29 ms | PASS |
| `namespace-100000` | `layerstack_init_ns` (cold) | 4,986,155,625 | 4,397,542,208 | 1.13× | +588.61 ms | comparative PASS; **absolute 2.7 s → FAIL — OWNER-WAIVED** |

Complete commands (`command_wall_ns`): 0.46 / 0.52 / 1.53 / **5.69 s**, all inside
the ≤ 15 s rule. `namespace-100000` also paid 18.57 s cold acquisition and 26.6 s
one-time fixture regeneration inside `preparation_wall_ns` (45.20 s); both are
excluded from the complete command under the rule's "excluding one-time
prepared-input validation" clause, and `wall_ns` (51.76 s) is recorded so the
exclusion is auditable. Cold contract: `VERIFIED_COLD`, 100,000 files, 125,169
pages, 0 resident, metadata VERIFIED, detector self-check 32 warm pages,
`fixture_digest 6fc793a9…` identical to the comparator's. 4/4 cleanups PASS,
4/4 independent proofs PASS (walls 2/2/3/7 s).

CANONICAL Store after the command (allocated/apparent): 5,152,768 / 20,570,112 /
304,939,008 (304,705,536) / 520,560,640 (515,366,912) B. Store growth per ingested
byte 54.6 → 20.6 → 3.05 → 0.52 B, i.e. sub-linear. Container `memory_current_bytes`
4.51 / 1.93 / 1.63 / 1.86 MB, swap 0, OOM kills 0; lifetime peaks 5.66/5.93/5.67/5.69
MB are lifetime numbers, not phase peaks (L18).

#### Defect found and repaired — four prepared native fixtures had been truncated

`namespace-10000`, `namespace-100000`, `store-footprint-unique-100000` and
`store-footprint-metadata-cardinality-100000` were each missing their 100 MiB
anchor file(s) — `payload/d0028/f002805`, `payload/d0071/f007187` +
`payload/d0193/f019315`, and the same pair for both store-footprint fixtures.
The two `init_namespace` samples on the `registered-fixture` tiers failed closed
with `LayerStack initialization scan receipt mismatch`; the two
`compact-low-tier-v2` tiers (anchors intact) passed. A manifest-versus-disk sweep
of all 44 fixture entries found exactly these four, and every damaged payload
directory carried mtime **2026-09-15 10:20** while intact ones still carried the
2023-11-15 generation time. It was silent because `runner.py` deliberately trusts
an owned *native* fixture's recipe and only re-validates content for non-native
masters.

Repair (no harness change, so `daa74be0…` and every banked receipt stay valid):
quarantined the four entries plus the pre-existing `.damaged-…` entry under
`benchmark-results/issue152/quarantine/`, let preparation regenerate them, and
proved byte identity — file-by-file SHA256 against the quarantined copy shows
10,002 identical files, exactly one added file (`payload/d0028/f002805`), and only
`manifest.json` differing (it embeds generation timings). Impact set re-run:
`namespace-10000`, `namespace-100000`; the two store-footprint cells are
re-collected in G7 on the regenerated fixtures.

An improved scan-mismatch error message was written and then **reverted**: it
moved the compilation seal off the frozen `79dab102…` for a cosmetic diagnostic.
The underlying gap (native fixtures are trusted, not re-validated) is recorded as
a limitation with a recommended cheap `stat`-only guard.

Evidence: `benchmark-results/issue152/g1/` (perf + verify receipts, wall seconds,
command logs) and `benchmark-results/issue152/quarantine/`. Group report:
issue #152 comment 5675349936.

### L23 — 2026-09-18: #152 G2 SDK-edit families (56 PASS + 5 NOT_RUN_OPTIONAL) and the mapped-write Commit-visibility regression

Identical collection identity to L22 (source `0debfccb…` @ `8b5e0955e`, product
`31a42c95…`, compilation `79dab102…`, harness `daa74be0…`, workload `821b2404…`,
image `layerfs-bench-infra:0debfccbfe56516f`). Command shape: one sample per cell,
`--repetition 1 --setup clone --perf-fast --collection-mode`,
600/630/900 s allowances, `LAYERFS_CONSTRUCTION_WORKERS=1`.

#### Results

56 of 61 registered selections collected — `edit_length_preserving` 12,
`edit_canonical_chunk_count` 12, `edit_length_changing` 32. **56/56 comparative
PASS, 56/56 cleanup PASS, 56/56 independent proofs PASS.** Worst ratio 1.24×
(`overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1`, +2.63 ms); 48 of 56
are faster than their v0.1.5 comparator, down to 0.50×, which is the expected
effect of removing the whole-Commit lifecycle lock on this route. Complete
commands 1–4 s (no 25 s exception needed); verification walls 0.32–0.50 s.

The five `edit_length_changing_capped` cells are **NOT_RUN_OPTIONAL**: they fail
closed with `family is not admitted to host-store execution`
(`shared/runner.py:31,309`). They are version-retained duplicates — verified
one-for-one against the measured `edit_length_changing`
`-result-capped-v2-ops-1` successors, same registered `fixture_bytes`
(524,283,904 ×4, 524,285,952 ×1) — and `QUICKSTART.md` states obsolete capped-edit
entries outside the host runner are not additional active cases. Not admitted to
`HOST_FAMILIES` deliberately: that harness change would resurrect a retired
workload and move the frozen harness identity.

#### Per-case guardrail counters (all 56 cells)

`capture_mode=Live`; `commit_pause_fence_ns=0` in 56/56 (no pause fence on the
Commit path); `live_backing_request_bytes=81` constant; `fuse_kernel_write_bytes=0`
and `fuse_host_frame_bytes=0`; `spool_write_bytes=0`;
`physical_spool_high_water_bytes=0`; `final_live_non_base_bytes=4096` at every
tier from 1 MiB to 500 MiB; `commit_cdc_bytes_scanned=4096` (65,536 for the
chunk-count family by design). cgroup `file_peak=4096` B on every cell
**including the 500 MiB tiers**, `file_dirty_peak=4096` B, `shmem=0`, `swap=0`,
`anon_peak` 0.66–1.26 MB — bounded and independent of fixture size, so the L18
file-cache-amplification signature is absent.

#### Correctness regression — kernel-dirty mapped bytes are outside the Commit frontier

Focused probes (env-gated, not part of any gate):

    RUSTUP_TOOLCHAIN=1.85.1 LAYERFS_LIVE_DOCKER=1 \
      LAYERFS_LIVE_DOCKER_IMAGE=layerfs-bench-infra:0debfccbfe56516f \
      cargo test -p layerfs-sdk --locked --test live_docker \
      running_commands_and_dirty_mappings_continue_across_commit -- --nocapture

Candidate: **FAIL 5/5** (`mapped snapshot exact stable bytes: path=held-a
later=false first_mismatch_index=0 expected=65 observed=0`). Control arm
(`/Users/yifanxu/layerfs-v016-control`, product `276c5970…`, image
`layerfs-bench-infra:40bb391e1efccde7`): **PASS 2/2** on the identical probes.
Boundary isolated by a temporary `msync(p, 4096, MS_SYNC)` before the client's
`ready` line: the first-Commit check then passes and the failure moves to the next
checkpoint, so the rule is exactly "kernel-writeback bytes are captured, kernel-only
dirty bytes are not". Root cause: `crates/layerfs-fuse/src/live_owner.rs:2070` —
*"Take an operation cut for an SDK edit transaction… Commit capture never calls
this; snapshots are stable by frontier ownership, not by pausing the workspace."*

Both diagnostic edits to `crates/layerfs-sdk/tests/live_docker.rs` were reverted
and the file hash restored (`a45c294cf0b7081d9fbfa4c5d353bba7b3bae80d5093c5583021fba18d3f08db`);
the tree was clean for the whole collection.

Not repaired, deliberately and in writing: the frozen spec
(`sandbox-local-snapshot-spec-and-plan.md` §10 and §9 boundary) already declares
that a local root snapshot does not establish kernel-dirty mmap visibility, forbids
the workarounds ("do not disable supported mappings, omit their bytes silently,
patch a dependency/kernel to pass"), and instructs that the exact remaining
limitation be reported when no compliant mechanism is found; the architecture doc
and the roadmap README's adoption recommendation carry it as "dirty shared-mmap
visibility stays unsolved". The two candidate repairs are out of bounds for this
campaign — a pause/drainage in the Commit path is a declared-architecture change,
the §9.1 direct-I/O variant is "PROPOSED, not adopted", and `AGENTS.md` §4 forbids
dependency/kernel patches. No registered selection is on this path: the registered
families write through ordinary FUSE/SDK routes, which are in the frontier.

Group report: #152 comment 5675466677.

### L24 — 2026-09-18: #152 G3 workspace-locality/churn (34 PASS + 3 REUSED-FROM) and the rewrite-route file-cache bound

Collection identity as L22/L23. `--seed 1 --setup clone --perf-fast
--collection-mode`, 600/630/900 s, one sample per cell.

34 fresh cells: **34/34 comparative PASS, 34/34 cleanup PASS, 34/34 proofs PASS.**
Worst ratios `workspace-clean-commit-1-compact-v2` 1.66× (+6.98 ms) and
`workspace-clean-commit-10-compact-v2` 1.62× (+7.32 ms), both accepted by the
< 10 ms absolute test; everything else ≤ 1.22×, 27 of 34 faster than v0.1.5.
Slowest complete command 12 s. Absolute gate `tiny-create-100-mixed-v4 < 1 s`
measured **61.41 ms**. B1 `tiny-create-500-mixed-v4` 244.35 ms (1.01×), B2
`tiny-bulk-create-500-mixed-v3` 4.755 s (0.83×), B3
`local-snapshot-create-25000-onebyte-v1` 10.381 s vs control `perf-CTRL-25k-r5`
8.874 s (1.17×) — all three **REUSED-FROM** their banked receipts, not re-run.

#### Guardrail 4 failure on the rewrite route (diagnostic, not a gate sample)

cgroup `memory.stat` sampled at ~50 ms during selected cells (L18 method):

| cell | payload | file peak | anon peak | current peak |
|---|--:|--:|--:|--:|
| dense-rewrite-1-compact-v2 | 1 MiB | 8,192 | 0.66 MB | 5.15 MB |
| dense-rewrite-10-compact-v2 | 10 MiB | 12,304,384 | 3.98 MB | 21.70 MB |
| dense-rewrite-100-mixed-v4 | 100 MiB | 108,109,824 | 18.78 MB | 132.77 MB |
| dense-rewrite-500-mixed-v4 | 500 MiB | **529,182,720** | 33.82 MB | 573.82 MB |
| tiny-bulk-create-500-mixed-v3 | 500 MiB | 2,412,544 | 12.48 MB | 24.40 MB |
| tiny-create-100-mixed-v4 | 100 MiB | 8,192 | 3.69 MB | 3.10 MB |
| workspace-clean-commit-500-mixed-v4 | 500 MiB | 4,096 | 0.38 MB | 2.99 MB |

`file` grows at ~1.03× the payload on the rewrite route; `shmem = 0`,
`file_writeback = 0`, `file_dirty ≤ 1 MiB` throughout, and the charge collapses to
4 KB at teardown, so the pages are the workspace's. Mechanism: create handles are
returned with `FOPEN_DIRECT_IO` (`filesystem.rs:1430`) which is why every
create-shaped cell is ≤ 8 KB, while a rewrite of a pre-existing file uses an
ordinary writable open that does not bypass the page cache.

**Control parity:** the reconstructed v0.1.5 control measured 525,750,272 B
(501.4 MiB) on the identical cell — parity with the candidate's 504.7 MiB
(+0.7 %). This is therefore **not** a v0.1.6 regression; L18's repair covers the
transfer path (reproduced: 2.3 MiB vs the recorded ≤ 2.6 MiB) and never covered
the rewrite route. Not repaired: the only bounding mechanism is the plan's
§9.1 direct-I/O-for-writable-opens proposal, explicitly *"PROPOSED, not adopted"*,
which removes supported writable shared mappings — a declared-architecture scope
decision this campaign may not take. Reported, not excused (AGENTS.md §1).

Diagnostics: `benchmark-results/issue152/g3-diag/` (7 candidate timelines +
`-memdiag` receipts) and `/Users/yifanxu/layerfs-v016-control/benchmark-results/issue152-control/`.
Group report: #152 comment 5675608355.

### L25 — 2026-09-18: #152 G4 — v0.1.6 could not truncate a workspace file (fixed, `58e4f7f47`)

#### The defect

All four `git_tool_workflow` cells failed on the frozen candidate with

    git ["commit","--no-gpg-sign","--no-verify","-m","layerfs v0.1.3 tool workflow"]:
      fatal: could not open '.git/COMMIT_EDITMSG': Invalid argument

The reconstructed v0.1.5 control (`276c5970…`, image
`layerfs-bench-infra:40bb391e1efccde7`) passed the same cells 2/2 on the same
harness, so it was a product regression.

#### How it was pinned

A full `git init/add/commit` sequence with the workload's exact `GIT_CONFIG`,
environment and `umask` passes in a fresh workspace, so the trigger needed the
fixture/workload state. Instrumenting the FUSE daemon and following the sample
container's `docker logs` live gave the end of the trace:

    LAYERFS-OPEN-ENTER ino=315 flags=0x20001 stateless=true   # the only O_WRONLY open in the run
    LAYERFS-OP Open            -> Ok
    LAYERFS-OP Setattr         -> size=Some(0)                # the kernel's O_TRUNC
    LAYERFS-TRUNCATE node=315 size=0
    LAYERFS-ERRNO Invalid                                     # EINVAL

#### Root cause

The sandbox-local rewrite moved the payload backing into the workspace
(`LocalSpool`) and the host immutable-base service rejects the removed payload
opcodes — `crates/layerfs-workspace/src/live_backing.rs` carries
`immutable_base_service_rejects_removed_payload_opcodes`, whose list includes
`wire::CHECK`. `truncate_async` was the one edit path never migrated (compare
`v0.1.5`, where the host owned the spool): it sent a `CHECK` frame to the host
before applying a prepared edit, so the host refused it, `PortError::Invalid`
became `EINVAL`, and every **size-changing** truncate failed. The kernel turns
`open(O_TRUNC)` into `setattr(size)`, so ordinary open-for-truncate failed too.

#### Fix — `58e4f7f47` (`layerfs-fuse: apply truncate edits locally`)

Remove the host frame; the window of still-referenced segments is expressed by
the local spool's `BackingRef` graph, and the write path already applies edits
with no host payload traffic. Focused regression test
`truncate_applies_locally_without_a_removed_host_payload_check` asserts a
size-changing truncate succeeds and only `SEED` reaches the backing; verified to
**fail without the fix** (`panicked … unexpected backing opcode 7`) and pass with
it. `tools/preflight.sh`: all steps passed.

#### Impact set and identity

Any cell that reached the branch would have failed, and G1–G3 all passed, so no
passing cell took it; the blast radius of the fix is exactly the four git cells.
New candidate identity: source
`957eae85366e771ae228305387cb2d9ed85d86c17558dbc5fb06260fe252ebbe` @ `58e4f7f47`,
**product `ce2336b71afe6b04b7daf8d063347bf07e01f55aae78f3a7cdc468750aca8350`**,
compilation `1d17454b4ca232b33e67540edd797d777db682a0e05ff4695db0cc608b74d238`,
harness `daa74be0…` and workload `821b2404…` unchanged, image
`layerfs-bench-infra:957eae85366e771a`. All 20 G4 cells were re-collected on the
new seal (not just the four) so the group is not split across two products; the
pre-fix receipts stay on disk under `benchmark-results/issue152/g4/`. G1–G3 keep
their recorded pre-fix identity and their numbers are not re-labelled.

#### G4 results on the new seal

`git_tool_workflow` 0.92× / 0.89× / 0.79× / 0.94×; `namespace_mutation`
0.83–0.92×; `directory_construction_traversal` 0.83–1.14×. **20/20 comparative
PASS, 20/20 cleanup PASS, 20/20 proofs PASS.** Complete commands ≤ 8.56 s
(`git-tool-500-mixed-v4`), inside the 15 s rule; its 29.85 s end-to-end wall is
preparation (closed-master clone + the one-time `docker cp` of the qualified Git
reference tree), excluded by the rule.

Group report: #152 comment 5676373486 (that comment contains a typo,
`--no-gpus-sign`; the workload argument is `--no-gpg-sign`).

### L26 — 2026-09-18: #152 G5 — host-continuation Commit reported itself as its own predecessor (fixed, `29835f44d`)

G5 collected `mixed_load_bearing` 4/4 PASS and then failed **8/8
`payload_create_read` verifications** with

    fs-benchmark-pro: host continuation returned/published Commit mismatch

while the same cells' perf samples passed and all four `mixed_load_bearing`
proofs passed. `host_continuation_proof`
(`benchmark/fs-bench-pro/src/workspace_bench.rs`, present since `95508a3d7`,
unmodified by the harness commits since #120, gated on
`case.family == "payload_create_read"`) commits twice in one live workspace and
asserts that the second Commit's `previous_head` is the first Commit's
`commit_id`. Its `host-continuation-commit-proof` step-0 record exists, so the
failure was in the lineage assertion, not in publication.

#### Root cause

`remote_commit.rs`'s host-continuation route re-bases the host shell onto the
published root (`rebase_host_workspace` sets `workspace.expected_head` to the
head just published) and only afterwards assembles the result through
`result_from_outcome`, which reads `workspace.expected_head`. The verification
receipt shows the consequence directly:

    WorkspaceCommitStatus { result: Created { previous_head: Some(CommitId(12875b96…)),
                                             commit_id:     CommitId(12875b96…) }, … }

Publication was correct — `published-root` and the branch head agree with
`commit_id`. The `Published` route is unaffected because it returns before any
re-base. `previous_head` is a public SDK field used for change detection and CAS,
so this is a correctness defect, not a cosmetic one.

#### Fix — `29835f44d`

Sample `expected_head` inside the block that already holds the workspace lock,
before the re-base, and assemble the result from it with a pure helper
(`result_from_pre_publication_head`). No extra lock; publication, published root,
storage and timers untouched. `tools/preflight.sh`: all steps passed.

Regression coverage: the registered proof itself — 8/8 FAIL without the fix,
8/8 PASS with it, PASS on the reconstructed v0.1.5 control on the same harness.
The pre-existing `crates/layerfs-workspace/tests/reconciliation.rs` assertion on
`previous_head` covers only the host-materialized route, which is why the
sandbox route's regression survived.

#### New candidate identity and impact set

Source `bebc8c9805e2acefa637881213e952ac835ec35f6149254dacd91effaad88c6e` @
`29835f44d`, **product
`dc2b3a14f45a4eb7d07b132562b3eadaba5e87644b517aaf6189ce1008e8490d`**,
compilation `36a30d3cd0f86cf4167f3b7ff2fc61db2d71823b1288b91c2aa109e521ffb7c2`,
image `layerfs-bench-infra:bebc8c9805e2acef`. Impact set by call path: only the
returned value of one field on the host-continuation route changes, so only the
8 `payload_create_read` cells were re-collected (perf + verify) — matching #152's
"re-run only the affected cases". `mixed_load_bearing` keeps the L25 seal;
G1–G4 keep their recorded identities.

#### Results

`mixed_load_bearing` 0.87× / 1.24× / 0.71× / **0.44×**; `payload_create_read`
0.74–1.38×. **12/12 comparative PASS, 12/12 cleanup PASS, 12/12 proofs PASS.**
Complete commands ≤ 4.15 s. The single cell above 1.25× is
`payload-create-1m-compact-v2` (1.38×, +11.29 ms) — accepted by the ratio test,
same shape at the other tiers 1.01× / 0.99× / 1.17×, recorded rather than
explained away.

Group report: #152 comment 5676750996.

### L27 — 2026-09-18: #152 G6 dedup block (59 PASS + 6 diagnosed material regressions + 1 proof)

Identity: source `bebc8c9805e2acef…` @ `29835f44d`, product
`dc2b3a14f45a4eb7d07b132562b3eadaba5e87644b517aaf6189ce1008e8490d`, compilation
`36a30d3c…`, image `layerfs-bench-infra:bebc8c9805e2acef`, harness `daa74be0…`,
workload `821b2404…`. 65/65 registered selections terminal: 64 performance
samples + `dedup-cdc-boundaries-proof` (PASS, 0.58 s). 64/64 cleanups PASS,
64/64 independent proofs PASS. Complete commands 0.4–13.7 s.

#### Six material regressions, all one cause

`dedup-cross-file-identical-500` 1.65× (+136.61 ms);
`dedup-cdc-scattered-500` 1.61× (+864.05 ms); `dedup-cdc-delete-100` 1.60×
(+34.70 ms); `dedup-cdc-overwrite-500` 1.59× (+144.41 ms);
`dedup-cdc-delete-500` 1.59× (+144.23 ms); `dedup-cdc-insert-500` 1.50×
(+128.86 ms). All six fail both halves of the bounded-acceptance rule and are
recorded as FAIL with this diagnosis rather than accepted.

Each is a single `initialize_layerstack` call (`pure_call_sum_ns`, one
`initialize` phase), i.e. host canonical construction.
`crates/layerfs-layerstack-store/src/objects.rs:3545`/`:4488` admit
`worker_limit.min(SMALL_CONTENT_WORKERS)` constructors, and directive 2 mandates
`LAYERFS_CONSTRUCTION_WORKERS=1` in every run, so initialization drops from the
released 4-way small-content parallelism to one producer. The #120 comparators
predate directive 2.

Direct proof (labelled diagnostic, not a gate row): re-running the worst cell
with `env -u LAYERFS_CONSTRUCTION_WORKERS` and everything else identical gives
**1,458.89 ms** against the comparator's 1,426.04 ms (1.02×), versus the gate
sample's 2,290.09 ms (1.61×). The entire delta is the single-worker directive.
Stored at `benchmark-results/issue152/g6-diag/`.

Context only: the group's median ratio is ≈ 0.85×, 46 of 64 cells are faster than
v0.1.5, and the effect scales with how much unique content construction has to do
(`dedup-cdc-common-body-500` 1.18× vs `dedup-cdc-scattered-500` 1.61×). The
`dedup-history-unrelated-500-mixed-v2` cell that #120 carried as its single
unrepaired S2 against a `< 15 s` target measures 12.254 s here (0.76×) → PASS.

#### Driver correction

The first G6 attempt passed `--setup clone` to `dedup_cross_file` and
`dedup_cdc_locality`, which are registered `fresh-output`: 30 invocations failed
immediately with `initialization requires a fresh output Store; clone is not
applicable`. Driver mistake, not a measurement; re-collected with the registered
policy. The failed directories were removed before this was recorded — the `rc=`
lines survive in `benchmark-results/issue152/g6.log`.

Group report: #152 comment 5676984615.

### L28 — 2026-09-18: #152 G7 — store_footprint clean (harness fix `b9bca593c`), six reliability injections not migrated

Identity: source `d1bbf882067d824b…` @ `b9bca593c`, **product
`dc2b3a14f45a4eb7d07b132562b3eadaba5e87644b517aaf6189ce1008e8490d` (unchanged)**,
compilation `2c4ec5ee…`, image `layerfs-bench-infra:d1bbf882067d824b`.

#### store_footprint — 6/6 PASS after a harness-only fix

All six cells first failed with `Store-footprint Workspace temporary-byte
accounting`. Instrumenting the message gave `commit edit_spool allocated=10
live=10 superseded=0 peak=10 fuse spool_write_bytes=0`: the Commit's own
accounting is internally consistent, and only the cross-subsystem conjunct
failed, because the host shell's FUSE write-spool metric is dead on this route
— v0.1.6 moved the payload spool into the sandbox (`projection.rs`: "The remote
workspace's physical spool lives in the sandbox"). Harness-only fix
`b9bca593c`: keep the internal-consistency assertions unconditional, apply the
host cross-check only where the host metric is populated, and keep the observed
versus expected error text. Product seal unchanged. Cells now 1.00× / 1.03× /
1.07× / 1.06× / 0.74× / 0.83× with clean proofs; canonical Store growth within
1.4 % of v0.1.5 and the metadata-cardinality overhead identical to v0.1.5's.

#### workspace_reliability — 21/28 PASS, 6 FAIL, 1 NOT_RUN_OPTIONAL

The six failures share one cause: their fault-injection points sit on the
pre-v0.1.6 host-owned routes. `candidate-failure-retry` arms
`INJECT_CANDIDATE_FAILURE` in `Workspace::build_candidate` (`changes.rs:333`)
while the host-continuation route builds through `build_remote_candidate`
(`remote_commit.rs:143`); `deferred-nospace` and `short-spool-write` inject into
the host shell's append path (`file_io.rs:466`, `:797`);
`published-presentation-failure-smoke-v3` injects into the host materialized
projection; `admission-batch-failure-retry` and `final-publication-failure-retry`
surface `faulted Commit did not surface exact injected error` and, for the
latter, the sandbox route's retained-stage guard
(`Workspace(Storage(InvalidInput("workspace stage retained")))`,
`remote_commit.rs:50`).

Control A/B: all six **PASS** on the reconstructed v0.1.5 control
(`/Users/yifanxu/layerfs-v016-control/benchmark-results/issue152-control3/`), so
they are not stale-in-both-arms tests.

Recorded as **FAIL (diagnosed)**, not WARN: their purpose is recovery evidence
and on the sandbox route that evidence does not exist. Not repaired here —
re-pointing six injections into `build_remote_candidate`, the sandbox admission
path and `LocalSpool` is feature-sized test instrumentation and #152 is an
execution/reporting issue. Follow-up named in the report. The other 21 proofs
(non-injection surface) PASS; `sustained-600s` is NOT_RUN_OPTIONAL
(`verification_supported: false`; the 600 s sustained proof is a separately
accounted optional long test).

Group report: #152 comment 5677526128.

### L29 — 2026-09-18: #152 G8 — historical_access NOT_RUN (sealed v2 Store unavailable), campaign final report

`runner.py --family historical_access --list` returns the intact v2 fixture
(`store_sha256 f323de0e0f9ae1030efc142402bc033ad427134dd8c69f21eb5b5d6ef7426eb7`,
branch `1101a0877559737f07bccc2ffe2a82d9f3`, 11 cases). Search for that Store:

1. `benchmark-results/host-store/prepared/` — no history/access preparation
   (this family prepares nothing) and no matching hash.
2. `benchmark-results/host-store/issue118/20260912/` — three sealed full157
   stores and the 2026-09-12 access receipts exist, but those runs used the v1
   template ids against a different store: `full157-candidate-1/…/frozen-measured-store/store.sqlite`
   hashes `0e767a7f…` with branch `1101a094892f39783da6dca308acdba50b`.
3. Every `store.sqlite` ≥ 50 MB under the whole `benchmark-results` tree (185
   candidates: 9 outside `prepared/`, 176 inside) plus all 6 in the control
   worktree were hashed — **no match**.
4. The path named by `docs/roadmap/0.1/0.1.5/issue101/results.md:64`
   (`layerfs-issue100-45mb-evidence/retained-full157-1/…`) was removed by the L17
   cleanup; a bounded search under `/Users/yifanxu` finds no such directory.

Disposition: **NOT_RUN** for 11 performance cases + 11 proofs, with that reason.
No Store was reconstructed, no substitute used, no v1 receipt promoted to a v2
row. `repository_history` (3 optional profiles) is **NOT_RUN_OPTIONAL**;
`workspace-sustained-600s` is **NOT_RUN_OPTIONAL**.

The campaign final report is committed at
`docs/roadmap/0.1/0.1.6/evidence/issue152-final-report.md`: identity chain,
terminal tally, the complete `family → per-test` table (196 collected cells plus
REUSED-FROM and non-collected terminal rows), the bug ledger, the architecture
guardrails, the resource tables, the remaining limitations and the decisions.
Group report: #152 comment 5677788950.

### L30 — 2026-09-19: the six `workspace_reliability` failures fixed; 27/27 proofs PASS on the frozen candidate

Work item handed over by L28 / the #152 final report (§7.2), specification
`docs/roadmap/0.1/0.1.6/issue152-reliability-fix-handoff.md`. Product commit
`ac729dfeb` — *fix(workspace): re-drive a failed sandbox publication; migrate the
six reliability injections*. Report:
`docs/roadmap/0.1/0.1.6/evidence/issue152-reliability-fix-report.md`.

#### Identity

| field | before (L28) | after (this entry) |
|---|---|---|
| source seal | `d1bbf882067d824b…` | `8308cd8e628a97cd8b7d17d184a8f69ff5f212d21b0646d84913a6df5d444e9a` |
| product seal | `dc2b3a14f45a4eb7…` | `970964e9af43a8bf57f0d7bec70736a94171f7beb62fc3378ea5cc4797500ebd` |
| compilation seal | `2c4ec5eea9a731b9…` | `bff3ff080d64671f9bf9ef7e73450afd4dbaaecbce86c91a5c6ae285d5b63743` |
| dependency seal | `9a13be19016e0991…` | `a1cf72ac4b77536d2d3b44c09872457a246673ca2eacf0997b11b293900709eb` |
| harness identity | `daa74be0…` | `daa74be0…` (unchanged) |
| workload source | `821b2404…` | `821b2404…` (unchanged) |
| image | `layerfs-bench-infra:d1bbf882067d824b` | `layerfs-bench-infra:8308cd8e628a97cd` |
| commit | `b9bca593c` | `ac729dfeb` (tree clean, `LAYERFS_SOURCE_DIRTY=false`) |

The product seal moved because #6 is a behaviour fix and #1/#2/#5 add
`test-instrumentation`-gated consume sites. Nothing in #1–#5 changes a released
build's behaviour; #3/#4's channel is compiled out entirely unless
`layerfs-fuse/test-instrumentation` is enabled, which only
`Dockerfile.layerfs` (benchmark image) does.

#### #6, the product defect

`commit_remote` refused **any** Commit while `pending_stage.is_some()`
(`InvalidInput("workspace stage retained")`), before the pending-completion
re-delivery path, while the materialized route re-drives. Deleting the guard was
not sufficient: the sandbox resolves one attempt at a time
(`live_owner.rs` CAPTURE returns `Busy` while a slot is retained) and
`LiveWorkspace::capture_frontier` *moves* the live dirty set into the attempt, so
abandoning it and capturing again would publish an empty candidate — silently
dropping the very data the retry is supposed to publish.

Fix: `commit_remote` samples the retained attempt, retains the capture summary
(`Workspace::pending_attempt`) before the first step that can fail, and a retry
re-pulls and re-publishes **that exact frozen generation**. The "workspace stage
retained" refusal now applies only when a stage exists with no attempt that can
be re-driven (the Busy/HeadMoved path, unchanged). `settle_attempt` clears the
attempt exactly when it is completed or cancelled; `Workspace::discard` and
`end_clean` treat an unsettled attempt as unfinished publication state.

Regression test `remote_commit::tests::failed_publication_is_re_driven_by_the_supported_commit_retry`
drives a real local live owner and the real `VerificationStoreFault::FinalPublication`
Store fault. Without the fix it fails with the recorded
`Storage(InvalidInput("workspace stage retained"))`; with it the retry returns
`Created` and the retained generation's bytes read back from the published root.

#### #1/#2/#5, consume sites moved onto the route that does the work

- `candidate-failure-retry`: the `VerificationFault::Candidate` consume plus
  `inject_candidate_failure_once` and the Store admission-fault activation
  (`verification_candidate`) now run before `build_remote_candidate`, mirroring
  `lifecycle.rs:94-106`; `build_remote_candidate` honours the one-shot flag the
  same way `build_candidate` does.
- `admission-batch-failure-retry`: with the fault active before construction, the
  construction-time cohort commit is counted, and the receipt shows the gate
  holding exactly as designed: `committed_early_transactions: 1`, `hit_count: 1`.
- `published-presentation-failure-smoke-v3`: the `PresentationResume` consume and
  the recorded `workspace.presentation_failed` now exist on the sandbox route at
  the transition where that route computes its own presentation state. A retained
  completion is deliberately **not** recorded as a presentation failure: it is
  re-delivered by the next Commit or End (unchanged design).

Threading decision, recorded as the handoff asked: no channel was made
process-global and no worker count changed. Each consume site was moved onto the
thread that performs the work the fault targets, which is the same thread that
arms it.

#### #3/#4, the payload append is in the container

Both fault channels are process-local and the append now happens in the sandbox
owner (`LocalSpool`), so migrating the site could not work — and this is the one
place where the fix is *not* benchmark-side: the arm has to cross a process
boundary. New one-shot arm on the existing authenticated control lane
(`wire::VERIFICATION_FAULT` / `VERIFICATION_FAULT_RECEIPT`, opcodes 52/53),
consumed in `LiveOwner::write_owned`:

- `NoSpace` lowers the workspace policy to the current charge for one append, so
  the real `ResourcePolicy::check` rejection and its public `ENOSPC` are produced
  by product code (the same injection shape the pre-v0.1.6 host shell used).
- `ShortAppend` completes half the reserved range and fails the append with `EIO`
  before any piece is applied.

Both are one-shot and receipt-checked (`RemoteVerificationFaultReceipt { fault,
hit_count: 1 }`), so the proofs keep their exactly-once evidence. The whole
surface sits behind the new `layerfs-fuse/test-instrumentation` feature,
propagated through `layerfs-workspace/test-instrumentation` and
`layerfs-daemon/test-instrumentation`; `Dockerfile.layerfs` is the only build that
enables it, so a released daemon carries neither the frames nor the injection
points. Unit test
`live_owner::verification_fault_tests::armed_append_fault_is_consumed_exactly_once_and_stays_on_the_receipt`
pins the receipt semantics (a first attempt at this returned the receipt *after*
consumption cleared the arm and was caught by the proof).

#### Result: 27/27 verification-supported proofs PASS

`benchmark/fs-bench-pro/verify-selected.py --family workspace_reliability
--case <case> --seed 1 --setup clone --image layerfs-bench-infra:8308cd8e628a97cd
--collection-mode --source 8308cd8e… --input <digest>` for every registered
verification-supported case, one sample each, fresh append-only outputs under
`benchmark-results/issue152/fix/<case>/`. **27 PASS, 27 cleanups PASS**, no
unconsumed-fault record in any receipt, `reused_proof_identities` empty (nothing
reused a prior receipt). Complete commands 1.69–6.02 s (6.02 s =
`workspace-exec-500`), inside the 15 s rule and far inside the 59 s hard budget.

The six repaired cases:

| case | status | key evidence |
|---|---|---|
| `workspace-final-publication-failure-retry-compact-v2` | PASS | `Integrity("injected qualified Workspace transaction failure")` on the first Commit, `FinalPublication hit_count: 1`, retry publishes (`proof-outcome-before-cleanup: Ok(())`) |
| `workspace-candidate-failure-retry-compact-v2` | PASS | `Integrity("injected Workspace candidate failure")`, `Candidate hit_count: 1`, retry publishes |
| `workspace-admission-batch-failure-retry-compact-v2` | PASS | `Integrity(…)`, `LaterAdmissionBatch hit_count: 1, committed_early_transactions: 1`, retry publishes |
| `workspace-published-presentation-failure-smoke-v3` | PASS | `Created` + `presentation_failed: true`, `PresentationResume hit_count: 1`, recover-start/recover-complete |
| `workspace-deferred-nospace-compact-v2` | PASS | ENOSPC at the write boundary, `RemoteVerificationFaultReceipt { NoSpace, hit_count: 1 }` |
| `workspace-short-spool-write-compact-v2` | PASS | EIO at the write boundary, `RemoteVerificationFaultReceipt { ShortAppend, hit_count: 1 }` |

Control A/B is unchanged and still applies: all six **PASS** on the reconstructed
v0.1.5 control (product `276c5970aabf`, source `40bb391e1efc`,
`/Users/yifanxu/layerfs-v016-control/benchmark-results/issue152-control3/`), so
none of them is a stale-in-both-arms test. `workspace-sustained-600s-compact-v2`
remains **NOT_RUN_OPTIONAL** (`verification_supported: false`).

#### Non-passing lines, stated as plainly as the passes

1. **A failed append now leaves a dead range packed in the sandbox spool
   segment.** Observed through the container-side counter (the host-side
   counterpart is the dead metric in §7.4): `physical_spool_allocated_bytes`
   4096 → 8192 after the injected failure in both #3 and #4, where the v0.1.5
   control read 4096 → 4096 (its short-append run raised only the *peak* to
   8192 and truncated back). The dead range is bounded by segment capacity
   (1 MiB) and retired with the segment; not repaired here because reclaiming it
   would change product spool policy, which this instrumentation work item does
   not own. Named for a future decision.
2. **The two proofs' `spool_segment_bytes` conjunct is vacuous on this route**
   (`verification_workspace_state` reports 0 for every remote workspace), which
   is the already-recorded dead host FUSE write-spool metric. The meaningful
   assertions — exact errno at the public boundary, zero acknowledged bytes,
   unchanged published snapshot, clean Discard — all hold.
3. **`changes::tests::tiered_spill_partial_writes_and_final_merge_are_retryable_and_clean`
   is a pre-existing parallel-run flake.** It counts process-wide file
   descriptors, so any concurrent test that opens a file can fail it. Verified
   on the pristine tree (`git stash` of this entire change set, same failure;
   `--test-threads=1`: 73/73 pass). `tools/preflight.sh` passed on the final
   tree; this is recorded because the flake can fail the suite at random.
4. **Seals moved, so no earlier perf row is re-labelled.** The sandbox Commit
   entry gains one uncontended workspace-lock acquisition and one retained
   `Option<CaptureSummary>` store outside every measured phase's *work*; the
   benchmark image's FUSE write path gains one relaxed atomic load per write
   (compiled out in a release build). Both are sub-microsecond against Commit
   totals of 10 ms–2 s. The handoff's impact set for #6 — the three
   `*-failure-retry` proofs plus the rest of `workspace_reliability` — was
   re-run in full; no perf row was re-collected, and no perf row's measured
   semantics changed. Flagged rather than silently claimed as unaffected.

Commands and receipts: `benchmark-results/issue152/fix/` (27 final receipts plus
`provisional-329a33bc/`, `provisional-4f82c19a/`, `provisional-b4c4afee/` — the
intermediate attempts, kept because receipts are append-only).
Group report: [#152 comment 5683593603](https://github.com/Ephemeral-AI-Lab/layerfs/issues/152#issuecomment-5683593603).

#### Owner decision and issue closure

#152 closed at the owner's direction on 2026-09-15
([closing record, comment 5683799190](https://github.com/Ephemeral-AI-Lab/layerfs/issues/152#issuecomment-5683799190)),
as an owner act: the issue's own charter forbids closure *as a campaign
consequence*. The decision recorded with it is **"the diagnosed slowness is
acceptable"** — the six G6 dedup single-worker time regressions (1.50–1.65×, one
cause: `LAYERFS_CONSTRUCTION_WORKERS=1`) and the earlier `namespace-100000` 2.7 s
cold-Init waiver. Those rows stay **FAIL as recorded** with the decision attached;
no row was re-labelled PASS, and the closure comment names the rows the decision
does *not* cover (the ~1.03× rewrite-route file cache — a resource guardrail at
v0.1.5 parity, not a timing miss — the kernel-dirty shared-mmap limitation, the
dead host write-spool metric and §5.1's failed-append dead range above). No merge,
tag or release follows; #151 keeps the experiment's create-500 / bulk-create-500 /
25k gates.

### L31 — 2026-09-19: retained deepseek-harness history profiles re-run on the v0.1.6 candidate (#153)

Owner follow-up after #152 closed: `repository_history` closed #152 as
`NOT_RUN_OPTIONAL` and had never run on the v0.1.6 sandbox-local candidate. Requested
and executed: the **157-commit `deepseek-full`** history (`stride-1`) and **`stride-3`**,
with `stride-10` as the shakedown, plus a paired reconstructed v0.1.5 control for
`stride-3`. Specification and group report:
[#153](https://github.com/Ephemeral-AI-Lab/layerfs/issues/153)
([comment 5684860153](https://github.com/Ephemeral-AI-Lab/layerfs/issues/153#issuecomment-5684860153));
in-tree report `docs/roadmap/0.1/0.1.6/evidence/issue153-retained-history-report.md`.

#### Identity

Candidate: source `8308cd8e628a97cd8b7d17d184a8f69ff5f212d21b0646d84913a6df5d444e9a` @
`7fab1027a` (clean), product
`970964e9af43a8bf57f0d7bec70736a94171f7beb62fc3378ea5cc4797500ebd`, compilation
`bff3ff08…`, dependency `a1cf72ac…`, harness `daa74be0…`, workload `821b2404…`, image
`layerfs-bench-infra:8308cd8e628a97cd`. Control (v0.1.5, reconstructed worktree
`/Users/yifanxu/layerfs-v016-control`): product `276c5970aabf…`, source `40bb391e1efccde7…`
@ `6ee1ec94c`, image `layerfs-bench-infra:40bb391e1efccde7`.

#### Results — 6/6 phases PASS, every state verified against its original oracle

| profile | states | Store allocated / apparent B | Commit sum (median) | perf wall | verify wall | verified path-states / bytes |
|---|--:|--|--|--|--|--|
| `stride-10` | 17 | 49,344,512 / 49,315,940 | 11.371 s (564.3 ms) | 98.0 s | 67.5 s | 101,477 / 561,010,345 |
| `stride-3` | 53 | 64,024,576 / 64,000,100 | 24.815 s (421.0 ms) | 241.7 s | 199.8 s | 306,861 / 1,676,767,835 |
| `stride-1` (157) | 157 | 83,947,520 / 82,677,860 | 64.108 s (341.7 ms) | 617.6 s | 570.6 s | 904,143 / 4,936,693,030 |
| control `stride-3` (v0.1.5) | 53 | 65,064,960 / 64,036,964 | 22.914 s (396.6 ms) | 235.4 s | 188.9 s | 306,861 / 1,676,767,835 |

Paired `stride-3` (identical selection, harness and fixture cache; byte-identical
canonical content 589,423,458 B / 73,476 objects in both arms): Store **0.984×** (1.6 %
less), Commit sum **1.083×** (+1.90 s; median +24.4 ms/commit), complete phases 1.027×
and 1.058× — both inside the bounded-acceptance rule's absolute branch. This is the
comparator that matters; the 2026-09-10 #100-era public row (100,700,160 B) and the
offline structural artifact (59,760,640 B) are superseded/different formats and were not
used as v0.1.6 comparators.

`stride-1` against the recorded 2026-09-12 rows (cited, not paired): allocated
83,910,656 / 83,943,424 / 83,959,808 B → **parity within 0.02 %**, apparent
82,583,652–83,525,732 B, canonical 871,588,115 B / 104,705 objects identical, Commit sum
61.991 s → 64.108 s (**+3.4 %**). Matched-Git comparators cited from the frozen Git
policy: Git53 49,332,224 B → candidate **1.298×**; Git157 56,373,248 B → **1.489×**.

Budgets: these are long selections outside #152's 15 s rule (declared in the issue);
per-state fixture install (18.6 / 63.7 / 132.0 s) is inside the measured work wall and is
reported, not excluded. Fixture preparation is one-time and outside the measured phases.
`LAYERFS_CONSTRUCTION_WORKERS=1` exported for every run; one sample and one verification
per profile; fresh output directories.

#### Non-passing lines

1. **Two pre-measurement harness transients**, no measurement affected: one
   `docker image inspect` on the candidate tag returned "No such image" (resolved by a
   fresh process; the same tag served every later run), and one run died in
   `compilation_seals()` with `rustc failed: deadline` against the 10 s seal deadline
   (`rustc +1.85.1 -vV` measures 0.33 s warm). Both were retried; the retries produced the
   receipts above. Recorded because a 10 s seal deadline is marginal on a loaded machine.
2. **Container-lifetime `memory_peak` and `file` peaks differ between arms** (candidate
   `stride-3` 159,789,056 B / 63,774,720 B vs control 137,150,464 B / 4,435,968 B). These
   are lifetime/domain values and decide nothing (`L18`); the direction follows the
   architecture — the candidate's payload spool and its page cache live in the sandbox,
   which is also why the candidate's host disk writes are 130.5 MB versus the control's
   817.9 MB for the same 53 states. Stated, not excused.
3. **No `historical_access` run**: its sealed v2 Store (`f323de0e…`) is still absent
   (`L29`), so that family stays `NOT_RUN` independently of this result.
4. These profiles are `admission_eligible: false` exploratory selections; nothing here is
   a release claim.

Receipts: `benchmark-results/repository-history/{stride-10,stride-3,stride-1}/` and
`/Users/yifanxu/layerfs-v016-control/benchmark-results/repository-history/stride-3/`
(634 MB of state receipts, custody manifests and per-state oracles).

#### Closure

#153 closed at the owner's direction on 2026-09-15
([closing record, comment 5685053999](https://github.com/Ephemeral-AI-Lab/layerfs/issues/153#issuecomment-5685053999))
with all three profiles PASS and the paired control attached. The closure accepts the
non-passing lines above **as recorded** — the two harness transients and the
container-lifetime domain differences — and does not waive them; `historical_access`
stays `NOT_RUN` until its sealed v2 Store reappears. No merge, tag or release follows.

### L32 — 2026-09-17: owner directive — retire the local preflight gate permanently

- Directive, verbatim: "i want permanent preflight disable ... because we are in big
  architecture shift, old preflight gate is useless and time consuming", followed by
  "update agents.md to not to bring preflight back".
- Actions: `tools/preflight.sh` was replaced by a retirement notice that runs no
  checks and exits 0 with an explicit statement that the zero status is not a pass;
  `AGENTS.md` §4 now states that the repository has no CI **and** no aggregate
  pre-push gate, forbids running or restoring it, and forbids reintroducing an
  equivalent aggregate gate, workflow or wrapper; `core/AGENTS.md`, `core/README.md`
  and the C2 README were updated to name the per-workspace commands instead.
- Consequence: nothing gates a push automatically, exactly as with L21. Verification
  becomes an explicit, reported act per workspace: for the replacement product
  `cargo +1.85.1 test/clippy/fmt --manifest-path core/Cargo.toml --locked` plus
  `core/tools/check_product_boundary.py` and its self-tests. A push must state which
  checks ran, which did not, and why; "preflight passed" is no longer a claim that
  can be made, and no commit may imply it.
- Scope: repository-wide, all workspaces, permanently — not a temporary suspension.
- Retained deliberately: the per-workspace commands themselves, the core product
  boundary guard, the LOC counter and its unit tests, and the production-LOC rule in
  `AGENTS.md` §4. Only the aggregate, minutes-long gate is gone.
- Reversal, if ever wanted: restore the script from git history
  (`git show 30f5d0633:tools/preflight.sh`), and revert the `AGENTS.md` §4 paragraph
  in the same commit so policy and tooling cannot disagree. Do not do this while the
  architecture shift is in progress.

### L33 — 2026-09-17: Stages 3-4 closeout after the independent review

- Scope: the component-decoupling batch (#168 / #169 under #165), reviewed at the
  pinned snapshot `91c3a0741fff64e8161d5c1b6e759f347ffbf757`. The review found the
  implementation substantially complete but not complete; this entry records what
  the closeout changed and every number it produced. Full gate table, per-packet
  changes and controls: `docs/roadmap/0.1/0.1.7/component-decoupling/stages-3-4-closeout-report.md`;
  raw evidence: `docs/roadmap/0.1/0.1.7/evidence/stages-3-4-closeout-20260916T235641Z/`.
- Fixed, with a control that fails without the fix: CHUNK advisory candidates now
  take the eligibility decision instead of failing the save (`ObjectMissing` in the
  control); the owner's publication ceiling reaches all four pooled read sites
  (structural control + cached-group control); checkpoint D holds one decoded
  representation per unfinished node, encodes and hashes once at final emission and
  releases superseded drafts (frontier peak `[2208, 2288, 2448, 2768, 3408]` bytes at
  1/2/4/8/16 edits versus `[6308, 12856, 26672, 57184, 129728]` with the release
  removed); an accepted metadata depth is usable to its cap of 50 (control fails at
  round 17 with the old charge and at the cap with the old walk bound); the pooled
  index invalidation, the value-group compression branch, the placement fit probe,
  the save-wide chain total and the pooled-role depth each have an oracle that fails
  without its fix.
- Counter: `tools/production_loc.py` removed 956 shipped reference lines through a
  `#[cfg(...)]`-contains-"test" test and counted 4 083 lines of test-only modules
  living under reference `src/`; both are fixed, each has a focused tool test, and
  the same corrected counter is applied to the pre-Stage-3 base `c38961f2f`
  (core 6 152) and to every commit since. Certified totals at the closeout commit:
  core **10 983** (C1 4 388, C2 5 863, telemetry 732) in 75 files; reference
  **65 417** in 193 files; combined **76 400**. The core subtotal is unchanged by
  the correction because `core/crates/*/src` has no `cfg` attribute.
- Memory: a new external instrumentation target
  (`core/crates/layerfs-storage/examples/memory_ledger.rs`) reports phase-local heap
  for six real product phases. At the E1b shape (24 leaves × 100 rows = 2 400 values)
  the pooled save peaks at 3 450 007 bytes of phase-local heap while the ordered set
  (57 600 bytes live), both codec workspaces (3 MiB) and SQLite work are live, and
  charges 81 482 710 bytes over 9 977 allocations; the cold first leaf charges
  3 472 168 over 464. Lifetime RSS for the example process is 16 777 216 bytes
  (`/usr/bin/time -l`), which is a lifetime number and not a phase number.
  `ru_maxrss` is reported `null` with its reason. Nulls and coverage are tabulated
  in the W7 evidence README.
- Verification actually run (each command individually, `--locked`,
  `LAYERFS_CONSTRUCTION_WORKERS=1`): the workspace suite at the closeout commit —
  43 targets, 263 tests, 0 failed, 93.4 s; `clippy --all-targets -D warnings`,
  `fmt --all --check`, `core/tools/check_product_boundary.py`, both tool test
  suites and `git diff --check` — exit 0. `tools/preflight.sh` was not run and no
  aggregate gate was added. Two commands exceed the 15 s performance-selection
  budget and are declared as verification runs in the evidence: `edit_reference`
  (55.5 s, nine sealed reference cases) and the workspace suite (93.4 s). No
  performance or memory gate is claimed from them.
- Open, with the owner: escalation E1 (a repeated-sample matched campaign; the
  verification contract allows one sample per case unless an owner-approved campaign
  says otherwise) and E2 (the pooled lane assignment against the design table —
  value groups in v6 with pooled leaf records in the v1 ordinary lane, and v5
  refused by scope). Both are recorded in the closeout report; neither blocks the
  packets that do not depend on them.
- No durability, release, tag or issue-state claim follows from this entry; both
  issues close only when their own gate rows are PASS or owner-waived.

### L34 — 2026-09-20: #190 retained-history archival audit and harness attribution preparation

Research only; no new performance or verification sample, no product change,
no release claim. The [campaign synthesis](../../0.1.7/evidence/stage-6-history-190-20260919T225614Z/SYNTHESIS.md)
links all five squad reports, raw copies, protocol, check logs and independent
review. Source pin: `9f35c49ad62956f131dc2676787f99d69659686e` with a declared
harness-only worktree diff. Unrelated architecture documentation edits were
preserved. No commit or push was made; no per-commit production LOC figure is
claimed, and production source diff remains empty.

Archived core raw timing/trace/phases were retained byte-for-byte, with hashes;
they lack an original binary/source identity receipt and are not promoted to
fresh or matched evidence. Existing trace counters yield exact nanoseconds:

```text
32,566,067,669 = 23,520,347,667 build/update
               + 7,769,904,041 save
               + 1,115,152,085 content
               +    65,188,166 harness input
               +     2,469,708 Store create
               +    93,006,002 unassigned state residual
```

Arithmetic residual is zero only with the explicit unassigned bucket; internal
tree/save mechanism attribution remains unmeasured. Raw root is
44,831,509,917 ns, correcting a 1,000,000-ns handoff transcription error.
Root minus children = 12,265,442,248 ns; archived invocation = 44,999,165,042 ns.
Do not interpret the prior invocation as a new measured command or a cold claim.

S4 retained original v0.1.6 raw receipts and independently authenticated their
custody. Exact Commit sum is 11,370,679,212 ns; the unmatched descriptive excess
is `32,566,067,669 - 11,370,679,212 = 21,195,388,457 ns`.
Historical source/product seals are `8308cd8e628a97cd8b7d17d184a8f69ff5f212d21b0646d84913a6df5d444e9a`
and `970964e9af43a8bf57f0d7bec70736a94171f7beb62fc3378ea5cc4797500ebd`.
Legacy Commit includes content and namespace work; wholesale exclusion of those
phases is falsified, but differing nested boundaries prevent causal subtraction.
The source call graph does not establish an unconditional whole-base traversal.
Historical worker counts and matched cache state remain unknown.

Harness instrumentation reuses public timed filesystem APIs, with opt-in
`LAYERFS_HISTORY_PHASES=1`, plus public per-state counters. The analyzer command is:

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-20260919T225614Z/squad-s1/analyze_spans.py --self-test
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-20260919T225614Z/squad-s1/analyze_spans.py docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-20260919T225614Z/squad-s1/archive-original
```

**Non-passing/unrun lines:** fresh stride10 and stride3 diagnostics NOT_RUN pending
an explicit complete-command ceiling: handoff says 15/25 s applies, earlier lane
ruling lifts the generic limit but requires an undeclared prospective ceiling.
Proposed 120/240 s diagnostic ceilings have not been authorized; elapsed waiting
is not approval. Per-run wall is null because no run started. GROUP_LEVEL and
true worker/index matched arms are NOT_RUN because no equivalent public lever
exists without prohibited product changes. Charged transaction bytes and codec
CPU attribution are unavailable. Stride1 is NOT_RUN/outside selected scope.
Historical cache is uncontrolled; no cold or admission comparison is eligible.

Checks were serial under core O_EXCL and legacy measurement flocks. Harness
locked release build and 116 tests pass; Python history checks, product boundary
guard/self-tests and attribution self-test pass. Warning-denying Clippy and
harness formatting fail on existing statements/styles; original and final
check logs remain in the campaign, including the reviewer-requested harness
temporary-lifetime correction and its repeated checks. Full core Cargo checks
were not run because product source was not edited. No CI or preflight ran.
The ledger does not close #190: keep the tripwire and obtain fresh detailed
evidence before proposing a separately authorized product fix.

### L35 — 2026-09-20: #190 bounded parent lookup batching/reuse, confirmed on stride3

The owner subsequently requested execution of the five-step optimization plan,
including targeted product changes, subagents, issue updates and LOC reporting.
The [campaign report](../../0.1.7/evidence/stage-6-history-190-opt-20260919T232858Z/README.md)
and [independent review](../../0.1.7/evidence/stage-6-history-190-opt-20260919T232858Z/review/REVIEW.md)
retain source/custody, every state, non-passing line and rejected attempt. This
entry is diagnostic evidence, not release admission or historical receipt promotion.

Only product `core/crates/layerfs-content/src/filesystem/update.rs` changes: bounded
parent `lookup_many` calls, bounded final-window authenticated-record reuse, and
batched omitted-metadata reads. Original reducer insertion order, format, validation,
worker count and resource ceilings remain. Architecture and external tests accompany
the source. First candidate interleaved value insertion and regressed eight quota
cells; it was rejected before performance. Final candidate exactly reproduces all
56 baseline quota outcomes. No budget increase was used to repair that failure.

**Production LOC: 85,533 -> 85,582 (delta +49).** Reference65,417 unchanged;
core20,116 ->20,165 (+49), content12,512 ->12,561. Same production-only
`tools/production_loc.py --json` method and version, runtime SQL included, inline
legacy tests/test/docs/harness/tooling excluded. Exact before/final source manifests
and independent recount retained. This is a worktree comparison; no commit/push.

Source HEAD `9f35c49ad62956f131dc2676787f99d69659686e`, declared dirty worktree.
Baseline binary SHA256 `c5826d20f217a73ed4d814b82c3f8d8cdcaaa7af94ead18c0291918c88d4a855`;
candidate `f3afbe222248aa1040004dd09095b63cd94dc6a8f919f48c18a99f3ce42b916e`.
Harness/dependency identities match; product manifests differ only at update.rs.
Manifest `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`,
tip `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`. All behavior switches unset,
`LAYERFS_CONSTRUCTION_WORKERS=1`, `LAYERFS_HISTORY_PHASES=1`.

| Selection | Baseline operation ns | Candidate operation ns | Candidate minus baseline ns | Provider waves before -> after | Objects before -> after |
| --- | ---: | ---: | ---: | ---: | ---: |
| Stride10 | 47,161,768,126 | 27,386,866,665 | −19,774,901,461 | 110,715 ->66,616 | 117,533 ->74,279 |
| Stride3 | 125,277,281,254 | 79,607,320,336 | −45,669,960,918 | 210,380 ->111,853 | 230,323 ->135,296 |

All 17/53 state roots match. All non-provider filesystem work counters and save
decision/transaction counts match; final database hashes are identical across each
pair. Directory work and previously unassigned post-directory work shrink; residual
5,210,486,416 /15,511,671,619 ns remains named rather than assigned by inference.
Read elapsed overlaps filesystem phases and is not separately added to totals.

One sample per arm/case, order baseline10/baseline3/candidate10/candidate3; independent
verification after each arm's performance collection. Complete performance command
walls66,657,423,500 /146,622,418,084 /40,871,159,292 /99,357,294,167 ns fit prospective
diagnostic120/240 s ceilings. Existing corpus preparation and incremental locked
builds reused; no prepared Store, no cold claim. Measurement locks serialized all
resource-sensitive work. Two preflight CPU-idle deferrals are not samples; all
observations retained. Desktop activity and uncontrolled intra-chain/OS cache remain.

Reproduction commands are retained per-run; the coordinator was invoked as:

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-opt-20260919T232858Z/collect_diagnostic.py baseline history-stride10
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-opt-20260919T232858Z/collect_diagnostic.py baseline history-stride3
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-opt-20260919T232858Z/collect_diagnostic.py candidate history-stride10
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-opt-20260919T232858Z/collect_diagnostic.py candidate history-stride3
```

Each also had one `--mode verify` invocation using the same sealed binary/Store.
Existing directories refuse recollection; a future campaign requires new outputs.
`analyze_results.py` reproduces derived numbers from retained raw files.

**Non-passing lines:** both stride3 verification-work rows are TARGET_MISS:
29,241,424,750 /29,978,286,084 ns against20,000,000,000 ns. Stride10 rows8,124,985,000 /
8,000,840,000 ns meet10 s. Samples compare1,083 /3,377 paths per arm, zero disagreements;
not exhaustive read-back. All verification commands fit60 s hard wall. O3 pins are
missing: all performance traces retain INCOMPLETE. Cache/performance admission is
INELIGIBLE. Baseline stride10 allocated49,688,576 B fails the49,344,512 B ceiling;
candidate49,192,960 B passes. Stride3 baseline/candidate62,402,560 /62,103,552 B pass
64,024,576 B. Apparent49,053,696 /61,767,680 B and full contents are unchanged, so
allocation differences are not attributed as product savings. Historical tripwire
remains open; this pair does not rewrite the original32.566-second observation.

Final core490 tests, example targets, warning-denying Clippy, fmt, boundary guard
and self-tests pass; harness117 tests and release builds pass. Harness Clippy still
fails at22 existing statements and harness formatting fails. Initial missing-docs
compile failure, new-test formatting failure, baseline structural red test and old
107-wave materialization expectation are retained; final materialization pins104
waves/300objects after the removed three-page reread, preserving its golden root.
No CI/preflight ran. Steps and rejection were posted to #190. Subtree summaries and
save streaming were considered and deferred: their causal costs are not isolated,
and measured handoff copying is insufficient to explain the remaining save time.


## L36 — #190 pooled physical-group reuse (2026-09-20)

Status: Research; diagnostic evidence, not release admission. Owner directed continuation after PR#194. Evidence: [pooled-read report](../../0.1.7/evidence/stage-6-history-190-pool-20260920T010152Z/README.md), [raw-derived results](../../0.1.7/evidence/stage-6-history-190-pool-20260920T010152Z/results.json), [protocol](../../0.1.7/evidence/stage-6-history-190-pool-20260920T010152Z/PROTOCOL.md), [independent review](../../0.1.7/evidence/stage-6-history-190-pool-20260920T010152Z/review/REVIEW.md).

Base9d82685f39460fabec6c810184d87adcccd53dc5 plus pinned instrumentation. Baseline binarya27b740fc7e92b61a600c9764b46ed3522c3ed229466c077daa2bad441b0f734; candidate0cfa938be11a61625da8fd4ef4306657860d6e9e548986983b9f9b3e34b375f8. Identical harness/dependency maps; only pool/read.rs and delta/read.rs differ between measured arms. Corpus manifest03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271 and tipb0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed retained. Reproduce with evidence/collect.py baseline|candidate history-stride10|history-stride3 [--mode verify]; each receipt carries absolute command, identities/environment, wall and limits. Source patches plus companion new-file patches preserve both measured arms.

One sample/arm/selection, orderbaseline10/candidate10/baseline3/candidate3, fresh Store/output, one construction worker, same detailed harness. Shared locks and >=70%CPU-idle/no-named-competitor preflight; desktop activity and OS/intra-chain cache uncontrolled, so admission INELIGIBLE. Ordinary gate budgets unchanged; prospective diagnostic120/240s and separate60s verification hard limits met. Complete perf command walls33,741,919,208→30,502,824,583ns /74,713,341,541→61,657,848,334ns.

Operation19,660,740,792→16,268,082,127ns (delta−3,392,658,665ns;17.256006276124044%) and56,736,225,583→44,591,815,082ns (delta−12,144,410,501ns;21.405037744771757%). Formula100*(baseline−candidate)/baseline. Nested provider9,856,060,754→6,648,561,878ns /37,227,856,889→25,319,737,028ns; outside-provider partition closes exactly, codec-only CPU remains NOT_MEASURED. Pooled physical decompressions55,676→2,723 /248,690→20,959; decoded bytes2,522,862,738→123,780,247 /10,302,425,176→877,790,656. Ordinary decodes513→1,163 /3,892→5,285 is the measured bounded-cache eviction cost. Record calls, requests, chains, value decoding and BLOB acquisitions remain identical.

All70roots, save decisions and paired Store bytes match; actual hashes independently rechecked. Verification work6,185,391,917→4,241,717,250ns (10s PASS both) and20,766,917,666→14,987,183,125ns (20s baseline TARGET_MISS, candidate PASS). Samples1,083/3,377paths per arm, zero mismatch/missing/unexpected; not exhaustive. Allocated49,192,960B both17-state arms and62,144,512→61,775,872B53-state arms pass49,344,512/64,024,576B ceilings. Apparent49,053,696/61,767,680B and Store bytes unchanged; allocated difference is not a product storage gain.

Allfour history O3 counter gates remain INCOMPLETE; cold/performance admission INELIGIBLE; historical time tripwire unresolved. No stride1 or signature/reference/subtree/streaming optimization. Initial test fixture FromSql compile error, collector import failure, lock-acquisition deferrals/stale empty private lock recovery and incorrect fmt command are retained; no performance sample was repeated. Final493core tests, examples, core Clippy/fmt, boundaryguard6self-tests and117harness tests PASS. Harness Clippy22inherited diagnostics and formattingFAIL remain; formatting includes new telemetry tuples. No CI/preflight/third-party changes.

Production LOC85582→85723(+141): reference65417unchanged; core20165→20306. Instrumentation+85 and isolated reuse+56. Same production counter, runtime SQL included; tests/harness/docs/tools excluded. Exact commit snapshots must be recounted if published. Large rawStores/binaries remain local with explicit custody manifest.


## L37 — #190 catalogue statement reuse: bounded ROI round (2026-09-20)

Status: Research; diagnostic evidence, not release admission. Owner authorized one narrow attribution step and at most one justified local change. **Retain the catalogue helper change and stop this optimization round.** [Report](../../0.1.7/evidence/stage-6-history-190-roi-20260920T014227Z/README.md), [protocol](../../0.1.7/evidence/stage-6-history-190-roi-20260920T014227Z/PROTOCOL.md), [treatment](../../0.1.7/evidence/stage-6-history-190-roi-20260920T014227Z/TREATMENT.md), [results](../../0.1.7/evidence/stage-6-history-190-roi-20260920T014227Z/results.json), [independent review](../../0.1.7/evidence/stage-6-history-190-roi-20260920T014227Z/review/REVIEW.md).

Base 81f4f1fefcd5f600742fb8b0fe4c0502354db09d. Immutable baseline executable reused from PR #195, SHA 0cfa938be11a61625da8fd4ef4306657860d6e9e548986983b9f9b3e34b375f8; original build provenance preserved, all compilation hashes match the base. Candidate SHA 82fb535ddc74eb84aebfaac98d90058600331eb52ca8a8feffc5341b222a9d11. Harness and dependency maps identical; only sqlite/pool.rs differs. Corpus manifest 03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271 and tip b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed unchanged.

Attribution used one native `sample` run, interval 5 ms, exact child PID, existing binary, no new product telemetry/build. Main-thread 7,307 stack samples; provider subset 1,759. Catalogue disjoint samples: preparation 204, finalization/drop 12, reset 87, execution 312, row decode 5, other 4. The 216 preparation/finalization observations selected the target; they are not exact CPU/elapsed savings. Reset remains necessary; finalization moves to eviction/closure. Profile operation 24,639,852,787 ns and complete collector wall 42,415,136,209 ns are separate diagnostics, never included in the matched speed comparison. Profile Store hash equals the normal stride10 pair; separate profile verification NOT_RUN.

One change: group_for uses existing bounded prepare_cached, same SQL/parameters/current rows/errors, unchanged default 16-entry cache. No data cache, wider value lifetime, pack query, policy, worker, codec or format changes. Public authorizer test measures five preparations for five baseline queries versus one for the candidate's common five; a sixth candidate query sees an insertion after absence without recompilation. Updated row, parameter binding, deletion and invalid/range errors remain correct. Whole-history preparation count and exact SQL CPU are NOT_MEASURED.

One unprofiled sample per arm/case, baseline10/candidate10/baseline3/candidate3. Operation 23,491,957,417→22,180,444,124 ns (delta −1,311,513,293; 5.582818279974068%) and 64,870,176,420→59,039,478,665 ns (delta −5,830,697,755; 8.988256355662305%), formula 100*(baseline−candidate)/baseline. Provider 9,596,396,756→8,437,296,469 ns /36,676,151,222→31,155,004,681 ns is nested. Complete wall **regresses** 39,267,932,708→39,909,975,875 ns (+642,043,167) on stride10; improves 88,637,897,000→82,042,953,250 ns on stride3. Outside-operation stride10 complete wall grows by 1,953,556,460 ns; not all of this is corpus work. No end-to-end stride10 win or repeatability claim.

Invocation CPU user+system 33,510,728,000→32,304,434,000 ns /77,940,045,000→71,847,405,000 ns. Lifetime peak RSS 239,861,760→251,953,152 B /249,135,104→250,085,376 B; not incremental phase memory or a claimed memory reduction. All 70 roots, save decisions and provider work counters except elapsed match. Actual hashes of five Stores and both binaries independently verified; both normal Store pairs are byte-identical.

Verification work 6,237,682,875→5,489,793,375 ns against 10 s: both PASS. Stride3 22,686,258,458→18,912,983,625 ns against 20 s: baseline **TARGET_MISS**, candidate PASS. Sampled 1,083/3,377 paths per arm; zero mismatch/missing/unexpected; not exhaustive. Allocated 49,180,672 B both stride10 arms and 62,402,560→62,103,552 B stride3 arms meet 49,344,512/64,024,576 B ceilings; unchanged apparent 49,053,696/61,767,680 B and paired bytes mean allocation variation is not an algorithmic saving.

Performance commands fit unchanged 120/240 s diagnostic ceilings; verifiers fit 60 s hard budget. Shared locks and quiet preflight used; candidate10 deferred for Cargo, baseline3 deferred at 66.7% idle, neither consumed a sample. Reviewer hash acquisition refused an empty private lock; root preserved it with both global flocks acquired and no owner/descriptor/alias. Origin UNKNOWN; no live run interrupted. Cache residency uncontrolled: admission **INELIGIBLE**. All four normal history O3 rows **INCOMPLETE**. Historical time tripwire remains unmet. No stride1, broad caching or further optimization. Every deferral/miss retained.

Final 494 core tests, 117 harness tests, core examples, Clippy/fmt, boundary guard and six self-tests PASS. Harness Clippy/fmt NOT_RUN again because byte-identical harness files match PR #195's retained 22 Clippy errors and formatting failures; these remain unresolved, not passes. No CI/preflight or third-party changes.

Production LOC: 85723→85722 (delta −1); reference 65417 unchanged, core 20306→20305. Attribution adds 0. Same production counter, runtime SQL included; tests/legacy inline tests/harness/docs/tools/generated outputs excluded. Exact first-parent/staged snapshot proof accompanies publication. Reproduce with evidence/profile_once.py baseline history-stride10 for labelled profiling; evidence/collect.py baseline|candidate history-stride10|history-stride3 [--mode verify] for normal rows; analyze_profile.py/analyze.py for raw arithmetic. Full commands, identities and limits are retained in receipts; large immutable payloads remain local under the custody manifest.

## L38 — #190 zero-count filter tested against the owner's one-second bar (2026-09-20)

Status: Research; rejected diagnostic candidate, no retained product change. The owner explicitly stated “1 second is good optimization, do not dismiss it.” This supersedes the initial 2-second investment screen prospectively. The old 1,699,077,376 ns whole-phase receipt and original rejection remain historical; they are not relabelled. [Report](../../0.1.7/evidence/stage-6-history-190-screen-20260920T024251Z/README.md), [revision](../../0.1.7/evidence/stage-6-history-190-screen-20260920T024251Z/ONE-SECOND-REVISION.md), [raw arithmetic](../../0.1.7/evidence/stage-6-history-190-screen-20260920T024251Z/results.json), [independent review](../../0.1.7/evidence/stage-6-history-190-screen-20260920T024251Z/review.md). PRs #194/#195/#196 remain intact.

Base ca13fb1709250be64711a9431146e1aceb390955; baseline binary 5372121b9bd7650896038c32c9f41a3f03d14903d3bbb5ac001f4d2ce3624f74, candidate 79b181ee2e9657b8a84069909d8a6f5754b1474ac2e9f3e3f560cc226e00eb33. Both include identical temporary zero-count timing; candidate filters New-row demands within existing batches. Only update.rs differs between arms, harness/dependency maps identical. Exact dirty-source patches and identity maps retained; no clean-seal admission claim. Corpus manifest 03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271, tip b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed, same 17 states. One worker, all eight behavioral switches unset, detailed phases enabled. Exact commands in receipts; reproduce using campaign/collect.py baseline|candidate history-stride10 [--mode verify] with new output campaign and matching seals; analyze.py re-derives append-only results/states.csv.

One performance sample per arm; existing phase-only baseline reused as evidence, never rerun. Operation 22,615,178,250→22,458,498,667ns, reduction 156,679,583ns: zero_count 53,181,962ns + outside zero_count 103,497,621ns = exact delta, residual 0. Target phase 1,699,077,376→1,645,895,414ns. **REJECT_BELOW_ONE_SECOND**, shortfall 843,320,417ns; observed variation outside the phase is not attributed to the filter. Whole command 42,162,170,750→41,570,775,750ns; CPU user + system 33,241,999,000→32,827,434,000ns; lifetime RSS 249,348,096→252,985,344B, largest state heap increment 75,509,110→75,508,823B. These are not phase RSS or causal memory claims. Provider waves 66,616→66,046, objects 74,279→73,709, canonical bytes 148,826,516→147,902,276; provider elapsed 8,572,820,583→8,522,085,111ns. Pooled physical/value/pack work is unchanged. Logical demand reduction does not establish larger physical-work or timing savings.

Both focused public-provider tests PASS: same roots/counts/metadata/removal, mandatory false-New rejection; normal batch 32 removes 4/9 pages in suffix/interior fixtures despite 133→4 demands. Separate identity-matched verification 5,557,719,750→5,240,016,750ns, both PASS against 10 s; full verification commands 5,571,945,292→5,284,788,083ns, within the 60 s hard budget. Both arms sampled 1,083 paths / 893 files / 5,619,947 bytes, with zero mismatches/missing/unexpected entries; not exhaustive. All 17 roots, save counters and Store bytes match; the reviewer independently rehashed both binaries and Stores. Store SHA ab64dab7aaed512ab93b7ccc46fd14f22e57791f642b39fdf2b94650a41a965f. Apparent 49,053,696B both; **allocated targets MISS both**49,369,088/49,647,616B versus49,344,512B, over 24,576/303,104B. Identical bytes do not erase misses.

Cold/performance admission INELIGIBLE (uncontrolled OS/intra-chain residency), both O3 rows INCOMPLETE, historical timer tripwire unresolved. Diagnostic 120 s performance / 60 s verification hard ceilings met; ordinary 15/25 s admission not claimed. Actual quiet preflights 83.66%/82.61% idle; one 67.72% deferral before child launch. Private empty-lock refusals for baseline verification/candidate performance consumed no sample; files and metadata retained after global-lock/no-owner/no-descriptor checks. Origin UNKNOWN; no interruption. Stride3 and full suite NOT_RUN for rejected candidate; stride1, broader cache, summaries and streaming NOT_RUN. Both locked release builds and focused tests PASS. Final product restored exactly to HEAD; previous 494 core / 117 harness tests and checks are historical, inherited harness Clippy/fmt failures remain unresolved. No CI/preflight/third-party changes.

Production LOC: **85722 → 85722 (delta 0)**; reference 65,417 / core 20,305 unchanged. Experimental +7 (+2 temporary span / +5 filter) and active candidate-specific test reverted; exact patches and test snapshots retained. Same tools/production_loc.py counter, runtime SQL included; tests, legacy inline test branches, harness, docs, tools and generated sources excluded; staged/first-parent proof accompanies the commit. Large binaries/Stores remain local with custody manifest. No further one-second target is established by this experiment; one-second optimizations remain welcome.

Subsequent owner ruling: small allocation overages are acceptable when accompanied by good time reduction. [Exact disposition](../../0.1.7/evidence/stage-6-history-190-screen-20260920T024251Z/ALLOCATION-RULING.md). Numeric target misses remain reported; storage is not an independent rejection reason. The candidate remains rejected for failing the revised one-second speed criterion. This conditional ruling does not waive cache/O3 qualification or authorize unlimited storage growth.

## L39 — #190 legacy-code review and next experiment ordering (2026-09-20)

Status: Research; source review only, no new measurement or product change. At the owner's request, three subagents compared historical root `crates/` with retained replacement `core/` at 4391f66d8f39fc4d179caf557176021fc6f2c700. [Synthesis](../../0.1.7/evidence/stage-6-history-190-legacy-review-20260920T031633Z/SYNTHESIS.md) links independent tree, storage and boundary/cross-review documents. Legacy production source remains identical to the timed 7fab1027a product; five later additions are examples/tests. Raw compiled identity ac729dfeb4ee923b0a42106a53d1c7de4cd4cfcb is preserved. Historical 17-row Commit sum 11,370,679,212 ns independently reproduced; no rerun. Latest phase-only baseline 22,615,178,250 ns is reused as existing evidence, never a new sample; difference 11,244,499,038 ns remains unmatched and causally unresolved.

Recommendation: one isolated live GROUP_LEVEL 19-versus-1 experiment next, NOT_RUN in this review. Payload level, workspace, workers, integrity/bounds and harness remain fixed. Two separate retained archival recompression logs measure 1,011,835,000 / 1,425,492,000 ns more codec CPU at level 19 for 262,222 fewer encoded bytes. They are ARCHIVAL_UNMATCHED, not current operation savings or allocation measurements. The owner's one-second criterion and conditional acceptance of small allocation overages make the tradeoff worth testing; no product default changes follow from those old logs.

Second direction: legacy demanded-group SQLite BLOB range reads versus current full-pack copies. Existing counters record 79,784 pooled acquisitions / 7,378,994,999 application bytes; not physical disk traffic. Core's full bounded-directory validation and absence of direct catalogue offsets/lengths prevent a trivial slice/cache-return shortcut. Existing rusqlite BLOB support is available, but fetch/parse attribution is required before larger parser work. Coordinator initially preferred this direction; STORAGE recommended codec first. BOUNDARIES preserved the disagreement; coordinator adopted codec-first after reviewing validation complexity. Neither direction has established a one-second current speedup.

Counter-evidence: validation directory pages read are zero in every retained stride10 state; entries examined 359 only on initial build. No demonstrated full-directory-scan target for summaries. Legacy dirty-directory processing is charged to Content, so its 342,355,542 ns Namespace cannot be compared to the entire current filesystem interval. Both products rewrite whole open-pack BLOBs. Negative-absence reuse and streaming remain deferred; exact subpath time NOT_MEASURED. Retained disjoint operation budget closes with residual zero, without claiming causal attribution. All previous O3/cache/historical-comparison gaps remain; successful PR #194/#195/#196 improvements and rejected PR #197 variant are unchanged.

No builds, tests, workload reads, performance or verification runs in this review. Proposed stride10/stride3 codec treatment and range-read treatment NOT_RUN. Documentation checks cover status banners, local links, JSON and exact arithmetic. Production LOC: **85722 → 85722 (delta 0)**; reference 65417 and core 20305 unchanged. Same production counter and exact first-parent/staged snapshots for publication; source scope/exclusions unchanged. No CI/preflight or third-party changes.

## L40 — #190 group compression level 1 live pair (2026-09-20)

Status: Research; diagnostic evidence, not release admission. Owner authorized the isolated experiment proposed in PR #198 and accepts a one-second improvement with small allocation overages for good time reduction. **Retain GROUP_LEVEL 1**, with final workspace validation recorded in the linked report. [Report](../../0.1.7/evidence/stage-6-history-190-group-20260920T032843Z/README.md), [protocol](../../0.1.7/evidence/stage-6-history-190-group-20260920T032843Z/PROTOCOL.md), [results](../../0.1.7/evidence/stage-6-history-190-group-20260920T032843Z/results.json), [canonical inventory](../../0.1.7/evidence/stage-6-history-190-group-20260920T032843Z/store-comparison.json), [independent review](../../0.1.7/evidence/stage-6-history-190-group-20260920T032843Z/REVIEW.md).

Base 605f6efc6a095a1b6335cbc5549dc0fda78ed9ab. Measured baseline SHA82fb535ddc74eb84aebfaac98d90058600331eb52ca8a8feffc5341b222a9d11; candidate SHA13bcfe86598d3c13f902d6ab7d0a15d24f9ed26bb2c67dadbfe1a34ea6108a2d. Both rebuilt locked; harness/dependency maps identical. Measured product diff exactly GROUP_LEVEL19→1. Payload3,16MiB encode workspace, worker count, integrity/window/frame limits, cache capacities and selection policy unchanged. Final source corrects historical comments and architecture; separate exact non-comment code comparison and final identity preserve measured custody rather than rewriting it.

One sample per case/arm, baseline10/candidate10/baseline3/candidate3. Stride10 operation22,506,420,003→20,914,489,418ns (reduction1,591,930,585;7.073228815545978%). Stride3 operation58,410,516,121→56,577,520,957ns (reduction1,832,995,164;3.138125265325286%). Store begin+accept+finish11,050,102,332→9,364,846,704ns /22,624,366,586→20,161,248,171ns: reductions1,685,255,628/2,463,118,415ns, offset by93,325,043/630,123,251ns more other operation work. Both arithmetic partitions close with residual0. Whole-invocation CPU32,955,231,000→30,396,308,000ns /71,778,254,000→69,653,420,000ns. Exclusive codec CPU NOT_MEASURED; do not substitute archival CPU. Single samples do not establish repeatability.

Complete command41,663,305,750→32,762,497,334ns /82,295,809,041→79,910,471,208ns. Untimed corpus-read differences7,336,564,411/578,822,457ns are not credited to compression. Lifetime peak RSS252,936,192→255,803,392B /254,623,744→254,066,688B; not phase-incremental memory or a claimed memory improvement. Exact largest-state heap increments and phase/counter details are retained in results.json.

Stride10 apparent49,053,696→49,324,032B (+270,336), allocated49,651,712→50,249,728B (+598,016). **Both numeric allocation targets miss**49,344,512B by307,200/905,216B. Owner's conditional time/space ruling permits the small overage; raw misses remain reported. Stride3 apparent61,767,680→62,152,704B (+385,024), allocated62,783,488→62,152,704B (−630,784), both below64,024,576B. Allocation variation is not a compression-size gain. Pack-body bytes45,035,732→45,297,954 /56,192,535→56,590,252 (+262,222/+397,717), pack counts255/437 unchanged.

All70roots and complete sorted ObjectId/role/canonical-length inventories match. Objects/canonical bytes52,032/380,921,328 and72,560/589,916,570 per arm; all quick_check resultsok. Physical Store hashes intentionally differ. Reviewer independently rehashed two measured binaries/four Stores and recomputed/directly compared all canonical tuples. Delta-selection counters and logical provider work match; pooled pack bytes increase606,776,450/1,706,142,667B, provider elapsed increases52,050,079/499,991,335ns. These are application bytes, not disk I/O. Save commits1,149→1,149 /1,470→1,471 under unchanged thresholds.

Separate identity-matched verifier work5,617,963,792→5,405,493,708ns (10s PASS both) and18,824,407,917→18,904,055,292ns (20s PASS both). Candidate stride3 verification is79,647,375ns slower. Each arm samples1,083/3,377paths, zero mismatch/missing/unexpected; not exhaustive. Complete verification commands5,633,108,834/5,449,594,167/18,847,427,041/18,948,330,625ns fit60s hard budget. All performance commands fit frozen120/240s diagnostic caps, not ordinary15/25s admission budgets. Cold/performance admission INELIGIBLE, all four O3 rows INCOMPLETE, historical v0.1.6 comparison unresolved.

Corpus manifest03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271 and tipb0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed; fixed17/53states, all8behavioral history switches unset, constructionworkers1, detailedphases1. Fresh Stores/output, existing immutable corpus, no cold/cache claim. Quiet preflight idle73.77/85.39/83.55/81.29%, no named resource competitors; desktop variation remains. Initial held-global-lock and empty-private-marker refusals consumed no performance sample. Files and metadata retained. Subsequent #192 coordination identified append+flock use of private O_EXCL markers; helper corrected and stale primary marker preserved/removed under ownership checks. Prior UNKNOWN receipts stay unchanged. Communication stopped at the owner's instruction after resolution; shared locks directly serialize remaining work. No live run interrupted, no sample repeated.

Both50-test focused codec/storage selections PASS. Final explicit core tests/examples/Clippy/fmt, boundary/self-tests, harness tests and final release build are reported in checks and the report; inherited unchanged harness Clippy/fmt failures remain unresolved rather than rerun or claimed passes. No new tests, telemetry, dependencies, third-party patches, CI/preflight, stride1, further levels or pack-range treatment. Reproduction: campaign collect.py baseline|candidate history-stride10|history-stride3 [--mode verify], with fresh outputs and matching source/binary seals; analyze.py results.json and post-verification compare_stores.py derive summaries. Exact commands/limits/identities are in receipts; large payloads remain local under the custody manifest.

Production LOC: **85722 → 85722 (delta 0)**; reference65417 and core20305 unchanged. One constant value changes; removed obsolete comments are not production LOC savings. Same counter/version/scope, runtime SQL included, tests including legacy inline branches/harness/docs/tools/generated excluded; exact first-parent/staged proof accompanies publication.

Final L40 validation: **494 core tests, 117 harness tests, 12 example targets, core Clippy/fmt, 122-file boundary guard and six guard self-tests PASS**. Final comment-corrected build SHA7dff19d828afe760a98ca614a9a99a6630bc94533347d78a467835ba7568a736 is recorded separately, not substituted into measured receipts. All code outside GROUP_LEVEL is executable-equivalent; source comments/architecture updated.

## L41 — #190 data-access continuation handoff (2026-09-20)

Status: Research; documentation-only continuation checkpoint. No new measurement, build, test or product change. The owner requested a current issue update and a prompt for the next agent to investigate differences in data-access patterns, reuse boundaries and execution pipeline. [Handoff prompt](../../0.1.7/issue190-data-access-handoff.md) preserves PR #194/#195/#196/#199, the rejected zero-count filter and the owner's one-second / small-allocation tradeoff rulings.

The handoff takes exact current numbers from L40's candidate receipts: stride10 operation 20,914,489,418 ns; provider 8,609,430,602 ns nested in filesystem 9,569,275,834 ns; Store 9,364,846,704 ns; content 1,607,234,583 ns; remaining child work 373,132,297 ns. Provider counts include 79,784 pooled pack fetches and 7,985,771,449 application bytes. These are neither fresh measurements nor an attribution of the 9,543,810,206 ns unmatched difference from historical Commit.

Next-stage order: attribute acquisition/copy/complete-directory parsing/reconstruction; consider one format-preserving selected-group read; review bounded within-operation reuse and pipeline costs as separate conditional directions. Preserve authentication, bounds, error/visibility behavior and the single-worker rule. No automatic cache growth, pipeline rewrite, codec repeat or subtree-summary work. New experiments remain NOT_RUN. The resolved global-flock/private-O_EXCL distinction and the owner's ban on further cross-task coordination messages are explicit; no new task or subagent was launched by this handoff work.

Documentation status/link/arithmetic checks and exact per-commit production LOC accounting accompany publication. Production LOC: **85722 → 85722 (delta 0)**; reference 65417 and core 20305 unchanged. Same counter/scope and exact first-parent/staged snapshots; no production source changes. All existing O3/cache/historical-comparison limitations remain open in #190.

## L42 — #190 read-path attribution and ordinal-ordered pooled-leaf catalogue resolution (2026-09-20)

Status: Research; diagnostic evidence, not release admission. **Retain** ordinal-ordered pooled-leaf resolution, with the workspace validation recorded in the linked report. [Report](../../0.1.7/evidence/stage-6-history-190-read-20260920T042617Z/README.md), [protocol](../../0.1.7/evidence/stage-6-history-190-read-20260920T042617Z/PROTOCOL.md), [treatment](../../0.1.7/evidence/stage-6-history-190-read-20260920T042617Z/TREATMENT.md), [results](../../0.1.7/evidence/stage-6-history-190-read-20260920T042617Z/results.json), [correctness](../../0.1.7/evidence/stage-6-history-190-read-20260920T042617Z/CORRECTNESS.md), [store comparison](../../0.1.7/evidence/stage-6-history-190-read-20260920T042617Z/store-comparison-v2.json), [custody](../../0.1.7/evidence/stage-6-history-190-read-20260920T042617Z/custody.json).

Base 9fe8eb290072d09594cde3ba67c3bbd1963d5e56, the continuation handoff on top of PR #199. Clean isolated worktree on branch codex/history-data-access; actual HEAD recorded per identity. Priority A was executed as written: attribute first, then one treatment. The pre-registered selected-group BLOB range read was **not** selected; the measured reason is recorded. Priority B's per-leaf pack-cache scope (a real 2.1-3.9 s target with a Store-write invalidation contract) and Priority C's pipeline remain **proposals**, unmeasured and unimplemented.

Attribution instrument: bounded aggregate counters, no per-object map, identical in both arms, carried as a **separate recorded patch** and not part of the retained change. Instrument v1 (baseline) charged the whole canonical row-resolution loop to `rebuild_ns`, making that field a container; the sample is retained and superseded by that named defect and is **not** the matched baseline. Instrument v2 (baseline2) splits it. Instrumented stride10 provider elapsed 9,019,067,220 ns: pack BLOB acquisition 3,928,606,618 (142,686 fetches, 12,080,963,142 application bytes copied), metadata value-group catalogue SQL 2,180,465,764 (278,927 statements), unattributed 1,305,940,532, locator SQL 666,844,674, pooled value materialise/authenticate/decode/convert 644,700,604, ordinary group decompression 177,929,427, control-area validation 47,134,250, ordinary record decode call 21,186,780, canonical leaf re-encode 17,180,051, physical pooled body rebuild 14,238,663, record framing 13,122,420, physical pooled body decode 1,717,437. Spans nest; the disjoint set closes against the provider elapsed. Ordinary-lane internal split, per-wave ceiling read, per-leaf identity hash, per-row value-cache lookup and the residual are named **NOT_MEASURED**. Pack bytes are application-buffer acquisition, not disk I/O.

Mechanism: `PoolReader::leaf_canonical_with_groups` memoised only the previous covering group and its comment claimed the leaf's rows are in ordinal order. They are ordered by **serial** (`decode_pooled_body` enforces strictly increasing serial), so the covering group changes about as often as the row does: 24.8 statements per leaf on stride10 and 31.8 on stride3, against 5.80 and 8.77 distinct covering groups per leaf, at 7,817 / 7,827 ns per statement. Treatment: visit the rows in ascending ordinal order and write each value back at its own row's index, so the same memo answers one statement per distinct group. Same statement, same validation, no new SQL, no cache, no lifetime, no format, no bound.

Measured, one sample per case/arm, baseline2 versus candidate2, identical instrumentation, fresh outputs, both under the shared global flocks. Stride10 operation 21,905,900,168→19,888,424,711 ns, **reduction 2,017,475,457 (9.209735%)**; catalogue statements 278,927→65,337 (exactly the arithmetic prediction); catalogue interval 2,180,465,764→584,740,322; provider 9,019,067,220→7,254,356,455; filesystem 10,008,188,499→8,228,260,706; Store begin+accept+finish 9,894,760,665→9,683,862,043; content 1,630,222,082→1,629,628,334; complete command 40,220,718,250→38,101,144,250; whole-invocation CPU 32,387,651,000→30,298,280,000. Stride3 operation 56,597,343,506→49,501,013,748 ns, **reduction 7,096,329,758 (12.538274%)**; statements 1,334,277→368,074; catalogue interval 10,443,893,588→3,315,711,491; provider 31,150,817,381→23,865,118,886; filesystem 32,620,645,790→25,336,780,793; complete command 80,483,422,875→73,127,405,000; CPU 69,718,698,000→62,567,150,000. Stride3 content +11,765,376 ns and Store begin+accept+finish +178,573,535 ns are reported as small regressions against the larger filesystem reduction. Operation and region figures are summed over all selected states including the first, which is what `phases-perf.operation_ns` measures and what L40 reports; an earlier revision of this campaign's derived JSON excluded state 1, moving the reductions by at most 4,543,959 ns, and is corrected in place rather than hidden.

Rejected treatment retained with its samples: one catalogue statement per leaf ordinal span (`candidate`, SHA 9958e0d1a09e2e1b7148b1f76b4be2f5cd84c620c9022c5028ded3df38d7abe0) gave stride10 19,607,302,749 ns (**−2,298,597,419**, larger) but stride3 58,481,684,581 ns (**+1,884,341,075 regression**), because a span statement costs 101,010 / 259,906 ns against 7,817 ns for a single-group statement: its cost tracks the width of the leaf's ordinal span, not the number of groups the leaf uses. It was replaced, not combined.

Equivalence: all 17 / 53 state roots match; canonical and value-group inventories match; the saved Stores are **byte-identical between the arms** in both cases (stride10 4af37932aa3391b12269de8130b9c66dc504f64ca78fc3e585f7afddabed8487 / 49,324,032 B apparent, 50,249,728 B allocated; stride3 f5c7ff5a6b4f0821aa9a21ac5250335c4c3fb889637a0c5345c5278caadc2a9e / 62,152,704 B, 62,152,704 B). Both stride10 and stride3 hashes equal the retained L40 level-1 candidate Stores exactly. The historical allocated targets 49,344,512 / 64,024,576 B are unchanged; stride10 misses by 905,216 B, inherited from L40 because this treatment changes no stored byte, and is not relabelled.

Separate identity-matched verification, one per performance identity: stride10 5,593,199,541→4,606,757,792 ns and stride3 19,864,253,708→16,126,274,334 ns, all four PASS against 10/20 s targets, 1,083 / 3,377 sampled paths per arm with zero mismatch/missing/unexpected and identical 2,415 / 8,739 group decodes; sampled, not exhaustive. All performance commands fit the frozen 120/240 s diagnostic caps (measured 38.1/36.3/80.5/73.1 s), not ordinary 15/25 s admission budgets. Cold/performance admission INELIGIBLE; all O3 pinned-counter rows INCOMPLETE; the unmatched historical v0.1.6 comparison remains unresolved. One stride3 candidate preflight was deferred at 66.73% CPU idle with no named competitor and consumed no sample. Corpus manifest 03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271, tip b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed, all eight behavioral switches unset, one construction worker.

Retained validation: **494 core tests, 117 harness tests, 12 example targets, core Clippy/fmt, 122-file boundary guard and six guard self-tests PASS**. The inherited unchanged harness Clippy/format failures are not rerun and are not claimed as passes. No CI, no retired preflight, no third-party change, no stride1, no level sweep, no range-read implementation, no new cache, no worker change. The retained source differs from the measured candidate2 binary only by the recorded instrumentation patch. Reproduction: campaign `collect_diagnostic.py ARM CASE [--mode verify]` with fresh outputs; `analyze.py`, `write_results.py` and post-verification `compare_stores.py` read retained receipts only. Large binaries and Stores remain local under the campaign custody manifest.

Production LOC: **85722 → 85725 (delta +3)**; reference 65417 unchanged and core 20305 → 20308. Same counter/version/scope, runtime SQL included, tests including legacy inline branches/harness/docs/tools/generated excluded; exact first-parent/staged proof accompanies publication. An independent `rederive.py` recomputes every headline figure and every identity/custody hash from the raw receipts without importing the analysis scripts; its retained output is `rederive-output.txt` and its failure list is empty.

## L43 — #190 admission-qualification handoff (2026-09-20)

Status: Research; documentation-only continuation checkpoint. No new measurement, build, test or product change. The owner asked why admission remains INELIGIBLE and the O3 rows INCOMPLETE, and for a handoff prompt to resolve them on top of the retained read-path work. [Handoff prompt](../../0.1.7/issue190-qualification-handoff.md); the diagnosis is also recorded on #190.

Finding 1, O3. All four retained performance traces carry `g1.o3-pinned-counters = INCOMPLETE|no pinned counters for this case|at least one O3 constant pinned for every admission case`, so the trace-derived row status is INCOMPLETE (`g1.o1-chain-complete` PASS, `g4.swaps` PASS). `src/main.rs:563` defines O3 as the pinned counters in `tests/golden/expected.tsv`; `src/workload/expected.rs:157-166` emits `Gate::incomplete` for a case with none; the table pins 217 case ids and `src/families/history.rs:1-9` places the `history.*` group deliberately outside the 217, `--smoke` and `--lane full`. The gate therefore reports that no frozen oracle exists for these rows, not drift. Pins are made only by `shared/pin_expected.py` from a named baseline run's receipts, and the table's header records the hazard: the re-baseline at `795fb1a2f` moved `counter:pool.commits` 19→18 when a codec change crossed one fewer 4 MiB charge boundary. `runner.py:1464-1491` is stricter than the driver and would return FAIL, not INCOMPLETE, for a row that publishes counters with no pinned constant.

Finding 2, INELIGIBLE. It is not a gate: no INELIGIBLE record appears in any of the four traces. `src/gates.rs:24-26` defines it as a failed precondition, and the rows declare `CacheState::CreatedInSample`/`StoreState::CreatedInSample` (`src/registry.rs:53-60,108-115`). The only enforced cache contract is `shared/cold.py`, whose `applies` fires only for `init_namespace`/`namespace-100000`; no residency contract exists for `history.*`. The primitives exist — `shared/residency.py` and `src/support/instruments.rs` implement the same mincore/msync rules and `disk_read_bytes` exists beside them.

Three structural blockers, in order: (1) `runner.py:136-147` grants ADMISSION_MODE only to `lane == "full"` and `runner.py:60-65` keeps the three `history.*` lanes out of `full`, so a history invocation is always sampled and INCOMPLETE whatever else is fixed; (2) the complete-command budget is ≤15 s with declared exceptions to 25 s (`benchmark/AGENTS.md:78-84`) while measured history commands are 38.1 s and 73.1 s, and `benchmark_rules.md:716-718` contemplates but does not freeze a longer budget for large-history and endurance cases; (3) the cache stance is a modelling question, because the workload is a same-process write-then-read replay whose Store is created in-sample, so `CreatedInSample` may be the correct declaration and the open question is whether it can be verified with residency plus device reads rather than whether a cold claim can be manufactured.

Both gaps are independent of any product change and were present in every earlier round. The handoff preserves the five retained improvements (through PR #202, product total 85,725 LOC), forbids adding the history rows to `registry::cases()`/`FROZEN_CARDINALITY`, hand-editing a pin, inventing a cold claim, moving reads into setup, shrinking a selection to fit and promoting the 120/240 s diagnostic caps; it requires a written disposition of each gap, the owner decisions named, the lock and quiet-check protocol, ledger/issue/LOC accounting, and keeps #190 open. No new task, subagent or cross-task message; the historical v0.1.6 comparison remains a separate unresolved item and is not a qualification gate for these rows.

Production LOC: **85725 → 85725 (delta 0)**; reference 65417 and core 20308 unchanged. Same counter/scope and exact first-parent/staged snapshots; no production source changes.

## L44 — #190 admission qualification: the disposition of INELIGIBLE and INCOMPLETE (2026-09-20)

Status: Research; qualification disposition, not release admission. **No measurement command, no build and no test-suite run.** Every figure is a constant read from source or a re-derivation over L42's retained receipts and traces. [Report](../../0.1.7/evidence/stage-6-history-190-qual-20260920T052703Z/README.md), [disposition](../../0.1.7/evidence/stage-6-history-190-qual-20260920T052703Z/DISPOSITION.md), [cache stance](../../0.1.7/evidence/stage-6-history-190-qual-20260920T052703Z/CACHE-STANCE.md), [counter selection](../../0.1.7/evidence/stage-6-history-190-qual-20260920T052703Z/counter-selection.txt), [runner replay](../../0.1.7/evidence/stage-6-history-190-qual-20260920T052703Z/runner-verdict.txt), [provenance](../../0.1.7/evidence/stage-6-history-190-qual-20260920T052703Z/state-provenance.txt), [checks](../../0.1.7/evidence/stage-6-history-190-qual-20260920T052703Z/CHECKS.md), [custody](../../0.1.7/evidence/stage-6-history-190-qual-20260920T052703Z/custody.json). Base 437683aa054d4cbd618d9f7abba509cd88660f54 on branch codex/190-admission-qualification, isolated worktree, clean.

**Both gaps are owner-blocked and they are one knot.** A history row cannot be PASS, so `shared/pin_expected.py::counters_of` refuses its receipt, so the pins that would let it pass cannot be generated. Owner decisions required, in DISPOSITION §3: D1 whether the rows carry a frozen O3 oracle at all; D3 which bootstrap route creates a first pin set for a row that cannot pass before it is pinned (the 217 were pinned on a tree where the O3 gate did not yet exist); D2 which counter names may be frozen; D4 whether an admission claim exists and in which frozen budget class; D5 whether the corpus-axis cache stance is acceptable for an admission row.

O3, measured. `runner.re_derive_pins` returns `FAIL: <case> has no pinned O3 constant; the oracle cannot have been gated` for all four retained traces (replay of the runner's own function), because they publish 421 / 1,249 numeric counters — the "publishes no counters" escape does not apply, so INCOMPLETE is the lenient face of a check that reports FAIL and increments `disagreements`. `pin_expected.py` pins every integer counter of every status==PASS receipt with no selection mechanism (one excluded instrumentation key). Surface measured from the retained traces: 42 distinct names (19 row-level + 23 per-state), 410 / 1,238 counters in the performance invocation, **2 moved** under the retained legitimate optimization (both `*_ns` durations: history.root_ns 38,174,166,166→36,067,700,458 and history.children_ns 21,905,900,168→19,888,424,711), 8 names with 217-row precedent, 34 without; pinning everything adds 421 + 1,249 lines to a 1,981-line table, and history-stride1 projects ~3,630 more (NOT_MEASURED). Replayed against the composed status the generator pins 0 constants; against a counterfactual PASS it pins 421 / 1,249. One legitimate change bounds hazard, it does not prove invariance — the table header's own counterexample moved counter:pool.commits 19→18 at 795fb1a2f.

INELIGIBLE, corrected. `--verify full` exists and wins over the lane default, so "a history invocation is always sample mode" is wrong as written; the row even publishes `admission: admission` (CaseSpec::new defaults Admission::Admission) while being in no admission set and in no `--lane full` — declared admission, never admitted. The budget classifies a formula, not the wall: stride10 budgeted **20.348 s** (candidate2) / 22.367 s (baseline2) — inside a declared 25 s exception, outside the 15 s ordinary limit; stride3 **50.356 s** / 57.419 s — outside both. Raw walls 38.101/40.221/73.127/80.483 s; HISTORY_LANES ∩ DECLARED_EXCEPTIONS is empty. New blocker: the declared phases do not reconcile — `phases.compose` reports 18,103,407,414 ns of the 40,220,718,250 ns wall outside every declared phase for the baseline2 stride10 run (23,021,619,251 ns of 73,127,405,000 for candidate2 stride3) against a ~1.05/1.71 s tolerance, and the runner fails that reconciliation closed to INCOMPLETE. The span is published, not hidden: `history.corpus_read_ns`, "the harness's own corpus reading: inside the root, between the children, untimed", measured 16,161,397,833 / 22,943,708,667 ns (root_ns − children_ns 16,179,275,747 / 22,971,459,252). Declaring it as preparation moves the honest stride10 command to 36.3 s and stride3 to 73.0 s; folding it into the operation keeps the absolute reductions 2,017,475,457 / 7,096,329,758 ns but lowers their share from 9.209735 / 12.538274 % to ~5.60 / ~9.79 %.

Cache stance, answered in writing (the engineering item). `CreatedInSample` is correct for the Store axis and must not be verified by residency or device attestation: `resident_pages == 0` is false by construction for a Store written inside the timed region, de-warming between states would invalidate the operation's own work, and `gates::device_attestation` is defined only for cold/de-warmed claims (and has **no call site anywhere in the tree**; no operation records `disk_read_bytes`). The undeclared axis is the **corpus**: immutable, identity-pinned (manifest 03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271 re-verified), read wholly inside the invocation but outside every measured child, residency uncontrolled across runs and unmeasured. Measured fingerprint of the Store claim: all eight published per-state read counters are zero at state 1 and only at state 1, in all four traces, with cumulative returned canonical bytes byte-identical between arms (148,826,516 / 377,985,893). Also corrected: `shared/cold.py` belongs to the **reference** harness (`benchmark/fs-bench-pro/shared/cold.py`, applies only to init_namespace/namespace-100000); the core harness has no cold.py, and `shared/residency.py` is reached only by shared/experiments.py and runner.py:1660's self-check.

Checks. Harness shared Python suite `python3 -m unittest discover -s shared -p 'test_*.py'` **PASS 136 tests in 2.575 s** (no test in it builds or spawns the binary); `python3 shared/pin_expected.py --self-check` **PASS**; corpus manifest SHA256 re-verified; the three round scripts reproduce their JSON/TXT byte-identically on re-run. Core `cargo test/clippy/fmt` and `core/tools/check_product_boundary.py` are **NOT rerun** — no production, core or harness-Rust file changed in this commit — and are cited from L42's recorded PASS rather than claimed here. No runner invocation of a history lane was started: 38.1-80.5 s cannot fit the 15/25 s budgets, so it is recorded NOT_RUN with those measured walls. The unchanged harness Clippy/format failures remain unresolved and are not claimed as passes.

Production LOC: **85725 → 85725 (delta 0)**; reference 65417 and core 20308 unchanged. Documentation and evidence only; same counter (`tools/production_loc.py`) over the exact first-parent and final staged `crates` + `core/crates` snapshots, runtime SQL included, tests/legacy inline test branches/harness/docs/tools/generated excluded. #190 stays open: closing needs the owner's D1/D4/D5 rulings and the historical v0.1.6 comparison, which this round did not touch.

## L45 — #190 declare the corpus reading, and measure the corpus axis (2026-09-20)

Status: Research; diagnostic evidence, not release admission. [Report](../../0.1.7/evidence/stage-6-history-190-corpus-phase-20260920T054215Z/README.md), [analysis](../../0.1.7/evidence/stage-6-history-190-corpus-phase-20260920T054215Z/analysis.txt), [collector](../../0.1.7/evidence/stage-6-history-190-corpus-phase-20260920T054215Z/collect.py). Base 99028875c on branch codex/190-corpus-phase-declaration. Harness only: `src/support/phases.rs` +31/−1, `src/workload/history.rs` +81/−0, `src/ops/history.rs` +68/−4 (+180/−5 in total), no product line, workload untouched.

What changed. `phases::add_preparation(ns)` lets a driver attribute a measured span of harness **input assembly** to the preparation phase, so the four declared phases account for an invocation whose assembly happens between the measured children; `history.*` declares its corpus reading there. That declaration can only make the budgeted command larger. `workload::history::ReadProbe` adds a diagnostic: `mincore` over a fresh mapping of each corpus file **before this invocation first reads it**, once per distinct path. `ops::history` publishes the probe and the chain's `rusage` disk_read_bytes delta. Nothing fails on either; nothing is de-warmed; no cache is pooled; no selection shrunk.

Measured, one sample per case, retained source without the instrumentation patch, both global flocks held, quiet preflight passed, established 120/240 s diagnostic caps. The phase reconciliation now **PASSES** where it previously failed closed: retained baseline2 stride10 left 18,103,407,414 ns of a 40,220,718,250 ns wall outside every declared phase and candidate2 stride3 left 23,021,619,251 of 73,127,405,000; the unexplained remainder is now the child's own non-phase work, 25.8 ms and 46.4 ms inside a ~1 s tolerance. stride10: wall 37.613 s, preparation 17.766 s (window 0.178 + declared corpus 17.588), operation 19.738993418 s, verification 0.016031292 s, declared sum 37.521 s. stride3: wall 75.121 s, preparation 26.033 s (corpus 25.463), operation 48.976026830 s, declared 75.038 s. Budget, both: ordinary 15 s NOT_RUN and declared exception 25 s NOT_RUN. Operation is within 0.8 / 1.1 % of the retained 19,888,424,711 / 49,501,013,748 ns, which is run-to-run variation and not a new claim.

Corpus axis, measured rather than assumed. stride10 probed 45,338 files before first read: 489,820,959 B, 62,319 pages, **0 resident**; 603,807,744 B read from the device across the chain. stride3 probed 78,439 files: 850,636,296 B, 107,780 pages, **4,298 resident (3.988 %)**; 977,244,160 B from the device. Two instruments agree; the second run shows the leavings of the first are real but small, which is the uncontrolled-residency hazard quantified instead of asserted. Positive control: the Store this run wrote reads back as 3,007/3,011 resident pages through the same mincore method. **Not a cold claim**: nothing was invalidated, one run per selection establishes no distribution, and the probe covers the files the chain read (the prepare phase's oracle reads are inside the preparation window and unprobed).

Correction to L44's disposition, by measurement. The disposition projected the declared commands at 36.3 s / 73.0 s from the retained corpus readings and concluded stride10 would fit a declared 25 s exception. Measured, they are **37.521 s / 75.038 s** and both exceed 25 s; the projection's arithmetic was right and its input was wrong. D4 therefore has three shapes: a new frozen large-history class (37.5 / 75.0 s), a prepared identity-checked corpus input (projected ~20 / ~50 s from the measured 27.9 MB/s across 45,338 files, 388 µs per file — a per-file cost, not a bandwidth cost; NOT_MEASURED and the setup-reuse route AGENTS.md §2 requires), or diagnostic only. The corrected disposition is cross-linked from the report; the pinned-oracle analysis, the pin surface, the bootstrap circularity and the corpus-axis cache answer are unaffected.

Checks. `cargo +1.85.1 test --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked` **117 tests PASS** on the committed source; release build **PASS**, binary SHA256 190424195506d4d54b24d253b483986d9aadf542fe125ae61225d833c2102ab1. Harness Clippy: 21 warnings and 1 denied-lint error (`never_loop`, src/ops/history.rs:1855 at HEAD), **all inherited**, none in the added lines — recorded, not fixed, not claimed. Harness format: the three touched files are rustfmt-clean; **121 pre-existing format diffs in other files were reverted rather than swept into this commit**. One collector attempt was refused by the child for pre-creating its output directory, consumed no sample, and is retained at runs/refused-history-stride10-exit2/. No pin written, no value hand-edited, no cold claim, no budget class changed, no case re-run for a better number, no diagnostic cap promoted. #190 stays open.

Production LOC: **85725 → 85725 (delta 0)**; reference 65417 and core 20308 unchanged. Harness-code change only (+180/−5 lines in `core/benchmark/fs-bench-pro-storage-content/src`, a benchmark harness, excluded from production LOC); same counter and scope over the exact first-parent and staged `crates` + `core/crates` snapshots.

## L46 — #190 stride1 measured, and the depth term it exposes (2026-09-20)

Status: Research; diagnostic evidence, not release admission. [Report](../../0.1.7/evidence/stage-6-history-190-corpus-phase-20260920T054215Z/README.md), [stride1 axes](../../0.1.7/evidence/stage-6-history-190-corpus-phase-20260920T054215Z/stride1-analysis.txt), [depth term](../../0.1.7/evidence/stage-6-history-190-corpus-phase-20260920T054215Z/depth-term.txt), [trend](../../0.1.7/evidence/stage-6-history-190-corpus-phase-20260920T054215Z/stride-trend.txt). Same branch and round as L45; `history-stride1` is the one `history.*` row no campaign had ever sampled. Cap declared **before** the sample by the retained convention (120 s/17 states, 240 s/53 ≈ 4.5 s of complete command per state): 157 × 4.5 = 706.5 → **720 s**, never approached. The driver refuses per-state phase nodes above 53 states, so stride1 ran the ordinary recording: 157 named children measured, `filesystem`/`storage.accept_loop`/`harness.*` inside the state total unnamed. Stated, not worked around.

Measured. stride1 wall 199.025 s, preparation 52.664 s (corpus 51.142 s), **operation 146.178 s over 157 states**, declared sum 198.876 s, **reconciliation PASS**, budget NOT_RUN under both the ordinary 15 s and the declared 25 s limits. Per state mean 931.1 ms, median 698.5 ms, min 22.2 ms, max 8,952.0 ms. Corpus: 180,128 files probed before first read (1,946,359,193 B, 245,733 pages), **2,933 pages resident (1.19 %)**, 2,369,449,984 B read from the device. Per state across the three rows: stride10 1,161.1 ms, stride3 924.1 ms, stride1 931.1 ms.

**The depth term — the round's substantive finding.** Per-state elapsed correlates with changed bytes (+0.990 / +0.963 / +0.896 for stride10 / stride3 / stride1), but nanoseconds per changed byte rises down the chain (stride1 35.2→98.9, stride3 35.3→76.4, stride10 29.4→50.2, first decile to last). Deciles confound content with depth, so the load-bearing test matched changed volume: within one changed-bytes band, second-half states against first-half states cost **1.58/1.95/1.56/1.25×** (stride1), **1.71/2.31/1.50/1.66×** (stride3), 1.22× (stride10, one usable band), with ns per byte roughly doubling (stride1 7–10 MB band 54.5→102.4; stride3 46.4→105.1). Localised by sub-phase, both product paths carry it: stride3 `filesystem` 47.5→843.4 ms/state (17.8×) and `storage.accept_loop` 78.0→658.3 (8.4×); stride10 142.6→1,002.1 (7.0×) and 216.9→1,023.1 (4.7×); the harness `content` span grows least (4.6× / 3.3×). Bounds: within-run comparisons, one sample per row, 2–21 states per band, matched on changed bytes only (not path count, tree size or predecessor structure), and the term is measured, not attributed to a mechanism. It is the first direct evidence that the retained-history save path costs more at constant content as the chain grows, in the read/update path #190 optimises **and** in the storage accept loop. It also gives the pack-cache proposal a companion question: whether the depth term is pack rewriting (the 4 MiB charge boundary the golden header records) or predecessor/similarity work.

Store bytes, the one axis where v0.1.6 is directly comparable (a byte count of the same selection's content, not a phase-scoped time). core apparent/allocated against V016_ALLOCATED: stride10 49,324,032/50,249,728 vs 49,315,940/49,344,512 → **+905,216 above**; stride3 62,152,704/63,078,400 vs 64,000,100/64,024,576 → **−946,176 below**; stride1 80,969,728/84,541,440 vs 82,677,860/83,947,520 → **+593,920 above**. On **content** the core is below the historical figure on stride3 and stride1 (−1,847,396 / −1,708,132) and +8,092 above on stride10. **Allocation is not a precise statistic**: the retained campaign read stride3's allocated bytes as 62,152,704 and this round reads 63,078,400 for Stores that hash identically (f5c7ff5a…), i.e. 1.5 % on identical content — the same order as stride10's +905,216 (1.8 %) and larger than stride1's +593,920 (0.71 %). Recorded as a finding for owner ruling 7, not a re-baseline.

Retained source reproduces the measured candidate's output exactly. This round built the retained tree **without** the instrumentation patch and re-ran stride10/stride3: both Stores hash identically to the retained campaign's candidate2 Stores (stride10 4af37932aa3391b1…, stride3 f5c7ff5a6b4f0821…). The retained source's own **timing** remains the one gap in the evidence chain; the output half is now closed by measurement rather than by argument.

Checks unchanged from L45 (117 harness tests on this source, release build PASS, inherited Clippy/format findings recorded not fixed, both flocks held, quiet preflight). No pin written, no cold claim, no budget class changed, no cap enlarged after a miss, no case re-run for a better number, no selection shrunk. #190 stays open.

Production LOC: **85725 → 85725 (delta 0)**; reference 65417 and core 20308 unchanged. Evidence and analysis scripts only in this step.

## L47 — #190 scaling: more chunks cost more, and the depth term is per-unit cost (2026-09-20)

Status: Research; diagnostic evidence, not release admission. [Report](../../0.1.7/evidence/stage-6-history-190-corpus-phase-20260920T054215Z/README.md), [scaling](../../0.1.7/evidence/stage-6-history-190-corpus-phase-20260920T054215Z/scaling.txt), [depth term](../../0.1.7/evidence/stage-6-history-190-corpus-phase-20260920T054215Z/depth-term.txt). Same round as L45/L46, same branch.

More chunks for the same history. All three rows cover the same 157 checkpoints: 17 states 377.4 changed MB → 19.739 s (52,297,706 ns/MB); 53 states 713.7 MB → 48.976 s (68,625,861 ns/MB); 157 states 1,719.8 MB → 146.178 s (84,996,709 ns/MB). Operation grows ×7.41 against ×9.24 states, and **ns per changed MB rises 63 %** from the coarsest to the finest split: finer chunking writes more total content (a path changed ten times inside a stride-10 span is stored once, ten times at stride 1) and every state pays the depth term.

The depth term is per-unit cost, not per-byte work. At matched changed volume, early half against late half: provider waves per changed MB **fall** (139.4→111.3, 140.6→66.1, 250.7→112.4), chain edges per record call stay **flat** (0.33→0.36; 0.34→0.39; 0.22→0.33), records per leaf request are near-flat (6.01→7.26), while pack fetches per wave rise **4.0/4.8/4.4×**, KiB copied per fetch rise 2.2/1.3/1.3×, the decoded-group hit rate falls 0.967→0.898 / 0.955→0.822 / 0.996→0.980, and **pack bytes copied per changed MB rise 7.1× (12.1 M→86.2 M) / 2.9× / 2.8×**. Elasticity of filesystem ns/MB against ln(state): +8,755,936 (stride10), +13,108,101 (stride3). So the read path makes fewer, much larger calls as the chain grows: for one state whose diff is 8 MB it copies 86 MB of application bytes late against 12 MB early. Natural reading: a bounded pack/decoded cache against a growing working set (512 KiB / 4 MiB class bounds). **Stated as the reading the counters point at, not a measured cache curve** — only the decoded-group cache publishes a hit counter and it falls modestly. The deciding experiment is a treatment.

Consequences. (1) L42's Priority B pack/decoded-cache **scope** treatment (per-leaf reuse with a Store-write invalidation contract, 2.1–3.9 s target on stride10, reviewed and never implemented) is the best-motivated next product experiment, and its value should grow with history length; the standing ruling against automatic cache growth is respected by scope-and-invalidation, and anything needing more retained bytes is an owner decision — the core is already 0.71–1.84 % above the v0.1.6 allocation on two rows. (2) Chunk-selection policy is the largest multiplier in the table (19.7 s at 17 states against 146.2 s at 157, 4.6× the content) and is a product decision, not a code one. Bounds: one sample per row, within-run comparisons, 2–21 states per band, matched on changed bytes only, amplification from published counters, cache attribution inferred.

Production LOC: **85725 → 85725 (delta 0)**; reference 65417 and core 20308 unchanged. Analysis scripts and evidence only.

## L48 — #190 scaling handoff for the next agent (2026-09-20)

Status: Research; documentation-only continuation checkpoint. No new measurement, build or product change. [Handoff prompt](../../0.1.7/issue190-scaling-handoff.md); the scaling findings it carries are also recorded on #190. It succeeds L43's admission-qualification handoff on the same issue: the qualification question is now parked and owner-blocked (D1–D5 in [DISPOSITION](../../0.1.7/evidence/stage-6-history-190-qual-20260920T052703Z/DISPOSITION.md) §3), and the product question is the depth term.

The prompt hands over: the five retained improvements through #202 (product total 85,725 LOC, reference 65,417 / core 20,308) plus the harness-side corpus-phase declaration and diagnostics from PR #204; the three-row measured starting point (operation 19.739 / 48.976 / 146.178 s for 17 / 53 / 157 states, all three reconciling PASS, all diagnostic); the scaling finding (operation ×7.41 for ×9.24 states, ns per changed MB 52.3 M → 85.0 M, depth elasticity +8,755,936 / +13,108,101 ns/MB per e-fold, matched-volume late/early 1.2–2.3×, and the counter evidence that it is per-unit cost — fetches per wave 0.5–2.9 → 2.2–14.0, KiB per fetch 1.3–2.2×, decoded-group hit rate 0.967 → 0.898, chain edges per record flat, pack bytes copied per changed MB up to 12.1 M → 86.2 M); the two candidate explanations and the experiment that separates them (L42's Priority B pack/decoded-cache scope with a Store-write invalidation contract, 2.1–3.9 s target, reviewed and never implemented); the Store-byte comparison against the v0.1.6 targets and the 1.5 % allocation movement measured on identical content; the v0.1.6 scope correction (the historical 11,370,679,212 ns is `commit_workspace_session_with_status` at `storage_smoke.rs:737-738`, so the "9,543,810,206 ns unmatched difference" is a whole operation minus a commit — no demonstrated shortfall on the matched interval, and no percentage claimed); the retained-source output closure (Stores byte-identical to the measured candidate, 4af37932… / f5c7ff5a…, while its timing remains the open gap); the full measurement and resource protocol; and an explicit do-not list (no automatic cache growth, no new format, no worker change, no shrunk selection, no enlarged timeout, no hand-edited pin, no invented cold claim, no promoted diagnostic caps, no history row added to the 217, #190 stays open).

Production LOC: **85725 → 85725 (delta 0)**; reference 65417 and core 20308 unchanged. Documentation-only.

## L49 — #190 the depth term was a pack-cache scope term, and one scope change removes it (2026-09-20)

Status: Research; **diagnostic evidence, not release admission**. [Report](../../0.1.7/evidence/stage-6-history-190-scope-20260920T062421Z/README.md), [mechanism](../../0.1.7/evidence/stage-6-history-190-scope-20260920T062421Z/mechanism.txt), [pre-registration](../../0.1.7/evidence/stage-6-history-190-scope-20260920T062421Z/PREREGISTRATION.md), [counters](../../0.1.7/evidence/stage-6-history-190-scope-20260920T062421Z/counters.txt), [equivalence](../../0.1.7/evidence/stage-6-history-190-scope-20260920T062421Z/equivalence.txt), [sub-phases](../../0.1.7/evidence/stage-6-history-190-scope-20260920T062421Z/subphases.txt). Succeeds L48 on the same issue; the qualification question (D1–D5) is untouched and still owner-blocked.

**Mechanism, confirmed from counters that already existed.** `encoding/delta/read.rs::Resolver::resolve_charged` built a `PoolReader` once per resolved inode leaf, so its pack cache (4 MiB) and decoded value cache (512 KiB) died with every leaf, while the decoded-group cache beside it is operation-scoped and is consulted *after* the pack body is acquired. Read from the retained campaign's own traces and Stores: stride10 79,784 pack fetches over at most 255 stored packs, i.e. **≥ 79,529 (99.7 %) fetches re-reading a pack the same state had already read**, 7,985,771,449 bytes copied = **176.3× the whole pack space**, decoded-group hit rate 0.951; stride3 427,384 fetches over at most 437 packs, ≥ 426,947 (99.9 %) repeats, 25,105,269,671 bytes = **443.6×**, hit rate 0.916. Structural counters do not carry it: chain edges per record call settle at 0.28–0.39 from state 4 on, records per leaf rise 2.2×, bytes copied per leaf rise 54×. So the depth term is repeated acquisition of whole committed packs discarded at the per-leaf boundary, not a deeper delta chain. What the counters could not say — how close together the repeats are — was fixed as the treatment's question before the first sample.

**One treatment, pre-registered, no bound moved.** The pooled reader's lifetime becomes its calling operation's: `BodyCaches` bundles the two stored-body caches a reading caller owns, `ReadSession` owns one reader for the whole read operation (as it already owned the operation-scoped decoded-group cache), the save's own `pool_reader` — the one the pooled lane and the depth walk already shared and `write_pack` already releases on every pack write — serves `MutationOwner::read_batch`, `resolve_location` and the selection input, and `Store::read_batch` keeps a reader for its one wave. Bounds unchanged (4 MiB / 512 KiB, both still released wholesale at the bound), no retained-bytes growth, no instrumentation added, single construction worker untouched, no other cache changed. `PoolReadCounters::since` keeps each chain's reported work its own. Invalidation contract unchanged and stated in code: a pack at or below the ceiling a read was authorized under is immutable (a save creates packs above the baseline publication watermark and publishes only by advancing it), every pooled consult checks the pack against the *current* ceiling before the cache answers, the ceiling is re-read per wave, and a writing owner releases its pack cache on every pack write.

**Measured, matched arms, one sample each.** baseline = the retained tree at `c4f757514` reusing the campaign's own binary (`190424195506d4d5…`); candidate = that tree + this change (`c7d48057089470e8…`); harness source unmodified in both. Operation (sum of named children): stride10 **19.500 → 16.746 s (−2.754 s, −14.1 %)**, stride3 **48.813 → 37.066 s (−11.747 s, −24.1 %)**. Complete command 38.871 → 35.792 s and 74.980 → 62.355 s; preparation 19.252 → 17.102 and 26.056 → 25.172 s, which is the declared corpus reading and a cache-state artefact of the fixed run order (corpus pages resident before first read 0.00 % / 8.25 % / 3.99 % / 6.52 %), **not credited to the operation**. Localised by the product's own tree: `filesystem` (read/update) −2,909 ms stride10 and −11,712 ms stride3, `storage.accept_loop` +135 / −26 ms (noise), `content` +2 / −5 ms; per state it grows with depth (stride10 `filesystem` early 158.5 → 116.1, late 760.6 → 475.1 ms). Counter movement is the pre-registered signature: `pack_fetches` 79,784 → **250** (0.003×) and 427,384 → **2,104** (0.005×), `pack_bytes` 7.99 GB → **16.6 MB** and 25.11 GB → **80.2 MB**, `value_group_decodes` 0.517× / 0.498×, while `physical_record_calls`, `physical_group_decodes`, `physical_group_cache_hits`, `chain_edges` and `leaf_requests` are **bit-for-bit unchanged (1.000×)** — the same work, almost none of the same bytes copied twice. The 4 MiB bound does bind (26 / 65 packs fetched in the last state against 255 / 437 stored); the 512 KiB value cache only halves its decodes, recorded as measured rather than proposed, since widening it is retained-bytes growth and an owner decision.

**Equivalence, and the one row that is not clean.** Every state root and every state content counter equal between arms; both saved Stores hash to the **constants the retained campaign recorded** (stride10 `4af37932aa3391b1…`, stride3 `f5c7ff5a6b4f0821…`), with equal canonical/value-group inventories (52,032/1,737/48,925 and 72,560/4,225/65,528). The harness's allocated axis moves anyway on identical bytes (50,171,904 → 50,249,728; 62,152,704 → 63,160,320), reproducing L47's finding that allocation is a property of extents rather than content. **`candidate-history-stride10` reconciles INCOMPLETE**: 1,911,291,834 ns of the collector's wall lies outside the child's own clock (tolerance 965,842,212 ns) — 1.883 s against 0.036–0.072 s in the six sibling observations, with the child's own phases reconciling (28.6 ms unaccounted). It is reported as INCOMPLETE and **not credited**: it inflates the wall comparison, and retention rests on the operation, the `filesystem` sub-phase and the counter collapse, all inside the declared phases. A start-up probe (both binaries, four runs each, 35.8–42.2 ms) rules out first-execution cost; no second sample of the row was taken to chase it.

**Verified separately, per identity, and stated as sampled.** Each of the four measured runs was verified in the harness's own verification phase against the identity its own trace recorded: the invocation reads the state roots and Store path out of that run's measured trace, resolves a **declared sample** of 64 paths per state through the product's read path, and compares presence, kind, size and digest with the corpus oracle. All four: **0 mismatches, 0 missing, 0 unexpected** (stride10 1,083 of 101,477 path-states, 893 files, 5,619,947 B; stride3 3,377 of 306,861, 2,756 files, 18,936,332 B), every comparison counter identical between arms, both `g6.*` gates PASS, and all four inside the 60 s hard budget (4.681 / 2.911 / 16.293 / 9.712 s). The row it produces is `INCOMPLETE` by construction — it is a sample, not a full read-back — and is not a release admission. Secondary observation: the read-back shows the treatment again in a separate process against byte-identical Stores (4.681 → 2.911 s, 16.293 → 9.712 s).

**Retained**, against the pre-registered rule (≥ 1 s on stride10, stride3 non-regression, unchanged stored bytes). Checks on the committed tree: core `cargo test --locked` **494 passed / 0 failed** across 72 binaries, `clippy --all-targets` clean, `fmt --all --check` clean, `check_product_boundary.py` PASS (122 files) and self-tests OK (6 ran); harness `cargo test --locked` **117 passed / 0 failed** and release build PASS with the inherited `unused_mut` warning at `src/ops/history.rs:1525` recorded not fixed. Every build, test and performance command ran under both global flocks after the quiet preflight. stride1 was **not** sampled, so the scaling claim is not asserted here. No pin written, no budget class changed, no cap enlarged after a miss, no selection shrunk, no cold claim invented, no case re-run for a better number. #190 stays open.

Production LOC: **85725 → 85778 (delta +53)** for the treatment commit (`d57d6f9aa`); reference 65417 unchanged, core 20308 → 20361. The mechanism-evidence commit before it (`fcfacdce3`) is 85725 → 85725 (delta 0). Same counter (`tools/production_loc.py`) and scope over the exact first-parent and staged `crates` + `core/crates` snapshots, runtime SQL included, tests/harness/docs/tools/generated excluded.

## L50 — #190 what `storage.accept_loop` is made of, and why the write pattern is not yet a target (2026-09-20)

Status: Research; **diagnostic evidence, not release admission**. Harness-only change, no product line. [Report](../../0.1.7/evidence/stage-6-history-190-save-20260920T070711Z/README.md), [save attribution](../../0.1.7/evidence/stage-6-history-190-save-20260920T070711Z/save.txt), [synthetic sizing](../../0.1.7/evidence/stage-6-history-190-save-20260920T070711Z/append_cost.py), [equivalence](../../0.1.7/evidence/stage-6-history-190-save-20260920T070711Z/equivalence.txt), [harness patch](../../0.1.7/evidence/stage-6-history-190-save-20260920T070711Z/harness-save-counters.patch). Continues L49 on the same issue and branch.

**Why.** L49 removed the read path's pack-acquisition term and left the save flat (+135 / −26 ms), which makes `storage.accept_loop` the largest single sub-phase — 9.13 s of stride10's 16.7 s operation, 18.4 s of stride3's 37.1 s — and the only one with no attribution: L42 refused to treat it because compression, indexing and transaction work were not separated. The product already computes those figures in `SaveOutcome`; the harness published one of them.

**The change.** `ops/history.rs` (+152 lines, harness only, excluded from production LOC): the existing `SaveTotals` accumulator gains `pack_appends`, `commits`, `statements`, `presence_queries`, the `chain.*` and `pool.*` sub-counters and a `rows()` accessor, so each state publishes 29 `history.state.<n>.save.*` figures and the chain totals gain the same fields. No new measurement: every field is read from the `SaveOutcome` the product already returned. Format deviation recorded rather than swept in: the added code carries 9 rustfmt hunks (the file carried 20 at `c4f757514`); reformatting moves the built binary (embedded `panic!` locations) and would leave the receipts describing a source no longer in the tree, so the measured source is kept as measured.

**Measured, matched arms, identical harness source (seal `264e88c5…`), one sample each.** stride10 operation 19.638 → 16.908 s (**−2.730 s**, reproducing L49's −2.754 s under a different harness source) and stride3 48.638 → 36.810 s (−11.828 s), with `storage.accept_loop` +0.030 / −0.085 s — the save path is untouched by L49 and this round says why. Equivalence: every state root and content counter equal, both Stores hashing to the recorded constants (`4af37932…` / `f5c7ff5a…`), and **every `save.*` counter identical between arms** — the same work, acquired differently.

**Attribution.** Stride10 totals: 45,239 FULL preparations, 38,230 trials (38,163 choosing PREFIX), 107,628 chain objects read and 133.9 MB of encoded bytes read for trials, 48,925 new values, 53,355 reused, 44,331 statements, 45,791 pack BLOB rewrites, 1,149 commits, 255 packs. Per inserted object, early (states 2–6) against late (last 5): `accept_loop` 93.6 → 212.5 µs (2.27×), **chain objects 1.039 → 2.409 (2.32×)**, **chain edges 0.433 → 1.603 (3.70×)**, encoded bytes 1.8 → 2.8 KB, while FULL preparations 1.06×, appends 1.07×, statements 1.06×, commits 0.83×, new values 0.99×. So the save path's **depth term is delta-base acquisition** — the only per-object quantity that grows like the time, and the only one that reads anything.

**The write pattern, sized and rejected as a target for now.** 44,141 of 52,032 objects sit in groups of exactly one, because the `WholeFile` lane seals on every record by construction; each group placement rewrites the whole pack BLOB (`UPDATE object_packs SET data = ?2`), and `write_pack` charges the whole rewritten pack to the transaction's 4 MiB byte budget, forcing a commit every ~40 appends. The Store's own shape therefore implies 45,791 appends and **4.43 GB rewritten to store 45.3 MB — 97.8×**. A synthetic replay of that exact pattern (same pack ids, group counts, intermediate sizes, pragmas and commit cadence) on a scratch copy of the retained Store costs **1.03 s** (0.15 s in-memory rebuild, 0.88 s SQLite) — a bound on one pattern, not the product's cost, and around rather than clearly above the one-second bar.

**Decision.** No treatment. The two candidates are a ~1.03 s synthetic bound (whose fix touches the visibility and atomicity of the open tail) and an unsized depth term; choosing between them on this evidence is the mistake L42 declined to make. The next step is one more attribution layer **with time**: bounded aggregate spans inside the accept path (selection and chain acquisition / compression / placement and write / SQL), identical in both arms and kept separate from unprofiled samples.

**And the L49 reconciliation gap is explained, with a corrected probe.** A freshly created executable's **first** run costs 0.7–2.0 s on this machine before `main` (1,959.5 / 669.4 ms first against 34.8–38.9 ms afterwards), outside `phases::begin`; L49's probe missed it by running after the first execution. That accounts for L49's `INCOMPLETE` and both stride10 rows here (2.018 / 1.823 s), and for every stride3 row's ~0.04 s, since by then both binaries have run once. Both stride10 rows are reported `INCOMPLETE` with a measured, non-product cause; the operation comparison reproduces L49 within 24 ms. The L49 report carries an appended correction.

Checks: harness `cargo test --locked` **117 passed / 0 failed**, release build PASS with the inherited `unused_mut` warning recorded not fixed; harness format not clean and not claimed (20 pre-existing hunks plus 9 added); harness Clippy not re-run, its inherited set recorded; core checks not run because no product line changed. Every build and performance command ran under both global flocks after the quiet preflight; nothing was refused, deferred or repeated. No pin written, no budget class changed, no cap enlarged, no selection shrunk, no cold claim invented, no case re-run for a better number. #190 stays open.

Production LOC: **85778 → 85778 (delta 0)**; reference 65417 and core 20361 unchanged. Harness code only (`core/benchmark/fs-bench-pro-storage-content/src`), which is excluded from production LOC; same counter (`tools/production_loc.py`) and scope over the exact first-parent and staged `crates` + `core/crates` snapshots.

## L51 — #190 stride1 measured: the scaling curve after the treatment, and where its residual depth term lives (2026-09-20)

Status: Research; **diagnostic evidence, not release admission**. Same round and branch as L50, addendum to [its report](../../0.1.7/evidence/stage-6-history-190-save-20260920T070711Z/README.md) §10; [the curve](../../0.1.7/evidence/stage-6-history-190-save-20260920T070711Z/scaling.txt). No product line changed; the two sealed binaries of L50 were reused unchanged.

**Why.** L49 sampled stride10 and stride3 and left stride1 unsampled, so the finest end of the scaling curve — the end the whole finding is named for — was unmeasured after the treatment. Both arms were sampled with the retained campaign's own declared 720 s diagnostic cap reused unchanged (its stride1 run used 28 % of it).

**The curve, L47's units.** operation and ns per changed MB, pre-fix → post-fix: 17 versions 19.739 s / 52,297,706 → 16.908 s / 44,796,163 (**−14.3 %**); 53 versions 48.976 / 68,625,861 → 36.810 / 51,579,091 (**−24.8 %**); 157 versions 146.178 / 84,996,709 → **105.726 s / 61,475,883 (−27.7 %)**. Least-squares slope against ln(versions) **14,706,448 → 7,491,080 ns/MB per e-fold (−49 %)**; finest against coarsest **1.625× → 1.372×**. So the benefit grows with version count exactly as predicted, the curve is flattened by about half, and it is not flat: a changed byte still costs 1.37× at 157 versions what it costs at 17.

**What was not removed.** At matched changed volume the within-chain depth term barely moved at stride1: bands 4–7 / 7–10 / 10–14 / 14–22 MB are 1.58 / 1.95 / 1.56 / 1.25× before and 1.54 / 1.84 / 1.54 / 1.32× after (−6 % to +6 %), against −9…−16 % at stride3. Depth elasticity of the state total against ln(state) fell on all three rows — stride10 12,461,811 → 8,201,141; stride3 17,892,411 → 11,130,816; stride1 25,328,300 → 14,954,837 ns/MB per e-fold — so the treatment removed a large term that grows with depth in absolute terms and left the within-chain residual the bands isolate essentially intact.

**Where the residual is — and it is not a cache.** With the save counters published per state for the first time, per inserted object early → late at 157 versions: **chain objects read for trials 2.04 → 6.32 (3.09×)**, **chain edges walked 1.25 → 4.54 (3.62×)**, while prefix trials 1.10×, FULL preparations 1.00×, pack BLOB rewrites 1.05×, INSERT statements 0.98×, commits 0.86×. Chain totals at stride1: 97,788 objects written, 78,619 FULL preparations, 70,710 trials, **648,299 chain objects and 653.7 MB of encoded bytes read for trials** (stride10 read 107,628 objects and 133.9 MB for 4.6× less content), 84,473 pack appends, 1,797 commits. Per-state elapsed correlates **+0.947** with chain objects, +0.938 with changed bytes and only +0.444 with objects written. The residual depth term is the trial's acquire-and-compare: a prefix trial must reconstruct the base it compresses against, and at 157 versions the chains are genuinely longer. No cache scope or bound touches it.

**Equivalence and verification.** Every state root and content counter equal; both Stores hash to `1635cf7bbbabdc7f9e4af81ac9c6b6a88f45be35b4100dda0f52394c85dcf418` — the **retained campaign's own stride1 Store**, recomputed from its artifact and used as a cross-round constant, making stride1 the third row whose bytes are provably unchanged. Inventory: 97,788 objects, 75,947 ordinary groups, 9,381 value groups, 855 packs, 73,043,937 pack bytes, 75,404 singleton groups. Identity-matched verification on both rows inside the 60 s hard budget: **9,996 of 904,143 path-states sampled, 0 mismatches, 0 missing, 0 unexpected**, identical counters between arms, both `g6.*` gates PASS (53.982 s baseline / 31.657 s candidate).

**One correction to L50.** L50 §5 concluded that first-execution cost "accounts for every observation in the family"; the candidate's stride1 row shows **1,878.3 ms outside the child's clock on that binary's third execution**, so that is too strong. First-execution cost (measured 0.7–2.0 s) is one demonstrated cause and explains both stride10 gaps and L49's, but a second out-of-clock cost exists and is unidentified. Both stride1 rows reconcile PASS (tolerance wall/50 = 3.2 s against stride10's 0.97 s) and no operation figure is affected, since the whole effect is outside the child's clock. Recorded as open, not re-explained.

Bounds: one sample per case per arm; stride1 runs the ordinary recording (the driver refuses per-state phase nodes above 53 states), so its `filesystem` and `accept_loop` spans are inside the state total and the curve's stride1 point is a state-total figure while stride10/stride3 are phase figures — the pre-fix and post-fix points are computed the same way on each row, which is what makes the comparison valid; both rows are diagnostics with admission `INELIGIBLE` and budget `NOT_RUN`. No pin written, no budget class changed, no cap enlarged or lowered after a miss, no selection shrunk, no cold claim invented, no case re-run for a better number. #190 stays open.

Production LOC: **85778 → 85778 (delta 0)**; reference 65417 and core 20361 unchanged. Evidence and analysis scripts only.

## L52 — #190 correction: the save path's depth term is resolution work, and its growth is reuse verification (2026-09-20)

Status: Research; **correction to L50 and L51**, same branch, no new sample, no product or harness line changed. [Report §11](../../0.1.7/evidence/stage-6-history-190-save-20260920T070711Z/README.md).

L50 §3 and L51 describe `save.chain.objects` / `save.chain.edges` as bytes and records read "for trials". That label is wrong: `SaveOutcome.chain` is `MutationOwner::chain_total`, which accumulates **every** chain resolution in the operation — a selection acquisition (`encoding/delta/select.rs::acquire`) **and** a reuse or membership resolution (`cas/membership.rs::stored_canonical` → `MutationOwner::resolve_location`, which does `accumulate(&mut self.chain_total, self.chain)` at `cas/owner.rs`). Every occurrence that finds an existing row reconstructs that row's chain to compare the stored bytes with the offered bytes, and that work lands in the same counter.

Recomputed from L51's own stride1 sample, per inserted object early → late: resolution events from **reuse** 0.1121 → **1.0321 (9.21×)**; resolution events from **trials** 0.6773 → 0.7451 (1.10×); chain objects per resolution **event** 2.59 → 3.56 (1.37×); `save.chain.objects` 2.04 → 6.32 (3.09×). Per-state elapsed correlates +0.947 with `save.chain.objects`, +0.948 with `.edges`, +0.805 with `save.reused`, +0.409 with `save.delta.trials` and +0.444 with `inserted`. So the depth term is still resolution work and chains are still longer late, but the growth is dominated by the **number** of resolutions, nine tenths of it reuse verification rather than trials. L50's and L51's measurements are unchanged; their interpretation is corrected, and the next target is the resolution path with the reuse comparison as its largest depth-driven component — not the trial's acquire-and-compare.

Production LOC: **85778 → 85778 (delta 0)**; reference 65417 and core 20361 unchanged. Documentation only.

## L53 — #205 the save path attributed in time, and the one treatment the split named measured to zero (2026-09-20)

Status: Research; **the instrument, the split, and a negative treatment result**. Branch `codex/190-pooled-scope`, tip `7bf428c4a` plus this round's sub-split and probe commits. [Report](../../0.1.7/evidence/stage-6-history-205-save-split-20260920T081022Z/README.md). #205 and #190 both stay open.

`storage.accept_loop` was one span around `for id { operation.accept(object) }` and 50–55 % of a `history.*` row's operation, with no attribution at all. `SaveProfile` (`cas/owner.rs`) now charges **seven disjoint nanosecond buckets** — resolution, FULL encode, delta encode, group codec, pack placement, SQL, commit — at the call site that does that work, accumulated into `OutcomeCounters` and published per state as `history.state.<n>.save.*_ns`. It is an aggregate, never a span per object: a stride1 row accepts ~10^5 objects and the driver refuses a per-object timer node. `resolve` is itself split five ways (`eligible`, `acquire`, `cost`, `reuse`, `pooled`), and the five parts are asserted to sum to `resolve` exactly on every row, so the refinement renames which bucket a charge lands in rather than re-scaling the split.

**The split, in seconds.** stride10 (17 states, 52,032 objects, operation 16.296 s, scope `save_ns − storage.begin` 9.312 s, `accept_loop` node 8.820 s = 94.71 %): resolution **4.247** (45.61 %), FULL 1.599 (17.17 %), delta 0.637 (6.84 %), group codec 0.044 (0.47 %), placement 0.262 (2.82 %), SQL 1.068 (11.47 %), commit 0.421 (4.52 %), remainder **1.034 (11.10 %)**. stride1 (157 states, 97,788 objects, operation 101.994 s, scope 47.821 s; `accept_loop` and `harness.index` are not recorded above 53 states by the driver's ordinary recording, so the scope overstates `accept_loop` by exactly those two terms): resolution **36.458** (76.24 %), FULL 3.348 (7.00 %), delta 1.403 (2.93 %), group codec 0.151 (0.32 %), placement 0.467 (0.98 %), SQL 2.104 (4.40 %), commit 0.882 (1.84 %), remainder **3.008 (6.29 %)**.

**Resolution, split five ways (stride1):** exact-reuse verification **16.544 s (45.38 % of resolution, 34.60 % of scope)**, pooled lane lookup + base **8.086 s (22.18 %)**, base acquisition **6.633 s (18.19 %)**, eligibility walk **5.186 s (14.23 %)**, post-trial cost walk **0.008 s (0.02 %)**. Per object early → late: reuse 85,249 → 169,408 ns (**1.99×**, 37.9 % of the scope's per-object growth), pooled 37,793 → 95,434 (**2.53×**, the only part growing faster than the scope's 1.73×), eligibility 19,609 → 68,257 (**3.48×**, fastest of all). At the coarser row the order is different and the split says so: there the pooled lane is the largest part (0.732 s, 59.5 % of resolution) and reuse verification is almost absent (0.096 s, 2.25 %).

Two things this settles that were listed as unknown: the **group codec is 0.151 s at stride1** (0.32 %) — L40's +1.685 s was the *delta* of level 19 against level 1, and this is the level-1 absolute, so codec level is not a lever worth one second; and the **post-trial cost walk is nil** (8 ms), so the depth cap binding that L51 reported is a policy fact, not a time cost at that site.

**The middle row separates a volume term from a cost term.** All three rows were sampled from one build (`68f0cc5e687c…`), the stride3 row last because the 17 → 157 pair cannot tell a term that grows because there is *more of it* from one that grows because *each unit* costs more. Resolution: **4.247 / 11.584 / 36.458 s** at 17 / 53 / 157 versions. **Exact reuse** 0.096 / 2.165 / 16.544 s (**172×**) is a **volume** term — 0.022 → 0.298 → 1.240 events per object (56× per object) while the cost per event moves only 85 → 100 → 136 µs. **The eligibility walk** 0.888 / 2.252 / 5.186 s (**5.84×**) is a **cost-per-walk** term — walks stay flat at ~0.75 per object and each walk costs 22.8 → 40.1 → 69.8 µs as chains lengthen. Pooled 11.05× and base acquisition 2.62× sit between them. Either end of the curve alone misleads: stride10 shows reuse at 1.03 % of scope, stride1 shows the eligibility walk at only 14.23 % of resolution. **The save path's own scaling:** operation 16.296 / 34.594 / 101.994 s (6.26×) against objects written growing 1.88×, so operation per object grows 3.33× (313.2 / 476.8 / 1043.0 µs) and the save scope grows **2.73× per object**; the uncharged remainder shrinks monotonically 11.10 % → 8.07 % → 6.29 %. Store equality holds on all three rows (`4af37932a…`, `f5c7ff5a6…`, `1635cf7bb…`).

**Three numbers the instrument retires.** L50/L51's +0.947 / +0.805 / +0.409 elapsed-versus-count correlations are superseded for attribution by the split above. L50's synthetic 1.03 s append bound is still synthetic. L49 → L50's harness-publishing effect (+0.162 s on stride10) remains the only clean harness-effect figure; the cross-arm differences this round reads (−0.612 s stride10, −3.732 s stride1, against the retained uninstrumented samples) have the **wrong sign to be an overhead bound** and are reported as reproducibility, not as a saving. The one clean within-build bound is `instrument2` against `instrument` on stride10: **+0.048 s** for the sub-split, which charges the same number of clock pairs.

**The treatment decision, from the rule's own order.** The split named B1 (do not re-verify what this save already verified; count the repeats first, because the brief recorded that the count did not exist). A measurement-only probe enabled solely by `LAYERFS_STORAGE_REUSE_PROBE=1` — it observes the completed verification, changes no decision, byte, row or root, and is off by default — counts every reuse occurrence whose identity this same operation had already verified. Measured: **121,301 reuse occurrences at stride1, 121,301 probe observations, 0 repeats**; at stride10 the per-call trace shows the probe reaching `reuse_or_collide` exactly 1,121 times of 1,121 and finding 0 repeats. Both runs' `save.reuse_repeat` is 0 in every state and 0 in total. The probe's own cost is a `BTreeSet` insert per occurrence: 121,301 insertions of a 32-byte key measured at **13.1 ms**.

**So B1 is not pre-registered, not implemented, and no saving is claimed for it.** A wave-scoped memo and a whole-operation memo would both save exactly nothing: within a wave a repeated identity is already answered by the wave-local `prepared` map before `reuse_or_collide` is reached, and across waves a row written earlier is in the wave's `by_id` snapshot, so the identity is found by lookup rather than re-resolved. L52's 9.21× growth in *reuse events per object written* is growth in the number of **distinct** identities reaching the reuse path, not in repeated work — and that growth is real (reuse is still 1.99× per object and 8.504 s of the late third), so any future treatment must make the **first** verification cheaper, which is a retained-base question the owner has reserved, not a memo. This round therefore ships **no product behaviour change**: the instrument, the split, and the negative result.

**What the split names next, with no treatment claimed.** The pooled lane's resolution (largest part at stride10, second at stride1, only part outgrowing the scope) and the eligibility walk (fastest-growing part; the winning candidate's chain is walked once to measure depth and then walked again by `acquire` to rebuild it — the brief's candidate C, whose counting was required before implementation).

**Equivalence and protocol.** Both instrumented rows reproduce the recorded Store constants exactly — stride10 `4af37932a…`, stride1 `1635cf7bb…` — with 17 and 157 state roots unchanged and the canonical inventory identical; the probe row reproduces stride1's constant too. The instrument observes and does not decide. One sample per case per arm, fresh `--output` per run, `collect.py` refuses an existing run directory; both global flocks held for every resource command with three deferrals retained on disk; quiet preflight recorded in each declaration; `--pre-execute` once per freshly built binary; caps unchanged (120 s / 720 s, never approached); `--locked`, Rust 1.85.1, one construction worker, no second lane. `collect.py` gained one recorded option (`--env`, published as `extra_environment`) so exactly one arm could enable the probe.

Checks as run: core `fmt --check` exit 0, `clippy -D warnings` exit 0, `test` exit 0 with **494 passed / 0 failed**, `check_product_boundary.py` PASS and its self-tests 6/6 OK; harness `test` exit 0 with **117 passed / 0 failed** and a release build exit 0. No CI and no `tools/preflight.sh` (permanently retired). Not run: verification mode (the rows are diagnostics, admission `INELIGIBLE`, every budget class `NOT_RUN`).

Production LOC: **85926 → 85978 (delta +52)**; core 20509 → 20561 (+52, `layerfs-storage` 7185 → 7237), reference 65417 unchanged. Method `tools/production_loc.py`, first parent `56ffd020b` against the final staged tree: `cas/owner.rs` +46, `cas/membership.rs` +5, `cas/lifecycle.rs` +1. The harness edits (`ops/history.rs`, `collect.py`) are excluded from the production count.

**Addendum — the operations, and the deferral question (same round).** The changed set has three non-empty kinds: **construct 46,029 / edit 135,003 / delete 36,614** paths at stride1 (`MetadataOnly` is 0 in all 157 checkpoints). Reuse is a **volume** term on constructs and edits, not on writes: `r(reused, modified)` = 0.813, `r(reused, added)` = 0.793, `r(reused, inserted)` = **−0.05**; reuse events run **0.56 per changed path** (121,301 / 217,646) and **2.64 per construct** at stride1. Two construction terms, separated by the harness's declared control `LAYERFS_HISTORY_CHUNK_PREDECESSORS=0` on stride10: `content` **1.585 → 1.039 s**, `filesystem` 5.146 → 3.490 s, `accept_loop` 8.820 → 5.847 s, operation **16.296 → 10.882 s (−33 %)** — so ~0.55 s of `content` is prior-state resolution read from the Store, not construction, and the remaining 1.039 s is flat at **24–40 µs per changed path across all 17 states**. Content construction does not degrade with history; the tree term is the larger and the growing one. **The base-offering policy is the largest single lever found so far (−33 % of the stride10 operation), at the cost of a larger Store — both axes required if ever treated.**

**The deferral question, and the successor directive.** v0.1.6's own `create` phase (= `create_workspace_session`, the FUSE mount) is **9–11 ms and flat across depth** (boundary-cycle 11.14 → 9.14 ms K10 → K100; large-hotset 10.17 → 9.77; namespace-inode 9.90 → 9.82) from `issue154/final-complete-matrix.json`. It materialises no content — branch pin, state dir, projection attach, tree-metadata read — so it is **not** a construction figure, and `create ≠ construction + mount`. The core pays eagerly inside the operation for what FUSE defers: `content` + `filesystem` = 1.04–5.15 s per state, growing with history. **FUSE relocates that cost rather than deleting it**: ingest still constructs and admits whatever it serves, so FUSE wins where the workload reads a small fraction of what it stores (v0.1.6's shape) and loses where it reads everything (this lane's shape, 904,143 path-states of a 4,936,693,030-byte history). The number that would settle it — **first-read latency after mount** — is missing on both sides: v0.1.6's `historical_access` declares performance `N/A` in all six cases and the core has never run it. #205's issue draft asks the successor to land FUSE, add a mount node for comparison, and measure first-read-after-history with the read fraction declared. The eager `filesystem` cost is what makes the Store dedupe a whole history in one operation, so whether to defer it is an **owner decision**, recommended and not ruled here.

## L54 — #209 the regression is a `LIMIT` clause, not the commit cadence; the step cannot be widened (2026-09-20)

Status: Research; **diagnostic evidence, not release admission**. Product change (`layerfs-storage`) plus harness-independent evidence. [Report](../../0.1.7/evidence/stage-6-history-209-rca-20260920T191016Z/README.md), [pre-registration](../../0.1.7/evidence/stage-6-history-209-rca-20260920T191016Z/PREREGISTRATION.md), [results](../../0.1.7/evidence/stage-6-history-209-rca-20260920T191016Z/results.txt), [second writer](../../0.1.7/evidence/stage-6-history-209-rca-20260920T191016Z/multi-writer-latency.txt), [raw runs](../../0.1.7/evidence/stage-6-history-209-rca-20260920T191016Z/runs/). Continues L53 on the same branch; the #205 2.06× regression the handoff `a02168adb` carried is root-caused.

**Order followed.** Root cause first: the instrument was extended to charge the locator lookups and the transaction boundary that no `SaveProfile` bucket named, two separation arms were sampled (`instr0` = shipped model instrumented, 33.116 s; `c1nocommit` = the same binary with step commits disabled, 25.523 s), and only then was one treatment pre-registered and written.

**The dominant cause is one SQL clause, and it is not the cadence.** `sqlite/lookup.rs::candidates` carries the publication-scoped locator query. The engine attribution puts **10.771 s of the 10.411 s `resolve_ns` bucket** into it — 380,380 queries at **28.32 µs** each; disabling every step commit lowers that to 21.78 µs and recovers 7.593 s, so the cadence is a real but *second* per-query effect. Isolated timing of the query texts against the run's own Store (30,000 calls each, same pragma profile) separates them: previous-model text **4.559 µs**, new-model text with publication scoping and **no** `ORDER BY`/`LIMIT` **3.488 µs** — the scoping is 1.07× *faster* than the previous query — and the same new-model text with `ORDER BY o.object_id,o.save_id LIMIT ?` **15.543 µs**. `ORDER BY` alone 3.485 µs; `LIMIT` alone 13.840 µs; `EXPLAIN QUERY PLAN` is identical with and without. **`resolve_ns` is a symptom, not a second root**, and the smaller measured bucket (`commit_ns`) is the one the `eb319aaa9` message blamed.

**The one treatment, pre-registered before it was written.** Remove `ORDER BY … LIMIT ?` and its bound parameter from that query; nothing else. Matched clean arms, one sample each, fresh `--output`, one binary per arm with its sha256 recorded, both global flocks held: **operation 33.116 → 26.467 s (−6.649 s, −20.1 %)**, `resolve_ns` 10.411 → 6.848 s, `commit_ns` unchanged at 3.015 → 3.154 s and **commits unchanged at 48,446**, so the multi-writer cadence is untouched. The strictly matched instrumented pair reads 33.116 → 25.630 s (−7.486 s) with `resolve_ns` 10.411 → 6.691 s and per-locator **28.32 → 10.00 µs**. **All 31 workload counters are identical in all four arms** (`commits` 48,446, `statements` 44,334, `pack_appends` 45,794, `packs_created` 255, `reused` 1,121, `inserted` 0, `full_records` 12,952, `prefix_records` 39,080, `chain.objects` 107,628, `pool.leaves` 1,738, and every derived `save.*` counter) and **the saved Store is byte-identical** (`7ea2fe6ccf13bc5a…`); only the arm that changed the model has different bytes (`64de588056101d65…`). Publication scoping, collision checking, the ownership watermark and failure cleanup are untouched.

**The step boundary cannot be enlarged — it fails, it does not wait.** A measurement-only, default-1 lever (`LAYERFS_STORAGE_COMMIT_EVERY`, removed after this round) committed once per N steps instead of once per step: N=1 both writers finish in 5/5 rounds with **zero** `OwnershipUnavailable`; **N=8, N=64 and N=100000 all fail in round 0** with `CleanupFailed { original: OwnershipUnavailable, cleanup: OwnershipUnavailable }`, because `busy_timeout` is zero by declared profile and the refused save's cleanup needs the lock it cannot get. So the design tension the handoff framed has no interior: there is no step size that amortises batching while W > 1 still works. **The second writer's wait on the shipped step, measured**: 1,290 of 1,306 calls sub-millisecond, the ~50 that are not are the group seals, so the lockable step is **p50 0.4 µs / p99 ≈11 ms / worst observed 37.6 ms**, and the writer that owns no lock pays it (solo mean 120–134 µs against pair mean 400–413 µs for the same calls). Both writers completed in every shipped-model round: the capability is retained and measured, not assumed.

**Open, and stated.** 10.11 s remains against the held 16.360 s: `commit_ns` +2.72 s, `filesystem` +3.35 s (out of this RCA's scope), and **≈4.8 s of accept growth that the seven buckets do not charge** (accept 8.949 → 16.875 s; buckets 6.79 → 14.818 s; the arm's own remainder is 3.499 s of a 22.255 s span). The largest uncharged candidate is `placement.rs::write_pack`, a whole-pack-body UPDATE charged to no bucket (255 packs, 45,298,203 bytes, average 177,640, maximum 262,112, so ~8.1 GB of BLOB rewritten across 45,794 appends) — **arithmetic over retained counters, named as a hypothesis and not measured as time**. `full_ns` (2.436 / 1.691 / 1.732 s) and `sql_ns` (1.864 / 1.889 / 1.995 s) move without any counter moving; the movement is confined to arms sharing a build and is recorded as between-sample variance of the class L53 reported. One preflight deferral (58.88 % idle) was written and retained; the sample was taken on retry into a new `--output`.

Checks as run: core `fmt --check` exit 0, `test --locked --workspace` **535 passed / 0 failed** (with `multi_writer` 5/5, `memory_bounds` 10/10, `visibility` 9/9, `persistence_failure` 8/8), `clippy -D warnings` exit 0, `check_product_boundary.py` PASS over 175 files and its self-tests 6/6 OK; harness `test` **117 passed / 0 failed** and a release build exit 0. No CI and no `tools/preflight.sh` (permanently retired). Not run: verification mode — every row is a diagnostic, admission `INELIGIBLE`, every budget class `NOT_RUN`. No cap promoted, no case re-run for a better number, no cross-binary effect size, no cell dropped.

Production LOC: **25394 → 25391 (delta −3)**; core 25394 → 25391 (−3, `layerfs-storage` only; reference unchanged), reference 65417 unchanged, combined 90811 → 90808. Method `tools/production_loc.py`, first parent `a02168adb` against the final staged tree; the whole delta is `sqlite/lookup.rs` (2 insertions, 5 deletions): the `LIMIT` clause and its bound parameter.

## L55 — #209 addendum: `write_pack` refuted at 1.44 s, L54's ≈4.8 s withdrawn, and the real residual is 1.59 s (2026-09-20)

Status: Research; **correction to L54**, same branch, no product line changed. [Addendum](../../0.1.7/evidence/stage-6-history-209-rca-20260920T191016Z/pack-write-addendum.md), [report §6 as corrected](../../0.1.7/evidence/stage-6-history-209-rca-20260920T191016Z/README.md), [raw runs](../../0.1.7/evidence/stage-6-history-209-rca-20260920T191016Z/runs/).

L54 named `cas::placement::MutationOwner::write_pack` as the largest uncharged accept-path candidate and priced the unexplained residual at ≈4.8 s. Both are now measured, and **both statements are withdrawn**.

**The hypothesis is refuted.** `write_pack` was instrumented directly — the call, the pack body bytes, the cache eviction, the body statement and the `UPDATE saves SET pack_ceiling` beside it — and sampled on two matched arms with the probe declared in `extra_environment`. It is **1.440 s** of 46,049 calls writing **4,487,746,188 bytes (4.18 GiB)** of pack body for a 49 MB Store: evict 0.003 s, `body` 1.255 s (27 µs and 98 KB per append), `ceiling` 0.182 s. 1.22 s of that was already inside `sql_ns`, so the genuinely uncharged part is **0.22 s**. The 4.18 GiB is real and cheap; arithmetic over counters was not allowed to stand as a result, and it did not.

**The ≈4.8 s was an arithmetic error.** L54's §6 subtracted the **retained #205 instrumented arm's** bucket sum (6.79 s) from the **new binary's** accept span — two different instruments across two different binaries, which is not a measurement. Read within one arm the remainder is small, and it is cadence-driven, not stable: `instr0` 3.499 s, `c1nocommit` 2.463 s, `c2locator` 1.922 s, `treatment2` 2.057 s, `packprobe` 1.670 s (of which 1.440 s is the now-measured `write_pack`), **`packprobe-nocommit` 0.850 s**.

**What is actually left is 1.59 s, not 10.1 s.** With both effects removed — the locator `LIMIT` fixed *and* step commits disabled — the same source reads **operation 17.950 s** against the held 16.360 s (1.10×), `accept_loop` 8.998 s, `resolve_ns` 4.364 s, `commit_ns` 0.060 s: **−15.166 s of the −16.756 s regression**. The two effects therefore account for 15.17 s, and the residual is 1.59 s. That arm is an **experiment, never a candidate** — it has no multi-writer capability, which is the whole point of the model. The shipped arm still reads 26.467 s because the cadence is a contract: `commit_ns` 3.154 s against 0.060 s unbounded, plus ≈2.6 s of unnamed per-step engine work, both of which only disappear with the cadence itself.

**Nothing about L54's treatment or capability claim changes.** The locator fix is still the one treatment, all 31 workload counters and the saved Store are still byte-identical, multi-writer is still retained and measured (zero `OwnershipUnavailable`), and the step is still not widen-able (it fails, it does not wait). `filesystem` (+3.35 s) remains outside this RCA's scope and unattributed. What changes is the size of the open problem, and the next question is what the last 1.59 s is.

Checks as run: product tree unchanged by this addendum (`git status` clean over `core/crates`, `crates`); the measurement-only probe and the step-commit lever were both removed after the round, so the tree at `c2197be73` plus this commit is the verified tree. L54's checks (core fmt/test/clippy, boundary check and self-tests, harness 117 tests and release build) stand unchanged. Not run: verification mode — diagnostics, admission `INELIGIBLE`, every budget class `NOT_RUN`.

Production LOC: 25391 → 25391 (delta 0). Diagnostic evidence only; the probe is not in the product tree. Method: `tools/production_loc.py`, first parent `c2197be73` against the final staged tree.

## L56 — #209 the 10.107 s attributed to the millisecond, and the commit-cadence handoff (2026-09-20)

Status: Research; **diagnostic evidence, not release admission**, and a continuation prompt. No new sample, no product line changed. [Attribution](../../0.1.7/evidence/stage-6-history-209-rca-20260920T191016Z/gap-attribution.md), [handoff prompt](../../0.1.7/issue-multi-writer-commit-optimization-handoff.md). Continues L54/L55 on the same branch.

**The gap is now closed arithmetically.** Against the held previous-model row (16.360 s), the shipped arm's +10.107 s decomposes as: accept span +7.926 s and `filesystem` +3.349 s, less the accept path's own unnamed growth +0.126 s, less −1.042 s from the other spans outside `accept_loop` — 11.149 − 1.042 = 10.107 s. The seven bucket Δ values sum to +5.871 s (`commit_ns` +2.717, `resolve_ns` +1.991, `sql_ns` +0.908, `full_ns` +0.121, `place_ns` +0.071, `delta_ns` +0.051, `group_ns` +0.007) and 7.926 − 5.871 = 2.055 s, exactly the unnamed growth inside the accept span plus the two spans outside it (0.126 + 1.929). **Every term reconciles to the millisecond.**

**The three big terms and their mechanism.** `commit_ns` +2.717 s: 48,446 commits at 65.1 µs against 1,149 at 380 µs — the statement got cheaper, the sequence got 42× longer, and priced per unit of work the regression is **6.8×** (9.5 µs of commit per append before, 65.1 µs now); inside the bucket `write::commit` is 2.809 s (58.0 µs) and `ownership::advance_pack`, one `UPDATE store_policy` per commit, is 0.345 s. `resolve_ns` +1.991 s: 380,380 locator calls carrying the `LIMIT` (+12.1 µs, now removed) and the cadence (+6.5 µs). `sql_ns` +0.908 s plus the uncharged per-iteration work: the same bytes inserted and rewritten 42× more often. **One sentence produces all three: the model kept every byte of work and removed the amortisation** — one transaction used to cover ~40 appends, so a page dirtied 40 times was written once.

**The next round is scoped to `commit_ns`, and the first hypothesis is untested and cheap.** `commit_ns` is 3.154 s of a 26.467 s operation (11.9 %) and pure overhead. The declared profile sets `journal_mode = MEMORY`, `synchronous = OFF`, `temp_store = MEMORY`, `busy_timeout = 0` and **never sets `cache_size`, `mmap_size` or `cache_spill`** — the product only reads them back "for evidence only". Measured on the run's own Store: **`cache_size` is the engine default (`-2000`, 2 MiB), `mmap_size` 0, `cache_spill` enabled**, against a **49 MB Store in 255 packs averaging 178 KB** and **4.18 GiB** of pack body written across 45,794 appends. `COMMIT` cost tracks the dirty page set, so a 2 MiB page cache is the first thing to test; it is a declared-profile change, not a format or contract change, and it has a memory axis that must be measured (`process_peak_rss` is 250–257 MB in all five arms; `memory_bounds.rs` guards the ceiling). Second: `advance_pack`'s 0.345 s, one `store_policy` UPDATE per commit for a watermark that need only be correct at publication — established against the arbitration invariant before anything is proposed. The handoff's acceptance bar is **stride10 strictly below 26.467 s with `commit_ns` strictly below 3.154 s and the second writer still streaming**; a change that recovers time by serialising, refusing or delaying the second writer has failed however fast it is. `filesystem` (+3.349 s, the largest single unexamined term) is explicitly **not** this round.

Production LOC: 25391 → 25391 (delta 0). Documentation only; the measured product tree is unchanged from `63fa15c49`. Method: `tools/production_loc.py`, first parent `09bfbd2a4` against the final staged tree.

## L57 — #209 `commit_ns`: the page cache is refuted by the engine's own page accounting, and the per-step policy re-assertion is removed (2026-09-21)

Status: Research; **diagnostic evidence, not release admission**. Product change (`layerfs-storage`) plus two new external cases. [Report](../../0.1.7/evidence/stage-6-history-209-commit-20260920T194819Z/README.md), [pre-registration](../../0.1.7/evidence/stage-6-history-209-commit-20260920T194819Z/PREREGISTRATION.md), [diagnostic outputs](../../0.1.7/evidence/stage-6-history-209-commit-20260920T194819Z/scratch/), [raw runs](../../0.1.7/evidence/stage-6-history-209-commit-20260920T194819Z/runs/), [analysis](../../0.1.7/evidence/stage-6-history-209-commit-20260920T194819Z/analyze.py). Continues L54–L56 on the same branch.

**The round's first hypothesis is refuted before any treatment was written.** L56 named a 2 MiB page cache against a 49 MB Store as the first thing to test. A replay of the step's statement shape on a copy of the measured run's own Store, **interleaved round-robin in one process**, six rounds of 2,000 commits per arm, writes **exactly 15.30 pages per commit in every arm** — declared profile, `cache_size = 64 MiB`, `cache_spill = 0`, both, `mmap_size = 256 MiB`, and a contract-breaking `journal_mode = OFF` diagnostic — at **1.86–1.92 µs per page**, an 1.8 % spread, with zero spills. A bare 4 KiB `pwrite` on the same volume is 1.723 µs, so the per-page cost *is* the write syscall. The mechanism is `sqlite3BtreeInsert`: it overwrites a row in place only when the new payload is the same size as the old (*"New entry is the same size as the old. Do an overwrite."*), and a pack grows on every append, so SQLite frees the overflow chain and writes a fresh one. `pages written == ceil(pack body / 4096)`; the 4.18 GiB of pack body L55 measured is 1,095,640 pages, and at ~2.5 µs a page that is the 2.809 s `COMMIT`. **The payload's pages are the stored format's price** — any fix (chunked packs, a pre-allocated row, incremental-blob writes) moves the Store hash and is therefore a different operation. One `mmap_size` arm that looked 23 % faster in a first, sequential version of the probe did not survive interleaving and is recorded as drift.

**The one treatment, pre-registered, is the per-step policy write.** `ownership::advance_pack` (one `store_policy` UPDATE per commit, priced by the instrumented arm at **0.345 s over 48,446 calls**) writes back the value already in the row on **48,191** of them, because `next_pack_id` moves only when `LanePlacement` starts a new pack (255 times); `placement::write_pack`'s `saves.pack_ceiling` UPDATE does the same on **45,794 of 46,049**. The treatment: **a step commits the policy state it changed, not the policy state it re-asserted** — the watermark is advanced only when it moved, the ceiling written only by the append that creates a pack. The watermark is **not** deferred to publication (that is the change that would let a second writer collide), and two new cases in `core/crates/layerfs-storage/tests/pack_watermark.rs` pin it from outside the crate: reading `store_policy` through an independent connection between steps, and two writers interleaved between steps. **Both fail on a structurally deferred watermark** (retained red run: `pack_watermark.rs:52` and a pack-identifier collision at `pack_watermark.rs:93`).

**Measured, one binary, one sample per arm.** Binary sha256 `106f181b31e70808cc224c8e00a6dbf9e2b1b232002818eb7025dd0dfe5cd36d` for both arms, the arm selected by a measurement-only `LAYERFS_STORAGE_POLICY_REASSERT` lever declared in `extra_environment` and removed before this commit. Gate pair: operation **17.105 → 16.499 s**, `commit_ns` **1.933 → 1.768 s** (39.9 → 36.5 µs per append). A declared **A B B A** diagnostic (run because §4's machine shift exceeds the effect) reads 16.634 / 16.451 / 16.381 / 16.524 s and 1.847 / 1.746 / 1.749 / 1.872 s: **drift-cancelled contrast −0.163 s operation and −0.112 s `commit_ns`**, with the window's own drift at −0.110 s and +0.025 s. Pooled over three control and four treatment rows: operation **−0.237 s (−1.41 %)**, `commit_ns` **−0.111 s (−5.90 %)**, while `resolve_ns` (+0.001 s), `sql_ns` (−0.012 s) and `filesystem` (+0.020 s) do not move — the treatment touches `commit_ns` and nothing else. **`commit_ns` separates without overlap in all seven rows** (control 1.847–1.933 s, treatment 1.746–1.829 s); the operation does not once the shipped row is included (16.739 s against a 16.524 s control minimum). The pre-registered prediction, rescaled to today's level, was −0.21 s / −0.25 s.

**The held absolute bar is not reproducible today, and that is measured rather than assumed.** The archived previous-round binary (sha256 `63ef5919…`, unchanged) read **26.467 s in its own session and 19.908 s in this one**, with identical counters and an identical Store; two binaries with the same product behaviour read 17.105 s and 19.908 s fifteen minutes apart. So the acceptance bar (operation < 26.467 s, `commit_ns` < 3.154 s) is met by the treatment arm **and by the control arm**, does not discriminate, and is **not claimed as this round's evidence**; the matched contrast is. Corpus residency reads 0 pages in every row of this round against 5,141 previously, so this round's rows are the colder ones.

**Equivalence and capability.** The saved Store is **byte-identical in all seven rows** (`7ea2fe6ccf13bc5a…`, 51,867,648 bytes, the constant L54 recorded) and **all 582 workload counters are identical** (38 `delta.*` plus 544 per-state `save.*`/`inserted`; `commits` 48,446, `pack_appends` 45,794, `packs_created` 255, `statements` 44,334, `inserted` 52,032, `chain.objects` 107,628, `pool.leaves` 1,738). The second writer still streams on the shipped step in both arms: three rounds of `solo`/`pair` over a 24 MiB payload, **both writers finishing in every round with zero `OwnershipUnavailable`**, pair p99 6.7–7.1 ms in the treatment arm against 6.4–7.1 ms in the control and worst observed 9.8 ms against 36.5 ms; `multi_writer.rs` 5/5. Four preflight deferrals (idle 58.15 / 61.59 / 72.38 / 74.50 % at load 10.5 / 5.5 / 6.2 / 7.7; two with another worktree's `cargo` named) were written and retained; each retried into a fresh `--output`. Two long-running `grep` processes at ~99 % CPU were present through the round and are declared interference shared by every row.

**Named for the next round, with its measurement.** D3 measured the hot per-step statements being prepared **fresh on every call**: `UPDATE object_packs SET data …` **4,723 ns against 102 ns** from the statement cache over 46,049 calls a run, `UPDATE store_policy …` 1,966 ns against 84 ns, `SELECT next_pack_id` 1,106 ns against 83 ns — a 0.2–0.4 s term inside `sql_ns` and the uncharged work, and a *separate* treatment from this one. `resolve_ns`'s cadence share (≈0.82 s) and `filesystem` (+3.35 s) remain untouched, as does the locator query.

Checks as run: core `fmt --check` exit 0, `test --locked --workspace` **537 passed / 0 failed** (535 plus the two new `pack_watermark` cases; `multi_writer` 5/5, `memory_bounds` 10/10, `visibility` 9/9, `persistence_failure` 8/8), `clippy -D warnings` exit 0, `check_product_boundary.py` PASS over 175 files and its self-tests 6/6 OK; harness `test` **117 passed / 0 failed** and a release build exit 0, harness source unchanged. No CI and no `tools/preflight.sh` (permanently retired). Not run: verification mode — every row is a diagnostic, admission `INELIGIBLE`, every budget class `NOT_RUN`. No cap promoted, no cross-binary effect size, no cell dropped, no case re-run for a better number.

Production LOC: **25391 → 25403 (delta +12)**; core 25391 → 25403 (+12: `layerfs-storage` 7585 → 7597), reference 65417 unchanged, combined 90808 → 90820. Method `tools/production_loc.py`, first parent `10df6ef28` against the final staged tree; per file `cas/lifecycle.rs` +9, `cas/placement.rs` +2, `cas/owner.rs` +1. The new external cases and this campaign's evidence do not contribute.

## L58 — #209 correction: the per-append commit multiple is 7.7×, not 4×, and the operation regression is window-dependent (2026-09-21)

Status: Research; **correction to L57's quoted comparison and to L56's source arithmetic**. Diagnostic rows, no product line changed. [Addendum](../../0.1.7/evidence/stage-6-history-209-commit-20260920T194819Z/previous-model-window.md), [raw runs](../../0.1.7/evidence/stage-6-history-209-commit-20260920T194819Z/runs/), [analysis](../../0.1.7/evidence/stage-6-history-209-commit-20260920T194819Z/analyze.py).

**What was wrong.** L57's report and its summary priced the shipped model's commit at 36.6–38.7 µs per append against the previous model's **9.5 µs** and called the multiple **~4×**. The 9.5 µs is arithmetically right for its own row (0.4367 s over 45,791 appends; 1,149 commits at 380.0 µs) but it was measured in the **2026-09-20T08:10Z** window, so the 4× divided one window's numerator by another window's denominator — the class of comparison this lane withdraws. `gap-attribution.md` §3's **6.8×** has a second defect: it divided 65.1 µs of `commit_ns` per *commit* by 9.5 µs per *append*.

**Re-measured, in one window, both sides.** The previous-model binary was never archived; it was rebuilt from `f039bcaf2` (`3d818895f70b…`, archived) and validated as the same operation by saving the **byte-identical retained previous-model Store constant `4af37932aa3391b1…`** in both of its rows, with the retained counters (1,149 commits, 45,791 appends, `chain.objects` 107,628). Harness source seal `04bcfab5…` on both sides — **the harness is identical and only the product crates differ**. Four rows back to back, one sample per arm per order, balanced `prev → shipped → shipped → prev` so the corpus-residency asymmetry (0 resident pages for whichever arm runs first, 5,141 for the second) is shared rather than charged: `prev-model` 11.033 s / 0.242 s, `shipped-b` 16.481 s / 1.794 s, `shipped-c` 16.714 s / 1.817 s, `prev-model-b` 10.921 s / 0.228 s.

| pair | operation | `commit_ns` | per append |
| --- | ---: | ---: | ---: |
| prev → shipped | 11.033 → 16.481 s (**1.49×**) | 0.242 → 1.794 s (**7.40×**) | 5.29 → 39.18 µs |
| shipped → prev | 10.921 → 16.714 s (**1.53×**) | 0.228 → 1.817 s (**7.96×**) | 4.98 → 39.68 µs |
| balanced means | 10.977 → 16.598 s (**1.51×**) | 0.235 → 1.806 s (**7.67×**) | **5.14 → 39.43 µs (7.67×)** |

**The corrected numbers.** The per-append commit multiple is **7.67×** here and **7.17×** on the 08:10 same-session pair (9.54 µs against 68.4 µs) — the ratio is **window-stable** while the absolute prices are not (the previous model reads 9.54 µs there and 5.14 µs here; the multi-writer model 68.4 µs and 39.43 µs). Per append the 08:10 shipped figure is 3.154/45,794 = 68.9 µs, so §3's multiple is **7.2×, not 6.8×**. And the **operation-level regression is 1.51× in this window against the 2.02× the #205 pair measured in its own** (16.360 → 33.123 s, same session, and that row stands): both models are faster here (previous 16.36 → 10.98 s, shipped 33.12 → 16.60 s) by different factors, so the commit multiple held and the operation multiple did not — itself evidence about where the regression lives.

**The gap in this window, decomposed.** operation +5.621 s = `commit_ns` +1.570 s (27.9 %), `resolve_ns` +1.528 s (27.2 %), `filesystem` +0.980 s (17.4 %), uncharged inside the accept span +0.780 s (13.9 %), `sql_ns` +0.501 s (8.9 %), `full`+`delta`+`place`+`group` +0.055 s, `content` +0.008 s, other spans +0.199 s. **`commit_ns` and `resolve_ns` are within 3 % of each other as the two largest terms**, which is why the next rounds are `resolve_ns` and `filesystem` rather than `commit_ns` again. Nothing in L57's treatment, its equivalence evidence or its capability claim changes: the Store is still byte-identical and all workload counters identical within the round's own binary pair.

Checks as run: no product line changed by this addendum (`git status` clean over `core/crates`, `crates`); L57's checks stand. One preflight deferral was written and retained during the addendum. Not run: verification mode — diagnostics, admission `INELIGIBLE`, every budget class `NOT_RUN`.

Production LOC: 25403 → 25403 (delta 0). Diagnostic evidence only; the rebuilt previous-model binary is an instrument, not a product line. Method: `tools/production_loc.py`, first parent `704580673` against the final staged tree.

## L59 — #209 the per-step statement cache measured: the bucket moved, the wall clock did not, and the treatment is withdrawn (2026-09-21)

Status: Research; **withdrawal, diagnostic evidence, not release admission**. **No product line is kept**: the tree is byte-identical to `f3e84c073` over `core/crates` and `crates`. [Report](../../0.1.7/evidence/stage-6-history-209-stmtcache-20260920T210202Z/README.md), [pre-registration](../../0.1.7/evidence/stage-6-history-209-stmtcache-20260920T210202Z/PREREGISTRATION.md), [raw runs](../../0.1.7/evidence/stage-6-history-209-stmtcache-20260920T210202Z/runs/), [successor prompt](../../0.1.7/issue-commit-time-rca-handoff.md). Continues L57/L58 on the same branch.

**The treatment, pre-registered before it was written.** L57's own report named the next item with its measurement: every SQL text the per-step path issues is parsed again on every call — `UPDATE object_packs SET data …` **4,723 ns against 102 ns** cached over 46,049 calls, `SELECT next_pack_id …` 1,106 ns against 83 ns, `BEGIN IMMEDIATE` and `COMMIT` likewise. The treatment put six texts through the connection's prepared-statement cache, predicting the operation **≤ 16.35 s**, `sql_ns` **≤ 0.96 s**, `commit_ns` **≤ 1.79 s** and `resolve_ns` **not above 4.45 s** (all against that window's 16.6 s scale), with withdrawal if the operation or `sql_ns` failed to fall.

**Measured, one binary for both arms, A B B A.** Binary sha256 `85605a522738e6…`, the control arm behind `LAYERFS_STORAGE_STATEMENT_CACHE=0`, declared in `extra_environment`, removed with the treatment. The driver's `b` arm carries the lever and the lever *restores* the old behaviour, so `b` is the **control**; four rows, the saved Store byte-identical in all of them (`7ea2fe6ccf13bc5a…`) with `commits` 48,446 in every row:

| arm | operation | `commit_ns` | `sql_ns` | `resolve_ns` |
| --- | ---: | ---: | ---: | ---: |
| treatment (`stmt-1a`, `stmt-4a`) | 22.256 / 22.770 s | 2.485 / 2.494 s | **1.279 / 1.343 s** | 5.985 / 6.170 s |
| control (`stmt-2b`, `stmt-3b`) | 22.536 / 22.537 s | 2.377 / 2.429 s | **1.590 / 1.581 s** | 5.994 / 5.944 s |
| means, Δ (treatment − control) | 22.513 → 22.537 s, **−0.024 s** | 2.490 → 2.403 s, **+0.087 s** | 1.311 → 1.586 s, **−0.275 s** | 6.078 → 5.969 s, +0.109 s |

`full_ns` −0.019 s, `delta_ns` −0.008 s, `place_ns` −0.008 s, `group_ns` −0.001 s, `filesystem` −0.058 s, `content` −0.016 s.

**Withdrawn under its own falsifier.** `sql_ns` fell by 0.275 s against a predicted 0.22 s — the parse confirming itself — and **the operation did not fall** (−0.024 s, inside the window's own −0.514 s first-row-to-last drift). Three readings, in order of support: (1) **the saving is redistributed**, `sql_ns` −0.275 s against `commit_ns` +0.087 s and uncharged per-step work +0.055 s, which nearly cancel, and the arms separate in both buckets; (2) the `commit_ns` rise **may be a time effect rather than an arm effect** — A B B A makes `commit_ns` a U in time (2.485, 2.377, 2.429, 2.494) while `sql_ns` is a hump (1.279, 1.590, 1.581, 1.343), and two samples per arm cannot separate those; (3) what is **not** claimed is that caching makes stride10 slower — only that it does not make it faster, which was the bar. **Why removing a parse from the pack `UPDATE` adds time to the commit that follows it is not established.**

**The diagnostics the round leaves behind, both of which killed a hypothesis.** D4: the connection's statement cache is a **16-entry LRU** and two cached texts vary with the call, so one width over capacity turns a 108 ns hit into a **10,677 ns** miss (128 widths: 18,477 ns; capacity 192: 200 ns) — but a measurement-only counter over a real run shows **380,444 locator calls at 126 distinct widths with 99.0 % of them one identifier wide** and **44,334 object-insert calls at 68 widths with 99.6 % one wide**, so the hot entries stay resident and the misses are tens of milliseconds. **Text construction and cache thrash are both refuted** as explanations of the ≈6.5 µs per locator call that separates the in-run cost (10.00 µs) from the isolated one (3.488 µs) — that ≈2.5 s remains unexplained and belongs to `resolve_ns`, not `commit_ns`. D5: the pack `UPDATE` replayed with the product's parameter shape and a realistic body cycle (96 KB ↔ 262 KB, 15,000 appends per arm, three rounds interleaved) makes a cached statement **4,575 ns cheaper** than a fresh one (31,036 against 35,611 ns) and prices **growth at +17.8 µs per append** against a fixed body — so the parse saving is real in isolation and the whole-chain rewrite is the format's price, confirmed independently. **The D5 probe's source was deleted with the tree before it was copied to the campaign directory**; the slip is recorded in the report rather than papered over, and its raw measurements are retained.

Checks as run, on the reverted tree: `fmt --check` exit 0, `test --locked --workspace` **537 passed / 0 failed**, `clippy -D warnings` exit 0, `check_product_boundary.py` PASS over 175 files and self-tests 6/6 OK. Harness unchanged and its source seal `04bcfab5…` identical in every row; not re-run this round. No CI and no `tools/preflight.sh`. Not run: verification mode — every row is a diagnostic, admission `INELIGIBLE`, every budget class `NOT_RUN`. No cap promoted, no cross-window effect size, no cell dropped.

Production LOC: **25403 → 25403 (delta 0)**; core 25403 unchanged, reference 65417 unchanged, combined 90820. Method `tools/production_loc.py`, first parent `f3e84c073` against the final staged tree; the product tree is byte-identical to that commit, so the delta is zero by construction rather than by coincidence.

## L60 — #209 confirmation window: the kept treatment reproduces on `commit_ns`, and the operation effect still does not resolve (2026-09-21)

Status: Research; **diagnostic evidence, not release admission**. **No product line changed**; the tree remains byte-identical to `704580673` over `core/crates` and `crates`. [Report](../../0.1.7/evidence/stage-6-history-209-confirm-20260920T212625Z/README.md), [pre-declaration](../../0.1.7/evidence/stage-6-history-209-confirm-20260920T212625Z/PREDECLARATION.md), [raw runs](../../0.1.7/evidence/stage-6-history-209-confirm-20260920T212625Z/runs/), [analysis](../../0.1.7/evidence/stage-6-history-209-confirm-20260920T212625Z/analyze.py).

**Why.** L57's treatment was measured in one window and the machine's level then moved by more than the effect (`commit_ns` 1.81 → 2.49 s, stride10 16.5 → 22.5 s for identical work), so one window's contrast cannot carry it. This window re-runs it with no source edited: the kept round's **archived lever binary** (`106f181b…`) as the matched same-binary instrument and the **shipped binary** (`3a6c1c20…`), which the current tree rebuilds **byte for byte**. `collect.py` / `with_locks.py` are byte-identical to the retained ones and the harness seal is `04bcfab5…`, so no harness change invalidates the pair.

**Arm convention, stated because the first pass of this window got it backwards.** `LAYERFS_STORAGE_POLICY_REASSERT=0` makes the flag false, which **is the treatment**; **unset is the control**. That is the opposite of L59's lever, where `=0` restored the old behaviour. The mislabelling was caught because the shipped row landed inside the "control" band; every row's arm is now checked against its receipt's `extra_environment`.

**A B B A, then the shipped row.** Control `confirm-1a`/`confirm-4a` 17.778 / 17.706 s operation with `commit_ns` **2.058 / 2.037 s** (44.94 / 44.48 µs per append); treatment `confirm-2b`/`confirm-3b` 17.794 / 17.772 s with **1.952 / 1.951 s** (42.62 / 42.61 µs); shipped 17.550 s with **1.932 s** (42.18 µs). Pooled: operation 17.742 → 17.706 s (**−0.036 s, −0.20 %**), `commit_ns` 2.047 → 1.945 s (**−0.103 s, −5.01 %**), `resolve_ns` +0.057 s, `sql_ns` +0.004 s, `filesystem` +0.081 s. **The arms do not overlap in `commit_ns`** (2.037–2.058 against 1.951–1.952), the shipped row is below both treatment rows, and the saved Store is byte-identical in all five rows (`7ea2fe6ccf13bc5a…`) with `commits` 48,446 everywhere.

**What it confirms.** The bucket effect reproduces in sign and size against the keeping window: **−0.111 s (−5.90 %)** there against **−0.103 s (−5.01 %)** here, in a window whose level is 8 % higher on the operation and 5 % higher on `commit_ns` — ~2.2 µs of a 44.7 µs per-append commit price, which is what the treatment claimed. **What it does not confirm** is the operation-level effect: −0.237 s in the keeping window against −0.036 s here, both smaller than the second window's own −0.228 s first-row-to-last drift. **The change is kept for what it provably removes — 48,191 of 48,446 watermark statements and 45,794 ceiling statements, with a byte-identical Store — not for a claimed wall-clock number**, and that is now the recorded basis rather than one window's total.

Checks: no product change, so L57's checks stand, and the tree's identity to `704580673` is verified by `git diff --stat` (0 lines). One sample per row, fresh `--output`, both global flocks, quiet preflight (idle 74.9 % / 78.4 %), no deferrals. Not run: verification mode — every row is a diagnostic, admission `INELIGIBLE`, every budget class `NOT_RUN`.

Production LOC: 25403 → 25403 (delta 0). Documentation and evidence only; the instrument binaries are not product lines. Method: `tools/production_loc.py`, first parent `341926632` against the final staged tree.


## L61 — #209 the step's transaction read with SQLite's own instruments: the commit path is closed, and the round withdraws (2026-09-21)

Status: Research; **diagnostic evidence, not release admission**. **No product line is kept and no treatment is pre-registered**; the tree is byte-identical to `704580673` over `core/crates` and `crates`. [Report](../../0.1.7/evidence/stage-6-history-209-instr-20260920T213740Z/README.md), [plans and bytecode](../../0.1.7/evidence/stage-6-history-209-instr-20260920T213740Z/scratch/plans.txt), [micro-probe](../../0.1.7/evidence/stage-6-history-209-instr-20260920T213740Z/scratch/step-cost-arms.txt), [instrument source](../../0.1.7/evidence/stage-6-history-209-instr-20260920T213740Z/scratch/step_probe.rs.txt), [raw runs](../../0.1.7/evidence/stage-6-history-209-instr-20260920T213740Z/runs/), [analysis](../../0.1.7/evidence/stage-6-history-209-instr-20260920T213740Z/analyze.py). Continues L57–L60 on the same branch.

**The instrument, and what its clock is.** Two observers, both behind `LAYERFS_STORAGE_RCA_PROBE=1`, both removed before this commit. `SQLITE_TRACE_PROFILE` on the save's **own** connection gives, once per completed statement execution, the statement's runtime **and the live `sqlite3_stmt_status` counters of that handle** — §2.2 and §2.5 in one observer, with no call site in the SQL layer needing to know. `sqlite3_db_status`/`sqlite3_status` are sampled around every `COMMIT` and every save on that same connection. **SQLite's profile duration is `(sqlite3OsCurrentTimeInt64() - startTime) * 1000000` and `sqlite3OsCurrentTimeInt64` is the millisecond clock**, so the per-statement figures are a **straddle estimator of total execution time** — unbiased in the mean, `sqrt(observed ms)` standard error — and never a per-call time. That is stated because the first reading of these numbers as per-call times is wrong by two orders of magnitude.

**Q1 — the step's transaction writes 27.839 pages per commit on the product's own connection.** `SQLITE_DBSTATUS_CACHE_WRITE` = **1,348,771 pages** over 48,446 commits; the pack body handed to `insert_pack`/`append_pack` is 4,487,746,188 B (4.18 GiB) = **1,118,898 pages = 24.433 per append**; the excess is **229,788 pages (4.74/commit, +20.5 %)** and is fully accounted: page 1's change counter (48,446), the `objects` primary-key btree and the `objects_save` index (≈97,000), the free-list trunk (≈45,800) and the pack's own leaf (≈45,800) — ≈245,000 against 229,788 measured. **`CACHE_SPILL` is 0.** The file grows by 976 pages while 4.18 GiB are written and the free list *ends lower* than it started (46 → 38). **`COMMIT` executes 3 VDBE opcodes and costs 56.124 µs, so its price is the page writes and nothing else: the commit path is closed for good.**

**Q2 — no statement does more work than its rows and indexes require.** `SQLITE_STMTSTATUS` over the whole run reads **0 for `FULLSCAN_STEP`, `SORT`, `AUTOINDEX` and `REPREPARE` in every one of the 21 buckets**, and every plan is a seek (`scratch/plans.txt` prints all nineteen with their bytecode). The `EXISTS` subquery is a coroutine evaluated once for the one row the `UPDATE` matches and the read-scope subquery is `Once`-guarded; no plan uses `packs_save` or `objects_save` and none needs to; the pack `UPDATE` compiles to `OP_Delete` + `OP_Insert`; the object `INSERT` costs 2 btree insertions and 2 foreign-key parent existence checks, 96.8 opcodes per statement for the ≈1.17 rows a group seal carries. **The pack cache is not thrashing either**: 719,722 hits against 19,771 fetches over 227 distinct packs, a **97.33 % hit rate** with 560 bounded wholesale clears — that hypothesis dies here.

**Q4 — `BEGIN IMMEDIATE` is 9.738 µs of the step and 0.473 s of the run, and none of it is removable.** Its VDBE work is 5 opcodes, so the price is the pager's transaction open: a micro-probe replaying the same transaction shapes over a copy of the run's own Store prices `BEGIN IMMEDIATE`+`COMMIT` at 4.321 µs against **0.504 µs for a deferred `BEGIN`+`COMMIT`**, so **≈3.8 µs is the eager RESERVED file lock**. It cannot go: the step's first statement is the `SELECT next_pack_id` that must read the watermark *under the write lock* before a pack identifier may be allocated from it, so a deferred `BEGIN` could hand out a pack id another writer has already used.

**Q3 — the second A B B A was not run, and the answer is offered as mechanism instead.** Owner direction during the round was to keep it cheap, so the four-row window was started and stopped after one child had written a partial Store; it is retained and marked, consumes no sample, and the window is recorded `NOT_RUN`. Three mechanism measurements replace it: (1) **the commit's content cannot change with a parse** — six full-step micro-probe arms all report 46.14 pages written per commit, and in the product `commit_ns` is 99.8 % the `COMMIT` statement, whose work is 3 opcodes; (2) **the parse's price, with its own control** — two arms that parse the pack text per call read 121.556 / 120.468 µs against 112.938 / 109.919 / 109.467 µs for three arms of *identical code* at three positions in the round-robin, so the identical arms span 3.5 µs (3.2 %) by position alone while the parse arms sit **10.5–11.0 µs** above them, against a parse measured alone at 4.782 µs — parse **and finalize** are what the step pays; (3) **the split's arithmetic closes** — `sql_ns` is 2.005 s of wall against 1.340 s of engine time inside it, a 0.665 s gap, and the parse (0.219 s) plus the transient copy at bind (0.168 s) plus the per-call prepare/finalize (≈6 µs × 45,794) account for it to within 0.1 s. **Reading: a time effect within L59's window, not an arm effect** — L59's own `commit_ns` was a U in time while `sql_ns` was a hump, and its window drifted −0.514 s first row to last. Labelled an inference; it is not the window the handoff asked for and it does not re-open L59's treatment.

**The round withdraws, and hands the owner the format's real price.** The only removable term left in the step is **re-parsing and finalizing the pack `UPDATE` on every append (10.7 µs × 45,794 ≈ 0.49 s)** — L59's mechanism, whose saving this round's split places in `sql_ns` and the uncharged remainder and **never in `commit_ns`**, the term the handoff owns — plus **copying the body into the engine at bind (3.678 µs × 45,794 ≈ 0.17 s)**, which is not removable through rusqlite because `bind_parameter` hardcodes `SQLITE_TRANSIENT` and this lane does not patch a dependency. Everything else is the format or the contract. **The format proposal is priced at ≈3.07 s, not the ≈1.5 s in circulation**: 24.433 body pages per append at the measured page price is **2.13 s inside `commit_ns`** (73 % of the bucket) and the btree delete-and-insert of the same pages is **0.95 s inside `append_pack`**. A chunked pack, a pre-allocated row or an incremental-blob write would each end it and each moves the Store hash, so it is an owner's decision and is not shipped.

**Custody and equivalence.** Two diagnostic rows, one sample each, fresh `--output`, both global flocks, probe declared in `extra_environment`: `rca-instrument` (binary `844a95ee2cb3f2b9…`, 25.986 s / `commit_ns` 2.912 s / `resolve_ns` 6.906 s) and `rca-packcache` (`bb1966025019cb2a…`, 17.697 s / 1.924 s / 4.765 s). **They are two different binaries and no cross-binary effect size is quoted**; what they show is the handoff's own window problem — the same work at a **1.47×** difference in wall clock. **The saved Store is byte-identical in both** (`7ea2fe6ccf13bc5a…`) and **the engine accounting is identical to the page** (`cache_write` 1,348,771, spill 0, hit/miss 6,382,657/5,989, `page_count` 11,687 → 12,663): the work did not move, the machine did. Both rows are diagnostics — admission `INELIGIBLE`, every budget class `NOT_RUN` — and no row's time is offered as an effect.

**The second writer.** No product line changed, so the shipped step is the step L57/L60 measured: `multi_writer.rs` **5/5** and `pack_watermark.rs` **2/2** this round, against L57's retained step-width probe (three rounds of `solo`/`pair` over 24 MiB, **both writers finishing every round with zero `OwnershipUnavailable`**, pair p99 6.7–7.1 ms). The step-width probe is recorded `NOT_RUN` and cited rather than re-derived, because the tree it measured is the tree this round leaves behind.

Checks as run, on the shipped tree: `check_product_boundary.py` **PASS** over 175 files, its self-tests **OK**, `fmt --check` exit 0, and `cargo test -p layerfs-storage` over `multi_writer`/`pack_watermark`/`memory_bounds`/`visibility`/`persistence_failure` **34 passed / 0 failed**. **Not run and said so**: the full `--workspace` test and clippy runs and the 117-test harness suite — no product line and no harness line changed, so the tree is the one L57 checked (537 passed / 0 failed, clippy exit 0, harness 117 passed / 0 failed). No CI, no `tools/preflight.sh` (permanently retired). Not run: verification mode.

Production LOC: **25403 → 25403 (delta 0)**; core 25403, reference 65417, combined 90820. Method `tools/production_loc.py`, first parent `6221cad63` against the final staged tree; no product line changed, so the delta is zero by construction rather than by coincidence.

