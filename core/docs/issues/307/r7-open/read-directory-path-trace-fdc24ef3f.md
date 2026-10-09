# Read and directory path at fdc24ef3f: exact trace, minimal design, directory link counts

> **Status:** Research; informative and not a product contract.

Read-only analysis by a research subagent at commit `fdc24ef3f`, 2026-10-09 (first report). Nothing was built or run; time savings are projections from counts.

Paths are relative to `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/crates/`. Short prefixes: `fuse/` = `layerfs-fuse/src/`, `dmn/` = `layerfs-daemon/src/`, `ws/` = `layerfs-workspace/src/`, `ovl/` = `layerfs-overlay/src/`, `cnt/` = `layerfs-content/src/`. **[S]** means read from source at fdc24ef3f; **[I]** means inference. Nothing in the repository was edited, built or run.

## 0. Headline

- **C09 reproduces exactly, residual 0**: 3,082 jobs, 1,026 reader grants, 514 length batches and 44,681 statement executions all fall out of the trace below.
- **One READ costs 6 jobs, 4 write transactions, 2 reader grants, 1 length batch and 87 executions.** Of the 87, 15 are trigger programs: SQLite counts each accounting trigger and each `ON DELETE CASCADE` action program as a run. That is why Lease shows 21,049 attempts but 28,754 executions in the C09 receipt (`559-C09-B-L-sample-fdc24ef3f/sample/receipt.json`). [I, validated by the exact total]
- **Nothing remembers base facts between requests.** Every READ starts with empty `BaseFacts` (`ws/operations/native_read.rs:230`), so it re-reads the inode, the metadata object and the file length through the Store before reading any data.
- **The canonical format carries neither a file length nor a directory link count** in the inode record or the directory entry (details in §2.4 and §3).

## 1. Exact trace

### 1.1 Common mechanics [S]

- **Threads.** A request's first step runs on the receive thread (`fuse/dispatch/admission.rs:253`). An owner job runs on the submitting thread when the connection is free (`dmn/overlay/owner.rs:351-366`), otherwise on the owner thread. `LeaveReceiver` (`fuse/dispatch/task.rs:320-335`) moves the continuation to a pool worker. `NextTurn` (`task.rs:303-315`) requeues it.
- **Write transaction.** The first writing statement of a job adds `PRAGMA freelist_count` (Startup), `BEGIN IMMEDIATE` and later `COMMIT` (`ovl/database/connection.rs:271-297`, `:228`). A job that writes nothing has none.
- **Executions.** The counter is `StatementStatus::Run` (`ovl/diagnostics/metrics.rs:131-132`). `foreign_keys=ON` is set at `ovl/database/profile.rs:59`.
- **Recurring statement groups.**
  - `check_native_mount` is W1 + L1: `state` (`ovl/lifetime/workspace.rs:34-62`) and the `native_mount` select (`ovl/lifetime/native.rs:49-78`).
  - `source_held` is L1 (`BASE_SOURCE_LOOKUP`, `ovl/lifetime/source.rs:79-103`).
  - `inode_at` is I2: the orphan probe at `gen=-1`, then `INODE_LOOKUP` (`ovl/namespace/inode.rs:72-92`, `ovl/lifetime/orphan.rs:40-43`).
  - `queue_closed` is W1 (`ovl/lifetime/close.rs:128-145`).
  - `file_ref` add is L1; drop is L2 (`ovl/lifetime/file_owners.rs:20-48`).
- **Job classes** are assigned at `dmn/overlay/native_job.rs:98-109` and `dmn/overlay/commands.rs:279-349`.

In the tables, W = Workspace, L = Lease (attempts + triggers), I = Inode, DE = DirectoryEntry, R = Reclaim, txn = Startup + Begin + Commit.

### 1.2 (b) READ of 128 KiB, all from base

Entry is `fuse/request/callbacks.rs:137-167`, then `fuse/request/reply.rs:77-86`, then `fuse/operations/lookup.rs:247-298` and `fuse/operations/read.rs:35-91`.

