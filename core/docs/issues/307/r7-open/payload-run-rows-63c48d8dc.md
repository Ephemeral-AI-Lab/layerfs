# Large-file payload path at 63c48d8dc: model, split, designs, floor

> **Status:** Research; informative and not a product contract. Read-only analysis, 2026-10-09.

Labels: **[S]** read from source at `63c48d8dc` (paths relative to `core/crates/`), **[R]** decoded from a named
receipt under `core/docs/issues/307/checks/r7-optimization-20261009/`, **[I]** inference, **[E]** estimate,
**[X]** scratch measurement on the macOS host (Python 3.14 `sqlite3`, SQLite 3.51.2, real schema v22 and
accounting SQL copied from `63c48d8dc`, incompressible `os.urandom` data, product pragmas). Scratch numbers are
relative only. Scratch directory: `.../scratchpad/research/payload/` (`runs.py`, `pages.py`, `interval.py`,
`vm.sql`, outputs `out-*.txt`). No repository file was touched; nothing was built or run from the repository.

## 0. Headline findings

1. **"64 Payload executions per WRITE" is 32 upserts plus 32 trigger sub-programs, not a new statement.**
   `executions` is `SQLITE_STMTSTATUS_RUN` [S `layerfs-overlay/src/diagnostics/metrics.rs:132`], and SQLite
   increments that counter at the `OP_Init` of every trigger sub-program as well as of the statement
   [I, `libsqlite3-sys-0.38.2/sqlite3/sqlite3.c:105520-105589`; X: the CLI prints "Number of times run: 2"
   for one fresh `CELL_PUT`, 1 after dropping `payload_account_insert`]. Receipt 679: Payload attempts 16384
   (32 per WRITE), executions 32768, rows changed 49152 (16384 direct + 32768 trigger) [R]. Step 10 added no
   pre-read. The ledger says the same of its statement column [R LEDGER "Baseline ... 652-703"].
2. **The measured cells never grow the overlay file and never call `fallocate` per job.** Class B runs after a
   warm-up whose Workspace was reclaimed, so the measured `dd` allocates from the freelist: logical bytes +94208,
   free pages −18631, allocation attempts **1**, observations 516 [R 679 `resource_diagnostics`, calls 23→24].
   The warm-up of the same receipt (calls 3→4, a growing file) shows 511 attempts and 1030 observations [R].
   The earlier note's "512 `fallocate` per 64 MiB" is true for a growing file only; option (e) is worth 0 ms in
   every measured cell and about 3 ms per 64 MiB on a growing file [R, section 2.4].
3. **Time per 128 KiB WRITE job (288 µs) is 76 % Payload upserts** (219 µs = 32 × 6.86 µs), 14 % COMMIT
   (39.5 µs), 6 % other statements (16 µs), 5 % outside any statement (13 µs) [R 679].
4. **Half of each upsert is the accounting trigger** [X]: VM steps 171 → 76 without it (the receipt's 165 is
   the same statement with a bound parameter instead of `randomblob()`); host payload time 160.0 → 85.4 ms
   per 64 MiB.
5. **Run rows cut the Payload family to 17–24 % and pages go down** [X]: per 64 MiB host payload time V0
   160.0 ms, 16 KiB 56.6, 32 KiB 38.4, 64 KiB 26.5, 128 KiB 20.9; file 73.04 → 66.46 / 65.36 / 64.82 /
   64.54 MiB; pager writes per COMMIT 49.8 → 43.3 / 41.8 / 41.2 / 41.1.
6. **Run rows are not free for random access** [X]: a cold random 4 KiB read costs 5.6 µs today, 7.4 (16 KiB),
   9.7 (32 KiB), 13.1 (64 KiB), 19.1 µs (128 KiB), because SQLite walks the overflow chain page by page; a
   random 4 KiB overwrite in place is on par up to 32 KiB, +13 % per job at 64 KiB and +28 % at 128 KiB. The
   bound is a real trade; I recommend **32 KiB** as the default and give 64 KiB as the alternative (section 4a).
7. **C06 and C11 cannot be reached for incompressible data at 128 KiB windows**: the time outside the owner
   (78.7 ms, 94.9 ms) already equals or exceeds the targets (78.6, 85.8). The owner floor of this SQLite design is
   about 45–50 ms per 64 MiB at 128 KiB windows and about 33–36 ms at 1 MiB windows [E from R and X]. Section 7.
8. **No R7 cell measures a local READ.** C08's `cat` is served by the kernel page cache (0 READ) [R 687]. The
   local read path (per-byte loop, three window copies) is therefore unpriced by any receipt.

## 1. Receipts

| Cell | Receipt | Command ms | Target | Requests | Mutation jobs / service ms | Payload attempts / runs | Outside owner ms |
| --- | --- | ---: | ---: | --- | --- | --- | ---: |
| C06 | 679 | 226.8 | 78.6 | 517 (512 WRITE) | 513 / 147.6 | 16384 / 32768 | 78.7 |
| C07 | 683 | 499.4 | 171.6 | 1550 (1536 WRITE, 1 COPY_FILE_RANGE) | 1538 / 319.5 | 32768 / 65536 | 178.5 |
| C08 | 687 | 261.8 | 106.1 | 521 (512 WRITE, 0 READ) | 513 / 151.1 | 16384 / 32768 | 109.8 |
| C10 | 695 | 400.2 | 178.2 | 1546 (512 READ, 1024 WRITE, 1 COPY_FILE_RANGE) | 1025 / 177.0 | 16384 / 32768 | 82.5 (+55.1 owner wait) |
| C11 | 699 | 242.4 | 85.8 | 517 (512 WRITE) | 513 / 146.7 | 16384 / 32768 | 94.9 |

All [R] via `core/target/r7-summary.py` and `r7-statements.py`. "Outside owner" is command − owner wait − owner
service.

## 2. Task 1 — exact model at 63c48d8dc

### 2.1 One 128 KiB WRITE into a fresh local file

Request path [S]: `layerfs-fuse/src/request/callbacks.rs:511-551` (window check `:523`, `write::write` `:541`) →
`layerfs-fuse/src/operations/write.rs:17-26` → `NativeFilesystem::mutate` (`request/mutate.rs:139`) →
`Permit::handoff` runs the first step on the receive thread (`dispatch/admission.rs:253-299`) →
`NativeMutationVisit::perform` (`layerfs-workspace/src/operations/native_visit.rs:260-332`) →
`Overlay::mutate_native_visit` (`layerfs-overlay/src/lifetime/native_visit.rs:154-191`) → Workspace
`file/write.rs:10-79` decides (no base read: `:67-68`) → `apply_checked`
(`layerfs-overlay/src/namespace/compound.rs:144-236`).

One owner job (class Mutation), one transaction:

