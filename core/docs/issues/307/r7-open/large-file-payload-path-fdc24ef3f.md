# Large-file payload path at fdc24ef3f: cost model, reclamation, design options

> **Status:** Research; informative and not a product contract.

Read-only analysis by a research subagent at commit `fdc24ef3f`, 2026-10-09. Scratch measurements were made on the macOS host with a system SQLite driven from Python (scripts in `large-file-scratch/`), so they are relative only; projections are labelled.

No repository file was touched. Scratch scripts are in `/private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/5590953f-a1d7-48dd-91d2-01dfcf7e82bf/scratchpad/largefile/` (`exp.py`, `exp2.py`, `exp3.py`, `gen.py`, plus `git show fdc24ef3f:` copies of `schema.sql` and `accounting.sql`).

Labels: **[src]** read from source at fdc24ef3f, paths relative to `core/crates/`. **[rcpt]** decoded from the retained C06 receipt `core/docs/issues/307/checks/r7-optimization-20261009/547-C06-B-L-sample-fdc24ef3f/sample/` (`daemon.logs.stderr` call 4 and `runtime.events.jsonl`). **[scratch]** measured on the macOS host with SQLite 3.51.x driven from Python, so relative only. **[inf]** inference.

## Headline findings

- **The warm-up's 256 ms is 62% owner time.** Owner service is 159.2 ms for 513 jobs; the `CELL_PUT` upserts alone are 105.3 ms (6.42 µs per cell) and COMMIT is 29.8 ms [rcpt].
- **About 91 ms is outside owner jobs and is not split by any receipt.** It covers docker exec and process start, 512 kernel round trips, kernel copies and the `Arc` copy.
- **Under 70 ms looks reachable only for the zero-filled benchmark data.** That needs zero elision plus 1 MiB windows. For non-zero data my estimate is 95–115 ms, with an in-SQLite floor of roughly 45–55 ms owner time [inf].
- **Zero-copy bind (option d) is not available.** rusqlite 0.40.2 always binds blobs with `SQLITE_TRANSIENT` (`rusqlite-0.40.2/src/statement.rs:655-667`), and the overlay crate has `#![forbid(unsafe_code)]` (`layerfs-overlay/src/lib.rs:6`).
- **Reclaiming the 64 MiB takes an estimated 130–170 ms** in 1,171 tiny transactions, which matches the observer finding the file still changing [inf].

## Task 1 — one 128 KiB WRITE into a fresh local file

### Ordered statements of the owner job [src]

The entry is `mutate_native_visit` (`layerfs-overlay/src/lifetime/native_visit.rs:116-151`), called from `NativeMutationVisit::perform` (`layerfs-workspace/src/operations/native_visit.rs:260-316`).

| # | Statement | Where |
|---|---|---|
| 1 | `SELECT … FROM workspace` (state) | `lifetime/workspace.rs:36` via `native.rs:75` |
| 2 | `SELECT owner,root,revoked FROM native_mount` | `lifetime/native.rs:54` |
| 3 | `native_file JOIN file_handle` | `lifetime/native_file.rs:55` |
| 4–5 | workspace state, then `file_handle` (`check_file`) | `lifetime/file_owners.rs:142,146` via workspace `mutation/job.rs:98` |
| 6 | workspace state (`source_rows`) | `lifetime/source.rs:75` via `namespace/compound.rs:28` |
| 7–8 | inode `gen=-1` probe, then `INODE_LOOKUP` | `lifetime/orphan.rs:41`, `namespace/inode.rs:83-90` |
| 9 | workspace state (`apply_checked`) | `namespace/compound.rs:140` |
| 10–11 | workspace state, then `file_handle` (second `check_file`) | `namespace/compound.rs:153` |
| 12 | `LAYER_ACTIVE` | `namespace/inode.rs:122-130` |
| 13 | `INODE_PUT`, the first write, which triggers admission | `namespace/inode.rs:161` |
| 13a | `PRAGMA main.freelist_count` | `database/connection.rs:291-297` |
| 13b | `state()` (fstat + lstat), `fallocate(KEEP_SIZE)`, `state()` again | `database/allocation.rs:68-69,183,166` |
| 13c | `BEGIN IMMEDIATE` | `database/connection.rs:279` |
| 14–45 | 32 × `CELL_PUT` | `payload/stream.rs:62` → `payload/layers.rs:121`, `database/statements.rs:10` |
| 46 | `FRONTIER_ADVANCE` | `namespace/inode.rs:254` |
| 47 | `COMMIT` | `database/connection.rs:228` |

