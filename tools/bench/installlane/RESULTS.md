# Install-only lane: measured results

Date: 2026-11-16. Host: 88-vCPU Docker sandbox, `/var/tmp` on the machine
filesystem (overlay on ext4), 125 GiB RAM; Python 3.11.2; SQLite 3.40.1.
Store-shaped model, one run per arm, each in a fresh process. **The 50 GB arm
was not run.**

The model is described in [`README.md`](README.md): 1 MiB packs, eight 128 KiB
canonical objects per pack, 25% of packs dead at the mark, content-addressed
dedupe references from post-mark commits to skipped packs. `store.build` is
the fresh-copy build (`journal_mode=OFF` target); `store.remark_delta` is the
open high-water mark + incremental re-mark + delta copy; `store.verify`
compares source and target closure at a fence serial. Writer and checkpoint
rates are host-specific; the contract finding is in the ratios, the *busy*
results and the correctness probes, not in absolute seconds alone.

Raw numbers: [`results-small.json`](results-small.json),
[`results-10g.json`](results-10g.json).

## 1. Small arm

### 1.1 WAL growth under the build snapshot

The build's read snapshot is modelled by a read transaction (in production it
is the `immutable=1` attachment, which also forbids every checkpoint). Two
writers commit 1 MiB rows; `wal_checkpoint(TRUNCATE)` is attempted while the
snapshot is open.

| probe | writer rate | committed | peak WAL | checkpoint while snapshot open | final checkpoint | rate |
|---|---|---|---|---|---|---|
| `hold`, 10 s | 420 MiB/s | 4,227 MiB | 4.50 GB | `busy=1` after 5.0 s busy timeout | 25.56 s | 168 MiB/s |
| `cap` 256 MiB, 10 s | 25.5 MiB/s | 256 MiB | 270 MB | `busy=1` | 0.69 s | — |
| `cap` 128 MiB + checkpoint loop | 217 MiB/s | 2,190 MiB | 135 MB | n/a | 0.006 s | 16,804 truncates |
| `catchup`, 1.08 GB backlog, 15 s | 307 MiB/s | 5,410 MiB | 6.84 GB | n/a | 41.80 s | 156 MiB/s |

Writer latency: `hold` p50 2.2 ms / p99 5.5 ms / max 10.1 s; `cap` p50 2.3 ms /
p99 6.2 ms, but the two writers waited **17.33 s** in the 10 s window after
the 256 MiB cap filled; `catchup` p50 2.1 ms / p99 3.9 ms / max 9.0 s.

Findings:

- The WAL grows by every committed byte while the snapshot is open; a
  `wal_checkpoint(TRUNCATE)` cannot run (it spends the connection's 5 s busy
  timeout and returns `busy=1`). The final checkpoint is **168 MiB/s** here,
  against a **420 MiB/s** writer rate: the backlog cannot drain while writers
  keep committing.
- A WAL cap with no checkpointer is a hard write pause: at a 256 MiB cap the
  writers committed 256 MiB and then stalled for the rest of the build. This
  is the same whole-build exclusion the frozen lane already imposes, plus a
  WAL on disk. With a checkpoint loop the cap is nearly free (135 MB peak,
  1.3 s of stalls), but that loop is exactly what the build snapshot forbids.
- `catchup` is the install step: a 1.08 GB backlog is handed over, writers
  continue, and **0 of 430 `wal_checkpoint(TRUNCATE)` calls succeeded** (all
  `busy`); the WAL grew to 6.84 GB (+366 MiB/s). The backlog only drains after
  writers stop.

### 1.2 Correctness

| probe | result |
|---|---|
| naive "new packs only" delta | missed **8 reachable objects** of 872: the post-mark commit deduped onto an object in a skipped pack, and its pack was never copied. `verify` failed. |
| closure re-mark + delta | copied the 63 new packs **and** the one skipped pack; 64 MiB delta, 0.34 s; `verify` clean (872/872 objects). |
| adversarial delta | a post-mark commit referencing every skipped object: **0 new bytes** written, delta **76.5 MB = 1.00× the dead set** (73 packs). The delta is bounded by the dead set, not by post-mark writes. |
| concurrent fence | delta copied at w1=199 and verified; 246 commits later at w2=445 the target missed **1,984 objects**. The catch-up after writers stopped (248 packs, 260 MB, 1.13 s) verified clean (3,577/3,577). A delta watermark is always behind the newest commit. |

