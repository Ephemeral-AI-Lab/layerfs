# Product-ready LayerFS implementation handoff prompt

Copy the prompt below into the implementation owner's task. This document is a
non-normative execution wrapper. The normative architecture, requirements,
measurements, and terminal contract remain in
[`implementation-roadmap.md`](implementation-roadmap.md) and the product package
that it references.

---

You are the sole implementation owner for the complete product-ready LayerFS
roadmap in:

```text
/Users/yifanxu/Ephemeral-AI-Lab/layerfs
```

Your objective is to implement every in-scope requirement in:

```text
/Users/yifanxu/Ephemeral-AI-Lab/layerfs/poc-product-ready/04-product-readiness/implementation-roadmap.md
```

and continue until the exact frozen source earns an honest terminal
`PASS_PRODUCT_READY`. A plan, scaffold, partial route, green unit test, one
working presentation, zero-row readiness result, intermediate benchmark,
`REVISE`, or implementation-caused `NO-GO` is not completion.

## 1. Read before editing

Read `implementation-roadmap.md` completely. Then complete its section 0
pre-read in the specified order, including:

1. every normative document under `poc-product-ready/`;
2. the listed current Core, Engine, VFS, FUSE, Apple, SDK, and test sources;
3. only the historical PoC evidence routed to the owner being moved; and
4. the unchanged external Cloudflare benchmark and Docker/FUSE handoff.

Trace actual callers and product paths with `rg` before moving or changing a
shared function. Plans and historical claims are not evidence. Preserve
unrelated user changes and never use destructive Git commands.

If normative documents conflict, reconcile the package with the smallest
coherent documentation correction, record the reason, and immediately resume
implementation. Do not pause for routine authorization.

## 2. Fixed product model

Keep these boundaries unchanged:

```text
DurableStore
  central authenticated CAS
  durable Branches and pushed Operation history
  LayerStacks and immutable Layers
  authoritative merge/rollback/retention/backup/restore
          ^
          | explicit Fetch / Push only
          v
WorkingStore
  independent disk-backed SQLite database and Working CAS
  fetched roots and DurableTrackingRefs
  WorkingRecorded Branch/Operation history
  unpublished candidates and transfer state
          |
          v
one private OperationWorkspace per arbitrary operation
  direct logical | mount/FUSE | materialization/APFS
```

- A LayerStack is ordered immutable Layer history, not a Branch.
- An Operation is arbitrary filesystem activity. LayerFS records its final
  filesystem effect; it does not classify tools such as edit, shell, compiler,
  test, or package manager.
- One commit exists: `OperationCommit`, from an exact Branch head to a new
  `OperationVersion`.
- Two forks exist: an exact retained Layer to a top-level Branch, and an exact
  completed parent `OperationRecordRef` to a child Branch.
- Two merges exist: child to exact immediate-parent Branch head, and any Branch
  depth to its inherited originating LayerStack head.
- Cross-tree and non-parent Branch merges are forbidden.
- Concurrent Operations use isolated workspaces and exact expected heads. A
  stale loser preserves a recoverable candidate and returns `Conflict`; it
  never creates accepted history.
- Working-only work performs no Durable RPC. Durable visibility changes only
  through explicit Push. Fetch and Push are infrequent control/data-plane
  operations, never syscall-plane operations.
- WorkingStore and DurableStore are physically distinct SQLite databases with
  distinct `StorageId`s, even in a single-host deployment.

## 3. Fixed canonical and performance model

Preserve one platform-neutral implementation of:

```text
frozen FastCDC 8/16/32 KiB
  -> immutable canonical objects
  -> ObjectId-addressed CAS
  -> persistent COW extent/namespace/inode/metadata trees
  -> directly readable immutable RootId
```

Canonical bytes, ObjectIds, authenticated reads, legacy-read behavior,
expected-head publication, ambiguous-outcome reconciliation, and retained-root
integrity are not migration variables. Do not create a second filesystem
representation, platform-specific canonical model, benchmark-only product
path, or temporary `layerfs-fs` crate.

Maintain the roadmap's complexity and resource gates. In particular:

- direct range reads resolve the path and touch only intersecting extent-tree
  paths and returned bytes;
- logical count-changing edits path-copy changed spines and never process an
  unaffected suffix;