- **That is 49 statements per write, and the receipt agrees.** It shows 25,197 attempts over 513 jobs. Per job: Workspace 6.02 (five of them the same state row), Inode 4.03, Lease 4.05, Payload 32 [rcpt].
- **`CELL_PUT` is a 329-opcode program.** It opens 8 write cursors (table root, `sqlite_autoindex_payload_1`, `payload_namespace_row`, `payload_generation`, and accounting four times), with 3 `IdxInsert`, 9 `MakeRecord` and 2 trigger programs [scratch `bytecode()` on the real schema].
- **Each fresh cell executes 165 VM steps and changes 3 rows** (one direct, two from the trigger) [rcpt: 2,703,360 steps and 49,152 rows over 16,384 cells].
- **The accounting trigger is `payload_account_insert`** (`sql/accounting.sql:46-49`): two `UPDATE accounting`, for `ns=0` and `ns=NEW.ns`.
- **Admission calls `fallocate` on every growing transaction.** Required is 256 MiB (`CLEANUP_HEADROOM + MUTATION_GROWTH`, `allocation.rs:11-12,126`) and the top-up fires whenever the tail is below it (`allocation.rs:136`). The receipt confirms 512 attempts requesting 512 × 256 MiB, and 1,033 `state()` calls of two syscalls each [rcpt]. `tests/transactions.rs:290` pins this rule.

### B-trees per cell

| B-tree | Purpose | Readers |
|---|---|---|
| `payload` rowid table | row plus overflow | all |
| `sqlite_autoindex_payload_1` (ns,serial,gen,cell_offset) | upsert conflict, point and range lookups | `CELL_LOOKUP`, `CELL_PUT`, `CELL_DROP`, `garbage.rs:129,138`, `composition.rs:203`, `orphan.rs:42,91,157`, `source_wait.rs:101` |
| `payload_namespace_row` (ns,rowid) | terminal reclaim cursor | only `maintenance/reclaim.rs:9` (`INDEXED BY`); rowid is otherwise used only at `reclaim.rs:175` |
| `payload_generation` (ns,gen,serial,cell_offset) | generation retirement | only `maintenance/garbage.rs:15` (`INDEXED BY`) |

On an unanalysed scratch copy the planner also picks `payload_generation` for `CELL_RANGE` and `CAPTURED_CELL_METADATA`, although the autoindex serves them equally [scratch].

### Pages per cell [scratch, real schema, 16,384 cells in 512 transactions]

Arithmetic for a rowid leaf at page size 4096: maxLocal 4061, minLocal 489, overflow capacity 4092. The record is about 4,110 bytes, so 489 bytes stay local and 3,621 go to one overflow page, wasting 471 bytes of it. Eight cells fit per leaf.

```
$ python3 -I exp.py v0 v0z v1 v2
## V0 real schema v21, 4096-byte cells, random data: file 76591104 bytes = 73.04 MiB, pages 18699, overhead 14.13% over 64 MiB
   payload                     pages= 18438 leaf= 2048 interior=  6 overflow= 16384 unused= 7907415 depth=3
   sqlite_autoindex_payload_1  pages=    73   payload_generation pages= 73   payload_namespace_row pages= 51   (depth 2)
   file growth per transaction (pages): [36, 37, 36, 36, 36, 38, ...] mean=36.39
## V0z zero data: identical pages
## V1 minus the two secondary indexes: pages 18575 (13.37%)
## V2 WITHOUT ROWID: pages 18789 (14.68%), interior=293, depth=5
```

That is 1.141 pages per cell: 1.125 in the table and 0.012 in indexes.