| # | Job (class) | Submitted at | Performed at | Statements | txn |
|---|---|---|---|---|---|
| 1 | `HandleSource` (Source) | `lookup.rs:247`, `dmn/service/filesystem_port.rs:301-325` | `ovl/lifetime/native_file.rs:11-37`, `native.rs:106-162` | W2 (state, `base_readers+1`); L6+3 (`native_mount`, `NATIVE_HANDLE_HELD`, insert `native_source`, `base_source`, `lease(5)`, `FILE_READERS_ADD`) | yes |
| 2 | `Observe` #1 (Read) | `lookup.rs:266`, port `:351-362` | `ws/operations/native_read.rs:105-124`, `ovl/lifetime/native_observation.rs:72-106` | W1; L4 (mount, source, `native_source`, `native_read` exists); I2. Returns `Needs[Inode]` | no |
| – | fact read | `lookup.rs:290-297` | `ws/mutation/facts.rs:112-135` on a worker | reader grant 1; inode table, metadata object, **length batch** (`ws/base/view.rs:139`, `dmn/store/read_scope.rs:55-69`, `dmn/store/read_handle.rs:74-86`) | – |
| 3 | `Observe` #2 (Read) | same | `native_observation.rs:107-178` | W2; L10+4 (the 4 checks, `decided=1`, insert `file_read`, `base_source`, `lease(5)`, `FILE_READERS_ADD`, `native_read`); I2 | yes |
| 4 | `FileRead` (Read) | `read.rs:74`, port `:255-278` | `ovl/lifetime/orphan.rs:136-149`, `ovl/payload/stream.rs:69-135` | W2; L5 (source ×2, `file_read`, orphan ×2); I1 (`LAYERS`). Returns `None` | no |
| – | data read | `read.rs:76-90` | `ws/operations/file/read.rs:93-124`, `ws/base/view.rs:151-183` on a worker | reader grant 2; root object, inode walk, `FileView::open`, `read_range` | – |
| 5 | `ReleaseFileRead` (Lifecycle) | `lookup.rs:302`, port `:416` | `file_owners.rs:320-349` | W4; L9+4 | yes |
| 6 | `ReleaseBaseSource` (Lifecycle) | `lookup.rs:306`, port `:419` | `source.rs:112-150`, `native.rs:195-228` | W3; L7+4 (includes the cascade program to `native_directory_read`, `ovl/../sql/schema.sql:267`) | yes |

- **Per READ:** 6 jobs (Source 1, Read 3, Lifecycle 2), 4 transactions, 2 grants, 1 length batch.
- **Executions per job:** 14, 7, 21, 8, 20, 17, which sum to **87** (W14, L41+15, I5, txn 12).
- **Hand-offs:** `LeaveReceiver` at `lookup.rs:290`, `NextTurn` at `lookup.rs:297`, two parked reader-ticket awaits, six owner turns. The second `LeaveReceiver` (`read.rs:76`) is ready at once on the worker.
- **Bytes resident or not:** jobs and statements are identical. The only difference is on the worker: a cache hit copies the object (`ws/base/cache.rs:78`); a miss goes to the Store and then inserts (`ws/base/client.rs:140-195`). `storage.reader()` builds a new `ReadState` on every call (`dmn/store/read_handle.rs:61-86`).

### 1.3 Other requests

