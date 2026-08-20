# Full-Create Optimization — Read Only After M4.5 Passes

Status: future execution note. Do not begin this program merely because M4.5
compiles, finishes its debug tests, or reports a fast same-middle edit. Begin
only after M4.5-6 is complete and an independent read-only audit accepts the
terminal implementation and release evidence.

Scope: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` on
`codex/empty-worktree`. Never modify the sibling `layerfs` repository. Preserve
the dirty tree and do not commit unless the user explicitly asks.

SQLite remains the authoritative durable Phase 4 engine. Memory is a later
semantic-parity/shared-core ceiling, not a substitute for durable SQLite.

## 1. Entry gate

Do not start F0 until all of these are true:

- M4.5-0 through M4.5-6 have terminal reports and exact evidence hashes.
- The corrected `C0` full-closure and `C1` changed-spine same-count edit paths
  produce identical roots, transitions, closure identities, reconstructed
  bytes, CDC sequence, range bytes, and publication outcomes.
- The transaction-owned same-open witness, complete-head comparison, one-use
  invalidation, ABA/genesis handling, lost-ack reconciliation, typed errors,
  exact live-Q accounting, and SQL/W/D labels have passed independent review.
- The retained 100-MiB same-middle release A/B satisfies its predeclared
  correctness, latency, counter, CPU, Q, RSS, and storage gates.
- The independent audit verdict is `PASS` or
  `PASS-WITH-NONBLOCKING-FINDINGS`; no authority, identity, closure,
  publication, durability, delta, malformed-input, exact-Q, or benchmark P0 is
  open.
- The accepted M4.5 terminal HEAD, complete dirty diff, release executable,
  fixture, prepared bases, raw rows, reports, commands, and toolchain have been
  frozen.

If any item is missing, resume M4.5 at the first unproven milestone. Do not use
full-create work to distract from or hide an unresolved small-edit defect.

## 2. Objective and retained baseline

The primary objective is the exact retained 100-MiB K64/F64 full-create
**durable-capture** path:

```text
source processing begins
  -> source read + frozen CDC
  -> canonical CAS mapping and object persistence
  -> complete pre-COMMIT qualification
  -> one durable SQLite COMMIT
  -> all measured handles/state dropped
