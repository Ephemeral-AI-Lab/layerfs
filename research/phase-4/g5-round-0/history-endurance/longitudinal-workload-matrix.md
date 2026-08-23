# G5 longitudinal workload matrix

Status: **prospective research contract; no G5 measurement authority**.

## Question

Single-operation speed is insufficient for a CAS + CDC + COW system. The
benchmark must establish whether the current operation remains correct and
bounded after a long sequence of prior operations:

```text
Does operation X remain byte-correct, identity-correct, fast, bounded,
durable, and recoverable after N edits, reopen events, materializations,
failures, retained roots, and garbage collection?
```

## Dimensions

The full Cartesian product is prohibited. Each axis is isolated first, then
one large-file interaction row checks that the conclusions compose.

| Axis | Values |
|---|---|
| File size | 1 MiB mechanism/history; 10 MiB mapping behavior; 100 MiB primary sentinel |
| Revision count | 1, 10, 100, 1,000 checkpoints |
| Edit shape | same-size; insert; delete; append; truncate; full replacement; no-op/revert |
| Edit size | 1 byte; 4 KiB; 64 KiB; 1 MiB where applicable |
| Edit position | early; middle; late; deterministic uniform random; deterministic 80/20 hot set |
| Authority lifetime | same-open; reopen every operation; reopen every 10/100 operations; final fresh process |
| Materialization cadence | never; every operation; every tenth operation; final revision only |
| Read cadence | selected ranges; every tenth operation; final full read; selected historical roots |
| Retention | latest only; sparse checkpoints; all roots; two branches |
| Failure | no fault; cancellation; lost acknowledgement; substitution; corrupt/missing state; interrupted GC |

## Required workflows

| ID | Sequence | Primary risk | Checkpoints |
|---|---|---|---|
| H01 | Create -> edit the same byte `N` times | Latency/resource growth despite stable locality | 1/10/100/1,000 |
| H02 | Create -> deterministic random same-size edits | Mapping fragmentation and lost reuse | 1/10/100/1,000 |
| H03 | Create -> move a fixed-size edit sequentially across the file | CDC and mapping-boundary churn | 1/10/100 |
| H04 | Alternate region A/B and repeatedly revert | CAS reuse, history-independent identity, storage plateau | 1/10/100/1,000 |
| H05 | Repeated early/middle/late insert/delete pairs | Suffix-linear count-changing work | 1/10/100; 1,000 on 1 MiB only |
| H06 | Repeated append, truncate, and re-extend | Tail locality, mapping growth, obsolete-object cleanup | 1/10/100/1,000 |
| H07 | Edit -> materialize after every edit | Seed churn, temp/native storage, clone/patch/fallback selection | 1/10/100 |
| H08 | Edit `N` times -> materialize only the latest root | Historical-scan and fragmentation risk | 10/100/1,000 |
| H09 | Edit -> range/full read every tenth edit | Read invalidation and current-root routing | 10/100/1,000 |
| H10 | Edit -> close/reopen every edit or every 10/100 edits | Reopen authority and first-operation cost | 1/10/100/1,000 |
| H11 | Build `N` roots -> reopen -> head/range/read/edit/materialize | History dependence after authority loss | 10/100/1,000 |
| H12 | Build `N` roots -> read roots 1, `N/2`, and `N` | Historical lookup dependence on root age | 10/100/1,000 |
| H13 | Fork two histories from one retained root | COW sharing and reachability | branch depths 10/100 |
| H14 | Drop roots -> GC with latest/sparse/branch roots retained | Shared-object safety and reclamation | 10/100/1,000 |
| H15 | GC while an old-root reader is pinned | Reader generation and deletion race | focused small fixture |
| H16 | Cancel/fault during edit/materialize/reconcile/GC | Old-or-new outcome, first error, cleanup, resume | focused small fixture |

## Fast size/revision allocation

Do not run every workflow at every size.

### 1-MiB history proof

Run many real revisions cheaply:

