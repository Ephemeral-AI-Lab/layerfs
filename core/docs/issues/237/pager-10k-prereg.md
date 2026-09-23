# #237: live 10k Save pager diagnostic preregistration

> **Status:** Research; informative and not a product contract.

## D13: one count diagnostic before any cache-policy treatment

Run one release-profile, perf-only `namespace-10000` native Init through the
existing public daemon-host route, at source `acb17d287` plus temporary
reporting-only instrumentation. Keep the 10,000-file, 300,000,000-B source,
four constructors, eight-object handoff, 512-object/4-MiB C2 wave, transaction
cadence, Store format, and **4,096-B SQLite pages** unchanged. Reuse the sealed
prepared source only outside the operation timer. Before the one public call,
rehash and invalidate every payload, then immediately check whole-input
residency without faulting pages in. If either check finds resident pages,
record `INELIGIBLE` and do not time the operation. Use a fresh output and fresh
Store. Full readback verification remains `SKIPPED` in this exploratory run.
Use the fixed stack/scope seed derived from the sealed fixture and case by
`cold_diagnostic.py --fixed-operation-identity`; transport/session keys remain
fresh. This fixes identity for a possible later policy pair without changing
the timed public route.

The temporary C2 instrument reads `PRAGMA page_size/cache_size/cache_spill`
and SQLite `sqlite3_db_status` on **each Save's actual owner connection**.
Snapshot `CACHE_USED`, `CACHE_HIT`, `CACHE_MISS`, `CACHE_WRITE`, and
`CACHE_SPILL` after acquire, after each completed preparation wave, and after
final publication before connection release. Record total deltas, the largest
observed single-wave deltas, and the largest **boundary-sampled** cache-used
bytes. `CACHE_USED` has no SQLite high-water counter; boundary sampling is not
an exact within-wave peak. Read counters without reset. Print one bounded line
per Save, identify the file Save by its 24k inserted objects, and retain the
instrumentation diff and raw stderr/receipt. This temporarily calls SQLite FFI
because pinned `rusqlite` has no safe `db_status` wrapper; restore all source
edits after the diagnostic. No unsafe diagnostic hook remains in product source.
No diagnostic elapsed time is a treatment speed comparison.

Decision rule: if file Save reports zero spills and low miss/write reread
pressure, reject a larger-cache/spill-OFF pair for now. If it reports positive
spills or substantial cache misses/rewrite pressure within the 4-MiB waves,
preregister one **distinct** default-cache versus 32-MiB/spill-OFF product-policy
pair with equal cold-source preparation, release build, worker count, fixture,
timer, fresh Stores and bounded-memory reporting. One sample per arm; do not
promote D13's instrumented time as an arm. The cache change would be a product
policy, with memory and Store-geometry consequences, not setup warming.

The preceding [EXPLAIN audit](sqlite-explain.md) found primary-key seeks and no
missing hot index. D12 separately measured 227.521 ms in transaction cadence
and 188.689 ms in SQL, but neither counter proves spills. An earlier Stage 0
probe found zero spills on its synthetic 1x/4x transaction shapes; this native
10k workload needs its own actual-connection reading.
