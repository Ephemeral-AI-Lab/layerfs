# #151 execution continuation — remaining measurement work

Written 2026-09-15 by the implementing agent when handing the remaining work
over after I0–I6 and the B3 adapter completed. Read this together with the
frozen documents and the append-only ledger.

## 1. Where the work lives

| Item | Value |
|---|---|
| Worktree | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-v016` |
| Branch | `codex/v016-sandbox-local-experiment` |
| HEAD | `adfe869a3` (B3 adapter, harness-only) |
| v0.1.5 control commit | `6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13` |
| Main checkout | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` at `7b73c4b33 [main]` — untouched, do not modify |
| Working tree | clean; `benchmark-results/` is ignored |

Read in this order before doing anything:

1. `docs/roadmap/0.1/0.1.6/experimental-agent-handoff-prompt.md` — the original
   handoff: lanes, gates table, acceptance rules.
2. `docs/roadmap/0.1/0.1.6/experimental-implementation-pipeline.md` — ordered
   gates, fixed rules, failure/iteration policy, completion checklist.
3. `docs/roadmap/0.1/0.1.6/sandbox-local-snapshot-spec-and-plan.md` — the B3
   workload definition, resource gates, volatile-fsync contract.
4. `docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md` — append-only
   progress record (L0–L5). **Append to it; never rewrite earlier entries.**
5. `docs/roadmap/0.1/0.1.6/evidence/issue151-implementation-design.md` — what
   was designed and why.
6. `benchmark/fs-bench-pro/QUICKSTART.md` — build, selection, control,
   binary-archive/seal and comparability rules.

## 2. What is complete (candidate side)

Implementation (all product changes are inside `crates/`):

| Phase | Commit | Content |
|---|---|---|
| I0 | `a31e942a5`, `25002b8e5` | frozen docs, source reading, design |
| I1–I2 | `141244a3f` | sandbox-owned `LocalSpool` under `/snapshots/<id>/`; frozen-frontier COW capture |
| I1–I3 | `ab2a6f9cb` | LiveOwner local ownership, volatile fsync, snapshot service lane (`CAPTURE`/`SNAP_RECORDS`/`SNAP_READ`); host payload/facts opcodes and checkpoint install/freeze/resume removed |
| I3–I5 | `55b531bc5` | host pulls frozen input; `build_remote_candidate` reuses the single-worker canonical builder; `commit_workspace_candidate` publication; `complete_generation` with revision guards; remote dirty/charged state observed from the sandbox |
| B3 harness | `adfe869a3` | `local-snapshot-create-25000-onebyte-v1` family + 3-step lifecycle + per-Commit verification (harness-only) |
| ledger | `164efede7` | L4/L5 |

Correctness evidence already collected:

- `RUSTUP_TOOLCHAIN=1.85.1 tools/test-fast.sh` →
  `PASS full workspace native tests in 111s with 4 bounded jobs` (whole
  workspace, `--all-features`, 39 binaries).
- Focused checks: `fastcdc_shifted_stream` (1), `extent_model` (21),
  `small_candidate` (6), `small_chain` (3).
- New-route integration tests inside `layerfs-workspace`: full LiveOwner
  lifecycle with two successive Commits, post-capture writes surviving into the
  next Commit, published-content readback, k100 point-lookup case, peer
  isolation, immutable-base opcode rejection.
- Harness self-checks: `fs-benchmark-pro self-check`, workload
  `self-check`/`workspace-self-check`, 96 `shared/test_*.py` tests.

**Nothing has been measured.** No gate sample exists, and the candidate
binary/image are not built yet.

## 3. What remains, in order

```
1. candidate build ──▶ 2. control build ──▶ 3. smoke ──▶ B1 ──▶ B2 ──▶ B3 ──▶ report
```

### 3.1 Candidate build

```bash
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs-v016
python3 benchmark/fs-bench-pro/shared/runner.py --build-host
export LAYERFS_BENCH_IMAGE="$(python3 benchmark/fs-bench-pro/shared/runner.py --build-image)"
```

An earlier attempt was interrupted after ~1 minute of dependency compilation;
`benchmark-results/host-store/builds/incremental-1.85.1-release` exists and can
be resumed. Docker Desktop must be running. Record the build identities
(host binary sha256, image ID, source seals) in the ledger.

### 3.2 Control arm (sealed v0.1.5 product, comparable harness)

The harness delta between v0.1.5 and the candidate is **harness-only** and must
be identical in both arms:

```bash
# from the candidate worktree
git diff v0.1.5 HEAD -- benchmark/ docs/general/ > /tmp/v016-harness.patch

git -C /Users/yifanxu/Ephemeral-AI-Lab/layerfs worktree add \
  ../layerfs-v016-control 6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13
git -C /Users/yifanxu/Ephemeral-AI-Lab/layerfs-v016-control apply /tmp/v016-harness.patch
# then build host + image inside the control worktree exactly as in 3.1
```

