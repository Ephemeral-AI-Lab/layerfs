# #237: bounded Core producer slabs against the v0.1.6 10k reference

> **Status: exploratory result, not release admission.** One preregistered
> Core control/candidate pair used the same 10,000-file, 300,000,000-B seed-1
> source manifest as the [v0.1.6 common-source run](v016-v017-common-source-results.md).
> Each arm imported a fresh, independent writable byte copy into a fresh
> SQLite Store. Final source payload residency was zero in all three arms;
> directory/inode metadata cache state was unqualified. The Core control had
> incomplete daemon telemetry **and** an incorrect preregistered binary hash;
> both facts remain in the [append-only protocol](slab-handoff-prereg.md).
> The actual timed binary is archived and identified in its receipt. No arm
> was rerun. The separate count diagnostic below is not a speed sample.

## What changed and what it achieved

The Core control sent one message per finalized object and one per file
completion through an eight-slot channel. The candidate kept the same four
C1 workers and the same single C2 Save owner, but sent ordered Object/Done
events in four-slot slabs. Ordinary slabs are bounded by 256 KiB canonical
payload, 512 objects and 512 completions. An object above 256 KiB travels in
one singleton slab under the unchanged 16-MiB canonical-object bound. Each
object still receives one `SaveHandoff::accept` call. Pack bytes stay **inside
SQLite BLOBs**, SQLite pages remain **4,096 B**, and the whole-file cutoff
remains **128 KiB**. The isolated candidate's
[source, test and architecture diff](evidence/slab-handoff/candidate-source-and-tests.diff.gz)
(SHA-256 `7feae59c3396883f4f70a112c56d081b083b3422d6defdfc6245e0641ed7e770`)
records the ordering and memory bounds. The root research product still has
the original message handoff; this result does not silently change it.

The candidate made **1,202** handoffs versus the Core control's **34,562**:
**96.52% fewer receives**, essentially the v0.1.6 reference's 1,203 slabs.
Its raw Core public call was **1,166.251 ms**, down **234.373 ms / 16.73%**
from the control's **1,400.623 ms**. The file loop fell **219.647 ms** and
receiver wait fell **252.544 ms**. These are one-shot observations, not a
variance estimate or a fully cold metadata result. The control's preregistered
manual-build Service hash differed from the exact H3 runner binary that ran;
the actual build/receipt pair is retained, but the exact-binary preregistration
failed. The pair is useful for diagnosis, not a formal admitted speedup.

## Side-by-side common-source comparison

All public throughput values use 300 decimal MB divided by that arm's public
wall. v0.1.6 used its real SDK `initialize_layerstack`; Core used its real
daemon native Init through `StackCreated`. Pipeline stage and process-resource
boundaries differ; numbers in the same row are observations, not automatically
substitutable durations.

| Observation | v0.1.6 release | Core message control | Core slab candidate |
| --- | ---: | ---: | ---: |
| Public wall | **750.626 ms** | **1,400.623 ms** | **1,166.251 ms** |
| Public throughput | **399.667 MB/s** | **214.190 MB/s** | **257.235 MB/s** |
| Public row status | `DIAGNOSTIC` | `INCOMPLETE` telemetry; separate binary prereg mismatch | `DIAGNOSTIC` |
| Direct pipeline / Core file loop | **709.704 ms** | **1,194.056 ms** | **974.409 ms** |
| Four-producer wall/construct sum | **2,577.802 ms** | **4,493.635 ms** | **3,882.160 ms** |
| Four-producer blocked-send sum | **806.749 ms** | **2,877.391 ms** | **2,155.469 ms** |
| Handoff/receiver calls | **1,203 slabs** | **34,562 Object/Done** | **1,202 slabs** |
| Receiver wait | **47.914 ms** | **449.902 ms** | **197.358 ms** |
| Receiver C2 `accept` | `NOT_MEASURED` | **740.128 ms**, 24,562 calls | **775.353 ms**, 24,562 calls |
| Source read calls / returned bytes | **27,951 / 300 MB** | **29,952 / 300 MB** | **29,952 / 300 MB** |
| Summed source-read syscall wall | `NOT_MEASURED` | **1,129.977 ms** | **1,044.891 ms** |
| Successful write COMMITs in timed arm | **75** traced | `NOT_MEASURED` | `NOT_MEASURED` |
| COMMIT wall in timed arm | **223.015 ms** for 74 of 75 | `NOT_MEASURED` | `NOT_MEASURED` |
| Object-row INSERT statements in timed arm | **639** traced | `NOT_MEASURED` | `NOT_MEASURED` |
| SQLite object rows / pack rows | **24,683 / 1,693** | **24,683 / 1,264** | **24,683 / 1,262** |
| SQLite pack BLOB capacity | **301,646,854 B** stored length | **331,350,016 B** reserved | **330,825,728 B** reserved |
| Core pack used/slack | `NOT_MEASURED` | **305,977,810 / 25,372,206 B** | **305,969,650 / 24,856,078 B** |
| Total Store + History apparent | **304,553,984 B** one DB | **334,274,560 B** | **333,725,696 B** |
| Total Store + History allocated | **318,783,488 B** | **335,695,872 B** | **335,695,872 B** |
| Process CPU observation | **1,626.476 ms** within old call | **1,923.214 ms** Service+daemon lifecycle | **1,787.510 ms** Service+daemon lifecycle |
| RSS observation | **78,757,888 B** old process t1 | **60,473,344 B** sampled Service max | **63,340,544 B** sampled Service max |
| Separate full reopened 10k/300-MB readback | **PASS** | **PASS** | **PASS** |