| Request | Jobs today | txn | Grants / length batches | Executions | Notes [S] |
|---|---|---|---|---|---|
| (a) OPEN, base file, read-only | Source 1, Read 2, Lifecycle 2 | 4 | 1 / 1 | 86 | `Source` (`native.rs:91-104`), 14. `Observe` #1, 7. Grant and length. `Observe` #2, 28: adds `file_handle`, `lease(7)`, `FILE_OPENS_ADD`, `native_file` and a retained read (`native_observation.rs:126-177`). Two releases, 20 + 17. |
| (c) READ, all local | Source 1, Read 2, Lifecycle 2 | 4 | **1 (unused)** / 0 | about 80 + 1 Payload | One `Observe` decides. `FileRead` runs `CELL_RANGE`. `read.rs:76-77` still takes a reader and leaves the receive thread although no inherited span exists. |
| (d) RELEASE | Lifecycle 1 | 1 | 0 | 17 | `callbacks.rs:168-219`, `native_file.rs:92-102`, `file_owners.rs:170-199`. W3, L7+4: deleting `file_handle` fires the accounting trigger, the cascade program (`schema.sql:236`) and the `native_file` trigger. |
| (e) OPENDIR | Source 1, Read 1–2, Lifecycle 2 | 4 | 0–1 / 0 | about 80 | `native_directory.rs:31-68`. It retains a `FileRead` on a directory. |
| (e) READDIR, n ≤ 64 entries | Lifecycle 2, Source 1, Read 3 | 4 | 1 | about 520 at n = 64 | See below. |
| (e) READDIR at EOF | Lifecycle 2, Source 1, Read 1 | 2 | 1 | about 35 | Handle, Read, Page, release. |
| (e) RELEASEDIR | Lifecycle 2 | 1 | 0 | about 20, R1+1 | `fuse/request/directory.rs:100-103`, `native_directory.rs:111-147`. Queues NATIVE_DIRECTORY maintenance, which later deletes cookies 64 per turn. |
| (f) LOOKUP, base file | Read 2 | 1 | 1 / 1 | 26 | Visit 1 is undecided, 7 (W1, L2, I2, DE2). Visit 2, 19: `add_native_lookup` inserts `native_lookup`, `lease(9)`, `file_custody` (`native.rs:250-304`). |
| (f) LOOKUP, base directory | Read 1 if resident, else 2 | 1 | 0, else 1 / 0 | about 21 | Needs the child's inode leaf, metadata object and directory root page resident (`facts.rs:121`). Adds `set_native_parent`. |
| (g) GETATTR, base file, no carried facts | Read 2 | 0 | 1 / 1 | 10 | Same undecided-then-decided pair. A local file is 1 job, 5 statements (W1, L2, I2), which matches C02. |
| (h) UNLINK, local file | Mutation 1 | 1 | 0 | about 30, R2+2 | `ws/operations/namespace/remove.rs:27-64`, `ovl/namespace/compound.rs:130-223`. `detach_orphan` (`orphan.rs:46-97`) enqueues SERIAL_RETIRE, inserts the orphan row and a `gen=-1` inode row, enqueues ORPHAN. |
| (h) FORGET that follows | Lifecycle 1 | 1 | 0 | about 17, R2+1 | `native.rs:337-366`. Deletes `native_lookup` and `lease(9)`, drops the lookup count, re-enqueues ORPHAN, `wake_orphan` (`ovl/maintenance/orphan.rs:211-222`). |
| (h) maintenance after it | about 6 turns per file, each its own txn | 6 | – | – | `maintain_orphan` and `retire_serial` (`maintenance/orphan.rs:9-210`). C03 shows 6,012 maintenance jobs for 1,000 files. |
| (i) READLINK | same as READ | 4 | 2 for a base link | about 87 | `NativeJob::Source` instead of `HandleSource`. A base target is read twice (`view.rs:140`, then `read.rs:80`). |
| (j) FORGET / BATCH_FORGET | Lifecycle 1 **per unit** | 1 per unit | 0 | 9 partial, about 17 last | Each unit is admitted separately (`fuse/request/inline.rs:42-87`). The last forget of a never-unlinked inode also queues one maintenance turn just to delete its `file_custody` row (`maintenance/orphan.rs:11-20`). |

**Why a base-file LOOKUP needs two visits.** The memory-only client has no length provider (`ws/base/client.rs:48-55`, `:91-96`), so the in-job supply fails (`ws/operations/native_visit.rs:151-159`). The request then takes a reader (`lookup.rs:230-237`) and visits again.

**READDIR page, job by job** (`fuse/operations/directory.rs:196-311`, `ovl/lifetime/native_directory_read.rs`, `ovl/lifetime/native_cookie.rs`):
1. `Handle` (Lifecycle): W1, L2.
2. `Read` (Source): takes a source and inserts `native_directory_read`.
3. `Page` (Read): `SOURCE_NAMES`, then **per local entry** `source_inode` = W1 + L1 + I2 (`native_directory_read.rs:124-128`).
4. `LeaveReceiver`, reader grant, listing. For each base entry `base().inode(serial)` walks the inode table from the root (`ws/operations/native_directory.rs:64`).
5. `PrepareCookies` (Read): one SELECT per name.
6. `PublishCookies` (Read): one INSERT per name plus its trigger, then `NextTurn`.
7. `ReleaseBaseSource`.

One READDIR publishes at most 64 names (`ovl/contract/types.rs:8`), which is why C03 shows 17 READDIRs for 1,000 files.

### 1.4 C09 reconciliation