| # | Statement | Family | Where [S] |
| --- | --- | --- | --- |
| 1 | `FENCE_FILE` (Workspace row + mount + descriptor in one statement) | Workspace | `lifetime/native_visit.rs:67-83`, text `database/statements.rs:166-168` |
| 2 | `INODE_LOOKUP` (orphan probe skipped while `orphan_seen` is false, `lifetime/orphan.rs:42-45`) | Inode | `namespace/inode.rs:85-93` via `compound.rs:364-371` |
| 3 | `LAYER_ACTIVE` | Inode | `namespace/inode.rs:124-132` |
| 4 | `INODE_PUT`, the first writing statement | Inode | `namespace/inode.rs:163-184` |
| 4a | `PRAGMA main.freelist_count` | Startup | `database/connection.rs:296-303` |
| 4b | `Allocation::state()`: `fstat` + `lstat`; `fallocate(KEEP_SIZE)` + `state()` again only when the tail is short | — | `database/allocation.rs:64-100,136-175,182-191` |
| 4c | `BEGIN IMMEDIATE` | Begin | `database/connection.rs:283-285` |
| 5–36 | 32 × `CELL_PUT`, each with one `payload_account_insert` trigger program of two `UPDATE accounting` | Payload | `payload/stream.rs:36-64` → `payload/layers.rs:109-127`; trigger `sql/accounting.sql:46-49` |
| 37 | `FRONTIER_ADVANCE` | Workspace | `namespace/inode.rs:286-291` |
| 38 | `COMMIT` | Commit | `database/connection.rs:233-234` |

`native_effect` returns at once for a WRITE (`lifetime/native_mutation.rs:51-57`); `detach_orphan` returns for
`nlink != 0` (`lifetime/orphan.rs:57-59`).

Per job: attempts 40 (Workspace 2, Inode 3, Startup 1, Begin 1, Payload 32, Commit 1); executions 72;
rows changed 98 (Payload 96, frontier 1, inode 1).

**Reproduction of receipt 679 (measured command, calls 23→24) [R]:**

| Family | Model for 512 WRITE | Receipt attempts | Residual (the other 5 jobs: CREATE, LOOKUP, GETATTR, FLUSH, RELEASE) | Receipt executions | VM steps | ns |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Payload | 16384 | 16384 | 0 | 32768 | 2703360 (165.0 per upsert) | 112355695 |
| Commit | 512 | 514 | 2 | 514 | 1542 | 20325997 |
| Begin | 512 | 514 | 2 | 514 | 2570 | 219353 |
| Startup | 512 | 517 | 5 | 517 | 3636 | 323710 |
| Workspace | 1024 | 1030 | 6 | 1030 | 49921 | 2952591 |
| Inode | 1536 | 1549 | 13 | 1551 | 122902 | 4685381 |
| DirectoryEntry | 0 | 7 | 7 | 8 | 230 | 13667 |
| Lease | 0 | 9 | 9 | 19 | 1090 | 87874 |

Rows changed, Payload: 49152 = 16384 × 3 [R]. `write_cells` 16384, `write_input_bytes` 67108864 are implied by
the stored counts (`payload_cells` +16384, `payload_bytes` +67108864) [R].

**Program shape [X, SQLite 3.51.2, schema v22]:** `CELL_PUT` is 334 opcodes with 8 `OpenWrite` (table,
`sqlite_autoindex_payload_1`, `payload_namespace_row`, `payload_generation`, accounting four times), 3
`IdxInsert`, 9 `MakeRecord`, 2 trigger programs (insert and update arms). A fresh upsert runs 165 steps [R]; 76
without the trigger; 60 without the trigger and the two secondary indexes; one aggregate `UPDATE accounting` is
41 steps [X `vm.sql`].

**Pages [X `pages.py`, `runs.py`]:** a 4096-byte cell is 489 bytes in the leaf and one overflow page (1.125
table pages per cell). 64 MiB is 18699 pages = 73.04 MiB: table 18438, autoindex 73, `payload_generation` 73,
`payload_namespace_row` 51. Pager writes per COMMIT: 48.8 on a growing file, 49.8 on freelist reuse (32 data
cells need 36.4 pages; about 12 are rewritten hot pages). The receipt gives 39.5 µs per COMMIT, about 0.8 µs
per page [R]; C01's small jobs give 10.4 µs per COMMIT [R 659].

**Syscalls per job [S, R]:** 1 `read` + 1 `writev` on `/dev/fuse`; `fstat` + `lstat` (one `state()`); about 49
`pwrite` of 4 KiB at COMMIT [I: unix VFS writes page by page]; `fallocate` + a second `state()` only on a
growing file (0 in the measured cell, section 2.4).

**Copies of each payload byte:**

| # | Copy | Where |
| --- | --- | --- |
| K1 | user buffer → page cache (no writeback cache) | kernel [I] |
| K2 | page cache → the daemon's receive buffer | kernel [I] |
| U1 | receive buffer → `Arc<[u8]>` (one 128 KiB + 16 allocation per request) | [S] `layerfs-fuse/src/operations/write.rs:23`, `layerfs-workspace/src/operations/types.rs:16-19`; later clones are `Arc` clones (`file/write.rs:51`) |
| U2 | `source.to_vec()`, one 4 KiB allocation per cell | [S] `layerfs-overlay/src/payload/stream.rs:50` |
| U3 | `sqlite3_bind_blob64(..., SQLITE_TRANSIENT)`: SQLite's private copy | [S] `rusqlite-0.40.2/src/statement.rs:655-667` |
| U4 | `OP_MakeRecord` builds the record | SQLite [I] |
| U5 | record → leaf (489 bytes) and overflow page | SQLite [I] |
| K3 | `pwrite` at COMMIT → the file's page cache | kernel [I] |

Five user-space copies and three kernel copies. A partly covered cell adds one point read (`CELL_LOOKUP`), a
zeroed 4608-byte window (`payload/cells.rs:58-67`), a merge and `trim` (`cells.rs:83-110`).

**Time split [R 679, Mutation jobs]:**

| | Per WRITE job | Per 64 MiB (513 jobs) |
| --- | ---: | ---: |
| Service | 287.7 µs | 147.6 ms |
| Payload upserts (32 × 6.86 µs) | 219.4 µs | 112.4 ms |
| COMMIT | 39.5 µs | 20.3 ms |
| Other statements (fence 2.9, frontier 2.9, inode 3 × 3.0, pragma 0.6, BEGIN 0.4) | 16.1 µs | 8.3 ms |
| Not in any statement (`state()`, 32 × `to_vec`, Workspace decision, turn bookkeeping) | 13.4 µs | 6.9 ms |

The statement total is for all 518 jobs of the interval; the five non-WRITE jobs hold 0.26 ms of service [R].

### 2.2 One 64 KiB WRITE (`cp` through the kernel's splice fallback)

Same job with 16 `CELL_PUT`: 24 attempts, 40 executions [S]. C07 reproduces: 512 × 32 + 1024 × 16 = 32768
Payload attempts, receipt 32768 / 65536 [R 683]. Service per 64 KiB job: 172.6 µs (C10: 176.96 ms / 1025)
[R 695], so a cell costs (287.7 − 172.6) / 16 = 7.2 µs and the job's size-independent part is about 57 µs [I].
The 64 KiB size comes from the kernel: our `COPY_FILE_RANGE` answers `ENOSYS` once
(`request/callbacks.rs:588-602`), and the in-kernel fallback copies through a 16-page pipe [I, kernel
recollection]; the receipts show exactly 1 `CopyFileRange` and 1024 WRITE per 64 MiB [R 683, 695].

### 2.3 One 128 KiB overwrite of base bytes (C11)