The old 223.015-ms COMMIT timer covers 74 diagnostic admission/publication
transactions; its 75th reservation COMMIT was traced but not individually
timed. The Core arms did not emit `SaveOutcome`, so their exact timed-arm
COMMIT/SQL counts cannot be reconstructed from the closed databases. Their
Store geometry is exact, but a pack-row count is not a COMMIT count. The CPU
and RSS rows have different scopes: old RSS is an exact new lifetime high
water, while Core Service peaks were sampled without full boundary coverage.
Producer sums overlap four workers; Core `construct_sum_ns` excludes some Done
sends while blocked-send sum includes them, so subtracting those two counters
would invent a producer-active time. The old `recv()` idle timer and Core
`recv_timeout()` boundaries also differ.

The [control raw receipt](evidence/slab-handoff/control/receipt.json) and
[candidate raw receipt](evidence/slab-handoff/candidate/receipt.json) retain
the operation, status, diagnostic line and resources. [Cold sidecars](evidence/slab-handoff/control/cold-launch.json)
and [candidate cold sidecars](evidence/slab-handoff/candidate/cold-launch.json)
show **0/27,503** resident source payload pages at final recheck, with
6.258/1.868-ms recheck-to-timer gaps. Source copies were independent; neither
timed operation read the prior arm's source files or Store. Metadata cache
residency remains unqualified, so neither arm is fully cold admission evidence.

The two Core performance receipts returned the identical root
`e7850f75a65309568e3454f7ab962aa16952d0c02218a55f42f3e2fd44ddf673`.
The closed Stores each contain 24,683 objects; an ordered hash of
`object_id`, role and canonical length is equal
(`7916ff14e95ba1c78e8d12b485d02c4843a84c5cec3aa7266f88477aebf15c5b`)
with the [query/encoding receipt](evidence/slab-handoff/object-id-identity.json).
The separate [control readback](evidence/slab-handoff/readback-control.json)
and [candidate readback](evidence/slab-handoff/readback-candidate.json) each
reopened its Store and History catalog, traversed 10,101 paths, and checked
all 10,000 files / 300,000,000 B. Both passed outside the public timers.
Neither verifier wall is part of the speed table. The Store and History page
sizes are 4,096 B, and `store_policy.small_file_threshold_bytes=131072` in
both. [Store geometry control](evidence/slab-handoff/geometry-control.json)
and [candidate](evidence/slab-handoff/geometry-candidate.json) include DB
hashes, pack headers, capacity and used bytes.

## Separate C2 transaction and SQL count diagnostic

