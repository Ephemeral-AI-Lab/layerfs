# #237: streamed 4-MiB waves under bounded long transactions

> **Status:** isolated aggressive research design at base `970854f2c`;
> no source edit or 10k timed sample at this preregistration. This treatment
> is not selected for the root product and requires a new host window.

## Question

The prior [64-MiB preparation-wave treatment](bounded-wave-experiment.md)
cut file-Save COMMITs 80→7 but made the public call slower, raised sampled
Service RSS to 247 MB and held Store arbitration for up to 267.9 ms. Can C2
keep its existing **512-object / 4 MiB minus one byte** pending wave, consume
producer slabs as they arrive, and still acknowledge roughly 56 MiB of
successive waves in one SQLite transaction? This separates canonical-buffer
ownership from transaction span. It does not assume the MEMORY journal or
page cache stays small just because the canonical batch does.

Fixed: 10,000 files / 300,000,000 B / seed 1 first, four existing C1 Init
workers, one C2 owner, packs inside SQLite BLOBs, 4,096-B database pages,
128-KiB small-file cutoff, in-place pack append and all real source/Store work
inside the public timer. No warm payload pages from previous runs; each arm
uses a fresh independent writable byte copy of the shared prepared master and
requires 0/27,503 resident source payload pages immediately before its call.
Verification is separate; metadata cache residency remains unqualified.

## Source-backed feasibility and explicit product risk

`SaveOperation::accept` crosses each 4-MiB wave in a separate call;
`MutationOwner::with_wave` currently owns a `MutexGuard` only for that call,
commits before unlock, and the Store's cross-handle registry is process-local.
Releasing the guard with SQLite `BEGIN IMMEDIATE` still open is unsafe:
another writer has zero busy timeout and can fail. Retaining a guard across
separate `accept` calls requires self-referential ownership or a new lock
service, neither selected. Instead, the native Service can pass its existing
producer-receiver loop into one scoped C2 `SaveHandoff` streaming callback.
That callback holds a stack-local guard and transaction while it consumes
slabs immediately; the `PendingBatch` limit stays 4 MiB. It COMMITs before
the callback releases the guard, and on any normal input/storage error it
rolls back an open transaction before unlock, leaving save cleanup to the
existing outer failure path. Unknown COMMIT/rollback outcomes quarantine the
save; no retry or guessed cleanup.

An ordinary segment stops after about **56 MiB canonical input**, final
file completion, or a **5-ms receiver idle timeout**, then releases the guard.
The first slab is received outside the guard, but further producer waits and
C2 encoding occur inside it. This
deliberately violates the current architecture's short-transaction preference
and can block an in-process second writer for a long time. A second process
can fail immediately at SQLite's write lock because the process-local mutex
does not coordinate it. Multiwriter contention is an explicit no-adoption
risk, not hidden as a benchmark exception. The default Service/C2 routes
outside native Init remain unchanged in this prototype.

At roughly 300 MB / 56 MiB, six streamed segments plus save-slot reserve,
remaining-tail wave and final publication could yield **≤9 file-Save
COMMITs**, at least 90% below an exact 90-commit recent count. The
owner's ≥90% target is the first mechanistic goal.
The candidate may miss due segment boundaries, ordinal reservations, wave
packing or an explicit transaction-budget break. Do not increase worker count,
queue capacity, canonical wave bound, timeout or page size to make the count
pass. Retain every miss.

## Bounded implementation and focused checks

The Service receiver continues to process each `ImportBatch` event in order,
including `Done` after its objects. It counts canonical bytes per segment but
does not retain a second payload collection. C2 may keep one write transaction
open across successive completed 4-MiB waves under the same guard. Its
existing `wave_rows` collision validation still runs at each wave end, and
every pack/locator/candidate write remains in that transaction. A high-water
transaction charge forces a COMMIT before continuing rather than silently
growing without bound. The experimental cap and the actual largest charged
rows/bytes and lock hold must be reported; SQLite MEMORY-journal, page cache
and RSS remain measured separately, not inferred from those charges.