Nothing is read from the base [S `layerfs-workspace/src/operations/file/write.rs:67-68`]. The first write
creates the active inode row with `inherited_cutoff` = the lower view's length (`namespace/inode.rs:134-149`);
bytes with no local row below the cutoff stay inherited. Every C11 window is cell-aligned, so there is no
partial cell and no `CELL_LOOKUP`: 16384 attempts, 32768 executions, identical to C06 [R 699]. The base side of
the whole command is 3 length batches, 3 reader grants and 0 object ids [R 699]. A partly covered cell over
base bytes would store a trimmed row with a validity mask and still read nothing from the base
(`payload/stream.rs:51-61`).

### 2.4 Admission [S, R]

`admit` runs once per writing transaction: `PRAGMA freelist_count`, one `state()` (two metadata syscalls),
then `Allocation::admit` (`database/connection.rs:291-331`). `required` = 256 MiB − reusable freelist bytes;
`fallocate` runs only when the tracked tail is below it (`allocation.rs:126-146`).

| Interval of receipt 679 | Admitted | `fallocate` attempts | `state()` calls | File growth | Service − statements |
| --- | ---: | ---: | ---: | ---: | ---: |
| Warm-up `dd` (calls 3→4), growing file | 518 | 511 | 1030 | +76312576 bytes | 149.29 − 139.24 = 10.05 ms |
| Measured `dd` (calls 23→24), freelist reuse | 514 | 1 | 516 | +94208 bytes | 147.86 − 140.96 = 6.89 ms |

So `fallocate` plus the second `state()` cost about 3.2 ms per 511 growing jobs, 6.2 µs each [R]. Statement cost
differs between the two shapes as well: upsert 6.32 µs growing against 6.86 µs reusing, COMMIT 50.1 µs against
39.5 µs [R].

### 2.5 One 128 KiB READ of local bytes [S; no receipt]

The owner job is `Command::FileRead` → `Overlay::read_file` (`layerfs-daemon/src/overlay/commands.rs:667`,
`layerfs-overlay/src/lifetime/orphan.rs:144-157`):

1. `check_file_read`: Workspace state, base source row, `file_read` row (`lifetime/file_owners.rs:330-348`,
   `lifetime/source.rs:74-103`).
2. `ORPHAN_LOOKUP` (`orphan.rs:153`), then inside `source_read` the Workspace state and source row again and
   `ORPHAN_LOOKUP` again (`payload/stream.rs:76-78`).
3. `LAYERS` (`payload/layers.rs:23-52`).
4. One `CELL_RANGE` per layer, 32 rows of 4 KiB for a dense window (`payload/stream.rs:190-202`); `stale` needs
   no SQL when the stamp equals the layer's epoch (`layers.rs:76-78`).

Copies per byte: SQLite assembles each row from leaf and overflow [I]; `row.get::<Vec<u8>>`
(`payload/cells.rs:20`); a **per-byte loop** with a validity test and `decided[]` bookkeeping
(`stream.rs:210-218`) into a zeroed `vec![0; n]` beside a second zeroed `decided` vector (`stream.rs:157,166`);
`value.clone()` of the whole `LocalRead` (`layerfs-daemon/src/overlay/file_port.rs:21`);
`local.get().clone()` again and `sink.write_all` into a fresh `Vec`
(`layerfs-fuse/src/operations/read.rs:83-86`, `layerfs-workspace/src/operations/file/read.rs:144`); the reply
`writev`. That is one row copy, one per-byte pass and three window copies.

Job fan-out per READ is known only for base files: 3 Read + 2 Lifecycle + 1 Source jobs per READ
(C10: 1543 / 1029 / 513 for 512 READ) [R 695]. Whether a local READ has the same fan-out is not shown by any
receipt, because C08 issues none [R 687].

## 3. Task 2 — the 79 ms outside the owner (C06)

| Part | ms | Basis |
| --- | ---: | --- |
| Docker exec create 2.9 + start 1.7 + status 2.0 + gaps | 6.9 | [R 679 `runtime.events.jsonl` 35–40]; every cell shows 5.9–8.2 ms |
| 512 WRITE round trips at C01's rate | about 21 | [I] C01: (215.1 − 7.0) ms / 5001 requests = 41.6 µs per small request [R 659] |
| Everything else: process start in the container, the kernel's two copies of 64 MiB and its page-cache work, `dd` zero-filling its buffer, the `Arc` allocation and copy, the reply | about 51 | unsplit; 71.8 ms inside the stream span − 21 |

Cross-checks [R]: C06's outside time per request is 138.9 µs against C01's 41.6 µs, so about 97 µs per WRITE
scales with bytes or is fixed start cost. C07 − C06 gives 99.8 ms for 1033 more requests (1024 WRITE of 64 KiB),
96.6 µs per request with half the bytes each; solving both with C01's 41.6 µs per request leaves about 50 ms for
64 MiB of byte-proportional work plus one process start in each cell.

What is unknown, and what would settle it:

- **The fixed start cost.** No cell runs a no-op command. One `true` command in the R7 harness measures it.
- **The `Arc` copy.** 131088 bytes is at glibc's default mmap threshold; the binary is glibc-linked with the
  system allocator [R `file core/target/r7-linux-release/release/layerfs-daemon`]. Whether each request pays an
  `mmap`/`munmap` or a heap trim with about 33 page faults is not observable in any receipt [I]. Per-thread
  `minflt` before and after the command would show it (the earlier per-request note proposes that observation).
- **Kernel copies.** Two copies of 64 MiB plus 16384 page-cache insertions; no counter.

A2's own 78.6 ms contains the same exec, start, kernel and round-trip costs, so our outside time is at most about
20 ms above A2's equivalent part [I].

## 4. Task 3 and 4 — designs, ranked by ms of owner service saved per 64 MiB of incompressible data

Scaling rule for every [E] below: receipt 679's family times (Payload 112.4 ms, COMMIT 20.3 ms, other 8.3 ms,
non-statement 6.9 ms) multiplied by the host ratio of the same family on a warm file [X]. The host ratio is
measured; applying it to Linux is the assumption.

| Rank | Change | Owner ms per 64 MiB (128 KiB jobs) | Saved | Storage per 64 MiB | Risk |
| --- | --- | ---: | ---: | --- | --- |
| baseline | today | 147.6 [R] | — | 73.04 MiB | — |
| 1 | (a) run rows, 32 KiB | about 61 | about 86 | 65.36 MiB | medium–high |
| 1′ | (a) run rows, 64 KiB | about 53 | about 95 | 64.82 MiB | medium–high |
| 2 | (d) aggregate payload accounting, alone | about 97 | about 50 | unchanged | medium |
| 3 | (f) 1 MiB window, after (a) | about 34–36 | about 15–19 in the owner, 19 outside | unchanged | policy |
| 4 | (c) drop both secondary indexes, alone | about 122 | about 26 | −124 pages | medium |
| 5 | (b) bind the borrowed slice, alone | about 144 | about 3 | unchanged | low |
| 6 | (d) after (a) | — | about 2.5 | unchanged | medium |
| 7 | (c) after (a) | — | about 1–2 | −10 pages | medium |
| 8 | (e) admission by the job's own bound | — | 0 in the measured cells; about 3 on a growing file | unchanged | medium |
| — | (g) COPY_FILE_RANGE | C07 and C10 only | section 4g | unchanged | medium |
| last, separate | zero elision | zero data only | section 4h | — | benchmark trap |

