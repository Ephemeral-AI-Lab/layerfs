# C01 per-file SQL at HEAD 3ae26648f: exact trace, VM-step causes, and diet

> **Status:** Research; informative and not a product contract.

Read-only analysis by a research subagent at commit `3ae26648f`, 2026-10-09. Step 8 (`e2521ae7c`) later removed the `request` table and its Frontier statements, and step 10 (`c31c2a42c`) landed part of the diet; file:line references are at `3ae26648f`.

The source-derived trace reproduces every measured family exactly (attempts and executions), with no residual. VM-step figures are inferred from SQL and schema text, not from EXPLAIN, because I kept to read-only inspection.

Prefixes: `OV` = `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/crates/layerfs-overlay`, `WS` = `.../layerfs-workspace/src`, `DM` = `.../layerfs-daemon/src`, `FU` = `.../layerfs-fuse/src`. "read" = read from source; "inferred" = my reasoning.

## 0. Headline

- Of the 102 attempts, about 60 are duplicate reads inside one job, re-validation of something the same job just read, or write-only rows.
- Without changing schema meaning, the cycle fits in **41 attempts / 55 executions**. With four schema changes it fits in **35 / 42**.
- Accounting triggers fire 16 times per file at roughly 70 VM steps each, about 30% of the 3.79k steps (inferred).
- `st` (the workspace state SELECT) runs 18 times per file; 7 are needed (one per job).
- After the diet the floor is the five COMMITs (37 µs measured); two of them exist only to delete the reply ticket.

## 1. Exact trace (read)

Mechanics:
- **Lazy transactions.** A job's reads run in autocommit; the first non-read-only statement triggers the freelist PRAGMA [Startup] and `BEGIN IMMEDIATE` (`OV/src/database/connection.rs:245-271`); `COMMIT` is at `:202`.
- **Executions.** `executions` is `StatementStatus::Run` (`OV/src/diagnostics/metrics.rs:132`), so each trigger firing and each FK-cascade program adds 1. FK parent seeks are inline opcodes and add none (inferred; the totals confirm it).
- **Dispatch.** `ObserveVisit`/`MutateVisit` at `DM/overlay/native_job.rs:225-226`, `CloseFile` at `:208-214`, `ReplyAttempted` at `DM/overlay/commands.rs:796-801`, issued from `FU/operations/mutation.rs:279-285`.

Abbreviations (family in brackets):

| Abbr | Statement | Issued at |
|---|---|---|
| st | workspace state SELECT [Workspace] | `OV/src/lifetime/workspace.rs:38` |
| mt | `native_mount` SELECT [Lease] | `OV/src/lifetime/native.rs:54` |
| nl | `native_lookup` point SELECT [Lease] | `native.rs:235` |
| nf | `native_file ⨝ file_handle` SELECT [Lease] | `OV/src/lifetime/native_file.rs:56` |
| fh | `file_handle` SELECT writable [Lease] | `OV/src/lifetime/file_owners.rs:146` |
| oi | orphan probe `gen=-1` [Inode] | `OV/src/lifetime/orphan.rs:41` |
| il | `INODE_LOOKUP` [Inode] | `OV/src/namespace/inode.rs:84-90` |
| da | `DIRECTORY_ENTRY_ACTIVE` [DirectoryEntry] | `OV/src/namespace/compound.rs:366-373`, `inode.rs:194-202` |
| dl | `DIRECTORY_ENTRY_LOOKUP` lower [DirectoryEntry] | `compound.rs:231-239`, `inode.rs:206-214` |
| la / ll | `LAYER_ACTIVE` / `LAYER_LOWER` [Inode] | `inode.rs:122-130` / `:135-144` |
| +T | one accounting trigger firing (`OV/sql/accounting.sql`) | +1 execution |

**Job 1: GETATTR(root)** — `observe_native_visit`, `OV/src/lifetime/native_visit.rs:63`. No transaction.

