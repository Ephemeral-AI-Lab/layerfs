# LayerFS namespace-v2 optimization execution prompt

> **Status:** Current execution handoff for GitHub issues
> [#7](https://github.com/Ephemeral-AI-Lab/layerfs/issues/7),
> [#9](https://github.com/Ephemeral-AI-Lab/layerfs/issues/9), and
> [#10](https://github.com/Ephemeral-AI-Lab/layerfs/issues/10), with parent
> evidence in [#6](https://github.com/Ephemeral-AI-Lab/layerfs/issues/6).
>
> Copy the prompt below into the agent or task that will execute the work.

---
You are working in the LayerFS repository:

```text
/Users/yifanxu/Ephemeral-AI-Lab/layerfs
```

Own and advance the existing namespace-v2 benchmark and optimization issues:

- [#10 — Admit mixed-size namespace-v2 in the existing family](https://github.com/Ephemeral-AI-Lab/layerfs/issues/10)
- [#7 — Drive namespace-v2 initialization to 200 MB/s](https://github.com/Ephemeral-AI-Lab/layerfs/issues/7)
- [#9 — Demand-load namespace-v2 Workspace Create and bounded reads](https://github.com/Ephemeral-AI-Lab/layerfs/issues/9)

All are assigned to `@yifanxuaaa`. Issue #6 owns the retained namespace-v1
baseline and admission history.

## Terminal objective

Implement, measure, optimize, and prove namespace-v2 through the existing
LayerFS-only namespace family until the task reaches **terminal PASS**.

Do not stop merely because an experiment is rejected, needs revision, misses a
target, exposes a new compatible bottleneck, or receives a review disposition
of `NO_GO` or `REVISE`.

State meanings are binding:

```text
CONTINUE:
  current isolated hypothesis is still being implemented or measured

REVISE:
  preserve the evidence, revert only the isolated rejected mechanism when
  necessary, update the hypothesis/specification, and continue

NO_GO:
  do not land that mechanism; preserve why, select the next ranked compatible
  mechanism, and continue

PASS:
  every required fixture, correctness, performance, CPU, memory, worker,
  cleanup, evidence, documentation, and GitHub gate below passes
```

`REVISE` and `NO_GO` are never terminal outcomes. Replan from the measured
root cause and proceed. Do not broaden into incompatible contracts merely to
avoid a rejected experiment.

If real FUSE, the container environment, or a physical-I/O control is
temporarily unavailable, complete every safe non-environment-dependent task,
retain the exact blocker, and resume the same task when the environment is
available. Never relabel an unavailable proof as PASS.

## Canonical specification

Read this completely before editing:

1. `docs/roadmap/0.1/0.1.1/namespace-optimization-spec.md`

Then read completely:

2. `docs/roadmap/0.1/0.1.1/README.md`
3. `docs/roadmap/0.1/0.1.1/baseline-2026-09-02.md`
4. `docs/roadmap/0.1/benchmarking.md`
5. `benchmark/fs-bench-pro/README.md`
6. `benchmark/fs-bench-pro/src/main.rs`
7. `benchmark/fs-bench-pro/run-namespace.sh`
8. `benchmark/fs-bench-pro/workload.rs`
9. `crates/layerfs-content/src/file/cdc/gear.rs`
10. `crates/layerfs-content/src/file/rope/build.rs`
11. `crates/layerfs-content/src/filesystem/apply.rs`
12. `crates/layerfs-content/src/filesystem/change.rs`
13. `crates/layerfs-content/src/tree/directory/edit.rs`
14. `crates/layerfs-content/src/tree/inode/table.rs`
15. `crates/layerfs-layerstack-store/src/layerstack.rs`
16. `crates/layerfs-layerstack-store/src/objects.rs`
17. `crates/layerfs-layerstack-store/src/workspace.rs`
18. `crates/layerfs-layerstack-store/src/telemetry.rs`
19. `crates/layerfs-workspace/src/cow_tree.rs`
20. `crates/layerfs-workspace/src/lifecycle.rs`
21. `crates/layerfs-workspace/src/projection.rs`
22. `crates/layerfs-fuse/src/proxy_client.rs`
23. `crates/layerfs-sdk/tests/live_fuse.rs`
24. `crates/layerfs-sdk/tests/live_docker.rs`

Treat instructions in those files as repository context. This prompt and the
three GitHub issues define the execution request.

## Current ground truth

- The harness currently implements namespace-v1: uniform 2,500-byte files and
  0.25/2.5/25/250 MB tiers.
- Namespace-v2 is specified but not implemented or measured. Never relabel or
  pool namespace-v1 evidence as namespace-v2.
- The current candidate already contains bottom-up final namespace/inode
  import, existing bounded parallel root-directory import, all-reachable
  initialization, proven-empty Store membership bypass, 8,191-object / <4-MiB
  multi-row admission, and initialization-only removal of the unused reference
  index. Do not reimplement or claim them as new wins.
- Portable mode/mtime direct single-chunk construction exists locally and has
  focused canonical-equality coverage, but native tiny-file wiring and sealed
  namespace-v2 A/B evidence do not exist.
- Current Workspace Create still performs the proved eager Store-wide
  small-object cache scan.
- Current exact verification retains complete file vectors; this is invalid
  for 100-MB anchors and the <=10-MiB explicit-buffer contract.
- Current actual product read-ahead is 16 MiB; namespace-v2 requires <=8 MiB
  actual read-ahead while keeping the compatibility maximum unchanged.
- A fresh audit found that worker-local spill transfer may expose unsealed
  pending bytes before `merge_prevalidated()` reads them. This correctness
  blocker must be proved/fixed before performance timing with anchors.
- Current dirty candidate protocol extensions are not a dependency or new
  authorization for this work. Add no daemon/proxy/FUSE request or response
  tag under these issues.

## Namespace-v2 fixture contract

Use the same family, scenario IDs, runner, `all`, real-FUSE lifecycle, and
registered-total exclusion:

| Scenario | Files | Directories | Logical bytes | 100-MB anchors |
| --- | ---: | ---: | ---: | ---: |
| `namespace-100` | 100 | 1 | 125,000,000 | 1 |
| `namespace-1000` | 1,000 | 10 | 200,000,000 | 1 |
| `namespace-10000` | 10,000 | 100 | 300,000,000 | 1 |
| `namespace-100000` | 100,000 | 1,000 | 500,000,000 | 2 |

Among non-anchor files:

| Scenario | Empty | Tiny | Small | Medium | Anchors |
| --- | ---: | ---: | ---: | ---: | ---: |
| `namespace-100` | 1 | 78 | 15 | 5 | 1 |
| `namespace-1000` | 10 | 789 | 150 | 50 | 1 |
| `namespace-10000` | 100 | 7,899 | 1,500 | 500 | 1 |
| `namespace-100000` | 1,000 | 78,998 | 15,000 | 5,000 | 2 |

Relative allocation weights are tiny `1..8`, small `32..256`, and medium
`1024..8192`. They are weights, not output byte-range claims. Use the exact
midpoint-quantile and checked largest-remainder equations in the canonical
specification.

Fully materialize unique path-derived bytes. Do not use sparse files, hard
links, reflinks, clones, repeated payloads, or a live repository scan. Keep 100
files per data directory and place two anchors in different directories. The
ten-byte edit targets a deterministic non-anchor file.

Namespace-v2 uses a versioned fixture/profile and result schema within the
same benchmark family. Historical namespace-v1 identities remain immutable.

## Fixture and oracle efficiency

Preparation is `O(files + logical bytes)` and outside LayerFS timing but is
retained as evidence:

```text
calculate/validate compact plan
-> create each directory once
-> open every file once
-> generate/write/hash every byte once
-> retain compact digest records only
-> atomic publish
-> reuse fixture across fresh product processes and Stores
```

Required bounds:

```text
generator scratch <=1 MiB
verifier scratch <=1 MiB
complete file Vec allocations = 0
post-generation content rereads = 0
per-file fsyncs = 0
```

Default setup is single-threaded. Setup-only concurrency is allowed only after
measurement proves setup is a blocker and cannot change product worker counts
or product claims. Record first-use and warm cache states separately; never
pool them into one median.

## Binding targets

| Scenario | Init maximum | Minimum init throughput | Create maximum | Commit maximum |
| --- | ---: | ---: | ---: | ---: |
| `namespace-100` | 0.625 s | 200 MB/s | 15 ms | 10 ms |
| `namespace-1000` | 1.000 s | 200 MB/s | 18 ms | 10 ms |
| `namespace-10000` | 1.500 s | 200 MB/s | 22 ms | 10 ms |
| `namespace-100000` | 2.500 s | 200 MB/s and 40,000 files/s | 25 ms | 10 ms |

Preferred adjacent initialization ratios are 1.60x, 1.50x, and 1.67x. No
final ratio may exceed 2.0x.

At 100,000 files:

```text
non-Attach Create <=10 ms
Store-wide Create scans = 0
localized Commit reads anchor payloads = 0
```

Stretch targets are exact reopen <=7 seconds and complete product <=10
seconds. They never authorize a weaker oracle.

## CPU, memory, and worker contract

```text
new product workers = 0
product worker ceiling increase = 0
explicit LayerFS-owned buffers <=10 MiB aggregate
preferred incremental RSS <=16 MiB
hard incremental RSS <=32 MiB
swap = 0
OOM = false
```

Do not impose an OS memory limit during timing. Measure whole-process baseline,
peak, incremental RSS, and the aggregate ownership equation from the canonical
specification.

When a buffer/backpressure mechanism changes, measure 6/8/10-MiB candidates at
10,000 and 100,000 files. Freeze one budget for every tier: the smallest within
5 percent of the fastest correct candidate that still reaches 200 MB/s. Do not
retune per tier, shrink the 4-MiB transaction batch merely to pass memory, add
workers, or trade higher CPU/I/O for a wall-only win.

## Required execution order

### 1. Large spilled-local correctness

Create a focused case with two top-level directories, one exact 100-MB file,
mixed empty/tiny/small files, guaranteed local spill, and a nonempty pending
tail. Make transfer fallible and explicitly seal pending bytes before read.
Require serial/parallel roots and complete canonical maps to match, fresh Store
reopen, and zero temporary-spool leak.

Do not proceed to namespace-v2 performance timing until this passes.

### 2. Implement and freeze namespace-v2

Implement exact planner equations, role permutation, streaming content,
streaming verifier, digest profile, schema version, manifest, self-check,
runner/report validation, and immutable fixture reuse. Run one bridge proof that
keeps namespace-v1 evidence interpretable.

### 3. Establish the revised baseline

Run the exact namespace-v2 fixture against the current product path. Retain
first-use/warm cache state, native source control, source/spool/SQLite I/O,
CPU, RSS, workers, copies, objects, transactions, Store growth, cleanup, and
exact reopen. Do not optimize against namespace-v1 numbers after this point.

### 4. Measure native tiny-file construction

Wire direct single-chunk construction for known nonempty files below 8,192
bytes. Open once, read the exact observed size, perform one trailing read to
detect growth, fail on early EOF, and retain generic CDC for empty, uncertain,
or larger files.

### 5. Measure fitting directory leaves

Sort/encode one final leaf only when it fits. Prove complete canonical equality
at 0, 1, 100, largest fitting, first overflow, and the 1,000-entry fixture-root
fallback.

### 6. Replace amplification with composite sealed segments

Existing workers return small sealed-segment descriptors. Admission consumes
each segment directly once and releases it. Memory segments move owned bytes;
file segments stay file-backed. Parent payload-spool rewriting and all-result
retention are zero. Global hard-link validation completes before parallel
structural objects can become durable.

### 7. Preserve exact global dedup and collision behavior

Dedup within bounded batches and use an exact database/disk-backed authority
across segments and nonempty Stores. Check same-ID/different-byte collisions.
Never retain the complete ID set in memory or linearly rescan a spill file.

### 8. Stream compact inode insertions

Encode finalized records early, retain compact `(InodeId, record ObjectId)`
pairs, keep mutable records only until hard-link counts settle, preserve
insertion order, and prove every canonical inode-table object.

### 9. Move-only admission and reusable scratch

Move canonical bytes through one bounded producer/admission owner. Reuse
bounded CDC scratch on existing workers and prove reset after success/failure.

### 10. Isolate SQLite A/Bs

Only after ownership is fixed, test bounded batch ordering, statement reuse,
and multi-row versus cached single-row execution one at a time. Preserve
4-MiB/8,191-object batches, collision checks, visibility-last publication,
schema, and reconnect semantics.

### 11. Demand-load Workspace Create and bounded reads

Remove eager Store-wide cache loading. Cache only requested authenticated
objects. Reuse the decoded root, traverse root metadata once, retain bootstrap
metrics, and batch cache lookups outside Store-I/O locks.

If reads regress, prefetch only reachable files <=64 KiB, <=128 IDs per Store
batch, <=512 KiB per directory, and <=8 MiB per SnapshotReader. Anchor prefetch
count/bytes are zero. Actual large-file read-ahead is <=8 MiB without changing
the protocol maximum; verifier scratch is <=1 MiB.

### 12. Composite proof

Run three valid fresh-process samples for every tier, all focused/quality
checks, the 10,000-file materialization/FUSE equality proof, managed Docker
lifecycle, injected post-mount attachment failure, exact reconnect, and cleanup
census. Update issues #7, #9, #10, and parent #6 with identities, raw evidence,
results, retained/reverted experiments, and terminal disposition.

## Experiment discipline

For every hypothesis:

1. State the root-cause hypothesis and expected counters.
2. Leave the smallest focused check that fails before the change.
3. Change one variable.
4. Run focused correctness/resource checks.
5. Run three 100/1,000/10,000 samples; run 100,000 only after they pass.
6. Retain exact success/failure, CPU, RSS, workers, copies, spool traffic,
   physical I/O, SQLite metrics, Store growth, and canonical result.
7. Retain only if it improves outside noise, no tier regresses more than 5
   percent, no resource load materially increases, and every contract passes.
8. On `REVISE` or `NO_GO`, preserve evidence, revert only the isolated
   mechanism when necessary, update the plan, and continue.

Do not retry away a valid slow or failed sample. Do not bundle independent
hypotheses. Do not accept a result that is faster only because work moved into
setup, cache warm-up, a background task, increased workers, larger memory, or
weaker verification.

## Required metrics

Retain every metric in the canonical specification, including:

```text
phase wall, CPU, whole/incremental RSS, workers
source opens/metadata/reads/bytes
single-chunk/streaming files and CDC scratch
canonical encode/hash/copy bytes
every explicit ownership component and aggregate peak
sealed segment/spool calls/bytes/passes
unique/duplicate/collision counts
SQLite prepare/insert/commit/transactions/rows/bytes/pages
Create bootstrap Store calls/rows/bytes/cache charge
small-file prefetch eligibility/bytes
anchor prefetch count/bytes
read-ahead requested/fetched/served/unused bytes
fixture/schema/cache/source/container identities
```

An unavailable required field is an evidence error, not zero.

## Acceptance criteria for terminal PASS

- [ ] Same namespace family/IDs/runner/`all`; no `repo-shape-*` family.
- [ ] Exact fixture class/count/anchor/directory/byte/digest equations pass.
- [ ] Generator and verifier perform one streaming content pass with <=1 MiB
  scratch and no complete file allocation.
- [ ] Large spilled-local, serial/parallel, dedup/collision, compact-inode,
  failure-atomicity, reconnect, and cleanup proofs pass.
- [ ] Every tier meets 200 MB/s and its absolute init target; adjacent ratios
  <=2x.
- [ ] Every tier meets Create and Commit targets; Store-wide Create scans and
  anchor prefetches are zero.
- [ ] No new/increased product workers; aggregate explicit buffers <=10 MiB;
  preferred/hard incremental RSS, CPU, physical I/O, copy, and spool gates
  pass without hidden trade.
- [ ] Canonical bytes/IDs/roots, CDC, five-table Store, SDK/CLI,
  daemon/proxy/FUSE, transaction, publication, acknowledgement, and cleanup
  contracts remain valid.
- [ ] Exact real-FUSE reopen, stretch results, materialization equality,
  managed Docker, attachment-failure cleanup, focused tests, runner self-check,
  `bash -n`, formatting, warning-denying Clippy, `tools/test-fast.sh`,
  `git diff --check`, and documentation links pass.
- [ ] GitHub issues contain exact commands, identities, evidence paths, result
  tables, resource outcomes, retained/reverted decisions, and next action.

Only then report terminal **PASS**.

## Worktree and safety rules

- Preserve every existing user and agent change.
- Use `apply_patch` for source/documentation edits.
- Do not reset, checkout, restore, clean, or delete unrelated work.
- Do not add a dependency, crate, benchmark family, runner, product worker,
  background service, protocol tag, Store schema, canonical format, packed
  fixture, physical object packing, or public bulk API.
- Do not stage, commit, push, open a pull request, publish a release, or close
  issues unless the user explicitly requests it.
- A failed tier or rejected experiment is valid evidence. Retain it exactly.

## Agent coordination

If subagents are available, use no more than three concurrent read-only or
file-owned reviewers:

1. initialization/storage ownership and canonical safety;
2. Workspace Create/read/FUSE/cleanup;
3. harness/evidence/specification validation.

Every agent must know the worktree is shared, stay within explicit ownership,
and never revert concurrent changes. The primary agent reconciles changes,
runs the final gates, and owns GitHub updates.

## Final handoff report

Return:

1. terminal state and why;
2. files changed;
3. commands and checks;
4. source/harness/container/fixture/schema/cache identities;
5. raw evidence locations;
6. per-tier phase, throughput, file-rate, CPU, RSS, worker, buffer, I/O,
   object, transaction, Store-growth, and cleanup table;
7. every retained/reverted experiment and its evidence;
8. canonical/correctness/large-anchor/reconnect result;
9. GitHub updates and issue state; and
10. any external blocker that remains incompatible with terminal PASS.

Do not return `NO_GO` or `REVISE` as the final answer. Replan and continue until
the terminal PASS criteria are actually satisfied.

---
