# Experiment `lanes2`: Store read lanes, round 4

> **Status:** Diagnostic prototype on branch `exp/lanes2` (base
> `c8cf56ab0e`, issue #684). Not a product contract. One sample per arm.
> Times are diagnostic; counts are exact. Disposable Store; kernel
> write-back off; fuser not patched.

## Goal and hypotheses

Make the cold read faster than the round-3 safe set (937 MB/s at 512K/4).
In the round-3 full arm the four lanes were busy 420 of 500 ms. Lanes
re-read about 10% of packs, and each window spawned its part threads.

1. **Shared packs.** Lanes share whole edge pack bodies, so no pack is read
   twice.
2. **Fixed pool.** A fixed pool of fetch threads per Workspace replaces the
   threads spawned per window.
3. **Deferred check.** The resolver's BLAKE3 check moves out of the lane
   lock but stays in the Store layer.
4. **Lane count.** The best N with these changes, and its CPU cost.

## Change (all off by default, each marked `Prototype (lanes2 …)`)

- **`LAYERFS_PROBE_LANES=sharepacks[,edgewhole]`**
  - Each Workspace session holds one `SharedPacks` map (`layerfs-storage`
    `read/shared.rs`), bounded at 4 MiB, oldest out first.
  - When a lane reads the first or last pack of a demand whole, it keeps the
    body there. That body has already passed the pack SHA-256 and frame
    checks.
  - Another lane that needs the pack copies it from the map and checks its
    frame again, as for a `published_body` hit.
  - `edgewhole` makes a lane read its edge packs whole.
- **`LAYERFS_PROBE_READ_POOL=M`** (1 to 16)
  - The Workspace's `ReadSession` starts M threads at its first window
    (`layerfs-workspace` `base/pool.rs`) and drops them with the session.
  - Each window's other parts run as jobs on the pool. Each part keeps its
    own fresh custody. Every part is waited for, and a part with no reply
    fails the window.
  - **`LAYERFS_PROBE_READ_PARTS=K`** sets the parts per window. The default
    is `READ_SESSIONS`.
- **`LAYERFS_PROBE_LANES=defer`**
  - `Resolver::deferring` skips the BLAKE3 check of the requested object of
    an ordinary-lane chain and marks it as deferred.
  - `ReadSession::objects_deferred` checks every marked object against the
    id it was requested by, after the lane lock is released and before
    returning. A mismatch fails the demand with
    `Integrity("dependency identity")`.
  - Dependencies, pooled leaves and program pages are still checked inside
    the lane. No consumer, cache or other caller sees unchecked bytes, so
    the provider contract is unchanged. The control reader (session 0)
    never defers.
- **`LAYERFS_PROBE_LANES=anylane`:** a demand that finds every lane busy
  waits on a condvar for the first lane freed, instead of the next lane in
  turn.
- **Probe additions:**
  - Cold-read CPU time (`getrusage` user and system).
  - `shared_hits` and `shared_kept`.
  - The stages `window_fetch` (a split window's wall time), `part_idle`
    (each part's wait at the window barrier) and `deferred_hash`.
  - From `680c966f` on, `steal_ticks` (always 0 on this VM) and `threads`.

The existing `READ_SESSIONS=N` lanes and `READ_WINDOWS=W` were reused. All
bounds are unchanged: 8 MiB window, two-window frontier, 64 MiB cache and
`DEMAND_BYTES`.

## Binaries

| SHA-256 | Source | Batches |
| --- | --- | --- |
| `12a9234728f39c8b635502eb50a570f91b7b99e4b5644ff379a23cb737589b2f` | working tree before the window stages: same switches, `fetch_pooled` still in `client.rs` | b1–b4 |
| `cfbc7e10765335b925b3a2a05a8fc53caf83f3f4cb38d010e3d82ea33d836866` | commit `94f91d3a3b` | b5 |
| `680c966f581650870f1753cbf801cadfcd30081f4fb82062f11de6d9d4942c31` | `94f91d3a3b` + probe `steal_ticks`/`threads` (commit after it) | b6, b7 |

All were built release, `--locked --offline -j 6`, in `rust:1.85.1-bookworm`.

## Arms

**Settings and checks:**

- Every arm runs the round-3 safe set at 512 KiB / 4 unless marked 128K.
  Every receipt shows `max_write` 524288 and `max_background` 4.
- Every arm exited 0.
- Every arm read the 500 MiB back against the source fold.
- `store_resident_before` was 0 in every arm.
- Every arm demanded 27,214 objects and passed both coherence checks.
- 60 arms in total.

**Columns:**

- N = lanes, W = windows in flight, Pool = pool threads.
- CPU = process user + system time over the cold read.
- Busy = Store-demand union time.

**Disturbance:**

- An arm is marked **DISTURBED** when its BLAKE3 µs per call is more than
  10% above its batch control's. The host had a load of 6–8 from other
  processes, and the VM's CPU speed varied by up to 2.4×.
- In those arms CPU time is inflated too.
- Batches 4 and 6 are disturbed as a whole and are not used for conclusions.

**Clean batch b5** (binary `cfbc7e10`; host load 2.4–3.5 inside the VM):

| Arm | N | W | Pool | `LANES` | MB/s | × ctl | Wall ms | CPU ms | Busy ms | Pack reads | Blob MB | µs/B3 | `VmHWM` KB |
| --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| b5-ctl | 4 | 1 | — | — | 923.7 | 1.00 | 568 | 1,809 | 486 | 2,747 | 567 | 13.1 | 132,276 |
| b5-s6 | 6 | 1 | — | — | 1114.8 | 1.21 | 470 | 1,873 | 392 | 2,852 | 576 | 13.3 | 149,336 |
| b5-s6-pool5 | 6 | 1 | 5 | — | 1105.5 | 1.20 | 474 | 1,814 | 403 | 2,949 | 585 | 13.2 | 148,068 |
| b5-s6-defer-pool5 | 6 | 1 | 5 | defer | 1128.9 | 1.22 | 464 | 1,812 | 396 | 2,930 | 581 | 12.6 | 148,452 |
| b5-s6-share | 6 | 1 | — | sharepacks | 1092.8 | 1.18 | 480 | 1,856 | 399 | 2,659 | 555 | 13.2 | 162,736 |
| b5-s6-w2 | 6 | 2 | — | — | 1214.1 | 1.31 | 432 | 1,905 | 398 | 2,939 | 586 | 13.8 | 193,628 |
| b5-s6-w2-pool5 | 6 | 2 | 5 | — | 1226.2 | 1.33 | 428 | 1,864 | 390 | 2,942 | 586 | 13.5 | 168,876 |
| **b5-s6-w2-pool5-defer** | 6 | 2 | 5 | defer | **1228.1** | **1.33** | 427 | 1,833 | 378 | 2,877 | 578 | 12.9 | 158,676 |
| b5-s8-w2-pool7-defer | 8 | 2 | 7 | defer | 1201.5 | 1.30 | 436 | 1,959 | 371 | 3,366 | 619 | 13.1 | 178,516 |
| b5-s4-w2-pool3 | 4 | 2 | 3 | — | 1135.3 | 1.23 | 462 | 1,675 | 446 | 2,644 | 559 | 13.3 | 152,884 |
| b5-s6-w2-pool5-defer-any | 6 | 2 | 5 | defer,anylane | 668.6 | DISTURBED | 784 | 3,116 | 736 | 2,888 | 584 | 21.7 | 165,968 |
| b5-s6-w2-pool10-defer | 6 | 2 | 10 | defer | 533.1 | DISTURBED | 984 | 4,274 | 897 | 2,928 | 586 | 30.8 | 185,704 |
| b5-s6-w2-pool3-defer | 6 | 2 | 3 | defer | 421.2 | DISTURBED | 1245 | 4,277 | 1217 | 2,922 | 580 | 31.1 | 159,744 |

The b5 stage split for the control against the best arm:

| Stage | b5-ctl | b5-s6-w2-pool5-defer |
| --- | ---: | ---: |
| Windows (count / ms) | 140 / 474 | 122 / 497 |
| `part_idle` (count / ms summed) | 550 / 198 | 729 / 433 |
| Locate (demands / ms) | 652 / 287 | 825 / 380 |
| Pack read (count / ms) | 2,747 / 905 | 2,877 / 1,045 |
| Lane-held BLAKE3 | 357 ms | 0 (`deferred_hash` 355 ms, outside the lane) |
| READ waits (`client_demand`) | 414 ms | 135 ms |

**128 KiB / 1, same binary, run right after b5:**

| Arm | N | W | Pool | `LANES` | MB/s | × ctl | Wall ms | CPU ms | Busy ms | Pack reads | Blob MB | µs/B3 | `VmHWM` KB |
| --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| b5-128-ctl | 4 | 1 | — | — | 800.2 | 1.00 | 655 | 1,774 | 467 | 2,499 | 548 | 13.4 | 127,844 |
| b5-128-s6-w2-pool5-defer | 6 | 2 | 5 | defer | 770.2 | 0.96 | 681 | 1,949 | 400 | 2,558 | 556 | 14.5 | 149,480 |

**Clean batches b2, b3** (binary `12a92347`):

| Arm | N | W | Pool | `LANES` | MB/s | × ctl | Wall ms | CPU ms | Busy ms | Pack reads | Blob MB | µs/B3 | `VmHWM` KB |
| --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| b2-ctl | 4 | 1 | — | — | 946.5 | 1.00 | 554 | 1,761 | 476 | 2,727 | 564 | 13.1 | 130,744 |
| b2-s4-best | 4 | 1 | 3 | sharepacks,edgewhole,defer | 914.9 | 0.97 | 573 | 1,748 | 501 | 2,281 | 543 | 12.5 | 141,388 |
| b2-s4-w2 | 4 | 2 | — | — | 1027.1 | 1.09 | 510 | 1,734 | 491 | 2,676 | 563 | 13.5 | 166,980 |
| b2-s6 | 6 | 1 | — | — | 1099.2 | 1.16 | 477 | 1,873 | 394 | 2,868 | 578 | 13.2 | 149,068 |
| b2-s6-pool5 | 6 | 1 | 5 | — | 1110.7 | 1.17 | 472 | 1,803 | 400 | 2,924 | 582 | 13.3 | 148,412 |
| b2-s6-best | 6 | 1 | 5 | sharepacks,edgewhole,defer | 964.5 | 1.02 | 544 | 1,749 | 476 | 2,326 | 554 | 12.6 | 165,036 |
| b2-s8 | 8 | 1 | — | — | 1090.3 | 1.15 | 481 | 1,940 | 398 | 3,124 | 599 | 13.3 | 164,516 |
| b2-s8-pool7 | 8 | 1 | 7 | — | 1097.5 | 1.16 | 478 | 1,920 | 403 | 3,174 | 599 | 13.4 | 162,916 |
| b3-ctl | 4 | 1 | — | — | 953.1 | 1.00 | 550 | 1,706 | 474 | 2,710 | 568 | 13.2 | 130,380 |
| b3-s6 | 6 | 1 | — | — | 1158.3 | 1.22 | 453 | 1,789 | 381 | 2,931 | 585 | 12.9 | 149,388 |
| b3-s6-pool5 | 6 | 1 | 5 | — | 1108.5 | 1.16 | 473 | 1,815 | 400 | 3,004 | 586 | 13.3 | 148,092 |
| b3-s6-share | 6 | 1 | — | sharepacks | 1100.9 | 1.16 | 476 | 1,849 | 400 | 2,573 | 550 | 13.2 | 160,932 |
| b3-s6-share-ew | 6 | 1 | — | sharepacks,edgewhole | 1067.0 | 1.12 | 491 | 1,860 | 412 | 2,289 | 545 | 13.3 | 164,240 |
| b3-s6-defer | 6 | 1 | — | defer | 1125.3 | 1.18 | 466 | 1,825 | 388 | 2,901 | 580 | 12.6 | 148,196 |
| b3-s6-defer-pool5 | 6 | 1 | 5 | defer | 1140.7 | 1.20 | 460 | 1,804 | 392 | 2,906 | 581 | 12.6 | 149,232 |

**Custody note: two settings sampled twice on one binary.** `b3-s6` and
`b3-s6-pool5` repeat the settings of `b2-s6` and `b2-s6-pool5` on the same
binary. That breaks the one-sample rule. They were run to give batch 3 an
N=6 reference. Both samples of each are listed, and neither is chosen over
the other. Their spread (1,099 against 1,158; 1,111 against 1,109) shows the
single-sample noise at N=6: about ±3%.

**Batch b1** (binary `12a92347`, N=4, all clean at 12.5–13.2 µs):

| Arm | MB/s | × ctl | CPU ms | Pack reads | Blob MB | `VmHWM` KB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| ctl | 930.1 | 1.00 | 1,792 | 2,740 | 566 | 131,016 |
| sharepacks | 949.8 | 1.02 | 1,716 | 2,442 | 540 | 140,392 |
| sharepacks,edgewhole | 943.8 | 1.01 | 1,692 | 2,262 | 538 | 140,640 |
| pool 3 | 970.0 | 1.04 | 1,673 | 2,713 | 567 | 131,876 |
| defer | 968.1 | 1.04 | 1,698 | 2,673 | 562 | 130,632 |
| defer,anylane, pool 5, 6 parts | 961.6 | 1.03 | 1,681 | 2,895 | 584 | 136,048 |
| all four, pool 5, 6 parts | 930.7 | 1.00 | 1,690 | 2,399 | 570 | 144,784 |

**Batch b7** (binary `680c966f`; control 953.2 MB/s, 13.5 µs):

- `s6-w2-pool5-nodefer-any`: **1,276.4** MB/s (1.34×), 13.6 µs, CPU 1,793 ms,
  `VmHWM` 157,768 KB.
- `s6-w2-pool5-defer-share`: 1,262.1 MB/s (1.32×), 12.9 µs, `VmHWM`
  172,204 KB.
- `s6-w2-pool11-k12-defer`: 1,148.0 MB/s (1.20×), 14.5 µs. It made 1,794
  demands and waited 1,088 ms on lane locks.
- `s5-w2-pool4-defer` (18.3 µs) and `s6-w2-pool8-defer` (17.0 µs) are
  DISTURBED.

**Batches b4 and b6:** disturbed as a whole (µs per call 15–31, varying
within each batch). They are kept in the receipts only. In b6 the control
read 386.9 MB/s at 24.9 µs.

## What the numbers say

- **H1, shared packs: refuted.**
  - Sharing removes the duplicate reads: at N=6, 2,931 → 2,573 reads, and
    2,289 with `edgewhole` (fewer than round-3 ref's 2,356). Blob bytes fall
    by 30–40 MB.
  - Pack-read time does not fall: 998 → 1,023 / 985 ms in b3, and 1,032 →
    1,028 ms in b5. The duplicate reads were guest page-cache hits, because
    the first lane had just faulted those pages in, so they cost almost
    nothing.
  - Sharing adds about 200–300 MB of body copies and +11–15 MB `VmHWM`. It
    reads 1–8% slower at W=1 (`edgewhole` −8% in b3).
  - At W=2 (b7) it is within noise of the best arm.
- **H2, fixed pool: neutral on speed, bounds memory at W=2.**
  - At W=1 the pool matches spawned threads: 1,106–1,111 against
    1,099–1,158 MB/s, with `VmHWM` 148 against 149 MB.
  - At W=2 the pool keeps the speed (1,226 against 1,214) and cuts `VmHWM`
    from 194 to 169 MB: 159 MB with `defer`, and 158 MB with `anylane` in
    b7. It also uses 40–70 ms less CPU.
  - The thread count is fixed: 11 threads with the pool, against 6 at the
    end of a spawn arm.
- **H3, deferred check: integrity identical, speed neutral.**
  - Lane-held BLAKE3 falls from 357 ms to 0. `deferred_hash` (355 ms) now
    runs outside the lane.
  - Speed: +1–4% at W=1 (b1 +4% at N=4, b5 +2% at N=6) and 0% at W=2.
  - Once there are 6 lanes they are not the scarce resource, so freeing
    them early buys little. It would matter if lanes were fewer than
    threads, for example under a PostgreSQL connection limit.
- **H4, lane count: N=6 with a second window in flight is the best
  measured.**
  - At W=1, N=6 adds 16–22% over N=4 in three clean batches. This differs
    from round 3 at 128K/1, where N=6 ≈ N=4. N=8 is no better than N=6
    (1,090–1,098).
  - At W=2 with the pool, in b5:

    | N | MB/s | CPU ms | `VmHWM` MB |
    | ---: | ---: | ---: | ---: |
    | 4 | 1,135 | 1,675 | 153 |
    | 6 | 1,226–1,228 | 1,833–1,864 | 159–169 |
    | 8 | 1,202 | 1,959 | 179 |

  - N=6 costs only 1–3% more CPU than the control (1,809 ms) for 25% less
    wall time. At W=1, N=6 costs +3.5% CPU.
- **Why the second window matters.**
  - At W=1 the window barrier left 13% of lane time idle (`part_idle`
    298 ms summed against 380 ms of window wall in b5-s6), and READs
    waited 414 ms (control) and 167 ms (N=6).
  - W=2 overlaps that barrier and cuts READ waits to 135 ms.
  - More parts than lanes (12 parts on 6 lanes) is worse: lane-lock waits
    reach 1,088 ms.
- **128K/1:** no gain (800 → 770 MB/s). The per-READ kernel round trip
  bounds this profile, as round 3 found.
- **`anylane`:** one clean sample, the best measured (1,276 MB/s). Its two
  other samples are disturbed, so it is inconclusive.

## Risks and gaps

- **Correctness:**
  - Checked only by the whole-file fold and the coherence checks. No
    corruption-injection test shows that a deferred mismatch is caught; that
    comes from reading the source.
  - Not run: concurrent writers, a part failing, unmount while a pool job
    runs.
- **`PROBE_AHEAD_FAILED` after unmount.** Three W=2 pool arms printed
  `PROBE_AHEAD_FAILED ProviderFailure { "Store reader stopped" }` in the
  re-read phase: `b4-s4-w2-pool3`, `b4-s6-w2-pool5` and `b5-s6-w2-pool5`.
  - A background window whose parts were still queued on the pool ran
    after unmount had stopped the reader. By design that failure fails
    nothing, and every check passed.
  - A product pool must drain or refuse its queued jobs at unmount.
- **Memory:**
  - The pool keeps M threads per mounted Workspace for its lifetime, idle
    or not.
  - Lanes add one SQLite connection each: 6 lanes against 4. For
    PostgreSQL, the connection budget question from parallel-store stands.
- **Switch scope:** `defer` and `sharepacks` apply only to sessions opened
  with lanes; the control reader never defers.
- **Not run:**
  - Clippy: `NOT_RUN`, the image has no clippy.
  - Unit and mount tests: `NOT_RUN`.
  - Durable: `NOT_RUN`, disabled by the owner until explicit
    reauthorization.
  - Combining with `nopacksha`/split locate (the full set): `NOT_RUN`.
- **Host disturbance:**
  - Two whole batches and five other arms were disturbed: host load 6–8,
    and the VM reports no steal time.
  - CPU time in disturbed arms is inflated and is not used.

## Recommendation

1. **Adopt N=6 lanes with two windows in flight, on a fixed per-Workspace
   pool of N−1 threads.**
   - 1,226–1,276 MB/s against the safe set's 924–953 MB/s in the same
     batch: **+31–34%**.
   - CPU about +1–3%. `VmHWM` +27–37 MB over the control, against +62 MB
     with spawned threads.
   - It needs:
     - a pool that drains its queued jobs at unmount
     - a decision on lanes against PostgreSQL connections
2. **Keep `defer` as the way to place the one BLAKE3 check, if the owner
   picks the resolver.** It keeps the provider contract and takes the hash
   out of the lane at no speed cost. It is not a speed lever at N=6.
3. **Drop shared packs (`sharepacks`, `edgewhole`).** Duplicate pack reads
   are page-cache hits, and sharing only adds copies.
4. **Next limits at the best arm:**
   - pack reads: 51% of Store work, SQLite blob I/O
   - locate: 19%, per demand
   - BLAKE3: 17%
   - At W=2 the Store is still busy 378 of 427 ms. The 16 KiB pages and
     split locate from `sqlite-read` attack the first two.

Receipts: [`../receipts/exp-lanes2/`](../receipts/exp-lanes2/), one log per arm
(60).