| Request | Count | Read | Lifecycle | Source | Grants | Length | Executions |
|---|---|---|---|---|---|---|---|
| LOOKUP `big` | 1 | 2 | 0 | 0 | 1 | 1 | 26 (W2, L8+3, I6, DE4, txn 3) |
| GETATTR root (resident) | 1 | 1 | 0 | 0 | 0 | 0 | 7 (W1, L2, I4) |
| OPEN | 1 | 2 | 2 | 1 | 1 | 1 | 86 (W12, L40+18, I4, txn 12) |
| READ | 512 | 1,536 | 1,024 | 512 | 1,024 | 512 | 44,544 |
| RELEASE | 1 | 0 | 1 | 0 | 0 | 0 | 17 |
| FLUSH | 1 | refused inline (`callbacks.rs:261-263`) | | | | | 0 |
| status `Command::State` (`dmn/control/status.rs:51`) | 1 | 0 | 1 | 0 | 0 | 0 | 1 (W1) |
| **Total** | | **1,541** | **1,028** | **513** | **1,026** | **514** | **44,681** |

By family: Lease 28,672 + 82 = 28,754; Workspace 7,168 + 19 = 7,187; Inode 2,560 + 14 = 2,574; Startup, Begin and Commit 2,048 + 6 = 2,054 each; DirectoryEntry 4. All match the receipt.

**Other receipts.**
- C05 Lifecycle is 932 in the find half by this trace (opendir 2 × 122, readdir 2 × 222, releasedir 2 × 122), not 892. With 1,000 RELEASEs and one status job it reproduces the receipt's 1,933.
- C03 Reclaim reproduces at 4,001 attempts and 7,002 executions (UNLINK 2+2, FORGET 2+1, one RELEASEDIR 1+1).
- The 2,000 GETATTRs in the rm half are 1,000 file stats (attributes dropped after WRITE) and 1,000 parent-directory refreshes. With `default_permissions` the kernel re-reads the parent's mode after every unlink. [I] That cannot be removed, only made cheap.

## 2. Minimal design

Shared principle, already used by LOOKUP, GETATTR and mutations: one owner job is the request's whole window in the owner, and install, revoke, close and reclaim are owner jobs too (`ovl/lifetime/native_visit.rs:1-5`). Per-request rows protected one thing only: keeping install behind the request (`base_readers`, `ws/mutation/facts.rs:18-19`). That is unnecessary for data:
- Base objects are immutable.
- The read path already reads from a non-current root (`ws/operations/file/read.rs:105-119`).
- An install changes no visible byte (`fuse/coherence/pages.rs:1-2`).

### 2.1 READ and READLINK as a read visit — largest win, medium risk

**P1. One read-only visit.** New job `ReadVisit{mount, serial, handle, offset, length}`. It checks the mount, the kernel reference (`visit_reference`, `native_visit.rs:27-57`) and the orphan row, then runs `LAYERS` and `CELL_RANGE`. It returns the composed local window plus the root to inherit from, or "no local row".
- **Fully local window:** reply from the job's own thread. No reader, no hand-off.
- **Inherited span:** `LeaveReceiver`, one reader grant, `read_range`, reply. Nothing afterwards.

| | Jobs | txn | Statements | Grants | Length batches | Hand-offs |
|---|---|---|---|---|---|---|
| Before | 6 | 4 | 87 | 2 | 1 | 2 |
| After, base bytes | 1 | 0 | about 5 | 1 | 0 | 1 |
| After, local bytes | 1 | 0 | about 5 | 0 | 0 | 0 |

- **Invariant kept:** local bytes are copied out inside the job, as `FileRead` does today.
- **Forced unmount and drain:** every request still holds a dispatch slot and passes the fence gate (`dmn/service/filesystem_port.rs:45-51`). `revoke_native_mount` already assumes a complete consumer drain (`native.rs:367-370`), exactly as it does for LOOKUP visits.
- **Cold-read failure:** `failed_base_read` is unchanged, and nothing is held to release.
- **READLINK** is the same visit with `offset = 0`.

**P2. Bounded base-fact cache**, beside the immutable cache.
- `(base_root, serial) → {kind, mode, mtime, links, content_root, length}`.
- `content_root → length`.
- Fixed capacity; eviction costs only a re-read.
- The length map is sound because length is a pure function of the content root (see §2.4).
- It removes the per-READ stat entirely and lets the worker skip the per-READ inode walk at `view.rs:159`.

