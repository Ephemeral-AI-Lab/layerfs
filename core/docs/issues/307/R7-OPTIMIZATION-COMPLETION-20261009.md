# R7 optimization: completion of the fourth lead run

> **Status:** Measured result and hand-back, 2026-10-09. Local `main` at
> `ffea8f9d5` plus the commit that adds this file. Nothing is pushed.

The assignment's stop rule has two halves. The second holds: no ranked row
is left whose predicted saving exceeds the one-sample spread without a
forbidden item. **The first does not hold: seven of twelve cells are above
their A2 target.** They are listed below with the measurement that shows
why, and none is relabeled.

Evidence: the campaign [ledger](checks/r7-optimization-20261009/LEDGER.md)
(fourth-lead sections, decisions L4-1 to L4-10), the
[candidates list](checks/r7-optimization-20261009/CANDIDATES.md), the
[audit of the run](r7-open/audit-ca51a0c37.md) and the research reports
beside it. Every number here is from a receipt named in the ledger; one
sample per identity, never a best-of.

## Final table

Class B (fresh mount, warm daemon), arm L, Disposable profile, one
construction producer. Time is the measured command on the in-container
clock, the clock A2 was recorded on (decision L4-8). Two samples of
identical product work are shown because the spread between them is the
main limit on what can be claimed: receipts 1160–1211 at `1166e7a4b` and
1220–1271 at `ffea8f9d5`. Owner jobs and statement executions are equal in
both. "Start" is this run's first sample of each cell (652–703 at
`63c48d8dc`).

| Cell | Command | A2 ms | L ms, two samples | Ratio to A2 | Result | Statements, start → now | Owner service ms, start → now | Overlay bytes, start → now |
| --- | --- | ---: | --- | --- | --- | --- | --- | --- |
| C01 | create 1000 files | 183.4 | 299.4, 302.5 | 1.63, 1.65 | above | 61004 → 44004 | 162.9 → 117.1 | 557056 → 430080 |
| C02 | create 1000, stat 1000 | 965.0 | 822.1, 962.4 | 0.85, 1.00 | below, then at target | 63006 → 46004 | 175.4 → 130.6 to 149.2 | 557056 → 430080 |
| C03 | create 1000, remove them | 410.8 | 422.1, 549.1 | 1.03, 1.34 | above | 127399 → 102540 | 300.4 → 216.8 to 238.0 | 557056 → 430080 |
| C04 | 100 directories, 1000 files | 952.6 | 695.9, 853.5 | 0.73, 0.90 | **below** | 59076 → 42644 | 170.9 → 117.5 to 152.3 | 458752 → 376832 |
| C05 | that tree, then `find` | 949.6 | 765.2, 755.1 | 0.81, 0.80 | **below** | 90226 → 46637 | 230.5 → 134.8 | 458752 → 376832 |
| C06 | write 64 MiB | 78.6 | 113.5, 128.7 | 1.44, 1.64 | above | 36918 → 9766 | 147.9 → 62.7 to 68.5 | 76685312 → 68468736 |
| C07 | write 64 MiB, copy it | 171.6 | 256.3, 262.4 | 1.49, 1.53 | above | 78022 → 23133 | 320.2 → 137.2 | 153100288 → 136708096 |
| C08 | write 64 MiB, read it | 106.1 | 117.6, 126.5 | 1.11, 1.19 | above | 37009 → 9783 | 151.7 → 59.9 to 64.3 | 76685312 → 68468736 |
| C09 | read a 64 MiB base file | 106.2 | 49.7, 62.2 | 0.47, 0.59 | **below** | 42610 → 1055 | 74.8 → 3.6 to 4.4 | 278528 → 229376 |
| C10 | copy a 64 MiB base file | 178.2 | 175.5, 149.1 | 0.98, 0.84 | at target, then below | 83625 → 14406 | 262.6 → 76.3 to 91.6 | 76685312 → 68468736 |
| C11 | overwrite a 64 MiB base file | 85.8 | 114.5, 116.1 | 1.33, 1.35 | above | 36982 → 9760 | 147.3 → 63.7 | 76652544 → 68468736 |
| C12 | `git init`, 100 files, add, commit | 189.3 | 214.3, 205.1 | 1.13, 1.08 | above | 59504 → 24146 | 133.7 → 63.9 | 405504 → 335872 |

All 24 rows: verifier PASS, custody KNOWN_STOP, cleanup Gone, no gap.

- **Below A2 in both samples: C04, C05, C09.** C02 and C10 are below in
  one sample and within 3 ms of the target in the other: at the target,
  not established below it.
