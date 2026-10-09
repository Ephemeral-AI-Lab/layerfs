# UNLINK, FORGET, orphan reclamation and closed-namespace reclamation at 63c48d8dc

Read-only research. Product source read only with `git show 63c48d8dc:<path>`. Receipts are the
`63c48d8dc` samples in `core/docs/issues/307/checks/r7-optimization-20261009/`: C03 = 667, C01 = 659,
C06 = 679. Scratch work is in `scratchpad/research/unlink-reclaim/` (`exp.py`, `exp2.py`, `exp3.py`,
`plans.py`, real schema v22 and accounting triggers, python sqlite 3.51.2 on macOS: relative numbers only).

Labels: [S] source at 63c48d8dc with file:line, [R] decoded from a named receipt, [I] inference,
[E] estimate, [X] scratch measurement. Paths are relative to `core/crates/`.

## Headline findings

1. **Every unlinked file costs 6 maintenance transactions that run between the client's requests** [R 667:
   6,011 maintenance jobs, 142.5 ms of owner-connection time, in a 716 ms command]. C01 has none [R 659].
   Almost all of it is self-inflicted: in 992 of 1,000 files the owner thread moves the file's cell into
   the orphan domain before the FORGET arrives, then deletes it again [R: `maintenance_data_bytes` 11,650
   and 2,973 Payload statements; arithmetic in section 1.4].
2. **The connection is 81 % busy during the rm half**, so a serial client collides with a maintenance
   step on most requests: 3,549 dispatch-task wakes in C03 against 0 in C01 [R `mount_work.wakes`], and
   57.6 ms more owner wait than C01 [R].
3. **`orphan_seen` is never cleared** [S `layerfs-overlay/src/database/connection.rs:26-30`]. The warm-up
   Workspace's first unlink sets it for the life of the daemon, so the measured C03 pays 14,039 extra
   Inode statements, 7,000 of them in the create half [I, from the family residual in 1.3]. Every later
   Workspace on that daemon pays one extra statement per inode read.
4. **After unmount, C01 spends 1,001 of its 1,135 reclamation transactions deleting one `file_custody`
   row each** [R 659 snapshots; S `lifetime/file_owners.rs:45-48`, `maintenance/orphan.rs:11-19`]. That is
   about 34 of 44.8 ms. The namespace is closed and is deleted wholesale a moment later.
5. **Closed-namespace payload deletion is 14 rows and 16 statements per transaction** because of a byte
   window for blobs that are not read by the code, while the delete trigger does load every blob
   [S `maintenance/reclaim.rs:7-9,167-182`, `sql/accounting.sql:50-53`; X `plans.py`]. One key-range
   DELETE per step at the product's own 64-row page is 2.6 times faster in scratch, and 4 times with a
   trigger that does not reference `OLD.data` and without the `payload_namespace_row` index [X].
6. **No timer-free definition of "idle" removes the collisions of a serial client** [I, argument in 2c].
   The effective lever is to leave nothing queued: finish small reclamation in the releasing job.

---

## 1. Exact model of the rm half at 63c48d8dc

### 1.1 Kernel requests per removed file

C03 is `for i in $(seq 1 1000); do echo $i > f$i; done; rm f*` [R `sample/command.sh`]. Requests: 9,021 =
C01's 5,001 plus 4,020 [R 667, 659].

| Request | Count | Why |
| --- | --- | --- |
| OPENDIR, READDIR ×17, RELEASEDIR | 19 | the shell expands `f*` once |
| GETATTR (directory) | 1,000 | each UNLINK invalidates the directory's attributes; the next path walk's permission check under `default_permissions` refreshes them [I; the same GETATTR appears once per create in C01] |
| GETATTR (file) | 1,000 | `rm` stats each argument; the WRITE invalidated the file's attributes [I] |
| UNLINK | 1,000 | |
| FORGET | 1,000 | single units, no batch [R: `BatchForget` 0, `forget_units` 1,000] |

No LOOKUP: the dentries from CREATE are still valid.

### 1.2 Owner job of each request

Each write transaction is `PRAGMA main.freelist_count` (Startup), `BEGIN IMMEDIATE`, `COMMIT`
[S `layerfs-overlay/src/database/connection.rs:276-302,233`]. A job that writes nothing begins nothing
[S same file :207-231]. Counts below are statement **attempts**; the receipt's "executions" add one per
trigger firing [S `diagnostics/metrics.rs:131-132`], so "Reclaim 7,002" is 4,001 attempts plus 3,001
trigger programs.

**GETATTR** → one `NativeJob::ObserveVisit`, class Read, no transaction
[S `layerfs-daemon/src/overlay/native_job.rs:104,225`; `layerfs-overlay/src/lifetime/native_visit.rs:100-147`].

| Statement | Family | Source |
| --- | --- | --- |
| `FENCE_LOOKUP` | Workspace 1 | `lifetime/native_visit.rs:60-66`, `database/statements.rs:163` |
| orphan-domain probe (`gen=-1`), only while `orphan_seen` | Inode 1 | `lifetime/orphan.rs:42-48` |
| `INODE_LOOKUP` | Inode 1 | `namespace/inode.rs:85-93` |

**UNLINK** → one `NativeJob::MutateVisit`, class Mutation, one transaction
[S `lifetime/native_visit.rs:154-191`; `layerfs-workspace/src/operations/namespace/remove.rs:27-65`;
`layerfs-workspace/src/mutation/job.rs:123-187`; `namespace/compound.rs:144-236`].

