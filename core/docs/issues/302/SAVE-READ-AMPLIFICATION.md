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