- **Above A2 in both samples: C01, C03, C06, C07, C08, C11, C12.**
- **Storage is lower in every cell than at the start of the run**: 1000
  small files −22.8 %, 64 MiB of data −10.7 %, the read cell −17.6 %.
  Store bytes are unchanged; allocated overlay bytes fell with the logical
  ones; the overlay's 256 MiB reservation is unchanged.
- **Every cell does less work**: statements −20 % (C03) to −98 % (C09);
  owner service −11 % to −95 %, taking the slower of the two samples.
- **Spread.** Two samples of identical work differ by up to 23 % with the
  regime unchanged (C04) and 30 % when it changes (C03). In C02 the same
  6002 jobs and 46004 statements took 830 and then 977 ms of container
  CPU. A single sample cannot establish a margin smaller than that.

## The cells above A2, and why

Three facts decide them. They are measured, and none is a product defect
this run could remove inside the rules.

**1. A mount that does no LayerFS work is already slower than A2 for C01
and C12.** The harness's passthrough arm (receipts 710–726) ran the same
commands through a daemon that does nothing: 285.5 ms for C01 and 244.7 ms
for C12 on the host clock, about 240 and 200 ms on the in-container clock
(the launch outside it is 40 to 45 ms; derived, not re-measured). A2 is
183.4 and 189.3. One synchronous request costs about 39 µs with zero
daemon work; C01 is 5002 requests.

**2. The scheduler runs in two regimes and A2's record fits the fast
one.** When the kernel keeps the caller and the daemon's receive thread on
one CPU (STACKED), a request round trip is about half the cost of when it
places them on two (SEPARATE). The product does not choose: it wakes no
other thread per request, and pinning, spinning and CPU limits are
forbidden. The label comes from the receive thread's context switches in
each receipt. Of the 24 final rows, 23 were SEPARATE and one MIXED (C03,
the faster of its two).
A2's C01 record costs 21.7 µs a request above native where every SEPARATE
sample here costs 42 to 44. The one STACKED C01 sample of this run (1047)
took 183.8 ms on the host clock, about 139 ms in-container, with the same
statements. A2 was recorded once per cell, with no regime observation.
[Report](r7-open/scheduler-regimes-47ca8f80e.md).

**3. For the 64 MiB cells the remaining owner time is SQLite storing the
bytes.** Per 128 KiB WRITE (C06, receipt 1187): 122 µs of owner service,
of which COMMIT is 42 µs (one `pwrite` per 4 KiB page, 33 pages) and the
row inserts 63 µs. Over 64 MiB that is about 22 ms and 32 ms.

| Cell | L ms | A2 ms | Passthrough floor, in-container (derived) | Room under A2 for all LayerFS work | Owner service now | Verdict |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| C01 | 299 to 303 | 183.4 | about 240 | none | 117 | not reachable in the SEPARATE regime |
| C12 | 205 to 214 | 189.3 | about 200 | none | 64 | not reachable in the SEPARATE regime |
| C03 | 422 to 549 | 410.8 | about 362 | about 49 ms | 217 to 238 | not reachable: 5018 transactions cost about 35 ms in COMMIT alone |
| C06 | 114 to 129 | 78.6 | about 57 | about 22 ms | 63 to 69 | not reachable: the page writes alone are about 22 ms |
| C07 | 256 to 262 | 171.6 | about 130 | about 42 ms | 137 | not reachable: two 64 MiB writes, about 44 ms of page writes |
| C11 | 115 to 116 | 85.8 | about 42 | about 44 ms | 64 | not reached; needs service under about 44 ms, where page writes and row inserts are about 54 |
| C08 | 118 to 127 | 106.1 | about 69 | about 37 ms | 60 to 64 | not reached; same arithmetic as C11 |

The floor column is one sample of the passthrough arm minus the launch;
it carries the same spread as everything else (C05 and C12 now run at or
below it), so "room" is an estimate. The direction is not in doubt for
C01, C12, C06 and C07; C08 and C11 are the two where a different payload
layout could change the verdict (see "What remains").

## What was done

Product commits, each one cause, with an exact count test before the
change and a sample after. All by implementer subagents in the main
checkout or in three parallel worktrees (owner decision L4-6), read and
merged by the lead.

