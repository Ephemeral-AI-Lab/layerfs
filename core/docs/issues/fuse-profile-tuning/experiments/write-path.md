# Experiment `write-path`: where a WRITE and a create go

> **Status:** Diagnostic research for #684 on branch `exp/write-path` (base
> `4db7d765c6`). Not a product contract and not for merge. Disposable Store,
> kernel write-back off, fuser not patched, read path unchanged. One sample per
> arm; counts are exact, times are diagnostic.

## Hypotheses

1. A WRITE is dominated by one SQLite transaction plus the kernel round trip.
2. The GETATTR per create targets the parent directory, re-validated by
   `default_permissions` after the create invalidates its attributes.
3. Cheap within-contract changes can remove statements, invalidations or
   waits.

Hypothesis 1 held for 4 KiB writes. It was wrong for sequential writes: the
largest single cost is the BLAKE3 whole-content digest of appended bytes
(#364), 45–52% of the WRITE callback. Hypothesis 2 is confirmed.

## Change

Commit `f50acbe6ef` (instrumentation and switches). The docs commit adds a
probe-only diagnostic, `LAYERFS_PROBE_BURN`.

**New `timer::stages` stages:**

- mount-ports: mutation visit, supply, recognizer, release; observe visit.
- fuse: mutation reply. GETATTR counts by kind, by descriptor, and whether
  the GETATTR names the parent of the latest CREATE or MKDIR.
- store-service: owner lock wait (plus waits over 5 µs), owner hold, and
  overlay statements by family.
- workspace: the overlay call, the decision and the publication.
- overlay: fence, check, inode rows, payload cells, append digest, settle,
  and the kernel effect. This adds a path dependency on `layerfs-telemetry`
  and a matching `Cargo.lock` line.

**Per-statement trace:** `LAYERFS_PROBE_SQLTRACE=1` prints `PROBE_SQL`
lines.

**Switches**, in `LAYERFS_PROBE_WRITE` (comma list; unset means none):

| Switch | What it does |
| --- | --- |
| `noresume` | Skips the digest-resume `SELECT sequence,size` when the window cannot resume: its offset is not 0 and not the end of the resumable slot. Also skips the carried-digest read of an inode the job itself creates. Equivalent by construction: both reads could only return rows that lead to no action. |
| `digestpar` | BLAKE3 of an appended window of 32 KiB or more runs on one process-wide helper thread while the job writes cells and COMMITs. `follow_digests` advances the followed length. The hasher is installed right after COMMIT, before the job returns and before `open.commit()`. Where the digest finishes in the same job (no descriptor), it joins at once. |
| `digestcheck` | Compares each helper result with an inline update. |
| `spin` | Tries the Workspace owner lock for up to 50 µs before parking. |
| `nodefperm` | **Diagnostic only:** mounts without `default_permissions`. Not a candidate. |

## Binaries

Every log's first line is the SHA-256 of the binary it ran.

| SHA-256 | Source | Arms |
| --- | --- | --- |
| `03df77f4…` | early timers | s1 |
| `1d254ae4…` | + overlay timers | s2 |
| `691048f8…` | + trace, noresume, digestpar, spin, nodefperm | b3 |
| `99c9ad6f…` | + digestcheck; digestpar without the 32 KiB floor | b4 |
| `6eae1ea52be3ad23dc6ca8d383abf3dbfbdf33402a5dd7b230d0410520af23df` | `f50acbe6ef` | b5 |
| `3cfbf2a2c452faa974a6ffdf935ba12f764ba3e85c29f5d52ef1f8f67f641409` | `f50acbe6ef` + probe `LAYERFS_PROBE_BURN` | b6 |

Every arm exited 0. The cold read's checksum passed in every arm.

## 1. Time split (b5 controls; BLAKE3 18.5 µs per call, the slow round)

Rows give µs per kernel WRITE. Nested rows are parts of the row above.
"Outside" is phase wall time minus the summed callbacks: the kernel round
trip and the user-to-kernel copy.

| Part | Seq 128 KiB | Seq 512 KiB (2 jobs) | Random 4 KiB |
| --- | ---: | ---: | ---: |
| Wall per WRITE | 298 | 938 | 82.0 |
| Outside the callback | 77 | 170 | 36.0 |
| WRITE callback | 220.8 | 767.0 | 46.0 |
| — owner job (mutation visit), lock wait | 0.9 | 1.2 | 0.8 |
| — owner job, held | 201.0 | 709 | 30.5 |
| —— fence and inode-row read | 4.2 | 6.4 | 3.9 |
| —— inode row write | 3.9 | 7.1 | 3.8 |
| —— payload cells (`fs_data` upserts) | 37.9 (4 × 9.4) | 124 | 10.0 (2 statements) |
| —— **append digest, BLAKE3 (#364)** | **100.4** | **401** | — (not an append) |
| —— decision and publication | 0.8 | 1.3 | 0.8 |
| —— BEGIN / COMMIT | 0.5 / 51.4 | 1.0 / 166 | 0.6 / 8.8 |
| — recognizer (claims) | 0.4 | 0.6 | 0.15 |
| — reply to the kernel | 5.7 | not timed (split path) | 5.3 |
| — release of the publication ticket | 1.2 | 2.0 | 1.1 |
| — rest: receive, input copy, compose | ≈11 | ≈52 | ≈8 |

**Statements per write** (b3 trace, fast round):

- Random 4 KiB, seven statements:
  - COMMIT 6.4 µs
  - `fs_data` upsert 4.5 µs
  - inode `SELECT` 2.3 µs
  - cell-shape `SELECT` 2.0 µs
  - inode `UPDATE` 1.5 µs
  - digest-resume `SELECT` 0.7 µs (`noresume` removes it)
  - BEGIN 0.4 µs
- Sequential 128 KiB: COMMIT 34 µs, and four `fs_data` upserts of 6.4 µs.

**Create of a 1 KiB file** (b5-ctrl128, 268 µs per file):

- Callbacks: LOOKUP 16.2, GETATTR 31.5, CREATE 38.6, WRITE 41.5 and
  RELEASE 19.7 µs (RELEASE is asynchronous).
- About 120 µs per file is outside the callbacks.
- 21 SQL statements per file across the five requests.
- Lock waits over 5 µs: 1,624 of 2,000 files, 31 ms in total.

**Kernel round trip.** It is 42–48% of a 4 KiB random write (25.5–36 µs
per write across the controls). The daemon cannot reach it. Four spinning
threads (`LAYERFS_PROBE_BURN=4`, b6) did not shorten it (582 → 602 ms), so
it is not explained by idle guest CPUs.

**Group commit (out of scope; potential only).** BEGIN plus COMMIT is
9.4 µs of an 82 µs random write. That is 11.5%, the most that sharing a
transaction across requests could save. At 128 KiB, COMMIT is mostly
writing the data pages (0.27–0.40 µs per KiB), which batching does not
remove.

## 2. The GETATTR per create

- **Target.** In every arm with `default_permissions`, all 2,001 GETATTRs
  were directories. All 2,001 named the parent of the latest CREATE or
  MKDIR. None named a descriptor. Mount 4 has 2,000 creates plus one mkdir.
- **Cause.** `nodefperm` (b3) is the test: GETATTR fell from 2,001 to 0, and
  create time from 223.0 to 169.7 µs per file (−24%). The LOOKUP callback
  doubled (12.6 → 26.8 µs), because the LOOKUP now meets the asynchronous
  RELEASE of the previous file on the owner lock.
- **Mechanism (Linux 6.12 `fs/fuse/dir.c`).**
  - A successful create calls `fuse_dir_changed(dir)`, which invalidates all
    basic attributes of the parent (`inval_mask`).
  - Under `default_permissions`, `fuse_permission` refreshes a directory
    whose mode, uid or gid is invalid. So the next path walk through the
    directory sends `FUSE_GETATTR`.
  - The CREATE reply carries only the child's attributes, so no reply field
    can spare this GETATTR.
  - The only ways to avoid it are to drop `default_permissions` (outside the
    contract) or to change the kernel.
- **The callback.** The GETATTR callback (30–34 µs) is about twice the
  LOOKUP callback. It is the request that most often meets the RELEASE of
  the previous file holding the Workspace lock, and its waits are the "over
  5 µs" waits. A parked waiter pays a futex wake-up of about 15–19 µs.

## 3. Arms

Compare only within a batch. "Digest" is µs per WRITE that the job spends on
the digest: the inline update, or, under `digestpar`, waiting for the
helper. "Waits" are owner lock waits over 5 µs during the creates.

| Arm | B3 µs | Seq MB/s | Seq WRITE cb µs | Digest µs | Rand µs/write | Rand cb µs | SQL/write | Create µs/file | GETATTR cb µs | Waits n / ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| b3-ctrl128 | 13.2 | 596.3 | 157.9 | 71.4 | 60.0 | 30.6 | 6.94 | 223.0 | 30.1 | 1,863 / 35.8 |
| b3-noresume | 13.1 | 601.3 | 159.3 | 71.6 | 56.8 | 28.8 | **5.94** | 215.2 | 28.0 | 1,772 / 31.0 |
| b3-digestpar | 13.4 | **808.1** | 117.4 | 24.5 | 54.5 | 30.1 | 6.94 | 214.0 | 26.2 | 1,511 / 27.9 |
| b3-spin | 13.1 | 615.7 | 155.8 | 71.4 | 58.9 | 30.8 | 6.94 | **196.1** | **16.5** | 1,028 / 6.8 |
| b3-nodefperm (diag) | 13.2 | 601.1 | 158.4 | 71.7 | 61.1 | 31.8 | 6.94 | 169.7 | — (0) | 1,593 / 26.9 |
| b3-trace128 (diag) | 13.2 | 606.5 | 157.4 | 72.1 | 57.8 | 31.9 | 6.94 | 234.7 | 35.8 | 1,964 / 44.0 |
| b4-ctrl128 | 18.4 | 441.9 | 221.1 | 99.9 | 77.5 | 43.1 | 6.94 | 259.6 | 31.4 | 1,676 / 32.3 |
| b4-all128 ¹ | 18.4 | **614.5** | 156.3 | 38.8 | 66.3 | 39.6 | 5.94 | 265.4 | 20.8 | 1,124 / 8.1 |
| b4-check128 ² | 18.4 | 381.7 | 271.7 | 147.9 | 71.8 | 39.4 | 6.94 | 268.9 | 30.9 | 1,590 / 29.9 |
| b4-ctrl512 | 18.3 | 525.1 | 796.6 | 396.4 | 70.7 | 38.0 | 6.94 | 265.9 | 31.3 | 1,809 / 32.0 |
| b4-all512 ¹ | 18.4 | **667.1** | 568.7 | 145.9 | 76.6 | 40.8 | 5.94 | 261.1 | 20.3 | 727 / 5.1 |
| b5-ctrl128 | 18.6 | 439.5 | 220.8 | 100.4 | 82.0 | 46.0 | 6.94 | 268.2 | 31.5 | 1,624 / 31.0 |
| b5-noresume | 18.5 | 463.0 | 213.7 | 101.1 | 73.2 | 42.8 | 5.94 | 224.6 | 27.9 | 1,404 / 25.9 |
| b5-digestpar | 18.5 | **560.9** | 168.8 | 44.3 | 75.4 | 40.2 | 6.94 | 264.2 | 30.1 | 1,504 / 27.6 |
| b5-spin | 18.4 | 462.8 | 210.1 | 99.7 | 70.8 | 39.5 | 6.94 | **191.9** | **16.6** | 420 / 4.0 |
| b5-all128 | 18.5 | **556.5** | 168.6 | 53.1 | 74.7 | 40.3 | 5.94 | 132.3 ³ | 15.7 | 168 / 1.5 |
| b5-ctrl512 | 18.5 | 558.7 | 767.0 | 401.2 | 78.0 | 42.3 | 6.94 | 273.7 | 33.3 | 1,765 / 34.9 |
| b5-all512 | 18.5 | **710.6** | 568.0 | 183.8 | 76.6 | 41.6 | 5.94 | 261.7 | 21.4 | 1,183 / 8.8 |
| b6-ctrl128 | 18.5 | 424.3 | 231.8 | 101.0 | 77.4 | 41.9 | 6.94 | 296.7 | 34.1 | 1,459 / 32.1 |
| b6-all128 | 18.6 | **494.4** | 186.7 | 38.2 | 89.8 ⁴ | 50.7 | 5.94 | 271.5 | 22.4 | 915 / 9.0 |
| b6-burn4 (diag) | 18.8 | 413.8 | 234.6 | 103.1 | 79.9 | 43.1 | 6.94 | 291.4 | 31.7 | 1,103 / 25.2 |
| b6-burn4-all128 (diag) | 18.6 | 533.1 | 177.2 | 54.3 | 80.0 | 42.1 | 5.94 | 288.6 | 22.1 | 382 / 2.9 |

The s1 and s2 arms (instrumentation shakedown) are in the receipts. In s2,
512 KiB / 1 wrote sequentially at 728.9 MB/s, against 572.3 MB/s at 128 KiB.

1. `all` = `noresume,digestpar,spin`.
2. `digestpar,digestcheck`. Every helper result equalled the inline update:
   2,048 sequential windows and 2,000 create writes, with 0 different. The
   check doubles the hashing, so its timings are not comparable.
3. An outlier. Callbacks were unchanged, but the time outside them fell from
   241 to 30 ms. It was not reproduced: b4-all, b5-all512 and b6-all were
   261–272 µs per file. A wall-time spread of about ±20% per create is
   normal here; b5-noresume, with one statement fewer, shows −16%.
4. Every stage of that phase, including untouched SQL, ran 20–30% slower.
   That is a CPU slowdown during the phase, not an effect of the switches.

## What worked

- **`digestpar` (sequential writes).** In every batch it gained 16–39% at
  128 KiB and 27% at 512 KiB.
  - 128 KiB: 596 → 808, 442 → 615, 440 → 561 (alone) or 557, and 424 → 494
    MB/s.
  - 512 KiB: 525 → 667 and 559 → 711 MB/s.
  - The WRITE callback falls by 41–65 µs per 128 KiB.
  - What remains is a wait of 24–54 µs per window, because the hash (100 µs
    at 1.3 GB/s) outlasts the cells and COMMIT.
  - BLAKE3 NEON is already linked (`blake3_hash_many_neon`); there is no
    feature to turn on.
- **`noresume`.** It removes exactly one statement per random write
  (6.94 → 5.94) and one per create (21 → 20), about 0.7–1.5 µs. The wall
  gain (1–5%) is within noise.
- **`spin` (creates).** In every batch it halved the GETATTR callback (30–34
  → 16–22 µs) and cut lock waits over 5 µs from 31–36 ms to 1.5–9 ms.
  Create time fell 12% in b3 and 28% in b5. In the combined arms it fell 8.5%
  (b6), or not at all (b4, before the 32 KiB helper floor, which gave 1 KiB
  writes a +8 µs hand-off).

## What did not work, or is not possible

- **Sparing the GETATTR:** impossible under `default_permissions` (§2).
- **An attribute cache for GETATTR:** not built. Its coherence needs every
  path that changes an inode row to invalidate the cache: kernel and SDK
  jobs, installs, revoke and maintenance. The upper bound is the callback
  left after `spin`, 16–22 µs of about 190–270 µs per create (≤ 10%).
- **Random 4 KiB writes have no cheap lever left:**
  - About 26 µs of SQL in seven statements.
  - About 9 µs for BEGIN and COMMIT, which only cross-request group commit
    could share (≤ 11.5%).
  - About 42–48% of the time is the kernel round trip.

## Risks

- **`digestpar`:**
  - It uses a second core for each appended window, and the Workspace lock
    is still held until the hash joins.
  - One process-wide helper thread serializes the digests of every
    Workspace. A product version needs a per-Workspace or bounded pool,
    and an owner decision on per-write CPU parallelism.
  - A helper panic panics the owner job.
  - On a failed COMMIT the hasher is still installed, as the inline in-place
    update would have been. This preserves the current semantics, but the
    inline update itself is not journaled on rollback (existing behaviour).
- **`spin`:** it burns up to 50 µs of CPU for each contended acquisition. CPU
  cost under several active Workspaces was not measured.
- **`noresume`:** none found. It skips only reads whose result cannot lead
  to a resume or a carried pin.

## Recommendation

1. **Take the digest off the serial critical path of sequential writes.**
   It is the largest lever: about 45–52% of a sequential WRITE callback.
   Prototype: overlap it with cells and COMMIT (+16–39%). A product form
   needs an owner decision on per-write CPU parallelism and on a bounded
   helper.
2. **Adopt `noresume`.** It is exactly equivalent and removes one statement
   per random write and per create.
3. **Consider a bounded spin before parking on the Workspace lock**, or
   shorten the asynchronous RELEASE job, which the next GETATTR or LOOKUP
   meets. Measure CPU cost with several Workspaces first.
4. **512 KiB requests** gave +18–27% sequential write in every control pair
   (s2 572 → 729, b4 442 → 525, b5 440 → 559). This is consistent with the
   main report.
5. **Leave for now:** the GETATTR itself (kernel behaviour), group commit
   (≤ 11.5% of a random write), and the kernel round trip.

## Checks and gaps

**Run:**

- Linux release build of the probe and every touched crate, `--locked
  --offline`.
- rustfmt on changed files.
- 25 probe runs, each once and each exit 0.
- `digestcheck` equality: 4,048 of 4,048.

**Not run:**

- Clippy: `NOT_RUN`. The image has no clippy component.
- Existing unit and mount tests: `NOT_RUN`.
- CPU-time accounting for `spin` and `digestpar`: `NOT_RUN`.
- Concurrent multi-Workspace load: `NOT_RUN`.
- Durable: `NOT_RUN`, disabled by the owner until explicit reauthorization.

**Limits of the evidence:**

- One sample per arm. Random-write and create wall times vary ±10–20%
  between identical settings; exact counts are the robust signal.
- The CPU slowed between b3 (BLAKE3 13.2 µs per call) and b4–b6
  (18.4–18.8 µs per call).