| Step | Statements | Source |
| --- | --- | --- |
| fence | Workspace 1 | `native_visit.rs:167` |
| parent inode (probe + lookup) | Inode 2 | `mutation/eval.rs:54-59` |
| name layers | DirectoryEntry 1 | `eval.rs:60-62` |
| target inode (probe + lookup) | Inode 2 | `eval.rs:112-118` |
| parent inode and name layers read again for the change | Inode 2, DirectoryEntry 1 | `mutation/job.rs:153-158` |
| target row: `LAYER_ACTIVE`, `INODE_PUT` | Inode 2 | `namespace/inode.rs:124-184` |
| parent row: `LAYER_ACTIVE`, `INODE_PUT` | Inode 2 | same |
| name layers a third time, then `DIRECTORY_ENTRY_DROP` | DirectoryEntry 2 | `inode.rs:226-239`, `compound.rs:202-207` |
| `ORPHAN_LOOKUP` | Lease 1 | `lifetime/orphan.rs:61` |
| enqueue `SERIAL_RETIRE(serial, active)` | Reclaim 1 | `orphan.rs:62-64` |
| `FILE_REFS` (1: the kernel lookup) | Lease 1 | `orphan.rs:65` |
| `INSERT INTO orphan` | Lease 1 | `orphan.rs:70-81` |
| `INODE_PUT` at `gen=-1` | Inode 1 | `orphan.rs:82-103` |
| enqueue `ORPHAN(serial, -1)` | Reclaim 1 | `orphan.rs:104` |
| `FRONTIER_ADVANCE` | Workspace 1 | `inode.rs:286-291` |

Total: Workspace 2, Inode 11, DirectoryEntry 4, Lease 3, Reclaim 2, one transaction.

**FORGET** → one `NativeJob::Forget`, class Lifecycle, one transaction
[S `layerfs-fuse/src/request/inline.rs:42-87`; `lifetime/native.rs:349-378`].

| Step | Statements | Source |
| --- | --- | --- |
| Workspace row, then mount row | Workspace 1, Lease 1 | `native.rs:79-82,49-71` |
| `native_lookup` row | Lease 1 | `native.rs:229-239` |
| delete `native_lookup`, delete its lease | Lease 2 | `native.rs:323-339` |
| `FILE_LOOKUPS_DROP … RETURNING` (0 left) | Lease 1 | `lifetime/file_owners.rs:39-44` |
| enqueue `ORPHAN` (conflict, no row) and `wake_orphan` | Reclaim 2 | `file_owners.rs:45-48`, `maintenance/orphan.rs:211-222` |
| Workspace row again for `queue_closed` | Workspace 1 | `lifetime/close.rs:128-131` |

Total: Workspace 2, Lease 5, Reclaim 2, one transaction.

### 1.3 Reconciliation of the foreground counts [R 667 against R 659]

`orphan_seen` is already true when the measured command starts, because the warm-up ran the same command
on the same engine [R `runtime.events.jsonl`: two commands, `/workspaces/1` then `/workspaces/2`]. The
create half therefore makes 7 probes per file that C01 does not: LOOKUP 2 (two fact rounds), CREATE 3,
WRITE 1, GETATTR 1 [S the `inode_at` call sites; I].

| Family (attempts) | C01 | + create-half probes | + rm half ×1,000 | Sum | Receipt | Residual (the 19 directory requests) |
| --- | --- | --- | --- | --- | --- | --- |
| Startup / Begin / Commit | 3,003 / 3,000 / 3,000 | | 2,000 | 5,000 | 5,074 / 5,071 / 5,071 | 71 transactions |
| Workspace | 7,001 | | 6,000 | 13,001 | 14,267 | 1,266 |
| Inode | 14,002 | 7,000 | 15,000 | 36,002 | 38,041 | 2,039 |
| DirectoryEntry | 7,000 | | 4,000 | 11,000 | 11,017 | 17 |
| Payload | 2,000 | | 0 | 2,000 | 2,000 | 0 |
| Lease | 9,000 | | 8,000 | 17,000 | 20,687 | 3,687 |
| Reclaim | 0 | | 4,000 | 4,000 | 4,001 | 1 |

Residuals [I]: the READDIR pages read each of the 1,000 entries' inode through the public `inode()`
(one Workspace statement and two Inode statements per entry: 1,000 and 2,000), and cookies cost one
SELECT and one INSERT per name (2,000 Lease) [S `lifetime/native_cookie.rs:47-56,86-91`]. What is left
is 266 Workspace, 39 Inode, 1,687 Lease and 71 transactions for 107 jobs of 19 directory requests; I did
not trace that path statement by statement. The one Reclaim is the RELEASEDIR enqueue.

Owner jobs: 5,001 (C01) + 2,000 Read + 1,000 Mutation + 1,000 Lifecycle + 107 directory jobs (Read 51,
Lifecycle 38, Source 18) = 9,108 [R 9,108, residual 0].

### 1.4 Maintenance caused by each file

Two items exist after UNLINK: A = `ORPHAN(serial, -1)` and B = `SERIAL_RETIRE(serial, 1)`, both ready
[S `lifetime/orphan.rs:63,104`]. One `Overlay::maintain` call serves one item in one cleanup transaction
[S `maintenance/ready.rs:147-206`]. The orphan row holds generations `(installed, active]` = `(0, 1]`
[S `orphan.rs:70-81`].

Sequence observed for 992 of 1,000 files (FORGET arrives after the first step):

| Step | Item, state | What it does | Rows | Source |
| --- | --- | --- | --- | --- |
| s1 | A, still held by the kernel lookup | moves the one cell from generation 1 to the orphan domain (`UPDATE payload SET gen=-1`) | 1 | `maintenance/orphan.rs:21-48`, `maintenance/source_wait.rs:73-104` |
| s2 | B | generation 1 is held by the orphan: `hold_item` | 0 | `orphan.rs:149-154` |
| s3 | A | after FORGET: releases the lower layer, re-readies B. If FORGET has not arrived yet: the same plus a cutoff update, 1 row | 0 or 1 | `orphan.rs:72-88` or `:50-59` |
| s4 | B | no cells, no shrink rows; the tombstone stays; item finished | 1 | `orphan.rs:155-209` |
| s5 | A | deletes the cell it moved in s1 | 1 | `orphan.rs:89-109` |
| s6 | A | deletes the orphan-domain inode, the orphan row and the `file_custody` row | 3 | `orphan.rs:128-147` |