| # | Stmt | Caller | Purpose |
|---|---|---|---|
| 1 | st | `native_visit.rs:74` → `native.rs:75` | workspace live, state |
| 2 | mt | `native.rs:76` | mount attached, not revoked |
| 3 | nl | `native_visit.rs:75` → `:34` | kernel holds the inode |
| 4, 5 | oi, il | `WS/operations/native_read.rs:184-186` → `WS/mutation/eval.rs:30` → `inode.rs:80-90` | root's local row (hit) |

Totals: W1, L2, I2.

**Job 2: LOOKUP(root, name) negative** — same entry point.
- Statements 1–3 as job 1.
- Round 1 (`native_read.rs:203-204`): oi, il (parent); da, dl. This ends in `Need::Name` because root is a base directory (`eval.rs:65-79`).
- Round 2, after the in-memory `supply` (`WS/operations/native_visit.rs:234-246`): oi, il, da, dl again, then Refused(Missing).

Totals: W1, L2, I4, D4. No transaction.

**Job 3: CREATE** — `mutate_native_visit`, `native_visit.rs:116`.

| # | Stmt | Caller | Trigger / FK | Purpose |
|---|---|---|---|---|
| 1 | st | `native_visit.rs:128` | | fence |
| 2 | mt | same | | fence |
| 3 | nl | `:132` | | parent reference |
| 4 | st | `WS/mutation/job.rs:101` → `compound.rs:28` → `source.rs:75` | | `source_rows`, round 1 |
| 5–8 | oi, il, da, dl | `WS/operations/namespace/create.rs:17-18` | | parent, name → Needs |
| 9 | st | `job.rs:101` | | `source_rows`, round 2 |
| 10–13 | oi, il, da, dl | `create.rs:17-18` | | same reads, now decides |
| 14–17 | oi, il, da, dl | `job.rs:125`, `:128` | | re-read to compute `inherited` |
| 18 | st | `compound.rs:140` | | `apply_checked` re-validation |
| 19, 20 | la, ll (file) | `inode.rs:122`, `:135` | | fresh serial: both miss |
| — | freelist PRAGMA, BEGIN | `connection.rs:265`, `:253` | | first write |
| 21 | `INODE_PUT` (file) | `inode.rs:161` | +T `accounting.sql:31`; FK seek | new inode |
| 22 | la (parent) | `inode.rs:122` | | hit |
| 23 | `INODE_PUT` (parent) | `inode.rs:161` | upsert-update, no trigger | mtime, entries |
| 24, 25 | da, dl | `compound.rs:175` → `inode.rs:186` | | whiteout decision |
| 26, 27 | da, dl | `inode.rs:226` | | exact duplicate of 24, 25 |
| 28 | `DIRECTORY_ENTRY_PUT` | `inode.rs:227` | +T `:39` | bind name |
| 29 | `TICKET_PUT` [Frontier] | `inode.rs:254` | +T `:151` | reply ticket |
| 30 | `FRONTIER_ADVANCE` [Workspace] | `inode.rs:260` | | revision, dirty counts |
| 31 | nl | `native.rs:285` (from `native_mutation.rs:76`) | | insert or increment; misses |
| 32 | INSERT `native_lookup` | `native.rs:258` | +T `:221` | kernel lookup reference |
| 33 | INSERT `lease(9)` | `native.rs:271` | +T `:95` | owner row |
| 34 | `FILE_LOOKUPS_ADD` | `native.rs:277` → `file_owners.rs:36` | +T `:203` (insert path) | custody count |
| 35 | INSERT `file_handle` | `file_owners.rs:100` | +T `:111` | descriptor |
| 36 | INSERT `lease(7)` | `file_owners.rs:106` | +T `:95` | owner row |
| 37 | `FILE_OPENS_ADD` | `file_owners.rs:112` | update path, no trigger | custody count |
| 38 | INSERT `native_file` | `native_mutation.rs:85` | +T `:248`; 2 FK seeks | mount ↔ handle |
| — | COMMIT | | | |