| Area | Commits | Counter that moved |
| --- | --- | --- |
| READ, READLINK and OPEN are one owner visit; a base file's length is remembered inside the existing cache allowance | `de4789299`, `dbaad86a5`, `16fa316ee`, `8ac4b4c4c` | READ of base bytes: 6 jobs, 4 transactions, 68 statements → 1 job, 0, 2 |
| Unlink, FORGET and release finish in the job that causes them | `3558fb2be`, `989bfa887`, `36e2ec328`, `2a23b82f2` | C03 statements 127399 → 102540; FORGET of a removed file queues no maintenance item |
| Create path: one inode read and write per job, no lease row for a descriptor or lookup, one row per open descriptor, accounting on the namespace row only | `dc0c0a6f7`, `77a2c137a`, `e62685c59`, `a0ae8b522` | C01 statements per file 61 → 44 |
| Payload rows of up to 32 KiB, range reclamation, a delete trigger that reads lengths | `e555b771f`, `9f7037e04`, `95a9ddde0` | 128 KiB WRITE 32 statements → 8; 64 MiB overlay −10.7 % |
| OPENDIR, READDIR and RELEASEDIR are visits; one stored row per reply | `1fe0ed54b`, `39e4ce766` | a reply of 64 names: 6 jobs, 384 statements → 2 jobs, 10 |
| The old source-holding path, its two tables and an unused index deleted | `900231d0a`, `a591d05ff`, `96bb1835e`, `289cc23e4` | −697 production lines in the last three; 1000 small files 462848 → 430080 bytes |

Production LOC over the run: 187485 → 187480 (−5), by the pinned counter;
each commit carries its own line.

Harness and method: the measured command is also timed inside the
container (`1166e7a4b`); every receipt carries per-thread scheduler
observations and a regime label (`eb170b029`); the class B runner waits
for the warm-up Workspace's cleanup (L4-1); the fast path and batches are
written into `docs/general/benchmark_instruction.md` (L4-4, L4-5).

Tests added for the risk the changes created: an ordinary, a detached and
a forced unmount with a READ inside its Store read, on real mounts and on
the lane (`0fa417962`, seven cases, no product hook). No defect observed.

## Rules check

A read-only audit of every product change of the run found **no
violation** ([report](r7-open/audit-ca51a0c37.md)); its concerns were
either fixed afterwards or are listed under "Decisions for the owner".

- **Generic, not fitted.** No product line recognises a path, name,
  command, size or content. No cell parameter appears as a constant. The
  in-place overwrite path and the hard-link, rewind and fragmented-write
  paths are exercised by tests, not by any cell.
- **Storage.** No index, copy or reservation was added for speed; two
  tables and one index were removed. One pattern can store more than
  before; it is the first item under "Decisions for the owner".
- **Memory.** Nothing new is resident per file, handle, offset or
  directory.

