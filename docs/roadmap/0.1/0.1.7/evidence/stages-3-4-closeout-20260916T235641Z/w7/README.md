# W7 evidence — memory instrumentation and the allocation ledger (G9, G11, G12)

## W7.1 — the external counting allocator

`core/crates/layerfs-storage/examples/memory_ledger.rs` installs a counting
`#[global_allocator]` in an **external target** (an example, never `src/`) and
brackets each real product phase with it. Every phase reports:

* `current` — bytes still held when the phase ended;
* `peak` — the largest residency *inside* the phase, measured against the
  residency the phase started from;
* `allocs` — allocation calls inside the phase;
* `charged` — bytes those calls requested in total.

The counts are requested sizes: allocator metadata, alignment padding and any
`mmap` the allocator serves outside `alloc` are not included, so they are lower
bounds on the true heap. That is a product hook nowhere: the allocator lives in
the example, the product source is untouched.

## W7.2 — the process scope, labelled

RSS is sampled separately, with `ps -o rss= -p <pid>`, **at phase boundaries
only**, and the window and coverage are stated in the receipt: a sample is a
whole-process figure, so it is a lower bound on a phase peak and never a
phase-local heap number. A lifetime high-water is reported separately as a
lifetime number: `/usr/bin/time -l core/target/debug/examples/memory_ledger`
reports `16777216` bytes maximum resident set size for the example process
(0.32 s wall). `getrusage`'s `ru_maxrss` is **not** read in-process, and the
receipt says `null` with the reason: it needs a platform interface this batch does
not add a dependency for. No lifetime counter is used as a phase number anywhere.

## W7.3 — the ledger

Measured live in the run: `pool_index_entries` 2 400, `pool_index_bytes` 57 600,
`candidate_index_bytes` 122 880 (reported by the operation that held it, not by a
test hook).

| Owner | Bound | Live multiplicity | Capacity/transient overlap | Lifetime | Release event |
| --- | --- | --- | --- | --- | --- |
| COW edit frontier `EditObjects::drafts` | `EDIT_DEFERRED_LIMIT` = 8 MiB − 1 charged from the representation each draft holds | one per edit operation | overlaps the base reads, the replacement scan and the commit walk | the operation | each superseded draft and the dropped range release their charge; the rest dies with the operation |
| Immutable bases (stored subtrees) | one page at a time, ≤ 8 192 B canonical each | one read per descent step | none: a stored page is read, used and dropped | one walk step | with the local that holds it |
| FULL and PREFIX alternatives | one canonical record each: ≤ 32 781 B chunk, ≤ cutoff+22 whole-file | one pair per object in flight | **simultaneous by design**: the cost comparison needs both | one selection | after the comparison, the loser is dropped |
| Five pack lanes (`Ordinary`, `Native`, `WholeFile`, `PooledMetadata`, `Singleton`) | one open group per lane (≤ 64 KiB ordinary, ≤ 16 KiB pooled, ≤ 16 MiB singleton) | up to five open groups, one per lane | one assembled pack while a lane writes (≤ 256 KiB, or ≤ 16 MiB + 4 096 singleton) | the save | the group is placed and freed; the open tail lives until the save ends |
| SQLite MEMORY journal | modified pages of one transaction, bounded by 8 191 rows / 4 MiB − 1 canonical plus one group and one pack of overshoot | one journal per open transaction | overlaps the writer's own buffers and BLOB copies | one transaction | `COMMIT`/`ROLLBACK`; the cleanup path now commits and reopens at the same bounds |
| SQLite BLOB copies | one copy per pack write and per row insert, ≤ the pack or record width | one per write | overlaps the journal for the same transaction | one write | with the statement that used it |
| Ordered-set nodes (`PoolIndex`) | `METADATA_INDEX_VALUES` = 131 072 entries × (key + ordinal) | one per Store, shared by every save | overlaps the decoded-value cache and the pack cache | the Store | whole-window reset, or `invalidate()` on a failed save |
| Pooled decoded-value cache | `VALUE_CACHE_BYTES` = 512 KiB | one per reader | overlaps the pack cache | the wave/chain | wholesale clear at the bound, or with the reader |
| Pooled pack cache | `DEPENDENCY_PACK_CACHE_BYTES` = 4 MiB | one per wave | overlaps the decoded-value cache | the wave | wholesale clear at the bound, or with the reader |
| Dependency pack cache (writer) | same 4 MiB | one per save | overlaps the codec workspaces | the save | wholesale clear at the bound, or with the operation |
| Admitted-FULL candidate index | `INDEX_BYTES` = 128 KiB fixed | one per save | overlaps the depth cache | the save | with the operation |
| Dependency depth cache | 4 096 entries | one per save | overlaps the candidate index | the save | cleared wholesale when full, or with the operation |
| Codec workspaces | `ENCODE_WORKSPACE_BYTES` = 2 MiB + `DECODE_WORKSPACE_BYTES` = 1 MiB | one pair per save or read | overlaps every encode/decode in the operation | the operation | with the operation; the pooled lane holds one reader and reuses it |
| Mapping builder levels | `(height + 2) × stream_flush_entries` entries | one builder per construction or scan | overlaps the frontier in the edit path | the scan | at `finish`, or per flush |
| Telemetry report | 1 024 nodes / 32 levels | one per timing scope | overlaps the operation it measures | the scope | with the report; disabled scopes allocate nothing |

