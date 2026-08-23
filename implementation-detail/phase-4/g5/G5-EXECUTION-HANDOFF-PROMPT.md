# G5 terminal execution handoff prompt

Copy the text below into the G5 execution task.

---

/goal Complete Phase 4 G5 from the closed G4 baseline through one honest G5
terminal PASS, make G6 eligible, and stop before G6. Work autonomously across
G5-0, G5-1, G5-2, G5-3, and the terminal audit. An intermediate `REVISE`,
`NO-GO`, failed screen, failed test, protocol defect, resource failure, or
performance miss is not a stopping point: preserve it, diagnose it with fresh
subagents, repair or redesign the smallest in-scope mechanism, create a new
versioned attempt, and continue. Never weaken a hard gate, delete a failed
attempt, selectively rerun rows, or manufacture a PASS.

## Repository, custody, and authority

Work only in:

```text
/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty
```

Required branch and starting committed checkpoint:

```text
branch: codex/empty-worktree
HEAD:   d58c5a1307253dfc221fe50de996c183deb9458a
```

Never touch sibling `layerfs` worktrees. Preserve all existing dirty/untracked
G5 research, H11 attempts, plans, and G4 evidence. Do not reset, clean, delete,
rewrite, relabel, or reuse a failed result as a PASS. Do not commit unless the
user separately asks. Do not start G6, WP5, VFS/SDK/application integration,
production promotion, profile selection, format migration, or destructive GC.

Current controlling artifacts:

| Artifact | SHA-256 |
|---|---|
| `implementation-detail/phase-4/g5/implementation-verification-plan.md` | `7a7092424d7bd7f55f8479791d04d4411b4cd9a1a7a5618355f5015cb7ee0acd` |
| `research/phase-4/g5-round-0/benchmark-contracts/g5-fast-iteration-contract.md` | `36495a4640e1d20591ece55f7f2ce35bd8b6ed76ccae41e43c288fa01f0635ba` |
| `implementation-detail/phase-4/baseline/current-benchmark-scoreboard.md` | `aae8a7abe2a13c3dfdf4adc006b31bc08a18fc05d02f7b7b06489d7ed0910b77` |
| `implementation-detail/phase-4/experiments/g4-materialization-acceptance/G4-STAGE-TERMINAL-v1.json` | `0297ca2e3b49ddb7d8d2d435713450dcc336397b53cbaaaee9647a46eebcede8` |

First freeze and report `pwd`, branch, HEAD, status, source/diff hashes,
toolchain, environment, and the hashes above. Stop before mutation if the branch,
HEAD, or controlling hashes differ unexpectedly. Dirty G5 files are expected
custody, not permission to rewrite historical attempts.

## Read before acting

Read completely before implementation:

- `implementation-detail/phase-4/g5/implementation-verification-plan.md`
- `research/phase-4/g5-round-0/README.md`
- `research/phase-4/g5-round-0/decision/final-synthesis.md`
- `research/phase-4/g5-round-0/decision/lane-dispositions.md`
- `research/phase-4/g5-round-0/reopen-authority/report.md`
- `research/phase-4/g5-round-0/concurrency-history/h11-result.md`
- `research/phase-4/g5-round-0/concurrency-history/resource-history-model.md`
- `research/phase-4/g5-round-0/benchmark-contracts/g5-fast-iteration-contract.md`
- `research/phase-4/g5-round-0/history-endurance/history-scaling-contract.md`
- `research/phase-4/g5-round-0/history-endurance/longitudinal-workload-matrix.md`
- H11 v1/v2 preregistrations, runners, analyzers, raw audits, and final-Q audit
- `implementation-detail/phase-4/baseline/g4-materialization-acceptance-baseline-v1.md`
- `implementation-detail/phase-4/baseline/current-benchmark-scoreboard.md`
- `implementation-detail/phase-4/experiments/g4-materialization-acceptance/G4-REPORT.md`
- `implementation-detail/phase-4/algorithm/spec.md`
- `implementation-detail/phase-4/algorithm/tests-and-benchmarks.md`
- `implementation-detail/phase-4/algorithm/complexity-analysis.md`
- `crates/layerfs-core/src/validation.rs`
- the actual Store/open/receipt/witness/scrub/edit/proof/publication paths in
  `crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs`
