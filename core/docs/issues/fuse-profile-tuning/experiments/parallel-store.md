# Experiment: parallel Store fetches within one Workspace

> **Status:** Diagnostic prototype on branch `exp/parallel-store` (base
> `4db7d765c6`, issue #684). Not a product contract. One sample per arm.
> Times are diagnostic; counts are exact. Disposable Store; kernel
> write-back off; fuser not patched.

## Hypothesis

In round 2 the best cold read (128 KiB / 1, `async,noverify`) read 361–383
MB/s. The Workspace's single Store read session was busy 94% of the wall
time. Fetching with 2–4 read sessions in parallel should raise the cold
read substantially.

## Change (all off by default)

- **`LAYERFS_PROBE_READ_SESSIONS=N`** (1–8). Two parts:
  - **Store side.** A mounted Workspace opens N read-only readers at mount
    (`read_service.rs` `ReadSet::mount`). Each reader has its own SQLite
    connection on the one Store file and its own kept caches. They are
    lanes of the Workspace's one `ReadSession` (`read_handle.rs`). A demand
    takes the first free lane, the session's own first. When every lane is
    busy, it waits for the next lane in turn. Failure custody stays the
    session's: one unknown outcome on any lane quarantines it. Unmount
    closes every lane.
  - **Client side.** `CanonicalClient::fetch_parts` (`base/client.rs`) splits
    each read-ahead window's chunk list into N contiguous parts. Part 1 runs
    on the calling thread over the client's source. Each other part runs on
    a scoped thread over a fresh custody (`StorePorts::fresh`). Every part
    is joined. The first failure, in part order, fails the window, as one
    batch failure did before. This works with both sync (`single`) and
    `async` read-ahead.
- **`LAYERFS_PROBE_READ_WINDOWS=W`** (1–4, `async` only). Up to W background
  windows of one stream may run at once. The frontier rule is unchanged: a
  fetch starts only while less than one window has been fetched past the
  reader. So at most two windows past the reader are ever fetched, and the
  effective W is at most 2.
- **Probe.** The cold-read line gains:
  - `store_busy_ns`: the union of time with at least one Store demand
    running, which does not double-count overlap
  - `store_peak`: the largest number of concurrent demands
  - `session_requests`, `session_wait_ns`: read-session lock waits
  - `readers`: the number of readers opened

**Bounds.** The read-ahead window (8 MiB), the two-window frontier, the
64 MiB cache allowance and `DEMAND_BYTES` are unchanged. A window's bytes
are only split, never multiplied. New, declared: each extra session keeps
one read-only connection (2 MiB page cache) and up to 3 MiB of kept reads
(`KeptReads::ORDINARY_BYTES`), plus N−1 short-lived threads per window.

## Binaries

| SHA-256 | Arms |
| --- | --- |
| `db6065ed9da3234a575afa9012aa23bd0ba9134f9880d5db56451b2ed52bfff9` | b1, b2, b3 (sessions only) |
| `219db55ecc480af07320f0ba2244e6601473ac7320e3ca0af05dc1f54f4f67f7` (adds `WINDOWS`; final source) | b4, b5 |

## Arms

Every arm exited 0, and its 500 MiB read matched the source fold. Every
arm fetched 27,214 objects with 27,215 BLAKE3 calls, so each object was
fetched and hashed once and nothing was duplicated.

Columns:

- Busy: the Store-demand union busy time and its share of wall time.
- Peak: the largest number of concurrent Store demands.
- Wait: read-session lock waits. `base_admit` (Store reader admission) was
  0–1 ms in every arm.
- Pack: the number of SQLite pack reads.
- µs/B3: µs per BLAKE3 call, the CPU-speed indicator.

**128 KiB / 1 in flight, `async,noverify`**

| Arm | N | W | MB/s | × ctl | Wall ms | Busy ms (%) | Peak | Wait ms | Pack | µs/B3 | `VmHWM` KB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| b1-r128-anv-s1 (ctl) | 1 | 1 | 373.2 | — | 1,405 | 1,315 (93.6) | 2 | 1.0 | 2,298 | 13.3 | 104,196 |
| b1-r128-anv-s2 | 2 | 1 | 593.5 | 1.59 | 883 | 780 (88.3) | 3 | 18.8 | 2,383 | 13.8 | 109,836 |
| b1-r128-anv-s4 | 4 | 1 | 747.6 | 2.00 | 701 | 455 (64.8) | 5 | 12.5 | 2,501 | 14.0 | 125,896 |
| b4-r128-anv-s1-w1 (ctl) | 1 | 1 | 385.0 | — | 1,362 | 1,278 (93.8) | 2 | 0.9 | 2,298 | 13.4 | 104,388 |
| b4-r128-anv-s1-w2 | 1 | 2 | 384.2 | 1.00 | 1,364 | 1,285 (94.1) | 2 | 35.6 | 2,298 | 13.3 | 115,612 |
| b4-r128-anv-s2-w2 | 2 | 2 | 613.9 | 1.59 | 854 | 749 (87.7) | 4 | 41.6 | 2,383 | 13.8 | 191,632 |
| b4-r128-anv-s4-w1 | 4 | 1 | 765.2 | 1.99 | 685 | 442 (64.5) | 5 | 11.3 | 2,502 | 13.9 | 125,708 |
| b4-r128-anv-s4-w2 | 4 | 2 | **785.4** | 2.04 | 668 | 432 (64.7) | 8 | 48.2 | 2,503 | 13.9 | 149,972 |
| b4-r128-anv-s8-w1 | 8 | 1 | 642.5 | 1.67 | 816 | 356 (43.6) | 9 | 10.6 | 2,674 | 14.1 | 157,040 |
| b5-r128-anv-s1 (ctl) | 1 | 1 | 374.7 | — | 1,399 | 1,307 (93.4) | 2 | 0.6 | 2,298 | 13.4 | 105,040 |
| b5-r128-anv-s3 | 3 | 1 | 739.5 | 1.97 | 709 | 562 (79.3) | 4 | 16.3 | 2,448 | 14.2 | 118,292 |
| b5-r128-anv-s6 | 6 | 1 | 729.7 | 1.95 | 718 | 367 (51.1) | 7 | 11.0 | 2,560 | 14.0 | 142,892 |

**512 KiB / 4 in flight** (`-e LAYERFS_PROBE_FUSE_WINDOW=524288 -e
LAYERFS_PROBE_FUSE_BACKGROUND=4`; every receipt shows `max_write` 524288,
`max_background` 4)

| Arm | Read | N | W | MB/s | × ctl | Wall ms | Busy ms (%) | Peak | Wait ms | Pack | µs/B3 | `VmHWM` KB |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| b3-r512-snv-s1 (ctl) | single,noverify | 1 | — | 380.9 | — | 1,376 | 1,349 (98.0) | 2 | 674.1 | 2,806 | 13.3 | 98,060 |
| b3-r512-snv-s2 | single,noverify | 2 | — | 511.6 | 1.34 | 1,025 | 926 (90.4) | 4 | 534.2 | 3,675 | 13.9 | 120,408 |
| b3-r512-snv-s4 | single,noverify | 4 | — | 639.7 | 1.68 | 820 | 584 (71.3) | 8 | 552.9 | 4,687 | 14.2 | 152,588 |
| b3-r512-anv-s1 (ctl) | async,noverify | 1 | 1 | 382.3 | — | 1,371 | 1,277 (93.1) | 2 | 1.8 | 2,341 | 13.1 | 186,928 |
| b3-r512-anv-s2 | async,noverify | 2 | 1 | 562.7 | 1.47 | 932 | 797 (85.5) | 3 | 0.4 | 2,498 | 13.5 | 113,388 |
| b3-r512-anv-s4 | async,noverify | 4 | 1 | 758.5 | 1.98 | 691 | 540 (78.2) | 4 | 0.2 | 2,715 | 14.4 | 193,804 |
| b5-r512-anv-s1 (ctl) | async,noverify | 1 | 1 | 357.9 | — | 1,465 | 1,353 (92.3) | 2 | 1.7 | 2,355 | 13.3 | 187,620 |
| b5-r512-anv-s3 | async,noverify | 3 | 1 | 726.0 | 2.03 | 722 | 585 (81.0) | 4 | 15.9 | 2,579 | 13.6 | 166,580 |
| b5-r512-anv-s4-w1 | async,noverify | 4 | 1 | 856.2 | 2.39 | 612 | 479 (78.2) | 5 | 11.4 | 2,675 | 13.7 | 190,676 |
| b5-r512-anv-s2-w2 | async,noverify | 2 | 2 | 606.2 | 1.69 | 865 | 840 (97.1) | 4 | 693.2 | 2,481 | 14.0 | 207,388 |
| b5-r512-anv-s4-w2 | async,noverify | 4 | 2 | **928.2** | 2.59 | 565 | 503 (89.0) | 8 | 669.1 | 2,725 | 14.0 | **300,928** |
| b5-r512-anv-s4-w2-arena2 | + `MALLOC_ARENA_MAX=2` | 4 | 2 | 751.4 | 2.10 | 698 | 623 (89.3) | 8 | 1,008.2 | 2,749 | 14.2 | 166,696 |

**Mislabelled arms (b2).** The b2 arms were meant to run at 512 KiB / 4.
The profile flags went into one shell variable that zsh did not split, so
every b2 receipt shows `max_write` 131072 and `max_background` 1. They ran
at 128 KiB / 1:

- `b2-r512-anv-s{1,2,4}` (379.0 / 590.4 / 696.6 MB/s) repeat the b1
  settings on the same binary. They were not intended, are not used as
  results, and are kept only for custody.
- `b2-r512-snv-s{1,2,4}` measured 128 KiB / 1 `single,noverify`, which was
  not otherwise measured: 290.0 / 420.7 / 512.7 MB/s (1.45× and 1.77×).

b3 reran the intended 512 KiB arms with explicit flags.

## What the numbers say

- **The hypothesis holds.** N=4 doubles the cold read at either profile,
  within its batch:
  - 128 KiB / 1, `async,noverify`: 373 → 748 and 385 → 765 MB/s.
  - 512 KiB / 4, `async,noverify`: 382 → 759 and 358 → 856 MB/s.
  - 512 KiB / 4, `single,noverify`: 381 → 640 MB/s (1.68×).
- **Scaling at 128 KiB / 1:**

  | N | 1 | 2 | 3 | 4 | 6 | 8 |
  | --- | ---: | ---: | ---: | ---: | ---: | ---: |
  | Gain | 1.0 | 1.59 | 1.97 | 1.99–2.00 | 1.95 | 1.67 |

  The gain flattens at N=3–4. At N=8 the read is slower: pack reads cost
  1,254 ms summed against 834 at N=4, and reply copies 144 ms against 79,
  on 8 vCPUs shared with the FUSE loops, the read-ahead thread and the
  probe's reader.
- **The next limit at 128 KiB / 1 is the serial READ path, not the Store.**
  At N=4 the Store is busy only 65% of the wall time (it was 94%). Wall is
  685 ms. Of that, READ callbacks are 310 ms (base-read waits for in-flight
  objects only 104 ms). The other ~375 ms is outside the callback: one
  kernel round trip per 128 KiB READ (4,000 of them), the user copy-out and
  the probe's own fold.
- **At 512 KiB / 4, READs still wait for the fetch.** At N=4, W=1, READ
  callbacks sum to 678–804 ms, mostly client demands waiting for in-flight
  objects. A second window in flight (W=2) gives the best number measured,
  928 MB/s, +8% over W=1 in the same batch. It raises peak concurrency to 8
  demands on 4 lanes (669 ms of lock waits) and `VmHWM` to 301 MB.
- **Memory:**
  - At 128 KiB / 1, `VmHWM` rises 104 → 110 / 118 / 126 / 143 / 157 MB for
    N = 2 / 3 / 4 / 6 / 8.
  - At 512 KiB / 4, `async` was already 187 MB at N=1 (round 2: 185 MB,
    undiagnosed), and W=2 at N=4 is 301 MB.
  - With `MALLOC_ARENA_MAX=2` the same arm peaks at 167 MB but reads only
    751 MB/s. So most of that peak is glibc per-thread arenas keeping freed
    window buffers resident, not more live data. The logical bounds (window,
    frontier, cache) are unchanged.
- **Duplicate pack reads, not duplicate objects.** Each lane keeps its own
  pack cache, so a pack that straddles a part boundary is read by two lanes.
  - At 128 KiB / 1, pack reads rise from 2,298 to 2,502 at N=4 (+9%) and
    2,674 at N=8.
  - At 512 KiB / 4 `single`, they rise from 2,806 to 4,687, because many
    small concurrent demands spread over lanes.
  - Pack SHA-256 calls stay about 2,250–2,390 in every arm.

## Risks and gaps

- **Correctness:** checked only by the probe's whole-file fold. Not run:
  - concurrent writers during a split fetch
  - unmount while a part thread runs
  - a part failing while others succeed (the code joins every part and
    fails the window)
- **Lanes only on the new-open path.** A reused (pooled/shared, PostgreSQL)
  reader gets no lanes. With PostgreSQL each lane would be one more server
  connection per mounted Workspace, which the `IDLE_READERS` budget does not
  count.
- **Threads:** N−1 threads are spawned per window (63 windows per 500 MiB).
  A product version would use a fixed per-Workspace or per-daemon pool,
  which would also cap the number of allocator arenas.
- **Not run:**
  - Clippy: `NOT_RUN`, because the build image has no clippy component.
  - Unit and mount tests: `NOT_RUN`. Only the probe binary was built
    (`--release --locked --offline`).
  - Durable: `NOT_RUN`, disabled by the owner until explicit
    reauthorization.
- **One sample per arm.** Controls varied 373–385 MB/s at 128 KiB and
  358–382 MB/s at 512 KiB across batches, so compare only within a batch.
  Guest 1-minute load at arm start was 1.5–4.3, with other experimenters'
  idle containers present. The heavy lock serialised every run.

## Recommendation

1. **Adopt parallel Store sessions with N = 4 (or the vCPU count / 2).**
   Split each read-ahead window into contiguous parts. This doubles the
   cold read at either profile with no change to the read bounds, no
   duplicate objects and about +20 MB peak memory at 128 KiB / 1.
2. Before a product version:
   - use a fixed fetch pool, not threads spawned per window
   - make lanes share one pack cache, or split on pack boundaries, to
     avoid duplicate pack reads
   - decide how lanes count against PostgreSQL connections
3. **Next limit:**
   - At 128 KiB / 1 it is the per-READ kernel round trip. Pair N=4 with the
     512 KiB / 4 profile, which reached 856 MB/s at W=1.
   - A second window in flight (W=2) adds about 8% at 512 KiB / 4 but costs
     about 110 MB of peak memory (allocator arenas). Do not adopt it
     without a bounded thread pool.

Receipts: [`../receipts/exp-parallel-store/`](../receipts/exp-parallel-store/).