- mount/FUSE keeps bounded disk-backed dirty ranges and never hydrates a whole
  file or materializes/captures a backing workspace;
- materialization/APFS reports unavoidable cold/full/count-changing physical
  work honestly and reuses changed-path/clone/patch routes only where correct;
- no complete namespace, extent, object, or version inventories are retained in
  memory;
- largest request/product buffer, queues, RSS, file descriptors, connections,
  temporary paths, and residue satisfy the roadmap gates; and
- no syscall, close, `fsync`, tool exit, workspace finalization, or Working
  `OperationCommit` contacts DurableStore.

## 4. Implement the exact target structure

Follow the current-to-target move map and final tree in roadmap section 1. Move
each owner once, migrate callers, and remove compatibility forwarding and the
old active owner crates when their consumers are complete. The final active
workspace contains the roadmap's target crates and responsibilities:

```text
layerfs-core
layerfs-storage
layerfs-working-store
layerfs-durable-store
layerfs-sync
layerfs-workspace
layerfs-mount
layerfs-materialization
layerfs-sdk
layerfs-service
```

Do not preserve `layerfs-engine`, `layerfs-vfs`, `layerfs-fuse`, or `layerfs-os`
as parallel semantic owners after migration. Reuse proven implementations;
avoid redesigning an algorithm during an ownership move unless a focused
failure proves the algorithm itself is wrong.

Implement every ordered phase and ledger row in the roadmap, including:

1. dependency and workspace boundary freeze;
2. presentation/runtime extraction;
3. portable logical algorithms in Core;
4. one authenticated SQLite Storage substrate;
5. WorkingStore policy and recovery;
6. DurableStore authority, nested Branch history, leases, compaction, backup,
   restore, and exact merge/rollback rules;
7. resumable, authenticated Fetch/Push with known-present avoidance and exact
   head transactions;
8. isolated OperationWorkspace lifecycle, quiescence, custody, conflict
   preservation, cleanup, and crash recovery;
9. direct logical SDK route;
10. real Linux mount/FUSE route;
11. real Apple materialization/APFS route;
12. thin SDK and service orchestration;
13. removal of obsolete active crates and forwarding; and
14. exact-source correctness, performance, recovery, and resource closure.

Both presentations are mandatory. Do not treat a passing mount as permission
to defer materialization, or a passing APFS campaign as permission to defer the
mount.

## 5. Fast implementation loop

For each causal change:

```text
preserve the failure
  -> trace the shared root cause and every caller
  -> make the smallest in-scope repair
  -> run one focused check/test for the touched owner
  -> continue to the next unresolved roadmap item
```

Use 1 MiB and 10 MiB fixtures during iteration. Use at most 100 MiB only for
declared scaling rows. Do not run long exploratory campaigns, repeated full
workspace closures, repeated preparation, or unchanged reruns for noise.

Run one complete workspace format/check/test/Clippy/release closure and one
authoritative campaign only after source freeze. Keep passing evidence and
rerun only the population invalidated by a causal source, environment, or
fixture change.

You are encouraged to launch read-only subagents for independent review at the
roadmap's milestone boundaries. They may inspect actual source, tests, raw
rows, counters, receipts, and cleanup. They must not replace the implementation
owner, edit the same files concurrently, or turn review into a new architecture
process.

## 6. Linux mount/FUSE and external fs-bench

Use the exact unchanged script:

```text
/Users/yifanxu/Ephemeral-AI-Lab/cloudflare-computer-bench/upstream/script/fs-bench.sh
SHA-256 0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef
```

Do not patch, copy, wrap around, skip, or relabel its twelve scenarios. Preserve
the roadmap's exact `REPS=3`, `WARMUP=1`, target randomization, `/var/tmp` and
`/tmp` populations, Linux/arm64 Docker limits, `/dev/fuse`, capabilities,
network isolation, tmpfs, tracing state, custody, timing equations, raw rows,
and cleanup receipts.

The authoritative result must satisfy every frozen `/var/tmp` and `/tmp`
latency-ratio gate, resource gate, functional/restart gate, and terminal
cleanup gate in the roadmap. Existing candidate-015 evidence is a regression
reference only; it does not prove the target crate/storage/workspace split.

