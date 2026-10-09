# Handoff 2: R7 after the verifier fixes; the optimization continues with the next agent

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

This continues [the first R7 handoff](HANDOFF-R7-BEAT-A2-20261009.md). That
document still holds the goal, the exit condition, the working method and
every standing rule; this one says where the run stopped, what is fixed, what
is open and how to pick it up. Read both. Where they differ on state, this
one is newer; where they differ on a rule, neither lifts the other.

## Owner direction, 2026-10-09 (second)

Relayed from a fork of the session and again through the session channel:

> write a note for the main agent to stop after fixing all of the verifier
> issues. and i want to handoff the rest of work of optimization to the next
> agent.

The previous lead therefore stopped after the directory link-count fix
below. The optimization loop itself is not finished: **no cell beats A2 yet.**
The earlier direction still describes the work you take over: research with
subagents, counts and big-O every round, aggressive batched changes ("if it
takes 100 steps, think about how to cut it into 10"), inside the product
boundaries (a Workspace per tool call; file count, file size and mutation
count bounded by resources only). The earlier delegation of owner decisions
("do not ask me question … you are the owner") was given to the previous
lead for that run; confirm with the owner before assuming it carries over.

## Unchanged: goal, exit, targets, rules

- **Goal and exit condition:** [first handoff, "The target"](HANDOFF-R7-BEAT-A2-20261009.md#the-target)
  and ["Exit condition"](HANDOFF-R7-BEAT-A2-20261009.md#exit-condition). Wall
  time below the A2 column of issue #306 in every cell, storage not worse,
  full suites on host and Linux, Clippy `-D warnings`, `fmt --check`, the
  boundary guard, then `R7-OPTIMIZATION-COMPLETION-<date>.md`.
- **Method:** one change, one sample, counts decide
  (["How to work"](HANDOFF-R7-BEAT-A2-20261009.md#how-to-work)); the
  iteration rule and the receipt fields every iteration reports are in
  [the benchmark instruction](../../../../docs/general/benchmark_instruction.md).
- **Forbidden and gates:** ["Still forbidden"](HANDOFF-R7-BEAT-A2-20261009.md#still-forbidden),
  ["Gates on every kept change"](HANDOFF-R7-BEAT-A2-20261009.md#gates-on-every-kept-change),
  ["Preservation"](HANDOFF-R7-BEAT-A2-20261009.md#preservation),
  ["Every commit"](HANDOFF-R7-BEAT-A2-20261009.md#every-commit), and the root
  and core `AGENTS.md`. In short: local `main` only, nothing pushed or
  published, no other worktree; Disposable Store profile only; no kernel
  writeback cache; permission checks unchanged; no third-party edit beyond
  the authorized fuser patches; no retry or replay; no test hook in product
  source; no workload, cache state, oracle or limit changed to turn a miss
  into a pass; every test command at most 120 s; receipts append-only;
  sample 212 stays FAIL; a Production LOC line on every commit. The four
  unrelated containers and the listed untracked files stay untouched.

A2 targets (command phase; C cells in ms, E cells in s):

| Cell | A2 | Cell | A2 | Cell | A2 | Cell | A2 |
| --- | ---: | --- | ---: | --- | ---: | --- | ---: |
| C01 | 183.416 | C09 | 106.178 | E04 | 4.324254 | E13/S | 58.601360 |
| C02 | 965.006 | C10 | 178.210 | E05 | 5.137499 | E14 | 16.288058 |
| C03 | 410.750 | C11 | 85.839 | E06 | 0.895150 | E15 | 0.640569 |
| C04 | 952.589 | C12 | 189.313 | E07 | 0.280095 | E16 | 0.226881 |
| C05 | 949.630 | E01/F | 0.025510 | E08 | 75.748050 | E17 | 3.344438 |
| C06 | 78.594 | E01/S | 0.025920 | E10 | 4.716923 | E18 | 4.346287 |
| C07 | 171.586 | E02 | 4.162990 | E11 | 6.423870 | E19 | 4.529085 |
| C08 | 106.146 | E03 | 21.399468 | E12/S | 42.201692 | E09 | none |

## Where things stand

Local `main`, nothing pushed. The commits of this run, oldest first:
`b313abdab` (step 4), `9306a9073` (5), `4398c013c` (6), `4dfec5c75` (7),
`e2521ae7c` (8), `8e2c280e4` (cell preparation), `6703a9ad9` and `fdc24ef3f`
(9), `889488dfb` (all-cells baseline), `c31c2a42c` and `5a9065b0b` (10),
`82c51a439` (the verifier fix), `5f379570e` (documentation), `a85551675`
(fix record), and the commit that adds this file. Overlay schema version is
22. The working tree is clean apart from the untracked files the first
handoff tells you to leave alone.

Latest sample of every C cell (class B, arm L, one sample each; all rows are
exploratory DIAGNOSTIC rows, not admission-eligible):

| Cell | Receipt | Commit | Verifier | Command ms | A2 ms | Ratio | Requests | Owner jobs | Statement attempts |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| C01 | 587 | `5f379570e` | PASS | 444.9 | 183.4 | 2.43 | 5001 | 5001 | 48003 |
| C02 | 531 | `fdc24ef3f` | PASS | 1156.5 | 965.0 | 1.20 | 6002 | 6002 | 93009 |
| C03 | 535 | `fdc24ef3f` | PASS | 782.5 | 410.8 | 1.90 | 9021 | 9108 | 149248 |
| C04 | 591 | `5f379570e` | PASS | 848.8 | 952.6 | 0.89 | 5342 | 5382 | 46225 |
| C05 | 595 | `5f379570e` | PASS | 912.2 | 949.6 | 0.96 | 5889 | 7264 | 72139 |
| C06 | 547 | `fdc24ef3f` | NOT_RUN | — | 78.6 | — | — | — | — |
| C07 | 551 | `fdc24ef3f` | NOT_RUN | — | 171.6 | — | — | — | — |
| C08 | 555 | `fdc24ef3f` | NOT_RUN | — | 106.1 | — | — | — | — |
| C09 | 559 | `fdc24ef3f` | PASS | 152.6 | 106.2 | 1.44 | 517 | 3082 | 36976 |
| C10 | 563 | `fdc24ef3f` | NOT_RUN | — | 178.2 | — | — | — | — |
| C11 | 567 | `fdc24ef3f` | NOT_RUN | — | 85.8 | — | — | — | — |
| C12 | 599 | `5f379570e` | PASS | 311.2 | 189.3 | 1.64 | 3436 | 4622 | 48643 |

- C02, C03 and C09 were not sampled again after step 10 and the fix; their
  rows are the `fdc24ef3f` baseline.
- C04 and C05 are below the A2 command time in one diagnostic sample each.
  That is not yet "beaten": the rows are exploratory and no storage
  comparison against A2's own bytes was made.
- The NOT_RUN rows report "pre-attempt cache predicate UNAVAILABLE: file
  identity changed during residency attestation" (candidate 3).
- **One-sample spread.** C01 with identical product code: 416.4 ms (516,
  `6703a9ad9`), 452.5 ms (527, `fdc24ef3f`); with identical counts after step
  10: 420.5 ms (579) and 444.9 ms (587). Treat a time change under about
  10 % as not established and decide by counts.
- Per created file in C01 now: 5 requests, 5 owner jobs, 3 write
  transactions, 48 statements, 61 executions, about 2,722 SQLite VM steps,
  175 µs of owner service, about 243 µs outside the owner. By family
  (receipt 579, µs per file): custody rows 37.5, Inode 34.0, COMMIT 32.3,
  Workspace 21.3, DirectoryEntry 10.1, Payload 6.8. EXPLAIN shows every hot
  statement as a primary-key seek; the cost is in the write programs
  (the inode upsert is 299 opcodes with 4 write cursors, a payload cell
  upsert 324 with 8), in 13 accounting-trigger runs per file and in the
  three commits.
- Mount and unmount in these samples: 5–12 ms and 4–6 ms.

## What steps 4 to 10 changed

All samples are C01:B:L, one each. The record of each step, with cause,
change, counts and checks, is in the
[ledger](checks/r7-optimization-20261009/LEDGER.md) under the step's heading.

| Step | Commit | Change | C01 command ms | Measured per 1000 created files |
| --- | --- | --- | ---: | --- |
| (before) | `87e234a62` | — | 2085.7 | 7000 requests, 15002 owner jobs, 278013 statement executions, owner wait 485 ms, service 1167 ms |
| 4 | `b313abdab` | A request's first step runs on its receive thread; an owner job runs on the submitting thread when the connection is free; wake-ups only for threads with work | 1204.6 | owner wait 485 → 126 ms; service 1167 → 910 ms |
| 5 | `9306a9073` | A transaction and its admission begin at the first writing statement; statement cache 48 → 256; fewer status calls; identities from a counter (schema 20) | 900.8 | service 910 → 575 ms; executions 278013 → 263010 |
| 6 | `4398c013c` | LOOKUP, GETATTR and every mutation are one owner visit that records no request source and reads base facts from resident objects | 585.9 | jobs 15002 → 7001; executions → 119005; reader grants 2001 → 0; service → 253 ms |
| 7 | `4dfec5c75` | FLUSH, FSYNC and FSYNCDIR answered `ENOSYS` once per connection | 479.2 | requests 7000 → 5001 |
| 8 | `e2521ae7c` | Reply tickets in engine memory (no `request` table, schema 21); a held turn serves one more queued job; a step made runnable by a thread is run by that thread | 455.1 | jobs 7001 → 5001; write transactions 5000 → 3000; executions → 101005 |
| 9 | `6703a9ad9`, `fdc24ef3f` | One receive loop per mount | 416.4 | owner wait 31.0 → 1.8 ms |
| 10 | `c31c2a42c` | Statement diet, first stage | 420.5 | attempts 88005 → 48006; executions 101005 → 61004; service 193 → 175 ms |

Owner decisions the previous lead took under the delegation, and where each
is documented. Review them; they are not the owner's own words.

| Decision | Where recorded |
| --- | --- |
| Reply tickets live in engine memory; a reply attempt needs no owner job; a capture or terminal cleanup that waited gets one `ReplySettled` job | ledger step 8; architecture notes [19](../../architecture/19-daemon-overlay.md), [21](../../architecture/21-daemon-owner.md), [77](../../architecture/77-native-mutation-coherence.md) |
| Visits record no request source; such requests do not count in `base_readers`; an install or revoke is not held back by a request between two visits | ledger step 6; notes [28](../../architecture/28-base-source-windows.md), [73](../../architecture/73-native-read-custody.md), [75](../../architecture/75-native-request-service.md), 77 |
| One receive loop per mount (`RECEIVE_SLOTS = 1`) | ledger step 9; notes 75, [76](../../architecture/76-native-mount-session.md) |
| FLUSH, FSYNC, FSYNCDIR reply `ENOSYS` once per connection | ledger step 7; notes 76, 77 |
| Statement cache capacity 48 → 256; Linux `fallocate` only when the tracked reserved tail is short | ledger step 5 |
| A submitting thread's turn serves one more queued job (`COMBINED = 1`) | ledger step 8; note 21 |
| The orphan probe is skipped until this engine has created an orphan (`orphan_seen`) | ledger step 10; note 19 |

## The verifier fix: directory link counts

Fixed in `82c51a439`; record in the ledger under "Verifier fix — F01".

- **Defect.** The product reported `st_nlink` 2 for every named directory;
  the native reference has 2 plus the number of child directories. C04, C05
  and C12 failed on that field only.
- **Fix.** The overlay inode row carries `subdirs`, the absolute number of
  child directories, maintained by mkdir, rmdir and directory renames in the
  publishing transaction (schema 22, no statement added). A base directory's
  count is derived outside the owner by listing it in bounded windows and
  reading the kinds of the listed serials, and remembered in a fixed memo of
  16,384 entries per daemon keyed by the directory's content root
  (`layerfs-workspace/src/base/links.rs`). An owner visit that does not find
  it stays undecided and writes nothing. The reply is 0 for a removed
  directory, otherwise 2 + `subdirs`. Commit stores nothing new; after
  capture and install the count is derived from the new base.
- **Result.** Receipts 591 (C04), 595 (C05), 599 (C12): verifier PASS; 587
  (C01): still PASS. Requests, owner jobs, write transactions and reader
  grants are equal before and after in all four cells. No verifier
  difference remains in any sampled cell.
- **Checks.** Host suites of overlay, workspace, fuse and daemon
  (`330-links-*`); Linux suites of fuse, overlay and daemon (`331-links-*`)
  with the new `mounted_links` and the restaged `mounted_commit` and
  `mounted_install`; Clippy host and Linux, fmt, boundary guard.
- **Documented in** architecture notes 19, 29, 30, 48, 73 and 77.

## Open candidates, ranked

The analyses behind these rows are in [`r7-open/`](r7-open/). They were
written by read-only research agents at `fdc24ef3f` (the statement trace at
`3ae26648f`); file:line references are at those commits, and everything they
mark as inference or estimate is exactly that. Time savings are projections
from counts unless a receipt is named.

1. **Measure the floor first (harness only).** About 243 µs per created file
   (5 requests) is spent outside the owner and no counter divides it. Source
   reading finds 0 futex syscalls and no second thread on the serial path,
   and only about 3–5 µs of user-space fixed cost per request, so the rest is
   believed to be vCPU wake-up latency (two wakes per synchronous request).
   If a wake costs 21 µs here, four synchronous requests and the shell
   already cost about 183 µs with zero daemon time, and C01 cannot be won by
   trimming alone. Two observations settle it: per-thread `schedstat` and
   context-switch counts of the daemon around the measured command (the
   runner already reads `/proc/1/status`; see the report for where), and one
   C01 sample of the passthrough arm `P` (`core/benchmark/r7-passthrough`) in
   this harness. Do this before spending on dispatch trimming.
   [per-request-floor-outside-owner-fdc24ef3f.md](r7-open/per-request-floor-outside-owner-fdc24ef3f.md)
2. **READ, OPEN, RELEASE and the directory requests as owner visits (H01).**
   C09: one 128 KiB READ is 6 owner jobs, 4 write transactions, 2 reader
   grants and 87 statement executions (the trace reproduces receipt 559 with
   zero residual). Design: one read-only visit, one hand-off, one grant; a
   bounded base-fact and length cache so a warm LOOKUP or GETATTR of a base
   file is one visit with no grant; OPEN as one visit; READDIR as one visit
   per page with one cookie row per page; FORGET that finishes small
   reclamation inline. Expected: C09 3,082 jobs → about 518; C05's directory
   traffic about 1,900 jobs → about 690; C03's 6,000 maintenance transactions
   mostly gone. Main risk named by the analysis: forced unmount and drain
   must account for an in-flight READ by the Fuse fence alone once the
   per-request rows go.
   [read-directory-path-trace-fdc24ef3f.md](r7-open/read-directory-path-trace-fdc24ef3f.md),
   [read-directory-path-summary-fdc24ef3f.md](r7-open/read-directory-path-summary-fdc24ef3f.md)
3. **Payload layout and closed-namespace reclamation (G01), with the five
   NOT_RUN cells.** A 4096-byte cell is 1.14 database pages (one overflow
   page each); a 128 KiB WRITE is 49 statements, 49 page writes and 311 µs of
   owner time; 64 MiB sequential is 159 ms of owner time of 256 ms. Reclaiming
   64 MiB after unmount is about 1,170 small transactions (130–170 ms
   estimated), which is why the class-B observer finds `overlay.sqlite` still
   changing and C06, C07, C08, C10 and C11 do not start. Proposed order:
   bind the borrowed slice; store all-zero whole cells above the cutoff as
   absence; reclaim by row window with one statement per step and idle
   bursts; whole-cell run rows up to 64 KiB (schema change, the only step
   with real correctness risk: captured-run reader, composition, orphan and
   garbage paths); then a 1 MiB write window. The runtime protocol already
   has a `cleanup KEY` readiness observation; waiting for it before the
   residency attestation is a readiness wait, not a predicate change, and
   would let those five cells be sampled today. Note for the owner: the
   projection under 70 ms for C06 holds for zero-filled data only; for
   incompressible data it is about 95–115 ms.
   [large-file-payload-path-fdc24ef3f.md](r7-open/large-file-payload-path-fdc24ef3f.md),
   scratch scripts in `r7-open/large-file-scratch/`
4. **Statement diet, the rest (E04b).** Landed: D1, D2, D4, D5, D6, D8, D9,
   D11 (step 10). Open: the job-scoped row memo (D3), layer columns in the
   inode read (D7), no `lease` rows of kinds 7 and 9 (D10), narrow accounting
   rows (D13), `native_file` folded into `file_handle` (D14), the CHECK
   rewrite. Step 10 showed that statement *count* is a weak proxy: 40 fewer
   statements bought 18 µs because the survivors are the expensive ones
   (COMMIT 10.8 µs, the fence 3.0 µs, Lease-family writes with their
   accounting triggers 4.2 µs). Prefer D13 and D10 (they remove trigger
   runs). Findings from the implementer: D10 changes `owner_rows`, which the
   mounted tests `mounted_drain`, `native_custody` and `native_mount` read as
   kernel custody, and `queue_closed` must then name the tables that hold
   custody; D15 (skip the cell pre-read) is not provable as specified because
   `put_cell` stores a cell at any aligned offset.
   [statement-diet-brief.md](r7-open/statement-diet-brief.md),
   [statement-diet-sql-trace-3ae26648f.md](r7-open/statement-diet-sql-trace-3ae26648f.md)
5. **Group commit (E05).** Three COMMITs are 32 ms per 1000 created files,
   each with a freelist pragma, two metadata calls and BEGIN. The overlay is
   created per daemon, never reopened and has one connection, so a commit
   protects nothing but the rollback of the current job: one outer
   transaction with a savepoint per job, committed by job count and bound
   bytes, before any non-grouped job (capture, composition, maintenance,
   close) and at unmount. To settle first: admission reads the committed file
   length and reserves `MUTATION_GROWTH` (128 MiB) per job, so a group needs
   either a proven per-job growth bound for native visits or a page-count
   read per job; a failed commit becomes an engine failure after replies were
   sent (equal to losing the daemon, which the Disposable overlay already
   allows — an owner decision to record); the in-memory journal must stay
   bounded; four tests in overlay `tests/transactions.rs` pin "one job, one
   transaction". Not researched beyond a reading of
   `database/connection.rs` and `allocation.rs`.
6. **Smaller levers, each with a count behind it.**
   - RELEASE's close (35 µs) runs in front of the next file's GETATTR while
     the client sleeps; replying first and running the close after the next
     request is served would move it off the critical path (needs a check of
     notes 75 and 76 for "RELEASE replied means close applied").
   - `Shared::pass` wakes the owner thread at the end of every job while a
     maintenance hint is set, not only when maintenance is due: one futex
     syscall per job and the collisions behind C03's 62 ms of owner wait.
   - GETATTR of the parent after every create or unlink could be answered
     from a revision-checked record with no owner job (5 → 4 jobs per file).
   - 1 MiB read and write windows (512 READs → 64 on C09), `FUSE_CACHE_SYMLINKS`,
     negative entries, no-open and no-opendir support: see the read-path and
     floor reports for what each needs. READDIRPLUS is not expressible with
     fuser 0.18.0 without an unauthorized patch.
   - Request services (`StorePorts`, `CanonicalClient`, a scoped Workspace)
     are built for every kernel request and used only on a base read.
7. **Retire the unreachable source-holding mutation path** (`apply_native`
   and the recorded-source mutation job). It is not reachable from the
   request service since step 6 and still compiles and is tested. Report the
   LOC honestly as a deletion of unreachable code, not a simplification.
8. **Directory link counts, follow-up.** The fix derives a base
   directory's count once per directory version per daemon: on first use,
   and again after an install that changed that directory (its content root
   is new). For a very large directory that is one O(entries) listing per
   Commit cycle. Two ways to remove it: carry the count of the retired local
   row across the install (the per-namespace table with a carry-over at
   generation retirement proposed in
   [read-directory-path-summary-fdc24ef3f.md](r7-open/read-directory-path-summary-fdc24ef3f.md)),
   or persist a derived count at Save in the Store, which is new cluster-one
   state and an owner decision. Also open: the memo capacity (16,384) is a
   choice, not a measurement; a base directory whose count is not remembered
   costs a second visit, unmeasured on a large base; the two new `ClientWork`
   counters (`directory_counts`, `directory_count_scans`) are not exported to
   the daemon diagnostics or the receipt.

## Audit against benchmark-fitting

The owner's rule, 2026-10-09: be aggressive and creative, but nothing may be
fitted to the benchmark — no warm-cache credit, no hard-coded or specialised
structure, no metadata or throughput limit that happens to suit the cells.
The previous lead's own audit of steps 4 to 10 and the fix:

- **No path recognises a workload.** A search of the product source of the
  four crates finds no benchmark name, cell id, command text or data pattern.
  No test hook, oracle, workload, limit or cache predicate was changed; the
  five NOT_RUN rows were left NOT_RUN rather than relaxing the predicate.
- **Every sample is cache class B**: a fresh mount after an identical warm-up
  on the same daemon. That is the declared class, not a hidden credit, but
  it means the daemon's immutable cache and prepared statements are warm in
  every number in this document. Classes A and C were never measured; say
  "class B" beside any time you quote.
- **Constants this run introduced or changed**, all fixed by source and none
  derived from a cell:

  | Constant | Value | Why it is not a data limit | Risk to watch |
  | --- | --- | --- | --- |
  | Receive loops per mount | 2 → 1 | One thread fewer | **Validated on serial commands only.** Concurrency is unmeasured; this is the change most exposed to having been tuned to the cells |
  | Jobs one held turn may also serve | 1 | Bounds a turn | Same: no concurrent sample |
  | Prepared-statement cache | 48 → 256 | Sized to the engine's fixed statement set (about 224 texts) | Must be re-derived if the statement set grows |
  | Resident object bytes read inside a visit; rounds per visit | 64 KiB; 4 | Bound one job; a larger need leaves the visit undecided and is read outside the owner | None known |
  | Directory-count memo | 16,384 entries | Fixed capacity; eviction only recomputes | Chosen, not measured; the cells have at most about 120 directories, so the cells cannot have tuned it |
  | Listing window of a derivation | 256 rows, 64 KiB | Streaming window, no total cap | None known |

- **Changes whose benefit shows only when facts are resident.** A visit
  decides in one job when the base objects it needs are in the immutable
  cache; otherwise it reads outside the owner and visits again. That is
  generic, but its measured gain is a class-B gain.
- **A trap in the open candidates.** Storing all-zero cells as absence would
  take the `dd if=/dev/zero` cells under their targets while incompressible
  data stays near 100 ms. It is a general, exact rule, but its benchmark
  effect is a sparse-file effect; report both numbers or it is fitting.
- **Not audited line by line:** the two subagent-written changes (step 10
  and the link-count fix).

## Fast iteration: what cost time, and how to avoid it

The method is in the [benchmark instruction](../../../../docs/general/benchmark_instruction.md),
written from this run with its measured times:

- [Loop time: three tiers](../../../../docs/general/benchmark_instruction.md#loop-time-three-tiers):
  a 9 s count tier after every edit, a 2 to 4 minute commit tier, the
  10 minute gate and the samples once per batch.
- [Finding the cost](../../../../docs/general/benchmark_instruction.md#finding-the-cost-scaling-round-trips-and-one-ranked-batch):
  the round-trip ladder with floors and unit costs, the ratios that expose a
  bad scaling factor, the ranked opportunity list, and how to apply it as
  one batch with one commit and one counter per row.
- [Root-cause analysis of a time](../../../../docs/general/benchmark_instruction.md#root-cause-analysis-of-a-time):
  the decomposition tree and the traps this run met.

Specific to this checkout:

- The sample needs a clean HEAD. Commit pending documents first; untracked
  files did not block the seal, a modified tracked file was not tried.
- Stop sample containers retained from FAIL or NOT_RUN rows before
  measuring; they hold idle daemons.
- Give an implementer a brief with the decided design, the rows, the proof
  required, the helper commands and stages that each leave the tree green.
  That is what allowed step 10 to be cut to its first stage cleanly when the
  owner changed direction.
- Docker can serve a stale test binary after a small edit (the rebuild
  finishes in two seconds and the old assertion still fails): `touch` the
  file, rebuild, use the next attempt number.
- "RECEIPT EXISTS" for the second `layerfs_daemon` unit binary is harmless.
- zsh expands unquoted globs in `grep --include=*.rs`; quote them.
- Peak resident memory is bimodal between identical samples.

## Not measured at all

- Every E cell (E01–E19). They need closed oracles and configurations like
  the C cells got in receipts 400–487; E07 and E08 need `node` in the image.
- Cache classes A and C; arms N (native) and P (passthrough) beyond the
  oracle references; mount and unmount as priced phases.
- Commit in any cell (the `commit` phase is UNAVAILABLE in every receipt)
  and every concurrency cell.
- **Edit-size scaling of a large-file edit.** The owner asked whether write
  and Commit cost follow the edit size rather than the file size. By source
  they should: a write stores only the 4 KiB cells it touches, capture seals
  a frontier without copying payload, and Commit builds an existing file
  through Content's localized edit entry point
  (`apply_indexed_edits_view_backed`, called from
  `layerfs-workspace/src/construction/captured/owner.rs`) and a new file
  through `construct_runs`. No R7 receipt shows it. Owed: one fixed-size edit
  to a base file at three sizes, command and Commit both timed, with counts.
- The handbook's 1×/2×/4× scaling check of the request path (the cost test
  compares the 10th and the 4,000th file by counts only).
- C06, C07, C08, C10 and C11 have no sample (see candidate 3).

## Mechanics

**Helper scripts live in `core/target/`, which is git-ignored. They are lost
if `core/target` is cleaned; copy them out first if you must clean.**

| Script | Purpose |
| --- | --- |
| `r4-build.sh check\|test-no-run\|clippy\|fmt\|guard <packages>` | Host cargo with the pinned toolchain (`+1.85.1 --locked`), bounded, under the checkout lock |
| `rx-run.sh <track> <attempt> <package> <binary> [filter]` | One bounded host run of one test binary with an append-only receipt (`RX_STAGE=r7-optimization-20261009`) |
| `rx-linux.sh build\|run\|clippy …` | The same in the pinned Linux image (FUSE device, `SYS_ADMIN`); `build` verifies source hashes inside the container first |
| `rx-count.py staged\|committed [out.json]` | Production LOC of first parent against the staged or committed tree with the pinned counter |
| `r7-iterate-C01.sh <n>` | One C01:B:L iteration at a clean HEAD: source seal, two release builds, build provenance, volume, clone, configuration, one sample (8 receipts from `<n>`) |
| `r7-iterate-cell.sh <cell> <n> [<build-provenance.json>]` | The same for any C cell; with a provenance sealed at this HEAD only the 4 cell receipts |
| `r7-iterate-all.sh <n> [cells…]` | One build and every C cell (52 receipts) |
| `r7-summary.py <receipt.json>` | Status, phases, opcode and job counts, statements by family, owner wait and service, storage |
| `r7-statements.py <receipt.json>` | Per-family attempts, runs, VM steps and time |
| `r7-configure-cell.py`, `r7-prepare-oracle.sh`, `r7-cell-oracles.json`, `r7-reference-config.py`, `r7-package-reference.py`, `r7-seal-build*.py`, `r7-snapshot-binaries.py`, `r7-reuse-build-provenance.py` | Cell preparation: native reference, closed oracle, packaged code bundle, sealed configuration (used by the iterate scripts) |
| `r4-locked.pl` | The checkout lock every helper takes; a busy lock is not an attempt |

A full Linux suite is one bounded `rx-linux.sh run` per test binary; the
previous lead drove that from a throwaway loop over the list `build` prints.
Docker can serve a stale test binary after a small edit: if a rebuild
finishes in two seconds and an old assertion still fails, `touch` the file,
rebuild, and use the next attempt number.

**State outside the repository.**

- `/tmp` does not survive a reboot. Each cell's closed oracle and packaged
  code bundle is under `/tmp/layerfs-r7-<cell>-packaged-code-20261009-3ae26648f-r<N>/`
  (C01: `/tmp/layerfs-r7-C01-packaged-code-20261009-<commit>/`), its native
  configuration under `/tmp/layerfs-r7-<cell>-reference-config-…/`;
  `core/target/r7-cell-oracles.json` indexes them with hashes. Prepared
  inputs: `/tmp/layerfs-r7-prepared-inputs-20261009` (3.5 GiB),
  `/tmp/layerfs-r7-full-fixture-20261009`, the sealed Stores
  `/tmp/layerfs-r7-full-sealed-20261009.sqlite` and `…cut-full-sealed…`, and
  `/tmp/layerfs-r7-representable-cuts-20261009` (7.1 GiB). If they are gone,
  receipts 400–487 and the ledger's preparation sections are the recipe.
- Docker volumes: masters `layerfs-r7-empty-master-20261009-2ac7cc762` (C01–C08,
  C12) and `layerfs-r7-big-master-20261009-b8d76c0a3` (C09–C11); native
  references `layerfs-r7-native-<cell>-reference-…`; one clone per sample
  `layerfs-r7-<cell>-L-<commit>-20261009`. **Never remove a master volume or
  anything that belongs to a failed attempt.** Clone volumes and stopped
  sample containers of committed, successful samples may be removed; record
  it in the ledger when you do.
- Pinned image `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.
- Sample containers retained by the runner for the earlier FAIL and NOT_RUN
  rows are stopped (`docker stop`), not removed: C04 `639c0b705e12`, C05
  `79e9319bbcf0`, C06 `418c877cf0bb`, C07 `3aa5f55771fb`, C08
  `b875beacbf42`, C10 `716925accad5`, C11 `bc65a9ccd3ea`, C12
  `bbbc4b4c5610`. They belong to failed or unrun attempts, so keep them
  until the owner says otherwise. Exited containers of successful samples
  may be removed. The four unrelated running containers named in the first
  handoff were not touched.

**Receipts.** Append-only under `checks/r7-optimization-20261009/`. Sample
groups are numbered; the next free number is 600. Test receipts use named tracks
(`320-diet-*` and `321-diet-*` for step 10, `330-links-*` and `331-links-*`
for the link-count fix); a rerun of the same binary takes the next attempt
number.

## Known unrelated test failures, and what was verified less than it should be

Failures that are missing preconditions, not regressions:

- daemon `complete_installed_roots` on host and Linux ("explicit closed
  preparation for clone setup");
- daemon `host_handoff` on host (needs a build-listed Linux binary);
- daemon `shared_processes` on Linux (needs a named volume).

The second `layerfs_daemon` unit binary shares its receipt name with the
library's; "RECEIPT EXISTS" for it is harmless.

Verified less than it should be:

- **Steps 10 and the fix were written by subagents** to the previous lead's
  briefs. The lead read the statement, fence, memo and projection changes and
  ran every suite, but did not read all 27 and 56 changed files line by line.
- **Workspace suites ran on host only** for step 10 and the fix (the crate is
  platform-neutral; the daemon's Linux suite exercises it through mounts).
- **No cell with a large base was sampled after the fix.** Every C cell that
  ran uses a small or empty base, so the derivation and the extra visit for
  base directories have never been timed. The first `find`- or `git
  status`-style E cell will show it.
- **C02, C03 and C09 were not resampled** after step 10 and the fix.
- **Concurrency.** Steps 4, 8 and 9 were tuned on a serial client. No
  concurrent cell was sampled; the "held turn serves one more job" path has
  no public observable.
- **Storage against A2.** Overlay allocated bytes include a fixed 256 MiB
  reservation past the end of the file (`MUTATION_GROWTH` +
  `CLEANUP_HEADROOM` in `layerfs-overlay/src/database/allocation.rs`). It is
  disk, not memory, and predates R7. Whether issue #306's storage column
  counts allocated or logical bytes was not checked.
- **Daemon peak resident memory** is bimodal across samples (14–48 MB) with
  no trend by step and no explanation.
- **Scaling.** Evidence that costs do not grow with files is the cost test
  (10th against 4,000th file, by counts) and source reading; the 1×/2×/4×
  sample series is owed.
- **`docs/general/benchmark_instruction.md`** (the iteration rule and the
  receipt fields to report) was written in this run and not cross-checked
  against the optimization guide and handbook; if they differ, they govern.
- **Issue #314.** Its four priorities (keep daemon owners alive, coherent
  kernel caching, runnable concurrent requests, release without discarding
  shared owners) are in the product and their mounted tests pass on Linux;
  of its optional list only FLUSH elision is applied. Its proof checklist
  was not gone through box by box.

## Memory and notes

`MEMORY.md` of the previous lead's session directory records the working
rules that were learned the hard way (pinned host toolchain, the two-minute
test ceiling, native mount proof mechanics, small steps). They are also in
the guides; nothing in memory overrides this document or the owner.
