# Benchmark instruction: the optimization iteration and the metrics it produces

> **Status:** Current general guide.

How an agent runs one optimization iteration against the mounted product
(cluster two: FUSE, daemon, Workspace, overlay) and which numbers every
iteration must report. It was written from the R7 run
([ledger](../../core/docs/issues/307/checks/r7-optimization-20261009/LEDGER.md),
[second handoff](../../core/docs/issues/307/HANDOFF-R7-BEAT-A2-20261009-2.md)).

This document adds no rule and lifts none. Cache state, setup reuse, budgets,
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

One iteration is one cause, one change, one sample per affected cell.

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
4. **Change one thing.** Several edits may be batched when they remove the
   same multiplier and are decided by the same counter. Record every design
   decision the change relies on and where it is documented.
5. **Prove it before timing it.** Tests for the changed behaviour and for
   every check a removed step used to make; the changed packages' suites on
   host and in the pinned Linux image; Clippy with warnings denied on both;
   `fmt --check`; the boundary guard. Every test command has a wall limit of
   at most 120 s.
6. **Commit, then sample once.** One sample per affected cell from a clean,
   sealed HEAD, into a fresh append-only receipt. No best-of, no repeat of an
   unchanged identity. A failed, incomplete or unrun selection keeps its
   receipt and its verdict.
7. **Decide by counts first.** Keep the change when the predicted counter
   moved as predicted, the verifier passes, no gate below is broken and
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
Both are in the git-ignored `core/target` directory and are lost if it is
cleaned. The array positions they decode are pinned to the product's
diagnostics source (`core/crates/layerfs-overlay/src/diagnostics/metrics.rs`
and the request accounting in `core/crates/layerfs-fuse`); a changed family or
opcode list must be updated in the helper, which warns when the lengths
differ.