Pager writes per COMMIT, from the CLI's cumulative "Page cache writes" (128 minus 64 transactions, divided by 64):

```
$ python3 -I gen.py <txns> <cell> <rows_per_txn> w.sql; sqlite3 w.sqlite < w.sql | grep "Page cache"
txns=64/128 cell=4096 rows=32   : 3355 / 6492  -> 49.0 pages per commit
txns=64/128 cell=65536 rows=2   : 2893 / 5469  -> 40.3
txns=64/128 cell=131072 rows=1  : 2887 / 5456  -> 40.1
txns=8/16   cell=65536 rows=16  : 2445 / 4573  -> 266 per 1 MiB transaction (spills 0 at cache_size=-2048)
```

The unix VFS issues one `pwrite` per page, so a commit is 49 `pwrite`s of 4 KiB. The receipt's 57.6 µs per COMMIT gives about 1.18 µs per page.

### Copies of each payload byte

1. **Kernel:** user buffer to page cache [inf].
2. **Kernel:** page cache to the daemon's `/dev/fuse` buffer [inf].
3. **`data.into()` into `Arc<[u8]>`** — `layerfs-fuse/src/operations/write.rs:23`, `layerfs-workspace/src/operations/types.rs:16-19`. The receive-loop thread runs the job inline (`dispatch/admission.rs:248-287`, daemon `overlay/owner.rs:356-366`), so there is no thread hop.
4. **`source.to_vec()`** — `payload/stream.rs:50`, one 4 KiB malloc per cell.
5. **`sqlite3_bind_blob64(…, SQLITE_TRANSIENT)`** — SQLite's private copy.
6. **`OP_MakeRecord`** — record assembly.
7. **`fillInPayload`** — 489 bytes into the leaf and 3,621 into a zeroed new overflow page.
8. **`pwrite` at COMMIT** into the file's page cache.

That is five user-space copies plus the `pwrite`. The in-memory journal also copies about 12 pre-existing hot pages per transaction.

### Budget

| | Per 128 KiB WRITE | Per 64 MiB |
|---|---|---|
| Statements | 49 | 25,197 [rcpt] |
| VM steps | about 5,750 | 2.95 M [rcpt] |
| Row changes | 32 direct, 64 trigger, 2 other | 49,152 payload [rcpt] |
| Pages written | 49 (196 KiB) | about 25,100, i.e. 98 MiB for 64 MiB of data [scratch] |
| File growth | 36.4 pages | 73.04 MiB [scratch] |
| Syscalls | 49 pwrite, 1 fallocate, 2 fstat, 2 lstat | about 27,700 |
| User-space memcpy | 5 × 128 KiB | 320 MiB |
| Owner time [rcpt] | 311 µs | 159.2 ms |
| — Payload upserts | 205.6 µs | 105.3 ms |
| — COMMIT | 58.3 µs | 29.8 ms |
| — Other statements | 26 µs | 13.4 ms |
| — Not in any statement (fallocate, stat, `to_vec`, turn bookkeeping) | about 21 µs | about 10.7 ms |

The command span is 256.5 ms and the exec stream span 249.9 ms [rcpt], so 250 minus 159 leaves the 91 ms from the headline. A no-op command in the same harness and the per-opcode counters would split it.

## Task 2 — local READ and `cp big big2`

**128 KiB READ of local bytes [src].** `read_layers` (`payload/stream.rs:113-135`) runs `LAYERS`, then one `CELL_RANGE` per layer returning 32 rows (`stream.rs:190-203`). `stale` needs no SQL when the epoch matches (`layers.rs:76`).

- **Per-row copies:** SQLite assembles the row from leaf and overflow; then `row.get::<Vec<u8>>` (`payload/cells.rs:20`); then a per-byte loop with a validity test and `decided[]` bookkeeping (`stream.rs:210-218`), not a `memcpy`.
- **Per-window copies:** `vec![0; n]` twice (`stream.rs:157,166`); `value.clone()` of the `LocalRead` (daemon `overlay/file_port.rs:21`); `local.get().clone()` and `read_file_window` into a fresh `Vec` (`layerfs-fuse/src/operations/read.rs:83-86`); then the reply into the kernel.
- **I/O:** 32 overflow and 4 leaf pages per window. The 2 MiB pager cache (`database/profile.rs:19-23`) holds 512 pages, so a 64 MiB sequential read is about 18.4k `pread`s [inf].
- **Top suspect:** the byte loop, at an estimated 1–2 ns per byte, or 65–130 ms per 64 MiB [inf, unmeasured]. A `copy_from_slice` path when `validity` is NULL and the range is undecided would remove it.