When FORGET wins the race (about 8 files): 4 steps, 5 rows, no move.

Evidence for the split [R 667]: `maintenance_data_bytes` = 11,650. File contents sum to 3,893 bytes and
so do the 1,000 cookie names ("fN" and "N\n" have equal lengths). 3,893 × (1 cookie sweep + 1 + m) =
11,650 gives m = 0.99: the cell was counted twice (moved, then dropped) for 99 % of files. Payload
statements in maintenance: 2,973 = 3 × 991 (two cell reads and the UPDATE of s1).

Reconciliation of the maintenance counters, after removing the directory's cookie sweep (17 steps, 1,001
rows, about 1,035 Lease and 34 Reclaim statements [S `maintenance/native_directory.rs:4-39`; I]):

| Per file | Model, x = share where s3 ran before FORGET | Receipt | Residual |
| --- | --- | --- | --- |
| steps = transactions | 6.0 | 5.99 | 0 |
| rows | 6 + x | 6.46 → x = 0.46 | fitted |
| Lease | 16 + x = 16.46 | 16.45 | 0.01 |
| Inode | 4 + 2x = 4.92 | 4.92 | 0 |
| Workspace | 4 + 2x = 4.92 | 4.92 | 0 |
| Payload | 3 × 0.99 = 2.97 | 2.97 | 0 |
| Reclaim | 24 + 2x = 24.9 | 25.96 | 1.0: the two `READY` selects of the turn that finds the queue empty (no transaction), not resolved further |

Totals [R 667, statement section 3]: 6,011 transactions, 74,328 statement attempts (Reclaim 25,992, Lease
17,484, Workspace 4,924, Inode 4,922, Payload 2,973, Startup/Begin/Commit 18,033), 126.7 ms of statement
time, 142.5 ms of turn time, 23.7 µs per step.

### 1.5 Time per unit [R, C03 minus C01 unless stated]

| Per removed file | µs | Basis |
| --- | --- | --- |
| Maintenance turns on the owner thread | 142.5 | `maintenance_ns` 142.5 ms |
| UNLINK service | about 75 | Mutation service +77.5 ms, less about 5 ms of create-half probes [E] |
| Owner wait added | 57.6 | 59.3 − 1.7 ms. Lifecycle 24.0 µs per job, Read 13.3, Mutation 5.4 |
| FORGET service | 33 | Lifecycle service +34.2 ms over 1,038 jobs |
| Two GETATTRs | about 14 | Read service +25.1 ms, less probes and READDIR pages [E] |
| Dispatch-task wakes | 3.5 wakes | 3,549 against 0 |

Foreground statement time added by the rm half, by family: Inode 37.9 ms, Lease 23.3, Reclaim 19.1
(4.77 µs each: every enqueue fires the accounting trigger), Commit 16.1 (7.8 µs each), Workspace 14.4,
DirectoryEntry 8.2. Whole rm half: about 336 ms wall for 1,000 files; the connection is occupied 142.5
(maintenance) + 131 (service) = 273 µs of each 336 µs.

### 1.6 Why the owner thread takes the connection between two requests

- A submitting thread serves its own job when the connection is free, nothing else is queued and
  maintenance is not due [S `layerfs-daemon/src/overlay/owner.rs:355-360`].
- When that job ends, `Shared::pass` records `db.maintenance_pending()` and, if a job is queued **or
  maintenance is pending**, calls `wake.notify_all()` [S `overlay/queue.rs:372-388`; `owner.rs:488`].
  After UNLINK the hint is true, so the owner thread is woken after every job.
- The owner's `poll` takes the connection; with no job queued `due = served >= 8 || job.is_none()` is
  true, and it runs one `maintenance_turn` [S `queue.rs:344-351`; `owner.rs:566-570`].
- While a turn reports work the owner does not wait: `if idle && !maintained { wait }`
  [S `owner.rs:588-591`]. It loops until `maintain` returns `None`.
- A job admitted while the owner holds the connection is queued without a wake [S `owner.rs:355-376`],
  waits out the step, and is then served by the owner thread, so its submitter has to be woken.

How often [R 667]: 6,011 turns in the rm half, 1.5 per request. 3,549 of about 4,107 rm-half jobs
(86 %) were not finished by their own submitter. The 8-job fairness bound never limits anything here:
the idle rule fires first every time.

### 1.7 After unmount at 63c48d8dc

Synchronous in the unmount call [S `layerfs-daemon/src/control/detach.rs:49-120`,
`control/unmount.rs:90-108`]: kernel detach, connection drain, one `Revoke` job (marks the mount revoked
and enqueues one `NATIVE` item [S `lifetime/native.rs:383-394`]), one `Close` job (sets `lifecycle=1`;
`queue_closed` finds the mount row and queues nothing [S `lifetime/close.rs:76-87,135-155`]). Two write
transactions. FORGET is not sent at unmount [I; R: 1,001 lookup rows are retired in C01].

Later, by maintenance:

1. `retire_native`: unreleased files, directories, then lookups, 64 per step. Each lookup is three
   statements and, at zero references, an `ORPHAN` enqueue and a wake
   [S `maintenance/native.rs:72-81`, `lifetime/native.rs:317-346`, `file_owners.rs:45-48`].
2. One `ORPHAN` step per serial that reached zero: no orphan row, so it deletes one `file_custody` row
   and its own item, in its own transaction [S `maintenance/orphan.rs:11-19`].
