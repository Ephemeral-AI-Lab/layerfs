# Read-path cost at fdc24ef3f: exact traces, a minimal design, and directory link counts

> **Status:** Research; informative and not a product contract.

Read-only analysis by the same research subagent at commit `fdc24ef3f`, 2026-10-09 (final report; it overlaps the trace beside it and differs in some proposals). Its section 3 proposes a per-namespace table for base directory link counts; the fix that landed uses a bounded memo instead (see the second handoff).

The per-request model below reproduces the C09 receipt with zero residual in every family, so the trace is the real cost structure. A base READ today costs 6 owner jobs, 87 statement executions, 4 write transactions and 2 reader grants; the design gets it to 1 job, about 4 statements, 0 transactions and 1 grant. The Task 3 design was already sent to "main"; a short summary is in section 3.

Scope notes:
- Everything was read with `git show fdc24ef3f:`; nothing was edited, built or run. The working-tree SQL diet by the other agent is not reflected.
- Paths are under `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/crates/`. Abbreviations: `fuse/` = `layerfs-fuse/src/`, `daemon/` = `layerfs-daemon/src/`, `overlay/` = `layerfs-overlay/src/`, `ws/` = `layerfs-workspace/src/`, `content/` = `layerfs-content/src/`.
- [S] = read from source, [I] = inference.
- Per-job totals are exact because they reproduce the receipt. The split of a job's statements among the functions named for it is my reading of those functions, not a per-constant count.

## 1. Exact traces

### Counting rules that make the numbers close

- **Write transaction = 3 statements.** Each one costs 1 Startup (`PRAGMA main.freelist_count`), 1 Begin and 1 Commit (`overlay/database/connection.rs:206-297`) [S]. `atomic()` begins lazily at the first writing statement.
- **`executions` counts trigger runs.** `executions` is SQLITE_STMTSTATUS_RUN; `attempts` is direct statements (`overlay/diagnostics/metrics.rs:91-153`) [S]. Trigger and foreign-key action sub-programs also increment RUN [I]. The exact fit supports this: C09 Lease attempts are 21,049 and executions 28,754, a difference of 15 per READ.
- **Trigger sources.** These are the accounting triggers in `overlay/sql/accounting.sql` and the ON DELETE CASCADE actions, with `foreign_keys=ON` at `overlay/database/profile.rs:58-60` [S].
- **`inode_at` is 2 Inode statements.** It runs the orphan probe and then INODE_LOOKUP (`overlay/namespace/inode.rs:72-92`, `overlay/lifetime/orphan.rs:40-43`) [S].

### (b) READ of 128 KiB from a base file

Callback `fuse/request/callbacks.rs:137-167`; flow `fuse/operations/read.rs:35-91` and `fuse/operations/lookup.rs:240-310`. Statement columns: W = Workspace, L = Lease attempts + trigger runs, I = Inode, Txn = write transactions.

| # | Step | Class | Where | W | L | I | Txn |
|---|---|---|---|---|---|---|---|
| 1 | `HandleSource` | Source | submitted `lookup.rs:247-250` → `daemon/service/filesystem_port.rs:301-325`; performed `daemon/overlay/native_job.rs:142-239` → `overlay/lifetime/native_file.rs:11-37`, `native.rs:106-162` | 2 | 6+3 | 0 | 1 |
| 2 | `Observe` #1 | Read | `lookup.rs:266` → port `:351-362` → `overlay/lifetime/native_observation.rs:72-188`; returns `Need::Inode` | 1 | 4+0 | 2 | 0 |
| – | hand-off | – | `LeaveReceiver` `lookup.rs:290`; grant #1 `:291` (port `:340-342`, `:119-141`); `plan.supply` `:292` → `ws/mutation/facts.rs:112-136` → `ws/base/view.rs:130-149`, 1 length batch (`daemon/store/read_scope.rs:55-68`); `NextTurn` `:297` | | | | |
| 3 | `Observe` #2 | Read | decided UPDATE `native_observation.rs:107-112`; `retain_serial_read` `file_owners.rs:225-271`; INSERT `native_read` `:164-177` | 2 | 10+4 | 2 | 1 |
| 4 | `FileRead` | Read | `read.rs:74` → port `:255-278` → `daemon/overlay/commands.rs:293`; `overlay/payload/stream.rs:69-135`, `layers.rs:23-52` | 2 | 5+0 | 1 | 0 |
| – | hand-off | – | `LeaveReceiver` `read.rs:76`; grant #2 `:77`; `read_file_window` `:85` → `ws/operations/file/read.rs:57-147` → `ws/base/view.rs:151-170`; reply `fuse/request/reply.rs:86, 138-156` | | | | |
| 5 | `ReleaseFileRead` | Lifecycle | `lookup.rs:300-310` → port `:416-418` → `commands.rs:307`; `file_owners.rs:320-349` | 4 | 9+4 | 0 | 1 |
| 6 | `ReleaseBaseSource` | Lifecycle | port `:419-421`; `commands.rs:311-327`; `native.rs:195-228`, `source.rs:124-150` | 3 | 7+4 | 0 | 1 |