**`cp` [src + inf].** We do not implement `COPY_FILE_RANGE`: the callback replies ENOSYS (`layerfs-fuse/src/request/callbacks.rs:588-602`, `absent()` at `:35-39`), as does `fallocate`. The rest is inferred from kernel and coreutils behaviour, not measured:

- `FICLONE` fails in the VFS without a request, because FUSE has no `remap_file_range`.
- `copy_file_range(2)` sends one `COPY_FILE_RANGE` per connection. After ENOSYS the kernel remembers and falls back to an in-kernel splice copy. Each fresh mount pays that one round trip.
- The fallback issues 128 KiB READs through readahead. WRITEs are bounded by the 16-page splice pipe, so about 1,024 WRITEs of 64 KiB rather than 512 of 128 KiB.

So `cp` roughly doubles the fixed per-job cost on the write side. The opcode counters (`request/accounting.rs`) would confirm.

## Task 3 — reclamation after unmount

**Path [src].** Unmount runs `Command::Close` (daemon `control/unmount.rs:98`), which calls `Overlay::close` (`lifetime/close.rs:76-87`). That sets `lifecycle=1` and calls `queue_closed` (`close.rs:128-145`), which inserts a `reclaim` row at `CLOSE_KEY` once no lease, mount, directory or pending reply ticket holds the namespace. Fourteen other release paths call `queue_closed` too.

**One step is one cleanup transaction** (`maintenance/reclaim.rs:41-117`):