Totals: W5, I11/12, D11/12, F1/2, L10/16.

**Jobs 4 and 6: reply-attempted** — `OV/src/lifetime/generation.rs:140`.

| # | Stmt | Caller | Trigger |
|---|---|---|---|
| 1 | st | `generation.rs:164` | |
| — | PRAGMA, BEGIN | | |
| 2 | DELETE `request` [Frontier] | `:165-174` | +T `accounting.sql:155` |
| 3 | st | `queue_closed`, `OV/src/lifetime/close.rs:129` | |
| — | COMMIT | | |

Totals each: W2, F1/2.

**Job 5: WRITE** (2 bytes at offset 0) — `mutate_native_visit` with a handle.

| # | Stmt | Caller | Trigger | Purpose |
|---|---|---|---|---|
| 1, 2 | st, mt | `native_visit.rs:128` | | fence |
| 3 | nf | `:130` | | descriptor |
| 4, 5 | st, fh | `job.rs:98` → `file_owners.rs:142`, `:146` | | `check_file` |
| 6 | st | `job.rs:101` | | `source_rows` |
| 7, 8 | oi, il | `WS/operations/file/write.rs:21` | | file inode |
| 9 | st | `compound.rs:140` | | apply |
| 10, 11 | st, fh | `compound.rs:153` | | `check_file` again |
| 12 | la | `inode.rs:122` | | hit |
| — | PRAGMA, BEGIN | | | |
| 13 | `INODE_PUT` | `inode.rs:161` | update, no trigger | size, mtime |
| 14 | `CELL_LOOKUP` [Payload] | `OV/src/payload/stream.rs:52` → `layers.rs:99` | | partial cell: old bytes (miss) |
| 15 | `CELL_PUT` | `stream.rs:62` → `layers.rs:121` | +T `accounting.sql:47` | cell |
| 16 | `TICKET_PUT` | `inode.rs:254` | +T | ticket |
| 17 | `FRONTIER_ADVANCE` | `inode.rs:260` | | |
| — | COMMIT | | | |

Totals: W6, L4, I4, P2/3, F1/2.

**Job 7: RELEASE** — `close_native_file`, `native_file.rs:91`.

| # | Stmt | Caller | Trigger / FK | Executions |
|---|---|---|---|---|
| 1, 2 | st, mt | `native_file.rs:45` → `native.rs:79-82` | | 2 |
| 3 | nf | `native_file.rs:46` | | 1 |
| 4, 5 | st, fh | `file_owners.rs:171` | | 2 |
| — | PRAGMA, BEGIN | | | |
| 6 | DELETE `file_handle` | `file_owners.rs:172` | +T `:115`; FK cascade (`OV/sql/schema.sql:242`) deletes `native_file`; +T `:252` | 4 |
| 7 | DELETE `lease(7)` | `file_owners.rs:182` | +T `:99` | 2 |
| 8 | `FILE_OPENS_DROP` | `file_owners.rs:38` | | 1 |
| 9 | `FILE_REFS` | `file_owners.rs:42` | | 1 (returns 1: the kernel lookup) |
| 10 | st | `file_owners.rs:198` → `close.rs:129` | | 1 |
| — | COMMIT | | | |

Totals: W3, L7/11.

**Reconciliation** (attempts / executions):

| Family | Sum by job | Total | Measured |
|---|---|---|---|
| Workspace | 1+1+5+2+6+2+3 | 20 | 20 ✓ |
| Inode | 2+4+11+4 | 21 / 22 | 21 / 22 ✓ |
| DirectoryEntry | 4+11 | 15 / 16 | 15 / 16 ✓ |
| Payload | 2 | 2 / 3 | 2 / 3 ✓ |
| Frontier | 1+1+1+1 | 4 / 8 | 4 / 8 ✓ |
| Lease | 2+2+10+4+7 | 25 / 35 | 25 / 35 ✓ |
| Begin, Commit, Startup | one per write transaction | 5 each | 5 each ✓ |