- the seed/clone/patch/fallback/native-publication paths in
  `crates/layerfs-engine/src/bin/phase4_g3_materialization.rs`

Trace every caller before changing a shared function. Treat the plan and prior
agent statements as hypotheses/contracts, not evidence. Local source, retained
raw evidence, exact tests, and direct counters outrank prose. External research
may inform alternatives, but only primary documentation/source is admissible
and it cannot substitute for LayerFS evidence.

## Mandatory subagent orchestration

You are the integration owner and final decision maker, but you must use
subagents for grounded exploration, implementation review, and evidence audit.
Subagents must read the relevant source/evidence themselves and must not treat
your proposal as proof.

At the beginning of every G5 milestone, launch parallel read-only lanes for:

1. **Correctness/trust/invariants** — trace receipts, scopes, object identity,
   exact errors, transaction/COMMIT, reconciliation, and threat boundaries.
2. **Performance/Big-O/resources** — recompute removable wall, counter
   equations, queue/work bounds, RSS/Q/storage, and falsifying thresholds.
3. **Benchmark/evidence/custody** — audit schedule, timer boundaries, prepared
   inputs, raw schema, analyzers, lock ownership, manifests, and fast budgets.

For G5-2/G5-3, also assign a projection/SQLite-concurrency review lane as slots
allow, possibly in a second wave. Implementation workers must own disjoint files
or responsibilities, know that other agents share the worktree, preserve each
other's edits, and never run measured campaigns concurrently. Only the
integration owner may hold the global benchmark lock or launch a measured
campaign.

Before each milestone disposition, synthesize subagent findings, resolve
disagreements against source/raw evidence, and record accepted/rejected
recommendations. Before terminal G5 PASS, launch a fresh independent read-only
audit wave covering source semantics, performance arithmetic, evidence/custody,
and final-manifest closure.

If an attempt returns `REVISE` or `NO-GO`, immediately launch targeted root-cause
subagents, preserve the attempt, select the smallest safe repair or a different
in-scope mechanism, and continue. Do not ask the user for routine authorization;
this prompt grants authority for in-scope G5 implementation, tests, short
experiments, versioned evidence, and repair iterations. Only an external
precondition outside the repository/scope can be a blocker. Source defects,
failed tests, protocol gaps, noise, and performance misses are repair work, not
external blockers.

## Non-negotiable architecture and durability

G5 remains CAS + CDC + COW + canonical K64/F64 + SQLite. Keep:

- `ObjectId = hash(canonical bytes)`;
- validation of every fetched canonical object against its requested ObjectId;
- hashing/validation of every newly supplied/generated object;
- exact canonical-byte equality before immutable incumbent reuse;
- mapping topology, transition, logical-role, length, and exact-error checks;
- expected-head comparison and one serialized writer;
- one writer transaction and one publication COMMIT per canonical edit;
- objects/transition before visible-head publication;
- rollback before COMMIT dispatch and fresh independent requested/prior/
  different/ambiguous reconciliation after an uncertain result;
- rollback journal `DELETE`, `synchronous=FULL`, `temp_store=FILE`,
  `mmap_size=0`, and the retained cache-spill policy;
- bounded caller/worker buffers, exact logical Q, explicit RSS/storage, and
  terminal cleanup;
- verified mode as the unchanged default/control and an explicit full snapshot
  verifier.

Do not add/change schema, canonical format/profile, metadata serialization,
compression, WAL, retry loops, a second writer COMMIT, worker pool, async
framework, VFS, dependency, network/distributed mechanism, persistent
cross-process seed, or concurrent GC. Never call `PRAGMA integrity_check`
LayerFS semantic authentication. Neither trusted nor verified mode provides
rollback freshness without external monotonic authority; report it as
`NotProtected`.

## Fast-iteration law

Every candidate/revision follows:

```text
touched focused tests
-> zero-row schedule assertion and dry-run
-> one complete <20-second mechanism screen
-> one frozen-source workspace/clippy/fmt/diff closure
-> one complete <=120-second measured gate
```

These are total campaign budgets, not per-row, per-operation, per-pair, or
per-subagent allowances. A complex matrix is allowed; repeated setup is not.

Required execution shape:

- build each release executable once per frozen candidate outside the campaign;
- hash source/diff/executable/fixture/base/sidecar/expectations once in preflight;
- use long-lived children for stateful micro-operations;
- prepare deterministic roots/edit logs once and give matched arms exact inputs;
- verify root, transition, length, route, transactions/COMMITs, work, and
  operation-local Q after every operation;
- perform full reconstruction/digest/snapshot verification only at frozen
  checkpoints and sequence end;
- run semantic/fault cases at 1/10 MiB and use 100 MiB only for a causal
  performance/scaling mechanism;
- retain compact numeric timing/work sidecars and a compact sequence record;
- no 500-MiB campaign in G5 fast iteration;
- no selective row deletion/replacement/reordering or retry-until-PASS;
- do not rerun passing full workspace/static closure while source is unchanged;
- stop a failing campaign immediately, preserve it, and create a new version
  only after a source/method repair.

## Controlling G4 scoreboard

Protect these retained 100-MiB operations:

| Operation | Retained value |
|---|---:|
| Durable full create | `279.463 ms / 357.829 MiB/s` |
| Same-open same-count edit | `8.043 ms` |
| Same-open early/middle `+1` | `5.108 / 4.576 ms` |
| Reopen/head | `3.583 ms` |
| First same-count edit after reopen | `154.019 ms` |
| First early/middle `+1` after reopen | `248.492 / 244.306 ms` |
| Authenticated returned 1-MiB range | `2.046 ms` |
| Warm/fresh canonical reconstruction | `237.214 / 237.381 ms` |
| First/full native materialization | `307.652 ms` |
| Prepared exact-root clone | `2.877 ms` |
| Prepared one-byte sparse projection | `4.104 ms` |
| Count-changing native full fallback | `329.237 ms` |

A protected latency regression is material only when both branches hold:

```text
candidate/control > 1.05
AND candidate_mean - control_mean >= 1.000 ms
```

For a fixed two-sample sum:

```text
candidate_sum * 100 > control_sum * 105
AND candidate_sum - control_sum >= 2,000,000 ns
```

Report every raw value, ratio, and delta even when not material. Identity,
topology, exact errors, direct work, transaction/COMMIT counts, durability,
reconciliation, Q/RSS/buffers/descriptors, storage, residue, chronology,
custody, and analyzer agreement are hard gates and have no materiality waiver.

## Milestone G5-0 — repair H11 evidence authority

### Objective

Remove the known H11 evidence blocker without changing the LayerFS algorithm.
Preserve v1/v2 byte-for-byte and create the next unused versioned attempt.

### Required implementation/evidence repair

- charge the expected-manifest String and parsed expectation vector;
- charge current/retained reachability sets under a frozen exact rule;
- charge history timings, transient formatting, and final report output;
- include traversal/reachability high-water in whole-harness Q;
- drop every owned capacity before reporting Q zero;
- emit selected historical root/transition/output tuples for both analyzers;
- consume the operation log or remove it from execution-authority claims;
- split preflight, SQLite connection open/profile initialization, and head lookup;
- never report incomplete SQL/open counters as complete;
- validate lock inode/token ownership before release;
- fsync referenced artifacts, result directory, terminal verification, and
  lock-release attestation;
- retain primary and independently implemented recomputation.

### Exact screen/gate

Use the deterministic 1-MiB N=1/10/100/1,000 balanced schedule. Complete wall
must remain `<20 s`; no broader campaign is needed for G5-0.

Hard expected mechanism values for the unchanged deterministic workload:

| Metric | Expected authority |
|---|---:|
| Current-live graph | `58 objects / 1,051,574 canonical bytes / 2,255 mapping bytes` |
| Unique-revision slope | `6 objects` |
| Canonical-byte slope | `23,030 bytes/revision` |
| Mapping-byte slope | `2,255 bytes/revision` |
| SQLite logical/apparent growth | about `24,858.9 bytes/revision` |
| Prior observed campaign maximum RSS | `14,057,472 bytes` |
| Existing hard RSS ceiling | `20,971,520 bytes` |
| Whole-harness terminal Q | exactly `0` after all drops |

Identity/work/storage tuples must remain exact; timing uses the frozen G5 dual
materiality rule. Analyzer agreement, custody, lock, descriptor/temp/work-root
cleanup, and terminal manifest verification are hard.

### G5-0 exit

`PASS` only when H11 is qualifying whole-harness evidence. Analyzer-level PASS
with omitted ownership is still `REVISE`. On failure, preserve the version,
repair the exact blocker, and run a new version. Do not start G5-1 until PASS.

## Milestone G5-1 — TrustedLocalDev reopen/edit fast path

### Objective

Make the first edit after reopen operation-local under an explicit weaker
`TrustedLocalDev` contract while preserving verified mode as the default and
byte-identical control behavior.

### Smallest implementation

- select integrity mode once at `Store::open`; reject per-operation switching;
- add a distinct single-use transaction-local `TrustedLocalEditScope` and the
  minimum explicit `EditBaseScope::{Verified, Trusted}` plumbing;
- share only exact head/open/transaction/store/profile/epoch/serial fencing;
- omit only eager `scrub_file(current_root)` and `scrub_file(parent_root)` in
  trusted scope establishment;
- keep all shared fetched/new/incumbent object validation unconditional;
- keep trusted/verified provenance through proof, edge accounting, publication,
  and report;
- add `trusted_assumed_*` counters; never increment verified receipt-covered
  counters for assumed edges;
- trusted proof never sets verified carry-forward in the first candidate;
- a later verified reopen always performs its complete scrub;
- define `verify_snapshot_closure(current_head, explicit_retained_roots)`
  exactly; unreachable object-table rows are a separate all-row audit;
- preserve receipt/schema/write shape; trusted receipt is head-tuple/fencing
  evidence, never closure provenance.

### Required semantic matrix

- trusted scope single-use/open/transaction/head fencing;
- trusted commit -> close -> verified reopen -> required full scrub;
- touched missing/mismatched/wrong-role/malformed object: both modes fail before
  COMMIT with exact prior head;
- unrelated corrupt object: verified scrub fails; trusted edit may commit;
  later access/full verification must fail exactly;
- DB-only rollback/current sidecar, DB+sidecar rollback, old valid head/receipt
  replay, wrong-store DB, missing/wrong-size/wrong-mode/symlink/replaced sidecar;
- rollback freshness labeled `NotProtected` in both modes absent external head;
- requested/prior/different/ambiguous COMMIT outcomes and no post-COMMIT relabel;
- one writer transaction and one publication COMMIT;
- terminal scope/permit/receipt/Q/journal/temp/residue zero.

Preserve exact error boundaries: `ValidationAuthorityUnavailable`,
`InvalidValidationReceipt`, exact `MissingObject(id)`, `IdentityMismatch`,
`WrongLogicalRole`, exact mapping/length errors, `PublicationConflict`, and
`AmbiguousDurability`. Raw SQLite corruption/I/O is not relabeled as CAS
identity failure.

### Measurement design

Run two comparisons:

1. frozen G4 verified executable vs G5 verified mode, protecting the old path
   and measuring implementation/instrumentation overhead;
2. G5 verified vs G5 trusted mode, isolating the trust-policy variable.

Use only retained operations:

```text
first-edit-after-reopen
same-middle
one-byte-{early,middle,late}
plus1-{early,middle}
```

Use at least 20 matched observations for the primary first-edit class and five
adjacent pairs for each secondary shape; report every sample. Reuse prepared
inputs within long-lived arms and do not invent `+1 late` or deletion here.

### Quantitative targets

The exact same-count removable-budget model is:

```text
154.019083 ms total
  = 3.726500 ms reopen/head
  + 143.041917 ms full authority
  + 5.068333 ms edit/mapping
  + 0.133917 ms proof
  + 2.048416 ms COMMIT

optimistic trusted model = 10.977166 ms
```

| Operation | Current | Modeled trusted | Direct target |
|---|---:|---:|---:|
| First same-count after reopen | `154.019 ms` | `10.977 ms` / `14.0x` | p50 `<=15 ms`, p95 `<=25 ms` |
| First early `+1` after reopen | `248.492 ms` | `8.691 ms` / `28.6x` | direct evidence, p50 `<=15 ms` |
| First middle `+1` after reopen | `244.306 ms` | `8.159 ms` / `30.0x` | direct evidence, p50 `<=15 ms` |

Hard mechanism equations:

```text
trusted complete-closure scrub calls/bytes = 0/0
verified complete-closure scrub bytes       > 0
healthy root/transition/database/work       = exact parity
trusted fetched-object authentication       = observed operation-local work
transactions/COMMITs                        = 1/1
terminal Q                                  = 0
```

Minimum causal improvement is `>=50%`; strong expectation is `>=80%`. A
semantic PASS without meaningful direct speed is `REVISE`, not G5-1 PASS.

### G5-1 exit

All semantics, two comparisons, protected operations, direct targets,
resources, two analyzers, raw custody, cleanup, and manifest must pass. On any
failure, preserve, repair/redesign within the fixed threat boundary, and run a
new version. Do not start G5-2 until PASS.

## Milestone G5-2 — bounded warm projection service

### Objective

Return durable canonical success without waiting for derived native freshness,
reuse qualified APFS clone/patch/publication primitives, and keep projection
state bounded during rapid changes.

### Required semantics/implementation

- retain synchronous `materialize_exact(root, destination)`; exact work is
  never coalesced or silently replaced;
- add benchmark-private `follow_latest` for one LayerFS-owned private projection;
- expose `canonical_root`, `projected_root`, `target_root`, `state`, and `route`;
- use one worker, one mutex/condition variable, one in-flight target, and one
  pending latest target; no queue/pool/framework;
- use the private active projection as the next root-bound read-only seed;
- allow one active projection and one private successor, then rotate seed only
  after successful publish/reconciliation;
- each canonical edit supplies a move-only hint containing parent root, target
  root, length class, and exact dirty ranges;
- accept only a continuous chain; merge overlapping/adjacent ranges;
- prospectively cap at 256 ranges, 8 MiB dirty bytes, and a 1-MiB streaming
  buffer; chain gap/count change/cap overflow -> `FullFallback`;
- never apply an `R2 -> R3` patch to an R1 seed;
- first candidate does not cancel a valid in-flight build; new targets merge
  into pending, preserving a continuous base;
- cancellation affects private output only; exact work never cancels;
- worker uses a read-only connection with no explicit full-reconstruction read
  transaction, zero writes, zero writer transactions, and zero COMMITs;
- no persistent cross-process seed; fresh process means miss/full fallback;
- no user-editable destination, GC, WAL, retry, second writer, or worker pool.

Seed rotation must prove:

```text
open/verify R2 successor descriptor
-> atomically publish/reconcile R2
-> install R2 as active projection/seed
-> release R1
```

Report rotations, roots before/after, descriptor acquisition/release/failure,
seed admission/hits/misses/rebuilds, and amortized hit cost.

### Hard state/timer equations

```text
requests = coalesced_before_start + builds_started
builds_started = published + cancelled + failed + stale_completed

pending high-water <= 1
in-flight high-water <= 1
terminal pending/in-flight = 0/0
terminal projected_root = last_requested_root on successful drain
projection writer transactions/COMMITs = 0/0
terminal worker/descriptors/temp/Q = 0
```

Record `t0` request, `t1` canonical ACK/reconciled-visible, `t2` enqueue, `t3`
worker start, and `t4` native ACK/reconciled-visible. Derive edit-to-ACK,
dispatch, queue wait, projection service, projection-request-visible, and
edit-request-visible. Retain the internal qualification + clone/fallback +
fetch + patch + sync + metadata + rename + directory-sync + reconciliation +
cleanup + unattributed equation.

