# R7 research: owner service time of the create path (SQL side)

> **Status:** Research; informative and not a product contract. Read-only analysis by a research subagent at commit `63c48d8dc`, 2026-10-09.

Cells C01, C02, C03, C04, C12. Product source read only at `63c48d8dc` (`git show`). Read-only research; nothing in the repository was changed.

Labels: `[S]` read from source at 63c48d8dc with file:line, `[R]` decoded from a named receipt, `[I]` inference, `[E]` estimate.
Paths under `core/crates/layerfs-overlay/src/` are written `overlay:<file>`, under `core/crates/layerfs-workspace/src/` `workspace:<file>`, under `core/crates/layerfs-daemon/src/` `daemon:<file>`.

Receipts: `core/docs/issues/307/checks/r7-optimization-20261009/<n>-<cell>-B-L-sample-63c48d8dc/sample/receipt.json`, decoded with `core/target/r7-summary.py` and `core/target/r7-statements.py`. Receipt numbers: 659 = C01, 663 = C02, 667 = C03, 671 = C04, 703 = C12.

Scratch: `/private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/f38d048c-3b28-460f-a66e-706b8d433ec1/scratchpad/research/create-sql/` (SQLite 3.51.0 through ctypes on the macOS system library, real `sql/schema.sql` + `sql/accounting.sql`, the daemon profile PRAGMAs). Scratch numbers are relative. VM steps, executions, cursors and pages written are properties of the statements and schema; scratch wall time is not used for prices (see section 2.1).

---

## 0. Result in one page

1. The five jobs of one created file are exactly **48 statement attempts, 61 executions, 3 transactions, 13 sub-programs** (12 accounting trigger firings + 1 FK cascade). The scratch replica reproduces receipt 659 per family with zero residual per file. `[S][R]`
2. Receipt 659: service 162.9 µs per file, of which statements 136.3 µs. By family per file: Lease 35.9, Inode 32.4, **Commit 31.2**, Workspace 17.8, DirectoryEntry 9.6, Payload 6.4, Startup 1.8, Begin 1.2. `[R]`
3. Where the 162.9 µs goes, by cause `[I]`, from a model that reproduces the receipt to 1 %:
   - COMMIT (page write-back, 35 pages per file at about 0.89 µs each): 31 µs
   - accounting triggers (12 firings x about 2.2 µs): 27 µs
   - custody rows that no reader needs or that duplicate another row (lease kinds 7 and 9, `native_file`): 29 µs including their triggers and commit pages
   - three INODE_PUT upserts (about 6.5 µs each): 20 µs
   - five fences: 14.6 µs
   - admission per transaction (PRAGMA + BEGIN + fstat + lstat), three times: about 16 µs `[E]`
   - job framing outside statements: about 14.5 µs
4. Ranked cuts per created file (model, standalone; they are not additive, see section 3.6):

| Rank | Cut | C01 file | touch | mkdir | 128 KiB WRITE |
| --- | --- | --- | --- | --- | --- |
| 1 | Group commit E05, 16 write jobs per group | -40 µs | -36 | -15 | -14 |
| 2 | Custody: D10 + D14 | -29 | -29 | -6 | 0 |
| 3 | Accounting written once per transaction from engine deltas | -16 (-27 inside a group) | -17 | -9 | -88 |
| 3' | Accounting triggers reduced to the namespace row (SQL only) | -13 | -12 | -6 | -45 |
| 4 | Inode statements: D3 + D7 + range CHECK + column-subset UPDATE | -18 | -15 | -12 | 0 |
| 5 | Fence state in engine memory | -9 `[E]` | -9 | -4 | -2 |
| 6 | Drop one capture index | -2 | -2 | -1 | 0 |

5. All of rank 1 to 4 together: **162 µs -> 66 to 68 µs per created file** (statements 48 -> 31, executions 61 -> 32, transactions 3 -> 0.19, sub-programs 13 -> 1). Without group commit: 162 -> 112 µs. `[I]`
6. Even at 68 µs of service, C01 keeps about 217 µs per file outside the owner (379.7 - 162.9), so the command would be about 285 ms against A2 183.4 ms. The SQL side alone cannot reach the target; the outside-owner cost is another agent's subject. `[R][I]`
7. New scratch finding that constrains the designs: a statement that runs a trigger opens a statement journal, so `SQLITE_FULL` aborts only that statement; a trigger-free single-row statement has none, so `SQLITE_FULL` cancels the **whole** transaction (autocommit returns to 1). Removing triggers therefore changes what a quota-full failure does to an open group. `[scratch savepoint.py]`

---

## 1. The statement trace

### 1.1 Mechanics that decide the counts

- One connection per daemon; `journal_mode=MEMORY`, `synchronous=OFF`, `locking_mode=EXCLUSIVE`, `temp_store=FILE`, `cache_size=-2048`, page 4096. `[S overlay:database/profile.rs:58-61]`
- A job starts in autocommit. Reads run without a transaction. The first statement that is not read-only triggers `begin()`: `admit()` then `BEGIN IMMEDIATE`. `[S overlay:database/connection.rs:276-331]`
- `admit()` runs `PRAGMA main.freelist_count` (family Startup) and `Allocation::state()`, which is `file.metadata()` plus `symlink_metadata(path)` with an identity comparison; it calls `fallocate(KEEP_SIZE)` only when `reserved_tail_bytes < required`. `[S overlay:database/allocation.rs:64-100, 120-190]` In receipt 659 the cumulative diagnostics show 45 allocation attempts against 17,454 freelist queries, so fallocate is rare. `[R 659 resource_diagnostics]`
- `COMMIT` at job end; a failed COMMIT quarantines the engine and reports Uncertain; an error rolls back if the connection is not already in autocommit; `allocation.state()` is read again after a failure. `[S overlay:database/connection.rs:211-272]`
- `executions` is `sqlite3_stmt_status(RUN)`. Each trigger firing and each FK cascade program adds one. `[S][scratch]`
- Every INSERT or DELETE on an accounted table fires one trigger, and that trigger runs two UPDATEs on 18-column `accounting` rows: the aggregate row `ns=0` and the namespace row `ns=NEW.ns`. `[S overlay:sql/accounting.sql]`

### 1.2 The five jobs of one created file (C01: `echo x > f` in the root)

`+T` marks a statement that fires one accounting trigger (one extra execution).