Before a public arm, run focused external checks for: several 4-MiB waves
under one acknowledgement; exact same-save readback and root; a failing
producer/constraint path that rolls back before unlock and cleans only its
private Save; an unknown-outcome path that does not retry; second in-process
writer progress/latency, pack-watermark collision and existing multiwriter
semantics. A failing correctness test blocks the public arm. A concurrency
regression can be retained as a measured no-go, with no root adoption.

## Prospective one-sample comparison

Build a clean integrated control from `970854f2c` in a separate worktree and
one candidate from this branch. Add the same once-per-file-Save aggregate
`SaveOutcome` reporting to both timed identities; archive the reporting diff
and pin actual binary/product/harness seals. Do not emit per-object or
per-wave stderr. Use the H3 native driver with
`--case namespace-10000 --independent-source-copy
--fixed-operation-identity`, distinct fresh outputs and independent source
copies, one control then one candidate sample, performance-only with
`verification=SKIPPED`. Do not start until the root task grants a host window.
Afterward, separately reopen both Stores and check all 10,000 files,
300,000,000 B, metadata and exact root/object IDs. Report public caller and
complete command wall, C2 COMMIT count/wall and SQL profile, actual maximum
lock span and transaction charges, process CPU/RSS, actual-owner SQLite
cache/spill counters if available, 4-KiB page size, Store apparent/allocated
bytes and pack capacity/used/slack. A 90% COMMIT reduction is not a speed
claim; reject if throughput or multiwriter/space/RSS materially regresses.
Both arms remain exploratory until metadata coldness, telemetry and full
verification are qualified.

## Focused count finding before a public arm

The first external 10-MiB stream check returned six COMMITs while rollback
passed. Source tracing found that `finish_inner` still seals each leftover lane
outside a wave and COMMITs each seal separately before its publication
transaction. The earlier rejected large-wave branch already tested a way to
coalesce final lane seals, validate their `wave_rows`, and publish under one
arbitration hold/transaction. This streamed prototype will apply that final
coalescing **only to saves that used the new stream callback**; ordinary save
routes retain their existing finalization. The check's `<=4` expectation is
kept as the focused gate. No 10k public operation has run.

## Frozen 10k exploratory go criteria

Compare the two **new** arms in one host window, control first. The file-Save
mechanism must report **≤9 COMMITs** against its own matched control's exact
count, with maximum accumulated transaction charge **≤8,191 rows and ≤128
MiB submitted bytes**. A raw candidate public call must be no slower than
its matched control. Its Service sampled maximum RSS must be no more than
control plus **16 MiB**; this is a broad sampled screen, not a phase peak.
Its Store apparent bytes and pack BLOB capacity may each grow by no more than
**1 MiB**, with allocated bytes reported separately. The longest streamed
arbitration hold must be **≤200 ms**. Both arms need identical canonical root
and ordered object IDs, full separate 10k/300-MB reopened readback, 4-KiB
pages and 128-KiB cutoff. Any definite failure, unknown persistence outcome,
missing metric or cache mismatch is a no-go. These are exploratory gates,
not release qualification: cross-process zero-busy contention and metadata
cache state remain unresolved even if every numeric gate passes. Retain all
failed rows and do not rerun an unchanged arm.

## Terminal result, 2026-09-23

The root task granted one matched control-then-candidate 10k window. Both
public operations completed once, with 0/27,503 resident source payload
pages, and both separate full readbacks passed. The candidate missed the
≤9-COMMIT, ≤200-ms lock-hold and ≤16-MiB sampled-RSS-growth gates; it is not
adopted. The exact side-by-side data, source identities, statuses and evidence
limits are in the [no-go report](streamed-transaction-result.md) and
[retained pair summary](evidence/streamed-transaction/pair/summary.json).