3. The mount row goes, `queue_closed` inserts the `reclaim` row [S `maintenance/native.rs:83-91`].
4. `reclaim_closed`: 12 phases, one page per transaction, an empty page costs a transaction to advance
   the cursor, then the final step [S `maintenance/reclaim.rs:41-117`].

The owner alternates live and closed work and runs consecutive steps while no job is queued
[S `owner.rs:553-615`]: verified, nothing to add there.

| Cell | Steps = transactions | Rows | Owner time | Unmount → Gone (wall) | Source |
| --- | --- | --- | --- | --- | --- |
| C01, 1,000 files | 1,135 | 4,005 | 44.8 ms | 55.7 ms | R 659 owner snapshots 3→10, `phases.cleanup` |
| C06, 64 MiB | 1,190 | 16,392 | 134.9 ms (150.4 measured sample) | 139 ms (173 with 55 ms polls) | R 679 snapshots 3→21, 24→29, `runtime.events` |
| C03 | 32 | 1,005 | 3.9 ms | under 56 ms (poll granularity) | R 667 |

Model for C01 [S + I]: 17 `NATIVE` steps + 1,001 `ORPHAN` steps + 116 closed steps (payload 72, names
16, inodes 16, 12 empty or final) = 1,134 against 1,135. Rows: 1,001 lookups + 1,000 + 1,000 + 1,001 +
3 = 4,005, exact. The `ORPHAN` steps take 33.6 of the 44.8 ms [R: snapshots 5→9].
Model for C06: 1,171 payload pages + 19 others = 1,190, exact. 113 µs per 14-row step.
C03's 1,005 rows are 1,000 inode tombstones of the removed files, which stay until unmount
[S `maintenance/orphan.rs:194-207`].

---

## 2. Design

### 2a. The releasing job finishes small reclamation

Where: `Overlay::file_ref`, the `left == 0` branch [S `lifetime/file_owners.rs:45-48`]. It is the single
place every last reference passes: FORGET, RELEASE, read release, lookup-owner release.

**(i) No orphan row** (a live file, a base file, any file at unmount). Delete the custody row in the
same transaction and enqueue nothing:
`DELETE FROM file_custody WHERE ns=?1 AND serial=?2 AND opens+lookups+readers=0`. This is exactly what
the `ORPHAN` step does today [S `maintenance/orphan.rs:11-19`], run one statement later instead of one
transaction later. The orphan probe is skipped while `orphan_seen` is false, which is exact.

**(ii) Orphan row exists.** Keep `enqueue(ORPHAN)` as the exact fallback, then call the existing step
functions in the order the owner thread would, inside the current transaction:

```
budget = 14 cells                       // the product's payload page, reclaim.rs:7-9, orphan.rs:92,158
loop at most 6 calls:                   // 2 for the orphan + 2 per lower layer, at most 2 lower layers
    maintain_orphan(A)                  // releases one lower layer, or drops a page, or finishes
    retire_serial(B) for a layer it released
    stop when both report done, when a call reports a hold, or when budget is spent
```

Nothing is reimplemented: `file_refs`, `generation_held`, `orphan_holds` and the captured-generation
check run inside those functions as today [S `maintenance/orphan.rs:21,29-30,151`]. The loop is a prefix
of a schedule that is already legal, namely the owner thread running these steps immediately after this
job; the only difference is that it commits with the release.

Bound: 14 payload cells (57,344 bytes of 4 KiB cells, the existing 64 KiB step window) and 64 shrink
rows, plus a fixed number of point statements. That is one maintenance step's worth of rows, which a
foreground job may already have to wait for. A larger file stops at the budget; its items are already in
`maintenance`, ready, and the owner thread continues. That is the "enqueue only the remainder" case and
it needs no extra code.

Two details:

- `maintenance_ready` must be restored to its previous value when the loop leaves nothing ready.
  Otherwise each FORGET wakes the owner for an empty probe. False stays exact: the rows this job added
  are gone in the same transaction.
- `retire_native` must not finish orphans inline: 64 lookups × 14 cells would no longer be one bounded
  step. It keeps the enqueue for case (ii) and uses case (i), one statement per lookup. The `rows <= 64`
  assertions then hold unchanged.

**Lean follow-up.** Run through the wrappers, the loop costs 27 statements for a one-cell file (Lease 11,
Reclaim 12, Inode 2, Workspace 2), several of them item bookkeeping on rows that are inserted and deleted
in the same job. Extracting the item-free cores of `maintain_orphan` and `retire_serial` (release lower
layer; drop one page of one layer; final delete) and calling those from both the maintenance wrappers
and the release path brings it to about 12 [E]. Same code, same holds, no second implementation.

**FORGET's own reads.** `check_native_attached` + `native_lookup_row` + `queue_closed` are four
statements that one fence statement returning the lookup row would replace
[S `lifetime/native.rs:354-357,376`; the fence is `database/statements.rs:150-164`]. Minus 3 statements
per forgotten inode, any workload.

**BATCH_FORGET.** fuser 0.18.0 has `Filesystem::batch_forget(&self, req, nodes: &[ForgetOne])`, whose
default calls `forget` per node [S `core/vendor/fuser-0.18.0/src/lib.rs:432`]. The product implements
only `forget` [S `layerfs-fuse/src/request/callbacks.rs:80-82`]. Overriding `batch_forget` needs no
third-party edit. Proposal: one permit, one job and one transaction per window of 64 units (the existing
page), `NativeJob::ForgetBatch`. Each unit's failures (`Stale`, underflow) are found by its read before
any write [S `lifetime/native.rs:355-360`], so a failing unit is recorded and skipped without rolling
the others back; unit accounting stays per unit. C03 has no batches; this matters for eviction and for
removing trees.

### 2b. UNLINK with a kernel reference and no open handle