Per READ: 6 jobs (3 Read, 1 Source, 2 Lifecycle); W 14, Lease 56 (41 attempts + 15 trigger runs), Inode 5, 4 transactions (12 statements) = **87 executions**; 2 grants; 1 length batch; 2 `LeaveReceiver` and 1 `NextTurn`.

Cache-resident and non-resident reads differ only inside the two Store steps (object reads on the worker); the jobs and statements are identical. One reason is that the resident client has no length provider (`ws/base/client.rs:48-55`, `:91-96`), so a base regular file can never be decided without leaving the receiver [S].

### Other requests

| Request | Jobs, in order | Grants | Statement executions |
|---|---|---|---|
| (a) OPEN base file, read-only (`callbacks.rs:115-136`) | `Source` (Source), `Observe` ×2 (Read; the second runs `retain_file` + INSERT `native_file`, `native_observation.rs:126-148`), `ReleaseFileRead`, `ReleaseBaseSource` (Lifecycle) | 1, plus 1 length batch | W 12, L 40+18, I 4, 4 txns = 86 |
| (c) READ of local bytes | as (b), but one `Observe` decides (local row) and `FileRead` returns the bytes via LAYERS/CELL_RANGE: 5 jobs | 1: `read.rs:77` takes it unconditionally [I] | not measured |
| (d) RELEASE (`callbacks.rs:168-219`) | `CloseFile` (Lifecycle), `file_owners.rs:170-199`, `native_file.rs:92-101` | 0 | W 3, L 7+4, 1 txn = 17 |
| (e) OPENDIR (`:220-236`) | `Source`, `Observe` (+1 with a hand-off when base facts are needed), `release_read`, `release_source` | 0–1 | not measured |
| (e) READDIR, data page, max 64 entries (`fuse/request/directory.rs:18-75`, `fuse/operations/directory.rs:196-377`) | `Handle` (Lifecycle), `Read` (Source), `Page` (Read), hand-off with `native_directory_listing` (one `inode(serial)` read per base entry, `ws/operations/native_directory.rs:64`), `PrepareCookies` (Read, 1 SELECT per name, `overlay/lifetime/native_cookie.rs:11-66`), `PublishCookies` (Read, 1 INSERT + trigger per new name, `:70-96`), `NextTurn`, `release_source` (Lifecycle) | 1 | about 3 per entry for cookies, plus source rows |
| (e) READDIR, EOF page | `Handle`, `Read`, `Page`, `release_source` | 1 | – |
| (e) RELEASEDIR (`directory.rs:76-134`) | `Handle`, `Close` (Lifecycle); enqueues NATIVE_DIRECTORY | 0 | – |
| (f) LOOKUP of a base file or directory (`callbacks.rs:48-79`, `lookup.rs:191-239`) | `ObserveVisit` ×2 (Read) with `LeaveReceiver :230`, `base() :231`, `supply :232`, `NextTurn :237` | 1, plus 1 length batch for a file | W 2, L 8+3, I 6, DirectoryEntry 4, 1 txn (INSERT `native_lookup`, `native.rs:250-283`) = 26 |
| (g) GETATTR (`:83-98`) | 1 `ObserveVisit` when facts are resident (root in C09); otherwise 2 visits + 1 grant | 0–1 | W 1, L 2, I 4 = 7 |
| (h) UNLINK of a local file (`:403-414`, `fuse/operations/mutation.rs:203-285`) | 1 `MutateVisit` (Mutation): `apply_checked` `overlay/namespace/compound.rs:130-223` → `detach_orphan` `orphan.rs:46-97` (enqueue SERIAL_RETIRE, INSERT orphan, INODE_PUT at domain −1, enqueue ORPHAN) | 0 | 1 txn |
| (i) READLINK (`:99-114`) | as (b) with `Source` in place of `HandleSource` and `readlink_window` (`read.rs:80`) | 2 | about 87 |
| (j) FORGET / BATCH_FORGET (`fuse/request/inline.rs:42-87`) | one Lifecycle job **per inode** even in a batch; `forget_native` `native.rs:337-366`: delete `native_lookup` + lease, FILE_LOOKUPS_DROP, FILE_REFS, enqueue ORPHAN + `wake_orphan` (`file_owners.rs:20-48`), `queue_closed` | 0 | 1 txn, then maintenance |

