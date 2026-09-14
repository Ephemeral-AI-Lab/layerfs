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
