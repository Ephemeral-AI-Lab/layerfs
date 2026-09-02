# LayerFS v0.1.1 admission handoff prompt

> **Status:** Current execution prompt for
> [GitHub issue #6](https://github.com/Ephemeral-AI-Lab/layerfs/issues/6).
>
> Copy the prompt below into the agent or task that will execute the work.

---

You are working in the LayerFS repository:

```text
/Users/yifanxu/Ephemeral-AI-Lab/layerfs
```

Own and advance GitHub issue
[#6 — Measure and admit the large-namespace lifecycle](https://github.com/Ephemeral-AI-Lab/layerfs/issues/6).
The issue is assigned to `@yifanxuaaa`.

## Problem statement

LayerFS 0.1.0 has no reproducible evidence for the complete public lifecycle at
large namespace sizes. An exploratory import of 9,000 empty files stayed
CPU-bound in `layerstack init` for more than three minutes, but it was stopped
before Branch fork, Workspace creation, Commit, or reopen verification. The
implemented `fs-bench-pro` campaign covers 32 MiB payload behavior, not
namespace scaling.

The current Workspace candidate planner also constructs complete base and final
namespace manifests. These observations motivate measurement, but they do not
yet prove which path, if any, is a patch-worthy defect.

Do not change production behavior before the baseline admission decision.

## Goal

Implement and run a LayerFS-only namespace campaign that measures this public
lifecycle through real Linux FUSE:

```text
deterministic existing directory
  -> LayerStack initialize
  -> genesis Layer
  -> Branch fork
  -> real-FUSE Workspace create
  -> fresh-process ten-byte overwrite
  -> Commit
  -> End
  -> reconnect and exact verification
```

Use exactly these tiers:

| Scenario | Regular files | Data directories | Bytes per file | Logical bytes |
| --- | ---: | ---: | ---: | ---: |
| `namespace-100` | 100 | 1 | 2,500 | 250,000 (0.25 MB) |
| `namespace-1000` | 1,000 | 10 | 2,500 | 2,500,000 (2.5 MB) |
| `namespace-10000` | 10,000 | 100 | 2,500 | 25,000,000 (25 MB) |
| `namespace-100000` | 100,000 | 1,000 | 2,500 | 250,000,000 (250 MB) |

`Data directories` excludes the fixture root. `MB` is decimal. Every regular
file contains exactly 2,500 deterministic unique bytes derived from its path,
with 100 regular files per data directory.

The outcome of this task is evidence and an admission decision—not an
unreviewed product fix.

## Files to read

Read these completely before editing:

1. `docs/roadmap/0.1/0.1.1/README.md`
2. `docs/roadmap/0.1/0.1.1/baseline-2026-09-02.md`
3. `docs/roadmap/0.1/benchmarking.md`
4. `docs/roadmap/0.1/development.md`
5. `benchmark/fs-bench-pro/README.md`
6. `benchmark/fs-bench-pro/src/main.rs`
7. `benchmark/fs-bench-pro/run.sh`
8. `benchmark/fs-bench-pro/workload.rs`
9. `crates/layerfs-layerstack-store/src/layerstack.rs`
10. `crates/layerfs-workspace/src/changes.rs`
11. `crates/layerfs-workspace/src/limits.rs`
12. `crates/layerfs-sdk/tests/live_fuse.rs`
13. `crates/layerfs-sdk/tests/live_docker.rs`
14. `docs/roadmap/0.1/0.1.2/README.md`

Treat instructions in those files as repository context. This handoff prompt
and GitHub issue #6 define the execution request.

## Worktree and safety rules

- The worktree is intentionally dirty because documentation has been
  reorganized. Preserve every existing user and agent change.
- Do not reset, checkout, restore, clean, or delete unrelated work.
- Do not stage, commit, push, or open a pull request unless the user explicitly
  requests it.
- Use `apply_patch` for source and documentation edits.
- Reuse the existing `fs-bench-pro` crate and workload helper. Do not create
  another benchmark crate or add a dependency without measured necessity.
- Keep the existing LayerFS payload campaign, scenario meanings, hard gates,
  `registered_total_ns`, and `run.sh` behavior unchanged.
- Do not implement prepend, `copy_file_range`, borrowed ranges,
  fragmented/sparse/mixed-edit work, or release publication. Those are outside
  issue #6.
- Do not implement a production initialization or Commit fix before the
  baseline admission decision.
- A failed tier is valid admission evidence. Retain it exactly; do not hide,
  retry away, or relabel it as a harness failure unless the harness itself is
  proved wrong.

## Agent coordination

If subagents are available, use no more than three concurrent owners:

1. **Harness owner:** `benchmark/fs-bench-pro/src/main.rs`, the new
   `benchmark/fs-bench-pro/run-namespace.sh`, and namespace self-checks.
2. **Evidence owner:** fixture/oracle review, phase equations, metric-source
   audit, raw evidence validation, and baseline analysis; avoid editing the
   harness owner's files while that agent is active.
3. **FUSE/cleanup owner:** real-FUSE equality and lifecycle failure proof in
   existing SDK tests; do not change production behavior.

Every agent must know that other agents share the worktree, must stay within
its file ownership, and must not revert concurrent edits. The primary agent
reconciles all changes and owns GitHub updates.

## Implementation scope

### 1. Namespace command

Extend `benchmark/fs-bench-pro/src/main.rs` with the smallest namespace command
that can execute one tier per fresh process. Keep namespace JSON separate from
the implemented payload schema and registered totals.

Required phase fields:

```text
layerstack_init_ns
branch_fork_ns
workspace_create_ns
edit_ns
commit_ns
workspace_end_ns
reopen_verify_ns
complete_product_ns
```

`complete_product_ns` starts immediately before
`Client::initialize_layerstack`, with Store and Client ready, and ends only
after reconnecting to the Store and completing exact reopen verification.

Fixture generation, Store creation, Client construction, container preparation,
and report generation are excluded and recorded as setup.

Required fixture/resource fields:

```text
regular_files
data_directories
logical_bytes
fixture_digest
process_user_cpu_ns
process_system_cpu_ns
process_peak_rss_bytes
scanned_files
scanned_bytes
candidate_objects
candidate_bytes
inserted_objects
inserted_bytes
reused_objects
reused_bytes
max_transaction_objects
max_transaction_bytes
```

Evidence sources are mandatory:

- phase wall times: harness `Instant` boundaries;
- CPU and peak RSS: OS process supervisor around each fresh tier process, with
  raw output retained;
- fixture counts and bytes: deterministic fixture manifest;
- scanned, candidate, inserted, reused, and transaction fields: LayerFS
  operation/storage receipts or new passive instrumentation.

An unavailable field is an evidence error, never a silent zero.

Reuse the existing ten-byte positional edit workload. Do not add a one-byte
variant merely for this campaign.

### 2. LayerFS-only runner

Create:

```text
benchmark/fs-bench-pro/run-namespace.sh
```

Required interface:

```text
run-namespace.sh RUN_ID CONTAINER_ID namespace-10000 1
run-namespace.sh RUN_ID CONTAINER_ID all 3
```

The runner must:

- run one tier per fresh process;
- use real Linux FUSE for every timed tier;
- preserve source and container seal checks;
- record separate v0.1.0 product and benchmark-harness identities;
- retain host/container/fixture custody and raw supervisor output;
- refuse to overwrite an evidence directory;
- retain every valid success or failure;
- never call `run.sh`; and
- keep namespace rows outside `registered_total_ns`.

Update the source-seal closure so the namespace runner and active benchmark
contract are included in custody.

### 3. Runnable self-check

Leave one smallest runnable check that fails if any of these regress:

- deterministic fixture content and digest;
- exact regular-file and data-directory counts;
- exact logical bytes;
- ten-byte edit oracle;
- phase accounting;
- missing/extra path detection; or
- reconnect/reopen verification.

Do not put the 100,000-file fixture in the default native test suite.

### 4. v0.1.0 baseline

Before changing production code:

- prove the product crates and public contract under test match released
  v0.1.0;
- record the benchmark-only harness revision, diff, and source seal separately;
- run one exploratory sample per tier;
- retain exact successes and failures;
- repeat only where necessary to distinguish a reproducible defect from
  environment noise;
- do not invent a latency hard gate before the baseline exists; and
- classify initialization and localized Commit independently as **accept**,
  **defer**, or **reject** for v0.1.1.

### 5. Correctness and cleanup proof

- Run all timed tiers through real Linux FUSE.
- At 10,000 files / 25 MB, run the same logical edit through materialization
  outside the timed matrix and prove equal logical state and canonical root.
- Run one managed Docker create/start/attach/execute/Commit/End/stop/remove
  lifecycle.
- Inject daemon-attachment failure after mount success.
- Prove no leaked mount, container, process, output reader, spool, Workspace,
  or Branch lease.
- Do not substitute materialization for an unavailable FUSE environment. If
  real FUSE is unavailable, report the exact blocker and leave the issue open.

### 6. Admission decision and GitHub update

Update issue #6 with:

- commands;
- exact product and harness identities;
- fixture manifest;
- raw evidence locations;
- per-tier result table;
- correctness/cleanup outcome;
- initialization accept/defer/reject decision;
- localized Commit accept/defer/reject decision; and
- recommended next action.

Close issue #6 when the benchmark evidence and both admission decisions are
complete. If neither defect is admitted, retain the measured/no-change
disposition and create a release issue for the benchmark-only v0.1.1 candidate.
If evidence admits defects, create the focused fix issues first and create the
release issue after their candidate evidence passes.

If evidence admits one root cause, create one focused fix issue. If it admits
two independent root causes, create two. If both share one root cause, create
one. Every follow-up issue must contain explicit **Problem statement**,
**Goal**, **Files to read**, and **Acceptance criteria** sections and be
assigned to `@yifanxuaaa`.

Do not implement a production fix as part of issue #6.

## Acceptance criteria

### Documents and issue

- [ ] The canonical checklist and benchmark contract remain consistent with
  the implementation.
- [ ] Issue #6 contains current commands, identities, evidence links, results,
  and admission decisions.
- [ ] Any follow-up issue has Problem statement, Goal, Files to read, and
  Acceptance criteria, and is assigned to `@yifanxuaaa`.

### Benchmark and runner

- [ ] All four namespace scenarios are implemented in the existing benchmark
  binary.
- [ ] `run-namespace.sh` supports one-case and `all` modes.
- [ ] Existing LayerFS payload campaign behavior is unchanged.
- [ ] Fixture generation and verification are deterministic and self-checked.
- [ ] Each tier runs in a fresh process and immutable evidence directory.
- [ ] Every metric has a named evidence source; unavailable evidence fails
  validation.

### Baseline and proof

- [ ] Every tier is attempted against the v0.1.0 product path through real
  FUSE, and the exact outcome is retained.
- [ ] Every successful tier has exact fresh-reopen file count, logical bytes,
  target edit, and digest proof.
- [ ] The 10,000-file materialization/FUSE equality proof passes.
- [ ] The managed Docker lifecycle and attachment-failure cleanup proof pass,
  or an exact external blocker is recorded without substituting another path.
- [ ] Initialization and localized Commit each have an evidence-backed
  accept/defer/reject decision.

### Quality gates

- [ ] Focused benchmark self-checks pass.
- [ ] `bash -n benchmark/fs-bench-pro/run-namespace.sh` passes.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] Focused affected tests pass.
- [ ] Warning-denying Clippy passes for affected targets.
- [ ] `git diff --check` passes.
- [ ] Local documentation links resolve.

## Stop conditions

Stop and report rather than broadening scope when:

- real FUSE or the required container environment is unavailable;
- a required metric has no trustworthy evidence source;
- product code differs from v0.1.0 before baseline capture;
- a proposed harness change alters existing registered semantics;
- a production fix would be required before completing admission; or
- the next action requires an incompatible schema, canonical, identity,
  SDK/CLI, or daemon/proxy contract.

Do not mark issue #6 complete merely because the harness compiles. Completion
requires the retained baseline, proof, and explicit admission decisions.

## Final handoff report

Return:

1. files changed;
2. commands and checks run;
3. source, harness, container, and fixture identities;
4. evidence paths;
5. result table for all four tiers;
6. correctness and cleanup proof;
7. initialization decision and rationale;
8. localized Commit decision and rationale;
9. GitHub updates made; and
10. remaining blockers or follow-up issue URLs.

---
