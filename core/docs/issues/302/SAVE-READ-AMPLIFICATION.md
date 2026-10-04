# Save/custody read amplification follow-up — issue302

Status: implementation mechanism review; no new speed/admission claim. Prior
localized7230d62f1 campaign remains frozen, recorded and unchanged. Existing
proof-content79.76%/74.06% reductions are separate from performance VFS traffic.

## Existing phase evidence, validated from raw logs

| Stride | Arm | Filesystem VFS requested B | Save/custody VFS requested B | Save/custody pack acquisitions |
| --- | --- | ---: | ---: | ---: |
| 10 | Reference | 400,667,566 | 812,164,935 | 16,484 |
| 10 | Candidate | 43,003,671 | 1,873,421,324 | 27,722 |
| 3 | Reference | 935,582,284 | 3,140,227,191 | 70,527 |
| 3 | Candidate | 395,619,626 | 7,137,391,744 | 119,524 |

These sum `HISTORY_ENGINE_WORK` disjoint per-state phase differences, values11
(VFS requested read bytes) and15(pack acquisitions), with exact state/counter
availability checks. They never sum cumulative provider counters, are not device
traffic, and do not isolate localization from original/candidate layout changes.
[Validated phase counts](checks/save-acquisition-sharing1/prior-phase-counts.json).

## Concrete duplicated ordinary input and bounded fix

Save's `wave` discovers physical candidates through `State.packs` and shares
ordinary decoded groups. Pooled `record` still acquired the same group's encoded
input through `PoolReader.packs` before consulting the shared decoded cache.
Depth discovery, exact reuse and pooled reconstruction therefore could open
another SQLite BLOB transaction/handle for a group the owner already acquired.

A public exact-reuse Save fixture observes pack IDs[2,1,2]: ordinary pack2 is
acquired twice. Failure is retained. The fix borrows existing owner encoded and
decoded ordinary caches together through `PooledCaches`; depth discovery,
reconstruction and exact reuse share the already-acquired immutable inputs.
The same fixture needs ordinary pack2 only once. No additional cache, canonical
base memo, expanded prefetch or policy/buffer/worker/transaction change.

Pooled value-group input keeps its existing reader-local cache and write
invalidation. Private ordinary groups remain sealed before locator visibility;
hits still validate descriptors, lanes, extent/record grammar, canonical IDs,
roles/lengths, dependency/cycle/chain work and eligibility. Same2MiB/4096body,
512KiB decoded/value,32MiB/4096output and singleton bounds. Public standalone
pooled-reader APIs retain their original owning cache path.

## SQLite navigation hypothesis and deferred alternatives