The 17 extra executions are 16 trigger firings plus 1 FK-cascade program.

## 2. Why the VM step counts are high

**Anchors (inferred).**
- **`st` ≈ 22 steps.** `st` is a rowid seek, an incarnation compare and 11 columns. With `FRONTIER_ADVANCE` at about 39, 18×22 + 2×39 = 474.
- **One trigger firing ≈ 70 steps.** Each trigger runs two `UPDATE accounting`, and `accounting` is an 18-column rowid table (`accounting.sql:2-21`), so each UPDATE reloads all 17 counters and rewrites the row (about 33 steps). Frontier fits: 4 × (about 29 for a one-b-tree PK write + 70) ≈ 396.

**Heaviest items, in order (estimates).**
1. **Accounting triggers:** 16 × ~70 ≈ 1.1k steps (Lease 9, Frontier 4, Inode 1, DirectoryEntry 1, Payload 1). The payload trigger is heavier (about 85) because of its `length()` terms.
2. **`INODE_PUT` ×3:** about 500 in total. The insert path is about 105; each upsert-update is about 185.
   - Fourteen NOT NULL checks and ten CHECKs run, including `kind IN (1,2,3)`, which builds an ephemeral IN-table (noted at `compound.rs:313-314`).
   - The `DO UPDATE` branch re-reads the row and re-runs the CHECKs for all 11 SET columns.
3. **`st` ×18:** about 400.
4. **Lease custody writes (10):** about 400 before triggers. `native_file` is the worst at about 60 (3 b-trees and 2 FK parent seeks).
5. **`CELL_PUT`:** about 86 before its trigger.
6. **Seven oi/il pairs (~260) and seven da/dl pairs (~190):** cheap each, but mostly repeats.

**Why the per-attempt averages are what they are.**
- Lease is 55 per attempt because 9 of its 10 writes carry a trigger.
- Frontier is 99 because it is 100% trigger-bearing writes.
- Payload is 91 because it is one 4-b-tree upsert plus the heaviest trigger.
- Inode is 43 because of the three puts.

**B-trees each write maintains (read, `schema.sql`).**

| Table | B-trees | Lines |
|---|---|---|
| `inode` | PK + `inode_capture` = 2 | `:34`, `:36` |
| `directory_entry` | PK + `directory_entry_capture` = 2 | `:44`, `:46` |
| `payload` | rowid + UNIQUE autoindex + `payload_namespace_row` + `payload_generation` = 4 | `:48`, `:56`, `:73-74` |
| `request` | PK = 1 | `:71` |
| `lease` | PK + `lease_resource` = 2 | `:81`, `:107` |
| `file_custody` | PK = 1 | `:130` |
| `file_handle` | PK + UNIQUE(ns,request) = 2 | `:138` |
| `native_lookup` | PK + UNIQUE(ns,owner) = 2 | `:217` |
| `native_file` | PK + 2 UNIQUE = 3 | `:240` |

Every insert also does an FK parent seek to `workspace(ns)`. Per file that is 22 b-tree inserts, 9 deletes, and 32 whole-row rewrites of two `accounting` rows.

## 3. Readers of the affected rows (read, by grep of `OV/src`)

- **`accounting`:** only `Overlay::resources` (`OV/src/database/accounting.rs:43-44`), reached from `DM/overlay/commands.rs:474-477,509-512`. The only constructor in `src` is `Resources { global: true }` at `DM/application/diagnostics/resources.rs:96`. No admission or product decision reads it; admission uses physical allocation (`connection.rs:260-300`). About 20 test files outside `src` assert on these counts, including `OV/tests/resources.rs` and `OV/tests/costs.rs`.
- **`lease`:**
  - `GENERATION_HELD` reads kinds 1 and 6 only (`OV/src/database/statements.rs:133`, used at `OV/src/maintenance/ready.rs:137`).
  - `OV/src/lifetime/operation_record.rs:91` reads kind 4.
  - `LEASE_LOOKUP` is used only for EXPLAIN (`OV/src/diagnostics/access_plan.rs:12`).
  - `close.rs:134` tests any-row EXISTS.
  - Kinds 5, 7 and 9 are never read by key, and their DELETEs do not check `changes` (`file_owners.rs:182-191`, `native.rs:322-327`). They are write-only apart from that EXISTS and the `owner_rows` count.
