# LayerFS namespace-v2 optimization execution prompt

> **Status:** Current execution handoff for GitHub issues
> [#7](https://github.com/Ephemeral-AI-Lab/layerfs/issues/7),
> [#9](https://github.com/Ephemeral-AI-Lab/layerfs/issues/9), and
> [#10](https://github.com/Ephemeral-AI-Lab/layerfs/issues/10), with the
> focused direct-admission continuation in
> [#11](https://github.com/Ephemeral-AI-Lab/layerfs/issues/11) and parent
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
- [#11 — Pipeline cold namespace initialization through bounded direct admission](https://github.com/Ephemeral-AI-Lab/layerfs/issues/11)
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

After every `REVISE` or `NO_GO`, immediately use available subagents to audit
the failed hypothesis, rank the next compatible bottleneck, and challenge its
correctness and resource accounting. The primary agent then reconciles one
revised plan and resumes implementation. Subagents are development reviewers,
not LayerFS product workers; their use does not authorize any runtime worker,
thread pool, or background service.

If real FUSE, the container environment, or a physical-I/O control is
temporarily unavailable, complete every safe non-environment-dependent task,
retain the exact blocker, and resume the same task when the environment is
available. Never relabel an unavailable proof as PASS.

## Non-negotiable boundaries

Do:

- measure the current critical path before changing it;
- make the smallest generic change that removes the proved bottleneck;
- preserve every valid success, slow result, and failed experiment;
- test through the existing public LayerFS, Store, SDK, real-FUSE, and Docker
  paths required below; and
- keep iterating against the binding throughput and resource gates until every
  terminal criterion passes.

Do not:

- add a pack format, packed fixture, lazy initialization, persistent cache,
  complete namespace manifest, Store schema or canonical-format change;
- add or increase a product worker, thread pool, background service, daemon,
  protocol tag, dependency, crate, benchmark family, runner, or public bulk
  operation;
- trade more CPU, memory, persistent or temporary storage, physical I/O, setup
  work, or warm state for a wall-time-only result;
- put exact correctness verification inside initialization timing, weaken an
  oracle, skip real FUSE, or substitute materialization for FUSE;
- restore a rejected mechanism without new evidence that invalidates its
  rejection;
- reset, checkout, restore, clean, or delete unrelated work; or
- stage, commit, push, open a pull request, publish, close an issue, or release
  without explicit user authorization.

The worktree is shared. Preserve every user and agent change and use
`apply_patch` for source and documentation edits.

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
four GitHub issues define the execution request.

## Current ground truth

- Namespace-v1 remains immutable historical evidence. Namespace-v2 is now
  implemented as the small-heavy 125/200/300/500-MB profile under the same
  four IDs with distinct schema/profile identities.
- The pre-direct retained baseline already contained bottom-up final namespace/inode
  import, existing bounded parallel root-directory import, all-reachable
  initialization, proven-empty Store membership bypass, 8,191-object / <4-MiB
  admission, compact inode pairs, sealed append-only segments, and
  initialization-only removal of the unused reference index. Do not
  reimplement or claim them as new wins.
- The pre-direct warm/uncontrolled-cache 100,000-file median is about 4.502
  seconds / 111.1 MB/s. Preparation is about 2.44 seconds; SQLite step and
  commit about 1.54 seconds; object-segment write and reread are about 647 MB
  each.
- Exact initialization-local metadata interning already proved 1.132 million
  canonical puts can fall to about 439,000 and pending duplicates from 708,845
  to 15,845. It was neutral alone because preparation and admission remained
  sequential.
- The rejected zero-capacity direct stream reached about 3.806 seconds with
  eight producers and 3.762 seconds with ten; ten added about one second of
  system CPU for only 1.2 percent wall improvement. Preserve that evidence and
  do not restore its rendezvous design.
- Workspace Create is demand-loaded and the retained warm median is about 15.4
  milliseconds at 100,000 files. Initialization issue #11 is the active
  performance blocker.
- Exact reopen now uses at most four per-node two-MiB proxy read-ahead entries
  and skips fully served responses. Retained product screens fetch exactly
  300/500 MB to serve 300/500 MB with zero unused bytes; remaining verifier
  time is a non-binding stretch miss and does not authorize a protocol tag,
  bulk read, prefetch, or weaker oracle.
- The retained all-tier init-only screen is
  `issue11-v3-retained-init-all-r001-20260903`: the first three tiers pass
  their absolute/rate and 1.30x/1.70x gates, while 100,000 records 3.040 s /
  164.5 MB/s. Incremental high-water is 53.35/59.28/66.13/102.51 MB, so every
  tier missed the former 32-MiB hard whole-process RSS gate even though
  explicit ownership remained below 10 MiB. The release gates are now 128 MiB
  for initialization HWM and 256 MiB for complete-lifecycle HWM.
  `SQLITE_DBSTATUS_CACHE_USED` grows
  from 988,416 bytes at T0 to 33,621,248 bytes at T1. The configured SQLite
  target must remain at most 64 MiB; the current 32-MiB setting is retained
  because it is the performance winner, while 64 MiB is only an allowance;
  rejected 12-MiB and 8-MiB cache trials prove the incompatible I/O/performance
  counter-trade under the unchanged rowid-plus-primary-key Store schema.
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

## Fast iteration and bootstrap policy

Optimize the development feedback loop as well as the product path when it is
measurably slow. Record this wall-time decomposition separately from product
metrics:

```text
edit -> incremental build -> focused test bootstrap -> fixture availability
     -> product sample -> evidence validation
```

Before adding optimization code, time these steps. If test start, process or
container bootstrap, fixture lookup, report validation, or another harness
step dominates iteration time, fix its generic root cause within the existing
crate, runner, and helpers. Reuse compiled artifacts and each immutable sealed
tier fixture across fresh product processes. Cache only build/setup artifacts
that are outside `T0..T7`; never reuse a Store, Client, LayerStack, Workspace,
product process, mutable fixture, or in-operation metadata state between timed
samples. Product bootstrap inside the public lifecycle remains in its assigned
phase; only harness bootstrap outside the lifecycle may stay outside product
timing.

Harness/bootstrap improvements must preserve all custody, seal, fresh-process,
failure-retention, and metric-source checks. Report their time separately and
never subtract them from, move product work out of, or relabel them as
initialization throughput. Do not add a dependency, worker, daemon, persistent
cache, correctness verifier inside initialization timing, or benchmark-only
product shortcut to accelerate the loop.

Use progressive validation so weak ideas fail cheaply:

1. Run the smallest focused correctness/resource check for the changed lane.
2. Run one unprofiled representative screen at the smallest tier that exposes
   the bottleneck; use 10,000 files for initialization pipeline changes unless
   counters prove only 100,000 exposes it.
3. Run one 100,000-file sample only after the screen is correct, within the
   applicable smaller-tier gate, and directionally improves the critical lane.
4. Repeat only enough to distinguish signal from noise while iterating.
5. Run three fresh-process samples for every tier and the full quality/proof
   matrix only when a candidate can plausibly reach terminal PASS.

Never rerun an unchanged passing suite during each inner loop. Rerun it when
its dependency surface changes and once for the terminal proof.

## Binding targets

| Scenario | Init maximum | Minimum init throughput | Create maximum | Commit maximum |
| --- | ---: | ---: | ---: | ---: |
| `namespace-100` | 0.416667 s | 300 MB/s | 15 ms | 10 ms |
| `namespace-1000` | 0.500 s | 400 MB/s | 18 ms | 10 ms |
| `namespace-10000` | 0.750 s | 400 MB/s | 22 ms | 10 ms |
| `namespace-100000` | 3.235294118 s | 153 MB/s and 30,600 files/s | 25 ms | 10 ms |

Binding adjacent initialization ratios through 10,000 files are at most 1.30x
and 1.70x. The prospective 100,000-file target is independent and includes the
authorized 10-percent release tolerance: at most 3.235294118 seconds, at least
153 MB/s, and at least 30,600 files/s. Never slow
the 10,000-file tier to manufacture a ratio pass. Keep 200 MB/s / 2.5 seconds
preferred and 250 MB/s / 2.0 seconds stretch; neither blocks release.
The first three floors are raised above the stale flat 200-MB/s gate because
the retained candidate already reaches 318.6, 450.3, and 418.0 MB/s. Preferred
non-binding throughput goals are 350, 500, 500, and 250 MB/s, respectively.

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
initialization CPU <=14.07 total CPU-seconds
explicit LayerFS-owned buffers <=10 MiB aggregate
configured SQLite connection-cache target <=64 MiB; retained setting =32 MiB
initialization whole-process incremental RSS <=128 MiB
complete-lifecycle whole-process incremental RSS <=256 MiB
swap = 0
OOM = false
```

Do not impose an OS memory limit during timing. Measure whole-process baseline,
peak, incremental RSS, and the aggregate ownership equation from the canonical
specification.

Use the fixed eight-producer, four-slab, 256-KiB/512-object candidate in issue
#11. Do not sweep queue sizes, retune per tier, shrink the 4-MiB transaction
batch merely to pass memory, add workers, or trade higher CPU/I/O for a
wall-only win. Change the fixed budget only when its measured occupancy or
blocking counters prove the specified bound cannot satisfy correctness or the
10-MiB aggregate limit.

## Required execution order

### 1. Reconcile the retained source

Begin from the committed retained candidate and its source/evidence identity.
Confirm that the rejected rendezvous stream, direct-tiny, directory-leaf,
filename-sort, path/open/fstat, reusable-CDC, generic hot-ID, and multi-row
`RETURNING` experiments are absent. Preserve their evidence; do not retry them.

### 2. Restore exact cold metadata interning

Use at most eight entries per existing producer keyed by exact `(InodeKind,
normalized mode, mtime seconds, mtime nanoseconds)`. Every process and Store
starts with an empty table; the first miss invokes the unchanged canonical
builder and later exact matches within the same operation reuse its root ID.
Prove cached/uncached canonical equality and all-unique bounded behavior.

Required 100,000-file diagnostic expectations are about 439,000 canonical
puts, at most about 16,000 pending duplicates, 99,000 exact hits, 2,000 misses,
and no material CPU or RSS increase.

### 3. Add coarse bounded direct admission

Use eight existing import producers. Each fills an owned slab capped at 256
KiB and 512 objects. Move slabs through a four-slot standard-library
synchronous channel to the calling thread, which remains the sole SQLite
owner. Carry one exact-dedup batch across every slab and directory under the
existing 4-MiB/8,191-object bounds.

At 100,000 files require zero object-segment write/read bytes, zero parent
payload rewrite/copy bytes, at most 2,200 slab handoffs, aggregate explicit
buffers at most 10 MiB, and no new worker or background task. Record queue
occupancy, blocked/idle time, active threads, context switches, CPU, RSS, and
physical I/O.

Path-independent objects may enter bounded admission while import continues.
Keep path-dependent structural records behind global cross-root hard-link
resolution before constructing or publishing the inode table, root, Layer, or
LayerStack. Do not add a second content scan.

### 4. Screen and measure

Run the focused checks whose dependency surface changed outside the timed
performance path. Run one 10,000-file screen; proceed when it meets the
400-MB/s floor, has no greater than 5 percent regression from the retained
median, and the targeted critical-lane counters improve. Then run one
100,000-file screen. Retain three fresh-process 100,000-file samples only after
that screen can plausibly meet every binding gate. Defer the unchanged full
equality, collision, hard-link, empty-segment, failure, reconnect, and cleanup
suite to the composite proof.

The prospective binding 100,000-file result, including the authorized
10-percent release tolerance, is at most 3.235294118 seconds, at least 153 MB/s
and 30,600 files/s, with at most 14.07 initialization
CPU-seconds, <=10 MiB explicit LayerFS ownership, <=128 MiB initialization
incremental HWM, <=256 MiB complete-lifecycle incremental HWM, the retained
32-MiB SQLite target under its 64-MiB ceiling, and no hidden storage, cache, or
worker trade.

### 5. Conditional SQLite A/B

Keep cached single-row insertion initially. Only when the combined result is
2.5--2.75 seconds and SQLite row step is still the critical lane may a fixed
128-row `INSERT ... ON CONFLICT DO NOTHING` statement without `RETURNING` be
tested. Keep the same transaction bounds and exact conflict-byte checks.
Retain it only if SQLite execution is at most 0.8 seconds and CPU, RSS, Store
bytes, physical I/O, and correctness do not regress.

### 6. Composite proof

After the 100,000-file target passes, run four fresh-process samples per tier
to retain three subsequent-sample medians, then set
`LAYERFS_NAMESPACE_RUN_COMPOSITE=1` so the canonical
runner—not an external proof manifest—executes all focused/quality checks,
materialization/FUSE equality, managed Docker lifecycle, injected post-mount
attachment failure, exact reconnect, and cleanup census. Update issues #11 and
#7, then #9, #10, and parent #6 with
identities, raw evidence, results, retained/reverted experiments, and terminal
disposition.

## Experiment discipline

For every hypothesis:

1. State the root-cause hypothesis and expected counters.
2. Leave the smallest focused check that fails before the change.
3. Change one variable.
4. Run focused correctness/resource checks.
5. Use the progressive screen above; do not run the full repeated matrix for a
   candidate that already fails a focused check or representative screen.
6. Retain exact success/failure, CPU, RSS, workers, copies, spool traffic,
   physical I/O, SQLite metrics, Store growth, and canonical result.
7. Retain only if it improves outside noise, no tier regresses more than 5
   percent, no resource load materially increases, and every contract passes.
8. On `REVISE` or `NO_GO`, preserve evidence, revert only the isolated
   mechanism when necessary, launch the bounded subagent audit, update the
   plan from the newly ranked critical path, and continue.

At the end of every 100,000-file miss, refresh the phase and ownership
breakdown, calculate the distance to the 3.235294118-second/153-MB/s binding
target, and report the 2.5-second/200-MB/s preferred outcome separately. Rank
the remaining lanes by recoverable wall time. Optimize the current critical
lane; do not keep tuning a lane after it leaves the critical path. A smaller
counter is not a win unless it advances a binding throughput, CPU, memory,
worker, I/O, or correctness gate.

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
SQLite object table/primary-key-index pages and bytes
SQLite object insert execution calls for any fixed-row A/B
Create bootstrap Store calls/rows/bytes/cache charge
small-file prefetch eligibility/bytes
anchor prefetch count/bytes
read-ahead requested/fetched/served/unused bytes
fixture/schema/cache/source/container identities
```

An unavailable required field is an evidence error, not zero.

### Post-timing SQLite write-path custody

After the product timestamp, open the exact completed Store read-only and
retain `dbstat` output for the `objects` table, its primary-key index, page
size/count, free-list pages, object rows, canonical bytes, and Store
amplification. Do not run `ANALYZE`, `VACUUM`, change pragmas, or combine
initialization-only values with a post-Commit Store state.

Record the read-only commands and raw output, including:

```bash
sqlite3 -readonly -header -column STORE.sqlite \
  'SELECT name, count(*) AS pages, sum(pgsize) AS allocated_bytes,
          sum(payload) AS payload_bytes, sum(unused) AS unused_bytes
     FROM dbstat GROUP BY name ORDER BY allocated_bytes DESC;'

sqlite3 -readonly STORE.sqlite \
  'EXPLAIN INSERT INTO objects(object_id, bytes)
   VALUES(zeroblob(32), zeroblob(1))
   ON CONFLICT(object_id) DO NOTHING;'
```

The expected current VDBE path is `NoConflict` plus `IdxInsert` into
`sqlite_autoindex_objects_1` and `Insert` into the rowid `objects` table.
Conflict payload reads are about 82 calls, 1,148 rows, 97 KiB, and 9--10 ms;
do not add a read cache, payload prefetch, reader worker, or larger read-ahead
without new contrary evidence.

Any attached profiler output is nonterminal diagnostic evidence. Retain:

```text
profiler tool/interval/duration
attached profiler result and unprofiled reference result
source commit and source seal
host identity
raw artifact path and SHA-256
exact permission or collection failure
```

Never convert call-stack samples into syscall counts. The current exploratory
profile took 6.103 seconds versus the unprofiled 4.502-second median and is
therefore unsuitable as a performance result. Its `/tmp` path is not durable
custody. `fs_usage` produced no data because elevated permission was
unavailable; record that blocker instead of reporting zero physical I/O.

## Acceptance criteria for terminal PASS

- [ ] Same namespace family/IDs/runner/`all`; no `repo-shape-*` family.
- [ ] Exact fixture class/count/anchor/directory/byte/digest equations pass.
- [ ] Generator and verifier perform one streaming content pass with <=1 MiB
  scratch and no complete file allocation.
- [ ] Large spilled-local, serial/parallel, dedup/collision, compact-inode,
  failure-atomicity, reconnect, and cleanup proofs pass.
- [ ] Every fresh process and Store starts with an empty metadata intern table;
  exact reuse is bounded to eight entries per producer and destroyed with the
  operation.
- [ ] Eight existing producers move at most four queued 256-KiB/512-object
  slabs to the calling thread as sole SQLite owner; no new worker exists.
- [ ] At 100,000 files, object-segment write/read, parent payload rewrite, and
  parent payload-copy bytes are zero and slab handoffs are at most 2,200.
- [ ] Read-only post-timing `dbstat` binds object-table, primary-key-index,
  page, row, canonical-byte, and amplification values to one exact Store.
- [ ] No initialization database-read cache, payload prefetch, reader worker,
  or larger read-ahead is added without a new measured payload-read hotspot.
- [ ] Any fixed-128 SQL A/B reports object-insert execution calls separately
  from submitted rows and uses a new result-schema identity.
- [ ] Every tier meets its scenario-specific init throughput and absolute
  target; adjacent ratios through 10,000 files are at most 1.30x and 1.70x,
  while the 100,000-file target remains independent.
- [ ] Every tier meets Create and Commit targets; Store-wide Create scans and
  anchor prefetches are zero.
- [ ] No new/increased product workers; aggregate explicit buffers <=10 MiB;
  initialization CPU <=14.07 seconds; initialization/product incremental HWM
  <=128/256 MiB; physical I/O, copy, and spool evidence has no hidden trade.
- [ ] Canonical bytes/IDs/roots, CDC, five-table Store, SDK/CLI,
  daemon/proxy/FUSE, transaction, publication, acknowledgement, and cleanup
  contracts remain valid.
- [ ] Exact real-FUSE reopen, stretch results, materialization equality,
  managed Docker, attachment-failure cleanup, focused tests, runner self-check,
  `bash -n`, formatting, warning-denying Clippy, `tools/test-fast.sh`,
  `git diff --check`, and documentation links pass.
- [ ] GitHub issues #11, #7, #9, #10, and #6 contain the applicable exact
  commands, identities, evidence paths, result tables, resource outcomes,
  retained/reverted decisions, and next action.

Only then report terminal **PASS**.

## Agent coordination

Use available subagents after every `REVISE` or `NO_GO`, and at major candidate
and terminal reviews. Use no more than three concurrent read-only or file-owned
reviewers:

1. initialization/storage ownership and canonical safety;
2. Workspace Create/read/FUSE/cleanup;
3. harness/evidence/specification validation.

Every agent must know the worktree is shared, stay within explicit ownership,
and never revert concurrent changes. Give each agent a distinct question so
they do not duplicate work. The primary agent continues useful independent
work while reviews run, reconciles their findings into one ranked plan, runs
the final gates, and owns GitHub updates.

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