**P3. Zero owner jobs.** A per-mount bounded entry `serial → (epoch, base_root, content_root, length)` asserts "no local inode row and no orphan for this serial". It is valid only while the Workspace mutation epoch is unchanged and the fence is not stopped.
- **Epoch:** one atomic per namespace, bumped by the owner inside every publishing job (`settle`, called at `compound.rs:222`) and by capture, install, close and revoke.
- **Who can invalidate:** write, truncate, unlink, rename-over, link, capture, install, revoke. All are owner jobs and all bump the epoch before their reply is sent.
- **Ordering:** a READ received after a mutation's reply sees the new epoch and takes P1. A READ racing an unreplied write may be ordered before it; the kernel's page lock already serialises the same page. [I]
- **Precedent:** reply tickets already live in engine memory (`ovl/database/connection.rs:31-35`).
- **Boundaries:** bounded cache, no per-file resident container. A mixed workload simply falls back to P1.
- **Risk:** medium. A missed epoch bump is the failure mode. Test it by bumping in one place and asserting every publishing path reaches it.

**Files:** `fuse/operations/read.rs`, `fuse/operations/lookup.rs`, `fuse/ports.rs`, `dmn/service/filesystem_port.rs`, `dmn/overlay/native_job.rs`, `ws/operations/native_visit.rs`, `ovl/lifetime/native_visit.rs`, `ovl/payload/stream.rs`, `ws/base/` (new cache).

### 2.2 OPEN and RELEASE

What depends on a descriptor row today:

| Behaviour | Depends on the row? | Already provided by |
|---|---|---|
| Content of an unlinked-but-open file is retained | No | Kernel lookup reference. An orphan is reclaimed only when opens + lookups + readers reach 0 (`file_owners.rs:42-45`), and the kernel cannot FORGET an inode with an open file. [I] `native_observation.rs:37-39` says the same. |
| **Addressing** an unlinked file for read, write or truncate | **Yes** | Nothing yet. The orphan domain is chosen only for descriptor-addressed changes (`ovl/namespace/inode.rs:113`, `compound.rs:159`); a serial-addressed read of an orphan file is `Missing` (`stream.rs:78-81`). Needs "kernel-referenced serial" addressing. |
| O_TRUNC | No | Arrives as SETATTR size 0 (`callbacks.rs:123-124`); atomic truncate is not negotiated (`fuse/mount/profile.rs:33-40`). |
| O_APPEND | No | Resolved by the kernel (`callbacks.rs:124`); no writeback cache. |
| Write permission of the descriptor | Engine check only (`file_owners.rs:160-162`, `compound.rs:153`) | The kernel refuses writes on a read-only descriptor and `default_permissions` checks mode at open. [I] The engine check becomes unreachable defence in depth. |
| Capture or commit waiting for writers | No | `capture_ready` looks only at reply tickets (`ovl/lifetime/generation.rs:120-130`). |
| Forced unmount custody | No | Revoke refuses only on `native_source` and `native_read` (`native.rs:374-377`); handles are retired by maintenance. Without rows there is nothing to retire. |
| CREATE returning an open descriptor (`ovl/lifetime/native_mutation.rs:80-97`) | Yes | Must be dropped together. Once the kernel has seen ENOSYS on OPEN, whether RELEASE still arrives for CREATE-opened files depends on the kernel version. [I, unverified for the VM] |

**P4. Stateless handles first.**
- OPEN becomes a read-only visit (reference plus kind check, or zero jobs under P3). It replies `fh = 0` and writes no row.
- RELEASE is answered inline with no job.
- READ, WRITE, GETATTR and SETATTR with `fh = 0` are routed by inode.
- This requires the orphan-addressing change in the table.

| | Jobs before | txn before | Statements before | Grants before | After |
|---|---|---|---|---|---|
| OPEN | 5 | 4 | 86 | 1 | 1 job (or 0), 0 txn, about 4 statements, 0 grants |
| RELEASE | 1 | 1 | 17 | 0 | 0 jobs |

**P5. Then answer OPEN with ENOSYS** when the kernel offers `FUSE_NO_OPEN_SUPPORT`. The flag exists in pinned fuser 0.18.0 (`src/ll/flags/init_flags.rs:44`). This removes the two round trips per file. [I] The kernel then uses keep-cache, which equals today's reply (`fuse/coherence/pages.rs:16-21`).

**Risk:** P4 medium-high, concentrated on unlinked-open semantics; it needs mounted tests for read, write and ftruncate after unlink and for rename-over. P5 is low once P4 holds, but confirm the VM kernel first.

