# Pre-registration — #219 round 16: the ordinal reservation is a block, not a leaf

Written **before** any edit and before the first run of this arm. This is a **product** change in the
pooled metadata lane's transaction cadence.

Control: **P2** `ns19-P2-repin-20260921T102900Z` (`6a1b2d9f2`, PASS, 13/13, 14/14):
`operation_work_ns` **1283.16 ms**, CPU 1297.49 ms, **`diag_commit_total_ns` 497.54 ms — 38.8 % of the
row**, `commits` **285**, `diag_begin_ns` 3.81 ms, `diag_wave_ns` 62.67 ms, `stored_records` 23,910,
`pack_bytes_written` 301,864,382. This arm compares against that row and **does not re-run it**.

## The one difference

**A save reserves pooled ordinals in blocks of at least 1024, so one acknowledged reservation covers
many leaves instead of one leaf.**

`cas/pool_lane.rs:119-130` reserves exactly the ordinals one leaf's fresh values need and then, its own
comment says, *"The reservation is its own step"*: `commit_reservation` clears `wave_held` for one call
so the reservation is `COMMIT`ted **inside the wave**, on purpose — an aborted save's ordinals are
never handed out again. That is a real durability boundary and this round does not touch it. What it
changes is its **granularity**, which is one leaf.

Read out of the control row's own Store (`raw/ordinal-census.txt`): `store_policy.next_ordinal` =
**10,164**, so 10,163 pooled values were handed out, over **207** catalogue groups — about **49 fresh
values per leaf**, and therefore ~207 reservations. The row's `commits` is **285**:

| commits | count | why |
| --- | ---: | --- |
| initial + final | 2 | `MutationOwner::acquire`, `finish_inner` |
| waves | ~73 | `WAVE_CANONICAL_BYTES_LIMIT` = 4 MiB − 1 over 302,231,057 canonical bytes = 72.06, and a wave's own `maybe_commit` **does not fire** for a wave whose transaction a reservation already closed |
| **ordinal reservations** | **~210** | one per leaf with fresh values |

**74 % of this row's COMMITs are ordinal reservations.**

**Why the count is the lever, with the pragmas as the mechanism.** The connection profile is
`journal_mode = MEMORY`, `synchronous = OFF` (`sqlite/connection.rs:36`, `:40`): a `COMMIT` writes no
journal file to disk and performs no sync, so what it does cost is **copying the dirty page images into
the database file**. SQLite keeps committed pages in its pager cache, so a page dirtied again in a later
transaction is written **again** — the pager's write traffic is the bytes of the store multiplied by the
number of transactions each page is dirty in. Every reservation closes the open transaction in the
middle of a wave, which splits a 4 MiB wave into as many transactions as it holds leaves, and every pack
a later seal appends to is rewritten in each of them.

`pack_bytes_written` — the bytes the *product* hands the pager — is 301,864,382, and the commit term is
497.54 ms for it, i.e. 607 MB/s. That is well below what a page-cache copy costs, and the difference is
the amplification this round is aimed at.

**No pragma is a variable of this arm.** The connection profile —
`journal_mode`, `synchronous`, `temp_store`, `cache_size`, `page_size`, `mmap_size`,
`cache_spill`, `busy_timeout` — is exactly what the control row ran it with, and
`src/sqlite/` is untouched by this round. The profile is cited only as the
*mechanism*: a `COMMIT` under it is a page-cache write with no journal file and no
sync, which is why re-dirtying a committed page costs a second write. Changing the
profile is a different question and is not asked here.

**The change.** `ORDINAL_RESERVE_BLOCK` joins the policy constants; the owner keeps the end of the
block it holds (`ordinal_block_end`); a leaf reserves a new block only when the fresh values it needs do
not fit the remainder of the current one, and the block is `max(fresh_count, ORDINAL_RESERVE_BLOCK)` so
a single enormous leaf still gets exactly what it needs. The reservation is still committed before any
value uses it, which is the same guarantee, now covering more values.

## The rule this round registers, and the two it replaced

The first rule written down here was **a fixed block of 1,024 ordinals**, and it was
measured against the product's own suites before any row was run. Two consequences
were measured, both from the block's size, and both are why the registered rule is
not that one:

1. **It hands out ordinals the values never use.** On `metadata_window.rs`'s
   1,312-leaf fixture, whose leaves carry a steady 100 values each, a fixed block
   returned **134,645 ordinals for 131,200 values** - 924 wasted per block, because
   the block is not a multiple of what a leaf needs.
2. **It pulls the retained window forward, which is a real cost and not a
   cosmetic one.** The window is the pool index's candidate set; a count that
   includes unused ordinals makes the index release values it still holds, and the
   same fixture's crossing group moved three leaves early. That is duplicate
   physical values in a workload that revisits old ones.