(a), (c) and (d) overlap: together they reach about 47–50 ms, not the sum.

### 4a. Run rows

**Row format.** Columns are unchanged. A row is one of:

- a *cell row*, exactly today's row: `length(data) <= 4096`, optional validity mask;
- a *run row*: `validity IS NULL`, `length(data)` a multiple of 4096, and the run inside one aligned slot of
  `RUN_BYTES`: `cell_offset / RUN = (cell_offset + length(data) - 1) / RUN`.

```sql
data BLOB NOT NULL CHECK(length(data) BETWEEN 1 AND 32768 AND (length(data)<=4096 OR
     (validity IS NULL AND length(data)%4096=0 AND cell_offset/32768=(cell_offset+length(data)-1)/32768))),
```

The CHECK was run in scratch at 64 KiB: a masked run, a run crossing a slot and a 5000-byte row are refused; a
3-cell run inside a slot is accepted [X]. Schema: `sql/schema.sql:55`, comment `:1`, `PRAGMA user_version`
`:283`, and the readback `database/startup.rs:136` (22 → 23). The database is created fresh by every daemon
(`database/startup.rs:61-69`), so there is no migration.

Two invariants, the first as today, the second new:

- I1: a row is wholly live or wholly stale. Shrink boundaries are cell-aligned (`payload/layers.rs:164`), and
  the shrink below splits the one row that straddles its boundary, so `stale(row.cell_offset, epoch)` stays the
  whole test for every reader.
- I2: live rows of one layer never overlap in bytes.

**Why an aligned slot.** Every row that can intersect `[a, b)` has `cell_offset` in `[a − a % RUN, b)`: one
indexed range, with `cell_offset + length(data) > a` as a residual filter that reads the record header only.
Plans stay `SEARCH` [X]. Without alignment a covering-row lookup would need an unbounded backward seek.

**Write algorithm** (`write_cells`, `payload/stream.rs:18-66`; proposed new file `payload/runs.rs` so
`stream.rs` stays small). The window is split into an optional head partial cell, whole cells, an optional tail
partial cell. Whole cells are cut at slot boundaries into pieces `[p, q)`. Per piece:

1. One metadata statement: `rowid, cell_offset, epoch, length(data), validity IS NOT NULL` of the rows with
   `cell_offset` in `[slot, q)` (at most `RUN/4096` rows, no blob loaded).
2. Stale rows that intersect `[p, q)` are deleted (garbage; today they are replaced one by one or wait for the
   cleaner).
3. A live run row that straddles `p` or `q` receives the intersecting bytes in place (incremental blob write);
   at most one on each side. The piece shrinks to the middle `[p', q')`.
4. Middle: if exactly one live dense row equals `[p', q')`, write in place. Otherwise one range `DELETE` of
   the rows inside `[p', q')` if there are any, then one `INSERT` of a run row `[p', q')` bound from the borrowed
   slice.

Head and tail partial cells: if the covering row is live, dense and long enough, write the bytes in place with
no read; otherwise today's path unchanged (point read, merge, upsert of a cell row).

Cases the lead asked for:

| Case | Work | Versus today |
| --- | --- | --- |
| Append, 128 KiB | 2 or 4 pieces: per piece 1 metadata read (0 rows) + 1 insert | 32 upserts |
| Append, 4 KiB at a time | 1 metadata read + 1 insert of a one-cell row. A neighbouring row is **not** extended: growing a blob rewrites it | 1 upsert; same rows, same bytes, same pages [X: 18.46 MiB both] |
| Overwrite inside a run | 1 metadata read + 1 in-place write; no index, no trigger, no size change | 1 upsert per cell |
| Overwrite straddling runs | 1 metadata read + up to 2 in-place writes + at most 1 delete and 1 insert per piece | 1 upsert per cell |
| Write into a hole | 1 metadata read + 1 insert of exactly the written cells | same bytes stored |
| 1-byte write at a cell boundary, inside a run | 1 lookup + 1 in-place byte | point read 4 KiB + upsert 4 KiB |
| Whole-window write over alternating cells | per piece: 1 metadata read, 1 range delete, 1 insert; the range ends as one run | 32 upserts; the fragmentation stays |
| Shrink inside a run | the one row containing the boundary is read and replaced: a run row for the whole cells below the boundary cell and, when `to % 4096 != 0`, today's trimmed cell row; the tail is freed at once | today rewrites at most one 4 KiB cell and leaves the tail to the cleaner |
| `truncate` to 0 or to a slot boundary | no row touched, as today | same |
| Regrow and rewrite | stale rows met by a write are deleted in step 2; I1 holds, so no row is half live | stale cells replaced by upserts |
| Sparse cell put inside a run (`put_cell`, `payload/access.rs:17-41`) | the run is split around that cell: at most one row rewritten | one cell upsert |

**Incremental blob write.** `rusqlite` is locked at 0.40.2 and the overlay already enables its `blob` feature
(`core/Cargo.lock:968-969`, `layerfs-overlay/Cargo.toml` dependencies) [S]. `Connection::blob_open`,
`Blob::reopen`, `Blob::write_at`, `Blob::read_at_exact` and `Blob::close` are safe functions
(`rusqlite-0.40.2/src/blob/mod.rs:218,257,297`, `blob/pos_io.rs:27,166`), so `#![forbid(unsafe_code)]`
(`layerfs-overlay/src/lib.rs:6`) holds. No new dependency. Checked in scratch [X]:

- an in-place write leaves `accounting` unchanged and fires no trigger; length and epoch are unchanged;
- `COMMIT` with an open write handle fails ("SQL statements in progress"), so the handle is opened inside
  `write_cells`, moved with `reopen`, and closed with `close()?` before the job returns;
- a write past the row's end is refused by the API.

Two consequences for the engine: the handle bypasses `metrics::query`, so it needs its own counters in
`PayloadWork` (opens, in-place bytes, elapsed) to keep the per-job receipt exact; and `before_write` admission
(`metrics.rs:110-117`) is not triggered by a blob write, which is safe only because `INODE_PUT` always precedes
`write_cells` in `apply_checked` (`compound.rs:176-229`) — assert it rather than assume it.

**Readers and what each needs.**