```

The retained M3 baseline is:

| Durable phase | Median |
|---|---:|
| Canonical CAS mapping and object persistence | `410.776 ms` |
| Pre-COMMIT closure validation | `388.155 ms` |
| SQLite COMMIT durability | `152.996 ms` |
| **Durable capture total** | **`953.829 ms`** |
| **Durable-capture throughput** | **`104.841 MiB/s`** |

Post-COMMIT evidence is measured and named separately:

| Verification phase | Median |
|---|---:|
| Fresh reopen/head | `1.155 ms` |
| Fresh full closure scrub | `272.815 ms` |
| Reconstruction | `429.985 ms` |
| Range verification | `0.656 ms` |
| **Complete lifecycle** | **`1,663.449 ms`** |
| **Complete-lifecycle throughput** | **`60.116 MiB/s`** |

Phase medians are independently selected and do not arithmetically reproduce
the total median. Every raw row must satisfy the exact disjoint timer equations.

The target is:

```text
primary durable target: <= 500.000 ms = >= 200 MiB/s
stretch durable target: <= 333.333 ms = >= 300 MiB/s
```

Never apply these thresholds to complete-lifecycle time, edit latency, debug
rows, logical-file-equivalent rates, or a Memory row.

## 3. Why more than one optimization is required

Removing all retained pre-COMMIT validation time gives only this planning
estimate:

```text
953.829 ms - 388.155 ms = 565.674 ms
100 MiB / 0.565674 s    = approximately 176.8 MiB/s
```

Therefore duplicate validation removal alone cannot reach 200 MiB/s. At least
another approximately `65.674 ms` must be removed from mapping/CAS, SQLite
crossings, COMMIT, or another directly observed durable phase.

This is Amdahl-style prioritization arithmetic, not benchmark evidence and not
a promise that any candidate will realize the full subtraction.

## 4. Frozen contracts

### CDC

The Phase-2 FastCDC contract remains exact:

```text
minimum = 8 KiB
target  = 16 KiB
maximum = 32 KiB
```

Do not change the boundary algorithm, chunk sizes, raw `ChunkId`, ordered CDC
sequence, or retained fixture to win the create benchmark. `K/F` are mapping
profile parameters, not CDC chunk sizes.

### CAS and canonical identity

- Preserve Phase-1 canonical Bytes and Directory identity.
- Validate canonical bytes before deriving or trusting semantic identity.
- Calculate ObjectId from the exact canonical bytes once where authority
  permits; do not re-encode or re-hash without a semantic need.
- Keep CAS immutable. An insert conflict/reuse must authenticate and compare
  the incumbent; it is not an existence-only success.
- Preserve exact created/reused/authenticated object and byte counters.

### Mapping, COW, root and delta

- Preserve Phase-3 workspace root, parent, delta operations, replay, COW, and
  complete strong-edge closure semantics.
- Preserve canonical K/F partition, nonfinal fullness, minimal height,
  cumulative summaries, directory order/adjacency/duplicate rejection, and
  typed malformed-input failures.
- Do not regress M4.5's accepted same-count path-local edit algorithm.
- Keep count-changing `+1` edits honestly suffix-linear under fixed ordinal
  grouping; no logarithmic claim.

### SQLite durability

- One synchronous caller-thread writer transaction.
- One publication COMMIT for the complete visible head.
- Root/delta/receipt/generation/authority publication is atomic.
- Preserve `synchronous=FULL` and the authorized rollback-journal path.
- No WAL switch, second database, append-only/pack path, workers, async, pools,
  VFS, hidden retries, or second durability boundary.
- Ambiguous COMMIT outcomes use fresh independent reconciliation.

## 5. Resource and metadata policy

Hard constraints:

- no `O(source bytes)` resident bytes, reference vector, visited set, or cache;
- no unbounded SQL parameters/text/results, object map, output, or batch;
- checked length/count/capacity arithmetic and exact overflow errors;
- exact summed-live `Q`, including overlapping owned capacities, returning to
  zero on all exits;
- external RSS, CPU, physical I/O, SQLite cache, and filesystem effects are
  separate `Observed`/`Unavailable` evidence, never substituted with Q or zero;
- no new serialized optimization metadata by default.

Constant memory limits may be relaxed aggressively but only under a
preregistered fixed cap. A larger bounded source buffer or insertion group is
acceptable when it preserves the same asymptotic class, has exact Q charges,
passes 100/512-MiB scaling checks and the analytical 100-GiB bound, and buys a
material measured phase improvement. A source-sized structure is not a
relaxation; it is a rejected algorithm.

The full-create construction proof, work receipts, and insertion groups must
be private, operation-local, transaction-owned, bounded, and nonserializable.
Any future persistent metadata requires a separate format/profile amendment,
exact canonical/physical overhead equations, independent goldens, 100/512-MiB
measurements, analytical 100-GiB projection, and explicit user approval.

For every row, report separately:

- source and raw bytes hashed;
- canonical chunk bytes encoded/hashed/written/authenticated;
- mapping/root/delta/receipt metadata bytes;
- objects created/reused/authenticated;
- SQL acquisitions/native prepares/executions/queries/rows and BLOB operations;
- `W` newly written canonical bytes and `D` under controlling definitions;
- logical, apparent, and allocated bytes for the database and every endpoint;
- CPU, exact Q, external RSS, peak footprint, sync and physical I/O where
  genuinely available.

## 6. F0 — Freeze accepted M4.5

F0 contains no performance source change.

Record:

- terminal branch, HEAD and parent;
- complete staged/unstaged/untracked implementation-diff SHA-256;
- all changed source-file hashes;
- accepted M4.5 release executable and source hashes;
- retained fixture/manifest and prepared-base hashes;
- M4.5 `C0/C1` raw JSONL and external-observation hashes;
- exact commands, Rust/SQLite/toolchain and host environment;
- independent audit report and terminal verdict.

Create the full-create control executable from this accepted source only after
correctness gates pass. Do not treat the old M3 executable as the new control;
retain M3 only for historical continuity.

## 7. F1 — COMMIT and physical-I/O attribution

Before changing write shape, distinguish:

- COMMIT dispatch, acknowledgement, and reconciliation;
- sync call count and sync wall time when observable;
- SQLite page-cache current/high-water, dirty pages and spill state;
- main database and rollback-journal write calls/bytes;
- journal/temp peak allocation;
- process user/system CPU;
- logical/apparent/allocated endpoint bytes.

Unavailable observations remain `Unavailable`. Do not use logical file length
as physical bytes, report a zero for an unsupported counter, or claim cold APFS
without a real observed conditioning mechanism.

F1's output is a report and counter baseline. If durable sync is already the
irreducible dominant floor with minimal writes/syncs, say so instead of adding
speculative SQLite settings.

## 8. F2 — Transaction-local full-create construction proof

This is the first major full-create algorithm milestone.

The current construction path already establishes useful facts while streaming:

```text
source chunk
  -> raw ChunkId
  -> canonical bytes
  -> ObjectId
  -> immutable insert OR fully authenticated incumbent reuse
  -> exact leaf summary
  -> exact branch summary
  -> mapping root
  -> workspace root + delta expectation