One [prospectively labelled count diagnostic](slab-handoff-prereg.md#prospective-count-only-follow-up-after-both-public-arms)
used the slab candidate algorithm plus three temporary once-per-Save logs.
It imported another fresh, independent copy with 0/27,503 resident payload
pages and the same root. Its daemon telemetry was `INCOMPLETE`; the public
time is deliberately **not** a third speed point. The exact
[reporting diff](evidence/slab-handoff/count-diagnostic.diff.gz) is archived
and was removed from final product source. Its
[raw receipt](evidence/slab-handoff/countdiag/receipt.json) and
[Service log](evidence/slab-handoff/countdiag/service.stderr) preserve all
three successful `SaveOutcome`s:

| Diagnostic Save | Inserted/reused | COMMITs | COMMIT wall | SQL bucket | Object INSERTs | Presence queries | Pack creates/appends |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| File | 24,364 / 198 | **90** | **259.975 ms** | **140.119 ms** | **1,394** | **74** | **1,256 / 1,126** |
| Prerequisite | 11 / 3 | **4** | **1.136 ms** | **0.700 ms** | **2** | **1** | **2 / 0** |
| Tree | 308 / 0 | **21** | **7.594 ms** | **11.961 ms** | **4** | **1** | **3 / 200** |
| **C2 total** | **24,683 / 201** | **115** | **268.705 ms** | **152.780 ms** | **1,400** | **76** | **1,261 / 1,326** |

C5 additionally makes two source-derived catalog write transactions; their
COMMIT-only wall was not measured, so the diagnostic's total successful write
count is **117** across C2+C5. This count differs from the earlier
common-source Core identity's 107, and the diagnostic made 1,203 rather than
the timed candidate's 1,202 slab sends. Those differences show that admission
schedule and Store layout changed between runs. Do not attach the diagnostic's
115 C2 commits or 268.705-ms timer to the timed candidate row. For context,
the v0.1.6 public arm traced 75 write COMMITs and 639 multi-row object INSERT
statements; its SQL schema, timer scope and public surface differ. The slab
change addressed handoff event count, **not** transaction count, and it did not
establish the owner's 90% DB-transaction reduction objective.

## Remaining 10k gap and next direction

The slab candidate remains **415.625 ms** slower than the same-source
v0.1.6 release run (1,166.251 vs 750.626 ms). Its Core file loop alone is
**264.705 ms** longer than the old 709.704-ms pipeline; those children are
the closest broad spans but are not isomorphic. The Core candidate's file
loop is almost completely accounted for by its **775.353-ms serial C2
`SaveHandoff::accept` wall** and **197.358-ms receiver wait** (1.698 ms
unattributed within that loop). The receive count now matches the old path,
so more channel batching alone has little remaining event-count headroom.
The old 47.914-ms receiver idle timer has a different terminal boundary and
must not be subtracted from Core's 197.358 ms as a promised saving.

The [common-source SQLite comparison](v016-core-sqlite-head2head.md) found
indexed object/pack seeks in both schemas, but old v0.1.6 published 24,683
locators with 639 batched INSERT statements whereas this separate Core
diagnostic used 1,400, and its file Save alone made 90 COMMITs. These counts
point to serial admission/SQLite work as the next measured mechanism, even
though their distinct identities preclude a runtime ratio. Another concrete
source difference is candidate search for small whole-file objects: v0.1.6
computes `small_signature` on its producer threads in
`crates/layerfs-layerstack-store/src/objects.rs`, while Core
`cas/selection.rs::pending_base_for` computes `candidates::signature(raw)`
on the single C2 owner when offered a WholeFile object. The shared fixture
has 9,399 small files. That serial signature work is a **source-backed
hypothesis**, not a measured timing attribution. Subsequent owner direction
stopped producer-side signature offload; it is not a selected treatment.

The other **150.920 ms** of raw old-to-candidate public gap sits outside
these differently defined pipeline/file-loop spans. Core's own top-level
Service children include a 46.741-ms scan and 29.404-ms tree-save finish;
the old prepare-import child includes its own 18.961-ms final root/inode
table. The Core Service lifecycle and daemon public call have distinct clock
boundaries, and the control lost a daemon event. Do not assign the remaining
gap to transport or SQLite without a matching timer. The historical
**578.245-ms / 518.8-MB/s** v0.1.6 observation had zero device-read bytes
and no cold-source contract; its value is archival rather than a valid cold
target. The fresh same-source v0.1.6 observation is **750.626 ms / 399.667
MB/s** under zero resident source payload pages, with metadata still
unqualified.

The final product source restored after the count diagnostic is byte-equal
to slab candidate commit `329325587` for the touched Service files. Focused
2,050-file native import with a 300-KB cross-boundary read and symlink
failure passed, as did the product boundary guard and formatting check.
The full Core workspace tests and six Core tool tests also passed. Full
workspace Clippy with `-D warnings` **failed** at
`operation/import_native.rs:200` on `clippy::collapsible_if`: the slab flush
condition contains a nested `if`. This style warning does not block a run;
the measured product source was left byte-identical rather than changing its
build identity after the single performance sample. The pair's two full
reopened readbacks passed. This is enough to keep the
slab candidate as an exploratory improvement while the serial C2 work is
investigated; it does not make the pair release-admitted.

## Final source checks

These checks ran once after the temporary count logging was removed, with
the measured slab candidate's Service source restored exactly:

| Command | Result |
| --- | --- |
| `python3 core/tools/check_product_boundary.py` | **PASS**, 261 production Rust/SQL files scanned. |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | **PASS**, 6 tests. |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | **PASS**. |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --workspace` | **PASS**, all workspace tests and doctests. |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace --all-targets -- -D warnings` | **FAIL**, one `clippy::collapsible_if` warning in `import_native.rs:200`; no product fix applied after the timed sample. |

Before the timed candidate, the focused command
`cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-service --test history native_directory_import_exceeds_bootstrap_and_reads_bytes`
also passed. It exercised 2,050 tiny files (forcing a worker beyond 512
completions), a 300-KB file read across a 256-KB offset, and a symlink
failure. The Clippy warning is a known verification miss on this research
identity; it is not reported as a PASS or silently waived.