## 2. 10 GiB arm

One 10 GiB fixture (10,240 packs, 2,575 dead = 2.70 GB / 25.2%, 81,920
objects). Mark, fresh-copy build, then the build snapshot stays open while
writers commit 6,348 MiB (the 6.2 GB the 32.5 s / 190 MiB/s premise names).
Then the re-mark, delta copy, checkpoint and verify.

| step | measurement |
|---|---|
| fixture | 18.0 s, 10.74 GB, 10,240 packs, 2,575 dead packs (2.70 GB) |
| mark (synthetic metadata walk) | 0.19 s; 7,665 live packs / 61,320 objects (8.04 GB) |
| build (fresh-copy) | **18.96 s** for 8.04 GB (434 MiB/s) |
| writers, snapshot held | 6,348 commits = 6.35 GiB in 18.26 s (**348 MiB/s**); WAL peak **7.36 GB**; p50 2.8 ms / p99 4.3 ms / max 9.05 s; 126 dedupe refs into skipped packs |
| frozen §7/§10 fence | mark tuple `(7665, 10241, 81920, 10.74 GB)` vs current `(14013, 16589, 81920, 17.39 GB)`: **not equal → `Conflict`** |
| incremental re-mark | scan 0.88 s; 6,380 delta packs (6,348 new + 32 skipped packs reached by dedupe), 6.69 GB |
| delta copy | 40.78 s for 6.69 GB (**164 MiB/s**) |
| checkpoint (`TRUNCATE`) after snapshot close | 7.38 GB in **38.43 s** (**183 MiB/s**) |
| verify | 0.57 s; source = target = 112,353 objects, `missing=0`, `extra=0` |
| **install-only fence** | checkpoint + delta + verify = **80.65 s** |
| current path (build under the lane) | **18.96 s** (fresh-copy; 32.5 s in the earlier prototype; the shipped `VACUUM INTO` build is 94 s at 10 GB per D2/T-G61) |

The delta copy is 0.91× the WAL and 1.005× the writer commits; the 126 dedupe
references added 32 packs that the mark had skipped. Verification passes, so
the closure re-mark is *correct*; it is not *bounded* by post-mark writes.

### Verdict

**Install-only is deferred.** Under the frozen contract the build is a single
write exclusion of the build duration. The measured install-only fence is
**80.65 s vs 18.96 s** (4.25× the fastest measured build; ≈2.5× the 32.5 s
premise; the shipped build is slower still). The three independent reasons:

1. **No bounded WAL.** Checkpoint throughput (156–183 MiB/s) is below the
   writer rate (176–420 MiB/s across runs), and with writers active
   `wal_checkpoint(TRUNCATE)` is `busy` on every call (0/430 and 0/3,105). A
   cap without a checkpointer just re-imposes the build-length write pause
   (256 MiB cap → 17.33 s stalled in a 10 s build).
2. **No bounded delta.** Content-addressed dedupe lets a post-mark commit
   reference a pre-mark object whose pack the mark skipped. A "new packs only"
   delta loses data (8 objects at 64 MiB); a closure re-mark is correct but
   its delta is bounded by the dead set, not by post-mark writes (1.00× the
   dead set with 0 new bytes in the adversarial probe). The dead set is the
   data the rebuild exists to free.
3. **No shorter outage.** Checkpoint + delta + verify is longer than the build
   it replaces, and it still needs a final quiesce (a delta watermark missed
   1,984 objects in the fence probe). The frozen tuple fence also always
   fails once writers continue.

The exact contract changes a future install-only design would need are listed
in `G7` of
[`gc-42-decisions.md`](../../../core/docs/issues/project-41/gc-42-decisions.md).
None was made here; no product code or contract text changed.

## Reproduce

```sh
python3 tools/bench/installlane/suite.py --arm small   --dir /var/tmp/installlane/small
python3 tools/bench/installlane/suite.py --arm ten_gib --dir /var/tmp/installlane/ten_gib
```

The 10 GiB arm writes about 35 GiB into `--dir` (source, target, WAL) and takes
about 3 minutes on the host above.