| # | Statement | Family | Writes | Source |
| --- | --- | --- | --- | --- |
| **Job 1 GETATTR(root)** | no transaction | | | |
| 1 | FENCE_LOOKUP | Workspace | - | `overlay:lifetime/native_visit.rs:40-94`, SQL `overlay:database/statements.rs:150-164` |
| 2 | INODE_LOOKUP(root) | Inode | - | `overlay:namespace/inode.rs:74-94` |
| **Job 2 LOOKUP(f), negative** | no transaction | | | |
| 3 | FENCE_LOOKUP | Workspace | - | as above |
| 4-7 | 2 x (INODE_LOOKUP(parent), NAME_LAYERS) | Inode, DirectoryEntry | - | root is a base directory, so the first round returns `Need::Name`, the name is supplied from resident objects and the job is evaluated again: `workspace:operations/native_visit.rs:224-249`, `workspace:operations/native_read.rs:202-211`, `workspace:mutation/eval.rs:65-79` |
| **Job 3 CREATE** | one transaction | | | `workspace:operations/native_visit.rs:154-191` |
| 8 | FENCE_LOOKUP | Workspace | - | |
| 9-12 | 2 x (INODE_LOOKUP, NAME_LAYERS) | Inode, DirectoryEntry | - | `workspace:mutation/create.rs:16-27` (`free_name`) |
| 13-14 | INODE_LOOKUP, NAME_LAYERS | Inode, DirectoryEntry | - | `workspace:mutation/job.rs:153-172` (inheritance loop) |
| 15-16 | LAYER_ACTIVE(file) miss, LAYER_LOWER(file) miss | Inode | - | `overlay:namespace/inode.rs:124-146` |
| 17 | `PRAGMA main.freelist_count`; then fstat + lstat | Startup | - | `overlay:database/connection.rs:291-331`, `overlay:database/allocation.rs:64-100` |
| 18 | `BEGIN IMMEDIATE` | Begin | - | `overlay:database/connection.rs:276-290` |
| 19 | INODE_PUT(file), insert `+T` | Inode | inode, inode_capture, accounting x2 | |
| 20 | LAYER_ACTIVE(parent) hit | Inode | - | |
| 21 | INODE_PUT(parent), upsert takes the UPDATE arm | Inode | inode (index unchanged) | |
| 22 | NAME_LAYERS | DirectoryEntry | - | `overlay:namespace/compound.rs:195-196` |
| 23 | DIRECTORY_ENTRY_PUT `+T` | DirectoryEntry | directory_entry, directory_entry_capture, accounting x2 | |
| 24 | FRONTIER_ADVANCE | Workspace | workspace | `overlay:namespace/inode.rs:286-291` |
| 25 | INSERT native_lookup `+T` | Lease | PK, UNIQUE(ns,owner), accounting x2 | `overlay:lifetime/native.rs:267-295` |
| 26 | INSERT lease kind 9 `+T` | Lease | PK, lease_resource, accounting x2 | same |
| 27 | INSERT file_handle `+T` | Lease | PK, UNIQUE(ns,request), accounting x2 | `overlay:lifetime/file_owners.rs:107-134` |
| 28 | INSERT lease kind 7 `+T` | Lease | PK, lease_resource, accounting x2 | same |
| 29 | FILE_OPEN_LOOKUP_ADD (file_custody insert) `+T` | Lease | PK, accounting x2 | |
| 30 | INSERT native_file `+T` | Lease | PK, 2 UNIQUE, accounting x2; 2 FK checks | `overlay:lifetime/native_mutation.rs:81-119` |
| 31 | `COMMIT` | Commit | 18.7 pages `[scratch]` | `overlay:database/connection.rs:233` |
| **Job 4 WRITE (2 bytes)** | one transaction | | | |
| 32 | FENCE_FILE | Workspace | - | |
| 33-34 | INODE_LOOKUP, LAYER_ACTIVE | Inode | - | `workspace:mutation/write.rs:21` |
| 35-36 | PRAGMA + stat pair; BEGIN IMMEDIATE | Startup, Begin | - | |
| 37 | INODE_PUT (UPDATE arm) | Inode | inode | |
| 38 | CELL_LOOKUP miss | Payload | - | `workspace:.../stream.rs:52` |
| 39 | CELL_PUT `+T` | Payload | payload, 3 indexes, accounting x2 | |
| 40 | FRONTIER_ADVANCE | Workspace | workspace | |
| 41 | COMMIT | Commit | 7.3 pages `[scratch]` | |
| **Job 5 RELEASE** | one transaction | | | `overlay:lifetime/native_file.rs:91-114`, `overlay:lifetime/file_owners.rs:194-229` |
| 42 | FENCE_FILE | Workspace | - | |
| 43-44 | PRAGMA + stat pair; BEGIN IMMEDIATE | Startup, Begin | - | |
| 45 | DELETE file_handle `+T`, FK cascade program deletes native_file `+T` (4 executions) | Lease | file_handle x2 trees, native_file x3 trees, accounting x4 | |
| 46 | DELETE lease kind 7 `+T` | Lease | lease x2 trees, accounting x2 | |
| 47 | FILE_OPENS_DROP (`UPDATE ... RETURNING`, returns 1) | Lease | file_custody | `queue_closed_at` issues no statement while lookups remain |
| 48 | COMMIT | Commit | 9.0 pages `[scratch]` | |

Per job and family (attempts / executions):

| Job | Workspace | Inode | DirEntry | Payload | Lease | Startup | Begin | Commit | Total |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| GETATTR | 1 | 1 | | | | | | | 2 / 2 |
| LOOKUP | 1 | 2 | 2 | | | | | | 5 / 5 |
| CREATE | 2 | 8 / 9 | 5 / 6 | | 6 / 12 | 1 | 1 | 1 | 24 / 32 |
| WRITE | 2 | 3 | | 2 / 3 | | 1 | 1 | 1 | 10 / 11 |
| RELEASE | 1 | | | | 3 / 7 | 1 | 1 | 1 | 7 / 11 |
| **file** | **7** | **14 / 15** | **7 / 8** | **2 / 3** | **9 / 19** | **3** | **3** | **3** | **48 / 61** |

The workspace test `core/crates/layerfs-workspace/tests/native_visit_cost.rs:199` pins exactly these constants (CREATE: Startup 1, Begin 1, Commit 1, Workspace 2, Inode (8,9), DirectoryEntry (5,6), Lease (6,12)). `[S]`

### 1.3 Reconciliation with receipt 659 (C01, 1000 files)

