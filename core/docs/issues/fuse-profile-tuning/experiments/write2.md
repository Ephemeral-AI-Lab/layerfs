# Experiment `write2`: CPU cost of `digestpar` and `spin`, and cheaper alternatives

> **Status:** Diagnostic research for #684 on branch `exp/write2` (base
> `c8cf56ab0e`). Not a product contract and not for merge. Disposable Store,
> kernel write-back off, fuser not patched. One sample per arm; counts are
> exact, times are diagnostic. Follows [write-path](write-path.md).

## Questions

1. What do `digestpar` (BLAKE3 append digest on a helper thread) and `spin`
   (50 µs lock spin) cost in CPU, alone and with four Workspaces writing at
   once?
2. Why does the RELEASE owner job block the next create's GETATTR, and can a
   shorter RELEASE replace `spin` without burning CPU?
3. Can group commit help 4 KiB random writes?

## Answers

- **`digestpar` costs 5–17% more CPU per MB for +19–30% single-stream
  sequential write**, and the helper keeps a second core about 35% busy
  (CPU per wall second 0.98 → 1.32–1.36).
- **`digestpar` as built makes four concurrent Workspaces 21–23% slower**
  (2,191–2,282 → 1,711–1,725 MB/s). Its one process-wide helper serializes
  every Workspace's digests. The round-3 safe set (`ctrl`) has this
  regression.
- **`digestidle` fixes that.** The helper takes a window only while it is
  idle; otherwise the job hashes inline. It keeps the single-stream gain
  (+19–26%) and is 3–11% *faster* than plain under four Workspaces
  (2,435 against 2,191 MB/s in batch C).
- **`spin` has no measurable CPU cost here.** CPU per create stayed within
  ±3% of plain in every batch. Its bound is the lock wait it turns into
  spinning: about 8–9 µs per create, about 5% of 176 µs of CPU.
