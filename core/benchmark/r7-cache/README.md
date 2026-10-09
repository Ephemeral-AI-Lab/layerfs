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
The nonempty Linux primitive test prepares and reads one page independently,
then requires the observer to issue one mmap/mincore/munmap and report that page
resident while its own payload-read count stays zero. It performs no eviction
or synchronization call and makes no claim that dirty pages can be evicted.
Each mincore vector is explicitly released before the next window allocation,
so the reported array-byte peak does not hide two overlapping vectors. This is
the vector field only, not whole-Python process residency.

Full native fixtures use the manifest transport instead of one command-line
argument per file. In untimed setup on the **actual native copy**, run:

```sh
python3 generate_manifest.py --root /native --output /code/FRESH-native.files.jsonl
```

The generator reads metadata only, retains and sorts the regular-file identity
rows, and seals their JSONL bytes. It reports the retained row count, setup wall
and its own process lifetime high-water mark with platform-specific units.
Those resources belong to manifest setup, never the daemon or a measured phase.
No symlink is followed or regular payload read. The manifest header records
schema `r7-file-manifest-v1`, the exact actual root device/inode, root path,
`device-inode-path` ordering and regular/physical file cardinalities. Each row
contains a relative path and exact device, inode, logical/allocated bytes,
mtime and ctime pins. Absolute paths under the declared root are also accepted
by the consumer. The manifest must be regenerated as declared setup for a
different actual byte copy; native inode pins cannot be borrowed from the host
or a previous clone.

Use its sealed SHA-256 for the pre-attempt observer:

```sh
python3 residency.py --class A \
  --file-manifest /code/FRESH-native.files.jsonl \
  --file-manifest-sha256 SEALED_SHA256 --root /native \
  --inventory-output /tmp/FRESH-native.residency.jsonl
```

The observer verifies the manifest hash before any per-file observation, then
streams one row at a time. Its input/JSON record window is 64 KiB; file mappings
remain 16 MiB and the live mincore array remains at most 4096 bytes on 4 KiB
pages. It retains only the previous ordering key and physical observation, with
no file-population-sized seen-set, path list or observation list. Opens are
anchored to one retained root descriptor, use `O_NOFOLLOW` on every component,
and refuse parent traversal or root escapes. File identity is compared to its
manifest pin before eviction, then checked again after mincore. Final root and
manifest identity checks protect the retained scope.

The small stdout receipt uses `r7-residency-stream-v1`, has `files: null`, and
references the raw per-file JSONL by path, SHA-256, row count and completeness.
It retains aggregate resident/total pages, physical-file count, declared aliases,
eviction attempts, manifest seal/cardinalities and zero payload reads. Every
ordinary row is marked `physical_file` and included in those aggregates. A
nonzero aggregate remains `INELIGIBLE` with zero product attempts. A failed
manifest/identity/order/primitive attempt remains `UNAVAILABLE`, with an
incomplete raw-inventory reference when any evidence was created; it is never
relabelled cold. Output must be a fresh file outside the measured root.

Duplicate physical inodes are rejected by default, matching the scalar helper.
`--allow-declared-aliases` permits an immediately adjacent explicit `alias_of`
whose descriptor identity matches the preceding physical observation. Its raw
row preserves the alias relation and copied residency observation, with
`included_in_aggregate: false` and no second eviction/mincore. No alias path is
silently omitted. The initial full fixture has no aliases at its recorded copy
scope; the flag is unnecessary there.

The scalar `--store`, `--overlay` and `--file` routes preserve their existing
JSON receipt, including every present/absent SQLite sidecar. They are separate
from manifest mode. Streamed tests cover two-file generator/consumer completeness,
raw hash, actual identity tampering before eviction, seal tampering before any
file attempt, rooted symlink refusal and explicit alias accounting. The existing
real Linux nonempty hot-page test still qualifies the actual mincore call.

The receipt validator requires the main predicate to be explicitly ELIGIBLE
with zero observed pages, or INELIGIBLE with positive pages and zero product
attempts. Unknown, failed and pending main predicates cannot qualify an
attempt. Both global Store and overlay database paths must be declared and
present for L; absent optional sidecars remain valid, and absent paths cannot
carry resident pages. Native streamed receipts retain their cardinality and
per-physical-file hint requirements.

A's pre-Mount backing predicate does not prove a cold command after Mount.
The fresh CanonicalCache and StorageFetch start empty, but startup SQL reads
profile/schema/history metadata and prepares statements. Mount reads canonical
root, inode and directory metadata retained in the immutable and pooled-reader
metadata caches. SQLite pager/prepared state and kernel metadata residency are
not fully observed. L:A command qualification therefore stays INCOMPLETE even
when all tested backing pages are absent. The runner keeps the original raw
phase observations and the validator refuses performance PASS for this scope;
no eviction, restart, larger budget or cache-profile substitution is added.