| Family | Trace x 1000 (att / exec) | Receipt 659 | Residual |
| --- | --- | --- | --- |
| Workspace | 7000 | 7001 | +1 |
| Inode | 14000 / 15000 | 14002 / 15003 | +2 / +3 |
| DirectoryEntry | 7000 / 8000 | 7000 / 8000 | 0 |
| Payload | 2000 / 3000 | 2000 / 3000 | 0 |
| Lease | 9000 / 19000 | 9000 / 19000 | 0 |
| Startup | 3000 | 3003 | +3 |
| Begin, Commit | 3000 | 3000 | 0 |

Residuals `[I]`: Inode +2 attempts is the first use of the root (the first GETATTR needs two rounds; the first CREATE inserts the root's local row, which adds a LAYER_LOWER and one trigger execution). Workspace +1 and Startup +3 belong to one extra Lifecycle job (jobs are 5001, Lifecycle 1001); three Startup statements with no Begin match a resources report (`page_count`, `freelist_count`, accounting SELECT). I did not identify that job from source. `[not verified]`

Scratch replica (`trace.py 1000`): 48.00 attempts and 61.00 executions per file, every family equal to the trace. VM steps per file, scratch 3.51.0 against receipt: Workspace 358 / 332.0, Inode 874 / 854.9, DirectoryEntry 266 / 254.0, Payload 186 / 181.0, Lease 1144 / 1090.0, Startup 21 / 21.0, Begin 15 / 15.0, Commit 9 / 9.0; total 2873 / 2757 (factor 0.96; the product bundles a different SQLite build). `[R][scratch]`

### 1.4 `touch` (C04)

- `touch` is CREATE + utimens + RELEASE with no WRITE. The FUSE layer drops the file handle from SETATTR unless a size is given (`handle.filter(|_| size.is_some())`), so the utimens SETATTR is inode-addressed: FENCE_LOOKUP, `Operation::SetAttributes`. `[S core/crates/layerfs-fuse/src/request/callbacks.rs:348]` One SETATTR per touch. `[R 671: Setattr 1000]`
- Per touch in a directory created in this Workspace (one evaluation round, because the directory is `created_above(born)`): `[S][I]`

| Job | Workspace | Inode | DirEntry | Lease | Txn |
| --- | --- | --- | --- | --- | --- |
| LOOKUP negative | 1 | 1 | 1 | | 0 |
| CREATE | 2 | 7 / 8 | 4 / 5 | 6 / 12 | 1 |
| SETATTR (INODE_LOOKUP, LAYER_ACTIVE, INODE_PUT, FRONTIER) | 2 | 3 | | | 1 |
| RELEASE | 1 | | | 3 / 7 | 1 |
| GETATTR(parent) | 1 | 1 | | | 0 |
| **touch** | **7** | **12 / 13** | **5 / 6** | **9 / 19** | **3** |

  That is 42 attempts / 54 executions. In a base directory (the scratch unit) it is 46 / 58, four more point reads.

### 1.5 `mkdir` (C04)

- Negative LOOKUP (Workspace 1, Inode 1, DirEntry 1; under the root Inode 2, DirEntry 2) plus MKDIR.
- MKDIR: Workspace 2; Inode 7 / 8 (8 / 9 under the root); DirectoryEntry 4 / 5 (5 / 6 under the root); custody is INSERT native_lookup `+T`, INSERT lease kind 9 `+T`, FILE_LOOKUPS_ADD `+T`, native_parent upsert `+T` = Lease 4 / 8 (`overlay:lifetime/native_directory.rs:9-19`); one transaction. `[S]`
- Unit LOOKUP + MKDIR under the root: 27 attempts / 33 executions, 13.4 pages written. `[scratch]`

### 1.6 Reconciliation with receipt 671 (C04)

Requests `[R 671]`: Lookup 1200, Getattr 1011, Setattr 1000, Mkdir 110, Release 1000, Create 1000, Opendir 10, Releasedir 10.

| Family | Model | Receipt 671 | Residual |
| --- | --- | --- | --- |
| DirectoryEntry attempts | 5000 + 120 + 450 + 90 = 5660 | 5660 | 0 |
| DirectoryEntry executions | 6770 | 6770 | 0 |
| Inode attempts | 12000 + 120 + 780 + 11 + 180 = 13091 | 13103 | +12 |
| Workspace attempts | 7431 | 7572 | +141 |
| Lease attempts / executions | 9710 / 20150 | 10130 / 20750 | +420 / +600 |
| Transactions | 3200 | 3250 | +50 |

All residuals fall on the 10 OPENDIR + 10 RELEASEDIR pairs (the source-holding directory path, which I did not trace statement by statement) and on first-use effects. `[I, not verified]` The 90 extra LOOKUPs are positive lookups of existing directories (Workspace 1, Inode 2, DirEntry 1, Lease 3, one transaction each). `[I]`

### 1.7 The other cells

- C02 (663): Workspace 8002 attempts / 378,068 VM steps, Inode 15,003 / 16,004 / 886,950. One extra GETATTR per file costs a fence (46 steps) and an INODE_LOOKUP (32 steps), no transaction. Statement times in this receipt are inflated by cold caches between `stat` processes, so I priced C02 from steps. `[R][I]`
- C03 (667): service 300.4 ms, owner wait 59 ms, Reclaim 4001 attempts. The create half is the trace above; unlink and reclamation are another agent's subject. `[R]`
- C12 (703): service 133.7 ms, 2868 transactions. `[R]`

---

## 2. Prices

### 2.1 Method

Scratch wall time on macOS is not usable directly: statements that open an ephemeral table (the `kind IN (1,2,3)` CHECK in INODE_PUT, `RETURNING`) cost 8 to 12 µs in the macOS system library, while the Linux receipt shows no such cost (INODE_PUT is about 6.5 µs there). `[scratch][R][I]`

Prices therefore use a model fitted to receipt 659 and checked against the family totals of 659, 667, 671 and 703:

    µs = 0.25 x attempts + 0.0309 x VM steps (product steps = 0.96 x scratch steps)
       + 1.16 per fence + 0.89 x pages written at COMMIT
    job framing 2.9 µs per job; admission bookkeeping + stat pair about 4.3 µs per transaction [E]

It reproduces receipt 659: statements 134.2 µs per file (actual 136.3), service 161.6 (actual 162.9). `[I]` The 4.3 µs admission figure is a class residual (non-statement time per job: Read 2.9 µs, Mutation 7.9, Lifecycle 7.2), not a direct measurement. `[E]`

Pages written per COMMIT are `SQLITE_DBSTATUS_CACHE_WRITE` in scratch (`pages.py`): CREATE 18.7, WRITE 7.3, RELEASE 9.0, total **34.98 pages per file**; file growth 0.056 pages per file. With 31.2 µs of Commit per file that is about 0.89 µs per page. `[scratch][R][I]`

### 2.2 Per statement

Columns: VM steps (scratch 3.51.0, exact), b-tree write cursors opened, trigger or cascade programs, product price `[I]`.

| Statement | VM steps | Write cursors | Programs | µs |
| --- | --- | --- | --- | --- |
| FENCE_LOOKUP | 50 | 0 (3 read) | 0 | 2.9 |
| FENCE_FILE | 63 | 0 (4 read) | 0 | 3.0 |
| INODE_LOOKUP | 33 | 0 | 0 | 1.3 |
| LAYER_ACTIVE / LAYER_LOWER | | 0 | 0 | 0.8 / 0.9 |
| NAME_LAYERS | 22-29 | 0 | 0 | 0.9 |
| **INODE_PUT** insert | 193 (118 without trigger) | 4: inode, inode_capture, accounting x2 | 1 | 6.7 |
| **INODE_PUT** update arm | 190 | 4 opened, 1 written | 0 | 6.3 |
| DIRECTORY_ENTRY_PUT | 134 (59 without trigger) | 4: directory_entry, directory_entry_capture, accounting x2 | 1 | 4.2 |
| CELL_PUT (2 bytes) | 169 (72 without trigger) | 8: payload, payload_generation, payload_namespace_row, autoindex, accounting x4 | 2 (insert and update arms) | 5.3 |
| FRONTIER_ADVANCE | 41 | 1: workspace | 0 | 1.6 |
| INSERT native_lookup | 131 (56) | 4: PK, UNIQUE(ns,owner), accounting x2 | 1 | 4.1 |
| INSERT lease | 121 (46) | 4: PK, lease_resource, accounting x2 | 1 | 3.8 |
| INSERT file_handle | 127 (51) | 4 | 1 | 4.0 |
| FILE_OPEN_LOOKUP_ADD | 117 (41) | 3 | 1 | 3.7 |
| INSERT native_file | | 5: PK, 2 UNIQUE, accounting x2; 2 FK checks | 1 | 4.3 |
| DELETE file_handle + cascade | 228 (78) | 9 | 3: trigger, cascade, cascade's trigger | 7.0 |
| DELETE lease | | 4 | 1 | 3.4 |
| FILE_OPENS_DROP | 55 | 1 + 1 ephemeral (RETURNING) | 0 | 1.9 |
| native_parent upsert | | 3 | 1 | |
| PRAGMA freelist_count | 7 | 0 | 0 | 0.59 |
| BEGIN IMMEDIATE | 5 | 0 | 0 | 0.41 |
| **COMMIT** | 3 | - | - | 10.4 mean: CREATE 16.6, WRITE 6.5, RELEASE 8.0 |
| SAVEPOINT / RELEASE | 4 / 4 | 0 | 0 | 0.4 each `[E]` |

One accounting trigger firing is 75 VM steps (97 for payload, which updates more columns), about **2.2 µs**. A created file fires 12 and runs one cascade: about 27 µs. `[scratch][I]`

Per job, statements only `[I]`: GETATTR 4.1, LOOKUP 7.1, **CREATE 71.8**, WRITE 26.3, RELEASE 24.8 µs.

EXPLAIN notes `[scratch explain.py]`: INODE_PUT is 325 opcodes and opens two ephemeral tables for the `kind IN (1,2,3)` CHECKs; DIRECTORY_ENTRY_PUT 177; CELL_PUT 338. Every fence and lookup statement is an indexed seek (no scan step).

### 2.3 B-tree mutations per file

Persistent tree mutations per created file: CREATE 34, WRITE 8, RELEASE 16 = 58 today; about 20 after D10, D13 and D14. `[scratch explain.py, I]` This number is what the group-commit admission rule in section 3.1 counts.

---

## 3. Designs

Units in the tables: "C01 file" is the five jobs above; "touch" is LOOKUP + CREATE + SETATTR + RELEASE + GETATTR in a base directory (46 / 58); "mkdir" is LOOKUP + MKDIR under the root (27 / 33); "128 KiB WRITE" is one WRITE job with 32 CELL_PUTs (40 attempts, 72 executions, 32 trigger firings, 5769 steps, 48.6 pages). The model underestimates the absolute cost of the 128 KiB WRITE (232.7 µs against 311 µs in the second handoff) because it does not price blob copying; only the deltas are claimed. `[I]`

Baselines (model service µs): C01 file 161.6, touch 150.9, mkdir 75.6, 128 KiB WRITE 232.7.

### 3.1 (a) Group commit E05

**What it is.** Consecutive short native visit jobs share one SQLite transaction. Each joined write job runs inside `SAVEPOINT j ... RELEASE j`. The outer `COMMIT` runs when a bound is reached or when a job that must not join arrives.

**Measured effect of grouping alone** (`matrix.py`, C01 file, g = write jobs per group):

| g | commits per file | pages written per file | service µs |
| --- | --- | --- | --- |
| 1 (today) | 3 | 34.98 | 161.6 |
| 4 | 0.75 | 17.4 | 135.7 |
| 8 | 0.375 | 8.9 | 126.2 |
| 16 | 0.188 | 4.71 | 121.4 |
| 32 | 0.094 | 2.53 | 118.9 |
| 64 | 0.047 | 1.42 | 117.7 |

At g = 16 the saving is about 40 µs per file: -27 commit pages, -12 admission `[E]`, -1 net statements (9 PRAGMA/BEGIN/COMMIT become 0.56, plus 6 SAVEPOINT/RELEASE). The curve is flat past 16 because the same hot pages (root directory leaf, accounting, workspace row, custody leaves) are rewritten by every job today and once per group afterwards. touch -36, mkdir -15, 128 KiB WRITE -14 (its pages are mostly new payload pages, which grouping cannot merge).

**Admission.** Today `admit()` reserves for one job: `MUTATION_GROWTH` 128 MiB, derived as 700 tree mutations x 43 pages + 100 overflow pages = 123,699,200 B, plus `CLEANUP_HEADROOM` 128 MiB. `[S overlay:database/allocation.rs:11-12; core/docs/issues/307/S6-RESERVATION-CONTRACT.md:91-127; core/docs/architecture/35-shared-physical-capacity.md:55-69]`
The rule that keeps this contract unchanged: **a group is one reservation window**. Admission runs once, at the first writing statement of the group, on committed state, exactly as today (PRAGMA, `state()`, optional fallocate, `BEGIN IMMEDIATE`). Each job class declares a fixed allowance of tree mutations and overflow pages; a job may join only while the sum of declared allowances stays within 700 and 100. A job with no small declared bound uses 700 and therefore runs alone. No preallocation grows. A group admitted at cleanup level is committed before a mutation joins. `[I, design]`
At 58 tree mutations per file today, 700 admits 12 files (36 write jobs); a 128 KiB WRITE consumes 32 or more overflow pages, so at most two or three join, which matches its small gain. `[I]`

**Memory bound.** The journal is in memory. It holds one original image per distinct page first modified in the transaction; the savepoint sub-journal is also in memory under `journal_mode=MEMORY`. `[I, from SQLite design; not measured: heap statistics are disabled in the system library]` Because a group never exceeds one job's declared reservation, the worst-case journal bound is the same expression as today's one-job bound. To keep it small in practice, also cap the group at 16 to 32 write jobs: distinct dirty pages are then 25 to 30 for C01 (about 120 KiB) `[E]`, and at most the 512-page cache (2 MiB) before SQLite spills. One group per daemon, since there is one connection per daemon.

**Failure of the outer COMMIT, or SQLite cancelling the whole transaction.** Checked in scratch (`savepoint.py`):

- A constraint failure inside a job leaves the transaction open; `ROLLBACK TO j; RELEASE j` removes that job alone, and accounting rows follow. Earlier jobs stay.
- `SQLITE_FULL` in a statement that runs a trigger (statement journal in use): the transaction stays open, the job rolls back alone, earlier jobs stay.
- `SQLITE_FULL` in a trigger-free single-row statement: SQLite rolls back the **whole** transaction (autocommit becomes 1); earlier jobs of the group are gone although their replies were sent.

So a group can be lost in three ways: outer COMMIT failure, an I/O error, or `SQLITE_FULL` in a statement without a statement journal. The engine must detect it (`is_autocommit()` after an error, which `transaction()` already tests at `overlay:database/connection.rs:249-257`) and treat it as today's failed COMMIT: quarantine and Uncertain. `[S][I]` Replies already sent for earlier jobs of that group then describe state that no longer exists. That is the same outcome as losing the daemon after replying, which the Disposable profile already allows (the overlay file is created with `create_new(true)` and never reopened; `overlay:database/profile.rs:24` says no crash durability is claimed; `overlay:database/startup.rs:62`). `[S][I]`
`SQLITE_FULL` from the filesystem is what the reservation prevents. `SQLITE_FULL` from an explicit page quota is not: with a quota set, either do not group, or accept quarantine on quota-full. That is an owner decision. `[I]`

**Which observable guarantee changes.** Exactly one: a reply is sent after the job's savepoint is released and before the outer COMMIT. Only this connection reads the database (one connection, EXCLUSIVE), so every later job sees the released state. What can observe the difference: (1) something outside the daemon reading or stat-ing `overlay.sqlite`; (2) crash timing; (3) a commit failure is attributed to a later request, not to the one that wrote; (4) diagnostics and tests that count Begin/Commit/Startup per job, `admitted_jobs`, `freelist_queries`. Scratch confirms the file length does not change while a group is open. Published mutations, reply tickets (`overlay:lifetime/tickets.rs`), permission checks and coherence are unchanged. `[S][I]`

**Who joins.** Opt-in on the Overlay: a job runs standalone unless the caller joins it to the open group, so every existing overlay test keeps "one job, one transaction". The daemon joins only the bounded native visit jobs (ObserveVisit, MutateVisit, CloseFile, Forget). Everything else commits the open group first: any non-joined `atomic`, `resources()` / `pages()` / `allocation()` (they read committed physical state, `overlay:database/accounting.rs:44-45`), maintenance turns, Capture, install, Close and unmount. `[I, design]`

**Idle detection without a timer thread or polling.** There is no event that distinguishes idle from the gap between two requests; only a clock could, and a timer thread or poll is forbidden. Findings from the owner loop: uncontended jobs run on the submitting FUSE thread (`daemon:overlay/owner.rs:251-379, 476-499`); the owner thread wakes when a job is queued or a maintenance hint is set (`daemon:overlay/queue.rs:333-400`). Waking the owner thread at every turn end to commit would commit in every gap and bring back the hand-off collisions that the inline path removed. `[S][I]`
Recommendation: a lazy group. It is committed by (1) the job and allowance bounds, (2) the next job that does not join, (3) the owner thread's existing idle path before it blocks in `shared.wait` (`daemon:overlay/owner.rs:548-593`), with the wake-up coming from the existing maintenance hint rather than a new one, (4) Close and unmount. Consequence to accept: after the last request of a burst, up to one group stays uncommitted until the next event. Nothing inside the daemon can observe that; an external reader of the overlay file can. `[I]` Whether the existing maintenance hint fires often enough to bound that interval is **not verified**.

**Tests that pin "one job, one transaction".** `[S]`

- overlay `tests/transactions.rs`: `a_read_only_atomic_job_begins_commits_and_admits_nothing` (93), `reads_then_writes_are_one_transaction_begun_at_the_first_write` (131), `a_failure_after_the_first_write_rolls_back_and_one_before_it_has_nothing_to_roll_back` (173), `consecutive_writes_that_do_not_grow_the_file_allocate_at_most_once` (291). These stay valid for standalone jobs; the group needs new tests beside them.
- overlay `tests/costs.rs`: `failed_atomic_job_retains_attempted_cost_and_rolls_back_logical_counts` (71).
- workspace `tests/native_visit_cost.rs:199` (Startup 1, Begin 1, Commit 1 per write job).
- daemon `tests/job_cost.rs:9` `completion_cost_matches_exclusive_owner_aggregate_and_retained_credit`.

### 3.2 (b) Accounting without two UPDATEs per row

**Readers of `accounting`.** One: `Overlay::resources` (`overlay:database/accounting.rs:44-45`), reached only from `daemon:overlay/commands.rs:480` and `:515`. No product job reads accounting to decide anything. `[S]` Tests cannot read it in the middle of a transaction, because every access is a job; but `costs.rs:71` requires the counts to roll back with a failed job. `[S]`

**Options measured** (`probes.py`, `matrix2.py`):

| Option | Mechanism | C01 file | touch | mkdir | 128 KiB WRITE |
| --- | --- | --- | --- | --- | --- |
| D13(a) narrow per-counter rows | triggers stay, smaller rows | -4 (steps 2873 -> 2727) | | | worse (5769 -> 6729 steps) |
| T2 one UPDATE `WHERE ns IN (0, NEW.ns)` | | worse (CELL_PUT 169 -> 203 steps) | | | worse |
| **T3 namespace row only** | trigger updates the namespace row; the aggregate is `SUM` over the namespace rows at read | -13.2 (steps 2430) | -11.7 | -6.4 | -45 (steps 4265) |
| **Engine deltas** | no triggers; each writer site adds to a fixed delta record; one `UPDATE accounting SET c = c + ? ... WHERE ns = ?` per touched row before COMMIT (58 steps per row) | -15.6 standalone, -27 inside a group | -16.6 | -9.4 | -88 (steps 2781) |

D13(a) and T2 are rejected by measurement.

**T3** is a SQL-only change: same tables, same rows, half the trigger work, rollback semantics identical because the trigger is still inside the statement. `resources(None)` becomes a SUM over at most one row per Workspace, on a path that runs only for reports. Statement journals stay, so the `SQLITE_FULL` behaviour of section 3.1 does not change. Trigger firings per file stay 12; steps per firing fall from 75 to about 37.

**Engine deltas** save more but carry obligations, each of which must be proved per writer site:

- Exactness by `changes()`: every hot writer is a single-row point statement (inode: one INSERT site used three times and five DELETE sites; directory_entry 1 / 2; payload 1 / 3; file_handle 1 / 1; file_custody 4 upserts / 3 deletes / 3 updates; native_lookup 1 / 1 / 2; lease 9 / 9). `[S]`
- Upserts need the insert-or-update fact: INODE_PUT and DIRECTORY_ENTRY_PUT already know it (`added`); the native_parent upsert does not. `[S]`
- Payload bytes need the old cell length on overwrite: a 26-step point read, or keep the payload trigger. `[scratch]`
- FK cascades delete rows the engine does not see (file_handle -> native_file, file_custody -> native_parent); D14 removes the first.
- A failed job must drop its delta: job deltas merge into the transaction deltas only when the job's savepoint is released.
- Memory: 18 counters x (job + group) x a fixed number of namespaces per group (for example 4; a group that would touch a fifth commits first). Fixed per daemon.
- Removing triggers removes statement journals from single-row statements, which changes the `SQLITE_FULL` outcome inside a group (section 3.1).

Recommendation: T3 first (small, SQL only, no new failure mode); engine deltas only if the 128 KiB WRITE saving (-88 against -45) is needed, and then starting with payload. `[I]`

### 3.3 (c) Custody rows

**Which later reader needs each Lease-family write** `[S]`:

| Row written at CREATE | Readers | Verdict |
| --- | --- | --- |
| native_lookup | fence, `native_lookup_row` (`overlay:lifetime/native.rs:235`), FORGET (349-378), retire window (`overlay:maintenance/native.rs:72-78`) | needed. Its `owner` column is only passed back into `drop_native_lookup` and the lease key; UNIQUE(ns,owner) is never an access path |
| lease kind 9 | none by key. Kinds read by key are 1 and 6 (`GENERATION_HELD`, `overlay:database/statements.rs:142-143`, `overlay:maintenance/ready.rs`) and the operation-record kind (`overlay:lifetime/operation_record.rs:91`). Kind 9 is covered only by the namespace-wide `EXISTS(lease WHERE ns)` in `overlay:lifetime/close.rs:145-147` and by the `owner_rows` counter | removable (D10) |
| file_handle | handle fences, release | needed |
| lease kind 7 | as kind 9 | removable (D10) |
| file_custody | decrements returning the remainder (`overlay:lifetime/file_owners.rs:20-51`), `detach_orphan` (`overlay:.../orphan.rs:65`), maintenance deletes, FK parent of native_parent | needed |
| native_file | FENCE_FILE / FENCE_HANDLE / NATIVE_HANDLE_HELD, `native_file_row` (`overlay:lifetime/native_file.rs:49-60`), FILE_WINDOW retire by mount (`overlay:maintenance/native.rs:7`), `retained_native_file` by request (62-73) | mergeable into file_handle (D14) |

Writers of kind 7: `overlay:lifetime/file_owners.rs:124` (native and non-native `open_file`), `overlay:lifetime/native_directory.rs:53`. Writers of kind 9: `overlay:lifetime/native.rs:290`, `overlay:lifetime/lookup.rs:46`. `[S]`

**D10.** Stop writing lease kinds 7 and 9. The Close test `EXISTS(lease WHERE ns)` must become `EXISTS(lease WHERE ns) OR EXISTS(file_handle WHERE ns) OR EXISTS(lookup_owner WHERE ns)` together with the existing native_mount and native_directory terms, or Close would report an owned Workspace as free. `owner_rows` changes meaning. With D10, native_lookup can also drop `owner` and UNIQUE(ns,owner). Measured: 48 / 61 -> 45 / 55, pages 34.98 -> 29.45, **-16 µs** per C01 file; touch -15.9; mkdir -5.7 (one lease row).

**D14.** Fold native_file into file_handle: file_handle gains `mount` (and the kernel request if needed); the retire window uses a partial index `(ns, mount, owner) WHERE mount IS NOT NULL`; UNIQUE(ns,request) is restricted to `request > 0`. `retained_file(route, request)` takes a positive request and can never match a native row (the native request key is `-source.owner`, `overlay:lifetime/native_mutation.rs:79`); the only constructor of `NativeJob::RetainedFile` is in a test (`core/crates/layerfs-daemon/tests/native_jobs.rs:240`). `[S]` Five b-trees become two, and the FK cascade program disappears. Measured on top of D10: 44 / 52, 2282 steps, pages 23.45, **-28.8 µs** per C01 file in total; touch -28.7. The scratch variant kept the empty native_file table, so the real saving is slightly larger. `[scratch]`

**Floor.** CREATE custody cannot go below three writes: native_lookup, file_handle, file_custody. `[I]`

Storage: fewer rows and three fewer b-trees per open handle and lookup. All transient rows, so the committed file does not shrink much, but nothing grows.

Tests that read `owner_rows`, `owner_details` or custody counts `[S]`: overlay `tests/native_lookup.rs` (14 resource reads, `retained_native_file`), `tests/native_directory.rs`, `tests/resources.rs` (6 tests), `tests/engine.rs:371` (`lease_exists`), `tests/native_visit_fence.rs` (StoredCounts snapshot at 111; `a_created_inode_takes_its_custody_without_a_read_and_a_duplicate_fails_whole` at 455); daemon `tests/native_mount.rs`, `edit_backing.rs`, `native_mount_routes.rs`, `mounted_drain.rs`, `native_custody.rs`, `native_jobs.rs:240`; the schema-version assertions (22 -> 23, and `overlay:database/startup.rs:136`).

### 3.4 (d) Other large constants

**INODE_PUT (three per file, about 20 µs).**

- D7: a plain INSERT for a new inode and a plain UPDATE for an existing one, in place of the 15-column upsert. A 12-column UPDATE is 111 steps against 190. D3: memoise the job's parent row so the repeated INODE_LOOKUP / LAYER_ACTIVE reads go away. Measured D3 + D7: 48 -> 37 attempts, 2463 steps, **-15.0 µs**; touch -14.9; mkdir -11.8.
- CHECK `kind IN (1,2,3)` -> `kind BETWEEN 1 AND 3`: removes both ephemeral tables and 14 to 28 steps per INODE_PUT (insert 193 -> 179, update 190 -> 162). About -1.3 µs per file on Linux by the model; larger where ephemeral tables are slow. Schema text change only; same constraint.
- Column-subset UPDATEs: the WRITE job changes size and mtime (63 steps); the parent update changes mtime, entries and subdirs (68 steps); today each is a 190-step upsert. About -1.9 µs more. Total for this group **-18.2 µs** (2355 steps).
- The "four write cursors" of INODE_PUT are inode, inode_capture and two accounting cursors. The update arm opens all four and writes one. T3 or engine deltas remove one or both accounting cursors; a plain UPDATE that does not touch indexed columns opens only the table.

**Secondary indexes.** `[S overlay:sql/schema.sql:37, 47, 68, 69, 102]`

| Index | Readers | Droppable |
| --- | --- | --- |
| `inode_capture(ns,gen,serial)` | INODE_CAPTURE (`overlay:database/statements.rs:5-7`; `overlay:lifetime/generation.rs:194`, `captured_reader.rs:83`), `composition.rs:137`, `maintenance/garbage.rs:81` | only with a primary-key reorder and per-generation probes; **not verified**; -2 pages per file, -2.3 µs, fewer bytes |
| `directory_entry_capture(ns,gen,parent,name)` | DIRECTORY_ENTRY_CAPTURE, SOURCE_NAMES (directory listing: `overlay:namespace/directory_entry.rs:104`, `captured_namespace.rs:28,115`), `composition.rs:64`, `garbage.rs:58` | no: it is the listing access path |
| `payload_generation` | `maintenance/garbage.rs:15` | no |
| `payload_namespace_row` | `maintenance/reclaim.rs:9` | no |
| native_lookup UNIQUE(ns,owner) | none | yes, with D10 |
| native_file's two UNIQUEs | request lookup, handle fence | folded by D14 |

A capture-index drop is a storage candidate, not a speed item.

**PRAGMA freelist_count and the two metadata syscalls per transaction.** PRAGMA 0.59 µs, BEGIN 0.41, stat pair and bookkeeping about 4.3 `[E]`; three times per file is about 16 µs. The lstat is the identity check of the backing file and the PRAGMA feeds the reservation arithmetic; both are part of the admission guarantee and should not be dropped. Group commit pays them once per group, which removes 94 % of them at g = 16 without weakening either. No separate proposal. `[S][I]`

**FRONTIER_ADVANCE per job.** 1.6 µs, twice per file. Under grouping the workspace row is already written once per group in pages; the remaining cost is the statement itself. It could be written once per group, but I did not trace every reader of the frontier inside a group. Low priority, **not verified**.

**Fence in engine memory.** A fence joins workspace state, native_mount and native_lookup (50 steps; the native_lookup probe alone is 16, the workspace state alone 23). The workspace and mount rows are written only by this connection, so a per-mount copy in engine memory can be exact and fixed-size, leaving the native_lookup probe. About -1.8 µs per fence, -9 µs per file `[E]`. This adds a cached copy that every writer of those rows must keep exact, which the owner's simplicity rule ranks below removing steps. Listed for completeness.

### 3.5 Predicted counters per proposal (C01 file)

| Proposal | Attempts | Executions | Transactions | Sub-programs | VM steps (scratch) | Pages written | Service µs |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Today | 48 | 61 | 3 | 13 | 2873 | 34.98 | 161.6 (receipt 162.9) |
| Group commit g=16 | 45.6 | 58.6 | 0.188 | 13 | 2880 | 4.71 | 121.4 |
| D10 | 45 | 55 | 3 | 10 | | 29.45 | 145.6 |
| D10 + D14 | 44 | 52 | 3 | 8 | 2282 | 23.45 | 132.8 |
| T3 | 48 | 61 | 3 | 13 (half the work each) | 2430 | 34.98 | 148.4 |
| Engine deltas | 54 | 55 | 3 | 1 | 2297 | 34.98 | 146.0 |
| D3 + D7 | 37 | 50 | 3 | 13 | 2463 | 34.98 | 146.6 |
| D3 + D7 + range + subset | 37 | 50 | 3 | 13 | 2355 | 34.98 | 143.4 |
| T3 + D14 | | | 3 | | | | 125.0 |
| T3 + D10 + D14 + inode group | 33 | 41 | 3 | 8 | 1501 | | 106.8 |
| same + g=16 | | | 0.188 | 8 | | | 75.8 |
| deltas + D10 + D14 + inode group, no grouping | 39 | 40 | 3 | 1 | 1625 | | 112.0 |
| **deltas + D10 + D14 + inode group + g=16** | **30.9** | **31.9** | **0.188** | **1** | **1219** | **3.44** | **68.1** (g=32: 65.8) |
| deltas + g=16 only | | | 0.188 | | | | 94.7 |
| D10 + D14 + g=16 only | | | 0.188 | | | | 101.7 |

Other units, all cuts + g=16: touch 150.9 -> 65.1 (g=32: 63.2); mkdir 75.6 -> 31.8; 128 KiB WRITE 232.7 -> 121.9 (model; absolute level underestimated).

Cell-level estimate `[E]`: group commit saves about 13.4 µs per transaction at g=16, so C01 about 40 ms of 162.9 ms service, C04 (3250 transactions) about 41 ms of 170.9, C12 (2868 transactions) about 38 ms of 133.7. C02's extra GETATTR is not helped by anything except the fence item. C03's reclamation statements were not priced here.

### 3.6 Interactions

- Grouping and custody overlap in commit pages: D10 + D14 save 11.5 pages per file (about 10 µs) that grouping would have saved anyway. D10 + D14 + g=16 is 101.7, not 161.6 - 40 - 29.
- Engine deltas gain from grouping (one flush per group instead of per transaction): -16 standalone, -27 inside a group.
- Fewer tree mutations per file (58 -> about 20) let more jobs fit one reservation window: 12 files per group today, about 35 afterwards.
- Removing triggers changes the `SQLITE_FULL` outcome inside a group (section 3.1). T3 does not.

---

## 4. Staged plan

Each stage is overlay first, then workspace, then daemon, with the count tests restaged at each layer. Every schema change bumps the schema version 22 -> 23 once (`overlay:database/startup.rs:136` and the tests that assert it).

**Stage A: inode statements (no schema semantics change, no new failure mode).** -18 µs per file.
- Overlay: D7 plain INSERT / UPDATE and column-subset UPDATEs; CHECK ranges in `sql/schema.sql`. Restage `tests/costs.rs` (`complete_mutation_reports_triggers_blob_delivery_and_physical_reservation`), `tests/compound.rs` (`compound_statements_keep_point_work_as_the_namespace_grows`, 378), `tests/native_visit_fence.rs` (`every_visit_statement_is_an_indexed_seek`, 169).
- Workspace: D3 parent-row memo within one job (job-scoped, not a cache across jobs). Restage `tests/native_visit_cost.rs:199` constants (Inode 8 -> about 5 for CREATE, 3 -> 2 for WRITE).
- Daemon: restage `tests/job_cost.rs:9`.

**Stage B: custody D10 then D14.** -29 µs per file; fewer bytes.
- Overlay: stop lease kinds 7 and 9; rewrite the Close EXISTS; drop native_lookup.owner and its UNIQUE; fold native_file into file_handle. Restage `tests/native_visit_fence.rs` (StoredCounts at 111, test at 455), `tests/native_lookup.rs`, `tests/native_directory.rs`, `tests/resources.rs`, `tests/engine.rs:371`, `tests/costs.rs`, `tests/compound.rs`.
- Workspace: `tests/native_visit_cost.rs:199` (Lease (6,12) -> (3,6) for CREATE; (3,7) -> (2,3) for RELEASE) `[I]`.
- Daemon: `tests/job_cost.rs`, and the `owner_rows` readers `native_mount.rs`, `edit_backing.rs`, `native_mount_routes.rs`, `mounted_drain.rs`, `native_custody.rs`; `native_jobs.rs:240` (RetainedFile).

**Stage C: accounting T3.** -13 µs per file, -45 µs per 128 KiB WRITE.
- Overlay: triggers write the namespace row only; `resources(None)` sums. Restage `tests/costs.rs` (both tests; VM-step and execution expectations), `tests/resources.rs`, `tests/compound.rs`. Execution counts do not change, so workspace and daemon count tests change only in VM steps if they assert them.
- Optional later step, payload first: engine deltas (another -43 µs per 128 KiB WRITE), with the obligations of section 3.2.

**Stage D: group commit.** -40 µs per file standalone; about -27 to -44 after stages A to C.
- Overlay: an explicit group (open at first write with today's admission, join with a savepoint, commit); whole-transaction-rollback detection; standalone `atomic` commits an open group first. Existing `tests/transactions.rs` tests stay as they are; add the group's own tests beside them (job rollback alone, group lost on outer failure, admission once per group, allowance bound, cleanup-level group not joined by a mutation). Restage `tests/costs.rs:71`.
- Workspace: each joinable job class declares its tree-mutation and overflow allowance; `tests/native_visit_cost.rs:199` gains a grouped expectation (Startup, Begin, Commit per group, SAVEPOINT / RELEASE per job).
- Daemon: the owner joins the four native visit jobs and commits on a non-joining job, the bounds, its existing idle path and Close. Restage `tests/job_cost.rs:9` and the mounted drain and unmount tests.
- Owner decisions needed before stage D: (1) accepting that a reply precedes the outer COMMIT; (2) behaviour with an explicit page quota; (3) that an idle daemon may hold one uncommitted group.

Order rationale `[I]`: A, B and C are independent of each other, need no owner decision, and together reach about 107 to 112 µs per file. D is the largest single item but changes an observable ordering and needs the three decisions.

---

## 5. Not verified

1. The single extra Lifecycle job in receipt 659 (+1 Workspace, +3 Startup).
2. The OPENDIR / RELEASEDIR statement path in C04 (residuals in section 1.6 are assigned to it by elimination).
3. The 4.3 µs admission cost per transaction: a class residual, not a measurement.
4. VM steps in the product's bundled SQLite: the receipt is 2 to 8 % below scratch 3.51.0 per family; I applied a flat 0.96.
5. Journal and sub-journal memory per group: reasoned from SQLite's design and from pages written; heap statistics are off in the system library.
6. Savepoint and `SQLITE_FULL` behaviour were run on macOS SQLite 3.51.0 only (`savepoint.py`), with `max_page_count` standing in for a full device.
7. Whether the existing maintenance hint wakes the owner thread often enough to bound how long a lazy group stays open.
8. Capture-index drop feasibility (generation bound, primary-key reorder).
9. Exact per-class tree-mutation allowances for group admission (58 per file is from EXPLAIN cursor counts, not from the product's own declaration).
10. Readers of the frontier inside a group, for writing FRONTIER_ADVANCE once per group.
11. The absolute cost of the 128 KiB WRITE: the model gives 232.7 µs against 311 µs reported in the second handoff; only deltas are claimed.
12. Nothing here was run against the product. All µs are model values fitted to receipt 659.

## 6. Scratch files

All in `/private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/f38d048c-3b28-460f-a66e-706b8d433ec1/scratchpad/research/create-sql/`:

- `lib.py` ctypes driver (attempts, RUN, VM_STEP, medians); `src/` pinned source exported with `git show 63c48d8dc`
- `trace.py` the five jobs, statement text copied from source; `python3 -B trace.py 1000`
- `explain.py` bytecode summary per statement (cursors, ephemeral tables, programs)
- `pages.py` pages written per COMMIT
- `probes.py` VM steps per statement and trigger variants (T2, T3, narrow rows, range CHECK)
- `variants.py`, `matrix.py`, `matrix2.py` the proposal matrices for C01, touch, mkdir, 128 KiB WRITE
- `savepoint.py` savepoint-per-job rollback and `SQLITE_FULL` inside a group