```

Carry that proof forward through a bounded builder frontier:

```text
memory = O(K + F*H + bounded chunk/page/SQL buffers)
```

No all-object receipt list is allowed. Fold evidence into leaf/branch/root
summaries as objects are constructed.

Implementation order:

1. Return opaque transaction-local evidence for a newly inserted verified
   object or a fully authenticated incumbent reuse.
2. Feed it into bounded leaf/branch/root summaries.
3. Keep complete pre-COMMIT closure verification as the authoritative `C0`.
4. Run bounded construction proof as shadow `C1` and require exact agreement.
5. Add adversarial missing/corrupt/incumbent/conflict/wrong-summary/root/delta
   tests.
6. Only after equivalence is proven may `C1` omit duplicate database replay.
7. Keep post-COMMIT reopen, scrub and reconstruction independent and complete.

Expected direct movement:

- approximately one complete pre-COMMIT closure pass disappears;
- pre-COMMIT object/edge/SQL/row/BLOB authentication falls according to exact
  construction-proof equations;
- source read, CDC, canonical creation, ObjectIds, roots, transition, closure,
  W, storage, transaction count and COMMIT count remain exact.

Classification: pass elimination. Overall full create remains
`Theta(source bytes + references)`.

## 9. F3 — Bounded immutable CAS insertion groups

Start only if F1/F2 counters show per-object SQLite crossings dominate the
remaining mapping phase.

Use both a fixed row cap and a fixed canonical-byte cap. Select exact candidate
caps in the milestone preregistration; do not copy an example into production
without measurement.

Every group must preserve:

- complete per-input canonical validation and classification;
- duplicate IDs and duplicate occurrences within the group;
- immutable insert behavior;
- full incumbent authentication on conflict/reuse;
- exact created/reused/authenticated counters;
- bounded SQL text, parameters, results and canonical buffers;
- one existing writer transaction and one final COMMIT.

The expected benefit is fewer SQLite API/statement crossings. Total row and
byte work stays linear. Reject or revise if mapping improves but COMMIT,
journal allocation, RSS/Q, or durable total regresses enough to erase the win.

## 10. F4 — Optimize one measured residual at a time

Break the remaining mapping phase into non-overlapping counters/timers where
possible:

- source read;
- CDC boundary scanning;
- raw chunk hashing;
- canonical framing/encoding;
- ObjectId hashing;
- SQLite binding and insert execution;
- conflict/incumbent authentication;
- leaf encoding;
- branch encoding;
- workspace root and delta encoding.

Do not double-count nested timers. If an API makes two stages inseparable,
label them inseparable rather than inventing a split.

Choose only the dominant observed remainder. Candidate narrow milestones are:

- remove one proven duplicate canonical encode or hash;
- reuse one bounded canonical buffer through the SQLite call;
- remove an existence probe when insert/conflict supplies the same decision
  and incumbent authentication remains exact;
- increase/reuse a bounded CDC input buffer while preserving exact boundaries;
- reduce native SQLite preparations only when native preparations—not cached
  statement acquisitions—are directly observed.

Each candidate gets its own control, predicted counter equation, A/B and
retain/revise/revert decision. Do not bundle CAS, CDC, hashing and SQL changes.

## 11. F5 — Retained 100-MiB decision campaign

Run only after focused/full correctness gates pass:

1. Build each frozen release executable once with `debug_assertions=false`.
2. Reject campaign/throughput invocation from a debug build.
3. Use the exact retained raw source and ordered CDC fingerprint/count.
4. Generate/preflight source and separately prepared isolated database images
   outside all timers.
5. Run one uncounted warmup per arm.
6. Run five adjacent balanced isolated `AB/BA` pairs.
7. Preserve every raw JSONL row and external macOS observation.
8. Verify exact timer and counter equations in every row.
9. Publish medians, min/max/spread, paired deltas and wins.

Material performance acceptance requires:

- at least 5% improvement in the preregistered affected metric;
- at least 4/5 paired wins;
- predicted direct counters move exactly;
- identities, closure, reconstruction, ranges, transaction/COMMIT and storage
  remain exact;
- CPU, Q, RSS, metadata, journal and endpoint allocation pass their protected
  gates.

Interpret durable-capture outcomes:

```text
<=500 ms: primary 200-MiB/s target reached
500-566 ms: inspect residual mapping and COMMIT evidence
>566 ms: construction-proof prediction missed or another phase expanded
COMMIT dominated: investigate only observed write/sync amplification
```

Do not run 512-MiB rows or the full profile campaign until the retained 100-MiB
result is internally consistent. Do not hide poor performance; stop stacking
changes and explain the dominant measured cost.

## 12. F6 — Complete-lifecycle optimization

Begin only after durable capture is stable. Preserve fresh verification as an
independent authority boundary.

### Fresh scrub

Reuse shared bounded, ordered, duplicate-preserving, missing-object-detecting
batch/walker machinery. Reduce database crossings and copies without reducing
required objects, edges, canonical bytes, raw hashes, closure order, or exact
error identity. Scrub remains linear in reachable authenticated closure.

### Reconstruction

M2 already reduced reconstruction statement acquisitions from approximately
`5,371` to `170`, but wall time improved only `7.169%`. Do not assume further
statement reduction is dominant. Measure row materialization, canonical
authentication passes, BLOB work, parser/hash passes, and output copies.

A borrowed/streaming path must authenticate bytes before semantic exposure,
must not let a SQLite row lifetime escape, and must keep output memory bounded.
Reconstruction remains linear in produced bytes.

Range verification remains exact, boundary-derived and separately timed.

## 13. Protect the accepted small-edit algorithm

After every full-create milestone, rerun focused M4.5 regressions:

- same-open authority issuance/invalidation and adversary cases;
- `C0/C1` exact semantic agreement;
- same-count changed-leaf and ancestor-spine counters;
- no complete pre-COMMIT closure replay;
- exact edit oracle, root, transition, closure, reconstruction and ranges;
- one transaction/COMMIT and lost-ack reconciliation;
- exact Q balance and no source-sized/all-reference state.

Full-create batching or construction proof must not weaken or materially slow
the accepted same-count path. Edit primary metrics remain latency and exact
changed work—not whole-file-size divided by edit wall.

## 14. Per-milestone execution contract

Before editing:

- state the one bottleneck and one-variable hypothesis;
- classify it as asymptotic, pass elimination, SQL-crossing reduction,
  copy/hash reduction, or diagnostic;
- define before/after algorithm and memory bounds;
- preregister the expected direct-counter equation;
- define affected phase, minimum useful effect, samples and decision rule;
- freeze correctness, CPU, Q, RSS, metadata/storage and COMMIT guards.

Before timing:

- run exact focused codec/identity/malformed/delta/closure/range/publication
  tests as affected;
- run same-count edit protection tests;
- verify fixture, source, prepared image and executable custody;
- run A/A calibration when timer/instrumentation substrate changed.

Before advancing:

- write the milestone report;
- update the rolling benchmark baseline;
- record implementation diff and every artifact hash;
- include raw rows, commands, equations, paired effects and resource evidence;
- issue exactly one retain/revise/revert/inconclusive decision;
- stop for review when correctness, counters and wall movement disagree.

A deterministic 0–5% work reduction may be retained as a mechanism-only
constant-factor change only if it adds no correctness, complexity, CPU,
memory, storage, or maintenance cost. It is not a throughput PASS. An
inconclusive candidate is not stacked under the next experiment.

## 15. CAS + CDC + COW optimization target

The intended optimized shape is:

| Layer | Target |
|---|---|
| CDC | One bounded source pass; exact frozen 8/16/32-KiB boundaries; no per-byte/per-chunk allocation amplification |
| Canonical CAS | One canonical construction and necessary ObjectId hash; bounded immutable insertion; authenticated conflict/reuse; no duplicate source-sized pass |
| File mapping | Bounded `K/F` frontier; exact summaries; no all-reference vector |
| Full create | `Theta(source bytes + references)` with no duplicate full closure replay and bounded SQLite crossings |
| Same-count COW | Changed CDC region + one leaf + ancestor spine; authority-correct changed-spine qualification |
| `+1` COW | Bounded persisted-reference streaming/spool; honest suffix work |
| SQLite | One transaction/COMMIT; bounded statements/batches; measured sync/physical allocation; no durability weakening |
| Memory | Later semantic-parity/shared-core ceiling, not durable throughput authority |

Big-O establishes scaling safety. Balanced release A/B establishes speed.
Neither substitutes for the other.

## 16. After F6

This program stabilizes the current candidate; it does not itself finalize
Phase 4. Continue under the controlling sequence:

```text
WP4-M  required 100/512-MiB file and directory profile campaign
WP4-P  choose one profile, delete losers/selectors, regenerate goldens
WP5    finalize the promoted shared mapping
WP6    implement real Memory semantic parity
WP7    integrate promoted mapping into production SQLite
WP8/9  establish fair unchanged-source Memory/SQLite baseline
WP10   optimize measured duplicate authentication/closure
WP11   optimize measured SQLite statements/transactions
WP12   optimize measured shared encode/hash/copy/CDC/COW work
WP13   storage-backend compatibility audit
WP14   stable-source final campaign and decision
```

Any optimization present during profile selection must be profile-neutral or
applied equivalently to every profile. Never compare an optimized default with
deliberately unoptimized challengers.

WP14 decides exactly one outcome:

1. retain SQLite after reaching at least 200 MiB/s durable capture;
2. retain SQLite and continue shared-core optimization because the Memory and
   SQLite evidence shows an engine-agnostic limit; or
3. authorize a separate specification for one named third backend only when
   optimized SQLite still misses the target and directly measured
   SQLite-specific cost dominates.

Do not implement a speculative third engine during F0-F6.

## 17. Required evidence references

- [M4.5 specification](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/wp4m/milestones/m4-5/spec.md)
- [M4.5-to-Phase-4 handoff](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/wp4m/f-series/planning/finalization-handoff.md)
- [Retained full-create lifecycle](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/wp4m/f-series/planning/retained-100-mib-lifecycle.md)
- [Detailed post-M4.5 plan](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/wp4m/f-series/planning/read-after-m4-5.md)
- [Rolling optimization ledger](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/wp4m/progress.md)
- [Phase 4 implementation plan](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/rollback/implementation-plan.md)
- [Algorithm specification](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/algorithm/spec.md)
- [Algorithm test contract](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/algorithm/tests-and-benchmarks.md)
- [Complexity analysis](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/algorithm/complexity-analysis.md)

## 18. Full-create completion condition

The immediate full-create program is complete only when:

- the accepted M4.5 small-edit result remains correct and protected;
- the retained 100-MiB durable-capture row is internally consistent in release;
- required phase/counter equations reconcile in every raw row;
- full create remains linear with bounded resident memory and controlled
  metadata;
- CAS/CDC/mapping/SQLite work has no known duplicate dominant pass;
- CPU, Q, RSS and physical/logical storage are accepted or honestly classified;
- the primary `<=500 ms` target is reached, or the remaining dominant limit is
  measured precisely enough to route the formal WP10-WP12 work;
- every retained/rejected experiment and exact artifact hash is recorded; and
- no qualification, profile-promotion, production-integration, or Phase 4 final
  claim is made ahead of its controlling work package.