Benchmark the Working mount syscall plane independently from Fetch/Push and
Durable transactions. Any hidden materialization, capture, per-syscall Durable
RPC, unavailable-as-zero counter, or benchmark-specific bypass is a failure.

## 7. Apple materialization/APFS campaign

Use the exact macOS/APFS environment and campaign in the roadmap. Prove real
physical projection with native tools, real Bash and `mmap`, supported metadata
and hard links, quiescent capture, exact no-op, managed edits, changed-path
refresh, reopen, reconstruction, provenance filtering, explicit fallbacks,
cleanup, and retained Stage 1 gates.

Keep Verified and Trusted measurements and claims separate. Never call an APFS
different-length physical rewrite a local logical edit, and never attribute an
unavoidable native full fallback to extent-tree locality. Conversely, do not
route a logically local edit through a full reconstruction when the direct
canonical or safe incremental presentation path applies.

## 8. Working/Durable and recovery campaign

Prove with physically distinct databases and exact IDs:

- new and resumed Fetch with authentication and known-present avoidance;
- new, resumed, stale, conflict, and ambiguous Push;
- durable Branch creation and advancement;
- nested Branch continuation at arbitrary depth;
- exact immediate-parent `ChildBranchMerge`;
- any-depth inherited-origin `LayerStackMerge`;
- cross-tree/non-parent rejection;
- Working and LayerStack rollback with lease enforcement;
- source Branch survival after merge;
- crash/reopen/reconciliation at each publication boundary;
- retention, compaction, backup, restore, and fresh reconstruction; and
- separate syscall, Working commit, transfer, and Durable transaction timers.

No filesystem syscall or Working-only commit may be hidden inside transfer
time, and no transfer may be hidden inside filesystem-operation latency.

## 9. Continuation mandate

Do not stop or wait for user authorization for routine in-scope implementation,
tests, migrations, benchmark preparation, causal repairs, or performance work.
The user has already authorized completion of the roadmap.

When any check reports `REVISE` or `NO-GO`:

1. preserve the exact failing row, log, source ID, environment, and counters;
2. determine whether the cause is product code, fixture/preparation, evaluator,
   environment, or a concrete platform constraint;
3. find the shared root cause and smallest correct in-scope alternative;
4. update the working plan;
5. implement the repair;
6. run the smallest focused proof; and
7. resume the ordered roadmap toward terminal PASS.

An implementation-caused failure, missing route, slow path, compile error,
test failure, or available fallback is never a terminal impossibility. If a
platform constraint is real, preserve concrete host evidence, exhaust safe
in-scope routes, implement the best conforming route, and continue every
unblocked requirement. Never fabricate a PASS.

Never weaken thresholds after observation, delete or replace failed rows,
rerun unchanged source hoping for noise, label fallback as incremental, report
unavailable counters as zero, bypass durability/correctness, or add semantic
environment switches and benchmark-only production code.

## 10. Terminal evidence and final response

Before the final response, verify the complete terminal contract in roadmap
section 17 against actual committed source and release artifacts. The final
report must include:

1. frozen commit/source manifest and resulting crate/file tree;
2. concise phase/ledger completion table with implementation and focused-test
   evidence;
3. Linux mount/FUSE functional, restart, external fs-bench, complexity,
   resource, and cleanup results;
4. Apple materialization/APFS correctness, performance, complexity, resource,
   and cleanup results;
5. Working/Durable Fetch/Push, Branch/LayerStack history, conflict,
   reconciliation, recovery, compaction, backup/restore, and reconstruction
   results;
6. raw artifact paths and checksums, exact environments, executable/image
   custody, timer equations, counters, and preserved failure ledger;
7. final workspace format/check/test/Clippy/release status; and
8. terminal disposition for every required class:

```text
PASS_WORKING_MOUNT
PASS_WORKING_MATERIALIZATION
PASS_WORKING_OPERATION_COMMIT
PASS_DURABLE_BRANCH_PUSH_FETCH
PASS_DURABLE_MERGE_HISTORY
PASS_END_TO_END_FRESH_RECOVERY
PASS_PRODUCT_READY
```

Only return terminal `PASS_PRODUCT_READY` after every preceding class passes on
the same frozen target-architecture source with complete evidence and no active
compatibility or benchmark-only path. Until then, replan and continue.

---