**Fallback if P4 is refused:** OPEN as one visit inserting a single handle row (1 job, 1 txn, about 8 statements) and RELEASE as one delete.

### 2.3 READDIR

**P6. One visit per page.**
- The visit validates the handle, resolves the cursor, reads the local names **with kinds in the same query** (correlated subselect instead of `source_inode` per entry), merges base pages read from memory, and writes **one cookie row per page**.
- Cookie row: `(ns, directory, page_seq) → the page's names as one blob`. The cookie is `page_seq × 256 + index`. Resuming at any cookie is one point read, then "after name".
- A page starting at the same name with identical names reuses its row.
- If base pages are not resident, the visit is undecided and writes nothing. A worker reads the pages and resolves kinds with the batch `lookup_inodes` (`cnt/filesystem/read.rs:238-241`), then the request visits again.
- A locally created directory never needs the base (`created_above`, `ws/mutation/eval.rs:71`).

| | Jobs | txn | Statements (n = 64) | Grants |
|---|---|---|---|---|
| Before | 6 | 4 | about 520 | 1 |
| After | 1 (2 when cold) | 1 | about 8 | 0 (1 when cold) |

- **Invariant kept:** name-based resume, so concurrent unlink (`rm f*`) cannot skip entries. A rank-based offset would, so I rejected it.
- **Boundaries:** unlimited directory size; state is indexed rows; one row is at most a reply window.
- **Risk:** medium (seekdir and rewinddir; an install between the two visits re-reads, it never replays an attempted job).
- **Files:** `fuse/operations/directory.rs`, `fuse/request/directory.rs`, `ovl/lifetime/native_directory_read.rs`, `ovl/lifetime/native_cookie.rs`, `ovl/maintenance/native_directory.rs`, `schema.sql`, `ws/operations/native_directory.rs`.

**OPENDIR and RELEASEDIR:** single visits (4 jobs → 1, 2 jobs → 1), dropping the retained directory read.

**Not now:**
- **`FUSE_NO_OPENDIR_SUPPORT` and `FOPEN_CACHE_DIR`.** Cookies would have to live for the inode's lifetime instead of the handle's, with reclaim on FORGET and revoke. Do after P6, as the same row keyed by serial.
- **READDIRPLUS.** Every returned entry takes a kernel lookup reference (3 row writes today, plus a later FORGET) and needs mode, mtime and length. For `find` that is pure cost. The escape of sending an entry without attributes is not expressible in fuser 0.18.0, which fills both the node id and the dirent inode from `attr.ino` (`src/ll/reply.rs:521`, `:530`), and patching the library is not authorised. P8 gives the same benefit for `ls -l`, `git status` and `tar`.

### 2.4 LOOKUP and GETATTR of base files with zero reader grants

**Where the length is [S].**
- Not in the inode record: `InodeValue` is kind, namespace reference count, content root and metadata root (`cnt/object/inode_leaf.rs:84-93`).
- Not in the directory entry: a leaf row is name → serial (`cnt/filesystem/directory/codec.rs:19-23`).
- Mode and mtime are in the per-inode metadata object (`cnt/filesystem/attributes/portable.rs:17-24`).
- Length comes from the Store catalogue for a whole-file object, or from the small file-state object for a chunked file (`layerfs-storage/src/read/length.rs:27-66`). `FileView::logical_len` exposes it too (`cnt/file/view.rs:52-54`).

So a bounded `content_root → length` map is sound and needs no format change.

**P7. Use the P2 cache inside the visit.** With facts resident, a LOOKUP or GETATTR of a base file is 1 visit, 0 grants, 0 length batches.

**P8. Read before visiting, and read siblings ahead.**
- When the cache predicts a miss (parent is a base directory, child fact absent), go to a worker first, read the facts, then make one deciding visit. A cold LOOKUP drops from 2 jobs to 1 and loses the `NextTurn`.
- The worker reads facts for the whole directory leaf it just opened in batches: one inode wave, one batch for the metadata objects, one `file_lengths` batch.
- `find` never triggers it; `git status`, `tar` and `ls -l` hit the cache for the following entries.
- Speculative reads are immutable facts: wasted I/O at worst, never a wrong answer.
- Also stop reading the child directory's root page for LOOKUP and GETATTR (`facts.rs:121`); `entries` matters only to rmdir and to the parent update.
- **Risk:** low-medium.

