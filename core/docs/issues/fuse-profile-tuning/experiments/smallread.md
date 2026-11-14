# Experiment: cold reads of many small files (`smallread`)

> **Status:** Diagnostic prototype on branch `exp/smallread` (base
> `c8cf56ab0e`), issue #684 round 4. Not a product contract. One sample per
> arm; times are diagnostic, counts are exact. All switches are off by
> default.

## Question

Every earlier experiment measured one 500 MiB file. The per-tool-call case
an agent hits most is different: `grep -r`, `cat` or a build opening
thousands of small committed files on a fresh mount. Where does that time
go, and what speeds it up?

## Probe and fixture

- **Fixture** (`prepare`, deterministic): `tree/` holds 20,000 text-like
  files of `512 · 2^(7u²)` bytes (0.5–64 KiB, median about 1.7 KiB, 155.5 MB
  in all) in 376 nested directories. The files are JavaScript-like word
  streams, so zstd is active. The Store is 588,890,112 bytes (with `rand` and
  `small/`). Every file is one whole-file object.
- **Phases.** Each runs in its own child process (`tree <fixture> <work>
  <phase>`), so the daemon object cache, lengths and metadata caches start
  empty. Each mounts a fresh Workspace, evicts the Store copy and checks
  `fincore` = 0 resident bytes (every arm: 0):
  - `read`: one thread opens, reads whole and closes every file, in
    directory order, as `grep -r` does.
  - `read8`: the same with 8 threads taking directories from one queue.
  - `stat`: one thread `lstat`s every file, as `find -printf %s` does.
  - `native`/`native8`: the same tree on the volume's ext4. Pages are
    dropped per file and dentries/inodes by `drop_caches=2`, with `fincore`
    = 0 before the walk.
- **Check.** Every walk folds `(path, bytes)` (or `(path, size, mode)`) into
  order-free totals, compared with the totals recorded at prepare. Every
  phase of every arm matched.
- **Recorded:** wall time, files/s, MB/s, CPU (`getrusage`), `VmHWM`, kernel
  opcode counts and callback time, Store stages, object demands and the new
  prototype counters.
- `LAYERFS_PROBE_TREE_ONLY=1` skips the large-file phases but still runs
  the coherence phases. The `b4-full-*` arms run the whole probe.
- **Control:** the round-3 safe set (512 KiB / 4, async, noverify, fasthash,
  4 read sessions, `COPY=all`, `WRITE=noresume,digestpar,spin`,
  READDIRPLUS plain, READDIR rows 129). `base` is no switch at all.

## Where the time goes (control, `b3-control`, 1 thread)

`read`: 3,330 ms, 6,007 files/s, 46.7 MB/s, `VmHWM` 100,216 KB. Native ext4
reads the same tree in 841 ms (`b1-control`).

| Opcode | Count | Callback ms | µs each |
| --- | ---: | ---: | ---: |
| OPEN | 20,000 | 289 | 14.5 |
| READ | 20,000 | 1,373 | 68.7 |
| RELEASE | 20,000 | 110 | 5.5 |
| READDIRPLUS | 1,448 | 408 | 282 (≈20 per entry) |
| OPENDIR / RELEASEDIR | 376 / 376 | 8 / 4 | — |
| LOOKUP, GETATTR, FLUSH | 1 each | — | — |

- **Callbacks** total 2,193 ms. The other 1,137 ms (57 µs per file) is
  outside the callbacks: kernel round trips, wakeups and the walker itself.
- **READ is the largest single cost**, and it is the Store:
  - 20,744 single-object Store demands take 1,026 ms (49 µs each), run one
    at a time (`store_peak` 1).
  - Inside them: locate 168 ms, pack reads 191 ms (2,275 packs, about 9
    objects per pack), zstd 222 ms (10 µs per object), BLAKE3 125 ms.
- **Owner jobs:** 63,276 jobs (3.2 per file) are held 350 ms in all.

**Already in place (checked):**

- **TTLs and page cache:** entry and attribute TTLs are 60 s, and
  `FOPEN_KEEP_CACHE` is set (`request/reply.rs`,
  `coherence/pages.rs`).
- **FLUSH is sticky ENOSYS:** 1 FLUSH per mount.
- **No LOOKUPs:** READDIRPLUS plain already removes all 20,000 LOOKUPs. The
  `base` arm sends 20,376 LOOKUPs and reads in 4,625 ms.
- **Store read-ahead is never used for small files:** the 8 MiB read-ahead
  applies to chunked content only, and these files are whole-file payloads
  (`read_ahead` count 0).
