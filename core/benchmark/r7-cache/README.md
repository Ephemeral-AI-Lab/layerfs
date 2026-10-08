# R7 per-file cache observer

Status: exploratory harness implementation; no cache class is inferred from a
hint. Linux qualification requires the retained real primitive test and each
sample's own pre-attempt receipt.

`python3 residency.py --class A --store /tmp/store.sqlite` performs exactly one
`posix_fadvise(DONTNEED)` per present file, then `mincore` without touching the
mapping. Store sidecars (`-wal`, `-shm`, `-journal`) are included and their absence
is explicit. Add `--overlay` and repeated `--file` to declare other content
residency. A 16 MiB mapping window and at most 4 KiB mincore vector on 4 KiB
pages bound observer memory independently of file size. No payload is read, no
fsync is performed, and no VM cache control is used.

Exit 3 means `INELIGIBLE`, with zero product attempts, because observed class A
pages remain resident. Exit 4 means `UNAVAILABLE`; the platform, primitive or
identity failure remains explicit. Failure is never reclassified as cold.

Class B observes residency without eviction. It is `PENDING_PHASE_COUNTERS`
until the actual measured phase's object-demand counter is supplied to
`class_status` (or `--object-demands` for an already retained phase). Zero
demands qualify that requirement. Class B remains exclusive to L. A declared
identical untimed call in an earlier mount of the same daemon and terminal
unmount are separate requirements the runner must prove.

Class C observes residency without eviction. Its warm-up completion and measured
start use the same monotonic clock. `--warmup-ended-monotonic-ns` provides a
preflight interval; the runner must separately preserve the exact interval at
measured start. A nonnegative interval strictly below 60 seconds is required.
The same mount and identical warm-up command are runner obligations.

These observations cover regular-file content pages. Kernel metadata caches,
reader/SQLite caches, daemon immutable cache, and natural own-write warmth must
be declared separately. The receipt's `ELIGIBLE` only concerns the tested cache
predicate; all R7 timing is exploratory and never admission-eligible.

The host is macOS; this Linux helper refuses that platform. The earlier macOS
helper is independently scoped and is not silently substituted for Linux.
External contract tests are in `test_residency.py`; run them under the checkout
lock and a wall bound at most 120 seconds.