**Statement diet for the visit (read side).** Keep the Workspace state and mount liveness in engine memory, and merge the two inode probes into one statement. GETATTR goes from 5 statements to 2. That is 2,000 GETATTRs in the rm half of C03.

### 2.5 FORGET

Today a write transaction is unavoidable: the lookup count is a row, plus `lease(9)` and `file_custody`.

**P9a. Finish reclamation inside the last FORGET.** When no reference remains, delete the orphan inode row, the orphan row, the custody row and up to 14 cells in the same job. Enqueue maintenance only when more remains.
- Unlink + forget goes from about 8 write transactions per file to 2.
- **Invariant kept:** bounded work per job.
- **Risk:** low-medium.

**P9b. One row per lookup reference.** Drop `lease(9)` and the `file_custody` counter for lookups; test existence on `native_lookup` instead. LOOKUP drops from 3 inserts + 3 triggers to 1 + 1.

**P9c. Bounded pending-lookup buffer in engine memory** — owner decision.
- Fixed capacity. Flushed in one transaction when full, and before any job that needs exact counts: unlink or rename-over of that serial, revoke, drain.
- A base-file LOOKUP becomes read-only, and a +1 / −1 pair cancels without SQL.
- It stays inside the boundary (bounded buffer, authoritative rows, disposable overlay that is never reopened).
- **Risk:** high (custody exactness).

**BATCH_FORGET.** One job per unit remains. The library delivers units as separate callbacks with no end-of-batch signal (`inline.rs:38-41`).

### 2.6 Kernel cache lifetimes

- Entry and attribute lifetime is 60 s (`fuse/request/reply.rs:18`).
- The product sends no kernel invalidation at all: no notify call exists in `layerfs-fuse` or `layerfs-daemon`. [S, grep]
- Correctness rests on "every change arrives as a request on this connection" (`pages.rs:8-10`) and on the kernel dropping what its own mutations change. [I]

**P10. Cache negative lookups.** Reply an entry with node id 0 and the same lifetime. The same argument covers it: only a create, link or rename through this mount can make the name appear, and the kernel instantiates the entry itself. Saves repeated misses for node module resolution, git and PATH probes.
- **Risk:** low, provided no non-FUSE mutator can act on a mounted Workspace. `Command::Namespace` exists (`dmn/overlay/commands.rs:181`); confirm it is never used on a mounted one.

**P11. `FUSE_CACHE_SYMLINKS`** (`init_flags.rs:56`). A symlink target never changes after creation (`ws/operations/namespace/create.rs:71`). One READLINK per inode lifetime. Risk: low.

**Optional later:**
- `FUSE_PARALLEL_DIROPS`, so git's parallel lstat is not serialised per directory.
- A larger read window (512 READs → 64 for C09). It is coupled to `max_write` and `WRITE_WINDOW` (`profile.rs:42-44`, `callbacks.rs:523`), so it is not free.

## 3. Directory link counts

**Where attributes come from [S].**
- **Reply:** `fuse/attributes.rs:46-53` projects 2 (or 0 once removed).
- **Local directory:** the inode row, created with `nlink: 1` (`create.rs:65`). `entries` is maintained by `touched()` on every create, remove and rename (`ws/mutation/eval.rs:121-138`).
- **Base directory:** `ws/mutation/facts.rs:118-135`. Links are the namespace reference count; `entries` comes from the directory root page (`ws/base/view.rs:116-122`).

**Canonical record [S].**
- A non-root directory's reference count must be exactly 1, and the root's 0 (`cnt/object/inode_leaf.rs:101-107`).
- Directory rows carry no kind (`directory/codec.rs:19-23`).
- So the base carries no link count and no child-directory count. Construction writes the row's `nlink` back as the reference count (`ws/construction/namespace/inodes.rs:100`), so that column must stay 1 for directories and cannot be reused.

**Design.**
1. **New `subdirs` column on `inode`** (zero unless kind is directory), carried through `INODE_PUT`, `INODE_LOOKUP`, `INODE_CAPTURE`, `decode` and the fold copy (`ovl/lifetime/composition.rs:135-199`). `Inode` gets the field. `touched()` takes a directory delta:
   - mkdir +1, rmdir −1;
   - a directory renamed across parents −1 and +1;
   - an empty directory replaced by rename −1.

   The parent row is already rewritten by each of these, so the cost is **zero extra statements**. The reply becomes `2 + subdirs`. GETATTR stays one row read, with no scan.