- **Kernel readahead adds READs only under concurrency:** each file is one
  READ of at most 64 KiB with 1 thread. With 8 threads the kernel sends
  23–27 k READs; the extra ones are cache hits.

`read8` (control): 1,339 ms. Store busy 910 ms, at most 2 demands at once.
The limit is the **2 receive loops per mount** (`RECEIVE_SLOTS`): only 2
requests are served at a time, whatever the number of readers.

`stat` (control): 465 ms, 43,033 files/s. It is all READDIRPLUS (356 ms).
In it are 20,744 single-id length demands (locate 139 ms), one per entry.

## Changes (all off by default)

`LAYERFS_PROBE_SMALL`, a comma list (`layerfs-workspace` `base/small.rs`):

- **`lengths`:** a READDIRPLUS window asks the lengths of all its inherited
  files in one demand (`provide_lengths`) before it composes attributes.
  Length demands: 20,744 → 1,072.
- **`sibling`:** a READDIRPLUS window remembers its small files (≤ 64 KiB)
  in listing order. The first base READ that misses on one of them fetches
  the siblings after it in one batch, on a thread of its own over a fresh
  custody.
  - The batch is marked in flight first, so the following READs wait for it
    instead of demanding again.
  - Bounds: 64 ids or 2 MiB per fetch, 8 MiB in flight in all, 256 windows.
  - A listing that reads no file (`ls -l`, `find`) fetches nothing: `stat`
    triggers 0 fetches.
- **`eager`** (implies `sibling`): once a directory has had such a miss, its
  later windows fetch their small files as soon as they are listed.
- **`parts`:** a fetch is split into `READ_SESSIONS` (4) parts, and each
  part is cached as soon as it lands.
- **`push`** (FUSE adapter, `request/reply.rs`): before replying to a
  read-only OPEN of an unchanged base regular file of at most 64 KiB, the
  daemon reads the whole file as a READ on the new handle would, and stores
  it in the kernel page cache with `FUSE_NOTIFY_STORE` (fuser
  `Notifier::store`; fuser is not patched).
  - The `read` that follows needs no READ request.
  - A read-only visit before the read and another after the store must name
    the same unchanged base content root. Otherwise the inode's pages are
    invalidated (`inval_inode`), so a store that raced a change does not
    stay visible.

`LAYERFS_PROBE_FUSE_LOOPS=N` (1–16, `layerfs-fuse` `session/state.rs`):
the number of receive loops per mount. Default 2.

The probe additions are the tree fixture, phases and native reference
(`examples/fuse_profile_probe/tree.rs`) and a `coherence_small` phase.

## Results

Binaries:

| Batch | SHA-256 |
| --- | --- |
| smoke | `d7799f11f55df2f57544edcc11b6decc41c3b92cf8c6adc2cbf169c465fd1df1` |
| b1 | `b81bbe097fedd758563b3d21a00ef54e363a728813f2e8822349c4b8c16676ad` |
| b2 | `0b106cc0cf10d8b240a363a6b56d16094c2763435bae89b4e0400e494f628dd4` |
| b3 | `7b17409ad2af27aa2422dfff5b00e787a377934b288af9c0959c09259833fc97` |
| b4 (final) | `0cdfd7c4fb25421892fe94c22429d43a84ab02ed8598106aa82573d968c86962` |

`lep` means `lengths,eager,parts`. Times are wall ms.

**Columns:**

- **B3 µs:** the Store resolver's BLAKE3 µs per call in `read`, the same
  22,416 objects in every arm. It is a CPU-speed and contention indicator.
- **HWM:** `VmHWM` in KB for `read` / `read8`.
- **Host load:** the 1-minute load at arm start was 2.1–4.1.

