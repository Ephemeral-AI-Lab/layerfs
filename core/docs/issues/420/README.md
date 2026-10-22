# #420: PostgreSQL for the global Store — experiments and recommendation

> **Status:** Research; informative and not a product contract.

Exploratory experiments for [issue #420](https://github.com/Ephemeral-AI-Lab/layerfs/issues/420),
run 2026-10-22 on branch `issue420-postgresql` (base `b5814bf3a8`). No product
code changed. Every figure is one sample per case; "projected" and "derived"
figures are labelled. Experiment code is under [`core/experiment/`](../../../experiment/);
raw receipts stayed local under `core/target/issue420/` (not committed, per the
documentation policy).

## Fixtures and reference

- **Base a:** a small Store, the size a C12 run creates.
- **Base b:** the #352 range-60 replay Store, 1,830.8 MB in SQLite
  (sha256 `98bfb7b44a35bda5f9f3a273a9ce308f73977d387de64e9b34651fd474a7011f`).
- **C12** (`git init`, 100 files, `git add`, `git commit`) ported to the JS SDK:
  [`c12_cycle_real.mjs`](../../../../sdk/js/test/verify/c12_cycle_real.mjs) and
  [`c12_capture_real.mjs`](../../../../sdk/js/test/verify/c12_capture_real.mjs).
  SQLite cycle sample: **299.8 ms** (exec + unmount 238.9 ms).
- Postgres 17 (`postgres:17-bookworm`) in a local Docker container. Network delay
  is projected by adding round trips × delay, not injected.

## 1. Speed: Postgres arms against the C12 sample

The C12 Store work (mount and Commit) was replayed per arm; the rest of the cycle
is reused from the SQLite sample ([`issue420-pg`](../../../experiment/issue420-pg/)).

| Arm | Remote round trips (mount / Commit) | Projected C12 cycle, base b: loopback / 0.5 / 1 / 2 ms |
| --- | --- | --- |
| SQLite (today) | 0 / 0 | 299.8 |
| Postgres, no host cache | 44 / 873 | 373 / 811 / 1,270 / 2,187 |
| Postgres + host SQLite cache, one round trip per call | 1 / 1 | 311.8 / 312.8 / 313.8 / 315.8 |

Without a host cache Postgres is unusable for this workload (every statement is
a round trip). With a host cache and one round trip per Commit it costs about
4–5% per cycle.

## 2. Contention: 4 writers on base b

The Postgres arms measure the lane plus row push only (no local Save). Correctness
checks for every case: linear head chain, acknowledged set equals the chain,
0 errors, 0 Busy, 0 retries, 0 duplicates.

| Arm | Publishes/s, 4 writers | Result |
| --- | --- | --- |
| Shared SQLite | 132.7 | PASS |
| Postgres, rows pushed inside the lock | 123.2 | PASS |
| Postgres, rows pushed before the lock | 294.6 | PASS |

## 3. Storage: Postgres layouts on base b

A straight port of the SQLite tables is 2,572.4 MB (1.41× SQLite): no clustered
tables, per-row headers and separate primary-key indexes. A size study grouped
small rows into compressed buckets ([`issue420-pg-size`](../../../experiment/issue420-pg-size/)).

| Layout | Size | × SQLite |
| --- | --- | --- |
| Straight port | 2,572.4 MB | 1.41 |
| Recommended buckets ("pr": grouped compressed inode values and binding hints, hash constraint on object locations) | 1,735.0 MB | 0.95 |
| Smallest buckets ("pb") | 1,646.5 MB | 0.90 |

### Recommended layout re-run ([`issue420-pg-pr`](../../../experiment/issue420-pg-pr/))

| Measure | Straight port | Recommended layout |
| --- | --- | --- |
| C12 cycle, base a, 1 / 2 ms | 314.4 / 316.4 | 309.2 / 311.2 |
| C12 cycle, base b, 1 / 2 ms | 313.8 / 315.8 | 338.7 / 340.7 cold; about 312 warm |
| 4 writers, rows before the lock | 294.6/s | 314.9/s (389.8/s warm, diagnostic) |
| 4 writers, rows inside the lock | 123.2/s | 179.8/s |
| Growth over 200 Commits | — | +21.8 MB (about 109 KB per Commit) |

The cold base-b miss came from a 2 GB memory cap on the test container (index
blocks read from outside shared buffers); with indexes preloaded it matched the
straight port. The full 4-writer arm with local Saves did not run (VM memory).

## 4. One bundle per Commit ([`issue420-bundle`](../../../experiment/issue420-bundle/))

Postgres holds packs (in page-sized slices, `pack_fit`), Commit rows and one
compressed columnar bundle per Commit; hosts apply bundles to their SQLite cache.
A full replay of all 72,426 bundles reproduced every content table exactly.

| Variant | Size | × SQLite | C12 cycle @2 ms | 4 writers |
| --- | --- | --- | --- | --- |
| Self-contained bundles (`col`, zstd 3) | 1,465.3 MB | 0.80 | 310.1 | 493/s with local Save (base-a caches); 898/s remote only |
| Globally numbered bundles (`colref`, zstd 3) | 1,355.4 MB | 0.74 | 313.3 | — |

Costs: Postgres can no longer answer object lookups; every host must hold a
complete cache before it mounts (full replay 35.8 s; snapshot copy 3 s); merge
inside the lane needs a redesign (unmeasured); bundles are kept forever;
`colref` adds 139 MB to every host cache.

## 5. Store-format study, both engines ([`issue420-format`](../../../experiment/issue420-format/))

Changes to the logical format, applicable to SQLite and Postgres alike. Savings
are simulated or derived on base b.

| Change | SQLite saving |
| --- | --- |
| `inode_value` grouped per Save run, column-wise, zstd | −208.4 MB |
| `binding_hint` child runs plus interned names | −80.6 MB |
| `object_location` keyed by 16-byte prefix (8-byte: −41.4 MB, owner call) | −24.8 MB |
| Slimmer commit rows | −20.2 MB |
| Payload zstd 9, metadata dictionary, inode-branch rebasing (new data) | −52.8 MB |

Conservative total: SQLite about 1,444 MB (−21%); Postgres with the recommended
layout about 1,518 MB. Projected C12 cost +1–2 ms.

## 6. Pack contents ([`issue420-payload`](../../../experiment/issue420-payload/))

Git content is 65% of stored payload. Git compresses objects with zlib, so the
Store keeps them verbatim; inflated, almost every blob equals a file the Store
already holds, and 100% of loose objects re-deflate byte-exactly at zlib level 1.

| Lever | Newest 10% of records | Whole Store if packs were rewritten |
| --- | --- | --- |
| Git loose objects stored as deltas against the matching file | 34.4 of 83.8 MB (41%) | 240 MB |
| Rewriting Git pack-file entries | 12.4 MB | 106 MB (rejected: about 0.8 ms per chunk read) |
| Chunk resemblance deltas | 3.0 MB | 184 MB (old `.git/index` history) |

The loose-object lever needs a pinned classic zlib dependency and a new record
kind, costs about 163 µs per object on Save and about 85 µs per object on read.

## Recommendation

1. **Postgres as the shared global Store for multi-host cloud use, with a host
   SQLite cache and one round trip per Commit.** Keep the SQLite Store for
   single-machine use. No object storage on the Commit path; add background
   tiering of old packs only if size or cost demands it.
2. **Layout: the recommended bucket layout plus `pack_fit`**, which keeps
   Postgres directly queryable so any host can mount immediately. One bundle per
   Commit is smaller but requires complete host caches, which conflicts with
   fresh per-tool-call sandboxes; revisit it only for long-lived hosts.
3. **Adopt the conservative format changes in the shared logical format**, so
   both engines shrink. Combined with item 2: about 1,485 MB (0.81× SQLite),
   derived from separate studies, not measured together.
4. **Dual engine, same content:** object ids, roots, commits and inode serials
   must be identical across engines; physical records (pack ids, inode-value
   ordinals, grouping) may differ. The inode-leaf canonical bytes already hold
   the full value; the ordinal exists only in the pooled physical body
   (`layerfs-content/src/object/inode_leaf.rs`). SQLite tables do not need to
   change for Postgres support.

### Owner decisions needed

- Allow Postgres as a second Store engine (changes the one-SQLite-file rule).
- Whether Postgres projects drop the Linux-only rule for Store calls.
- `synchronous_commit=off` as Postgres's Disposable equivalent.
- The format changes (a migration of about 600 MB of rows on base b).
- The Git loose-object lever (new dependency; slower Git reads).
- Declined unless asked: 8-byte object keys (silent false-dedupe risk) and
  content roots as ordinals (about +80 µs per cold leaf read).

### Not yet verified

- An end-to-end C12 on Postgres (all cycle figures are projections).
- The full 4-writer arm with local Saves on range-60-sized caches.
- Real network delay.
- Merge inside the lane on Postgres.
