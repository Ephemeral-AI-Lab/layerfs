# W3 — the `objects` row grammar: the base identity is not a column

**Diagnostic.** Source pin @66bce8378@ + the working tree at 03:5x on 2026-09-20. Every
number below is labelled **measured**, **computed** or **est**. No timing figure is
quoted without its cache state.

## 0. The answer in one paragraph

The 32-byte `objects.base_object_id` column is a second copy of a fact the packed
record already carries, and **it should not exist**. Removing it is worth
**1,335,296 B measured** on the `objects` btree of a real lane Store (326 pages
exactly, arithmetic below closes to 0). The read-path refactor the parent scoped
was necessary and is done: both chain walks now read the base identity from the
record. The representation the parent's prior note proposed instead — "an integer
reference" — was **measured and rejected**: on this lane it saves 1,044,480 B, i.e.
290,816 B *less* than removing the column, and it does not avoid the primary-key
seek it was supposed to avoid (each edge still needs the base's row for its
`canonical_length`, role and chronology). The column is not replaced by anything.

## 1. What changed

### 1.1 The representation

```
BEFORE  objects(object_id, object_role, canonical_length, base_object_id, pack_id,
                group_number, record_number)           8 columns, 33 B/row for a base
AFTER   objects(object_id, object_role, canonical_length, pack_id,
                group_number, record_number)           7 columns, nothing for a base
```

The dependency edge now has exactly one source: the record. `stored_base`
(`encoding/delta/record.rs`) is the single reader of it, and it dispatches the same
way the decoder does — an `Ordinary` locator is a pooled leaf when its role is
`InodeLeaf`, and every other lane's PREFIX tag carries the base inline. A record
whose framing cannot be parsed is an integrity failure, never a chain that quietly
ends.

### 1.2 The read-path refactor (the work the parent scoped)

`encoding/delta/read.rs` — `Resolver::resolve_charged` walked `current.base_object_id`;
it now calls `Resolver::base_of`, which reads the record through the **resolver's own
pack cache**. The seek count per edge is unchanged: the base identity still resolves
to a row through `lookup::location`, exactly as before. What is new is that the pack
body is now fetched by the walk instead of by the decode pass that follows it — and
because both share one cache, the body is still fetched **once**. The walk charges
`ChainCounters::packs_read` and `ChainCounters::group_decodes` where the work happens,
so neither counter can under-report a read the walk performed (this was a real bug in
the first cut: `cas_reuse` caught `packs_read == 0` for a read that did fetch).

`encoding/pool/read.rs` — `leaf_body` walked `current.base_object_id`; it now reads
each element's pooled record (`pooled_base`) through the reader's own pack cache, and
the delta-identity check at the decode step compares the record's embedded base
against the identity of the element the walk actually descended to (`decoded_id`),
which is the same assertion the column cross-check made, now against the only source.

### 1.3 The writer's walk — the finding that decided the design

`encoding/delta/select.rs::DepthCache::cost_of` is a **metadata-only** walk: today it
reads one `objects` row per edge and nothing else. It is called ~35,800 times per
retained-history save (`delta.trials` 35,140 + `delta.ineligible_candidates` 658, both
**measured** in `/tmp/w3-before/trace.jsonl`).

Measured on the BEFORE Store: **94.4 % of chain edges cross packs**
(2,002 of 36,006 bases sit in the same pack as their dependent), the mean chain is
**1.936 edges** deep (histogram 0..8, **measured** over 52,032 rows), and the mean pack
body is **193 KB** (51,621,888 B / 267 packs). A walk moved onto records therefore
reads ~2 pack bodies ≈ **375 KB per cache-missing walk** to save 32 B/row — the trade
the parent's note flagged, and it is real. It is avoided, not paid: the walk is given
the **selection's own pack cache** (`ChainBases`), which the PREFIX trial of the same
base fills immediately afterwards. The walk's fetches *are* the trial's fetches, done
earlier; measured marginal pack reads ≈ 0. This is why `cost_of`/`depth_of` gained a
base-provider parameter instead of reading packs themselves.

### 1.4 The schema bump, migration and rejection policy