| Reader [S] | Assumes today | Needs |
| --- | --- | --- |
| `compose_layers`, `payload/stream.rs:190-219` | rows start at or after `start − start % CELL` | lower bound at the slot, residual length filter; row bytes are already handled by length (`:209`). Unverified detail: `Stored::valid` and `expand` reject `data.len() > CELL_BYTES` (`cells.rs:35`); `expand` must stay cell-only |
| `stored` / `CELL_LOOKUP`, `payload/layers.rs:92-108` | exact key | covering-row lookup (`cell_offset <= ?` in the slot, `DESC LIMIT 1`) for the partial-cell path, shrink, `cell_at` |
| `shrink`, `payload/layers.rs:143-194` | boundary cell is its own row | the split above |
| `cell_at` / `put_cell`, `payload/access.rs:17-104` | one cell per row | read the cell slice of a covering run; split on a sparse put |
| `captured_run_step`, `payload/captured_runs.rs:62-108,126-135` | `length <= CELL_BYTES` (`:83`), next seek `offset + CELL_BYTES` (`:97-100`), window ends at the cell end (`:127`) | accept the run shape; next seek `offset + length` rounded up to a cell; emit the window to the row end. Emitting one cell at a time from a 32 KiB row would load the row once per cell |
| `CapturedRunCursor::advance`, `payload/captured_types.rs:104-118` | `cell.offset + cell.length` | already length-based; `CellMetadata` keeps offset and length |
| Workspace scan, `layerfs-workspace/src/construction/captured/scan.rs:141-154` | window and capacity `<= CELL_BYTES`, mask `<= MASK_BYTES` (`:145-147`) | bound by `RUN_BYTES` and `RUN_BYTES / 8` |
| `compose_cell`, `lifetime/composition.rs:200-228,249-296` | one cell per step | per row: when the upper layer has no live row in the row's range and the row is live and below the cutoff, move it with `UPDATE payload SET gen,epoch` as `move_orphan_cell` already does (`maintenance/source_wait.rs:101`); otherwise today's per-cell merge for each cell of the row, then drop the row. One step is at most `RUN_BYTES` |
| `move_orphan_cell`, `maintenance/source_wait.rs:73-114` | same | same rule; its `UPDATE` branch already moves a row without composing |
| `clean_live_item` (STALE), `maintenance/garbage.rs:127-146` | row wholly stale or live | unchanged under I1 |
| `retire_generation` phase 0, `garbage.rs:13-34`; orphan and serial retirement, `maintenance/orphan.rs:42,91,157`; terminal reclaim, `maintenance/reclaim.rs:9,167-182` | rows, `LIMIT 14` | unchanged; a step frees up to 14 runs. The 14 came from a 64 KiB byte window (`reclaim.rs:7`); deleting loads no blob bytes, but freeing walks each chain |

**Accounting.** Triggers stay per row: `payload_bytes` is unchanged and exact; `payload_cells` becomes a row
count (`sql/accounting.sql:46-53,166-169`, `database/accounting.rs:9`). `debt_upper_bytes`
(`accounting.rs:86-93`) only gets smaller. In-place writes change neither.

**Daemon charges** (`layerfs-daemon/src/overlay/commands.rs`): `CapturedRun` reply `CELL_BYTES + MASK_BYTES`
(`:390-394`) becomes `RUN_BYTES + RUN_BYTES / 8`; `CapturedCell`/`SourceCell` (`:386-388`), `Publish` (`:363-371`)
and `ReaderSymlink` (`:403-408`) stay one cell. Maintenance byte accounting counts row bytes already.

**The bound, from the product.** SQLite stores a blob beyond 489 bytes as a singly linked chain of 4092-byte
overflow pages and has no index into it under `auto_vacuum=NONE` (`database/profile.rs:58`), so reaching byte
`x` of a row reads `x / 4092` pages first [I, SQLite `accessPayload`]. `RUN_BYTES / 4092` is therefore the
worst-case page walk of one random access and of one in-place write, and the unit of every maintenance step.
It must also not exceed the read or write window. Measured [X, minimum of three runs, warm OS cache, cold
2 MiB pager]:

| | V0 | 16 KiB | 32 KiB | 64 KiB | 128 KiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| Sequential 128 KiB writes, payload ms per 64 MiB (warm file) | 160.0 | 56.6 | 38.4 | 26.5 | 20.9 |
| … COMMIT ms | 48.5 | 44.0 | 41.0 | 39.2 | 38.3 |
| Sequential 64 KiB writes, payload ms | 158.3 | 62.1 | 39.9 | 28.0 | 28.6 |
| Pager writes per COMMIT, 128 KiB job (growing / warm) | 48.8 / 49.8 | 42.3 / 43.3 | 40.8 / 41.8 | 40.2 / 41.2 | 40.1 / 41.1 |
| File size after 64 MiB | 73.04 MiB | 66.46 | 65.36 | 64.82 | 64.54 |
| Random 4 KiB overwrite in place, µs per job | 33.0 | 33.1 | 31.3 | 37.3 | 42.3 |
| … payload part, µs | 13.0 | 12.1 | 13.2 | 16.6 | 23.3 |
| Whole-row rewrite instead of in-place, µs per job | — | 92 | 66 | 86 | 133 |
| 1-byte write at a cell boundary, µs per job | 35.5 | 28.6 | 30.4 | 32.2 | 39.9 |
| Sequential read, SQL delivery ms per 64 MiB | 34.0 | 21.6 | 18.3 | 18.8 | 17.9 |
| Random cold 4 KiB read, µs (best of row query, `substr`, incremental) | 5.6 | 7.4 | 9.7 | 13.1 | 19.1 |
| Reclaim of 64 MiB by 14 rows, ms / transactions | 221.6 / 1172 | 69.5 / 294 | 42.7 / 148 | 30.0 / 75 | 23.8 / 38 |

Other one-row costs at 64 KiB [X]: truncating a run to half is 79 µs; moving a run to another generation by
`UPDATE` is 75 µs against 268 µs for the same 64 KiB as sixteen cell rows.

Worst cases against today, so nothing is hidden:

- **Random 4 KiB read of bytes that were written in large windows gets slower**: +1.8 µs (16 KiB), +4.1 µs
  (32 KiB), +7.5 µs (64 KiB) on the host. `substr()` does not help; SQLite loads the whole value. This applies
  only to READs the kernel page cache does not serve.
- **Random 4 KiB overwrite inside a run**: par at 16 and 32 KiB, +13 % per job at 64 KiB, +28 % at 128 KiB.
- **Shrink strictly inside a run**: one row of at most `RUN_BYTES` is rewritten once, against one cell today.
- **Storage**: no shape stores more. A k-cell run needs at most k overflow pages plus one leaf cell where today
  needs k overflow pages and k leaf cells; one-cell writes produce today's rows exactly [X `shapes`: alternating
  cells 18.46 MiB both, appends 18.46 MiB both, overwrite of the alternating shape 36.64 MiB both]. A stale run
  holds at most what its stale cells hold today until the same cleaner item runs.

**Recommendation: 32 KiB.** It keeps 91 % of the 64 KiB saving (about 86 of 95 ms [E]), keeps random overwrite
at today's cost, and halves the random-read penalty. 64 KiB matches the kernel's 64 KiB splice WRITE exactly
(one row per `cp` WRITE instead of two) and is worth about 8 ms more per 64 MiB sequential [E]. 128 KiB is not
justified: +28 % on random overwrite for 4 ms. This is a choice for the lead; both are generic.

**Predicted counters per 128 KiB WRITE into a fresh file** (32 KiB / 64 KiB): Payload attempts 8 / 4 (half
metadata reads, half inserts), executions 12 / 6, rows changed 12 / 6, VM steps about 4 × (165 + 33) = 790 /
2 × 198 = 400 (from 5280) [X program sizes], pager writes per COMMIT 41.8 / 41.2, user-space copies of each
byte 4 (U1, U3, U4, U5), `payload_cells` per 64 MiB 2048 / 1024, overlay pages 16732 / 16593 (from 18699).

**Memory.** One job at a time owns the connection, so the additions are per daemon, not per request: SQLite's
transient bind copy and record buffer of one row (2 × `RUN_BYTES` instead of 2 × 4 KiB), and one row loaded
per read step. Captured-scan windows and their daemon charge grow from 4.6 KiB to `RUN_BYTES × 9/8`.