Must wait for the last reference: payload cells, shrink rows, the orphan row and its `gen=-1` inode row,
the `file_custody` row. The kernel may still OPEN or GETATTR that node id
[S `layerfs-workspace/src/operations/native_read.rs:188-201`; pinned by
`layerfs-overlay/tests/native_lookup.rs:161`], and the orphan row is what keeps generations `(floor, top]`
of this serial from being retired by a Commit in between [S `lifetime/orphan.rs:35-39`,
`maintenance/garbage.rs:19,88`, `lifetime/composition.rs:151`].

Stays in the unlink transaction, as today: name row, tombstone, parent counts and time, revision, orphan
row, orphan-domain inode row.

Can leave the unlink transaction:

1. **`SERIAL_RETIRE` when an orphan row is created.** It can only hold itself (step s2);
   `release_orphan_layer` enqueues and readies it for every layer it releases
   [S `lifetime/orphan.rs:111-141`]. Minus 1 statement and 1 wasted transaction for every file unlinked
   while referenced, open or not.
2. **`ORPHAN` while only lookups hold the file** (`opens = 0`, `readers = 0`). The migration it starts
   serves readers and writers of the orphan; there are none. The last lookup drop already enqueues and
   wakes it [S `file_owners.rs:45-48`]. To keep today's eager migration for a file opened after its
   unlink, the open of an inode that has an orphan row enqueues and wakes it (two statements on a rare
   path, in `retain_file`). An orphan whose lower layers are not migrated yet is an existing state: a
   captured generation parks it the same way [S `maintenance/orphan.rs:29-40`], and fold and retire skip
   held serials.

With 1 and 2 nothing is ready after UNLINK, `maintenance_pending()` is false, and the owner thread is
not woken. Without them, 2a alone still leaves steps s1 and s2 to race the FORGET.

**Clear `orphan_seen`.** After a transaction that deleted an orphan row, read `orphan_rows` of
`accounting` namespace 0 (trigger-maintained, transactional [S `sql/accounting.sql:86-93`]); zero means
no orphan row and no orphan-domain inode row exists in any namespace, so the flag returns to false,
exactly. One statement per orphan deletion; it removes one statement from every inode read otherwise.

### 2c. Scheduling

What an event-only rule cannot do [I]. After the last request of a stream exactly one wake exists: the
notify at the end of that job's turn. If the owner does not start a step at that wake, reclamation
stalls until some unrelated job arrives; `layerfs-daemon/tests/owner.rs:138`
(`idle_cleanup_runs_without_an_explicit_reclaim_or_status_job`) pins that it must not. The owner cannot
tell that wake from the wake after any other request of a serial client. So without a clock it must
start a step in every gap, and a step longer than the gap collides with the next request. Counting jobs
before the next step (any back-off) stalls when the stream stops inside the count. Today's rule is
already the event-only optimum for a serial single-job client.

What does help, without a timer:

1. **Nothing pending** (2a, 2b, 2e): no wake at all. This is what removes C03's 6,011 turns.
2. **Exact hint**: restoring `maintenance_ready` removes the empty-probe wakes.
3. **Skip the wake while a request is known to be in flight**: the dispatch layer knows requests it has
   received and not completed. A job of a multi-job request, or one of several concurrent requests, is a
   gap that is certainly not idle. Proposed definition: *idle = the connection is free, no job is queued
   and no native request received by any mount is uncompleted, observed when a turn or a request ends*.
   One per-daemon atomic counter, fixed size. It does not help a serial stream of single-job requests.
4. **Fewer, cheaper steps** (2d): collisions are at most one per step, so 262 steps cost less than 1,190.

Bounds kept: a job still waits for at most one step; the 8-job rule is untouched
[S `overlay/queue.rs:190-192,346`]; reclamation still starts by itself.

The alternative that does remove the collisions is one bounded `Condvar::wait_timeout` on the existing
owner thread before an idle turn that follows served jobs (not a thread, not a loop). It adds its
duration to every cleanup and it is a new timeout. The brief excludes it; I list it only because it is
the one complete answer. **Decision for the owner if 1 to 4 prove insufficient.**

### 2d. Closed-namespace reclamation

Current statements [S `maintenance/reclaim.rs`]:

```
READY   SELECT ns,cursor FROM reclaim INDEXED BY reclaim_ready WHERE queue_key=?1 AND ns>?2 ORDER BY ns LIMIT 1
phase 0 SELECT rowid,length(data)+ifnull(length(validity),0) FROM payload
          INDEXED BY payload_namespace_row WHERE ns=?1 ORDER BY rowid LIMIT 14
        DELETE FROM payload WHERE rowid=?1 AND ns=?2                      -- once per row
phase 1 SELECT parent,name,gen FROM directory_entry WHERE ns=?1 ORDER BY parent,name,gen LIMIT 64
        DELETE FROM directory_entry WHERE ns=?1 AND parent=?2 AND name=?3 AND gen=?4   -- once per row
phase 2, 5-8, 10: the same shape with LIMIT 64; 3, 9, 11: LIMIT 64 inside a 64 KiB value window
empty page: UPDATE reclaim SET cursor=cursor+1 ...                        -- its own transaction
```

The trigger `payload_account_delete` uses `length(OLD.data)`, so the DELETE's main program loads columns
`data` and `validity` in full for every row: `Column` opcodes on columns 6 and 7 without the length-only
flag, gone when the trigger no longer names them [X `plans.py`, sqlite 3.51.2]. Without the trigger a
4 KiB cell's single overflow page would not be read at all [I, from SQLite's `clearCell`].

Proposed, one key-range DELETE per step on the primary key or the unique index, all of which start with
`ns` [S `sql/schema.sql:35,45,57,66,76,…`]:

```
SELECT serial,gen,cell_offset FROM payload WHERE ns=?1 ORDER BY serial,gen,cell_offset LIMIT 1 OFFSET 63
  found:  DELETE FROM payload WHERE ns=?1 AND (serial,gen,cell_offset)<=(?2,?3,?4)
  absent: DELETE FROM payload WHERE ns=?1          -- fewer than 64 left: phase ends in this step
rows = changes()
```

and the same for `directory_entry (parent,name,gen)`, `inode (serial,gen)`, `file_custody (serial)`,
`shrink`, `orphan`, `orphan_wait`, `maintenance`, `native_cookie (owner,cookie)`. Plans: `SEARCH … USING
COVERING INDEX sqlite_autoindex_payload_1 (ns=? AND (serial,gen,cell_offset)<(?,?,?))` and `USING
PRIMARY KEY`; no scan, sort, temporary b-tree or list subquery [X `plans.py`]. The
`rowid IN (SELECT … LIMIT n)` form measures the same but materializes a list and a Bloom filter.

Further steps, each separate:

- **Fold empty phases**: advance through phases that delete nothing inside the same step, at most 12.
  Minus about 11 transactions for every closed namespace.
- **Trigger without `OLD.data`**: the trigger keeps `payload_cells`; `payload_bytes` is subtracted by
  the deleting code from lengths it already selects in every path
  [S `maintenance/orphan.rs:91,157`, `garbage.rs:14,128`, `reclaim.rs:9`]: one aggregate
  `SELECT total(length(data)+ifnull(length(validity),0))` over the step's key range (reads row headers,
  not blobs) and one `UPDATE accounting … WHERE ns IN (0,?1)`. The database is created by each daemon
  and never reopened [S `database/connection.rs:33-35`], so the trigger text can change with no migration.
- **Drop `payload_namespace_row`**: the key-range form uses the unique index `(ns,serial,gen,cell_offset)`.
  The only reader of that index is `reclaim.rs:9`, with `explain_closed_reclaim` at `:129` [S `git grep`
  over `core/` outside docs]; no test names it.

Scratch on the real schema, journal MEMORY, synchronous OFF, 2 MiB cache [X `exp2.py`, `exp3.py`,
median of 3, quiet run]:

| 16,384 cells of 4 KiB (C06) | Transactions | Statements | ms | Step, median ms |
| --- | --- | --- | --- | --- |
| A. current | 1,187 | 22,336 | 226–229 | 0.185–0.189 |
| B. key-range, n = 14 | 1,187 | 7,126 | 141 | 0.112 |
| B. key-range, n = 32 | 528 | 3,172 | 100 | 0.186 |
| B. key-range, n = 64 | 272 | 1,636 | 86–87 | 0.32 |
| C. n = 64, trigger without `OLD.data`, bytes from lengths | 272 | 2,150 | 66–69 | 0.24–0.26 |
| D. as C, bytes settled once at the end of the phase | 272 | 1,638 | 64 | 0.233 |
| F. as D, index dropped | 272 | 1,638 | 56 | 0.213 |
| n = 256: B / D / F | 80 | 484–486 | 78 / 55 / 48 | 1.16 / 0.81 / 0.71 |

| 1,000 files of 2–5 bytes (C01), closed phases only | Transactions | Statements | ms |
| --- | --- | --- | --- |
| current | 133 | 4,679 | 23.8–24.2 |
| key-range, n = 64 | 77 | 466 | 9.6–10.4 |
| key-range, n = 512 | 21 | 130 | 8.0 |

The scratch transaction counts for "current" equal the product's (1,187 against 1,190; 133 against 116
plus custody, which scratch leaves to phase 8).

Dropping the index removes 274,432 bytes of 76,591,104 (0.36 %) for C06 and 20,480 of 425,984 (4.8 %)
for C01 [X].

**n = 64**, from the product, not from a cell: it is the page of every other terminal table
[S `maintenance/reclaim.rs:12-24,152,186,210,236,282,296`], of `FILE_WINDOW` and `DIRECTORY_WINDOW`
[S `maintenance/native.rs:7-8`], and of the tests' `rows <= 64`
(`layerfs-overlay/tests/engine.rs:92,172`, `native_lookup.rs:462`, `layerfs-daemon/tests/mounted_drain.rs:96`).
Payload's 14 is 65,536 / (4,096 + 512) [S `reclaim.rs:7-9`], a byte window for blobs the code does not
read.

