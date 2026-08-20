# Note to Read After M4.5

Status: post-M4.5 planning note only. This document does not claim that M4.5, the 100 MiB throughput target, profile selection, or production integration has passed.

Scope: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` on `codex/empty-worktree`. SQLite remains the authoritative Phase 4A disk engine. Do not resurrect append-only/pack storage, add another database, weaken durability, add workers/async, or introduce source-sized state.

## Executive direction

After M4.5, the primary performance objective is the retained-fixture 100 MiB full-create durable-capture path. Same-count small-edit latency remains the primary edit objective and a protected regression workload throughout subsequent full-create optimization.

The ordering is:

1. Finish M4.5 correctly and stop for an independent read-only audit.
2. Freeze the accepted M4.5 implementation, executable, fixture, prepared images, benchmark protocol, and raw evidence.
3. Optimize 100 MiB full-create durable capture toward at least 200 MiB/s.
4. Preserve the accepted same-count edit algorithm and rerun focused edit regressions after every later milestone.
5. Optimize post-COMMIT scrub and reconstruction only after the durable-capture result is stable.
6. Treat count-changing `+1` edits and leading directory insertions as separate suffix/topology problems, not as ordinary small same-count edits.

Do not mix the same-count edit experiment with full-create throughput experiments. They have different algorithms, counters, timer denominators, and acceptance criteria.

## What M4.5 must have completed

M4.5 exists to recover the strongest edit-algorithm result from rejected M4 without reusing its invalid authority model.

For an eligible same-count edit, the target complexity is:

```text
mutation:
O(changed CDC bytes + changed references + K + F*H)

pre-COMMIT qualification:
O(K + F*H + changed/new authenticated closure + H^2)