After (h) + (j), each unlinked file takes about 6 maintenance write transactions (`overlay/maintenance/orphan.rs:9-210`, `ready.rs:147-206`). C03 shows `maintenance_jobs` 6,012 for 1,000 files, and Reclaim 4,001 attempts / 7,002 executions.

### C09 reconciliation: (f) + (g) + (a) + 512 × (b) + (d) + status

| Quantity | Sum | Model | Receipt |
|---|---|---|---|
| Read jobs | 512×3 + 2 (OPEN) + 2 (LOOKUP) + 1 (GETATTR) | 1,541 | 1,541 |
| Source jobs | 512 + 1 | 513 | 513 |
| Lifecycle jobs | 512×2 + 2 + 1 (RELEASE) + 1 (status `Command::State`, `daemon/control/status.rs:51`) | 1,028 | 1,028 |
| Owner jobs | | 3,082 | 3,082 |
| Reader grants | 1,024 + 1 + 1 | 1,026 | 1,026 |
| Length batches | 512 + 1 + 1 | 514 | 514 |
| Workspace | 7,168 + 12 + 3 + 2 + 1 + 1 | 7,187 | 7,187 |
| Lease | 28,672 + 58 + 11 + 11 + 2 | 28,754 | 28,754 |
| Inode | 2,560 + 4 + 6 + 4 | 2,574 | 2,574 |
| Startup / Begin / Commit, each | 2,048 + 4 + 1 + 1 | 2,054 | 2,054 |
| DirectoryEntry | LOOKUP only | 4 | 4 |
| Total executions | | 44,681 | 44,681 |

Cross-check on C05: Lifecycle 1,933 = 1,000 (RELEASE) + 932 (OPENDIR 2×122 + READDIR 2×222 + RELEASEDIR 2×122) + 1 status. The task's "+892" is therefore approximate.

## 2. Minimal design

### 2.1 READ of base bytes: 6 jobs → 1

- **One read-only visit.** A `ReadVisit(handle, offset, length)` job checks the `native_file` row and mount, runs `inode_at`, and runs LAYERS/CELL_RANGE only when a local inode row exists. Local bytes are copied inside the job. It returns either the local data or "base at (base root, serial)" under the existing class-3 visit source (`overlay/lifetime/source.rs:79-103`).
- **One hand-off.** One `LeaveReceiver`, one reader grant, `read_range` against the cached `content_root`, then the reply. There is no post-reply job. The grant is skipped when there is no inherited span.
- **Before → after per READ.** Jobs 6 → 1; executions 87 → about 4 (0 Lease writes, 0 triggers); transactions 4 → 0; grants 2 → 1; length batches 1 → 0; hand-offs 3 → 1.
- **C09 after.** About 518 jobs (3,082 today), about 2,200 statements (44,681), 513 grants (1,026), 1 length batch (514).

What the removed steps protected, and why it still holds:

| Removed | Invariant | What still enforces it |
|---|---|---|
| `native_source` + `base_source` + lease rows | bytes come from the root the view was decided on | Store objects are immutable and the `content_root` is captured in the visit; the visit source guard `source.rs:85-90` rejects a decision made against a replaced base [S] |
| `native_read` / `file_read` rows | an orphan's local content survives the read | local bytes are copied inside the single job, so nothing spans the hand-off [I] |
| the same rows | drain and forced unmount account for the request | `revoke_native_mount` refuses while those rows exist (`native.rs:371-382`) [S]. After the change this must rest on the fuse fence alone (`fuse/request/state.rs:58-89, 144-185`). **Verify this before removing the rows; it is the main risk.** |
| `stat` + length batch per READ | read clamps at file length | length comes from the handle's base fact (2.2) |

- **Files.** `fuse/operations/read.rs`, `lookup.rs`; `daemon/service/filesystem_port.rs`, `daemon/overlay/commands.rs`, `native_job.rs`; `overlay/lifetime/native_file.rs`, `file_owners.rs`, `payload/stream.rs`; `ws/operations/native_read.rs`, `file/read.rs`.
- **Risk.** Medium.
- **Boundaries.** No resident growth, no retry, and a cold-read failure remains EIO for that request only.

Three smaller wins on the same path:
- `read.rs:85` clones the 128 KiB `LocalRead`.
- `ws/base/cache.rs:62-79` copies a `Vec` on every cache hit, and `ws/base/view.rs:76-78` re-reads the root object on every `reader()` call (`content/filesystem/read.rs:82-92`). Return shared buffers and keep one decoded root per base root.
- `fuse/mount/profile.rs:31-65` sets `max_background(1)`, which serialises kernel read-ahead [S].

**Owner decision: READ_WINDOW.** It is 128 KiB (`overlay/contract/types.rs:12-14`) while FUSE_MAX_PAGES is already negotiated. A 1 MiB read window would turn 512 READs into 64. This is the largest single lever for large sequential reads, at the cost of 1 MiB per in-flight request.

**Zero-job READ.** This is possible with a bounded memory cache of "serial has no local row", stamped by a namespace mutation counter. I do not recommend it yet: it puts request accounting entirely outside the owner.

### 2.2 LOOKUP/GETATTR of base files: one visit, zero grants when resident

- **Where length comes from.** It comes from Storage, not the inode record: for WholeFile, canonical length minus overhead from the catalogue; for FileState, a small state object (`layerfs-storage/src/read/length.rs:9-68`) [S].
- **Change.** Add a bounded LRU in the base client holding the base file fact, including `content_root` and length. Give `resident()` a length provider that reads only that cache. Carry the fact from LOOKUP into OPEN and READ.
- **Before → after.** First touch stays at 2 visits + 1 grant; the length batch moves into that same grant. Every later LOOKUP, GETATTR, OPEN and READ of the same file goes from 2 visits + 1 grant + 1 length batch to 1 visit with 0 grants.
- **Invariant.** A fact is a pure function of (base root, serial), and objects are immutable; the visit source guard covers an install.
- **GETATTR statements.** Merging the orphan probe into INODE_LOOKUP and caching workspace/mount liveness in engine memory (invalidated by the same transactions that change them) takes GETATTR from 7 to about 2. This matters because the kernel re-fetches the parent's attributes after every create and unlink under `default_permissions` (about 3 GETATTRs per file on C03) [I].
- **Files.** `ws/base/client.rs`, `cache.rs`, `view.rs`; `ws/mutation/facts.rs`; `overlay/namespace/inode.rs`.
- **Risk.** Low.

### 2.3 OPEN and RELEASE

**Recommended first: OPEN as one visit.** Use the visit source and insert the descriptor rows in one transaction, with the kind taken from the cached fact. That is 5 jobs / 86 statements / 4 transactions → 1 job / about 14 statements / 1 transaction. RELEASE is already 1 job. Risk: low.

**FUSE_NO_OPEN_SUPPORT** (reply ENOSYS only when the kernel offers the flag). What depends on a descriptor row today:

| Behaviour | Covered without the row? |
|---|---|
| unlinked-but-open file keeps its content | Yes. The kernel lookup reference retains it, as `native_observation.rs:37-39` and `ws/operations/native_read.rs:188-190` state [S] |
| writes/truncates to an unlinked file go to the orphan domain | **No.** Routed only when descriptor-addressed: `owned = changes.open.is_some()`, `compound.rs:157-162`, `inode.rs:113` [S] |
| reads of an unlinked file | **No.** `read_layers` returns Missing for nlink 0 unless descriptor-addressed (`stream.rs:128-133`, `orphan.rs:136-149`) [S] |
| size change "only through the descriptor" (`callbacks.rs:346-348`) | **No.** Needs an inode-addressed path |
| descriptor write-permission check (`compound.rs:153`) | Yes, by the VFS under `default_permissions`; this one is defence in depth |
| O_TRUNC / O_APPEND | Yes. Size-0 SETATTR and kernel-resolved offset (`callbacks.rs:115-136`) [S] |
| Commit | Yes. `capture_ready` ignores handles (`overlay/lifetime/generation.rs:120-130`) [S] |
| unmount / drain | Yes. Revoke checks only source/read rows (`native.rs:371-382`) [S] |
| keep-cache open flag (`fuse/coherence/pages.rs`) | Yes. The kernel applies it by default under no-open [I] |
| CREATE | Still returns a handle and receives RELEASE [I]; it may keep its descriptor row |

Verdict: feasible, but the three "No" rows are engine work at medium-high risk. On C09 it saves under 0.3%; it pays only on cells that open many small files. Do it after 2.1–2.2.

### 2.4 READDIR

- **Cookies: one row per page.** Replace the per-name SELECT and INSERT with one row per page holding that page's names; cookie = page number and index. Cookies must stay resolvable per entry, because the kernel can resume from the middle of a page [I].
- **Kinds: one batch.** Resolve kinds with `lookup_inodes` (`content/filesystem/read.rs:238-241`) instead of 64 single reads.
- **Merge jobs.** Merge `Handle`, `Read` and `Page` into one visit, and publish in a second.
- **Before → after per data page.** Jobs 6 → 2; about 190 statements → about 6; grant 1 → 1.
- **OPENDIR and RELEASEDIR.** OPENDIR as one visit, 4–5 jobs → 1. RELEASEDIR 2 jobs → 1.
- **C05.** Directory traffic drops from about 1,900 jobs to about 690.
- **Invariant.** Cookie stability within a handle; the page row stores the names actually returned.
- **Risk.** Low-medium.
- **One visit per page with no rows.** This is possible only for a directory with no local entries, using the canonical ordinal as the cookie (branches carry `subtree_count`, `content/filesystem/directory/codec.rs:25-34`). I found no seek-by-ordinal reader, so it would need new cluster-one read code. Defer.
- **FUSE_NO_OPENDIR_SUPPORT / FOPEN_CACHE_DIR.** Both need inode-scoped stable cookies; today they are handle-scoped. Defer.
- **READDIRPLUS: not now.** fuser 0.18.0 uses `attr.ino` for both the node id and the dirent inode (`ll/reply.rs:517-530`), so an entry without a lookup reference cannot be expressed without an unauthorised patch. Every entry would also cost a lookup row and a file length.

### 2.5 FORGET and unlink

FORGET needs a write transaction even for an inode with no local row, because the kernel reference count is a `native_lookup` row. It must stay a row: a per-inode count in memory would grow with the workload.

Cheaper within that constraint:
1. **One job per batch.** BATCH_FORGET becomes one job and one transaction instead of one per inode (`inline.rs:42-87`).
2. **Skip the maintenance item for plain base inodes.** When no orphan row or payload exists, delete the custody row inline.
3. **Reclaim small orphans inline.** For an unlinked local file, run the first bounded reclamation step (at most 14 cells) inside the FORGET transaction, calling the same checks as `maintain_orphan`. About 6 maintenance transactions per file → 0–1. On C03 this removes roughly 5,000 transactions and their Reclaim statements.

Risk: medium. `orphan_holds` and capture holds must be reused, not reimplemented. FORGET is not sent at unmount [I]; namespace-close reclamation already covers that.

### 2.6 Kernel cache lifetimes