**Foreground stall bound after the change**: one 64-row step. In scratch 0.32 ms with today's trigger
(1.7 × today's step) and 0.21–0.24 ms with the lean trigger (1.15–1.3 ×); p99 0.37–0.57 ms. In product
units [E, scratch ratios applied to 113 µs]: about 0.19 ms and 0.13–0.15 ms. To keep the bound at
today's time with today's trigger, n = 32 does it (0.186 ms in scratch, equal to current) and still
removes 56 %.

Order matters: with today's trigger a 64-row payload step loads up to 295 KiB of old blobs. So either
the lean trigger goes first, or payload stays at n = 14 or 32 until it does.

### 2e. Unmount

Synchronous part: section 1.7; two transactions; nothing to remove.

What the later work can drop for a closed namespace:

| Work today | Rows it exists for | Proposal | Saving |
| --- | --- | --- | --- |
| 1,001 `ORPHAN` steps (C01) | one `file_custody` row each | 2a (i): deleted with the lookup, no item | −1,001 transactions, −33.6 ms [R] |
| 12 empty-phase transactions | cursor advance | fold into one step | −11 transactions, about −0.4 ms [E] |
| row-by-row pages | every table | 2d | C01 closed phases 116 → about 50 steps |
| 3 statements per lookup in `retire_native` | custody of a mount that is gone | three key-range statements per 64 lookups, plus an `INSERT … SELECT` of `ORPHAN` items for serials that have an orphan row | about −2 ms per 1,000 lookups [E]; optional |
| 1,000 inode tombstones (C03) | hide a base inode of the same serial | a file born in the never-captured active generation has no base inode to hide | −16 steps; **unverified**, and `layerfs-overlay/tests/orphans.rs:216` pins a tombstone |

Not proposed: letting the closed phases take `native_lookup`, lease and custody rows directly. It needs
`queue_closed` to ignore some lease kinds [S `lifetime/close.rs:145-147`], which changes a hold.

---

## 3. Predicted effect, invariants, tests

### 3.1 C03 command [R today → predicted]

| Counter | Today | After 2b + 2a (wrappers) | With the lean cores and FORGET fence |
| --- | --- | --- | --- |
| Owner jobs | 9,108 | 9,108 | 9,108 |
| Foreground write transactions | 5,071 | 5,071 | 5,071 |
| Maintenance jobs = transactions | 6,011 | 17 (cookie sweep) | 17 |
| Maintenance rows | 7,462 | about 1,001 | about 1,001 |
| Reclaim attempts, foreground + maintenance | 29,993 | about 13,500 | about 7,000 |
| Inode attempts (`orphan_seen` cleared) | 38,041 | 24,002 | 24,002 |
| All statement attempts, foreground + maintenance | 179,557 | about 118,000 | about 101,000 |
| Owner wait | 59.3 ms | 2–4 ms [E] | 2–4 ms [E] |
| Dispatch-task wakes | 3,549 | under 50 [E] | under 50 [E] |

Time [E]: −57 ms wait; −18 to −53 ms for 3,500 wakes at 5–15 µs; −14 to −21 ms for 14,039 probes; −19 ms
for 4,000 enqueue statements; +51 ms (wrappers) or +23 ms (lean) of statements moved into FORGET; plus
an unknown, because a longer FORGET job delays the GETATTR that follows it more often. Net about −60 to
−100 ms with the wrappers and −85 to −125 ms lean: **C03 from 716 ms to roughly 590–655 ms**. Not enough
alone for the 410.8 ms target; UNLINK itself is 75 µs per file and reads the parent inode and the name
layers three times each (rows 2, 5 and 8 of its table in 1.2).

### 3.2 After unmount [R today → predicted, E]

| | Transactions | Owner time |
| --- | --- | --- |
| C01 today | 1,135 | 44.8 ms |
| C01 after 2a (i) | about 134 | about 11 ms |
| C01 after 2d as well | about 67 | about 6 ms |
| C06 today | 1,190 | 134.9 ms |
| C06 after 2d, n = 64, trigger as now | about 262 | 51–57 ms |
| C06 with the lean trigger | about 262 | 39–44 ms |
| C06 without the index as well | about 262 | 33–37 ms |

### 3.3 Invariants

| Invariant | What still enforces it |
| --- | --- |
| Orphan content survives while open or referenced | the inline loop runs only at `left == 0`, read by the same `RETURNING` statement, inside the same transaction; the orphan row and its inode row are still created at unlink |
| Capture and generation holds | evaluated by `maintain_orphan` and `retire_serial` themselves; a held layer leaves its item in `maintenance`, woken by `wake_generation` as today |
| Nothing lost at forced unmount | a FORGET either commits whole or leaves the lookup row for `retire_native`; `retire_native` keeps its 64-row page |
| Reclamation automatic and bounded | inline part bounded by 14 cells; the remainder is ordinary queued items; the owner's idle rule is unchanged |
| Exact accounting | triggers unchanged in 2a and 2b; the lean trigger moves only `payload_bytes` to code that already selects the lengths |
| One writer, short jobs | everything is inside the job that was running anyway |

Concurrent callers and several Workspaces: all of it is per `(ns, serial)` inside one owner job. Today
one Workspace's `rm` makes every Workspace's jobs queue behind its maintenance steps; afterwards the
reclamation is charged to that Workspace's own FORGET jobs in its own lane.

Memory: no new resident structure. Storage: fewer transient rows; the index removal reduces the overlay.

### 3.4 Tests that pin the old behaviour

No test asserts exact statements for UNLINK, FORGET or their maintenance: the count tier's
`layerfs-workspace/tests/native_visit_cost.rs:199` covers the five jobs of one created file only. That
test has to be written first, at two directory sizes and beside unrelated rows.

| Test | Pins | Affected by |
| --- | --- | --- |
| `layerfs-overlay/tests/orphans.rs:419` `inode_reads_probe_the_orphan_domain_only_once_an_orphan_was_created` | the probe starts and never stops | clearing `orphan_seen` |
| `orphans.rs:250` `custody_and_orphan_jobs_keep_point_work_beside_unrelated_owners` | work of custody and orphan jobs | 2a |
| `orphans.rs:31, 216, 356`; `resources.rs:53`; `lifetimes.rs:78` | state before and after draining; a parked migration | 2a, 2b: re-read each before restaging |
| `layerfs-overlay/tests/native_lookup.rs:161` | a removed file opens under lookup custody | must keep passing; needs the wake-at-open of 2b |
| `native_lookup.rs:282, 430`; `native_revocation.rs:109`; `native_directory.rs:176, 259` | forget checks; `maximum_rows <= 64` | 2a (i), batch |
| `layerfs-overlay/tests/native_visit_fence.rs:272, 489` | forget followed by a drain | 2a |
| `layerfs-overlay/tests/engine.rs:59, 123` | `rows <= 64`, `data_bytes <= 65536`, no scan or temp b-tree in `explain_closed_reclaim` | 2d: the explain constant, and `data_bytes` for payload |
| `layerfs-overlay/tests/resources.rs:5` | `maintenance_targets > 0` after a shrink | not by 2a (a shrink, not a release) |
| `layerfs-workspace/tests/nonfile.rs:14`, `namespace_daemon_cases/orphans.rs`, `captured_namespace_holds.rs` | page bounds; orphan holds across captures | 2b |
| `layerfs-daemon/tests/owner.rs:53, 138` | idle cleanup with no further job; `maintenance_rows >= 512`, `>= 130` | any scheduling change; 2d keeps rows via `changes()` |
| `layerfs-daemon/tests/mounted_drain.rs:1395-1466` (FP-31), `:517-525` (FP-21) | `jobs >= lookups/64`, `rows <= 64 × jobs`, batched units | batch job, 2a (i) |
| `layerfs-daemon/tests/mounted_cycles.rs:162-200` | maintenance settles after each unmount | all |

---

## 4. Staged plan

Scope of the listing: commit 63c48d8dc, production LOC from `tools/production_loc.py --files` run on a
`git archive` of that commit; only files a stage touches are shown, so bracketed totals cover the files
listed. Tests are outside `src/`, in each package's `tests/`.

```
layerfs-overlay/                    [2718]  (partial)
  sql/                               [551]
    accounting.sql                    272   (O8: payload delete trigger)
    schema.sql                        279   (O9: drop payload_namespace_row)
  src/
    database/                        [436]
      connection.rs                   286   (O5: orphan_seen)
      statements.rs                   150   (O6: forget fence)
    lifetime/                       [1035]
      close.rs                        135   (O6)
      file_owners.rs                  357   (O2, O4: last-reference branch; O3: wake at open)
      native.rs                       376   (O6: forget; D1: batch)
      orphan.rs                       167   (O3: enqueue rules)
    maintenance/                     [696]
      native.rs                        85   (O2: no inline finish in retire_native)
      native_directory.rs              39   (O7)
      orphan.rs                       218   (O4, O6: item-free cores)
      ready.rs                        225   (O4: flag restore)
      reclaim.rs                      309   (O7: key-range steps, folded phases)
layerfs-daemon/                     [1785]  (partial)
  src/
    overlay/                        [1226]
      native_job.rs                   231   (D1: ForgetBatch)
      owner.rs                        553   (D2)
      queue.rs                        442   (D2)
    service/                         [511]
      filesystem_port.rs              511   (D1)
layerfs-fuse/                        [965]  (partial)
  src/
    ports.rs                          234   (D1)
    request/                         [731]
      callbacks.rs                    658   (D1: batch_forget)
      inline.rs                        73   (D1)
```

Overlay first:

| Stage | Change | Proving counter |
| --- | --- | --- |
| O1 | exact-count test for unlink, forget and their drain (tests only) | none moves; pins 22 (UNLINK) and 9 (FORGET) attempts besides the 3 transaction statements each, and 6 steps |
| O2 | 2a (i): custody row deleted at the last reference when no orphan row exists | C01 post-unmount maintenance jobs 1,135 → about 134 |
| O3 | 2b: no `SERIAL_RETIRE` with a new orphan; no `ORPHAN` while only lookups hold; wake at open | C03 `maintenance_data_bytes` 11,650 → about 7,786; maintenance jobs 6,011 → about 4,000 |
| O4 | 2a (ii): the last release finishes a small orphan; flag restored | C03 maintenance jobs → 17; dispatch wakes 3,549 → under 50 |
| O5 | `orphan_seen` cleared at zero orphan rows | C03 Inode attempts 38,041 → 24,002 |
| O6 | item-free cores; FORGET fence | statements of FORGET for an unlinked one-cell file: 36 → about 18 |
| O7 | 2d: key-range steps (payload at 14 or 32 until O8), folded phases | C06 post-unmount statements about 22,000 → about 7,000 at n = 14 |
| O8 | payload delete trigger without `OLD.data`; payload page 64 | C06 post-unmount transactions 1,190 → about 262 |
| O9 | drop `payload_namespace_row` | overlay logical bytes after C06: −0.36 % |

Then daemon:

| Stage | Change | Proving counter |
| --- | --- | --- |
| D1 | BATCH_FORGET as one job per 64 units | FP-31 in `mounted_drain`: owner jobs and transactions per batch k → ⌈k/64⌉ |
| D2 | no owner wake while a native request is uncompleted (2c item 3) | dispatch wakes of a multi-job cell that runs with maintenance pending |

O2 to O5 are independent of the daemon and each leaves the tree green on its own. O3 without O4 is
already an improvement (4 steps instead of 6, no move).

## 5. What I could not verify

- The 19 directory requests' statements (residuals in 1.3) were assigned by subtraction, not traced.
- Which kernel code path issues each GETATTR; the counts fit, the mechanism is from memory of FUSE.
- All time predictions. The scratch runs are python on macOS; I applied their ratios to Linux receipt
  times. The cost of one dispatch wake (5–15 µs) and of one orphan probe (1.0–1.5 µs) are estimates.
- Whether a longer FORGET job delays the following GETATTR enough to eat part of the gain (section 3.1).
- That no reader depends on an orphan held only by lookups being migrated early (2b item 2). Fold, retire
  and the captured-generation park tolerate it by source; the orphan tests have to confirm it.
- That a tombstone of a file born in the never-captured active generation is unnecessary (2e, last row).
- SQLite internals stated from knowledge rather than measured: that deleting a row with one overflow
  page does not read that page when no trigger names the blob. The EXPLAIN evidence only shows that the
  trigger forces the column load.
- Nothing was built or run against the product; no test was executed.

## 6. Also seen

- A held orphan migrates **one cell per transaction** [S `maintenance/orphan.rs:42-48`]: a 64 MiB file
  unlinked while open is 16,384 transactions. Outside this subject.
- `payload_account_update` also names `OLD.data` [S `sql/accounting.sql:166-169`], so every cell
  overwrite loads the old blob. Relevant to the rewrite cells.
