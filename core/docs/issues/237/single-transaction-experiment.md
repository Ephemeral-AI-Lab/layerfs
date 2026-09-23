# #237: one write transaction across the 10k file Save

> **Status:** Rejected feasibility design before a candidate commit, build or
> public sample. Retained as an iteration record. SQLite database pages remain
> **4,096 bytes** and the construction cutoff remains **128 KiB**.

This proposal was superseded by the [bounded-wave experiment](bounded-wave-experiment.md).
The [uncommitted design diff](evidence/single-transaction/rejected-long-transaction.diff.gz)
was not built or timed. `with_wave` releases the Store arbitration mutex after
each wave; keeping SQLite's write transaction open across that release would
make a concurrent writer's nonwaiting `BEGIN IMMEDIATE` fail between waves.
That knowingly breaks the current multiwriter behavior and obscures the
transaction-cost question. The owner selected larger bounded waves that still
commit before the mutex is released. The rest of this file is the original
prospective plan, preserved for audit, not an active run plan.

## Cancelled precursor

A 512 KiB default construction cutoff was considered and briefly edited in this
isolated worktree, then cancelled at the owner's direction **before any commit,
build or timed sample**. All those edits were restored. No performance or
readback evidence exists for that hypothesis.

## Mechanism and predicted limit

The integrated baseline source is `480de7464`. Its C2 `flush_batch` admits at
most 512 objects and 4 MiB canonical bytes per wave. `with_wave` takes Store
arbitration, does a wave under one SQLite transaction, then calls
`maybe_commit` before releasing arbitration. The 10k file Save therefore pays
roughly 80 `BEGIN IMMEDIATE`/`COMMIT` cycles. The **primary mechanistic target**
is `SaveOutcome.commits <= 8` for the candidate's file Save versus roughly
80 for control, ideally **2** total (slot reservation and final publication).
An earlier, separately
instrumented 10k Save charged **227.521 ms** to all COMMIT calls; this is an
upper bound on the saving from eliminating those calls, not a matched
prediction. One large final COMMIT may cost more than one small COMMIT, and
connection close is within the public operation. The integrated public caller
currently has a **1.252325 s** raw observation, so even a hypothetical 227 ms
caller saving would leave about **1.025 s**, above the historical 0.578245 s
time. The earlier row's commit total cannot be subtracted from the integrated
row to claim an outcome.

The candidate will keep wave membership, dependency checks, collision checks,
group queue flushes, worker count and pack grammar unchanged. It will retain
the write transaction across successive waves of **one Save** and acknowledge
it with final publication. The finished Save will still expose its output only
after publication. Failure before that point must roll back this transaction
and leave no published output. Same-save reads use its own connection and must
read objects written by prior waves. The candidate widens the transaction
accounting limit to 1 GiB / 65,535 rows while retaining the 4 MiB / 512-object
wave. It commits and starts a fresh transaction before the next wave if the
accumulated transaction would exceed that bound. The 10k file Save is expected
to fit within one such transaction; the exact counted bytes and rows must be
measured. Any earlier forced ordinal-reservation commit is separately counted.

This experiment deliberately gives up the current per-wave multiwriter
fairness: SQLite admits one writer, so another save may fail its nonwaiting
`BEGIN IMMEDIATE` while this Save is active. It also raises a material RSS and
rollback-journal risk under the current MEMORY journal. Those are explicit
reasons to reject product adoption even if the single 10k row is faster. The
candidate must not silently relax 4 KiB SQLite pages, change source caching,
or move any source read or Store write outside the public timer.

## Prospective comparison

1. Control: `480de7464` algorithm plus the same once-per-file-Save diagnostic
   code used in both arms, committed as its own control identity; one
   release-profile public `namespace-10000` Init at a fresh output path.
2. Candidate: one distinct committed product identity with only the
   transaction-lifetime treatment layered on that instrumented control; one
   release-profile public Init at another fresh output path.

The shared Service diagnostic prints the file Save's exact `SaveOutcome`
counts once, after `finish` and still inside the public caller timer. Its
[source diff](evidence/single-transaction/shared-saveoutcome-instrumentation.diff.gz)
is retained. The copied H2 research driver is SHA-256
`767292e7b5bfc9a06291239b471247a79474b91ce2ca0db8fc9092807e2c09a4`;
both arms use this exact file with `--independent-source-copy
--fixed-operation-identity` and a distinct `--out` path. No full-cache or
fully cold metadata claim follows from that driver.

Both arms use the same sealed seed-1 fixture: **10,000 files and 300,000,000
logical bytes including the 100 MB anchor**. The sealed master may be reused
only to prepare a **new independent writable byte copy and path for each arm**.
Both calls use a fixed stack/scope identity and the same runner, fixture
manifest and H2 `--independent-source-copy` method. Each copied source is fully
hashed, its payload pages invalidated, and its whole payload checked
nonfaultingly at zero residency immediately before the timer. The parent's
host capability check found `/usr/sbin/purge` unavailable (`Operation not
permitted`) and `sudo -n purge` required a password. Do **not** use
`--purge-before-operation`. New paths prevent reuse of metadata from a
previous measured run, but copy/setup metadata may remain cached, so neither
arm qualifies as a fully cold metadata row. Label the pair exploratory and
`INELIGIBLE` under the unchanged runner cache contract. **Neither arm may run
until a host measurement window is supplied.** No warm source payload from an
earlier run may credit either timer. One sample per arm; preserve `NOT_RUN`,
failure, `INCOMPLETE` and `INELIGIBLE` rows. Do not repeat a sample to select a
time.

Record source, binary, harness and fixture identities; page size; caller and
complete-command wall; the file Save's exact `SaveOutcome.commits`, inserted
objects and `profile.commit_ns`/`diag.begin_ns` from identical once-per-Save
diagnostics in both arms;
Service/daemon CPU and RSS; temporary memory-journal use if observable; and
closed-Store apparent/allocated bytes, object/pack/group counts, pack capacity,
used bytes and spare bytes. The exact root and full stored object-ID digest
should match under fixed IDs. Independently reopen the candidate Store and
verify every path, kind/mode/mtime and all 300 MB of file content against the
prepared manifest; verification wall is outside the throughput number. The
same check on the control is required for a matched integrity comparison.

Run focused same-save read, failure rollback and Store-open checks at the
candidate source identity. Multiwriter tests may fail under the documented
fairness tradeoff; retain those failures, do not relabel them as a product
PASS. A dense 10k Store does not prove #229 sparse-history compactness, so
that separate proof remains `NOT_RUN` unless actually run. The historical
518.8 MB/s target is **0.578245 s** for this numerator and must be reported
as missed if the candidate is slower.

## Attempts and result

No product candidate or public sample yet. The stronger purge-based cache
preconditioner was rejected by the host; the independent-copy protocol above
replaces it prospectively. The host window is pending. This record will append
the command, output paths, all attempt statuses and decision without rewriting
earlier evidence.