3. And a smaller discontinuity is enough to lose a delta: with contiguous ordinals
   a one-value leaf's COPY/INSERT program is **50 bytes against a 57-byte FULL**;
   with the next block's first ordinal it is **57 against 57**, a tie, and a tie
   stores FULL. That is why the first reservations stay exact.

The registered rule is therefore three parts, each answering one of those:

- **the first 4 reservations of a save are exact**, so a save with few leaves - and
  every small workload - hands out exactly the ordinals it did before;
- **after that a block is `fresh_values_of_this_leaf x 16`**, a multiple of the
  demand just observed rather than a constant, so a steady workload consumes its
  blocks exactly and hands out no more ordinals than it uses;
- **the window counts values, not reservations** (`ownership::note_window`, charged
  once per leaf with the ordinals that leaf actually handed out), so the crossing
  point is the value count's and not the block's;
- **the block's unused tail is released when the save publishes**, by a
  compare-and-swap on the reservation this save made (`ownership::release_ordinals`),
  so a partly-consumed final block leaves no hole. A second writer that reserved in
  between makes the swap fail and the tail is simply not reclaimed: a wasted tail,
  never an ordinal handed out twice.

`raw/ordinal-census.txt` replays that rule over the control row's Store: **17
reservations** where the row made 207, with **10,163 ordinals handed out either
way**.

## Price, count and width

| | count | width |
| --- | ---: | ---: |
| today: reservations | ~207 | 49 values each |
| arm: reservations | **17** | 4 exact, then 13 blocks of ~784 |
| arm: commits | **~85** | was 285 |

Two widths grow and are declared, not hidden:

- **ordinals burnt on an aborted save**: up to `ORDINAL_RESERVE_BLOCK − 1 = 1023` per save, from the
  block's unused tail. The space is 2^32 ordinals and the value is that a handed-out ordinal is never
  handed out again, which is exactly the property the per-leaf commit exists to keep.
- **the retained-window count over-claims**: `reserve_ordinals` advances `metadata_window_values` by
  the reserved count (`sqlite/ownership.rs:205-211`), so the window now claims up to 1023 values that do
  not exist. The window is a candidate-cache boundary — `advance_window` drops cached candidates below
  a start ordinal and `sync` reads catalogue groups from it — and its *start* is still the first ordinal
  of a block that is being used, so no read can be skipped. Only the count is loose, and only against
  the 131,072-value cap.

## Prediction, in the instruments' own units

| instrument | P2 | predicted | derivation |
| --- | ---: | ---: | --- |
| `commits` | 285 | **78–95** | 2 + the waves no reservation closed (~62–73) + 17 reservations |
| **`diag_commit_total_ns`** | 497.54 ms | **350–430 ms** | −68 to −148: the transactions fall 3.4× and the pages they rewrite fall with them |
| `diag_begin_ns` | 3.81 ms | **1.0–1.6 ms** | one `BEGIN IMMEDIATE` + `next_pack` read per transaction, ~13.4 µs measured |
| `diag_wave_ns` | 62.67 ms | unchanged ±5 | the wave count and its query are untouched |
| `operation_work_ns` | 1283.16 ms | **1150–1215 ms** | −68 to −133 |
| CPU user+system | 1297.49 ms | **1170–1235 ms** | the same work, counted |
| `pack_bytes_written` | 301,864,382 | **exactly unchanged** | nothing about packing changes; a control |
| `statements`, `pack_appends`, `packs_created`, `stored_records` | 7,666 / 6,603 / 1,270 / 23,910 | **unchanged** | a control |
| all 14 pins + digest | — | unchanged | |

## What would refute it

1. **`commits` >= 200.** The block did not take, and whatever the row does is not this mechanism.
2. **`diag_commit_total_ns` >= 480 ms.** The commit term is byte-bound rather than transaction-bound:
   the count is not a lever, and **the serial floor's largest term cannot be attacked this way at all**.
   That is the outcome this registration is most concerned to be able to tell apart from a working one,
   and it would be reported as a refutation of the direction, not of the round.
3. `operation_work_ns` >= 1265 ms with clauses 1 and 2 satisfied: the count fell and the row did not.
4. Any of the 14 pinned counters moves, `pipeline.commits` is not the one that moves, or
   `digest:filesystem_root` != `1d6fba29…3857847`, or the row is not PASS 13/13.
5. **Something other than the commit term moves.** `pack_bytes_written`, `statements`, `pack_appends`,
   `packs_created` and `stored_records` are controls: they must be *identical*, because this change
   alters when a transaction is acknowledged and nothing else.
6. `tests/metadata_pool.rs`, `tests/metadata_pool_index.rs`, `tests/metadata_window.rs` or
   `tests/metadata_chain.rs` fails.
