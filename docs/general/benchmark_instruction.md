# Benchmark instruction: the optimization iteration and the metrics it produces

> **Status:** Current general guide.

How an agent runs one optimization iteration against the mounted product
(cluster two: FUSE, daemon, Workspace, overlay) and which numbers every
iteration must report. It was written from the R7 run
([ledger](../../core/docs/issues/307/checks/r7-optimization-20261009/LEDGER.md),
[second handoff](../../core/docs/issues/307/HANDOFF-R7-BEAT-A2-20261009-2.md)).

This document adds no rule and lifts none, with one exception stated where
it applies: the owner's direction of 2026-10-09 to rank the opportunities and
apply them as one batch per iteration. Cache state, setup reuse, budgets,
sampling and verdicts are governed by the root [AGENTS.md](../../AGENTS.md)
sections 1 to 3, the [measurement workflow](agent-measurement-policy.md), the
[benchmark rules](benchmark_rules.md) and the owning family contract; report
layout for a registered benchmark family is governed by the
[report template](../../benchmark_agent_report.md). The binding
performance-engineering rules are in the [optimization guide](optimization-guide.md)
and the fuller procedure, counter groups and scaling checks in the
[optimization handbook](optimization-handbook.md); this document is the short
per-iteration checklist with the exact receipt fields. Where this document
and one of those differ, they govern.

## The iteration rule

One iteration is one ranked batch of separately counted changes, one commit
per change, and one sample per affected cell at the tip of the batch.

1. **Start from a receipt, not from an idea.** Take the latest sample of the
   cell and fill the metrics table below per unit of work (per created file,
   per READ, per 4 KiB written, per directory entry).
2. **Model the cost from source.** For each kernel request of the command,
   list its owner jobs, each job's statements and transactions, each Store
   read and each thread hand-off, with file and line. The model is accepted
   only when its totals reproduce the receipt's counts; state any residual.
3. **State the cause as a count and predict the count after the change**
   before editing: "the command spends N of X per unit because …; expected M
   from a model in which …". Give the order of growth for files, bytes and
   mutations; a change that adds a term depending on any of them is refused.