**Tests that pin the old shape** (all under `core/crates/`):

- `layerfs-overlay/tests/resources.rs:14` `payload_cells == 32` after a 131072-byte write; `:87,98,104`
  `== 512` / `<= 512` for 2 MiB; `:32-33` 1 cell and 1 byte after `resize(1)` (holds only with the synchronous
  split).
- `layerfs-overlay/tests/payload_fragmentation.rs` `dense_fragmentation_does_not_enlarge_a_later_request`:
  `(over_dense.0, over_dense.2) == (over_fresh.0, over_fresh.2)` and
  `edge_fresh.0 == over_fresh.0 - 32 * 2 + 25 * 2 + 2`. The first becomes "the fragmented overwrite costs no
  more than a fixed bound and less than today"; the second encodes two executions per cell DML.
- `layerfs-overlay/tests/payload.rs` `shrink_work_is_independent_of_discarded_data_and_regrow_reads_zero`:
  `shrink_large == shrink_small` (statements can stay equal; VM steps do not depend on blob size) and
  `pages_after.1 - pages_before.1 <= 2`, which the split breaks by freeing the tail at once;
  `holes_cost_no_rows_and_tiny_and_dense_files_report_measured_pages` prints pages per cell.
- `layerfs-overlay/tests/captured_runs.rs:39` `read.data.len() <= CELL_BYTES`; `:197` `metadata_rows == 97`;
  `:256,264` `(metadata_rows, stale_rows) == (20, 10)` and `(19, 9)` (these build files from one-cell writes and
  should keep their counts).
- `layerfs-overlay/tests/costs.rs:21-24` `write_cells == 1`, `cell_copy_bytes == CELL_BYTES`; `:62`
  `returned_blob_bytes == CELL_BYTES + 64`.
- `layerfs-overlay/tests/engine.rs:199`, `startup_cost.rs:56`, `directory_links.rs:172` `schema_version == 22`.
- `layerfs-daemon/tests/captured_runs.rs:122-123` `read.data.len() == CELL_BYTES`, capacities
  `<= CELL_BYTES` / `<= MASK_BYTES`.
- `layerfs-workspace/tests/payload.rs` (three reference-model tests) and `layerfs-overlay/tests/payload.rs`
  `layered_reads_match_a_reference_model_across_writes_resizes_capture_and_install` are shape-independent and
  are the safety net. They need new cases with writes larger than one run and shrinks inside a run.

**Risk.** Medium–high. Eight readers change in one commit because the writer cannot create a run before every
reader understands it. The shrink split and the stale-row deletion are the places a byte can be lost.

**Concurrency.** One connection, one job at a time: nothing here adds a wait. A job's turn gets shorter. With
several mounted Workspaces the rows are separated by `ns` as today.

### 4b. Copies

| Copy | Change | Saves per 64 MiB | Notes |
| --- | --- | ---: | --- |
| U2 `to_vec` (`stream.rs:50`) | bind `source` directly; `store` takes borrowed slices (`layers.rs:109-127`) | about 3 ms [E; X: +8.6 ms on the host with the copy] | 16384 allocations gone. `costs.rs:24` pins `cell_copy_bytes == CELL_BYTES`. Subsumed by (a), which binds the slice of a run |
| U3 bind copy | not removable: rusqlite 0.40.2 always passes `SQLITE_TRANSIENT`; a zeroblob insert plus blob write was no faster in the earlier scratch | 0 | — |
| Per-byte read loop (`stream.rs:210-218`) | for the top layer, a row with `validity` NULL is one `copy_from_slice` into the undecided range plus a `fill` of `decided` | unmeasured; about 1–2 ns per byte is 65–130 ms per 64 MiB of local READ [E] | no receipt has a local READ |
| `LocalRead` clones (`file_port.rs:21`, fuse `operations/read.rs:80,85`) and `sink.write_all` (`file/read.rs:144`) | move the window out of the completion; hand `local.data` to the reply | 2–3 copies of each window | touches the completion's ownership; no receipt prices it |
| U1 `Arc<[u8]>` (`write.rs:23`) | needed while the request may park; a fixed pool of 16 window buffers per mount would remove the allocation, not the copy | unknown (section 3) | 2 MiB per mount resident for every mount; I do not recommend it before the fault count is measured |

### 4c. Secondary indexes on `payload`

| Index | Only reader [S] | Replacement | Invariant |
| --- | --- | --- | --- |
| `payload_namespace_row (ns,rowid)` | terminal reclaim, `maintenance/reclaim.rs:9` (`INDEXED BY`), delete by rowid `:175` | page through the UNIQUE index: `WHERE ns=?1 ORDER BY serial,gen,cell_offset LIMIT 14`. Without that reorder the planner sorts every step: reclaim 222 → 659 ms [X]; with it the plan is `SEARCH ... sqlite_autoindex_payload_1 (ns=?)` [X] | none beyond the key order; holds for orphan rows (`gen = −1`), which share the table |
| `payload_generation (ns,gen,serial,cell_offset)` | `retire_generation` phase 0, `maintenance/garbage.rs:14-16` (`INDEXED BY`) | walk the generation's inode rows through `inode_capture (ns,gen,serial)` as phase 3 already does (`garbage.rs:78-101`) and delete each serial's rows through the UNIQUE prefix | every payload row `(ns,serial,gen)` has an inode row `(ns,serial,gen)` until its payload is gone |

The invariant holds on the paths I read: writes use the layer `put_inode_domain` just wrote
(`namespace/compound.rs:176-229`, `namespace/inode.rs:163`); consolidation creates the upper inode row first and
deletes the lower one only when no cell remains (`lifetime/composition.rs:187-199,215-242`); orphan moves target
the `gen = −1` row (`maintenance/orphan.rs:41-48`); terminal reclaim deletes payload before inodes. For the
orphan domain, `retire_generation` skips held serials in both phases (`garbage.rs:19,88`), so their inode rows
stay with their payload. **Not verified:** the delete order inside `retire_serial` and the orphan teardown
(`maintenance/orphan.rs:85-177`). A row left without its inode row would leak until the Workspace closes.

The unanalysed planner uses `payload_generation` for `CELL_RANGE` today [X]; after the drop the UNIQUE index
serves the same search.

Saving alone: host payload 160.0 → 126.3 ms, COMMIT 48.5 → 42.4 ms, about 26 ms on Linux [E]; 124 fewer pages.
After (a): about 1–2 ms and 10 pages. Per small file it removes 2 of 3 index inserts per payload row. Tests:
plan assertions in `payload_fragmentation.rs:13-17` (must stay `SEARCH`), the reclaim and retirement tests in
`engine.rs` and `lifetimes.rs`.

### 4d. Accounting triggers

Cause: every payload row insert runs one trigger program of two `UPDATE accounting`, each rewriting a
17-column STRICT row with 16 CHECKs (`sql/accounting.sql:2-20,46-49`): 95 of 171 VM steps, 2 of 3 changed rows,
47 % of the host statement time [X].