- It runs `READY`, then one phase page. Phases 0–11 are payload, names, inodes, operation records, old reclaim rows, shrink steps, maintenance, orphan, file custody, owned records, waits and indexed records. The final step deletes the `reclaim` and `workspace` rows.
- An empty page costs one extra step to advance the cursor (`reclaim.rs:102-109`).
- **Payload phase:** a `SELECT … INDEXED BY payload_namespace_row … LIMIT 14` (`reclaim.rs:9`), then 14 × `DELETE FROM payload WHERE rowid=?1 AND ns=?2` (`reclaim.rs:167-182`). The 14 comes from a 64 KiB byte window, although deleting touches no blob bytes.
- **Other tables** use `LIMIT 64`.
- **Each payload delete** removes 3 index entries, frees one overflow page and fires `payload_account_delete` (`accounting.sql:50-53`). The trigger reads `length(OLD.data)`, which makes SQLite load the old blob, one overflow page read per row [inf from SQLite's delete codegen].
- Each step also pays the freelist pragma, fstat and lstat, BEGIN and COMMIT.

**Scheduling [src].** The owner thread takes a maintenance turn when 8 jobs have been served or when no job is queued (daemon `overlay/queue.rs:346`, `:190-191`). When idle it loops without waiting while steps report work (`overlay/owner.rs:553-592`). `maintenance_turn` alternates live and closed work, one step per turn (`owner.rs:595-615`). A submitter that finds the connection busy queues behind the step (`owner.rs:355-376`), so foreground stall is one step.

**Freed pages** go to the freelist. `auto_vacuum=NONE` (`profile.rs:58`) and nothing vacuums, so the file never shrinks. Later admissions credit the freelist (`allocation.rs:129`), so `fallocate` stops while pages are reused.

```
$ python3 -I exp2.py reclaim       (copies of the V0 / run-row databases, real triggers)
current statements, limit 14      : 220.6 ms, transactions=1172, rows=16384, file 76591104 -> 76591104, freelist 0 -> 18631 of 18699
row window 64                     : 178.7 ms, 257 transactions
row window 512                    : 166.0 ms, 33 transactions
one statement per step, 512 rows  :  82.6 ms, 33 transactions
64 KiB run rows, limit 14         :  32.7 ms, 75 transactions, rows=1024
128 KiB run rows, limit 14        :  26.8 ms, 38 transactions, rows=512
```

**Estimates [inf, using Linux unit costs from the receipt]:**

- **16,384 cells:** 1,171 transactions × (14 deletes at 4–6 µs + about 60 µs fixed) ≈ 130–170 ms. No receipt measures it, because the run was killed first.
- **1,000 two-byte files:** about 72 payload steps, 17 name steps, 17 inode steps and 12 others, roughly 3,000 rows and 12–15 ms.

That explains the observation: for 100–170 ms after unmount the file takes `pwrite`s every 100 µs or so.

**Proposals:**

1. **Window by rows, one statement per step** (`DELETE … WHERE rowid IN (SELECT … LIMIT n)`). This removes about 1,100 transactions and most per-statement overhead, 2.7× in scratch. With n = 256–512 a step is 1.5–3 ms.
2. **Idle bursts.** Let a turn continue while the queue is empty, up to a fixed budget. The bound that must hold: one SQLite writer, and a foreground job waits at most one step. I would use 64 rows when jobs are queued and up to 512 when idle; today the bound is about 0.13 ms.
3. **Fewer rows (run rows, Task 4b).** 512–1,024 rows per 64 MiB gave 27–33 ms in scratch, including the chain walk SQLite needs to free multi-page overflow.
4. **Stop the trigger loading `OLD.data`.** Have terminal reclaim account from the `SELECT`'s lengths in one aggregate update per step.
5. **Not viable:** true O(1). SQLite frees page by page; per-namespace tables need runtime DDL, which re-prepares the whole statement cache, and `DROP` is one unbounded step; side files are forbidden.
6. **Harness side:** the runtime protocol already lists `cleanup KEY` (`runtime.events.jsonl` sequence 6), and `observe_cleanup` returns `Gone` when reclaim is complete (`close.rs:23-62`). Waiting for it before the residency attestation is a readiness wait, not a retry.

## Task 4 — options

Engine-only comparison on an in-memory database, so no VFS writes:

```
$ python3 -I exp3.py                                             ms per 64 MiB, 4 runs
4 KiB cells, bound blob, 3 b-trees + table (current shape)       149.3 150.0 168.1 146.7
4 KiB cells, bound blob, UNIQUE index only                       116.7 116.4 116.4 120.1
16 KiB rows, bound blob                                           64.4  65.0  67.5  65.2
64 KiB rows, bound blob                                           40.6  41.6  41.0  39.7
64 KiB rows, zeroblob + blob write                                39.0  37.5  37.7  39.4
128 KiB rows, bound blob                                          35.2  36.0  36.9  36.9
128 KiB rows, zeroblob + blob write                               36.4  34.9  36.1  35.4
64 KiB rows, bound blob, 1 MiB transactions                       37.7  32.8  38.7  39.4
```

Storage and overwrites:

```
$ python3 -I exp2.py rows overwrite
X16  16 KiB rows : 17013 pages, 3.84% overhead      X64 64 KiB rows : 16593 pages, 1.28%, depth 2
X128 128 KiB rows: 16522 pages, 0.84%               X1M (16 x 64 KiB per transaction): growth 258 pages per MiB
overwrite V0 4 KiB cells, upsert                 : 22.7 us each, file growth 0
overwrite X64, read + merge + upsert whole row   : 81.7 us each, growth 0, freelist 0
overwrite X64, incremental blob write            : 23.3 us each, growth 0
overwrite X128, incremental blob write           : 32.0 us each, growth 0
```

A same-size `UPDATE` rewrote in place: no freelist churn appeared.

All times below are [inf] projections from the receipt's unit costs. The baseline is 250 ms wall and 73.04 MiB. "Est. lines" are rough production-line estimates.

| Option | 64 MiB sequential | 1,000 small files | 4 KiB random overwrite | What breaks | Est. lines / risk |
|---|---|---|---|---|---|
| (a) 1 MiB WRITE window | Saves 448 jobs of fixed cost (about 27 ms) and 448 round trips (unsplit; if 60–100 µs each, 27–45 ms): 180–195 ms. Storage unchanged. | none | none | See note (a) | about 30 / low–medium |
| (b) page-size change | 8192: one cell per leaf, +100%. 16384: three per leaf, +33%. 65536: fifteen per leaf, +6.7%, depth 2. | 65536 makes every dirty page a 64 KiB write and the empty database at least 62 roots × 64 KiB ≈ 4 MiB | — | storage rule | reject |
| (b) cells of 4092·k bytes | exact packing | — | cells misalign with 4 KiB FUSE pages, so every page write straddles two cells | — | reject |
| (b′) run rows up to 64 KiB | Payload 105 → about 27 ms, COMMIT 30 → about 24 ms: about 166 ms. Storage 73.04 → 64.8 MiB. | none (rows ≤ 4 KiB unchanged) | on par if done by incremental blob write (23 vs 23 µs); whole-row rewrite is 3.6× slower | See note (b′) | 350–450 / medium–high |
| (c) drop the two secondary indexes | Payload −22%: about 225 ms, −124 pages. Nearly moot after (b′). | two fewer index inserts and entries per file | none | See note (c) | about 60 / medium |
| (c) WITHOUT ROWID | slower (330 vs 286 ms host), depth 5, +90 pages; also rules out `sqlite3_blob_open` | — | — | — | reject |
| (d) zero-copy bind | not available (headline) | — | — | — | — |
| (d) safe subset | Bind the slice instead of `to_vec`: one copy and 16k mallocs fewer, about 4–6 ms. Zeroblob plus blob write was no faster than bind. A 32-row statement saves about 16 ms; (b′) subsumes it. | negligible | none | `stream.rs:50`, `layers.rs:109-127` signature | about 15 / low |
| (e) zero elision above cutoff | Zero data: no payload rows, owner about 23 ms: about 114 ms; with (a), 50–70 ms. Storage about 0. Non-zero data unchanged. | none | zeros below the cutoff or inside an existing row are still stored | `write_cells`; tests that count cells for zero-filled writes | about 40 / low–medium |
| (f) group commit | Saves about 12 hot page rewrites per job: about 7 ms. | COMMIT about 10 → 1 µs per create | saves most of the commit | See note (f) | about 150 / high |

**Note (a).**
- Split the constants: `layerfs-fuse/src/mount/profile.rs:42-44` sets `max_write` from `READ_WINDOW`.
- `MAX_INPUT_BYTES` (`dispatch/types.rs:8`) scales with the window, so 16 handoffs become 16 MiB; `OwnerConfig.bytes` defaults to 8 MiB (daemon `overlay/owner.rs:29`).
- Window checks live at `callbacks.rs:523`, `operations/write.rs:18`, workspace `operations/file/write.rs:18` and `namespace/compound.rs:117`.
- The pager needs at least 2 MiB: a 1 MiB transaction dirties about 266 pages, and the SDK examples set `pager_kib` to 1024.
- Affected tests: fuse `tests/dispatch.rs:129,142`, and `tests/payload_fragmentation.rs`.
- A 1 MiB job is about 2 ms today and about 0.6 ms after (b′), so do (b′) first.

**Note (b′).** A row covers a whole-cell run that never crosses a 64 KiB boundary; partial and sparse cells keep today's ≤ 4 KiB rows with a mask.
- **Schema:** `sql/schema.sql:54` CHECK, and the version check at `database/startup.rs:136`.
- **Write and read:** `write_cells` (`stream.rs:18`); the range lower bound (`stream.rs:197`).
- **Shrink:** `shrink` must truncate the row at the boundary (`layers.rs:143`).
- **Captured reader:** `captured_runs.rs:81-100,127` and `captured_types.rs:42,106` assume one cell per row and window; workspace `construction/captured/scan.rs:145-146` rejects windows above `CELL_BYTES`.
- **Composition and GC:** `compose_cell` (`lifetime/composition.rs:249`), `move_orphan_cell` (`maintenance/source_wait.rs:73`), stale cleaning (`maintenance/garbage.rs:127`).
- **Daemon charges:** `overlay/commands.rs:366-406`.
- **Tests:** overlay `payload.rs` (`holes_cost_no_rows…`), `payload_fragmentation.rs` (`dense_fragmentation_does_not_enlarge_a_later_request`), `captured_runs.rs` (all), `costs.rs:21`; workspace `payload.rs`; daemon `captured_runs.rs`.

**Note (c).** The reclaim cursor (`reclaim.rs:9,175`) moves to the autoindex's `ns` prefix. `retire_generation` phase 0 (`garbage.rs:13-34`) must be driven through `inode_capture`. That needs the invariant "a payload layer implies an inode row at the same generation", which I did not verify for the orphan domain.

**Note (f).** Admission would have to charge uncommitted growth, because file length no longer shows it (`connection.rs:286-311`), and force a commit before idle, maintenance, capture, close and resource reports. The contract "one job = one transaction" changes (`connection.rs:206-267`), and four tests in `tests/transactions.rs` pin it.

**Why (e) is valid for both readers [src].** In `compose_layers`, bytes at or above the layer's cutoff with no row are settled as zero (`stream.rs:220-222`). In the captured reader, `gap_bounds` returns a `Zero` gap for `at >= layer.cutoff` (`captured_runs.rs:165-166,141-146`), and workspace treats it as changed (`scan.rs:179-184`). A new file has cutoff 0 (`inode.rs:144-145`), and a shrink lowers it (`layers.rs:191`).

The rule is: a fully covered cell at or above `layer.cutoff` whose bytes are all zero is stored as absence, with one range `CELL_DROP` for the run to remove any existing row. Below the cutoff zeros must still be stored, because absence there means inherited.

### Recommended target and order

**Target:** cell rows on the 4 KiB grid that may hold a whole-cell run up to 64 KiB, absence for zero cells above the cutoff, partial overwrite of a run row by incremental blob write (no trigger fires, same size), and a 1 MiB WRITE window. 64 KiB matches the `cp` write size and keeps overflow chains at 16 pages.

Projection [inf]: non-zero 64 MiB at about 50 ms owner time and 95–115 ms wall, 64.8 MiB stored; the zero benchmark at 50–70 ms and about 0 MiB; reclaim of 64 MiB under about 35 ms.

Steps, each independently testable, one sample each:

0. **No code.** Run a no-op command in the C06 harness and read the opcode counters to split the 91 ms, and add the `cleanup KEY` wait before attestation.
1. **Bind the borrowed slice** instead of `to_vec`. Small, and proves the copy accounting.
2. **Zero elision (e).** Test with a reference model over shrink and regrow, base-covered zeros and a captured run.
3. **Reclaim by row window with one statement per step, plus idle bursts.** Verify the foreground stall bound with a queued job.
4. **Run rows (b′), schema v22.** The writer and local reader first; then the captured reader and scan; then composition, orphan and garbage paths. This is the only step with real correctness risk.
5. **WRITE window to 1 MiB (a),** with the separated constants, owner byte credit and pager size.
6. **Optional cleanups:** read the workspace state once per job instead of five times; drop the secondary indexes (c); group commit (f) only if small-file COMMIT still matters.

### Decisions for you

- **Zero elision is what gets the benchmark under 70 ms.** `dd if=/dev/zero` then becomes a sparse-file test. The rule is general and exact, but the receipt should say that incompressible data would land near 100 ms.
- **Fallocate hysteresis** would save most of the 512 `fallocate` calls (part of the 10.7 ms non-statement time). It lets the reserved tail dip up to about 16 MiB below the 256 MiB bound before topping up. Allocated bytes never exceed today's, but the per-job growth guarantee weakens from 128 to about 112 MiB, which changes the admission contract; I left it out of the ordered steps for that reason.
- **Run rows and sparse storage.** The design keeps 4 KiB rows for partial and sparse writes so no cell stores more than today. Fixed 64 KiB extents would be simpler but can enlarge a sparse 4 KiB write up to 16×, which the storage rule forbids.