### Fast screen and gate

Screen `<20 s` total:

- small state/fault/cancel/restart tests;
- one 100-MiB exact clone;
- one 100-MiB one-byte sparse patch;
- one 10-MiB precommitted count-changing enqueue storm that outruns the worker;
- one 100-MiB final fallback;
- one foreground edit while the worker reads.

Gate `<=120 s` total:

1. exact-every-root 64-operation same-size sequence for seed rotation and
   additive latency;
2. latest-following 100-operation same-size sequence for no-lag throughput;
3. forced count-changing queue-pressure storm for actual coalescing;
4. one final 100-MiB fallback/convergence sentinel;
5. exact old/current root reads while projection is behind;
6. foreground writer/worker-reader contention;
7. frozen small-fixture publication/cancel/restart faults;
8. primary and independent recomputation.

The same-size projection stage (`4.104 ms`) is already faster than the same-size
canonical producer (`8.043 ms`); it is a throughput/no-lag test, not the primary
coalescing proof. Count-changing projection (`329.237 ms`) vs middle `+1`
canonical edit (`4.576 ms`) creates the controlling ~72x queue pressure.

### Quantitative targets

| Metric | Hard target | Strong target |
|---|---:|---:|
| Exact projection service p50/p95 | `<=5 / <=8 ms` | same |
| Sparse projection service p50/p95 | `<=6 / <=10 ms` | same |
| Same-open edit-to-projected p50/p95 | `<=18 / <=30 ms` | `<=15 / <=25 ms` |
| Trusted first-reopen edit-to-projected p50/p95 | `<=22 / <=35 ms` | `<=18 / <=30 ms` |
| Final count-change convergence after last COMMIT | report all | `<=400 ms` |
| Combined foreground+worker RSS | `<=32 MiB` | `<=24 MiB` |
| Individual owned buffer | `<=1 MiB` | same |
| Pending/in-flight | `<=1 / <=1` | same |
| Projection SQLite writes/transactions/COMMITs | `0 / 0 / 0` | same |
| Unexpected Busy/Locked | `0 / 0` | same |

Protect foreground operations with the dual G5 materiality rule. Report active/
temp logical, apparent, and allocated bytes and reader-retained old generations;
do not infer unique physical bytes from APFS clone allocation.

### G5-2 exit

Exact/latest semantics, root-correct repeated rotation, chain/fallback rules,
faults, SQLite contention, latency/throughput, resource/storage bounds,
protected operations, analyzers, cleanup, and custody all pass. Repair any
failure in a new version; do not start G5-3 until PASS.

## Milestone G5-3 — history, concurrency, and lifecycle closure

### Objective

Using qualifying H11 authority, prove that current-root work and bounded
projection remain stable across retained history and basic concurrent use.

### Required compact matrix

- retained revisions N=1/10/100/1,000;
- same-byte hotspot edits and deterministic random edits;
- A/B alternation and revert;
- current/historical range reads and exact historical materialization;
- multiple immutable readers plus one canonical writer;
- same-target projection conflict and exactly one projection writer;
- worker read vs foreground writer progress;
- active drain/cancel shutdown and abandoned-temp restart;
- branch/revert history;
- explicit retained roots and read-only reachability;
- complete-store backup/restore with rollback classification;
- aggregate work, latency, Q/RSS, descriptors, buffers, queue, native storage,
  SQLite growth, and cleanup.

Current-root direct work must not enumerate retained history. For deterministic
non-genesis mechanism classes, exact SQL/query/row/BLOB/authenticated-object
work must match the frozen expected class; latency uses the dual materiality
rule. Preserve/report the H11 diagnostic storage model:

```text
6 objects
23,030 canonical bytes
2,255 mapping bytes
about 24.9 KiB SQLite image per unique revision
```