Harness comparability to verify explicitly (QUICKSTART): the runner hashes
`runner.py`, `runtime.py`, `cold.py`, `verify-selected.py` for its harness
identity; the workload sources are compiled into the image. Compare the
recorded identities of the two arms and state the comparison in the ledger.
Product source is expected to differ — that is the measured variable.

### 3.3 Smoke before the gates

Run one small registered case through the full public path in the candidate
worktree (setup → workspace → workload → Commit → End → verification) to prove
the new route works end-to-end in Docker before spending gate samples, e.g. the
lowest `tiny-create`/`tiny-stat` tier with `--perf-fast --collection-mode`.
A smoke is not a gate sample; record it as a smoke with its identities.

### 3.4 Gates (strict order, one sample per case per arm)

| Gate | Case | Family | Flags that must match across arms |
|---|---|---|---|
| B1 | `tiny-create-500-mixed-v4` | `tiny_file_churn` | `--seed 1 --setup clone --perf-fast --collection-mode` |
| B2 | `tiny-bulk-create-500-mixed-v3` | `tiny_file_churn` | same |
| B3 | `local-snapshot-create-25000-onebyte-v1` | `local_snapshot` | `--seed 1 --setup fresh --perf-fast --collection-mode` |

Pattern per arm:

```bash
bash benchmark/fs-bench-pro/families/<family>/perf.sh \
  --case <case> --seed 1 --setup <clone|fresh> \
  --image "$LAYERFS_BENCH_IMAGE" --perf-fast --collection-mode \
  --product-timeout 600 --timeout 630 --setup-timeout 600 \
  --output <fresh-path-inside-benchmark-results>
```

The tier-500 extended allowances (600 s product / 630 s outer / 600 s
preparation) are explicitly authorized for the tier-500 pair; the runner
defaults (120/130) are otherwise in force. Keep `--collection-mode` reporting
separate from acceptance.

Verification is a **separate** run with `verify.sh`/`verify-selected.py` using
the exact source/input/image identities from the performance receipt. For B3
the runner additionally verifies each Commit's published state (C1/C2/C3) in
verify mode (`step-canonical-verification` records), plus the final canonical
and reopened-native checks and cleanup assertions.

Gates to enforce (from the pipeline document, do not restate differently):

- time: `candidate ≤ control + max(0.15·control, 3 ms)` per phase (exec,
  complete Commit, End/cleanup, whole workflow; each B3 cycle independently)
- CPU: `candidate ≤ control + max(0.15·control, 1 ms)`
- memory: peak-sum `≤ control + max(15%, 8 MiB)`
- 64 MiB aggregate accounted allocations; ≤ 8 MiB staging/transport buffers
- B1/B2 peak temporary backing `≤ control + max(15%, 1 MiB)`
- B3 absolute temporary backing peak `≤ 32 MiB`
- real public FUSE path only; one worker; no new dependency/lockfile changes;
  report canonical Store growth separately from temporary backing

### 3.5 Report

Update the ledger for every attempt (pass or fail) with command, identities,
raw receipt paths, metric values, gate decision and next action. Then write the
final result: applicability table (which result validates which
implementation), measured times/CPU/memory/storage, non-pausing and
multi-Workspace evidence, remaining limitations, and an adoption
recommendation. Do not claim release readiness or an unmeasured speedup.

## 4. Hard rules carried forward

- Failure at a gate: stop, diagnose that gate, make the smallest relevant fix,
  then one necessary new candidate sample. Never rerun an unchanged valid
  candidate for a better number; never delete an inconvenient receipt.
- Do not mix candidate revisions across cases in a single success claim; if a
  fix changes a previously passed case's path, collect one new sample there.
- Preserve failed/invalid receipts with exact identities.
- No product or dependency shortcuts, no budget increases, no timeout
  extensions, no workload changes inside `benchmark/` beyond the already
  committed B3 adapter.
- Do not modify the main checkout; it holds unrelated uncommitted docs.
- Keep the ledger append-only; keep cleanup checks in the report.

## 5. Known limitations to carry into the final report

1. **Retention amplification** — writes during an in-flight transfer retain COW
   copies and those nodes are skipped at completion, so hot files are
   re-transferred by the next Commit. Bounded by the 64 MiB allocation gate;
   unmeasured under a hostile write-during-transfer workload.
2. **No cross-snapshot diffing** — each Commit pays the full dirty-frontier
   walk and transfer; C3 in B3 re-transfers bytes the Store already holds
   (content-addressed dedup prevents new storage, not transfer).
3. **mmap dirty visibility is unsolved** — capture sees owned buffers only;
   kernel-only dirty state from mmap writes is out of scope per the spec's
   volatile contract and the pending conditional-mapping proposal.
4. **No durability/recovery by design** — snapshots are disposable; a lost
   snapshot fails that Commit rather than triggering repair.

## 6. First three commands

```bash
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs-v016
git log --oneline -3 && RUSTUP_TOOLCHAIN=1.85.1 tools/test-fast.sh   # confirm the sealed candidate still passes
python3 benchmark/fs-bench-pro/shared/runner.py --build-host          # resume the interrupted build
```
