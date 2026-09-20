# #209 format round — pre-registration: reserve the directory, pre-allocate the row, write appends incrementally

> Status: Research; **pre-registration, written before any product line was changed**.
> Round of 2026-09-21 continuing [#209](https://github.com/Ephemeral-AI-Lab/layerfs/issues/209)
> from `73e0b961c`. This is **an owner-authorized format change**: it moves the Store
> hash, which is why L57/L59/L61 all refused to ship it as an optimisation. The owner
> authorized it in session on 2026-09-21 ("proceed with format first").

## 1. Why the format, in one line

The instrumented round before this one measured, on the product's own connection, that
a step's transaction writes **27.839 pages per commit** of which **24.433 are the pack
body it just grew**, and that the body is rewritten in full because
`sqlite3BtreeInsert` overwrites in place only when the new payload is **the same size**
as the old. The body is 2.13 s of the 2.912 s `commit_ns` bucket and 0.95 s of
`append_pack`. **That is the largest removable term left in the operation.**

## 2. The design measurement, taken before the design was chosen

`scratch/pack_row_probe.c` replays the four candidate write paths over a copy of the
measured run's own Store, arms interleaved round-robin in one process, reading
`SQLITE_DBSTATUS_CACHE_WRITE` around **every** `COMMIT` so no step is priced without
the pages it wrote. 150 steps per arm, three rounds, 266,256-byte row (a 4 KiB reserved
directory plus a 256 KiB pack limit), bodies of 1,450 bytes appended one at a time
([`scratch/pack-row-arms.txt`](scratch/pack-row-arms.txt)):

| arm | µs/step | pages/commit |
| --- | ---: | ---: |
| today: whole-row `UPDATE`, front directory, growing row | 2202.0 | 34.158 |
| same-size row, front directory, bodies shift by 16 B | 90.0 | **29.067** |
| same-size row, **reserved directory**, append-only tail | 49.2 | **3.353** |
| **incremental blob**: directory + tail ranges only | **36.1** | **3.367** |

**Three things this settles, none of which was known before it was run:**

1. **Pre-allocating the row is not enough.** A same-size payload still costs 29.067
   pages/commit against 34.158, because the group directory sits **at the front** of the
   pack and one new 16-byte entry shifts every body byte — every page's content changes,
   so SQLite's `btreeOverwriteContent` memcmp guard
   (`if( memcmp(pDest, pX->pData+iOffset, iAmt)!=0 ){ sqlite3PagerWrite(...) }`) never
   fires. **The layout is the blocker, not the payload size.**
2. **Reserving the directory at a fixed offset is the fix.** Bodies then never move, an
   append changes exactly two byte ranges, and the commit's page count falls **10.2×**
   (34.158 → 3.353). The floor of ≈3.35 pages is page 1's change counter, the pack's
   leaf page, the directory page and the tail page — and it does **not** grow with the
   body.
3. **The incremental-blob write is the cheaper way to spend those pages** (36.1 µs
   against 49.2 µs per step) because the whole 266 KB row is no longer bound through
   `sqlite3_bind_blob(..., SQLITE_TRANSIENT)` and copied per step.

## 3. The treatment, pre-registered

**Reserve the pack directory at a fixed offset, record the assembled length in the pack
header, pre-allocate the row, and write an append as an incremental blob write of only
the bytes that changed.**

| element | today | treatment |
| --- | --- | --- |
| body base offset | `HEADER_LEN + dirent × groups` (moves every append) | `HEADER_LEN + dirent × group_count_limit` (**constant**) |
| assembled length | `length(data)` | a `u32` in the header (`HEADER_LEN` 16 → 20) |
| row length | exactly the assembled length, grows every append | reserved in 32 KiB steps, **constant between growth events** |
| append write | `UPDATE … SET data = ?2` with the whole pack | `sqlite3_blob_write` of the directory entry and the new body only |

**Prediction, stated against the instrumented row's own window (`commit_ns` 2.912 s,
27.839 pages/commit, `append_pack` 20.68 µs, operation 25.986 s):**

- `delta.rca.pages_at_commit / commits` **≤ 6.0** (from 27.839);
- `commit_ns` **≤ 1.0 s** (from 2.912 s), i.e. **−1.9 s or better**;
- `append_pack` engine time **≤ 6 µs/call** (from 20.68 µs), i.e. **−0.6 s or better**;
- the transient body copy disappears with the blob write: **−0.17 s**;
- **operation −2.5 s to −3.2 s (−10 % to −12 %)** in a matched in-window pair.

**Falsifier, and it is a withdrawal:** if `pages_at_commit` per commit does not fall
below **10**, or the operation does not fall by **≥ 1.5 s** against a control arm
sampled from the same binary in the same window, the layout was not the blocker and the
treatment is **withdrawn** with the same evidence.

## 4. What this costs, stated before it is measured

- **The Store hash moves.** That is the point: the layout changes, every pack is
  re-framed, and the byte-identical-Store rule of L57/L59/L60/L61 **does not apply to
  this round**. The new hash becomes the new constant, and the comparison that must
  still hold is the **workload**: all 582 counters identical — the same appends, groups,
  objects, packs and chain counts.
- **Space.** The reserved directory costs `group_count_limit × directory_entry_len` per
  pack: 4,096 B for the ordinary, native, pooled and singleton lanes and 1,024 B for the
  compact whole-file lane — **+1.0 MB on a 51.9 MB Store (+2.0 %)**. The stepped row
  reservation costs at most 32 KiB per pack, 16 KiB on average — **+4.1 MB (+7.9 %)**.
  Total **≈ +5.1 MB, +9.9 %**, and it is reported beside the time, never instead of it.
- **Migration.** The pack framing version is bumped and `FORMAT_PROFILE` goes 1 → 2.
  This lane rejects older schemas and never migrates them, so **a Store written by the
  current product is not readable by the new one and vice versa**. That is an
  owner-visible consequence of the authorization and is recorded here rather than
  discovered later.
- **Memory.** No bound is raised: the reserved directory is a constant per lane, the row
  reservation is bounded by `PACK_LIMIT`, and `memory_bounds.rs` must stay green.

## 5. Measurement requirements for this round

Both writers, every time: stride10 operation and the seven buckets on a matched pair
sampled **from one binary in one window**, the concurrent path with second-writer latency
and throughput, and `multi_writer.rs` green. A change that recovers time by making the
second writer wait, fail or be refused has failed. Fresh `--output` per run, both global
flocks, receipts append-only, one sample per case per arm.

Checks: `cargo +1.85.1 test/clippy/fmt --manifest-path core/Cargo.toml --locked`,
`core/tools/check_product_boundary.py` and its self-tests, the harness suite (117 tests)
and a release build. No CI; `tools/preflight.sh` is permanently retired and must not be
used or restored.