- **`reldefer` replaces `spin` without spinning.**
  - The RELEASE job of a freshly written file finishes the file's followed
    digest (#364): a `SELECT` of the latest row, then `BEGIN`, an
    `fs_digest` upsert and `COMMIT`. That is 8.6 µs under the Workspace
    lock, and the next file's GETATTR arrives during it.
  - With `reldefer`, the RELEASE keeps the finished digest in the journaled
    open state. The next job that is not light writes it first, in its own
    transaction.
  - RELEASE hold falls from 8.6 to 2.0 µs, and the GETATTR callback from
    31.0 to 13.8 µs (`spin`: 18.4). Lock waits over 5 µs fall from 1,793
    (36 ms) to 41 (0.8 ms). Statements per create fall from 20 to 18, and
    CPU per create from 182 to 168 µs (−7%).
- **Group commit was not built.** A single 4 KiB random writer never
  queues: one WRITE is in flight, with 0.6 µs of lock wait per write. So
  there is nothing to group in the probe's random phase. Four writers in one
  Workspace bound the gain at about 5% (§4).

## Change

Commit `decb5578e4`. All switches are off by default.

**Probe** (`fuse_profile_probe.rs`):

- Each write phase and the cold read record `cpu`: process user+system
  time from `getrusage`, CPU per wall second, CPU per MB or per file, and
  the `layerfs-digest` helper thread's run time from its `schedstat`.
- `callbacks` per phase: FUSE callback counts and wall time by opcode,
  summed over all mounts (new process-wide counters in
  `layerfs-fuse/src/request/accounting.rs`). They are read before the
  read-back, so the read-back is not in them. The per-mount `kernel` receipt
  still includes the read-back.
- **Read-back after every write phase**, through the same mount with the
  kernel pages dropped (`POSIX_FADV_DONTNEED`; with write-back off every
  written page is clean):
  - sequential: every 1 MiB chunk
  - random: every distinct written block, plus about 1,000 sampled unwritten
    blocks against the native copy
  - creates: every file
- **Contention phases** (four threads, one barrier, wall and CPU over the
  whole set):
  - `contend_seq`: 4 Workspaces × 64 MiB.
  - `contend_create`: 4 Workspaces × 500 files of 1 KiB.
  - `contend_random_4k`: 4 Workspaces × 4,096 writes into `rand`.
  - `shared_random_4k`: one Workspace, 4 writers, each into its own new
    64 MiB file. Writers to one file serialize on the kernel inode lock
    (shakedown `b1-ctrl`: 64.5 MB/s, the same as one writer).
- **`digest_commit`:** 120 files written through the mount by appends.
  Every fourth file is closed after 200 KiB and reopened to append the
  rest. Then one Commit, and a fresh mount reads every file back.
  - The Store must find each chunked file's root by the true BLAKE3 of its
    bytes (`file_roots`), so a wrong or lost digest fails the phase.
  - New stages `captured_asked` and `captured_found` count the Commit's
    digest lookups.

**Switches** in `LAYERFS_PROBE_WRITE` (comma list; the earlier `noresume`,
`digestpar`, `digestcheck` and `spin` are unchanged):

| Switch | What it does |
| --- | --- |
| `reldefer` | A kernel RELEASE, and an observe visit (LOOKUP, GETATTR, OPEN), is a light job. A light close moves the file's finished digest state from `digests` into a new journaled map, `closed`, with no SQL. Every job that is not light writes the waiting digests first, inside its own transaction, with the original close logic (latest-row check, `fs_digest` upsert, resumable slot). `captured_digests` writes them before it answers. At most 64 wait; a light close that reaches 64 writes them all itself. |
| `digestidle` | With `digestpar`: a window goes to the helper only if it is idle (`HELPER_BUSY`); otherwise it is hashed inline. |

**Why `reldefer` keeps the contract.**

- Light jobs read no `fs_digest` row and write no inode row. So when the
  next non-light job writes the digest, the latest-row check sees exactly
  what the RELEASE would have seen.
- The digest lands in the same row with the same sequence. The only
  difference is that it commits in a later transaction.
- If that job fails, rollback restores both the SQL and the `closed`
  entries, because `closed` is journaled like `digests`.
- A Commit's capture is a non-light job, and `captured_digests` writes any
  that are left, so a Commit sees the same digests.
- Terminal unmount deletes the overlay file, so a waiting entry is dropped
  with the rest of the open state. Nothing outlives the Workspace.
- Unmount drained in every arm (the harness asserts `Drained`).

## Binaries

| SHA-256 | Source | Arms |
| --- | --- | --- |
| `c5841b57b206ac13fccf1446ad6f6d0528d1be12d16eabfbefe86d7f24164925` | shakedown; `shared_random_4k` wrote one file | b1-ctrl |
| `fa72dd0e350e465791d6a39983f51228dba63ef8a42d6cea686d8ace55c0ed4a` | + four files in shared; no `callbacks` (per-mount `kernel` includes the read-back) | batch A |
| `96a30deff8b8011ef851f87b385776ee30026515474b4ef2963221970a3e831c` | + process callbacks, `digestidle` | batch B |
| `27578c121093795883e101dcd3c26d69f8ae76aee93fe01305d483ff6c3e55a4` | `decb5578e4` (+ `digest_commit`) | batch C (final) |

**Every arm** uses the round-3 safe set. Only `LAYERFS_PROBE_WRITE`
changes; it always includes `noresume`:

- `ctrl`: `digestpar,spin`, the round-3 safe set
- `plain`: nothing more
- `dp`: `digestpar`
- `dpidle`: `digestpar,digestidle`
- `rd`: `reldefer`

**Every one of the 21 arms** exited 0, verified every written byte, read the
500 MiB back with the right fold, and passed both coherence checks. In
batch C, `digest_commit` passed in every arm: 90 of 90 chunked roots were
found by their true digest, and 120 of 120 captured digests were found.
`c-rd-dpidle-check` compared 1,647 helper results with inline hashing; none
differed.

**CPU speed:** BLAKE3 was 13.1–13.4 µs per call in batches A and C. In
batch B it drifted from 18.1 µs (`ctrl`) to 14.0 µs (`rd`, `spin`) within
the batch, so compare batch B with care.

## Results

### 1. Sequential write: 256 MiB, 1 MiB writes, 512 KiB / 4

| Arm | B3 µs | MB/s | CPU µs/MB | CPU/wall | Helper ms | 4 WS MB/s | 4 WS CPU µs/MB | 4 WS CPU/wall |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| a-plain | 13.2 | 724.7 | 1,347 | 0.98 | 0 | 2,233.7 | 1,701 | 3.80 |
| a-dp | 13.3 | **884.6** | 1,520 | 1.34 | 162.5 | **1,724.7** | 1,665 | 2.87 |
| a-ctrl | 13.3 | 759.4 | 1,750 | 1.33 | 168.6 | 1,701.8 | 1,626 | 2.77 |
| b-plain | 16.9 | 671.8 | 1,456 | 0.98 | 0 | 2,281.7 | 1,669 | 3.81 |
| b-dpidle | 15.0 | **801.5** | 1,703 | 1.36 | 181.5 | **2,344.7** | 1,729 | 4.05 |
| b-ctrl | 18.1 | 674.7 | 2,071 | 1.40 | 204.9 | 1,682.2 | 1,707 | 2.87 |
| c-plain | 13.1 | 710.6 | 1,377 | 0.98 | 0 | 2,191.0 | 1,655 | 3.63 |
| c-dp | 13.0 | **926.3** | 1,447 | 1.34 | 161.7 | **1,722.5** | 1,614 | 2.78 |
| c-dpidle | 13.0 | **892.5** | 1,513 | 1.35 | 164.4 | **2,435.4** | 1,712 | 4.17 |
| c-ctrl | 13.1 | 889.6 | 1,487 | 1.32 | 162.2 | 1,711.7 | 1,659 | 2.84 |
| c-rd-dpidle | 13.1 | 929.8 | 1,456 | 1.35 | 162.7 | 2,483.0 | 1,694 | 4.21 |

"4 WS" is `contend_seq`: 4 × 64 MiB at once.

- **Hashing on the helper costs only 5% more:** 156.6 ms on the helper
  against 148.9 ms inline (a-dp, a-plain).
- **The extra CPU comes from elsewhere:** cell writes and COMMIT run 10–15%
  slower while the helper hashes (cells 49 → 56 ms, COMMIT 64 → 71 ms),
  and there are the hand-off wake-ups (system time +12 ms).
- **Under four Workspaces with `dpidle`,** the helper hashed 69 ms of the
  run; the rest was hashed inline on the receive threads.

### 2. Creates: 2,000 files of 1 KiB; batch C, one Workspace

Rows give µs per file. "Sync callbacks" is LOOKUP + CREATE + WRITE +
GETATTR, the requests the writing process waits for. RELEASE is
asynchronous.

| Arm | Wall | CPU | GETATTR cb | RELEASE cb | RELEASE job hold | CREATE cb | Sync callbacks | All callbacks | Lock wait | Waits > 5 µs | SQL statements |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| c-plain | 228.1 | 181.7 | 31.0 | 17.9 | 8.6 | 29.7 | 105.5 | 123.5 | 20.9 | 1,793 / 36.4 ms | 20 |
| c-spin | 213.6 | 175.9 | 18.4 | 14.2 | 8.7 | 29.5 | 92.5 | 106.8 | 9.0 | 1,452 / 9.9 ms | 20 |
| c-ctrl | 210.8 | 178.1 | 18.3 | 14.3 | 8.9 | 29.8 | 92.9 | 107.2 | 8.4 | 1,338 / 9.6 ms | 20 |
| c-rd | 213.4 | **168.5** | **13.8** | 7.7 | **2.0** | 33.9 | 92.4 | 100.1 | **3.6** | **41 / 0.8 ms** | **18** |
| c-rd-dpidle | 211.1 | **166.9** | **13.7** | 7.6 | 2.0 | 32.8 | **90.2** | **97.9** | 3.8 | 46 / 0.7 ms | 18 |

Batch B shows the same pattern. GETATTR callbacks were 27.4 µs (plain),
18.5 (`spin`) and 13.8 (`rd`). Sync callbacks were 103.5, 93.4 and 92.9 µs
per file.

- **CREATE now carries the moved work:** +4 µs (29.7 → 33.9), 6.2 µs of
  `digest_flush` per file.
- **The RELEASE job loses** that work plus its own BEGIN and COMMIT.
- **GETATTR stops waiting.** At 13.8 µs it costs the same as a LOOKUP
  (13.2).
- **Wall per create** fell 228 → 211–214 µs with `spin`, `rd` and both.
  That is within the ±10–20% create noise, so the callback and wait counts
  are the evidence.

**Four Workspaces creating at once** (µs per file, aggregate wall):

| Arm | Wall | CPU | GETATTR cb | Lock wait | Waits > 5 µs |
| --- | ---: | ---: | ---: | ---: | ---: |
| c-plain | 76.3 | 248.0 | 30.8 | 16.5 | 944 / 26.8 ms |
| c-spin | 73.8 | 244.5 | 21.4 | 8.2 | 696 / 8.9 ms |
| c-ctrl | 74.3 | 246.3 | 21.6 | 8.1 | 707 / 8.6 ms |
| c-rd | 72.9 | **235.1** | **16.5** | **4.4** | **126 / 2.4 ms** |
| c-rd-dpidle | 73.3 | **236.9** | 16.1 | 4.2 | 88 / 1.7 ms |

`spin` CPU per create, single and four-Workspace, across batches (plain →
`spin`):

| Batch | Single | Four Workspaces |
| --- | ---: | ---: |
| A | 171.4 → 172.4 | 259.1 → 251.8 |
| B | 174.9 → 178.3 | 269.5 → 240.7 |
| C | 181.7 → 175.9 | 248.0 → 244.5 |

No measurable cost. Spinning replaces futex parks, which also cost CPU.

### 3. 4 KiB random writes: 16,384 writes

- **No switch changes them.** One writer: 66.2–69.7 MB/s and
  10,850–11,660 CPU µs per MB in every arm of batch C.
- **Four Workspaces:** 178.9–190.5 MB/s, 2.7× one writer. The guest
  becomes CPU-bound: CPU per wall second is about 3.0, and the hold per
  write grows from 20 to 35 µs.
- **One Workspace, four writers on four files:** 130.8–140.0 MB/s, 2× one
  writer.

### 4. Group commit: potential, not built (batch B and C counts)

**One writer, the probe's `random_write_4k`:**

- Each pwrite waits for its reply, and write-back is off, so exactly one
  WRITE is in flight.
- Lock wait is 0.6 µs per write, with 18–42 waits over 5 µs in 16,384
  writes.
- No job ever queues behind another, so there is nothing to commit
  together. The 11.5% bound from [write-path](write-path.md) assumed a
  queue that this workload never forms.

**Four writers in one Workspace:**

- The Workspace lock is 60% busy: 17.4 µs hold per write, of which BEGIN
  plus COMMIT is 5.7 µs.
- Each writer's cycle is about 115 µs: a 57 µs callback plus a 59 µs round
  trip.
- Sharing BEGIN and COMMIT across a full queue saves at most about 5.7 µs
  per cycle, about 5%. The lock is not the bottleneck.

**Custody cost of group commit:**

- Each member's failure needs its own savepoint and its own open-state
  undo mark.
- A failed group COMMIT would make every member's outcome uncertain.
- Every member must wait for the last member's COMMIT before it replies.

That is too much for about 5%, so it was left out.

## What worked

- **`reldefer`:**
  - Removes the RELEASE-to-GETATTR lock collision without spinning.
  - Cuts two statements per create and 7% CPU per create (5% under four
    Workspaces).
  - Every digest still reaches the Commit intact (90/90).
- **`digestidle`:**
  - Keeps `digestpar`'s single-stream gain: +19% (B) and +26% (C).
  - Turns the four-Workspace regression (−21–23%) into a gain: +3% (B)
    and +11% (C).
- **CPU accounting:** `digestpar` adds 5–17% CPU per MB and runs a second
  core at about 35% of wall time. `spin` adds none measurable.

## What did not work, or was not done

- **Group commit:** not built (§4).
- **Wall time per create:** the visible gain of `spin` and `reldefer`
  (about 6–7%) is inside the create noise. One sample per arm cannot
  confirm it; the counts can.
- **`reldefer` moves work, it does not remove it.** About 6 µs of
  `SELECT` and upsert per file lands in the next CREATE. Only BEGIN,
  COMMIT and the collision go away.

## Risks

- **`reldefer`:**
  - A failure of the deferred upsert fails the next non-light job, which may
    be unrelated to the closed file. Under Disposable this is only
    capacity or I/O failure. A product form could instead drop a waiting
    digest on failure: a missing digest only makes the Commit build the file
    in full.
  - The light-job list (RELEASE and observe visits, including OPEN) is an
    invariant. Any future light job must not write inode rows or read
    `fs_digest`.
  - The light flag is a thread-local reset by a guard. A panic resets it.
- **`digestidle`:** one helper still serves the whole process. It caps the
  extra core at one but gives the gain to whichever Workspace finds the
  helper idle.
- **Write-back stayed off and the kernel round trip is unchanged.**

## Recommendation

1. **Replace `spin` with `reldefer`** (or a product form of it). It gives a
   lower GETATTR, fewer waits and fewer statements, with less CPU and no
   spinning. `spin` is cheap here but no longer needed.
2. **Do not ship `digestpar` with one shared helper.** If the owner accepts
   a second core per write, ship it as `digestidle`, with a bounded helper
   and inline fallback. That gives about +20–30% sequential write for about
   +5–17% CPU per MB, and no loss under concurrent Workspaces.
3. **Drop group commit for kernel writes.** The single-writer case cannot
   use it, and the concurrent case is bounded at about 5%.
4. **Proposed safe set for round 5:** `noresume,digestpar,digestidle,reldefer`
   in place of `noresume,digestpar,spin`. In batch C it gave 930 against
   890 MB/s single, and 2,483 against 1,712 MB/s for four Workspaces.

## Checks and gaps

**Run:**

- Linux release build of the probe, `--locked --offline`.
- rustfmt on changed files.
- 21 probe runs, each once, each exit 0, each with every written byte read
  back.
- Coherence and truncate coherence, seek relist and modified `ls -l` on the
  final binary: all pass.
- `digest_commit`: 90/90 in all eight batch-C arms.
- `digestcheck`: 1,647 of 1,647 equal.

**Not run:**

- Clippy: `NOT_RUN`. The image has no clippy component.
- Unit and mount tests (`layerfs-overlay` `tests/digests.rs`, daemon
  `file_digests.rs`): `NOT_RUN`. With the switches unset the code paths are
  the original ones.
- An injected failure of the deferred upsert, or an unmount with digests
  waiting: `NOT_RUN`. The second follows by construction: the waiting map
  is open state, dropped with the overlay file.
- Durable: `NOT_RUN`, disabled by the owner until explicit reauthorization.

**Limits:**

- One sample per arm.
- The CPU speed drifted inside batch B.
- Create wall times vary ±10–20%. Counts, callbacks and CPU per unit are the
  robust signals.
