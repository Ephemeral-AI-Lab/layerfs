# #219 round 19 — the commit term is a page-write term, and the pack capacity **is** flushed

Status: **Instrument.** No product line changed, no arm registered as a shippable change, no gate
claimed. One labelled diagnostic ran, once; its whole stdout is `raw/commit_page_price.txt`.
Pre-registration, written before the first edit and the first run:
[`pre-registration.md`](pre-registration.md).

**Owner ruling honoured: the database page size is 4 KiB** — read and asserted on all six arms, never
set. No product source, no product manifest and no pragma was touched; `sqlite/connection.rs` is
untouched.

---

## Why it ran

Round 18 took **27,022,997 ns** out of `diag_commit_total_ns` by removing **1,101,824 B** of index —
**24.5 ns per byte**. The same term prices the row's 301,865,004 B of pack payload at **1.43 ns per
byte**. One term, two prices, 17× apart. This round reads the engine's own counters to find out which
of the two descriptions is the term's law.

The instrument is squad C's arm D, registered two campaigns ago and never built
(`issue219-squadC-cadence-20260921T044258Z/pre-registration.md`): `sqlite3_db_status(CACHE_WRITE /
CACHE_SPILL / CACHE_USED)` and `sqlite3_status(PAGECACHE_*)` on the product's own connection, plus
`page_count`/`freelist_count` and the file's `st_size`/`st_blocks`.

## Question 1 — the commit is priced per page-write

| arm | `pages_written` | `cache_write` | `commit_ns` | ns per page-write |
| --- | ---: | ---: | ---: | ---: |
| `reference` | 320 | **322** | 1,302,042 | **4,044** |
| `store-unindexed` | 337 | **339** | 2,819,583 | **8,317** |
| `store-indexed` | 601 | **604** | 5,298,417 | **8,772** |

**`cache_write` is the flush, and it tracks `page_count` growth to within three pages on every arm**
(322/320, 339/337, 604/601 — the three are the schema and pointer pages). Refutation 2 did not fire, so
the counter measures what this round takes it for.

**Refutation 1 fires.** The price per page-write is **not one constant**: 4,044 ns for the reference's
shape against 8,317 ns for this Store's, **2.06×**, just past the registered 2× bound. So this round
does **not** claim a single page price. It reports what it measured, and the pair that decides the
index question — `store-unindexed` against `store-indexed`, identical but for one index — agrees to
**5.5 %** (8,317 vs 8,772 ns).

**What Q1 settles is round 18's 17× puzzle.** The index adds **265 page-writes** in one transaction
(604 − 339) and **+2,478,834 ns** of commit — **9,354 ns per index page-write**. Round 18's 27.02 ms of
product commit movement is therefore **2,889 page-writes for 265 pages**, i.e. each of the index's pages
is written about **10.9 times across the row's 95 transactions**. The index was never expensive per
byte; it was expensive because **a B-tree page is rewritten many times while an append-only pack page
is written once.** "Byte-bound" and "page-bound" coincide for the pack and diverge for the index, and
that is the whole of the discrepancy.

## Question 2 — the declared pack capacity **is** written to disk

Round 17 measured that this row's `object_packs.data` sums to **332,922,880 B = 1,270 × 262,144 =
`packs_created` × `PACK_LIMIT`**, because `sqlite/write.rs:88-95` creates every pack zero-filled at
capacity with `zeroblob(?2)`, while only **301,865,004 B** of content is written into them. The
difference, **31,057,876 B = 7,582 pages**, was recorded `NOT_MEASURED` in time. Three arms, 1,270 packs
each at `PACK_LIMIT`:

| arm | payload written | `pages_written` | `cache_write` | `cache_spill` | `commit_ns` | whole transaction |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `packs-zeroblob` | **0 B** | 81,439 | **81,441** | 61,440 | 92,483,792 | 490,069,750 |
| `packs-partial` | 301,865,030 B | 81,439 | 155,260 | 135,259 | 43,861,291 | 617,453,208 |
| `packs-full` | 332,922,880 B | 81,439 | **81,441** | 61,440 | 96,580,625 | 493,618,000 |

**Refutation 3 fired: `packs-zeroblob` writes all 81,441 pages for a payload of zero bytes.** The
declared capacity is not free, and the mechanism is exact: **a `zeroblob`'s overflow chain still
requires every page's next-page pointer to be written**, so every page of the reserved capacity is
dirtied even though its payload is zeros. The confirmation is the pair's cost: **`packs-zeroblob` and
`packs-full` write the same 81,441 pages and cost 490.07 and 493.62 ms of transaction time — 0.7 %
apart for 31 MB more payload.** Content is written into pages the reservation had already paid for.