| Structure | Bound | Scope | Grows with data |
| --- | --- | --- | --- |
| Base-file length memory | 264 bytes charged per entry inside the existing immutable-cache allowance (8 MiB in the harness: at most about 31,700 entries if it held nothing else) | one per daemon, shared by every Workspace; LRU with the objects | no |
| Row memory of one job | 4 inode rows, about 0.5 KiB | the running job; one per daemon | no |
| Payload bind and record | 2 × 32 KiB transient (was 2 × 4 KiB) | the one connection | no |
| READ visit reply | 128 KiB + 16 KiB, charged to the owner allowance | in-flight request; at most 17 per mounted Workspace | no |
| READDIR visit reply | about 84 KiB charged (about twice the former largest; the auditor's arithmetic) | in-flight request | no |
| Inline release | 6 step calls, 14 payload keys, 64 rows | the releasing job | no |

- **Constants introduced or changed**: `RUN_BYTES` 32768 (was one 4 KiB
  cell a row; L4-3), `RELEASE_CALLS` 6, `INLINE_PAGES` 8, maintenance page
  14 cells / 64 rows (the former literals), length charge 264. Past each
  bound the work is queued or served by the indexed path; none refuses.
  No queue, slot, timeout, thread or window constant was touched.
- **Forbidden mechanisms**: no thread, loop that waits, sleep, timer,
  retry, dependency, vendor edit, cfg flag, `fsync`, writeback or pinning.
- **Cache state.** Class B was the declared class throughout. The length
  memory survives a mount, so in class B the warm-up fills it: one length
  demand per cell in C09, C10 and C11, pinned cold by `read_cost.rs`. No
  class A oracle is prepared, so there is no class A sample.

## Gate at the tip

- Host, four packages, every suite: 124 of 126 binaries at `289cc23e4`
  (`355-index-host.txt`); the two failures are `complete_installed_roots`
  and `host_handoff` on their known prerequisites. The tip adds tests
  only; `store_read_drain` 4 of 4 on host (`356-merge-host.txt`).
- Linux, four packages, every suite, at the tip: 126 of 128 binaries
  (`357-tip-linux.txt`); the two failures are `complete_installed_roots`
  and `shared_processes`, known before this run and untouched by it.
- `fmt --check`, Clippy `-D warnings` on host and in Linux, boundary
  guard: pass for the four packages at `289cc23e4`, and again for the
  daemon package (the only one the tip changes) at the tip.
- There is no CI; nothing here claims one.

## Decisions for the owner

Each is a choice the lead made or left, with the alternative.

1. **A rewound directory handle can store more than before (L4-9).**
   Reply rows are keyed by the name a reply resumes after. If a name is
   added early in a large directory and one open handle is rewound and
   listed again, every later window is stored again: about N / 64 rows of
   up to 16 KiB for N names, where the old layout stored one row for the
   new name. It is bounded by the READDIR calls on that handle and
   released at RELEASEDIR; a directory opened for each listing (`ls`,
   `find`, `git`, `scandir`) never meets it and stores about 60 times
   fewer entries than before. Remedy: retire a handle's earlier replies
   when it is read from offset 0, which POSIX permits and which changes
   what an offset taken before a rewind means. Not done without your word.
2. **READ, OPEN and the directory reads hold no SQL custody (L4-2).** The
   dispatcher's drain is what holds an unmount back; the new tests stage
   it. Force beside a request that does not finish stops Retained at
   Requests and leaves the mount aborted and mounted, as the contract
   says. A future Store collector must not rely on `base_readers` to see
   an in-flight READ.
3. **`RUN_BYTES` 32 KiB (L4-3)** raises the payload row bound, the
   captured window and the captured reply charge (4608 → 36864 bytes)
   eightfold. Bounded, and storage fell; it is still a larger unit.
4. **Simplicity.** The auditor names three simpler forms: drop the
   in-place overwrite branch (L4-10: kept on its count, no cell measures
   it); finish an orphan inline only when it fits one page with no lower
   layer; drop the narrowed inode updates, which have no counter of their
   own.
5. **The dropped foreign key** from descriptor rows to `native_mount`
   (L4-7) and the constraints that went with the removed tables.

## What remains, and why it was not taken

| Row | Predicted | Why not |
| --- | --- | --- |
| Group commit (several jobs in one outer transaction) | about −20 ms a cell | A reply would precede the commit of its mutation. Changes what a reply means; flips no verdict |
| A larger SQLite page for the overlay | about −15 ms per 64 MiB | The page size is pinned by the overlay profile, and 38 tables and indexes hold at least one page each: storage rises in every cell that stores little. Computed, not measured |
| Payload bytes outside SQLite | the one change that could bring C08 and C11 under A2 | Contradicts the one-database rule of the product model. An owner decision, not a tuning step |
| Eliding all-zero data | large for these cells only | The cells write zeros; a gain that exists only for zeros is fitting |
| Larger write windows, a maintenance timer, forcing the same-CPU regime | — | Forbidden: a larger limit, a timer, pinning or spinning |
| A resident READ answered on the receive thread | about −9 ms in C09 and C10 | Inside the spread; lowers the ceiling for several readers; reverses a design line |
| `COPY_FILE_RANGE` in the daemon | about −58 ms in C10 | A new request kind; C10 is at or below A2 without it |
| `BATCH_FORGET`, statement-cache and profiling rows (E01, E02, E03) | small | Not worked |

## Open items

- **One sample per cell, large spread.** The verdicts "below" for C02 and
  C10 need more than one sample to be established; the rules allow one.
- **The floor was measured on the host clock in the SEPARATE regime** and
  is converted here by subtracting the launch. It was not re-measured on
  the in-container clock or in the STACKED regime.
- **A2 has no regime label.** Whether its records were STACKED is
  inferred from its per-request cost, not observed.
- **The 64 MiB cells write and read zeros only.** The overlay stores them
  in full, so its bytes and times are not a best case; Commit and Store
  figures for those files would be.
- **Not staged**: Force when the held read is released inside the drain
  window; the fenced halves of OPEN and READDIR; rollback of an inline
  release; `accounting_reference` with reply rows; a host test of a
  hard-link alias under a visit LOOKUP (not confirmed either way).
- **Harness**: `evidence_jobs.py:1101` (the E04 verifier, untouched)
  expects the payload tuple of one 4 KiB cell per row.
- **E cells** are unstarted: no oracle or configuration is prepared.
- **Housekeeping**: worktrees `layerfs-r7-b2`, `-b3`, `-b4` and their
  merged branches remain on disk; five retained sample containers of
  NOT_RUN rows are stopped, not removed.