resident memory:
O(H + K + F + bounded page/chunk/SQL/output buffers)
```

The `H^2` term is acceptable for an initially bounded ancestry scan. Do not introduce a tree, set, or cache unless counters show ancestry membership is material.

Before M4.5 is considered complete, it must establish:

- a move-only, single-use authority witness owned by the active SQLite writer transaction/snapshot;
- exact authentication of the complete prior visible head in that transaction;
- a corrected full-closure `C0` control;
- a changed-spine `C1` candidate differing from `C0` only in qualification algorithm;
- shadow agreement between incremental and complete verification before incremental verification authorizes publication;
- independently prepared expected operation, edited fingerprint, ordered CDC sequence, result root, transition, and inserted/removed bytes before COMMIT;
- exact preservation of file-root mode and every canonical summary;
- complete prior-head comparison including generation, root, transition, and receipt/authority tuple;
- exactly one writer transaction, one complete-head publication, and one COMMIT dispatch;
- fresh independent read-only reconciliation for actual ambiguous COMMIT outcomes;
- exact `MissingObject(ObjectId)` and failure provenance;
- exact checked live-Q accounting returning to zero on every exit;
- correct SQL acquisition/query/execute/row labels;
- higher-authority W/D definitions preserved, with new canonical-write/authentication counters separately named;
- focused release-mode `C0/C1` A/B evidence;
- an independent read-only audit checkpoint before promotion or follow-on work.

The three comparison identities are:

- `A0`: historical frozen M3 evidence;
- `C0`: corrected M4.5 correctness/measurement substrate with complete pre-COMMIT closure;
- `C1`: byte-identical to `C0` except changed-spine qualification.

Interpret them as:

- `A0 <-> C0`: correctness and instrumentation substrate cost;
- `C0 <-> C1`: causal changed-spine algorithm result;
- `A0 <-> C1`: cumulative historical continuity.

Do not attribute an `A0 <-> C1` bundle directly to the changed-spine algorithm.

The primary small-edit result is wall latency plus exact changed-work counters. Never calculate `100 MiB / edit latency` and call it storage or edit throughput. The rejected M4 approximately 2.2 ms durable-edit result is motivation only, not accepted evidence.

## Primary post-M4.5 objective: retained 100 MiB full create

The primary throughput objective is:

> Reduce retained-fixture 100 MiB full-create durable capture from approximately 953.829 ms to at most 500 ms, equivalent to at least 200 MiB/s.

The stretch objective is:

```text
durable capture <= 333.333 ms
approximately 300 MiB/s
```

These are durable-capture objectives unless the controlling documents explicitly resolve the target as a complete-lifecycle objective. Until that denominator conflict is resolved, every report must publish both values with explicit names:

```text
durable_capture_mib_per_s
complete_lifecycle_mib_per_s
```

Never emit an unqualified `throughput_pass` while the denominator authority remains unresolved.

### Retained M3 phase budget

| Full-create durable phase | Median | Approximate share |
|---|---:|---:|
| Canonical CAS mapping and object persistence | 410.776 ms | 43.1% |
| Pre-COMMIT closure validation | 388.155 ms | 40.7% |
| SQLite COMMIT durability | 152.996 ms | 16.0% |
| Durable capture total | 953.829 ms | 100% |
| Durable-capture throughput | 104.84 MiB/s | - |

Post-COMMIT retained M3 evidence is separate:

| Lifecycle verification phase | Median |
|---|---:|
| Fresh reopen | 1.155 ms |
| Full closure scrub | 272.815 ms |
| Reconstruction | 429.985 ms |
| Range verification | approximately 1 ms |
| Complete lifecycle | approximately 1,663 ms |
| Complete-lifecycle throughput | approximately 60.1 MiB/s |

The previous approximately 54.77 MiB/s smoke result was a full-cycle diagnostic from a different implementation/timer state. It must not replace the retained M3 phase baseline.

### Amdahl consequence

Eliminating the complete 388.155 ms pre-COMMIT replay gives only a planning estimate:

```text
953.829 ms - 388.155 ms = 565.674 ms
100 MiB / 0.565674 s = approximately 176.8 MiB/s
```

That remains approximately 65.674 ms above the 500 ms / 200 MiB/s target. Therefore the full-create program must improve at least two areas:

1. Remove the redundant pre-COMMIT full traversal.
2. Remove at least approximately another 66 ms from mapping/CAS or COMMIT.

The arithmetic is a prioritization estimate, not benchmark evidence. Do not publish it as a measured result.

## Post-M4.5 milestone sequence

### F0 - Freeze the accepted M4.5 checkpoint

After the independent audit accepts M4.5:

- record Git HEAD and the complete dirty implementation-diff hash;
- record benchmark source and release executable hashes;
- freeze fixture, prepared-base, authority, and expectation manifests;
- freeze commands, environment, toolchain, SQLite profile, and raw evidence;
- retain `C0` and `C1` as permanent small-edit regression controls;
- update the progress ledger and M4.5 milestone report;
- do not claim production integration, profile selection, or promotion.

No new performance implementation belongs in F0.

### F1 - COMMIT and physical-I/O diagnosis

Establish write/sync observations before changing write shape so later insertion batching remains attributable.

Observe, where genuinely available:

- COMMIT dispatch and acknowledgement separately;
- actual reconciliation outcome;
- SQLite page-cache current/high-water and spill state;
- dirty pages;
- main database and rollback-journal write calls and bytes;
- sync calls and sync wall time;
- rollback-journal and temporary-file peak allocation;
- process user and system CPU;
- logical, apparent, and allocated bytes for the database and every sidecar/temp endpoint.

Unavailable observations must be represented as `Unavailable`, never zero or a logical-length substitute.

Do not change:

- `synchronous=FULL`;
- rollback-journal durability;
- the one-transaction/one-COMMIT rule;
- visible-head-last publication ordering;
- mmap/WAL policy;
- caller-thread synchronous execution;
- the database or schema.

If sync latency dominates and write/sync counts are already minimal, record the durability floor instead of adding speculative tuning.

### F2 - Transaction-local full-create construction witness

This is the largest exposed full-create opportunity.

The existing CAS/build path already performs important authentication while streaming the source:

- supplied canonical bytes are validated;
- ObjectIds are calculated;
- newly created rows are inserted;
- conflicting rows are fetched and fully authenticated before reuse;
- mapping parents are built from exact authenticated child IDs.

Carry that work forward as a bounded, private, nonserializable, transaction-owned construction proof:

```text
authenticated chunks
-> leaf summaries
-> branch summaries
-> exact mapping root
-> exact workspace root/transition expectation
-> complete publication expectation
```

Bound construction-proof memory by the existing builder frontier:

```text
O(K + F*H)
```

Never retain a source-sized receipt list, all-reference vector, object cache, or visited set.

Implementation order:

1. Return opaque per-put transaction-local evidence for either newly inserted verified bytes or a fully authenticated incumbent reuse.
2. Keep the current full pre-COMMIT verifier authoritative.
3. Fold receipts into bounded leaf/branch/root summaries in shadow mode.
4. Require exact agreement between the bounded construction proof and the complete verifier.
5. Permit the construction proof to omit the duplicate database replay only after adversarial shadow tests pass.

Expected direct counter movement:

- pre-COMMIT SQL calls, rows, BLOB reads, and physical authentication fall by at least approximately 95%;
- approximately 100 MiB of repeated authentication work disappears;
- source fingerprint, CDC sequence, canonical bytes, ObjectIds, roots, transition, closure, storage, transaction count, and COMMIT count stay exact;
- post-COMMIT reopen, scrub, and reconstruction remain independent and unchanged.

This removes a duplicate linear pass, but total full-create complexity remains `Theta(source bytes + references)`. Full creation cannot beat the lower bound of reading the input and durably storing unique data.

### F3 - Bounded immutable CAS insertion batches

After F2, mapping/CAS becomes the main controllable durable-capture phase.

The retained fixture has approximately:

- 5,284 chunk references;
- 83 file leaves;
- 2 branches;
- roughly 5,372 new objects overall.

Introduce a private bounded insertion group with both a row and byte limit, for example:

```text
rows <= 31 or 64
AND
canonical capacity <= 1 MiB
```

Flush when either limit is reached.

Every batch must preserve:

- complete validation of each submitted canonical object;
- one classification/result per input occurrence;
- complete incumbent authentication on conflict/reuse;
- duplicate-ID handling within the batch;
- immutable CAS semantics;
- exact created/reused counters;
- bounded SQL text, parameter, result, and canonical-buffer memory;
- one writer transaction and one final COMMIT.

With a 31-row bound, approximately 5,372 individual insertion executions could approach:

```text
ceil(5,372 / 31) = 174 batches
```

That is a database-crossing reduction, not a total Big-O improvement. Fixed bounded batches leave row and byte work linear.

Primary gates:

- mapping improves by at least 5%;
- durable capture improves by at least 5%;
- at least 4/5 paired runs improve;
- predicted execute/query counters move exactly;
- canonical bytes, object counts, endpoint storage, transactions, and COMMITs remain exact;
- CPU, exact Q, RSS, peak journal/temp allocation, and COMMIT do not violate protected gates.

If mapping improves but COMMIT expands enough to erase durable-capture improvement, reject or revise the batching change.

### F4 - Residual mapping/CAS breakdown

After F2 and F3, divide the remaining mapping phase into independently counted work:

- source read;
- CDC;
- raw chunk hashing;
- canonical encoding;
- ObjectId hashing;
- SQLite parameter binding;
- insert execution;
- conflict handling;
- mapping-leaf encoding;
- mapping-branch encoding;
- root and transition encoding.

Only optimize the dominant observed remainder. Candidate one-variable milestones include:

- insert-first CAS instead of existence-probe-then-insert where that probe exists;
- eliminating one proven repeated hash;
- borrowing one canonical buffer through a SQLite call;
- eliminating one proven repeated canonical encode;
- reducing native SQLite preparations only when native preparations are directly observed.

Do not combine these changes. Each milestone requires its own predicted counter equation and control/candidate A/B.

### F5 - Reassess the 500 ms gate

After the construction proof and bounded insertion work, rerun the exact retained 100 MiB full-create campaign:

- release mode only;
- one warmup;
- five balanced isolated control/candidate pairs;
- alternating `AB`/`BA` order;
- exact separately prepared starting images;
- source/fixture preflight outside timers;
- disjoint phase equations;
- raw JSONL and external macOS observations;
- exact CPU, Q, RSS, storage, SQL, hash, authentication, and COMMIT counters.

Interpret outcomes as:

- `<=500 ms`: the 200 MiB/s durable-capture objective is reached;
- `500-566 ms`: inspect the residual mapping and COMMIT evidence;
- `>566 ms`: the construction witness missed its predicted work or another phase expanded; reconcile counters before continuing;
- COMMIT-dominated: investigate only observed write/sync amplification without weakening durability.

Do not proceed to 512 MiB or the full profile campaign until the retained 100 MiB result is internally consistent.

### F6 - Post-COMMIT lifecycle optimization

Only after durable capture is stable should complete-lifecycle throughput become the main optimization target.

The two largest retained post-COMMIT phases are:

- reconstruction: approximately 429.985 ms;
- fresh full scrub: approximately 272.815 ms.

#### Fresh scrub

Use bounded, ordered, duplicate-preserving, missing-object-detecting batches. Reuse the accepted bounded reference/CTE machinery rather than creating a second walker.

Expected effect:

```text
per-reference SQLite calls
-> approximately one call per bounded leaf-sized batch
```

Scrub remains `Theta(reachable canonical bytes + edge occurrences)` because independent authentication must inspect the closure. Rows, authenticated bytes, raw hashes, exact error identities, and closure semantics must not decrease.

#### Reconstruction

M2 already reduced reconstruction statement acquisitions from approximately 5,371 to 170, while reconstruction wall improved only 7.169%. Therefore statement reduction alone is no longer a sufficient optimization hypothesis.

Instrument and attack only a measured residual:

- row-materialization copies;
- repeated canonical authentication;
- SQLite BLOB opens and complete-byte passes;
- output copying;
- parser/hash double passes.

A borrowed or streaming BLOB path must fully authenticate before exposing semantic bytes. It must not allow a SQLite row lifetime to escape its callback and must remain bounded.

## Small-edit objectives after M4.5

### Same-count file edit: protected primary edit workload

This is the M4.5 workload. Its continuing requirements are:

- latency remains mostly independent of total file size for fixed edit size/tree height;
- exact changed-spine counters remain bounded;
- no complete pre-COMMIT closure replay returns;
- no source-sized or all-reference state appears;
- one transaction and one COMMIT remain exact;
- root and transition equal an independently prepared fresh-build oracle;
- fresh scrub and reconstruction remain complete and independent.

After every later full-create optimization, rerun focused same-count edit correctness and counter regressions. Full-create batching or construction authority must not weaken or slow the accepted changed-spine path.

### Count-changing `+1` edit: currently suffix-linear

Under the current fixed-ordinal mapping, insertion of one reference shifts the suffix. The honest bound is:

```text
O(changed CDC region + suffix references/objects/bytes)
```

Required near-term behavior:

- traverse persisted references instead of rebuilding/rechunking the entire source unnecessarily;
- use bounded streaming or file spool state;
- report exact suffix references, objects, canonical bytes, and unreachable old bytes;
- keep `+1` explicitly nonqualifying for a logarithmic/path-local claim.

A genuine expected-locality `+1` improvement requires a separately authorized deterministic content-defined/prolly mapping profile. That changes mapping topology and intentionally changes object/root/transition IDs. It is a later format/profile project, not a post-M4.5 micro-optimization.

### Directory replacement and insertion

The current flat-directory index imposes structural limits:

- replacement may avoid reading every entry page but must still authenticate/decode and rewrite the flat index;
- leading insertion may repartition a suffix and remain linear in entries.

Near-term exact-ID work should:

- construct directory pages linearly without repeated clone/re-encode of the growing candidate page;
- route replacement by authenticated descriptor counts/ranges;
- decode and rewrite only the selected page plus required wrapper/index objects;
- verify exact ordered names, exact targets, adjacency, duplicate rejection, partition fullness, and closure digest.

A radix-directory topology is a later profile project requiring independent scaling evidence and intentionally different IDs.

## Per-milestone small-step protocol

Every optimization milestone must change one independently measurable variable.

### Before implementation

Preregister:

- exact code-path change;
- one primary bottleneck and hypothesis;
- algorithmic classification: asymptotic, pass elimination, SQL crossing, copy/hash reduction, or diagnostic;
- expected direct-counter equation;
- affected phase;
- minimum useful wall effect;
- protected correctness/resource metrics;
- sample count, extension trigger, and retention/rejection rule.

### Correctness before timing

Require:

- exact retained source fingerprint;
- exact ordered CDC fingerprint/count;
- exact canonical bytes and ObjectIds;
- exact root and transition;
- exact complete reachable closure;
- expected edited/full-created source reconstruction;
- exact range bytes where applicable;
- typed malformed/missing/conflict/tamper failures;
- exactly one writer transaction and one COMMIT;
- fresh independent reopen, scrub, and reconstruction;
- exact checked Q and overflow behavior.

### A/B protocol

1. Freeze control and candidate implementations and executables once.
2. Calibrate the corrected benchmark using A/A pairs for the primary scenario.
3. Use separately prepared, hash-verified database images outside measured intervals.
4. Run one uncounted warmup per arm.
5. Run five adjacent balanced pairs with alternating `AB`/`BA` order.
6. Compute paired effects, not only independent arm medians.
7. Preserve all raw rows.
8. Add exactly 15 pairs only under a predeclared ambiguity/RSS trigger.
9. Never optionally stop or rerun only unfavorable rows.

### Decision classes

| Decision | Required evidence |
|---|---|
| Correctness pass | Exact identity, expected result, closure, Q, storage, transaction, COMMIT, and durability outcome |
| Material performance pass | At least 5% paired median improvement, at least 4/5 wins, above A/A noise, predicted counters, protected gates pass |
| Mechanism-only retention | Exact redundant work removed, 0-5% wall result, no resource/complexity cost; no throughput claim |
| Algorithmic pass | Mathematical bound plus exact work/scaling counters; no automatic throughput claim |
| Reject | Correctness failure, counter mismatch, unbounded memory, identity/storage/publication drift, or protected regression |

An accepted milestone becomes the next rolling control. A rejected milestone is reverted. An inconclusive milestone is not stacked with another candidate optimization.

### Required milestone report

Before advancing, record:

- implementation diff and files changed;
- before/after algorithm and complexity bound;
- memory-bound formula;
- predicted and observed counter equations;
- exact commands, hashes, and custody manifests;
- five raw paired rows and artifact hashes;
- phase medians, min/max/spread, paired deltas, wins, and noise comparison;
- CPU, exact Q, RSS, physical/logical storage, SQL, BLOB, hash, and authentication evidence;
- correctness and adversarial tests;
- retained/revised/reverted/inconclusive decision;
- updated rolling benchmark baseline.

## Complexity and optimization classification

| Work | Honest classification |
|---|---|
| M4.5 same-count changed-spine | Genuine pre-COMMIT asymptotic improvement from full-snapshot work to changed-region/spine work |
| Full-create construction witness | Removes one complete linear pass; total full create remains linear |
| Bounded multi-row CAS insertion | Reduces SQLite crossings; fixed-bound total work remains linear |
| Batched scrub reads | Reduces SQLite crossings; independent scrub remains linear |
| Borrowed/single-pass BLOB work | Constant-factor copy/hash reduction |
| COMMIT observation/tuning | Durability-stack diagnosis, not algorithmic complexity improvement |
| Streaming range sink | Memory-class improvement from `O(returned bytes)` eager output to a bounded window |
| Current-profile `+1` | Suffix-linear; no logarithmic claim |
| New content-defined file profile | Possible expected-locality improvement with intentionally different IDs; adversarial worst case may remain linear |
| Current flat-directory replacement | At least linear in flat index descriptors |
| New radix-directory profile | Possible lookup/replacement scaling improvement with intentionally different IDs |

Do not confuse a 96% SQL-statement reduction with a 96% wall-time reduction. M2 demonstrated 5,371 to 170 statements but only a 7.169% reconstruction wall improvement. Likewise, large copy reductions in M1/M1b produced only small wall effects. Counters establish mechanism; paired wall measurements establish performance.

## Priority summary

| Priority | Objective | Primary metric |
|---:|---|---|
| Immediate | Complete and independently audit M4.5 | Same-count edit correctness, latency, changed-spine counters, authority |
| Primary after M4.5 | Retained 100 MiB full create | Durable-capture wall and MiB/s |
| Protected throughout | Same-count edit | No correctness, latency, counter, CPU, Q, RSS, or storage regression |
| Secondary after capture target | Complete lifecycle | Scrub and reconstruction wall/MiB/s |
| Later | Current-format `+1` and directory scaling gates | Exact suffix/index/rewrite work |
| Conditional later | New file/directory canonical profiles | Scaling versus storage/compatibility cost |
| After candidate proof/profile selection | Production integration | Production Engine A/B and parity, not benchmark-shadow inference |

## Final instruction after M4.5

The first post-M4.5 implementation should not be another edit optimization or a broad mixed refactor. Establish physical/COMMIT measurement, then implement the bounded transaction-local full-create construction witness in shadow mode. Once it safely removes the duplicate pre-COMMIT replay, implement bounded immutable CAS insertion batches. Re-measure the retained 100 MiB row after each accepted milestone and stop stacking changes until its counter equations and phase effects reconcile.

The primary success condition is a correct, bounded, one-transaction, one-COMMIT 100 MiB durable capture at or below 500 ms. Same-count edit locality is the protected algorithmic success condition. Neither objective may be obtained by weakening canonical identity, authentication, durability, typed failures, independent post-COMMIT verification, bounded memory, or honest timer boundaries.