Change: drop the three payload triggers and keep the same two counters exact from the engine: each payload DML
site adds its row and byte delta to a per-transaction pair, flushed by one `UPDATE accounting ... WHERE ns IN
(0, ?)` before COMMIT in `transaction` (`database/connection.rs:233`). The sites are `store` (`layers.rs:121,131`),
the deletes in `garbage.rs:24,138`, `composition.rs:223`, `source_wait.rs:96,108`, `maintenance/orphan.rs` and
`reclaim.rs:175`. Deletes get exact bytes from `RETURNING length(data)+ifnull(length(validity),0)`. An upsert
cannot report the row it replaced, so the write needs the old lengths first: one metadata range read per
window, which (a) performs anyway.

What the triggers protected: exactness on every path with no cooperation from the code. A missed site drifts
silently, and `resources()` feeds admission debt (`database/accounting.rs:86-93`). Only tests that compare the
counters with a count of the table would catch it.

Predicted per 128 KiB WRITE on today's shape: executions 64 → 32 + 1 read + 1 update, rows changed 96 → 34,
VM steps 5280 → about 2500. Saving about 50 ms per 64 MiB alone [E], about 2.5 ms after (a). It is a schema
change (v+1). Because (a) removes fifteen sixteenths of the trigger runs without touching the triggers, I rank
(d) as a follow-up to (a), not a first step. The same lever exists for every other table: a C01 file runs about
ten trigger programs; that is outside this subject.

### 4e. Admission

In the measured cells nothing can be saved: 1 `fallocate`, one `state()` per job [R]. On a growing file the
refill fires on every transaction because any growth leaves the tail below 256 MiB (`allocation.rs:136`).

Amortizing without a larger reservation and without a weaker guarantee: admit each job against **its own**
growth bound instead of the blanket `MUTATION_GROWTH` — a WRITE job declares, say, its input plus a fixed page
overhead; the refill fires when `tail < CLEANUP_HEADROOM + bound(job)`. Every job still has its full bound
reserved before `BEGIN`; the reservation is still topped up to exactly 256 MiB; a job with the blanket bound
refills as today. `fallocate` then runs once per about 127 MiB of growth: 511 → 1 per 64 MiB. Saving about 3 ms
per 64 MiB of growing writes [R]. The `state()` call stays: it observes truncation and path identity
(`allocation.rs:64-100`). Files: `connection.rs:177-186,291-331` (`atomic` gains a bound), `allocation.rs:120-180`.
Pinned by `tests/transactions.rs:159-161,307-309`, `tests/costs.rs:36-37`, `tests/startup_cost.rs:48-50`. It
changes the S6 admission contract's wording and needs a derived, reviewed bound per job class; rank last.

### 4f. A window larger than 128 KiB

Constants [S]: `WRITE_WINDOW`, `READ_WINDOW` = 128 KiB (`layerfs-overlay/src/contract/types.rs:12,14`);
`max_write` and `max_readahead` are set from `READ_WINDOW` (`layerfs-fuse/src/mount/profile.rs:42-44`);
`MAX_INPUT_BYTES = WRITE_WINDOW + 510`, `HANDOFFS = 16` (`layerfs-fuse/src/dispatch/types.rs:4,8`);
`OwnerConfig.bytes` = 8 MiB (`layerfs-daemon/src/overlay/owner.rs:29`); a mutation's charge is its input plus
facts plus 4606 bytes (`layerfs-workspace/src/operations/native_visit.rs:255-257`). Checks:
`request/callbacks.rs:523`, fuse `operations/write.rs:18`, workspace `file/write.rs:18`, overlay
`compound.rs:128`, `stream.rs:122,144`. The kernel's ceiling is 256 pages, 1 MiB [I].