Do not relabel these diagnostic slopes as general population claims. G5-3 does
not add destructive GC. If a later product requirement authorizes GC, it starts
separately with explicit retained roots, reader/projection pins, read-only mark
audit, and exclusive stop-the-world sweep; mark failure means no sweep.

### G5-3 exit

History classes, current/historical identity, concurrency, writer progress,
shutdown/restart, reachability, resource/storage bounds, protected operations,
two recomputations, custody, and cleanup pass in a `<=120 s` complete gate.
Repair any failure in a new version before terminal audit.

## G5 terminal audit

Perform a fresh read-only audit wave after G5-3. Recompute every milestone from
raw data and manifests; do not trust milestone summaries alone.

G5 terminal PASS requires all of:

1. qualifying corrected H11 whole-harness authority;
2. TrustedLocalDev direct semantics and speed while verified mode remains intact;
3. no trusted capability laundering or verified carry-forward;
4. exact/latest projection semantics and root-correct repeated seed rotation;
5. one-slot pending/in-flight bounds and exact conservation equations;
6. zero projection SQLite writer transactions/COMMITs and zero unexpected
   Busy/Locked;
7. combined memory/Q/buffer/descriptor/native-storage bounds;
8. history/concurrency/lifecycle closure;
9. protected create/edit/reopen/range/reconstruction/full-materialization/
   fallback results with no material regression;
10. exact errors, one canonical transaction/COMMIT, durability, reconciliation,
    old-or-new native publication, and terminal cleanup;
11. workspace tests, clippy `-D warnings`, fmt, tracked/untracked diff/whitespace,
    source/diff/executable/fixture/base/sidecar hashes, primary and independent
    analyzers, complete versioned manifest, and terminal read-only verification;
12. explicit limitations: warm/fresh/cold class, physical-I/O unavailability,
    process-lifetime seed, rollback freshness `NotProtected`, append-only/no-GC,
    benchmark-private/non-production, and no 500-MiB claim.

Generate a versioned G5 terminal report, final scoreboard, complete manifest,
hash list, limitations, and G6 handoff. Update the Phase 4 roadmap/index without
erasing G4 or G5 failed history. Mark `G5 PASS; G6 eligible` only when every
hard gate above passes. Stop before G6 and leave the work uncommitted.

## Autonomous repair loop

For every intermediate failure:

```text
preserve failed attempt and hashes
-> classify semantic / method / performance / resource / custody root cause
-> launch fresh targeted subagents
-> synthesize against source and raw evidence
-> repair the smallest shared cause or choose a different in-scope mechanism
-> focused tests
-> zero-row dry-run
-> new <20 s screen
-> one new <=120 s gate only after screen/static PASS
-> fresh audit
```

Do not stop at a candidate `NO-GO`; a mechanism no-go means select another
mechanism within the same milestone invariants. Do not repeatedly rerun
unchanged source hoping for noise. Do not amend thresholds after observation.
Do not broaden into format/schema/profile/WAL/retry/pool/VFS/GC/G6 work to
rescue a G5 candidate. The terminal target is an evidence-backed PASS, never a
negotiated or relabeled PASS.

## Final response

Return one self-contained terminal handoff containing:

- `G5 PASS` disposition and explicit `G6 eligible; not started`;
- milestone versions, source/diff/executable hashes, artifact roots, manifest
  counts, and read-only verification results;
- changed files and exact test/static results;
- verified/trusted correctness and threat-boundary matrix;
- control/candidate p50/p95/max, paired ratios/deltas, timer equations, direct
  work, SQL/BLOB/COMMIT, Q/RSS/buffer/descriptor/storage, Busy/Locked, queue/
  build/seed-rotation, and cleanup results;
- before/after Big-O table with unchanged lower bounds called out;
- protected-operation scoreboard and all material/nonmaterial regressions;
- every preserved REVISE/NO-GO attempt and the repair that superseded it;
- unsupported/Unavailable observations and limitations;
- confirmation that no production integration, G6, WP5, profile/format/schema,
  persistent seed, GC, sibling worktree, commit, reset, clean, or evidence
  deletion occurred.

---