### A method finding that changes how the term must be read

`cache_spill` is **61,440 pages** on the zero-content and full arms, and **135,259** on the partial one.
**Most of the flush happens during the transaction, before the COMMIT**, so `commit_ns` alone
undercounts it — which is why the raw `ns_per_cache_write` field reads 1,136 ns for `packs-zeroblob`
against 8,772 ns for `store-indexed`. The honest per-page price is the **whole transaction over
`cache_write`**, and that is what the report uses: **6,017 ns** (`packs-zeroblob`) and **6,061 ns**
(`packs-full`), agreeing to **0.7 %**. `packs-partial` is cheaper per page (3,976 ns) because its pages
are dirtied twice — once by the insert's spill and once by the in-place write — and the second write to
a resident page is cheap.

## What this means for the row

The row's own `diag_commit_total_ns` is **431,236,291 ns** and its Store holds **81,280 pack pages**
(`pack_bodies_bytes` 332,922,880 ÷ 4096, which `page_count` 81,987 confirms: 81,280 pack + 707
non-pack). That is **5,306 ns per page-write** — the same price the pack arms measured independently,
and 431,236,291 ÷ 5,306 = 81,280 exactly.

**7,582 of those 81,280 pages — 9.33 % — hold no content at all.** At the row's own measured price:

| | pages | ms |
| --- | ---: | ---: |
| pack pages carrying content (301,865,004 B) | 73,698 | 391.0 |
| **reserved capacity no content reached (31,057,876 B)** | **7,582** | **40.2** |
| at the pack arms' independent 6,017 ns per page | 7,582 | **45.6** |

**So round 17's space finding is a time lever after all: 40.2–45.6 ms of a 431.24 ms term is the pager
writing pack capacity that holds nothing.** It is 32–36 % of the remaining 126.73 ms gap to the 1 s
target, and it is measured on the engine's own counters at both ends.

## The direction this opens

The tail is not an accident of the codec; it is `PACK_LIMIT mod group size`. Round 17 measured the
average unreached tail at **24,455 B per pack**, which is about half a ~50 KiB group — so **the waste
is proportional to the pack count**, and there are 1,270 packs because `PACK_LIMIT` is 256 KiB.

| | packs | reserved | content | tail |
| --- | ---: | ---: | ---: | ---: |
| today, `PACK_LIMIT` 256 KiB | 1,270 | 332.9 MB | 301.9 MB | **31.1 MB** |
| `PACK_LIMIT` 1 MiB, same `GROUP_LIMIT` | ~295 | 309.3 MB | 301.9 MB | **7.4 MB** |

**Predicted: ~23.7 MB, 5,781 pages, ≈ 30.7 ms** — and it is a policy constant, not a format change, so
it moves `PACK_LIMIT` alone and leaves `GROUP_LIMIT`, the framing and every stored byte as they are.
This is commission direction #3's neighbourhood, and it now has a second and *measured* reason:
L73/L74's group-boundary lever was about statement and seal counts; this one is about pages the pager
writes for bytes nobody stored. It is **not** registered here — it is the next round's to register,
with its own prediction and its own refutation.

## What is not claimed

No product change, no arm, no gate, no release claim, and no claim that the commit term can be reduced.
The diagnostic measures the **engine** on a replica of the row's shapes; it does not measure the
product, and the transfer of the 5,306 ns per page-write to the row is an arithmetic agreement between
two instruments (431,236,291 ÷ 81,280 = 5,306 ns), not a paired measurement. The three locator arms are
single samples and their commits are 1.3–5.3 ms, so the 2.06× spread in Q1 may carry timing noise; it
is reported as measured and no law is claimed from it. `PACK_LIMIT` 1 MiB is an arithmetic prediction
from round 17's measured tail, not a measurement.

## Checks as run

- `cargo +1.85.1 test --release --locked --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml --test commit_page_price -- --nocapture --test-threads=1` — **1 passed, 0 failed**, 1.88 s.
- Lock parity after adding `libsqlite3-sys = "=0.38.2"` (the version and checksum the lock already carried through `rusqlite`): `shared/test_lock_parity.py` — **PASS, 46 shared entries, 0 mismatches**; the lock gained exactly one line and `--locked` still holds.
- **Not run:** the product's suites and `check_product_boundary.py` — no product source or product
  manifest was touched, and the harness is not product source; the harness's own wider suite beyond
  this diagnostic and the parity check.

## Production LOC

**31683 → 31683 (delta 0).** Method `python3 tools/production_loc.py --root <tree>`. This round adds a
harness test and two harness manifest lines, both outside the counted scope.
