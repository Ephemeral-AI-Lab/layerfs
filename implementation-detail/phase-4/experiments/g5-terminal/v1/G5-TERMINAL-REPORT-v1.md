# Phase 4 G5 terminal report

Disposition: **G5 PASS; G6 eligible; G6 not started**.

The accepted chain is G5-0 v9, G5-1 v27, G5-2 v3, and G5-3 v3. A final
read-only follow-up found two P1 terminal-description corrections: G5-2 seed
admission makes exact/sparse end-to-end complexity linear in the seed, and one
v27 method-manifest documentation path evolved after acceptance. The
append-only `G5-TERMINAL-CORRECTION-ADDENDUM-v1.json` records both. A second
append-only `G5-TERMINAL-CORRECTION-PRECISION-ADDENDUM-v1.json` clarifies the
configured SQLite page-cache budget and retained-history complexity. No
product, source, benchmark row, threshold, or terminal measurement changed;
no P0 or open P1 remains. This closes benchmark mechanisms only; production
integration remains deferred.

## Milestones

| Milestone | Terminal result | Complete wall | Peak RSS | Main qualified result |
|---|---|---:|---:|---|
| G5-0 v9 | PASS | 9.254 s | 14,090,240 B | eight-row whole-harness history/Q/reachability authority |
| G5-1 v27 | PASS | 95.098 s | 18,563,072 B | Verified no-regression and 93.77–94.79% paired-median Trusted improvement |
| G5-2 v3 | PASS | 0.590 s | 8,093,696 B | 250,000-byte exact/sparse projection, bounded mailbox, explicit fallback |
| G5-3 v3 | PASS | 4.782 s | 18,923,520 B | 1,000 revisions, checkpoint projection, ABA/history read, 10 MiB 2R1W |

All screens are below 20 seconds. G5-1/2/3 gates are below the prospective
150-second total cap; G5-0 passed its stricter historical 20-second cap. The
accepted G5-3 input adoption reused 12,063,779 sealed bytes and took 12.4 ms;
no file was copied or regenerated.

## Verified and Trusted boundary

| Boundary | Verified | TrustedLocalDev | Terminal result |
|---|---|---|---|
| Default mode | yes | explicit opt-in | PASS |
| Store lifetime | full closure authority | trusted edit-base scope only | PASS |
| Touched/new/incumbent identity | unconditional | unconditional | PASS |
| Verified receipt-covered work | may authorize verified carry | never synthesized from trusted assumptions | PASS |
| Trusted history then Verified reopen | complete scrub required | cannot bypass | PASS |
| Publication | expected head, one transaction, one COMMIT | same | PASS |
| Lost ACK / COMMIT error | fresh requested/prior/different/ambiguous reconciliation | same | PASS |
| Rollback freshness | `NotProtected` | `NotProtected` | honestly limited |

G5-1 contains 200 measured operation rows and 56 CompleteRoundTrip
checkpoints. All seven G4-Verified versus G5-Verified operations have no
material regression. G5-Verified versus Trusted paired-median improvements are
9,377–9,479 basis points; Trusted p50 is 7.871–9.418 ms and p95 is
8.829–10.346 ms across the seven supported edits.

## Projection, history, and concurrency

G5-2 keeps semantic request policy orthogonal to execution route. Exact roots
are non-coalescible; only bounded latest-following state may replace pending
latest work. Projection SQLite is read-only/query-only with zero writer
transactions and COMMITs. The foreground retains exactly one transaction and
one COMMIT. Service samples cover Worker T3 through native ACK T4, not edit T0
through native ACK T4. The gate route samples are n=1 exact, n=67 sparse, n=1
ordinary fallback, and n=1 contended fallback. Exact and sparse gate p50/p95
are 0.828/0.828 ms and 1.265/1.469 ms. Ordinary and contended fallback remain
distinct at 1.775 and 2.806 ms; the contended class is not presented as
isolated performance. The orthogonal policy populations remain 64
ExactEveryRoot and 100 LatestFollowing submissions.

G5-3 retains 1,000 distinct 1 MiB revision roots. It verifies revisions
1/10/100/1,000, reconstructs those roots plus terminal 1,001, then completes
A→B→A and reads 4 KiB from exact historical B. Four checkpoint pairs submit,
start, and publish eight exact/latest projections with eight seed rotations,
zero coalescing, zero fallback, Q0, and zero residue.

The separate 10 MiB sentinel keeps two immutable readers open while one writer
publishes one transaction/COMMIT. Both readers observe exact prior and new
heads; the prior-root historical range remains readable. Busy/Locked are zero.
Writer and both readers observe `cache_size=1,280` pages at 4,096 bytes. Their
3 × 1,280 × 4,096 = 15,728,640 bytes is a **configured aggregate SQLite
page-cache budget**, not an observed allocation or hard memory ceiling. It is
8,847,360 bytes below the configured three-connection 2,000-page reference.
Product-process RSS remains the actual hard bound: screen 18,563,072 bytes and
gate 18,923,520 bytes against 20,971,520 bytes. Observed connections are 3
high-water and 0 terminal.