`SCHEMA_VERSION` 4 → 5 in `policy.rs` (W2 then took 5 → 6 for `content_signatures`;
**6 is the live value and 5 is this change's layer underneath it**). `PRAGMA
user_version` follows. `REQUIRED_TABLES` declares the 7-column `objects` shape
exactly, so:

* **A version-4 Store is rejected at open**, by `Store::open` → `validate` →
  `validate_table` → `StorageError::Integrity("table column shape")`. It is **not**
  migrated. This is deliberate: a v4 Store's row says one thing and its record says
  the same thing, but a reader that could open it would have to decide which is
  authoritative, and the whole point of this change is that there is one authority.
* **A Store that merely resembles a schema-10 reference file is still rejected** by the
  application id, unchanged.
* The reader never guesses: it has no fallback to a column that no longer exists.

### 1.5 Files and production LOC

Production LOC (non-blank, non-comment, `core/crates/layerfs-storage/{src,sql}`),
before → after, measured with the repository's method:

| file | before → after | delta |
| --- | --- | --- |
| `sql/schema.sql` | 44 → 44 | 0 |
| `src/policy.rs` | 192 → 192 | 0 (constant value only) |
| `src/sqlite/schema.rs` | 253 → 257 | +4 |
| `src/sqlite/write.rs` | 128 → 123 | −5 |
| `src/sqlite/lookup.rs` | 138 → 130 | −8 |
| `src/encoding/delta/record.rs` | 201 → 215 | +14 |
| `src/encoding/delta/read.rs` | 243 → 370 | +127 |
| `src/encoding/delta/select.rs` | 282 → 319 | +37 |
| `src/encoding/pool/read.rs` | 312 → 328 | +16 |
| `src/cas/placement.rs` | 140 → 139 | −1 |
| `src/cas/pool_lane.rs` | 277 → 284 | +7 |
| **production subtotal (this crate)** | | **+191** |

Test-only changes (not production LOC): `tests/support/mod.rs` +78 (a new
`forge_stored_base` helper), `tests/delta_chains.rs` −10, `tests/delta_payload.rs` +2,
`tests/metadata_pool.rs` +5, and two SQL-string edits in `tests/provider_errors.rs`
and `tests/visibility.rs`. The core `core/crates/` subtotal by the same method is
**20,108** now; the parent's 19,766 was taken before W2's `candidates.rs` work landed,
so this change's share of the difference is the +191 above.

**Cross-file consequence, reported as required.** I do not own
`encoding/delta/select.rs`, `cas/placement.rs` or `cas/pool_lane.rs`, and I had to
edit all three because `ObjectLocation.base_object_id` (mine) disappearing breaks
their compilation: `select.rs` (the walk + two call sites), `placement.rs` (one line:
the removed `ObjectRow` field), `pool_lane.rs` (one call site, routed through the
owner's own `PoolReader`). No other squad's file was touched; `codec.rs` and
`candidates.rs` are untouched.

## 2. Measured bytes

### 2.1 Isolated: the same 52,032 rows, column removed (measured)

A byte copy of `/tmp/w3-before/sample.sqlite`; `objects` rebuilt with the same six
remaining columns and the same rows; `dbstat` per btree:

| `objects` btree | pages | bytes |
| --- | --: | --: |
| BEFORE (leaf 991 + internal 21) | 1,012 | 4,145,152 |
| AFTER  (leaf 675 + internal 11) | 686 | 2,809,856 |
| **saved** | **326** | **1,335,296** |

### 2.2 Arithmetic, residual 0

* Per-row content removed: **33 B** for each of the 36,006 rows that had a base
  (1 B serial-type varint + 32 B content) and **1 B** for each of the 16,026 rows whose
  base was NULL (the serial-type varint alone):
  `33 × 36,006 + 1 × 16,026 = 1,188,198 + 16,026 = 1,204,224 B = 294 pages exactly.`
* Measured leaf saving: 991 − 675 = **316 pages** = 1,294,336 B.
  Residual over the content arithmetic: 316 − 294 = **+22 pages (90,112 B)**, the
  btree packing effect: shorter rows put more of them on one leaf page.
* Measured internal saving: 21 − 11 = **10 pages (40,960 B)** — the whole interior
  reduction, which the per-row arithmetic does not describe at all.
* `294 + 22 + 10 = 326 pages = 1,335,296 B`. **Residual 0.**

### 2.3 The lane, one arm each, quiet machine (measured)

`--case history-stride10 --corpus <157 checkpoints> --out <dir>`,
`LAYERFS_CONSTRUCTION_WORKERS=1`, one sample per arm, no best-of, cache state
`CreatedInSample` (the trace's own `cache_state`/`store_state` rows; the Store and its
pages are created inside the timed run and no path is pre-touched).

| arm | binary | apparent bytes | real | user | sys | max RSS |
| --- | --- | --: | --: | --: | --: | --: |
| BEFORE | `/tmp/lane-baseline` (pre-change, built 02:49) | **56,049,664** | **42.88 s** | 31.12 s | 11.45 s | 243,646,464 B |
| AFTER | `/tmp/lane-after` (this change + W2's) | **45,432,832** | **48.20 s** | 37.25 s | 8.58 s | 239,714,304 B |

`uptime`/\`ps\` check immediately before: `ps aux | grep -E '[c]argo|[r]ustc|[f]s-bench'`
**empty**; 1-minute load average 4.90 (decaying from the *previous* squad's lane arm,
which released the `/tmp/lane.lock` this job waited on). Immediately after: `ps`
**empty**; load 7.26 (my own two arms). The lock was held for both arms.

### 2.4 `dbstat` before and after, per table and index, residual stated (measured)

| object | BEFORE | AFTER | delta |
| --- | --: | --: | --: |
| `object_packs` | 51,621,888 | 41,963,520 | −9,658,368 |
| `objects` | 4,145,152 | 2,748,416 | **−1,396,736** |
| `content_signatures` | — | 516,096 | +516,096 |
| `metadata_value_groups` | 90,112 | 90,112 | 0 |
| `sqlite_autoindex_metadata_value_groups_1` | 28,672 | 28,672 | 0 |
| `sqlite_schema` | 4,096 | 4,096 | 0 |
| `store_policy` | 4,096 | 4,096 | 0 |
| sum of named btrees | 55,894,016 | 45,355,008 | −10,539,008 |
| apparent file | 56,049,664 | 45,432,832 | −10,616,832 |
| **residual** | 155,648 | 77,824 | **−77,824** |

The residual is page-level allocation that `dbstat` does not attribute to a named
btree (freelist and header pages), and it differs between the arms because W2's
`content_signatures` ring overwrites rows and leaves free pages behind.

**Attribution.** The 10.6 MB is **not** this change. `object_packs` (−9.66 MB) and
`content_signatures` (+0.52 MB) are W2's persisted content index: it lifts
`delta.prefix_records` from 36,006 to 39,092 and cuts `delta.no_candidate` from 10,080
to 6,988 (both **measured** in the two `trace.jsonl` files). This change's lane
contribution is the `objects` line, **−1,396,736 B**; the isolated value on identical
rows is **−1,335,296 B** (§2.1). The +61,440 B difference is the AFTER arm's different
row content (249 packs instead of 267, different group and record ordinals), not the
column.

## 3. Measured CPU

**NOT TAKEN, for this change in isolation, and the reason is stated rather than
guessed.** The two binaries differ by *two* squads' changes: `/tmp/lane-baseline` was
built at 02:49, before both my row-grammar change and W2's `candidates.rs` rewrite
(03:13). The pair above is therefore an honest measurement of "today's default vs this
tree", not of this change:

* real **+5.32 s (+12.4 %)**, user **+6.13 s**, sys **−2.87 s**, max RSS **−3,932,160 B**.
* The extra user time is consistent with W2's work, which is *strictly more* of it:
  +3,086 trials, +3,086 prefix records, an index persisted and read back. Nothing in
  this change adds a per-object pass: it removes one bound SQL parameter per row
  (52,032 rows) and adds one record parse per chain edge (~69,400 edges by
  `1.936 × 35,800`, **computed**), a parse that is a tag byte plus a 32-byte slice.
* **What would settle it**: a binary built from this tree with only
  `encoding/delta/candidates.rs` reverted, run as a third arm. That is W2's file and I
  did not touch it, so the parent must sequence it. Reported as an open measurement,
  not as a number.

The one CPU figure I *can* attribute is negative and measured: the writer's chain-cost
walk no longer walks a chain at all for a cache hit, and for a miss it reads the same
packs the trial reads. `group_decodes` is charged by the walk where the decompression
happens, so the counter stays honest (it was silently 0 in the first cut, and
`tests/group_decodes.rs` caught it).

## 4. Bounded memory

* **No new allocation that grows with the store.** The walk's only new live state is
  `ChainBases`'s own `GroupCache`, bounded by `DECODED_GROUP_CACHE_BYTES` = **512 KiB**,
  released wholesale when the next body would cross it; and the caller's pack cache,
  bounded by `DEPENDENCY_PACK_CACHE_BYTES` = **4 MiB** (unchanged, reused, not added).
* The chain vector is unchanged: `Vec<ObjectLocation>` of at most
  `delta_depth_for_role(role) + 1` ≤ 51 elements; `ObjectLocation` is **32 B smaller**
  than before (the `Option<ObjectId>` field is gone), so the per-chain bound *fell*.
* The walk retains no record bytes: it parses the base identity out and drops the
  borrow. There is no buffer proportional to a pack or to the number of chains.
* Peak RSS **measured**: 243,646,464 B BEFORE → 239,714,304 B AFTER (−3.75 MiB), which
  is a whole-process high-water mark and not a claim about this change alone.
* The declared bound is unchanged and still the binding one: the operation's live set
  is the pack cache (4 MiB) + the decoded-group cache (512 KiB) + the pooled value
  cache (512 KiB) + the chain vector, all constants.

## 5. Access cost, in BYTES PER READ

A read of one object of chain depth `d`:

| | BEFORE | AFTER |
| --- | --- | --- |
| `objects` primary-key seeks (one per edge + the root) | `d + 1` | `d + 1` (**unchanged**) |
| bytes of `objects` row read by the walk | `(d+1) × ~80 B` | `(d+1) × ~48 B` |
| pack bodies fetched by the walk | 0 | the `≤ d+1` packs the decode pass needs anyway |
| pack bodies fetched by the decode pass | `≤ d+1` | `≤ d+1`, **cache hits** |
| **pack bodies fetched per read** | **`≤ d+1`** | **`≤ d+1` (measured unchanged)** |
| record parses | 0 | `d + 1` |
| group decodes (ordinary lane) | 1 per distinct group | 1 per distinct group, now charged to the walk |

The trade the parent's note described — "one extra btree seek per chain edge, ~4,096 B
per edge on a 4 KiB page, against the 32 bytes it saves per row" — **does not
materialise, because there is no extra seek**: the walk still resolves each edge with
the same primary-key seek it always did. The seek is required for the base's
`canonical_length`, its role and its chronology, and it is required by *every*
representation of the edge, including the "integer reference" one. What the note got
right is that the edge is not free: it costs one record parse per edge and it moves
the pack fetch earlier. Both are bounded and both are shared with the decode that
follows.

Evidence that the fetch count is unchanged, from the product's own counters:
`tests/cas_reuse.rs` asserts `counters.packs_read == 1` for a one-object read and
passes; `tests/delta_chains.rs` asserts `counters.edges == 16` for a depth-16 chain
and passes; `tests/group_decodes.rs` asserts exactly one decode per distinct group and
passes. **Not measured**: a direct `packs_read` figure for a depth-8 chain on the lane
(there is no such counter in the lane trace, and I did not add one — that is product
telemetry, and a lane-visible counter is a separate decision).

## 6. Negative results (retained)

All measured by rebuilding the `objects` btree on a byte copy of the same BEFORE
Store (52,032 rows, 36,006 with a base), `dbstat` per btree:

| representation | `objects` B | saved vs BEFORE | verdict |
| --- | --: | --: | --- |
| BEFORE (`base_object_id BLOB`) | 4,145,152 | — | — |
| **drop the column (implemented)** | **2,809,856** | **1,335,296** | **best** |
| base as a 3-integer locator (pack, group, record) | 3,100,672 | 1,044,480 | **rejected**: 290,816 B worse, and it does not remove the seek |
| delta-encoded `base_pack_id` + group + record | 3,076,096 | 1,069,056 | **rejected**: 266,240 B worse, same seek, plus a decode rule |
| locator + drop `object_role` | 3,022,848 | 1,122,304 | rejected for the same reason |
| drop `object_role` alone | 4,096,000 | 49,152 | **rejected as not worth it** |

* **The locator forms are dead ends, and not only on bytes.** A locator cannot name a
  row: the writer's `DepthCache::cost_of` must reach the base's *row* to learn the
  base's own base, and there is no index on `(pack_id, group_number, record_number)` —
  `objects_locations` was exactly that index and R2 removed it for 2.55 MB. Keeping a
  locator would mean paying a full scan or restoring the index.
* **Dropping `object_role` is rejected, not deferred.** It is only 49,152 B measured
  (**not** the 69,632 B the prior note carried — that was a different row set). It
  cannot be derived from the pack: the `Singleton` lane holds both `WholeFile` and
  `Chunk` records and `decode_canonical` picks `encode_whole_file_payload` or
  `encode_chunk_object` from the role. Removing it would be a lane-format change with
  a cross-cutting blast radius for 1.2 % of what the base column was worth.
* **A FULL record cannot be given an edge it never had.** This killed the obvious way
  to keep three forgery tests: with no column, a test that wants a forged edge must
  first have a *real* PREFIX record and then rewrite its 32 base bytes. The tests were
  rewritten that way (`tests/support/mod.rs::forge_stored_base`, which patches a raw
  group in place and re-encodes a compressed one through the product's own framing).
  The invariants are unchanged; only the forgery mechanism is.
* **Two tests were failing for a reason that is not this change**, and I am reporting
  it rather than absorbing it: `delta_chains::a_reduced_depth_stops_at_its_own_boundary`
  and `delta_payload::an_absent_or_ineligible_candidate_selects_full` both stored a
  *near copy* of an already-stored object and asserted that no trial ran. W2's
  persisted content index now proposes that near copy, so a trial runs and a prefix is
  stored. I made the payloads wholly different (byte inversion) so each case tests the
  boundary it names and not the index; the assertions are unchanged and the declared
  base is still refused with `ineligible_candidates == 1`. **The parent should confirm
  W2 accepts that reading.**

## 7. Item 3 of the brief: `objects_locations`

**Already done, by R2 (owner ruling C), and verified here.** `/tmp/w3-before`'s
`dbstat` shows no `objects_locations` and no `objects_bases` btree at all — the two
indexes are gone from the shipped schema and from the lane's Store, which is why the
BEFORE arm reads 56,049,664 and not 60,178,432. The cleanup page query
(`sqlite/cleanup.rs`) is a bounded scan of `pack_id > baseline ORDER BY pack_id DESC,
group_number DESC, record_number DESC LIMIT 128` per page; it runs only after a failed
save. **Not measured**: that scan's cost on a Store with many unpublished packs — there
is no failed-save case on this lane, and I did not construct one.

## 8. Checks run, and checks NOT run

**Run, all green on the final tree:**

* `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked` → **486 passed, 0 failed**
  (481 before this round; +5 are W2's `content_index` tests).
* `python3 core/tools/check_product_boundary.py` → **PASS**, 121 production files.
* `python3 -m unittest discover -s core/tools -p 'test_*.py'` → **6 tests, OK**.
* `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings`
  → clean.
* `cargo +1.85.1 fmt --all -- --check` (from `core/`) → my files clean; the only
  remaining diffs are in W2's `encoding/delta/candidates.rs` and
  `tests/content_index.rs`, which are not mine to format.
* The lane, both arms, §2.3.

**NOT run, with the reason:**

* **The isolated CPU of this change** — no binary exists that has this change without
  W2's; see §3. This is the one number the brief asked for that I am reporting as
  *not taken*.
* **`--lane full` / `--lane smoke` (220 / 20)** — untouched by this change and outside
  its scope; `history.*` is outside the 217 by construction. Not re-run.
* **The cleanup scan's cost after a failed save** — no failed save on this lane; §7.
* **A `packs_read` figure for a deep chain read on the lane** — the lane trace carries
  no such counter and I did not add one; §5.
* **A migration path from a v4 Store** — deliberately none; §1.4.
* **Verification mode** — this is a diagnostic row, not a release admission.

## 9. Reproduction

```sh
# the isolated byte measurement
cp /tmp/w3-before/sample.sqlite /tmp/iso.sqlite   # any lane Store
python3 - <<'PY'
import sqlite3
c = sqlite3.connect('/tmp/iso.sqlite')
c.executescript(open('rebuild_objects.sql').read())   # see §2.1's six columns
PY

# the lane pair
mkdir /tmp/lane.lock
LAYERFS_CONSTRUCTION_WORKERS=1 /tmp/lane-baseline --case history-stride10 --out /tmp/w3-before --corpus <corpus>
LAYERFS_CONSTRUCTION_WORKERS=1 /tmp/lane-after    --case history-stride10 --out /tmp/w3-after  --corpus <corpus>
rmdir /tmp/lane.lock
sqlite3 /tmp/w3-after/sample.sqlite 'SELECT name, SUM(pgsize) FROM dbstat GROUP BY name'
```