| Arm | read | files/s | read8 | files/s | stat | read CPU ms | HWM | B3 µs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: |
| b1 native (ext4) | 841 | 23,791 | 314 | 63,677 | — | 314 | — | — |
| b1 base (no switch) | 4,625 | 4,325 | 2,299 | 8,700 | 1,279 | 3,884 | 98,184 / 98,796 | 5.78 |
| b1 control | 3,687 | 5,424 | 1,319 | 15,167 | 464 | 3,236 | 100,828 / 111,472 | 5.58 |
| b1 lengths | 3,612 | 5,538 | 1,307 | 15,303 | **424** | 3,170 | 100,112 / 110,496 | 5.58 |
| b1 sibling | 3,201 | 6,248 | 1,082 | 18,475 | 468 | 3,827 | 130,324 / 175,844 | 5.77 |
| b1 loops8 | 3,674 | 5,443 | **863** | 23,169 | 478 | 3,217 | 108,192 / 137,740 | 5.58 |
| b1 lengths+sibling+loops8 | 3,300 | 6,060 | 888 | 22,531 | 465 | 3,923 | 135,188 / 175,828 | 5.77 |
| b2 control | 3,631 | 5,508 | 1,471 | 13,598 | 483 | 3,178 | 99,964 / 111,208 | 5.58 |
| b2 sibling | 3,237 | 6,179 | 1,402 | 14,271 | 508 | 3,855 | 128,908 / 169,980 | 5.86 |
| b2 eager | 3,112 | 6,427 | 1,259 | 15,885 | 484 | 3,879 | 130,116 / 169,120 | 5.90 |
| b2 eager+parts | 3,000 | 6,668 | 1,254 | 15,947 | 456 | 3,882 | 130,376 / 162,796 | 5.98 |
| b2 loops4 | 3,725 | 5,369 | 963 | 20,766 | 504 | 3,263 | 102,984 / 132,596 | 5.61 |
| b2 lep+loops8 | 2,950 | 6,779 | 1,218 | 16,424 | 442 | 3,872 | 133,896 / 184,004 | 5.96 |
| **b3 control** | **3,330** | 6,007 | **1,339** | 14,938 | **465** | 3,019 | 100,216 / 111,224 | 5.58 |
| b3 lep | 2,992 | 6,684 | 935 | 21,386 | 435 | 3,829 | 130,352 / 165,712 | 5.97 |
| b3 push | 2,923 | 6,843 | 1,316 | 15,201 | 463 | 2,762 | 99,924 / 111,784 | 5.53 |
| **b3 lep+push** | **2,332** | 8,576 | 965 | 20,723 | 438 | 3,424 | 130,992 / 165,520 | 5.97 |
| **b3 lep+loops4** | 2,928 | 6,831 | **787** | 25,410 | 461 | 3,819 | 132,696 / 178,792 | 5.96 |
| b3 lep+push+loops4 | 2,340 | 8,548 | 877 | 22,803 | **434** | 3,410 | 132,116 / 170,772 | 5.93 |
| b4 full control | 3,640 | 5,494 | 1,339 | 14,932 | 455 | 3,179 | 100,808 / 111,444 | 5.55 |
| b4 full lep+loops4 | 2,988 | 6,693 | 890 | 22,476 | 486 | 3,856 | 130,992 / 172,716 | 5.97 |
| b4 full lep+push | 2,367 | 8,449 | 1,306 | 15,313 | 461 | 3,494 | 131,188 / 164,592 | 6.15 |
| b4 full lep+push+loops4 | 2,798 | 7,149 | 1,246 | 16,053 | 473 | 4,143 | 130,664 / 176,064 | 6.95 |

**Headline, batch 3 (same binary, back to back):**

- **1 thread (`grep -r`):** control 3,330 ms → lep+push 2,332 ms. That is
  −30%, or 1.43×, and 46.7 → 66.7 MB/s. CPU rises 3,019 → 3,424 ms (+13%).
  - b4 repeats it at 3,640 → 2,367 ms (−35%).
  - The READ requests disappear: 20,000 → 0.
  - Prefetch moves the Store work off the request: 20,744 demands → 5,119
    batched demands. Store busy falls 1,025 → 567 ms.
- **8 threads:** control 1,339 ms → lep+loops4 787 ms. That is −41%, or
  1.70×. b4 repeats it at 890 ms (−34%).
- **stat:** 465 → 434–438 ms (−6%) with `lengths`. b1 gave −9%.
- **What is left:** native ext4 reads the tree in 841 ms (1 thread) and
  314 ms (8 threads). LayerFS lep+push is 2.8× native.

**The pieces (from the stage split):**

- **`push`** removes the READ request. OPEN grows from 14.5 to 43 µs, since
  it now holds the read and two extra visits.
  - Alone it gives −12%, because it still pays one Store demand per file.
  - With prefetch, the OPEN finds the object in the cache.
- **`sibling`/`eager`/`parts`** turn READ from 68.7 into 26.8 µs per file
  (lep):
  - `finish` falls 1,053 → 182 ms.
  - About 376 synchronous misses remain, one per directory.
  - `parts` lands the first siblings early (+4% over eager).
- **More receive loops** only help parallel readers.
  - With 8 loops, owner-lock waits grow to 1.3–4.1 s summed (`b2 lep+loops8`
    is slower than `b3 lep+loops4`), and CPU rises.
  - 4 loops was the best of those measured for 8 readers.
  - The best `read8` arm, lep+loops4, still has owner waits of 722 ms summed.