2. **Base value B.** It is a pure function of the directory's content root, because a serial's kind never changes. [I]
   - **Phase A, no cluster-one change.** A bounded `content_root → B` cache, filled when the directory's base fact is produced on a worker by listing leaves and resolving kinds with batch `lookup_inodes`. This is one scan per directory version per daemon, never per GETATTR and never in the owner.
   - **Phase B, exact with no scan at all — owner decision.** Persist B at Save as a Store-derived fact beside the length facts. Both producers already know it: Project Init sees kinds, and Commit has the captured row's `subdirs`. This touches cluster-one Storage and Persistence but not the canonical format.

**Lifecycle of one directory.**
- **Seen in base:** `2 + B`, from the cache or the derived fact.
- **First local change:** the parent row is written from the base fact through `touched()`, so the row holds the absolute `B ± delta`, as `entries` does today.
- **Captured:** the sealed row stays readable (`INODE_LOOKUP`, generation ≤ active). A later touch copies it up with its count.
- **Installed:** rows at or below `installed` are no longer read (`ovl/database/statements.rs:4`). The fact comes from the new root's content root: Phase A recomputes once; Phase B reads the value construction saved.

**Kernel coherence:** after mkdir, rmdir or rename the kernel drops the parent's attributes itself; the 1,000 parent GETATTRs in C03 show it. [I] No notification is needed.

**Risk:** low for the column, medium for Phase A's first-touch scan on very large directories.

## 4. Ranking and order

| Rank | Proposal | C09 | C03 | C05 | 103k-file read cells | Risk |
|---|---|---|---|---|---|---|
| 1 | P1 + P2 read visit and fact cache | 6 → 1 job, 4 → 0 txn, 87 → 5 stmts, 2 → 1 grant per READ; removes most of the 70 ms owner service | small | none | every READ of tar, cp, git grep, node | medium |
| 2 | P3 zero-job READ | 1 → 0 jobs per READ | none | none | same | medium |
| 3 | P7 + P8 base facts in one visit, sibling read-ahead | LOOKUP and OPEN only | none | small | git status, tar, ls -l: 2 jobs + grant → 1 job, 0 grants per file | low-medium |
| 4 | P4 (then P5) stateless handles | 6 jobs, 5 txn saved once | 1,000 RELEASE jobs and txns | 1,000 RELEASE | 6 jobs + 5 txn per file | medium-high |
| 5 | P6 READDIR visit + OPENDIR/RELEASEDIR visits | none | 17 pages: about 100 → 17 jobs, about 8k stmts | find half: about 1,860 → about 470 jobs, about 1,300 → about 220 txn | find, git status, tar traversal | medium |
| 6 | P9a + P9b forget and unlink | none | about 6,000 maintenance txn and much of the 62 ms queue wait | none | git checkout (rename-over, unlink) | low-medium |
| 7 | Statement diet for visits | small | 2,000 GETATTR × 3 stmts | about 1,100 GETATTR | every stat | low |
| 8 | P10 + P11 negative entries, cached symlinks | none | none | none | node, git | low |
| 9 | Directory link counts (§3) | – | – | verifier correctness | verifier correctness | low / medium |
| 10 | P9c lookup buffer; no-opendir; larger read window | – | – | – | further cuts | high |

**Order, each step independently testable.**
1. **Read visit and fact cache** (P1, P2), plus skipping the reader for fully local reads, plus READLINK. Gate: C09 at 1 Read job and 0 transactions per READ.
2. **Directory visits** (P6, OPENDIR, RELEASEDIR) and the **`subdirs` column**. Gate: C05 and the rm half of C03.
3. **Base facts** (P7, P8, P10, P11). Gate: LOOKUP at 1 visit, 0 grants when warm.
4. **Stateless handles** (P4), then P3, then P5 after checking the kernel. Gate: mounted unlinked-open tests.
5. **Forget and unlink reclamation** (P9a, P9b) and the visit statement diet. Gate: C03 maintenance jobs near 0.

**Not verified:**
- The VM kernel's RELEASE behaviour after an ENOSYS OPEN.
- Whether any non-FUSE mutator can act on a mounted Workspace.
- All time savings are estimates derived from job and statement counts, not measurements.