## Complexity

For the history rows below, `R` is retained revision states processed, `S` is
the Verified current-closure/file bytes per revision, and `U_R` is unique
retained SQLite/CAS objects accumulated through those revisions.

| Path | Before/Verified lower bound | Qualified G5 path | Result |
|---|---|---|---|
| Verified reopen/edit authority | Θ(S) authenticated closure | Θ(S), unchanged | no authority weakening |
| Trusted same-size edit | Θ(S) scrub before publication | Θ(changed path + touched/new authentication) | direct 93.77–94.79% paired improvement |
| Fixed-radix count change | Θ(suffix) | Θ(suffix), unchanged | fast absolute target not claimed |
| Exact native projection | full materialization Θ(S) fallback | Θ(S): whole-seed descriptor hash precedes clone; clone payload alone O(B) | qualified only for warm 250,000-byte mechanism |
| Same-offset sparse projection | full materialization Θ(S) fallback | Θ(S+B): whole-seed descriptor hash plus dirty-range work; patch payload alone O(B) | qualified only for warm same-size route |
| Different-length projection | Θ(S) | Θ(S) streamed `FullFallback` | correctness/no-mislabel only |
| Mailbox retained state | potentially request-count dependent | O(1) in-flight + O(1) bounded pending | exact/latest conservation PASS |
| G5-3 Verified history fill | each revision scrubs its current closure | Θ(R·S) total fill | exact 1,000-revision mechanism; not a sublinear-history claim |
| Current-root history work | must not enumerate R retained roots | no R enumeration; SQLite lookup may add O(log U_R) plus operation-local closure/path work | checkpoint SQL/work stable from N=10 through N=1,000 |
| Retained reachability/statistics | retained revisions plus unique objects | Θ(R+U_R) | read-only observation |
| Benchmark evidence metadata | one compact tuple per retained edit | O(R) | one final receipt; no per-edit file/fsync |
| Append-only retained storage | grows with unique retained revision work | unchanged | no destructive GC |

## Protected operations and resources

Create, edit, reopen, range, reconstruction, full materialization, fallback,
exact errors, durability, reconciliation, storage, and cleanup remain protected
by the sealed G4/G5 milestone chain. No fallback is presented as a fast route.
The final G5-3 gate has Q high-water 701,165 B and terminal Q0, maximum buffer
1,048,576 B, file descriptors 5→5, logical/apparent/allocated store
25,964,576/25,964,576/26,398,720 B, and seed/temp/work-root residue 0/0/0.

## Preserved failures

- G5-0 v1–v8 remain preserved with their exact analyzer, Q, custody, compile,
  schema, allocation, and ownership failures; v9 is the fresh-row PASS.
- G5-1 v1–v14 preserve preregistration, custody, timer, lifecycle, and harness
  corrections; v15–v17 preserve timer/analyzer attempts; v18–v22 preserve the
  failed batched-scrub/RSS lane; v23 preserves ineffective leaf batching;
  v24 preserves mechanism evidence and anti-cheat correction; v25–v26 preserve
  diagnostic/process-shape authority; v27 is the clean prospective PASS.
- G5-2 v3 attempts 1–2 are zero-row mechanical failures; attempt 3 preserves
  the analyzer-alias failure after product success; attempts 4–5 preserve the
  clone-inventory and physical-allocation observations; attempt 6 is PASS.
- G5-3 v1 preserves the 22,102,016-byte RSS screen failure; v2 preserves the
  private-copy mechanical failure and the 21,643,264-byte direct RSS NO-GO;
  v3 preserves its initial writer-only diagnostic and passes with all three
  concurrency connections bounded to 1,280 pages.

No failed row was relabeled, no post-observation threshold was weakened, and
no unchanged candidate was retried for favorable noise.

## Limitations and handoff

The controlling limitations are in `LIMITATIONS-v1.md`. They include the
warm/preconditioned label, unavailable controlled-cold/physical-I/O evidence,
rollback freshness `NotProtected`, per-child RSS, suffix-linear count change,
250,000-byte G5-2 scope, one-child G5-3 scope, read-only/no-GC reachability,
process-lifetime seed, private nonadversarial filesystem scope, and
benchmark-private/non-production status.

The v27 frozen method manifest contains 54 rows. Current live paths rehash
53/54 because `current-benchmark-scoreboard.md` was intentionally updated after
the accepted campaign: frozen 11,563 bytes / `aae8a7...`, current 12,237 bytes
/ `9184b5...`. The v27 source, executable, raw rows, analyzers, input manifest,
final manifests, and terminal decision are unchanged. This is evolved shared
documentation, not accepted-evidence corruption.

The final manifest is also the terminal hash list. Its corrected read-only
verification checks every listed file after roadmap updates. G6 is eligible
but no G6 code, experiment, measurement, or algorithm selection was started.