- **`request`:**
  - `capture_ready` (`generation.rs:128-136`, called at `DM/overlay/owner.rs:430`).
  - `capture` (`generation.rs:187-191`).
  - `queue_closed` (`close.rs:134`).
  - `pending_publications` (`OV/src/lifetime/frontier.rs:39-58`); `Command::PendingPublications` has no constructor in the DM, FU or WS `src` trees.
  - The first three need only "any ticket for this ns?". Capture parks until tickets settle (`docs/architecture/21-daemon-owner.md:58-60`).
- **`file_custody`:** `FILE_REFS` at `file_owners.rs:42`, `orphan.rs:60` and `OV/src/maintenance/orphan.rs:21`; deletes at `maintenance/orphan.rs:14,142` and `OV/src/maintenance/reclaim.rs:16-17`. It decides orphan retention, so it stays.
- **`native_lookup`:** `native_lookup_row` (`native.rs:235`) and the retire window (`OV/src/maintenance/native.rs:73`). Stays.
- **`native_file` / `file_handle`:** `native_file.rs:56,69`, `NATIVE_HANDLE_HELD` (`statements.rs:136`), `check_file` (`file_owners.rs:146`), `retained_file` (`:126`), `FILE_WINDOW` (`maintenance/native.rs:7`).
- **`workspace` writers (15 sites):** `inode.rs:262`; `native.rs:139`; `source.rs:26,145`; `file_owners.rs:252`; `close.rs:81,100`; `generation.rs:43,194`; `OV/src/lifetime/composition.rs:24,118,142,162,174,193`; `workspace.rs:21`; `reclaim.rs:90`.

## 4. The diet

Classes: K keep, M merge, C cache, R remove. "Same-job argument" means: the single owner runs one job to completion (`connection.rs:12-13`, `compound.rs:10-13`), and install, revoke, close and maintenance are owner jobs too (`native_visit.rs:1-5`).