- H01, H02, and H04 at 1,000 operations;
- H05/H06 at 100 or 1,000 only when the mechanism screen predicts the total
  remains inside its frozen segment budget;
- H12-H16 on small fixtures;
- complete reconstruction and digest verification only at frozen checkpoints
  and sequence end, not after every revision.

### 10-MiB mechanism proof

Run:

- 100 deterministic same-size edits;
- 100 count-changing edits;
- materialization every tenth edit;
- reopen every tenth edit;
- selected old/current root reads.

### 100-MiB primary sentinels

Run only:

- 100 deterministic same-size edits;
- 10 deterministic count-changing edits;
- one final current-root reconstruction;
- one final first/full materialization;
- one same-open full read;
- one clone/no-op and one incremental patch;
- one reopen followed by head, range, first edit, and first materialization;
- one controlled host-buffer-cold approximation if all preconditions qualify,
  otherwise one explicit `Unavailable` record.

This separates the history-count and file-size variables while retaining one
real interaction proof.

## Checkpoint verification

Every operation must verify cheap exact invariants:

- returned/current root;
- expected length and edit classification;
- selected fast/fallback route;
- transaction and COMMIT count where applicable;
- checked work/counter bounds;
- terminal Q;
- descriptor/temp/residue state.

At revisions 1, 10, 100, and 1,000 where applicable, additionally record:

- p50/p95/max latency for the completed interval;
- objects created, reused, authenticated, live, and unreachable;
- canonical and mapping bytes created/rewritten;
- suffix references/bytes and CDC rejoin work;
- SQL query/row/BLOB counts;
- RSS, Q, descriptors, buffers, queues, and seed entries;
- logical/apparent/allocated DB, authority, journal, temp, native, and total
  store bytes.

At sequence end:

- reconstruct the current root and compare the frozen expected digest;
- verify selected historical roots;
- materialize the current root when the workflow requires it;
- audit reachability and storage;
- verify terminal Q, descriptors, locks, temporary files, and residue.

## Expected state custody

Each workflow consumes a deterministic, versioned, hashed operation log. Each
entry identifies:

```text
parent revision
operation and offset
removed and inserted bytes
expected length
expected root or profile-specific expected root
expected content digest
expected edit classification
expected fast/fallback route
```

Control and candidate consume the same semantic operation log. A new mapping
profile may have a different root manifest, but it must produce the same
logical bytes and independently frozen profile-specific identities. Random
workloads use a frozen seed and materialized edit log; they are never generated
during the measured campaign.

## Compact raw evidence

A 1,000-operation workflow is one append-only sequence record with compact
checkpoint and distribution arrays, not 1,000 large JSON records. The runner
must still stop and report the exact operation index on the first invariant
failure.

Full per-operation timings may be retained in a compact numeric sidecar when
needed for independent recomputation. Large repeated explanatory strings are
not duplicated in every row.

## Operations intentionally deferred from this matrix

- 500-MiB files;
- multi-day soak/endurance;
- thousands of concurrent clients;
- multi-terabyte GC;
- true device/controller-cold claims;
- multi-file directory/workspace/VFS/application behavior; and
- cross-platform clone/durability qualification.

Those require separate integration or infrastructure evidence and must not
slow the core algorithm-selection loop.

## G5-1 terminal amendment

G4 is now PASS/CLOSED and the first H11 attempts have completed. The controlling checkpoints are `1 / 10 / 100 / 1,000`, not only `10 / 100 / 1,000`. The exact diagnostic sequence and final whole-harness Q blocker are recorded in [H11 result](../concurrency-history/h11-result.md). A corrected H11 must pass before it can become the sentinel for a future full G5-C gate; v2 does not authorize this document's concurrency, released-root GC, branch/revert, cancellation, or capacity rows.

The complete H11 lock-to-terminal wall is `8,551,146,875 ns` under a hard `20,000,000,000 ns` screen budget. The older `<=120 s` number elsewhere in Round 0 is a prospective full-gate ceiling and is not the H11 budget.