| | 128 KiB | 1 MiB |
| --- | ---: | ---: |
| WRITE requests per 64 MiB (`dd`) | 512 | 64 |
| WRITE requests per 64 MiB (`cp` fallback, 16-page pipe) | 1024 | 1024, no gain [I] |
| READ requests per 64 MiB | 512 | 64 |
| Input held per in-flight WRITE | 128 KiB | 1 MiB |
| Per mount, 16 slots by count | 2.06 MiB | 16 MiB |
| Read job reply: data + `decided` + inherited bitmap | about 0.27 MiB | about 2.1 MiB |
| Dirty pages of one transaction | about 41–50 | about 270 of the 512-page pager |
| Owner turn of one job after (a) | about 0.1 ms | about 0.55 ms [E] (today's 128 KiB job is 0.29 ms) |

Saving per avoided request: 41.6 µs outside the owner (C01's rate) plus about 30 µs of size-independent job
cost inside it (16 µs statements, about 8 µs hot pages at COMMIT, about 6 µs not in a statement) [R, I]. For
C06, C08 and C11: 448 × about 72 µs ≈ 32 ms, of which about 19 ms is outside the owner.

**Is "fewer, larger slots under the same byte credit" expressible? Yes.** The dispatcher already records each
slot's input bytes (`dispatch/admission.rs:213-218`); admission would add a byte sum per lane and wait, as it
waits for a slot today, while the sum would exceed the existing 16 × `MAX_INPUT_BYTES` = 2,105,312 bytes. That
admits two 1 MiB writes per mount. The daemon-wide bound is already in bytes: the owner's 8 MiB credit admits
seven 1 MiB jobs where it admits about sixty 128 KiB ones, for any W Workspaces × E Execs. Neither byte bound
changes. What does change is `WRITE_WINDOW` itself, a named job bound, the length of one owner turn that every
other mount waits behind, and the read reply size. Whether that is "raising a limit" is the lead's decision; I
would not do it before (a).

### 4g. COPY_FILE_RANGE

Today: one request, `ENOSYS`, then READ + WRITE pairs with 64 KiB WRITEs (`request/callbacks.rs:574-602`) [S, R].

What the rows can express [S `sql/schema.sql:19-36,48-58`]: an inode row has one `inherited_cutoff`, meaning
"bytes of **this serial** below the cutoff with no local row come from this serial's lower view". A payload row
holds bytes. Nothing can say "inherit from content X at offset". A new file has cutoff 0. So a copy must copy
bytes; a reference copy of base content would need a new row kind read by every reader and by Commit, which I
do not propose.

A correct bounded implementation:

1. `copy_file_range` callback: flags must be 0; both inodes and both descriptors resolved and fenced like READ
   and WRITE; the reply is the byte count.
2. One request copies at most one window and may return short; the kernel returns the short count and `cp`
   loops [I]. No request does unbounded work.
3. Inside: the existing read path of the source window (local composition plus inherited spans through the
   Store, exactly a READ's custody), then the existing write visit at the destination offset. Two owner visits,
   not atomic across them, which the call does not require.
4. The kernel drops the destination's cached pages itself after the reply [I]; the product sends no
   notification today (note 77) and I did not verify that none is needed here.
5. Memory: one window per in-flight copy, the same bound as a READ reply.

Effect [E]:

- C10 (base source): 512 READ + 1024 WRITE → 512 copies: 1024 fewer round trips ≈ 43 ms outside the owner; 512
  fewer write jobs ≈ 15 ms of fixed job cost; 64 MiB no longer crosses the kernel boundary twice.
- C07 (local source): 1024 WRITE → 512 copies: 512 fewer round trips ≈ 21 ms; but the 512 source reads, which
  the page cache answers today for free, become local read jobs. With today's read path that could cost more
  than it saves; after (a) and the read-path copies it should be a net gain of roughly 20–40 ms.

Risk medium: a new opcode with read and write custody in one request; needs its own coherence review.

### 4h. Zero elision — zero-filled data only; ranked last and separately

Rule: a fully covered all-zero cell at or above the layer's cutoff is stored as absence. It is exact for both
readers (`payload/stream.rs:220-222`, `payload/captured_runs.rs:156-171`). It saves nothing for any non-zero
byte, and nothing for C11 even with zeros: below the cutoff absence means inherited, so zeros over base bytes
must be stored. For C06 it removes every payload row and would leave about 20–25 ms of owner service [E]. The
cells write `/dev/zero`; a gain from this is a property of the benchmark's data, not of the product.

## 5. Staged plan

Each stage is one commit that leaves the tree green; the proving counter is read from one class-B sample at
the tip (C06 unless stated).

| Stage | Change | Files | Proving counter | Risk |
| --- | --- | --- | --- | --- |
| 1 | Bind the borrowed slice; no `to_vec` for whole cells | overlay `payload/stream.rs`, `payload/layers.rs`; `tests/costs.rs` | `cell_copy_bytes` per 64 MiB 67108864 → 0; statement counts unchanged | low |
| 2 | Local read: slice copy for dense top-layer rows; one state read and one orphan lookup per read job | overlay `payload/stream.rs`, `lifetime/orphan.rs`; a new `PayloadWork` field for bytes taken by the per-byte path | that field = 0 for a dense local READ in an overlay test (no R7 cell has a local READ) | low |
| 3 | Run rows, schema v23: CHECK, writer, every reader in section 4a, captured windows up to one run, daemon charge | overlay `sql/schema.sql`, `database/startup.rs`, `payload/*`, `lifetime/composition.rs`, `maintenance/source_wait.rs`; workspace `construction/captured/scan.rs`; daemon `overlay/commands.rs`; architecture notes 19 and 28; the tests listed in 4a | Payload attempts per 128 KiB WRITE 32 → 8 (32 KiB) or 4 (64 KiB); executions 64 → 12 or 6; `payload_cells` per 64 MiB 16384 → 2048 or 1024; overlay logical bytes must fall (76685312 → about 68.5 M or 68.0 M) | medium–high |
| 4 | Aggregate payload accounting, schema v24 | overlay `sql/accounting.sql`, `database/connection.rs`, the delete sites | Payload executions = attempts; rows changed per WRITE = rows written + 2 | medium |
| 5 | Drop `payload_namespace_row`, then `payload_generation` (two commits), schema v25/v26 | overlay `sql/schema.sql`, `maintenance/reclaim.rs`, `maintenance/garbage.rs` | b-tree write cursors of the insert program (`explain_program`); warm-up cleanup time must not rise | medium |
| 6 | COPY_FILE_RANGE | fuse `request/callbacks.rs`, a new operation file; workspace and daemon ports | C10 requests 1546 → about 522 | medium |
| 7 | Window by bytes under the existing byte bounds (only if the lead accepts the changed job bound) | overlay `contract/types.rs`; fuse `mount/profile.rs`, `dispatch/*` | C06 requests 517 → about 69 with the per-mount admitted-bytes bound unchanged | policy |
| 8 | Admission by the job's own growth bound | overlay `database/connection.rs`, `database/allocation.rs` | allocation attempts on a growing 64 MiB 511 → 1 (warm-up interval of the receipt) | medium |
| separate | Zero elision | overlay `payload/stream.rs` | reported as a zero-data result only | trap |

Stages 1 and 2 are independent of the schema. Stage 3 is the schema change and stands alone. Stages 4 and 5 are
worth little after 3 and can be dropped if the sample after stage 3 shows the trigger and index time is gone.

## 6. Predicted cells after stage 3 [E]

Owner service only; time outside the owner unchanged.

| Cell | Mutation service today | After run rows (32 / 64 KiB) | Command today → predicted |
| --- | ---: | ---: | --- |
| C06 | 147.6 | about 61 / 53 | 226.8 → about 140 / 132 |
| C11 | 146.7 | about 61 / 53 | 242.4 → about 157 / 149 |
| C08 | 151.1 | about 62 / 54 | 261.8 → about 173 / 165 |
| C07 (512 × 128 KiB + 1024 × 64 KiB) | 319.5 | about 150 / 125 | 499.4 → about 330 / 305 |
| C10 (1024 × 64 KiB) | 177.0 | about 90 / 72 | 400.2 → about 313 / 295 |

The warm-up reclaim (138 ms today) falls with the row count: 222 → 43 / 30 ms on the host [X].

## 7. Task 6 — the floor

For incompressible data this design must, per 64 MiB, copy the bytes three times in user space after the
request copy (bind, record, page) and issue about 16.4 k `pwrite` calls; no row shape removes either.

| Configuration | Owner service per 64 MiB | Basis |
| --- | ---: | --- |
| Today | 147.6 ms | [R 679] |
| Run rows 64 KiB + aggregate accounting + no secondary indexes + borrowed bind, 128 KiB jobs | about 45–50 ms | [E]: Payload about 17, COMMIT about 15, other statements 8, not in a statement about 4–6 |
| The same with 1 MiB jobs | about 33–36 ms | [E]: the 448 jobs' fixed parts removed |

Against the targets:

| Cell | Target | Outside owner today | Best case, 128 KiB windows | Best case, 1 MiB windows |
| --- | ---: | ---: | ---: | ---: |
| C06 | 78.6 | 78.7 | about 125–130 (1.6×) | about 94–97 (1.2×) |
| C11 | 85.8 | 94.9 | about 140–145 (1.65×) | about 110–113 (1.3×) |

**C06 and C11 cannot be reached inside the rules for incompressible data.** At 128 KiB windows the time
outside the owner alone equals C06's target and exceeds C11's. With 1 MiB windows, run rows and every other
stage, both remain about 20–30 % above target, and that already assumes the lead accepts the larger job bound.
What could still move: the roughly 51 ms outside the owner that no receipt splits (section 3); if the `Arc`
allocation turns out to fault on every request, removing that is a product fix worth measuring. Zero elision
reaches C06 only for zero-filled data and does not help C11 at all.

C07, C08 and C10 are also not reached by stage 3 alone; C10 needs COPY_FILE_RANGE and its 55 ms of owner wait
on the read side removed, which is another subject.

## 8. Not verified

- Host-to-Linux scaling of the scratch ratios. Receipt unit costs are Linux; ratios are macOS with SQLite
  3.51.2, the product bundles 3.53.2 on Linux (`core/Cargo.lock:692-693`).
- The kernel statements (copies, 16-page splice pipe, 256-page ceiling, short `copy_file_range`, page-cache
  invalidation) are recollection, not read locally.
- Whether the `Arc<[u8]>` allocation faults per request; the fixed command start cost.
- The inode-row invariant for `payload_generation` on the orphan teardown and `retire_serial` paths.
- That `SQLITE_DBCONFIG_DEFENSIVE` (`database/profile.rs:56`) permits incremental blob writes on the Linux
  build; scratch did not set it.
- The fan-out and statement times of a local READ; no receipt contains one.
- Every design in section 4 is unimplemented; counters are predictions from scratch programs, not product runs.
- Architecture notes 19, 28 and 77 were not re-read for wording that pins one cell per row or the absence of
  `COPY_FILE_RANGE`.