**No owner is unowned.** The two caches the review named as unbounded are bounded
above; the ones that were already bounded keep their bound and now state it.

## W7.4 — SQLite honesty

The receipt states what the profile sets and what it does not:
`journal_mode = MEMORY`, `synchronous = OFF`, `temp_store = MEMORY`,
`foreign_keys = ON`, `busy_timeout = 0`; **no** `cache_size` and **no**
`mmap_size` pragma is ever set, so the engine's page cache, the MEMORY journal and
`temp_store` b-trees are charged to no budget of this product and the engine
maxima are the host library's (the build links the system `libsqlite3`).
`cache_size` is not a total RSS cap and is not presented as one.
`content-io-memory-audit.md` now carries the candidate profile beside the
reference rows it audits, so the reference's "requested 32-MiB cache/connection"
is not read as the candidate's setting. Connection multiplicity is one per open
operation: one writer behind a single `BEGIN IMMEDIATE` with a zero busy timeout,
readers unbounded. That connection count is **unqualified, not bounded**: no
budget covers it, and the receipt says so rather than implying one.

## The measured ledger (G12's input and result)

`w7-verify.log`, run `--pooled-leaves 24 --pooled-rows 100` (2 400 distinct
values, 24 leaves — the E1b shape):

| Phase | current (B) | peak (B) | allocs | charged (B) | rss before → after (B) |
| --- | ---: | ---: | ---: | ---: | --- |
| `c1.construct.chunked-1mib` | 2 285 238 | 1 097 482 | 89 | 1 113 971 | 3 538 944 → 5 226 496 |
| `c2.save.chunked-1mib` | 1 231 563 | 3 854 983 | 936 | 10 206 559 | 5 259 264 → 13 336 576 |
| `c2.save.pooled.cold` (1 leaf) | 1 234 285 | 3 352 437 | 464 | 3 472 168 | 13 385 728 → 13 647 872 |
| `c2.save.pooled.warm` (23 leaves) | 1 297 933 | 3 450 007 | 9 977 | 81 482 710 | 13 647 872 → 16 384 000 |
| `c2.read.pooled` | 1 306 101 | 1 101 257 | 155 | 1 131 103 | 16 400 384 → 16 400 384 |
| `c2.abandon.cleanup` | 1 306 421 | 3 293 723 | 30 | 3 295 001 | 16 400 384 → 16 400 384 |

Interpretation, kept to what the numbers say:

* #168's simultaneous index/codec/SQL memory question has an input and a result:
  with the ordered set live (57 600 B), the codec workspaces (3 MiB) and SQLite
  work (journal + BLOB copies) overlapping, the pooled save's phase-local heap
  peak is **3.45 MiB** and the whole phase charges 81.5 MiB across 9 977
  allocations — i.e. the *rate* of allocation, not the residency, is what the
  pooled lane spends on 2 400 values.
* The whole example process never exceeds **16 777 216 B** RSS (lifetime,
  `/usr/bin/time -l`), of which the debug binary image is a large fixed part.
* The chunked save's peak (3.85 MiB) is the pack assembly plus the transaction's
  rows and BLOBs, inside the declared pack and transaction bounds.

### Nulls and coverage

| Number | Value | Reason |
| --- | --- | --- |
| in-process `ru_maxrss` | `null` | needs a platform interface (libc) this batch does not add as a dependency; the label is therefore sourced from `/usr/bin/time -l` instead |
| allocator metadata and alignment | not counted | the counters record requested sizes |
| `mmap`-served allocations | not counted | the counting allocator sees only `alloc`/`realloc` |
| engine page cache, MEMORY journal, `temp_store` maxima | unqualified | host `libsqlite3` defaults; no pragma sets them |
| connection count | unqualified | one per open operation; no budget covers it |
| pooled-lane phase | one sample | this is a wiring/instrumentation receipt, not a campaign; W8 owns repeated samples |

## Commands and exits

* `w7-verify.log`: the example run through cargo (exit 0) and the same binary run
  directly under `/usr/bin/time -l` (exit 0), with the binary's sha256 recorded.
* `w7-verify.log` continues with the focused suites, the workspace suite, clippy,
  fmt, the product-boundary check, both tool suites, `git diff --check` and the
  production LOC pair.

## What this artifact does not prove

* It is **not** a performance or memory qualification: one sample, debug profile,
  an in-process fixture and no comparison arm. W8 owns the matched campaign.
* The phase numbers are *requested* bytes, not RSS deltas; the RSS column is a
  sampled whole-process figure and is labelled as such in the receipt itself.
* Release events are the code's, verified by reading the paths; the ledger does
  not measure a deallocation directly.
