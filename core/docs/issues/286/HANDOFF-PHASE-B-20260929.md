# #286 next-agent handoff: Phase B benchmark iteration

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Issued 2026-09-29. Parent [#284](https://github.com/Ephemeral-AI-Lab/layerfs/issues/284),
> benchmark sub-issue [#286](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286), draft [PR #285](https://github.com/Ephemeral-AI-Lab/layerfs/pull/285).
> Phase A is complete at product/test source `671f4a46f8f354cf764d3d1555529b73a5c939e0`;
> its documentation source is `4a4447db1abcbfccea0bcf1c79b9cc1d80ac2f04`.
> This prompt replaces the archived Phase A implementation-only assignment.
> No Phase B sample has been run by this publication.

## Paste this prompt to the next agent

You are the sole construction owner for **#286 Phase B** of #284. Organize,
evaluate and optimize the integrated Phase4.5 product, using benchmarks as the
evaluation points. **Do not restart Phase A or stop after the first three
families. Do not merge PRs, close issues or claim release admission.**

### 1. Establish your owned source

The integration checkout is:

```text
/Users/yifanxu/.codex/worktrees/phase45-active-integration/layerfs
branch: codex/issue284-phase45-integration
review base: codex/issue264-phase45
starting product/test: 671f4a46f8f354cf764d3d1555529b73a5c939e0
Phase A handoff: 4a4447db1abcbfccea0bcf1c79b9cc1d80ac2f04
```

First run `pwd`, `git status --short --branch` and `git rev-parse HEAD` in the
exact checkout. Read #284/#286/#285 and inspect current ancestry. This prompt's
publication adds documentation after4a; record that real head before editing.
Do not infer a directory/head from another checkout or reset to an older pin.

At publication, these unrelated pending files existed and were preserved:
`core/docs/architecture/README.md`, `core/docs/benchmark/fs-bench-pro/README.md`,
`core/docs/architecture/proposal/unified-telemetry-20260929.md`. Inspect their
current ownership; do not stage, stash, delete or revert them. If they remain,
obtain/reuse a suitable clean owned managed worktree at the published source
for sealed builds, and record its branch/path. Stage only your owned work.
Do not alter the old `layerfs-273-finalize` checkout, other owners' worktrees
or #260/#263/#269/#274 heads. Preserve any newer authorized work.

Read root/core `AGENTS.md`, benchmark-tree rules, benchmark/release/documentation
policies, [the Phase B plan](PHASE-B-PLAN-20260929.md),
[Phase A implementation report](../284/PHASE-A-IMPLEMENTATION-REPORT.md),
port manifest/evidence index beside it, and the source-pinned design/layout/
baseline links in the plan. Relevant architecture/component contracts win
over a planning proposal. Use worktree-local locked Cargo targets and scratch.

The Phase A dev correctness image is not a release/performance artifact.
Build/seal only needed release driver/verifier/image artifacts, preserving
repository-root ARMv8 flags and locked dependencies. Reuse a qualifying
compilation seal outside timers; do not use another worktree's Cargo target.

### 2. Execute the complete family order

1. **`init_namespace`** first: sole existing public SDK runner, default100 then
   1000 files; explicit10000/100000 tiers stay separately registered/visible.
2. **`history_retention`** second: stride10/3/1 with17/53/157 retained states.
   **Update and enforce the hard storage gate before qualification.**
3. **`workspace_write`** third: all nine append/dispersed/repeated x100/512/4097
   public SDK/POSIX-FUSE cells. **Correctness and the speed gate must meet.**
4. **`workspace_commit`**: retained clean/one-edit controls, full lowering,
   occupied headroom, live G1/G2, pins, failure/unknown/local-C5 and cleanup.
5. **`workspace_namespace`**: deep component access, inherited/resident/unrelated
   scaling, moves/replacements, listing and retained namespace selections.
6. **`workspace_mutations`**: prospectively specified mixed ordinary mutations,
   retained/live views and checked failure custody.
7. **`workspace_shell_package`**: existing refresh/package cases and registered
   broader many-file workloads; report the remaining #256 scope honestly.

**Those first three are the first wave. Continue the rest after all three pass.**
Implement family ownership under `core/benchmark/fs-bench-pro/families/` using
existing SDK/Rust backends and shared helpers. Create modules as implemented;
no empty scaffolding, second Init runner or duplicate benchmark framework.
Preserve historical IDs/profiles/receipts. Freeze complete case/arm order,
workload/oracle, gate versions and limits before each new selection.

### 3. Preserve the first-wave contracts

Init uses `host-direct-sdk-v2`, `core-sdk-init-fixture-v2`, seed1, locked
release driver/verifier from `target/release/examples/`, a15s complete command
and9.5s separate lite verifier. Declare its full-namespace/sampled-content
scope accurately. Current source-cache-uncontrolled rows are INELIGIBLE and
`performance_gate=NOT_FROZEN`; freeze a new metric/threshold/profile before
numeric PASS. Do not silently turn its command budget into a raw SDK target
or transplant the separate historical100000-file2.7s cold target.

Family1 follows the existing functional Init completion profile: correctness,
15s command/9.5s verifier budgets and cleanup must pass before the history
checkpoint. Report raw SDK time and numeric INELIGIBLE honestly. Do not add a
new numeric Init target as an ordering blocker; a numeric claim requires a
separate prospectively frozen contract.

For history, implement the omissions identified in the plan: the old backend
reports C2 allocation without enforcing the storage threshold, and its trace
does not persist real C5 retained history. The updated primary hard gate uses
actual `st_blocks*512` for **C2 Store + C5 History + any distinct required
persistent index**, counting embedded indexes only once. Use strict ceilings:

```text
stride10 /17 states: total allocated <49,344,512 B
stride3  /53 states: total allocated <64,024,576 B
stride1 /157 states: total allocated <83,947,520 B
```

Persist genuine retained state through public C5 APIs. Record per-owner and
compound allocated/apparent/logical bytes, canonical counts, pack bodies,
freelist and sidecars. Version the compound case/profile/receipt/gate
prospectively, preserving original C2-only `g1.o6-below-v016` and old IDs.
Enforce the gate in evaluator, verifier and report: missing/unreadable
measurement is INCOMPLETE; shared/reflink attribution is INELIGIBLE; a valid
exclusive total >=ceiling is FAIL. None can pass. Freeze deterministic C5
binding/incarnation/stack/branch/scope/policy, genesis and per-state Commit/
Layer/retention work and expected persisted row/root counts before collection.
Reopen/query every selected retained root through public C5 APIs; no raw SQL
fake history. Source schemas are C2 user_version10/six application
tables and C5 user_version1/seven; do not confuse SQLite schema_version with
the product version or copy the legacy four-table assertion.

Prove each root against independent pins, every state tree against the fixed
corpus oracle, all canonical counters, and deterministic10% bytes/endpoints
deduplicated by distinct object. Preserve stride3 canonical589423458/73476
and stride1 871588115/104705 pins; stride10 first-run pins cannot be zero or
silently reset. The current at-most64-path trace check is insufficient.
The backend lacks a complete independent expected-root ledger: identify and
freeze its approved sealed-reference or independent pin-generation source
before qualification; the candidate's own trace/replay cannot supply it.
Keep the InProcess/no-prepared-Store policy. Verifier ceilings10/20/30s and
prospectively declared history-specific complete-command ceilings apply;
generic15/25s and60s verifier defaults do not. Time remains diagnostic.
Stride1 is run-only; optimize shared mechanisms using stride10/3 evidence.

Workspace3x3 uses the fixed10MiB all-A master and exact one-byte append/
dispersed/repeated schedules. **All100/512 cells: complete command <=15s;
all4097 cells: <=25s.** External driver launch-to-exit includes lifecycle,
Mount, arbitrary Exec, Commit, Status and checked cleanup. Require known
successful Commit, independent full old/new bytes and the separate9s
verifier. Record intervals without inventing unavailable Host counters.
Cache eligibility has its own verdict; under-budget INELIGIBLE is not numeric
PASS. Do not add an invented speedup threshold or average away a failing cell.

Keep original #248 separated4097/8194-byte, native8192/default8MiB and actual
10240 custody outcomes distinct. New SDK8192 needs prospective registration.
Full64MiB lowering retains its three ordered edits,67108662 final bytes,
eight replacement bytes,64MiB quota,10s Stage and60s functional command.
Retain occupied2MiB Stage/4MiB Commit and arbitrary reordered Base-copy
proofs, full-byte old/G1/new/G2/pin oracles and physical ownership/refunds.

Include a separate deterministic live SDK stopping/refusal selection. A live
deadline, lost result or checked release is not that proof. Recheck affected
known-C1/local-C5 held leases, response Budget and token/metadata/cleanup
custody at final product source; record unaffected proof reuse explicitly.

### 4. Mandatory iteration loop A: every round is recorded

For every invocation/round, including setup failures, partial attempts,
FAIL/INELIGIBLE, verifier misses and cleanup failures:

1. Allocate a fresh append-only raw output path. Freeze committed measured
   product/harness/method and record source/tree/seals/build/image/host,
   fixture/oracle/profile/gate IDs, exact commands and limits.
2. Append an [experiment-log](EXPERIMENT-LOG.md) entry and committed round
   report with per-case/arm raw stats, independent correctness, hard-metric,
   numeric eligibility and cleanup verdicts, issues/cause evidence and the
   **next concrete fix or command**. Missing data is unavailable, never zero.
3. Diagnose the current family's actual shared cause from retained receipts.
   A new performance attempt requires meaningful changed product/harness/
   method identity; never replay an unchanged arm for a better time.

One round may cover one prospectively declared family selection; every
invocation still has its own row and evidence. Keep performance/verifier
walls separate. Run exploratory performance-only diagnostics only where the
family allows them; mandatory Init verification remains mandatory. A SKIPPED
verifier cannot satisfy correctness or trigger the earlier-family checkpoint.

### 5. Mandatory iteration loop B: commit and performance comment

Commit an actual patch before its official measurement. Then commit the
append-only round report/receipt manifest and push your owned branch. **Every
round gets a #286 issue comment** linking measured source, code/report commits,
family/case stats, limits/gates, failures, issues and next action. A failed
no-change round still gets a real report commit; no empty product commit.
The owner explicitly authorizes these comments and progress summaries in
#284/PR#285. Do not message unrelated people/tasks or merge/close automatically.

For every commit, compare the first parent and exact final staged tree using
`python3 tools/production_loc.py --json --root <snapshot>`, then confirm the
committed result. Record reference/Core/combined before/after and signed
delta in commit message and round handoff, including docs/test-only delta0.
Starting totals: reference65417, Core70022, combined135439. Do not substitute
the older donor's counts or count tests/docs/harness in production LOC.

### 6. Iterate fast and defer earlier-family sweeps

Do not rerun already-successful unchanged unit tests. After a patch, run only
the narrow changed-behavior check and current-family evaluation. **While
working on family n, do not run families1..n-1 after each patch.**

Only when n passes correctness **and** its required speed/storage gate,
freeze the checkpoint, then check1..n-1 once. Reuse an allowed exact-identity
PASS proof explicitly instead of resampling an unchanged arm. If an earlier
family regresses, preserve it, fix the shared cause, re-establish the affected
current-family gates, then check affected earlier families. Advance to n+1
after the checkpoint passes. INCOMPLETE/INELIGIBLE/NOT_RUN cannot substitute
for a mandatory numeric gate. Finish all seven families or name a precise
external stop with evidence; work independent engineering parts meanwhile.

At final changed source, perform required owning Core tests/examples/fmt/
Clippy/product-boundary checks once, split into commands under3min. Host
Clippy is `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked
--workspace --all-targets -- -D warnings`; preserve root ARMv8 flags, do not
replace them with `RUSTFLAGS=-D warnings`. Run
`python3 core/tools/check_product_boundary.py` and its tool self-tests.
No CI/preflight, third-party edits, inline product tests, fsync, new workers,
quota lifts or timeout lifts. Production caps remain999 and lib/mod200 lines;
algorithm/format/boundary changes update architecture in the same commit.

Additional host memory work stays deferred to#283; preserve actual
Budget/quota/physical/pin/refund safety. Respect symmetric cache declarations
and numeric eligibility without turning deferred observations into a repeated
probe campaign. Reuse setup/builds/images outside timers only. No warm credit,
best-of, shrinking workload, changed oracle or receipt rewriting. All original
failed, dirty and partial attempts remain in custody.

### 7. Final delivery

Publish the complete seven-family selection/status table, commit-linked round
index, correctness/gate/cleanup evidence and remaining issue/disposition map.
Local `core/target/` raw files are gitignored, not GitHub raw-evidence URLs;
publish compact receipts/manifests and source-pinned reports. Old nine numeric
rows remain INELIGIBLE; the separate earlier frozen control remains NOT_RUN
until executed. Do not claim32KiB/128KiB pinned SDK reads from16KiB success,
or pure-move Commit/#256/streaming from finite matrix success.

If an exact owner/profile/host decision is necessary, present the concrete
source-backed missing decision and next executable step; keep independent
engineering moving. Do not mark Phase B complete at a partial receipt or
promising prototype. Publish a reviewable merge recommendation only after
the actual scoped gates are satisfied or explicitly dispositioned. Keep
PR#285 draft and unmerged; merging needs separate owner instruction.