| ID | Class | Change | Saved per file | Invariant today → why it still holds | Risk |
|---|---|---|---|---|---|
| D1 | R | Reuse the job's first `st`. Use `source_rows_at(source, state)` in `decide` (as `native_visit.rs:77` already does), and pass `&state` to `apply_checked`, `check_file` and `queue_closed`. | 11 st (CREATE 3, WRITE 4, reply 1×2, RELEASE 2), ~240 steps | Fence against close, install and capture. The only workspace write inside these jobs is their own final `FRONTIER_ADVANCE`. Class-3 `source_held` is already an in-memory compare (`source.rs:85-90`). | Low |
| D2 | R | Drop `check_file` after `nf` in the WRITE visit (`job.rs:98`, `compound.rs:153`) and in `close_native_file` (`file_owners.rs:171`). Keep `!writable → Invalid` (`:160-162`) as an in-memory check on the `OpenFile` that `nf` returned. | 3 fh | Descriptor exists and is writable. `nf` proved the same row in the same job. Non-native callers keep `check_file`. | Low |
| D3 | R | Job-scoped row memo: at most 2 inodes and 2 name-layer results, dropped at the job's first write. Round 2 and the `job.rs:125-128` re-reads hit it. | 6 Inode + 6 DE | Decisions use current rows. No write precedes them in the job. Fixed size, not workload-sized. | Low–medium (plumbing) |
| D4 | R | `name_inheritance` runs at `compound.rs:175` and again at `inode.rs:226` with identical arguments: pass the result. With D3, take it from decide's read. | 4 DE | Same-job argument. | Low |
| D5 | M | da + dl → one seek, SQL below. | 2 DE | Same PK range. | Low |
| D6 | C | Skip `oi` unless a sticky `orphan_seen` flag is set. Set it at the only `INSERT INTO orphan` (`orphan.rs:63-74`); never clear it. | 4 Inode (7 together with D3) | `gen=-1` inode rows are created only after that INSERT (`orphan.rs:75-95`) or under an existing orphan row (`inode.rs:113-118`). The database is never reopened (`connection.rs:27-28`). A rollback leaves only a false positive, the same pattern as `maintenance_ready` (`connection.rs:20-22`). | Low |
| D7 | R/M | `il` also returns `gen, epoch, height`. `put_inode_domain` derives the layer: `gen == active` gives la's values; otherwise la misses and ll equals that row's size. A fresh reserved serial gets a plain INSERT. A known active row gets a plain UPDATE. | la×3, ll; ~240 steps from the two upsert-updates | Layer facts are current (same job). A plain INSERT turns a serial collision into a constraint failure rather than a silent update. Excludes the orphan domain. | Medium |
| D8 | R | `add_native_lookup` pre-read (`native.rs:285`) for a just-created inode: insert directly. | 1 Lease | PK(ns,mount,serial) rejects a duplicate. Not valid for LINK. | Low |
| D9 | M | Fence = st + mt + reference in one SELECT, SQL below. | 10 Lease attempts (mt 5, nl 3, nf 2) | Same rows, same Stale/Closed mapping (`workspace.rs:61-69`, `native.rs:60-70,83-88`, `native_visit.rs:52-56`). | Low |
| D10 | R | Stop writing `lease(7)` and `lease(9)` for native custody. `queue_closed` tests the detail tables (or relies on its `native_mount` EXISTS). | 3 Lease + 3 triggers, ~320 steps | A closed namespace is held while custody exists. Native custody rows are retired before the `native_mount` row (`maintenance/native.rs:33-90`). Non-native `open_file` and `lookup_owner` need their own EXISTS or keep their rows. | Medium |
| D11 | M | One custody upsert for create+open; drop + refs via RETURNING. | 2 Lease | Same counts. | Low |
| D12 | schema | Ticket rows → `workspace.reply_pending` counter; drop `request`. | Frontier 4/8 → 0, ~390 steps | Capture and reclaim wait for reply attempts; all three live readers need only "> 0". | Medium; see §6 |
| D13 | schema | Accounting: (a) one row per (ns, counter), so a trigger UPDATE is ~13 steps instead of ~33; or (b) drop the per-ns half, which has no `src` reader; or (c) in-memory aggregate. | (a) ~40 steps × remaining triggers; (c) all trigger executions | (a) and (b) keep exact transactional counts. (c) does not. | (a) low, (c) high |
| D14 | schema | Fold `native_file` (mount, request) into `file_handle`. | 1 Lease, FK cascade, 2 triggers | Needs a partial index for the retire window (`maintenance/native.rs:7`) and per-request uniqueness. | Medium |
| D15 | C | Skip `CELL_LOOKUP` when `layer.epoch == 0` and the cell offset ≥ the layer's old size. | 1 Payload | Cells of a generation lie below a size its row has had; size falls only through `shrink`, which bumps epoch (`inode.rs:157-159`, `layers.rs:180-183`). I did not audit the cell movers (`maintenance/orphan.rs`, `composition.rs`). | Medium, unverified |

Statements kept as is: `il`, `DIRECTORY_ENTRY_PUT`, `CELL_PUT`, `FRONTIER_ADVANCE`, INSERT `native_lookup`, INSERT `file_handle`, DELETE `file_handle`, BEGIN, COMMIT.

Merged SQL:

```sql
-- D9 fence, inode-addressed (?3 mount owner, ?4 serial)
SELECT w.active,w.captured,w.revision,w.base_root,w.dirty_inodes,w.dirty_directory_entries,
       w.lifecycle,w.installed,w.captured_revision,w.base_readers,w.consolidating,
       m.owner,m.root,m.revoked,
       (SELECT 1 FROM native_lookup l WHERE l.ns=w.ns AND l.mount=?3 AND l.serial=?4)
FROM workspace w LEFT JOIN native_mount m ON m.ns=w.ns WHERE w.ns=?1 AND w.incarnation=?2;
-- handle-addressed: replace the scalar subquery (?5 = handle)
       (SELECT f.writable FROM native_file n JOIN file_handle f ON f.ns=n.ns AND f.owner=n.owner
         WHERE n.ns=w.ns AND n.mount=?3 AND n.owner=?5 AND f.serial=?4)
-- D5 name layers: the row with gen=?4 is active; the first row with gen<?4 is lower
SELECT gen,serial,inherited FROM directory_entry
 WHERE ns=?1 AND parent=?2 AND name=?3 AND gen<=?4 AND gen>?5 ORDER BY gen DESC LIMIT 2;
-- D11
INSERT INTO file_custody(ns,serial,opens,lookups) VALUES(?1,?2,1,1)
  ON CONFLICT(ns,serial) DO UPDATE SET opens=opens+1,lookups=lookups+1;
UPDATE file_custody SET opens=opens-1 WHERE ns=?1 AND serial=?2 AND opens>0
  RETURNING opens+lookups+readers;
-- D12
UPDATE workspace SET revision=?2,reply_pending=reply_pending+1,dirty_inodes=dirty_inodes+?3,
  dirty_directory_entries=dirty_directory_entries+?4 WHERE ns=?1;
UPDATE workspace SET reply_pending=reply_pending-1 WHERE ns=?1 AND incarnation=?2 AND reply_pending>0
  RETURNING lifecycle,captured,base_readers;   -- changes<>1 => Stale; the row feeds queue_closed
-- D6 fallback when orphans exist: one statement, first row wins, no sort
SELECT <cols>,gen FROM inode WHERE ns=?1 AND serial=?2 AND gen=-1
UNION ALL SELECT * FROM (SELECT <cols>,gen FROM inode WHERE ns=?1 AND serial=?2 AND gen<=?3 AND gen>?4
                         ORDER BY gen DESC LIMIT 1) LIMIT 1;
```

Answers to the specific questions:
- **(a) Accounting.** The rows are observation-only (§3). Deltas flushed with one UPDATE at commit do not help much while there are five write transactions: a 17-column delta UPDATE over two rows is about as costly as two or three triggers (inferred). D13(a) or (b) is the cheap win; D10 and D14 remove five firings outright.
- **(b) `st` per job.** Counts are 1, 1, 4, 2, 5, 2, 3. Each extra run is a helper re-validating by itself: `source_rows`, `apply_checked`, `check_file`, `queue_closed`.
- **(c) Read pairs.** oi + il: D6, with the UNION form as fallback. da + dl: D5. Of the seven pairs of each, only four il and two name seeks are needed.
- **(d) Reply ticket.** Readers are in §3. D12 keeps the custody in SQLite as one integer.
- **(e) RELEASE.** It shrinks from 10 statements to 3.
- **(f) WRITE.** Handle validation is `nf` once (D2). The cell lookup is D15.

## 5. Minimal per-job lists and projection

After D1–D11 (no change to schema meaning; D10 only stops writing rows nobody reads by key):