4. **Change one batch, one commit per cause.** Owner direction 2026-10-09:
   do not spend one full proof and one sample per candidate. Build the batch
   as described under [one ranked batch](#finding-the-cost-scaling-round-trips-and-one-ranked-batch):
   every change in it has its own cause sentence, its own predicted counter
   and its own commit, so it can be judged and reverted alone. Record every
   design decision a change relies on and where it is documented.
5. **Prove it before timing it.** Tests for the changed behaviour and for
   every check a removed step used to make, at the tier the
   [loop-time table](#loop-time-three-tiers) gives for each moment: the count
   tests after every edit, the changed scope at every commit, and once at
   the tip of the batch the changed packages' suites on host and in the
   pinned Linux image, Clippy with warnings denied on both, `fmt --check`
   and the boundary guard. Every test command has a wall limit of at most
   120 s.
6. **Sample once, at the tip.** One sample per affected cell from a clean,
   sealed HEAD, into a fresh append-only receipt. No best-of, no repeat of an
   unchanged identity. A failed, incomplete or unrun selection keeps its
   receipt and its verdict.
7. **Decide by counts first, change by change.** Keep a change when its
   predicted counter moved as predicted, the verifier passes, no gate below is broken and
   storage is not worse. Command time decides only when it moves by more than
   the measured one-sample spread (about 10 % in R7: identical code gave
   416.4 ms and 452.5 ms). If the counter did not move, the model was wrong:
   revert or explain, and record it either way.
8. **Record the iteration** in the campaign ledger: cause, change, commit,
   decisions used, checks with receipt names, the before/after table, the
   verdict, and what remains per unit of work.

Gates on every kept change:

- No scan, sort, automatic index or statement re-prepare on a request path.
- No resident container that grows with files, bytes, directories or
  mutations; state lives in indexed rows or in fixed-capacity caches.
- No limit, queue, buffer, cache lifetime, thread count or timeout raised as
  the fix.
- Permission checks, kernel writeback (off), persistence profile and public
  contracts unchanged.
- No retry or replay; one attempted operation.
- No path that recognises a benchmark command or its data.
- Storage (Store and overlay, logical and allocated) not worse for the cell.

## Metrics every iteration produces

All of these come from the sample receipt
(`<n>-<cell>-<class>-<arm>-sample-<commit>/sample/receipt.json`). Counted
intervals are recorded for the measured command and for the measured mount,
each with a before and an end snapshot. Report raw nanoseconds and bytes; add
per-unit values beside them.

### Outcome

| Metric | Receipt field |
| --- | --- |
| Row status, verifier, custody, cleanup, completion gaps | `row_status`, `verification_status`, `custody_status`, `cleanup_status`, `completion_gaps` |
| Phase wall time: mount, command, output streams, unmount, cleanup, verifier, commit | `phases.<name>` |
| Priced command against its limit | `complete_command_ns`, `complete_command_wall_stop_ns` |
| Cache class and whether the measured phase demanded Store objects | `cache.class`, `cache.object_demands`, `cache.residency` |

### Kernel requests

| Metric | Receipt field |
| --- | --- |
| Requests per FUSE opcode (39 counters) | `opcodes.by_opcode` |
| How requests ended: handed off, answered inline, refused, forget units, terminal, unadmitted | `opcodes.disposition` |

### Dispatch and threads

| Metric | Receipt field |
| --- | --- |
| Received, admitted, completed, receive units, steps, wake-ups, parked, retained | `mount` |
| Workers configured, entered, live; queued, running, parked | `dispatch` |
| Receive loops configured, created, entered, exited, joined | `loops` |

A serial command should show one step per completed request and zero
wake-ups.

### Owner jobs

| Metric | Receipt field |
| --- | --- |
| Jobs per class: Read, Mutation, Lifecycle, Source, Capture, OperationRecord | `completed` |
| Queue wait and service time, total and per class | `owner.queue_wait_ns`, `owner.service_ns`, and the per-class arrays the summary helper decodes |
| Admitted, outstanding, queued, peak queued | `owner` |
| Maintenance jobs, rows, time and bytes during the interval | `owner.maintenance_*` |
| Peak request bytes held; scheduler state bytes | `owner.peak_credited_bytes`, `owner.scheduler_bytes` |

### SQL, per statement family

Families: Startup, Begin, Commit, Rollback, Workspace, Inode, DirectoryEntry,
Payload, Frontier, Capture, OperationRecord, Lease, Explain, Reclaim. Request
work is `sql_foreground`; background work is `sql_maintenance`.

| Metric | Field in each family record |
| --- | --- |
| Statements issued | `attempts` |
| Executions including trigger and cascade sub-programs | `executions` |
| SQLite VM steps | `vm_steps` |
| Elapsed time | the statement-work observation (decoded by the statements helper) |
| Rows returned; rows changed, direct and total | `rows_returned`, `direct_rows_changed`, `rows_changed` |
| Bytes bound and returned | `bound_bytes`, `returned_value_bytes`, `returned_blob_bytes` |
| Must be zero on a request path | `fullscan_steps`, `sorts`, `autoindex_rows`, `reprepares` |

Write transactions are the Commit family's `attempts`.

### Store and immutable cache

| Metric | Receipt field |
| --- | --- |
| Reader grants, reader queue wait, peak outstanding | `reader` |
| Object batches and ids, length batches and ids, serial reservations | `store` |
| Cache hits, misses, evictions, upstream batches, authenticated bytes | `cache` (the counter block inside the interval) |

### Payload

| Metric | Receipt field |
| --- | --- |
| Cells written, partial cells, input bytes | `payload_foreground.write_cells`, `partial_write_cells`, `write_input_bytes` |
| Bytes copied and zeroed for cells and read windows | `cell_copy_bytes`, `cell_zeroed_bytes`, `read_local_copy_bytes`, `read_window_zeroed_bytes` |

### Disk admission, pages and stored rows

| Metric | Receipt field |
| --- | --- |
| Preallocation calls, bytes requested, refusals, file metadata observations | `allocation_work` |
| Overlay logical, allocated and reserved-tail bytes | `allocation_state` |
| Database pages, free pages | `pages` |
| Rows per table kind (inodes, names, payload cells and bytes, custody, orphans, records, reply tickets) | `stored_counts` |

### Storage and memory

| Metric | Receipt field |
| --- | --- |
| Store logical and allocated bytes | `observations.store_logical_bytes`, `store_allocated_bytes` |
| Overlay logical and allocated bytes | `observations.overlay_logical_bytes`, `overlay_allocated_bytes` |
| Daemon peak resident memory | `observations.daemon_vmhwm_bytes` |

Overlay allocated bytes include a fixed 256 MiB reservation past the end of
the file; compare logical bytes for growth and allocated bytes for the
storage verdict.

### Derived values

- Every count divided by the unit of work, and jobs per request, statements
  per job, VM steps per statement.
- Time outside the owner: command time minus owner queue wait minus owner
  service, per request.
- Time per statement and per VM step, per family.
- Ratio to the target and, once a passthrough-arm sample of the same cell
  exists, ratio to that floor.

## The iteration report

One before/after table per sampled cell, in the ledger. Keep both counts and
times in it: a count without its time misleads (in R7, 45 % fewer statements
bought 18 µs because the cheap ones went).

| Measure | Before `<receipt> at <commit>` | After `<receipt> at <commit>` | Change |
| --- | ---: | ---: | ---: |
| Row / verifier | | | |
| Command ns; ratio to target | | | |
| Mount / unmount ns | | | |
| Requests, by opcode | | | |
| Owner jobs, by class | | | |
| Write transactions | | | |
| Statements / executions / VM steps | | | |
| Owner service ns / queue wait ns | | | |
| Command minus owner wait and service | | | |
| Wake-ups; steps per completed request | | | |
| Reader grants; Store batches | | | |
| Maintenance jobs during the command | | | |
| Scans / sorts / auto-indexes / re-prepares | 0 | 0 | 0 |
| Store logical / allocated bytes | | | |
| Overlay logical / allocated bytes | | | |
| Daemon peak resident memory | | | |

Below the table: the verdict (kept, reverted, or kept on counts with the time
not confirmed), the cause and the predicted count restated against the
result, the remaining cost per unit of work, and anything the sample did not
establish.

## Loop time: three tiers

Run the smallest tier that can refute the edit just made. The full gate runs
once per batch, not once per edit. Wall times are from the R7 receipts
(`330-links-*`, `331-links-*`, samples 580 to 591) and one run of the count
tier on 2026-10-09; they are upper bounds per binary summed, on this host.

| Tier | When | What runs | Wall |
| --- | --- | --- | ---: |
| Count | after every edit | `check` of the edited package; the six exact-count binaries; the one behaviour binary of the edited path, filtered to the test | 9 s for the six binaries with nothing to rebuild, plus the compile of the edit |
| Commit | once per commit of a batch | host suite of each package whose source changed; in Linux only what the host cannot run; `fmt --check` | 2 to 4 minutes |
| Gate | once per batch, at its tip | four packages on host and in Linux, Clippy on both, `fmt --check`, guard | about 10 minutes |
| Sample | once per batch, after the gate | seal, release build, one sample per affected cell | 23.5 s for C01 as the first cell (12.4 s of it the release build), 6.5 s more for C04; a longer command adds its own time |

**Count tier (about 30 s).** The receipt's per-request statements,
executions, transactions and jobs are reproduced in-process, with no mount,
no container and no release build, by tests that assert the exact numbers:
overlay `costs`, `compound`, `transactions`, `native_visit_fence`; workspace
`native_visit_cost` (the five jobs of one created file, family by family);
daemon `job_cost`. A predicted count is therefore checked here, seconds after
the edit, and the sample only confirms it.

```bash
RX_ONLY=costs,compound,transactions,native_visit_fence,native_visit_cost,job_cost \
  python3 -B core/target/rx-suite.py host <summary-file> layerfs-overlay layerfs-workspace layerfs-daemon
```

Restage a count test with the true new number in the same commit; never
loosen it to a range. If the path being changed has no such test, write it
first (two sizes, equal per-unit `DatabaseWork`): it is the fast loop for
every later edit of that path.

**Commit tier (2 to 4 minutes).** What each piece costs:

| Piece | Binaries | Test time | Wall |
| --- | ---: | ---: | ---: |
| overlay, host | 26 | 12.3 s | ≤ 49 s |
| workspace, host | 27 | 13.1 s | ≤ 63 s |
| daemon, host | 58 | 37.6 s | ≤ 162 s |
| daemon, Linux, all | 58 | 81.0 s | ≤ 151 s |
| daemon, Linux, the 21 binaries that run nothing on host | 21 | | ≤ 84 s |
| overlay, Linux | 26 | 5.5 s | ≤ 34 s |
| fuse, Linux | 4 | 0.2 s | ≤ 6 s |

- Most of the wall is per-binary start (lock, container, process), not
  tests: 70 of 151 s for the daemon in Linux, 124 of 162 s on host. Select
  binaries by name; do not run a package to reach one test.
- 21 daemon binaries run zero tests on host and are the only mounted
  coverage: `forced_producers`, `forced_smoke`, `forced_unavailable`,
  `forced_unmount`, `mounted_commit`, `mounted_commit_failures`,
  `mounted_concurrency`, `mounted_cycles`, `mounted_drain`,
  `mounted_failure_scope`, `mounted_fusectl`, `mounted_install`,
  `mounted_links`, `mounted_parking`, `native_application`,
  `native_coherence`, `native_custody`, `native_mount`,
  `native_mount_routes`, `native_mutation`, `observed_application`. Run
  these in Linux (pass the list as `RX_ONLY`) and the other 37 on host; the
  two full daemon suites then overlap on nothing that matters at this tier.
- A typical overlay-plus-workspace commit is 49 + 63 + 84 s. A commit that
  touches only one request path needs its package on host plus the mounted
  binaries that exercise that path.
- The slow binaries are `mounted_drain` 21 s, `mounted_concurrency` 13 s,
  `complete_installed_roots` 12 s, `mounted_install` 7 s; the next eight are
  3 to 5 s each. `complete_installed_roots` fails on both sides for a reason
  unrelated to this work, and `host_handoff` (host) and `shared_processes`
  (Linux) likewise: they belong to the gate, where a change in how they fail
  is seen, not to this tier.
- Before changing a constant or a count, search the tests for it and restage
  them in the same commit. In R7 a changed receive-loop count broke eight
  test binaries and a moved reply ticket three, each found one suite run at
  a time.
- Run `fmt` before the suites. Build with `--no-run` first.

**Gate (about 10 minutes).** The run walls above sum to 465 s; fuse on host,
workspace in Linux, the two test builds and Clippy (16 to 18 s for four
packages in Linux) were not timed separately and make up the rest. The
link-count fix ran the daemon host suite three times and the workspace suite
twice for one change: more than six minutes of repeated full runs (2 × 162 s
and 63 s) where the count and commit tiers would have caught each restaged
assertion.

**Sample.** One release build serves every cell at that HEAD
(`r7-iterate-all.sh <n> [cells]`). What makes a sample step repeat: a HEAD
that is not clean, sample containers retained from failed rows still holding
idle daemons, and a stale test binary served by Docker after a small edit
(`touch` the file, rebuild, next attempt number).

No tier replaces the one above it, and none lifts the 120 s ceiling on a
test command.

## Finding the cost: scaling, round trips and one ranked batch

All of this is read from one existing receipt and from tests that take
seconds. It needs no new sample.

### The round-trip ladder

Write one row per level, per unit of work, with the floor beside it. Example:
C01, receipt 579, per created file.

| Level | Per file | Floor | Receipt field |
| --- | ---: | --- | --- |
| Kernel requests | 5 | the requests the kernel must send for the command's system calls; a passthrough-arm sample of the cell shows them | `opcodes.by_opcode` |
| Thread hand-offs | 0 | 0 for a serial caller | `mount`: wake-ups, steps minus completed |
| Owner jobs | 5 | at most 1 per request that needs the database; 0 for one answerable from validated memory | `completed` |
| Write transactions | 3 | 1 per request that mutates; 0 per read | Commit `attempts` |
| Statements | 48 | one per row the operation must read or write; list the rows | `attempts` |
| Executions | 61 | statements plus the triggers that are required | `executions` |
| VM steps | 2,722 | the opcode count of each plan (`EXPLAIN`) with no loop | `vm_steps` |
| Reader grants, Store batches | 0 | 0 on a resident path; 1 batch per request on a cold one | `reader`, `store` |
| Bytes copied per byte written or read | | 1 | `payload_foreground` |

A level above its floor is a round-trip candidate. Its size is the excess
count times the unit cost of that level. Unit costs from the same receipt
(re-derive them from the current one):

| One | Costs |
| --- | ---: |
| request, outside the owner | 49 µs (243 µs over 5 requests) |
| owner job, service | 35 µs on average |
| write transaction, the COMMIT alone | 10.8 µs |
| statement | 3.0 µs on average; custody 4.2 µs, fence 3.0 µs |
| VM step | 53 ns |

One removed request is worth about a job plus 49 µs, or 28 average
statements. That is why the [handbook's direction order](optimization-handbook.md#8-direction-order-when-several-apply)
starts with round trips and requests.

### The scaling factor

A bad factor is a per-unit count that depends on something other than the
unit. Three checks, cheapest first.

1. **Ratios inside one receipt** that should be constants:

   | Ratio | Bad when | Meaning |
   | --- | --- | --- |
   | `vm_steps` ÷ `executions`, per family | far above the plan's opcode count | a loop over rows inside one statement |
   | `rows_returned` ÷ `attempts` on a point read | above 1 | a range where a key was meant |
   | `fullscan_steps`, `sorts`, `autoindex_rows`, `reprepares` | not 0 | a scan, a sort, a missing index, a statement compiled again |
   | `executions` − `attempts` | grows per unit | trigger or cascade work per row |
   | `cell_copy_bytes` ÷ `write_input_bytes`; `read_local_copy_bytes` ÷ bytes read | above 1 beyond one cell at each edge | copy amplification: cost follows the window or the file, not the edit |
   | `store.object_ids` ÷ `object_batches` | about 1 with many batches | point reads where a batch was possible |
   | reader grants ÷ requests | above 1 | one request re-entering the Store |
   | cache `evictions` during the command | not 0 | the working set exceeds a fixed cache; cost becomes a function of total size |
   | `owner.peak_queued`, maintenance rows, per unit | grows with units | a queue or a debt that builds |
   | `pages` growth ÷ bytes written | above the row format's ratio | page overhead (a 4 KiB cell is 1.14 pages today) |
   | `daemon_vmhwm_bytes` between two cells of one family | grows with units | a resident container |

2. **Two cells that differ in one dimension**, or the same cell's counts
   divided by its units: the per-unit columns must be equal. Any counter
   that is not proportional to the units is either a fixed cost (it does not
   move) or a scaling defect (it moves faster).
3. **A two-size test in-process.** Run the operation at N and 4N, and again
   beside 1× and 16× unrelated rows, and at cycle 2 and cycle 10; assert
   equal `DatabaseWork` per unit. The patterns to copy:
   `the_five_jobs_of_one_created_file_cost_exactly_this_at_any_directory_size`
   in workspace `native_visit_cost` and
   `a_fixed_small_change_costs_the_same_counted_work_as_the_base_grows` in
   daemon `captured_commit`. The
   [handbook, section 6](optimization-handbook.md#6-detecting-worse-than-linear-scaling)
   gives the growth-factor table and the adversarial shapes.

Timing never shows an exponent here; one-sample spread is about 10 %.

### The opportunity list

Fill the ladder and the ratios for every sampled cell in one pass, then list
every excess, not only the largest. One row per opportunity:

| Field | Content |
| --- | --- |
| Level and excess | "2 of 3 write transactions per file", "13 trigger runs per file" |
| Unit cost | measured, from the family or class time of this receipt |
| Predicted saving per unit | excess × unit cost |
| Cells and saved time | units of each affected cell × saving, and that as a share of the cell's gap to its target |
| Proving counter | the receipt field and the value it will have |
| Order of growth | unchanged, or the term removed |
| Decisions needed | none, a design note, or the owner |
| Scope | crates and files; tests that assert the old count |

Rank by saved time as a share of the gap, summed over cells, and break ties
towards the smaller scope. Use the measured time of the family, not the
statement count: in R7 the diet removed 40 of 88 statements per file and
bought 18 µs, because the cheap ones went, while custody held 37.5 µs in 9
statements. Check the list against the gap before starting: on C01 the time
outside the owner alone (243 µs a file) exceeds the target (183 µs), so no
ranking of database work reaches it and the first rows must be requests and
per-request cost.

Source models are the slow part (15 to 30 minutes each in R7). Start one
read-only agent per request family at the beginning of the iteration, in
parallel, each required to reproduce the receipt's counts exactly; the list
is written when they return, once.

### One batch

Take rows from the top of the list into one batch while all of these hold:

- Each row has its own proving counter, in a different family, class or
  level from the others, predicted before editing. One sample then judges
  every row.
- Each row is its own commit and reverts alone. Two rows that edit the same
  statement or the same contract are one row.
- Rows that need an owner decision or change a public contract wait for it
  in a later batch.
- Commits go bottom-up by crate (overlay, workspace, fuse, daemon), each
  passing the commit tier, so the batch can be cut at any commit.
- One writer per crate at a time; research agents read with
  `git show <commit>:<path>` meanwhile.

Then: the gate once at the tip, one sample of every affected cell from one
build, one before/after table per cell with one line per row of the batch
(predicted counter, observed counter).

Reading the result:

- A row whose counter did not move as predicted had a wrong model: revert
  that commit alone, and record it. Its count test already said so at the
  count tier if one existed.
- Counts right and time better by more than the spread: keep the batch. The
  time belongs to the batch; do not divide it between rows.
- Counts right and time worse: sample the intermediate commits, one sample
  each (each is its own identity), to find the commit that moved it.

Five rows cost about five commit tiers, one gate and one sample set (about
25 minutes of checks) instead of five gates and five sample sets (about 50).

## Root-cause analysis of a time

Work down this tree and stop at the first branch that holds most of the
time. Every number is in the receipt; the summary helper prints the first
two levels.

```
command time
├── owner queue wait                       owner.queue_wait_ns
│     per class wait against per class service; owner.peak_queued;
│     mount wake-ups; steps above completed; maintenance jobs in the
│     interval; reader queue wait
├── owner service                          owner.service_ns
│   ├── statements, per family             time, attempts, executions, vm_steps
│   │     time ÷ vm_steps high  → not SQL work: journal and pager (COMMIT is
│   │                             3 steps and 10.8 µs), page allocation
│   │                             (pages, allocation_work), blob bytes
│   │     vm_steps ÷ executions high → the plan: EXPLAIN, cursors, loops
│   │     executions − attempts high → triggers: which, and what each maintains
│   └── service minus statement time       encoding, validation, copies
│         (payload_foreground), Store reads inside the job (store, cache)
└── outside the owner                      command − wait − service
      requests × per-request cost: receive, decode, reply, kernel and
      client time. Not divided further by the receipt.
```

1. **Place it.** Phase first (`phases`: mount, command, unmount), then the
   three branches, per unit. C01 at 579: 420.5 ms = service 175 + wait 2 +
   outside 243. The largest branch is the subject; the others are not.
2. **Fixed or per unit.** A count that is the same in a small and a large
   cell is fixed cost (group A of the handbook); only the rest scales.
3. **Write the cause as a count**, as in step 3 of the iteration rule, and
   name the one counter that would be different if the cause were something
   else.
4. **Refute it with the cheapest check that can fail**, in this order:
   `EXPLAIN` and `EXPLAIN QUERY PLAN` on the real schema; a count test at
   two sizes; the source model reproducing the receipt; a sample. A check
   that cannot come out against the hypothesis is not a check.
5. **Confirm by prediction.** The change is the proof only if the predicted
   counter moved by the predicted amount and the total moved with it.

Traps met in R7:

- **Time moves between branches.** Step 9 took queue wait from 31.0 ms to
  1.8 ms and "outside" rose by 9.7 ms: the close job's time went from
  waiting in the queue to running before the next request was read. Step 5
  cut wait by 66.5 ms while "outside" rose by 98.2 ms. Judge the command
  total and all three branches together.
- **A count is not a time.** See the diet above. Rank and predict in
  microseconds.
- **"The database is slow"** when 58 % of the command is outside the owner.
- **Same counts, different time, under 10 %:** noise. Do not resample.
- **Same counts, different time, over 10 %:** the environment or the cache
  state, not the code. Check `cache.object_demands` and cache misses (the
  phase paid cold reads), `owner.maintenance_*` and `sql_maintenance`
  (cleanup from the warm-up still running), `allocation_work`
  (preallocation inside the phase), and other containers on the host.
- **A FAIL or NOT_RUN row has no performance number.** A FAIL's differences
  are in `sample/runtime.events.artifacts/*-verify.stdout`; a NOT_RUN's
  reason is in `sample/cache.stdout`. Fix the cause in the product or
  report the row; its timing is not evidence.
- **Allocated bytes are not growth.** The overlay's 256 MiB reservation is
  disk reserved past the end of the file, not memory and not data written.
- **"Outside" is one number.** Until per-thread CPU time and a
  passthrough-arm sample exist, say that it is undivided; do not assign it
  to the kernel or to the daemon.

## What the receipt does not yet contain

Until these exist, say so in the report instead of inferring them.

- Per-thread CPU time and context switches of the daemon around the measured
  command. Without them, time outside the owner is one undivided number.
- A passthrough-arm sample of each cell in the same harness, as the floor.
- Job time per opcode (today only per class), and pages written per commit.
- Trigger work separated from the statement that fired it (today only
  executions minus statements).
- The Commit phase, concurrent clients, and a 1×/2×/4× size series.

## What the metrics do not replace

They show where the cost is, not why. Each kept step in R7 needed the source
model of step 2; the report confirms or refutes that model. They also cover
only what was sampled: a change tuned on a serial command says nothing about
concurrent clients, Commit, or a cell that did not run.

## Helpers

`core/target/r7-summary.py <receipt.json>` prints the outcome, request, job,
statement, Store and storage rows; `core/target/r7-statements.py
<receipt.json>` prints per-family statements, executions, VM steps and time.
`core/target/rx-suite.py host|linux <summary-file> <package>...` builds once
and runs each test binary once under its own wall limit; `RX_ONLY=a,b,c`
restricts both the build and the run to the named integration-test binaries.
`core/target/r7-iterate-all.sh <first-receipt-number> [cells]` samples the
named cells from one build. All are in the git-ignored `core/target`
directory and are lost if it is cleaned. The array positions they decode are pinned to the product's
diagnostics source (`core/crates/layerfs-overlay/src/diagnostics/metrics.rs`
and the request accounting in `core/crates/layerfs-fuse`); a changed family or
opcode list must be updated in the helper, which warns when the lengths
differ.