- **TTL is 60 s** (`fuse/request/reply.rs:18`). The product sends no kernel notifications [S, grep]. The stated basis is that every change arrives on this connection and an install changes nothing visible (`fuse/coherence/pages.rs:1-21`) [S].
- **Longer TTL.** Under that same basis a much longer TTL is safe. It helps only tasks longer than 60 s.
- **Negative lookups.** A refusal is replied as an error (`reply.rs:72`), so ENOENT is not cached and every probe of a missing name is a full LOOKUP. Replying with a zero-node entry plus TTL would cache it.
- **Blocking check.** Negative caching is safe only if no control-plane path can create a name in a mounted view outside the kernel. The workspace crate has a non-native mutation API, and I did not confirm it is unreachable for a mounted Workspace.
- **Value.** High on build/git-style cells, none on C09, C03 or C05.

## 3. Directory link counts (summary of what was sent)

- **Today.** `fuse/attributes.rs:46-56` returns 2 for every live directory and 0 for a removed one. The canonical record carries neither a link count nor a child-directory count, and directory entries carry no kind (`content/object/inode_leaf.rs:83-107`, `content/filesystem/directory/codec.rs:16-35`) [S].
- **Local rows.** Add an absolute `subdirs` column to the overlay `inode` row, maintained in `ws/mutation/eval.rs:121-138` (`touched`). The parent row is already written in the same transaction, so this costs 0 extra statements.
- **Base directories.** Add one indexed row per (namespace, base directory) in a new table. It is filled once by an off-owner windowed scan, triggered by a new need, and inserted in the LOOKUP's existing transaction.
- **Capture/install.** Read the hidden retired row if present, otherwise the table. `retire_generation` phase 3 (`overlay/maintenance/garbage.rs:78-102`) carries the value into the table before deleting the row.
- **Reporting.** 2 + count; a removed directory stays 0.
- **Tests.** Only `layerfs-fuse/tests/attributes.rs:55-58` pins 2.
- **Cost.** One listing plus kind lookups per base directory per namespace, at its first LOOKUP. Removing that scan needs a Store-side derived count, which is an owner decision.

## 4. Ranking and implementation order

| Rank | Change | C09 | C03 | C05 | Read-heavy cells | Risk |
|---|---|---|---|---|---|---|
| 1 | 2.1 READ as one visit | about −95% statements, −83% jobs, −50% grants | – | – | very high | medium |
| 2 | 2.2 base-fact + length cache; cheaper GETATTR | small on its own | medium (3 GETATTRs per file) | medium | high for many small files | low |
| 3 | 2.3 OPEN as one visit | negligible | – | small | high for many small files | low |
| 4 | 2.5 FORGET batch + inline reclaim | – | high | small | – | medium |
| 5 | 2.4 READDIR page rows + batched kinds | – | – | high | medium (tree walks) | low-medium |
| 6 | READ_WINDOW 1 MiB (owner decision) | −87% requests | – | – | very high for large files | low-medium |
| 7 | negative lookup caching, longer TTL | – | – | – | high on probe-heavy cells | medium |
| 8 | no-open, no-opendir, zero-job READ | <0.3% | – | small | medium | high |

Steps, each independently testable:

1. **Directory link counts (section 3).** It is a verifier correctness failure and touches the schema once. Test: mounted `st_nlink` equals 2 + subdirectories before and after capture + install; a removed directory stays 0.
2. **Base-fact and length cache, shared buffers, GETATTR statement merge (2.2).** Test: a second LOOKUP/GETATTR of a base file is 1 visit with 0 grants and 0 length batches; a failed cold read is EIO for that request only.
3. **READ as one visit, then OPEN as one visit (2.1, 2.3).** Test: C09 counters show about 1 job and 1 grant per READ, 0 Lease writes and 0 transactions on READ; forced unmount with a parked READ still accounts for it; an unlinked-open file still reads.
4. **FORGET batch + inline bounded reclaim (2.5).** Test: C03 maintenance transactions per unlinked file drop from about 6 to at most 1; orphan and capture-hold tests are unchanged.
5. **READDIR page rows, batched kinds, single-visit OPENDIR/RELEASEDIR (2.4).** Test: a resume from a mid-page offset returns no duplicate and no skipped name under concurrent create/unlink; C05 directory jobs are about 2 per page.

Not verified:
- whether the fuse fence alone accounts for an in-flight READ at revoke (step 3);
- whether a non-kernel mutator can reach a mounted view (negative caching);
- how path `truncate` without a descriptor is handled today (no-open);
- kernel behaviours marked [I].