| Job | Ordered statements | Per family |
|---|---|---|
| GETATTR | fence; il | W1, I1 |
| LOOKUP | fence; il (parent); name | W1, I1, D1 |
| CREATE | fence; il (parent); name; BEGIN; INSERT inode +T; UPDATE inode (parent); `DIRECTORY_ENTRY_PUT` +T; `TICKET_PUT` +T; `FRONTIER_ADVANCE`; INSERT `native_lookup` +T; custody upsert +T; INSERT `file_handle` +T; INSERT `native_file` +T; COMMIT | W2, I3/4, D2/3, F1/2, L4/8 |
| reply ×2 | st; BEGIN; DELETE `request` +T; COMMIT | W1, F1/2 each |
| WRITE | fence (handle); il (file); BEGIN; UPDATE inode; `CELL_LOOKUP`; `CELL_PUT` +T; `TICKET_PUT` +T; `FRONTIER_ADVANCE`; COMMIT | W2, I2, P2/3, F1/2 |
| RELEASE | fence (handle); BEGIN; DELETE `file_handle` (+T, FK cascade, +T); custody UPDATE…RETURNING; COMMIT | W1, L2/5 |

| Family | Now | After D1–D11 | + D12, D14, D15 |
|---|---|---|---|
| Workspace | 20 | 9 | 9 |
| Inode | 21 / 22 | 7 / 8 | 7 / 8 |
| DirectoryEntry | 15 / 16 | 3 / 4 | 3 / 4 |
| Payload | 2 / 3 | 2 / 3 | 1 / 2 |
| Frontier | 4 / 8 | 4 / 8 | 0 |
| Lease | 25 / 35 | 6 / 13 | 5 / 9 |
| Begin + Commit | 10 | 10 | 10 |
| **Total** | **102 / 119** | **41 / 55** | **35 / 42** |

With D13(c) the last column's executions drop to 35.

Projected VM steps (inferred): 3.79k → about 2.5k after D1–D11 → about 1.4–1.6k with D12, D13(a) and D14. Projected statement time, using the measured µs per attempt by family: 197 → about 115 → about 95 µs, of which 37 µs is the five COMMITs.

**Ranking by VM steps saved:** D10 (~320) and D7 (~290) lead, then D13(a) (~40 per remaining trigger; ~360–520), D12 (~390), D1 (~240), D3 (~190), D14 (~150), and the rest under 60 each.

**Ranking by attempts saved:** D3 (12), D1 (11), D9 (10), D7 (4), D4 (4), D6 (4), D12 (4), D2 (3), D10 (3).

**Suggested order by risk.** First D1, D2, D4, D5, D6, D8, D9 and D11: all are local, about 37 attempts, with no schema or contract change. Then D3 and D7. Then D10 and D13(a). Then D12 and D14. D15 only after auditing the cell movers.

## 6. Boundary notes

- **Resource-only bounds hold.** D3's memo is a fixed window of one job and D6 is one boolean. No proposal adds a container that grows with files, bytes or mutations; all custody stays in indexed rows.
- **Cross-job caching rejected.** I do not propose caching `workspace` or `native_mount` rows across jobs: 15 writer sites plus rollback would have to invalidate it, and it needs a per-namespace map.
- **No retry or replay.** Every merged statement is one attempt with the same Stale/Closed outcomes.
- **Permission checks unchanged.** None of the removed statements is one; the read-only-descriptor refusal is kept in memory (D2).
- **D12 weakens two things.**
  - Exact ticket identity is lost: today releasing the same (revision, gen) twice is Stale (`generation.rs:165-177`); a counter detects that only at zero.
  - `pending_publications` paging goes away; it has no `src` caller, but tests may use it.
  - An in-memory ticket would also remove the two reply transactions (about 20 µs more) but moves custody out of SQLite, which needs an owner decision.
- **D10 changes the held predicate** of `queue_closed`. Non-native `open_file` (`file_owners.rs:64`) and `lookup_owner` (`OV/src/lifetime/lookup.rs:46`) must stay covered.
- **D13(c) loses exact `payload_bytes`.** A full-cell overwrite never reads the old length (`stream.rs:49-50`). D13(b) removes per-namespace counts, which only tests read.
- **D7's plain INSERT is stricter than today.** D15 rests on an invariant I did not verify against `composition.rs` or `maintenance/orphan.rs`.
- **Schema changes** (D12, D13, D14) need a `PRAGMA user_version` bump (`schema.sql:288`) and the readback check at `OV/src/database/startup.rs:136`.