- **`lengths`** batches the READDIRPLUS length demands: 20,744 → 1,072
  demands, locate 139 → 115 ms.

**Combined arms vary more than the 15% noise rule allows:** lep+push+loops4
gave 2,340 / 877 ms in b3 but 2,798 / 1,246 ms in b4. Every stage of the b4
arm was 15–20% slower for identical work (zstd 272 against 228 ms, BLAKE3
7.0 against 5.9 µs). The guest CPU was contended in that arm, and the cause
was not diagnosed. Push without extra loops reproduced (2,332 / 2,367 ms).

## Coherence

On the final binary (`b4-full-*`):

- **Large-file coherence:** `coherence` and `coherence_truncate` pass in
  all four arms.
- **New `coherence_small` phase** (on a 1,013-byte tree file):
  - A reader opened before a write sees the write.
  - A new open after the write sees the new bytes.
  - A truncate by path while a reader is open reads 0 bytes past the new
    end, from both the old reader and a new one.
  - A second Workspace still reads the base bytes.
  - It passes in every arm. With `push` on, 4 opens were stored and 2 were
    skipped: the writable open, and the open of the changed file.
- **Unchanged outputs:** all listing outputs are equal (`ls -l`
  `7dfdae789d97b55a`, `ls` `053cff30e17f4f55`, `find` `f6bc3f8bacb470ed`,
  modified `3412b3d5c485930b`).
- **Revalidation:** `push_invalidated` was 0 in every arm. The probe does not
  construct a writer racing the store (see Risks).

The large-file phases of the b4 full runs moved without an attributable
cause:

- The cold read gave 533 / 577 / 698 / 701 MB/s.
- Pack-read time differs while the counts are the same.
- `push` only skips files over 64 KiB.

This matches the uncontrolled host cache of the VM disk; treat those as
noise.

## Risks

- **`push`:** an OPEN does the read even when the opener reads nothing or
  little. That is at most 64 KiB, and only for read-only opens of unchanged
  base files.
  - **Possible deadlock:** `NOTIFY_STORE` locks page-cache pages. A store
    that meets a page locked by a READ waiting for a busy receive loop could
    stall. This was not seen, but it is not proven impossible.
  - **Coherence:** it rests on the before/after visit argument, not on a
    proof.
  - **RELEASE backlog:** with 8 readers and 2 loops, 4,656 RELEASEs were
    still in the kernel's background queue (`max_background` 4) when the
    mount was unmounted (`release` 15,344). The heavier OPENs crowd out the
    asynchronous RELEASEs. Engine open-file state grows meanwhile.
- **Prefetch:**
  - Reads up to 2 MiB per directory window on the first miss, even for a
    reader that wanted one file.
  - Costs +30 MB `VmHWM` with 1 reader and +55–65 MB with 8: concurrent
    fetch buffers, per-custody pack caches and threads.
  - Costs +13–27% CPU.
  - Spawns one thread per fetch part (4,000 per walk); a product needs a
    pool.
  - Its objects are content-addressed and authenticated as on demand
    (Store BLAKE3, `noverify` as in control).
- **More receive loops:** each loop keeps a receive buffer of `max_write`
  + 4 KiB (516 KiB at 512 KiB), and every added loop contends for the one
  Workspace lock. The doc comment on `RECEIVE_SLOTS` already warns that
  writers slow down. Writers were not measured here.
- **Not run:**
  - Clippy: unavailable in the image, `NOT_RUN`.
  - Crate tests: the scripts build only the probe.
  - NFS.
  - A racing-writer test for `push`.
  - Durable: disabled by the owner, `NOT_RUN`.

## Recommendation

1. **Adopt `lengths`:** one length demand per READDIRPLUS window. It changes
   no semantics.
2. **Adopt sibling prefetch** (`eager,parts`) **with a fixed thread pool:**
   1 thread −10–18%, 8 threads −30%. It changes no semantics: objects are
   content-addressed and authenticated as on demand.
3. **Make receive loops configurable:** 4 for parallel tool calls, measured
   with writers before it becomes a default.
4. **Owner decision:** `push` (`FUSE_NOTIFY_STORE` at OPEN). It is the
   largest single-thread gain (a further −22%, 1.43× in all), but it adds a
   kernel-cache write path whose coherence and deadlock freedom need a
   design and a racing-writer proof.
5. **Next limits:**
   - OPEN+RELEASE: 2 requests and 2–3 owner jobs per file.
     `FUSE_NO_OPEN_SUPPORT` would remove both requests but drops descriptor
     custody, so it needs an owner decision.
   - Owner-lock contention with more loops.
   - zstd at 10 µs per small object.