SQLite's official btree implementation follows overflow-page links and maintains
a cursor-local overflow-page lookup array. A separate BLOB handle can repeat
navigation even when LayerFS returned-byte totals are small. This supports
investigating reuse of already-known sibling demands in one existing acquisition,
not a cross-write handle cache. It is a mechanism hypothesis, not attribution of
all observed workload traffic. [SQLite source](https://github.com/sqlite/sqlite/blob/master/src/btree.c).

Existing dependency discovery already coalesces known groups per pack. Sharing
its acquisition with reconstruction addresses a concrete repeated-open cause
without changing density/reuse promotion; the failed whole-first experiment
stays rejected. Format-sized directory acquisition could reduce the current
min(length,4120)prefix to validated lane/version control width (tight ordinary/
native24+16*16=280B), but needs header validation, exact extent checks, added-call
and page-navigation accounting. No such change is included in this treatment.
Independent physical group rows remain a later schema/publication/storage option.

## Proof and next measurement

Affected storage/persistence all-target checks, warning-denying workspaceClippy,
fmt,boundary449/23selftests PASS. Full frozen Core checks and one labeled
`history17-save-acquisition-cause-v1` count vehicle follow. The count vehicle
constructs a fresh real17state Store from the same sealed prepared corpus with
release/locked binaries,one worker and60s complete command bound; it records
phase counts/cold boundaries under unchanged policy. Verification is SKIPPED,
speed/admission NOT_APPLICABLE. It is not an unchanged performance resample.

Reproduce once at a frozen source with a fresh owned output:

```
python3 core/benchmark/fs-bench-pro/diagnostics/run_save_acquisition_cause.py --out <fresh-owned-output>
```

Accept a reduction in Save VFS requests and repeated acquisitions before making
any new broad performance claim. Preserve all failures and omissions; previous
qualified/failed rows are not relabeled. No deadline, cache or worker inflation.

## Shared-cache count result (source0446bf884)

The one count diagnostic completed in46.832501708s of the declared60s command
budget. Source and database cold boundaries passed. Save/custody requested VFS
bytes fell from1,873,421,324 to1,836,602,313 (36,819,011 fewer,1.9654%); pack
acquisitions fell from27,722 to25,989 (1,733 fewer,6.2514%). Filesystem requests
were28,377,170B/679acquisitions. Store allocation stayed49,594,368B.
The comparison is a labeled mechanism diagnostic against the retained prior
phase counts, not a new matched speed qualification. Verification SKIPPED;
admission NOT_APPLICABLE. [Receipt](checks/save-acquisition-sharing1/count-receipt.json).
Full Core502tests/95targets, Clippy, fmt and boundary449/23selftests PASS.

## Separate format-sized directory treatment

Against0446bf884, scoped acquisition validates the fixed24B header first, then
reads exactly the lane/version's complete reserved directory width through the
same read transaction and BLOB handle. Tight ordinary/native directories are
280B; legacy directories retain their full width. The maximum4120B allowance,
reserved-slot validation, group selection/promotion rules, canonical/dependency
authentication and separate whole-pack audit are unchanged. Malformed control
fields fail before directory/body reads. This adds one BLOB read per acquisition;
its navigation cost must be observed, not assumed beneficial.

The real SQLite fixture first failed because the scoped prefix was4120 rather
than280B. After the change, its selected100B range must pay380B across three
reads with one open/close; corrupt magic/version/count/length/reserved fields
must fail after only the24B control read. New count result remains pending.

Format-directory covering checks: full Core502tests/95targets, workspace all-target
Clippy with warnings denied, fmt and boundary449/12 boundary selftests PASS. Count diagnostic
will run once after freezing this separate treatment.

## Format-directory count result (source8759d992b)

One diagnostic completed45.198178958s/60s, with source/per-state DB cold checks
PASS; verification SKIPPED and admission NOT_APPLICABLE. Same17states/roots,
canonical inventory and49,594,368B Store allocation as the shared-cache treatment.

| Treatment | Save VFS requested B | Save acquisitions | Lifetime successful BLOB B | Lifetime BLOB read calls |
| --- | ---: | ---: | ---: | ---: |
| Shared caches0446 | 1,836,602,313 | 25,989 | 1,187,219,829 | 53,445 |
| Exact directory8759 | 1,798,505,411 | 25,989 | 1,162,204,149 | 80,370 |

Save reduction38,096,902B =100*(1,836,602,313−1,798,505,411)/1,836,602,313
=2.074314%. Lifetime BLOB bytes fell25,015,680B while reads grew26,925.
The lifetime columns cannot be attributed to Save alone. No byte figure is a
device traffic measurement. Filesystem28,142,041B/679acquisitions; construction
3,774,754B/62. Driver32.227982417s, internal operation31.015470959s andCPU
24.288983s are diagnostic observations, not a matched speed/admission result.
No repeated sample or comparison against a warm source.

[Identity, cold checks, counters and immutable raw receipt hash](checks/format-directory1/count-summary.json).
Raw receipt/SQL/VFS/close/log/manifest retained under
`benchmark-results/fs-bench-pro/issue302-format-directory-cause1/`.
Reproduction command is the count command above with this fresh output name.
Release/locked, local incremental build; sealed prepared corpus reused outside
child, fresh17state database construction inside child; one worker. No image.
Ordering scratch checked empty; database retained as diagnostic artifact.
No independent content proof for either new treatment; updated matched
10/3/1 qualification and Durable/Init/all-seven remain NOT_RUN. Historical
7230d62f1 qualification is unchanged and is not promoted onto this source.

Both treatments reduce the counted mechanism modestly. Relative to the original
localized723 Save total,1,798,505,411 is74,915,913B (3.9989%) lower; it remains
about2.21 times the earlier reference812,164,935B, whose layout differs.
Further work should first identify repeated pack/group demands and navigation
by phase/domain from a labeled count diagnostic, then consider bounded existing
cohort acquisition sharing. Larger buffers, whole-first promotion and cross-write
BLOB handle retention are not justified by these results.

Production LOC8759:139619→139635(delta+16), reference65417 unchanged, core
74202→74218; exact parent/staged product snapshots, same versioned counter and
inline-test/nonproduct exclusions. This result-only commit has unchanged139635
(delta0). [Count confirmation](checks/format-directory1/production-loc.json).

Issue updated with implementation/count result: https://github.com/Ephemeral-AI-Lab/layerfs/issues/302#issuecomment-5980190482

## Isolated remaining-amplification investigation (2026-10-04)

Worktree `save-vfs-amplification/layerfs`, starting at4032cfe75. Prior owner
checkouts are read-only references. PostgreSQL/MinIO M4 pause is unchanged.
One prospective labeled count diagnostic extends the existing history17 cause
vehicle with external SQLite BLOB acquisition events: pack ID, offset, requested
length, result and delegated VFS request counter before/after each call. Snapshot
events bind the existing disjoint construction/filesystem/Save phase boundaries.
Final retained DB descriptors allow untimed domain/lane classification; no body
is inspected or prefetched by the observer. At most64 live handle identities are
tracked; streamed JSONL avoids workload-sized retained telemetry memory. The
single construction producer and original60s complete diagnostic budget remain.
Real SQLite observer calibration covers successful/error read/open attribution.
This is a cause instrument, not an unchanged performance sample or admission.
First-use worktree-local release/locked preparation finished in15.82s. The
runner records its incremental build/sealed executable custody independently.

Hypotheses to distinguish before implementation: repeated demand after the
wave-wide discovery pass versus per-handle overflow navigation and whole-choice
returned bytes. Analyze existing receipts first; compare prospective trace counts
with earlier retained counts only as mechanism diagnosis. No physical-device-byte
claim, larger cache, base memo, cross-write handle retention, workers or proof
relaxation. Required product checks and new matched qualification are pending.

The first trace at2b39dfe50 completed all17states, reproducing prior phase counts
exactly: Save1,798,505,411VFS requested B/25,989acquisitions; filesystem
28,142,041B/679. Source/per-state DBcoldPASS. Version18 grouped whole-file
sparse acquisitions8362 returned335,745,568B and requested665,542,917B inside
BLOB read calls;4321 repeated identical range demands within a state. Version18
whole acquisitions10257 returned660,142,543B, VFS639,484,716B;6905 identical
reacquisitions. These counts establish repeated demand and requested-byte
amplification, not device traffic or the reason for each cache miss.
Raw:`benchmark-results/fs-bench-pro/issue302-save-vfs-trace-cause1/`.

A separate prospective caller-cause instrument records at most32 backtrace
addresses at each BLOB open, relative to the producing sealed executable's image
base. Offline symbolization assigns discovery versus selection/reconstruction;
no product hook, alternative operation or data pre-read. Same one fresh17state
count child,60s cold envelope, no speed/admission claim. This differs from the
first cause instrument by observing the callers needed to settle the remaining
mechanism hypothesis; it is not a second performance sample.

## Caller result and one demand-owned implementation

Caller diagnostic26a453961 completed under the fixed60s budget, all source and
DBcold checksPASS and same phase totals as8759. External symbolized stacks
attribute Save's BLOB reads as follows (not the SQL/metadata remainder):

| Caller | Acquisitions | Returned BLOB B | VFS requested B inside BLOB reads |
| --- | ---: | ---: | ---: |
| Eager wave discovery |10439|636139189|778636499|
| Candidate depth/probe |7729|354577818|498578452|
| Pool value index |5763|95832979|82976381|
| Pooled base reconstruction |772|7885315|6428290|
| Ordinary base reconstruction |835|28802936|49190721|
| Exact reuse |451|17299881|25916072|

[Two retained count identities and raw hashes](checks/save-vfs-caller1/caller-summary.json).
The requested-byte observations support a demand-lifetime problem and sparse
BLOB navigation overhead; they do not identify physical storage traffic.

Remove only Save's eager physical prewalk. Batched locator membership, object and
advisory order, required depth checks, actual base/exact-reuse reconstruction and
all canonical/dependency authentication remain. Reader prefetch is unchanged.
No representation/layout/schema/promotion/cache/worker/publication policy change.
Each admission now consumes its required physical chains in the existing owner
caches instead of first scanning a wave whose input can exceed2MiB retention.
Architecture05 describes this algorithm in the same implementation commit.

Public exact-reuse pressure fixture:37packs/256objects beyond2MiB. Before113
acquisitionsFAIL. After74; the initial test expected38 and therefore still
failed. Source analysis confirms the unchanged metadata sparse-first then
whole-on-sibling promotion legitimately needs up to2acquisitions/pack. The
assertion is corrected to74, preserving the before113failure; this is a test
expectation correction, not a product policy or cache-bound change. Both failed
logs are retained. Final frozen Core checks and changed-product count diagnostic
follow; no new speed/qualification is claimed before those results.

Implementation covering checksPASS:503tests/96targets with0ignored,
locked workspace/all-target Clippy-Dwarnings, fmt-all/check,449file boundary,
23tool selftests,2count-parser tests and external realSQLite observer probe.
Exact commands/logs inchecks/save-demand1; original failed/raw logs retained in
benchmark-results. No CI/aggregate preflight. Final prospective changed-source
history17count diagnostic retains60s complete bound, new fresh output, source
and per-state DBcold, same fixture/build seals and no proof/speed admission.
ProductionLOC139635->139620(delta-15); reference65417 unchanged, core
74218->74203, exact parent/final staged snapshots and unchanged product scope
elsewhere, same archive-counted parent counter/exclusions. No migration/retirement.

## Demand-owned count result and residual cause experiment

One b1151f732 count child complete44.362384458s/60s, coldPASS. Save
1,039,486,710VFS requested B/16,462acquisitions:42.202748% fewer requests and
36.657817% fewer acquisitions than8759. Filesystem27,593,177B/679 retains
localized savings. Same613packs/45,558,472sealed pack bytes and49,594,368B Store
allocation. Current count remains27.989607% above original812,164,935B reference;
16,462acquisitions are near reference16,484. This is a cause count, not speed.
[Receipt/attribution](checks/save-demand1/count-summary.json).

Residual version18 sparse Save reads:5065acquisitions,193,426,533returnedB versus
395,380,277requested VFS B inside BLOB reads. The201,953,744B difference is
close to the227,321,775B whole-Save reference gap, but these observations do not
prove every gap byte has that cause. No physical device attribution is claimed.
SQLite's documented cursor-local overflow navigation lazily populates a page
list and traverses preceding links on a fresh high-offset read.

Prospective count-only structural vehicle`sqlite_overflow_navigation.c` observes
one32768B BLOB read at each distinct offset0/32768/131072/196608 in a closed,
prepared262144B SQLite row, plus offset0 in a32768B row. Each case uses a separate
fresh read-only child, original system SQLite,4KiB pages/cache_size-2048/mmap0,
source-file invalidation and residency attestation, unchanged delegated VFS
observer,5s child budget. Setup contains no LayerFS product, expected-result
prefill or timed producer work. Returned zeros are checked inside the count
child. These counts test offset navigation versus independent physical rows;
they support neither a product speed nor storage/admission claim. No production
schema change is authorized or implemented by this mechanism experiment.
