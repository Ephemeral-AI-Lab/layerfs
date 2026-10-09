# C12, C05, C04 at 63c48d8dc: request mix, exact owner model, and single-visit forms for OPEN / READ / directory requests

> **Status:** Research; informative and not a product contract. Read-only analysis by a research subagent at commit `63c48d8dc`, 2026-10-09.

> Status: research, read-only. Nothing was built, run or edited. Product source read only with
> `git show 63c48d8dc:<path>` (snapshot copies in `research/dir-git/src/`). Receipts decoded with
> `core/target/r7-summary.py`, `r7-statements.py` and `research/dir-git/dump.py`.
> Labels: [S] source at 63c48d8dc with file:line, [R] decoded from the named receipt, [I] inference,
> [E] estimate. Kernel statements are recollection of Linux `fs/fuse` (6.x) and are all [I].

Path prefixes: `F/` = `core/crates/layerfs-fuse/src/`, `D/` = `core/crates/layerfs-daemon/src/`,
`O/` = `core/crates/layerfs-overlay/src/`, `W/` = `core/crates/layerfs-workspace/src/`,
`V/` = `core/vendor/fuser-0.18.0/src/`.

Receipts used (all class B, arm L, one sample, DIAGNOSTIC / verifier PASS): C12 = 703, C05 = 675, C04 = 671
(counts identical to 599 / 595 / 591 [R]); cross-checks on C01 = 659, C02 = 663, C09 = 691.

---

## 0. Headline

1. **The job model is exact.** Jobs by class, reader grants, write transactions, Workspace, Lease and Reclaim
   statements reproduce 671 and 675 with zero residual, and 703 with zero residual in jobs/grants and a
   residual of at most ~14 statements that depends on one unknown (how many of C12's 785 mutations published
   nothing). Section 2.
2. **Two request paths were never converted to owner visits** (ledger step 6 converted LOOKUP, GETATTR and every
   mutation): (a) OPEN / OPENDIR / READ / READLINK still run *source job -> observe job -> [file-read job] ->
   two post-reply release jobs*; (b) READDIR runs *six* jobs per data page and four per end page, RELEASEDIR two.
   An OPEN of a local file costs 4 jobs, 58 statement attempts, 4 write transactions, about 169 µs of owner
   service; a READ of 2 local bytes costs 5 jobs, 64 attempts, 4 transactions, 1 Store reader grant and one
   thread hand-off, about 189 µs; a READDIR data page of 10 names costs 6 jobs, 114 attempts, 4 transactions,
   1 grant, about 246 µs.
3. **C12 (317.7 ms, target 189.3):** OPEN + READ are 59.6 ms of the 133.7 ms owner service (45 %) and 71 % of
   all Lease statements. One-visit OPEN and one-visit local READ remove about 51 ms of service
   (about 46 ms of command time [E]). That is the largest thing in this report's subject, and it is not enough:
   178.6 ms of C12 is outside the owner (52 µs for each of 3436 requests), which alone is 94 % of the target.
   The remaining levers are request elimination (`FUSE_NO_OPEN_SUPPORT`: -458 requests, an owner decision) and
   the per-request floor (another agent's subject).
4. **C05 (880.5 ms, target 949.6):** `find` adds 547 requests that cost 57.7 ms of service, 11.6 ms of owner
   wait, 222 grants + hand-offs and 223 maintenance jobs (8.2 ms) for cookie retirement. One visit per request
   with one cookie row per page brings that to about 13 ms: -55 to -62 ms [E].
5. **C04 (823.4 ms, target 952.6):** nothing of this subject is left in it (10 OPENDIR/RELEASEDIR, about 1 ms).
   47 % of C04 is process creation (about 390 ms for 1100 processes [E]), 32 % the per-request floor, 21 %
   owner service of requests that are already one visit.
6. **New facts for the plan:** fuser exposes `batch_forget` as an overridable trait method (`V/lib.rs:430-436`
   [S]); the product's statement that a batch arrives only as repeated callbacks (`F/request/inline.rs:38-41`
   [S]) holds only for the default implementation. `ReplyEntry::entry` takes the node id from `attr.ino`
   (`V/reply.rs:236-244` [S]), so a negative entry (node id 0 with a lifetime) needs no library change. The
   READDIR reply capacity is *not* exposed to the callback (`V/request.rs:299-310` [S]), which decides how
   cookies can be published.

---

## 1. Counting rules (all [S])

- **A write transaction is 3 statements:** `PRAGMA main.freelist_count` [Startup], `BEGIN IMMEDIATE` [Begin],
  `COMMIT` [Commit]. A job begins lazily at its first writing statement (`O/database/connection.rs:211-290`),
  so a job that only reads has no transaction.
- **`attempts` are statements issued; `executions` add one per trigger or cascade program** (accounting
  triggers `core/crates/layerfs-overlay/sql/accounting.sql`, FK cascades `sql/schema.sql:236-237,245,268`).
  This report counts attempts. Families: W = Workspace, L = Lease, I = Inode, D = DirectoryEntry, P = Payload,
  R = Reclaim, T = one transaction (3 statements).
- **Classes** (`D/overlay/commands.rs:279-350`, `D/overlay/native_job.rs:98-109`,
  `D/overlay/native_directory_job.rs:64-72`): Source = `Source`, `HandleSource`, `FileSource`, `OpenSource`,
  directory `Read`; Read = `Observe`, `ObserveVisit`, `FileRead`, directory `Page`, `PrepareCookies`,
  `PublishCookies`; Mutation = `MutateVisit`, `Mutate`; Lifecycle = everything else (`CloseFile`, `Forget`,
  directory `Handle` and `Close`, `ReleaseFileRead`, `ReleaseBaseSource`, `State`).
- Helper costs used below: `state`/`live` = W1 (`O/lifetime/workspace.rs:52-72`); `native_mount_state` = L1
  (`O/lifetime/native.rs:49-71`); `check_native_mount` = `check_native_attached` = W1+L1 (`native.rs:74-88`);
  `source_state` for a recorded source = W1+L1 (`O/lifetime/source.rs:74-103`); `retain_native_source` =
  L4+W1 (`native.rs:106-162`); `retain_serial_read` = L4+W1 (`O/lifetime/file_owners.rs:255-301`);
  `retain_file` = L3 (`file_owners.rs:94-134`); a visit's fence = W1 (`O/lifetime/native_visit.rs:40-94`,
  statements `O/database/statements.rs:150-172`); `inode_at` = I1, plus one orphan probe once this engine has
  created an orphan (`O/namespace/inode.rs:74-94`, `O/lifetime/orphan.rs:42-48`).

---

## 2. The exact model at 63c48d8dc

### 2.1 Per-opcode table

Hand-offs: LR = `LeaveReceiver` (the request leaves the receive loop for a pool worker), NT = `NextTurn`.
"One visit" = decided in a single owner job that records no request source (ledger step 6).

| Opcode | Owner jobs in order (class) | Statements (attempts) | T | Grants | Hand-offs | One visit? |
|---|---|---|---|---|---|---|
| LOOKUP negative, local parent | `ObserveVisit` (Read) | W1 I1 D1 = 3 | 0 | 0 | 0 | yes |
| LOOKUP negative, base parent, facts resident | `ObserveVisit` (Read), two in-job rounds | W1 I2 D2 = 5 | 0 | 0 | 0 | yes |
| LOOKUP positive, local child | `ObserveVisit` (Read) | W1 I2 D1 L2-4 (+1 for a directory) | 1 | 0 | 0 | yes |
| LOOKUP/GETATTR of a base regular file | `ObserveVisit` x2 (Read) | first is undecided | 0-1 | 1 + 1 length batch | LR, NT | **no**: length has no memory-only provider |
| GETATTR (local or resident base) | `ObserveVisit` (Read) | W1 I1 = 2 | 0 | 0 | 0 | yes |
| SETATTR (utimens / chmod / truncate) | `MutateVisit` (Mutation) | W2 I3 = 5 | 1 | 0 | 0 | yes |
| CREATE (local parent) | `MutateVisit` | W2 I7 D4 L6 = 19 | 1 | 0 | 0 | yes |
| MKDIR (local parent) | `MutateVisit` | W2 I7 D4 L4 = 17 | 1 | 0 | 0 | yes |
| WRITE (one cell) | `MutateVisit` | W2 I3 P2 = 7 | 1 | 0 | 0 | yes |
| UNLINK / RENAME / LINK / SYMLINK | `MutateVisit` | not traced here | 1 | 0 | 0 | yes |
| RELEASE | `CloseFile` (Lifecycle) | W1 L3 = 4 | 1 | 0 | 0 | yes |
| **OPEN**, local file | `Source` (Source), `Observe` (Read), reply, `ReleaseFileRead`, `ReleaseBaseSource` (Lifecycle) | W11 L34 I1 = 46 | 4 | 0 | 0 | **no** |
| **OPEN**, base file | as above with a second `Observe` | W12 L38 I2 | 4 | 1 + 1 length batch | LR, NT | **no** |
| **READ**, local bytes | `HandleSource` (Source), `Observe` (Read), `FileRead` (Read), reply, `ReleaseFileRead`, `ReleaseBaseSource` | W13 L35 I3 P1 = 52 | 4 | **1, unconditional** | LR | **no** |
| READ, base bytes (C09) | as above with a second `Observe` | W14 L39 I3 | 4 | 2 + 1 length batch | LR x2, NT | **no** (another agent) |
| READLINK | as READ with `Source` | about as READ | 4 | 1-2 | LR | **no** |
| **OPENDIR** | `Source`, `Observe`, reply, `ReleaseFileRead`, `ReleaseBaseSource` | W11 L34 I1 = 46 | 4 | 0 | 0 | **no** |
| **READDIR**, data page, N local names, offset 0 | `Handle` (Lifecycle), `Read` (Source), `Page` (Read), `PrepareCookies` (Read), `PublishCookies` (Read), reply, `ReleaseBaseSource` | W15+N, L35+3N, I1+N, D1 = 52+5N | 4 | **1, unconditional** | LR, NT | **no** |
| **READDIR**, end page | `Handle`, `Read`, `Page`, reply, `ReleaseBaseSource` | W11 L25 I1 D1 = 38 | 2 | 1 | LR | **no** |
| **RELEASEDIR** | `Handle`, `Close` (Lifecycle) | W3 L8 R1 = 12 | 1 | 0 | 0 | **no** (2 jobs) |
| FORGET | `Forget` (Lifecycle), one per inode even inside a batch | W2 L5 (+R2 when the last reference goes) | 1 | 0 | 0 | yes, unbatched |
| FLUSH / FSYNC / FSYNCDIR | none: `ENOSYS`, sticky | 0 | 0 | 0 | 0 | – |
| STATFS | none: inline | 0 | 0 | 0 | 0 | – |
| ACCESS, xattr family, READDIRPLUS, locks, LSEEK, FALLOCATE, COPY_FILE_RANGE, IOCTL, POLL, BMAP | none: `ENOSYS` | 0 | 0 | 0 | 0 | – |

Sources for the rows that are this report's subject:

- **OPEN** `F/request/callbacks.rs:115-136` -> `F/request/state.rs:144-185` -> `F/request/reply.rs:48-130`
  -> `F/operations/lookup.rs:240-299` (source at `:247-250`, `observe` at `:266`), release at `:300-310`
  [S]. Jobs: `Source` = `D/service/filesystem_port.rs:301-325` -> `O/lifetime/native.rs:91-104`:
  `check_native_mount` (W1 L1) + `native_lookup_row` (L1) + `retain_native_source` (L4 W1) = W2 L6, T.
  `Observe` = `filesystem_port.rs:351-362` -> `W/operations/native_read.rs:105-124` ->
  `O/lifetime/native_observation.rs:72-188`: `check_native_source` (W1 L3, `native.rs:178-194`), SELECT
  `native_read` (L1, `:88-94`), decision (I1), UPDATE decided (L1, `:107-112`), `retain_file` + INSERT
  `native_file` (L4, `:126-148`), `retain_serial_read` + INSERT `native_read` (L5 W1, `:164-177`) = W2 L14 I1,
  T. `ReleaseFileRead` = `file_owners.rs:350-379` + `source.rs:124-150` = W4 L8, T. `ReleaseBaseSource` =
  `source.rs:112-150` + `native.rs:195-228` = W3 L6, T. **The read retained by `Observe` protects no byte of
  an OPEN reply**; it exists because `read = true` for every non-attribute observation
  (`native_observation.rs:40-55, 158-177`).
- **READ** `callbacks.rs:137-167`; `lookup.rs:240-299`; then `F/operations/read.rs:64-91`: `local_read`
  (`:74`), `LeaveReceiver` (`:76`), reader grant via `immutable` (`:77`, `filesystem_port.rs:340-342,
  119-141`) **before** looking at whether the window has an inherited span, `read_file_window` (`:85`) [S].
  `FileRead` job = `O/lifetime/orphan.rs:144-157` + `O/payload/stream.rs:69-135` + `O/payload/layers.rs:23-52`
  = W2 L5 I1 P1. A window with no inherited span never touches the base
  (`W/operations/file/read.rs:98-147`: only `local.span` reads it) [S].
- **OPENDIR** `callbacks.rs:220-236`; `Observe` -> `observe_native_directory`
  (`native_observation.rs:57-71, 149-157`) -> `retain_native_directory` (`O/lifetime/native_directory.rs:31-68`,
  L4) [S].
- **READDIR** `callbacks.rs:237-246` -> `F/request/directory.rs:18-75, 136-220` ->
  `F/operations/directory.rs:196-217` (`Handle`, `Read`), `:243-311` (`Page`, LR at `:263`, grant at `:264`,
  `PrepareCookies` at `:294`), `:344-378` (`PublishCookies`, NT at `:372`), `:317-332` (release) [S].
  Engine: `Handle` = `native_directory.rs:70-81` (W1 L2); `Read` = `O/lifetime/native_directory_read.rs:34-51`
  (W3 L9, +L1 for a cookie offset `:8-33`); `Page` = `:99-135`: `check_native_directory_read` (W2 L3, `:79-96`)
  + `source_directory_entry_window` (`O/namespace/directory_entry.rs:19-40`: `source_state` W1 L1, parent
  inode I1, `source_directory_entries` `:86-108` W1 L1 D1) + **one `source_inode` per local name** (W1 L1 I1
  each, `native_directory_read.rs:124-128`) = W4+N L5+N I1+N D1; `PrepareCookies` =
  `O/lifetime/native_cookie.rs:11-66`: check (W2 L3) + 2 SELECT + **one SELECT per name** + 2 UPDATE =
  W2 L7+N, T; `PublishCookies` = `:70-96`: check + SELECT + **one INSERT per name** + UPDATE = W2 L5+N, T;
  release = W3 L7, T (`native.rs:224-226` adds `queue_native_directory`).
- **RELEASEDIR** `F/request/directory.rs:76-134` (`Handle` at `:100`, `Close` at `:103`);
  `native_directory.rs:111-147` [S].
- **FORGET** `callbacks.rs:80-82` -> `F/request/inline.rs:42-87` -> `native.rs:349-378` [S].
- **ENOSYS once / inline** `callbacks.rs:258-300, 574-683`, `F/request/state.rs:110-143` [S].

### 2.2 Reconciliation

**`find` in isolation: 675 minus 671** [R]. The difference is Getattr +100, Opendir +112, Readdir +222
(111 data pages of N = 10 and 111 end pages), Releasedir +112, Statfs +1.

| Quantity | Model | 675 - 671 |
|---|---|---|
| Source jobs | 112 + 222 | +334 = 334 |
| Read jobs | 100 + 112 + 111x3 + 111x1 | +656 = 656 |
| Lifecycle jobs | 112x2 + 222x2 + 112x2 | +892 = 892 |
| Reader grants | 222 | +222 = 222 |
| Transactions | 112x4 + 111x4 + 111x2 + 112 | +1226 = 1226 |
| Workspace | 112x11 + 111x25 + 111x11 + 112x3 + 100 | +5664 = 5664 |
| Lease | 112x34 + 111x65 + 111x25 + 112x8 | +14694 = 14694 |
| Inode | 112 + 111x11 + 111 + 100 | +1544 = 1544 |
| DirectoryEntry | 222 | +222 = 222 |
| Reclaim | 112 | +112 = 112 |

Residual zero in every family.

**C04 (671)** [R]: Lookup 1200, Getattr 1011, Setattr 1000, Mkdir 110, Flush 1, Release 1000, Opendir 10,
Releasedir 10, Create 1000. The lookups split 1110 negative and 90 positive: the 90 are the kernel's forced
revalidation of an existing parent under `LOOKUP_EXCL` when `mkdir -p a/b` calls `mkdir("a")` again [I].

| Quantity | Model | Receipt |
|---|---|---|
| Mutation jobs | 1000 + 110 + 1000 | 2110 = 2110 |
| Read jobs | 1200 + 1011 + 10 | 2221 = 2221 |
| Source jobs | 10 | 10 = 10 |
| Lifecycle jobs | 1000 + 10x2 + 10x2 + 1 (`Command::State` of the status call) | 1041 = 1041 |
| Reader grants | 0 | 0 |
| Transactions | 2110 + 1000 + 10x4 + 10 + 90 (positive lookups) | 3250 = 3250 |
| Workspace | 1200 + 1011 + 2110x2 + 1000 + 10x11 + 10x3 + 1 | 7572 = 7572 |
| Lease | 1000x3 + 1000x6 + 110x4 + 90x3 + 10x34 + 10x8 | 10130 = 10130 |
| Reclaim | 10 | 10 = 10 |
| Inode, DirectoryEntry | fitted split of the mutation rows (table 2.1), total exact | 13103, 5660 |
| Attempts, all families | 46225 + 3 observer `Startup` | 46228 = 46228 |

The I/D split between CREATE, MKDIR and SETATTR is a fit [I] (unique small-integer solution; the first MKDIR
under the base root costs 2 more Inode statements than the other nine); every other row is from source.
**C05 (675)** = C04 + the `find` table, so it is exact in the same sense.

**C12 (703)** [R]: Lookup 957, Forget 8, Getattr 865, Setattr 5, Mkdir 97, Unlink 106, Symlink 1, Rename 8,
Link 102, Open 229, Read 110, Write 235, Flush 1, Release 460, Opendir 5, Readdir 11, Releasedir 5,
Create 231.

| Quantity | Model | Receipt |
|---|---|---|
| Mutation jobs | 5+97+106+1+8+102+235+231 | 785 = 785 |
| Source jobs | 229 + 110 + 5 + 11 | 355 = 355 |
| Read jobs | 957 + 865 + 229 + 5 + 110x2 + 3x3 + 8x1 | 2293 = 2293 |
| Lifecycle jobs | 460 + 8 + 229x2 + 5x2 + 110x2 + 11x2 + 5x2 + 1 | 1189 = 1189 |
| Reader grants | 110 + 11 | 121 = 121 |
| Transactions | 785 + 460 + 8 + 229x4 + 5x4 + 110x4 + 3x4 + 8x2 + 5 + p | 2662 + p = 2868, p = 206 |
| Workspace | 8021 + (local names listed) | 8121, so 100 names |
| Lease | about 15850 + (positive lookups) | 16296, so about 445 = 206 x 2.2 |
| Reclaim, Inode, DirectoryEntry, Payload | not reproduced per opcode | 37 / 11610 / 3390 / 585 |

The Read-job equation forces 3 READDIR data pages and 8 end pages (3a + b = 17, a + b = 11) [R]. `p` is the
number of positive LOOKUPs (each takes the kernel lookup reference in a transaction,
`O/lifetime/native_visit.rs:126-133` [S]). **Residual:** the model assumes every one of the 785 mutations
published. Each one that did not (a refusal or an unchanged result) lowers Workspace by 1 and transactions by
1, so the exact statement is `names_listed - k = 100` and `p - k = 206` for an unknown `k`. With the root
directory at 101 names plus a small second directory, `k` is about 3 to 14 [I]. Inode, DirectoryEntry and
Payload need the UNLINK / LINK / RENAME traces, which I did not make. C09 (691) cross-checks the OPEN and READ
rows: Lease 512x39 + 38 + 4 + 3 = 20013 = 20013 [R].

### 2.3 Thread hand-offs

`mount_work` of 703 reports 3436 received units, 3749 steps and 321 wakes [R]: 313 steps beyond one per
request. READ contributes 110 `LeaveReceiver` and READDIR 11 `LeaveReceiver` + 3 `NextTurn`; the rest are
requests that found the owner occupied by a previous request's post-reply release job. In 671 there is none of
this subject (grants 0).

---

## 3. Where the time is, in microseconds

### 3.1 Unit costs from the receipts [R]

| | C12 (703) | C05 (675) | C04 (671) |
|---|---|---|---|
| Command ms | 317.7 | 880.5 | 823.4 |
| Owner service ms / wait ms | 133.7 / 5.3 | 230.5 / 14.8 | 170.9 / 3.2 |
| Outside the owner ms (per request µs) | 178.6 (52.0) | 635.2 (107.9) | 649.3 (121.5) |
| Foreground statement ms (attempts) | 115.2 (48646) | 199.0 (72142) | 147.2 (46228) |
| Trigger / cascade programs | 10861 | 18087 | 12851 |
| Service not in statements, µs per job | 4.0 | 4.3 | 4.4 |
| Maintenance jobs / ms inside the command | 48 / 1.7 | 233 / 8.7 | 10 / 0.5 |
| µs per attempt: W / I / D / L / T (3 statements) | 2.07 / 1.77 / 1.62 / 2.62 / 9.53 | 2.46 / 2.64 / 1.78 / 2.82 / 10.4 | 3.18 / 2.74 / 1.69 / 4.18 / 10.84 |
| Service µs per job: Read / Mutation / Lifecycle / Source | 16.3 / 58.8 / 32.7 / 31.6 | 15.1 / 54.5 / 31.0 / 35.1 | 9.4 / 53.6 / 34.8 / 48.4 |

`find` alone (675 - 671): service +57.7 ms, statement time +51.8 ms, owner wait +11.6 ms, maintenance +8.2 ms,
command +57.1 ms [R] (the command difference is smaller than service + wait because the two samples differ by
their own spread).

### 3.2 Service per request, today [E, fitted to the class totals of each receipt]

| Request | Service µs | of which before the reply | Method |
|---|---|---|---|
| OPEN (local) | 169 | 91 (`Source` 33 + `Observe` 58) | C12 unit costs x table 2.1 + 4 µs per job |
| READ (local) | 189 | 111 + one thread hand-off | same |
| RELEASE | 24 (C12) to 33 (C01/C04) | – (kernel does not wait) | class Lifecycle |
| OPENDIR | 132 (C05) | 80 | `find` difference |
| READDIR data page, N = 10 | 246 | 208 | `find` difference |
| READDIR end page | 99 | 61 | `find` difference |
| RELEASEDIR | 40 | – | `find` difference |
| LOOKUP negative / GETATTR | 6 to 9 | all | C01, C02 |
| LOOKUP positive | 24 to 40 | all | C12 / C04 fit |
| CREATE / MKDIR / SETATTR | 72 / 64 / 29 (C04) | all | C04 fit, sums to 108.7 of 113.2 ms |
| Mutation average in C12 | 58.8 | all | class Mutation |

Check: the C12 rows sum to 134.4 ms against 133.7 measured; the `find` rows to 58.5 against 57.7.

### 3.3 C12: which opcode families hold the 128 ms gap

| Family | Requests | Service ms | Outside the owner at 52 µs ms | Total ms |
|---|---|---|---|---|
| Mutations (CREATE, WRITE, LINK, UNLINK, MKDIR, ...) | 785 | 46.2 | 40.8 | 87.0 |
| LOOKUP (751 negative, 206 positive) | 957 | 9.1 | 49.8 | 58.9 |
| **OPEN** | 229 | **38.8** | 11.9 | 50.7 |
| GETATTR | 865 | 4.8 | 45.0 | 49.8 |
| RELEASE | 460 | 10.8 | 23.9 | 34.7 |
| **READ** | 110 | **20.8** | 5.7 + hand-off | 26.5+ |
| Directory (OPENDIR 5, READDIR 11, RELEASEDIR 5) | 21 | 3.6 | 1.1 | 4.7 |
| FORGET, FLUSH | 9 | 0.3 | 0.5 | 0.8 |
| Wait | – | 5.3 | – | 5.3 |

- **Per request C12 spends 92 µs** (39 service, 1.5 wait, 52 outside); the target's 189.3 ms is 55 µs per
  request for the same count. Even with zero owner service C12 would be at 178.6 ms.
- **The process part of C12 is small and unmeasured:** three `git` executions and one shell. Taking the 49 µs
  per-request floor of the earlier analysis (`r7-open/per-request-floor-outside-owner-fdc24ef3f.md`) leaves
  178.6 - 168.4 = about 10 ms for process creation and git's own computing [E]. No filesystem change removes
  it.
- **What this report's changes recover:** OPEN one visit -31.9 ms service; READ one visit -19.5 ms service,
  -110 grants, -110 hand-offs; directory -3.0 ms. A post-reply job runs on the receive thread before it reads
  the next request (one poll runs the whole request: ledger step 4 and
  `r7-open/per-request-floor-outside-owner-fdc24ef3f.md`; not re-read here [I]), so most of the post-reply
  service is command time for a client that issues its next call at once. Estimate **-46 ms of command time (range 30 to 54)** [E].
  C12 would be near 272 ms: still 1.44 times the target.
- **What is left after that:** mutations 87 ms (their service is the statement and trigger diet of another
  subject: C12 runs 10861 trigger and cascade programs, about 27 ms [E]); LOOKUP + GETATTR 109 ms, of which
  95 ms is the per-request floor; RELEASE 35 ms.

### 3.4 C05 and C04: the fixed cost of the processes

| | C04 (671) | C05 (675) |
|---|---|---|
| Outside the owner | 649.3 ms | 635.2 ms |
| Requests x 49 µs | 261.8 ms | 288.6 ms |
| Remainder = processes (1100 `mkdir`/`touch`, plus `find`, `wc`) [E] | 387.5 ms = 352 µs per process | 346.6 ms |

The two remainders differ by 41 ms for workloads that differ by two processes: that is the one-sample spread,
so read "350 to 390 ms" [E]. **It is 40 to 47 % of the command and no filesystem change removes it.** The
requests each process issues are in section 5.

C04's owner service by request [E]: CREATE 72 ms, RELEASE 33 ms, SETATTR 29 ms, LOOKUP 12.5 ms, GETATTR
8.4 ms, MKDIR 7 ms, OPENDIR + RELEASEDIR 1.7 ms. All but the last are already one visit; 12851 trigger
programs (about 32 ms [E]) are the largest item inside them.

---

## 4. Single-visit forms

Common shape, the one step 6 established (`O/lifetime/native_visit.rs:1-5` [S]): one owner job; its first
statement is a fence that reads the Workspace row, the native mount and the kernel's own reference on the
inode or descriptor the request names; the job's source is the current base, valid only inside the job
(class 3, `native_visit.rs:26-34`, `source.rs:85-90`); base facts come from resident cache objects inside the
job; a visit that cannot decide writes nothing and the request reads the missing facts outside the owner and
visits again holding nothing.

What every converted request stops recording, and what enforces the same thing afterwards:

| Removed | It protected | What still enforces it |
|---|---|---|
| `native_source` + `base_source` (kind 2) + `lease` (kind 5) + `file_custody.readers` + `workspace.base_readers` | the target's rows and the base root between the request's jobs and until its post-reply release | there is no "between": one transaction. Before it the fenced kernel reference holds the inode; after it the descriptor row does. Install, revoke, close and reclamation are owner jobs and cannot interleave (same argument and same delegated decision as step 6, architecture notes 28, 73, 75, 77) |
| `native_source.decided` | at most one decision per request | one visit decides once by construction |
| SELECT `native_read` duplicate check | a repeated request identity | `native_file` and `native_directory` are UNIQUE on (ns, mount, request) (`sql/schema.sql:235, 255`) |
| `native_read` + `file_read` + `base_source` (kind 1) retained by OPEN/OPENDIR | nothing in the reply: it carries no byte | – (same reasoning as ledger step 2 for attributes) |
| the same rows for READ | local payload across the hand-offs, including an orphan's | the bytes are copied inside the job; a window with an inherited span carries (base root, serial) to the base read, which is another agent's design |
| `revoke_native_mount` refusing while source or read rows exist (`native.rs:383-394`) | a revoke during request processing | revoke is called only after detach and drain (`native.rs:379-382`); the request is in the dispatcher's custody for its whole life |
| `retained_native_source` / `retained_native_read` observation after a lost completion | finding what a lost job acquired | a lost visit acquired either nothing or the descriptor, which `retained_native_file` / `retained_native_directory` still find by request |

### 4.1 OPEN as one visit

- **Job** `NativeJob::OpenVisit` (class Read): `FENCE_LOOKUP` (W1) -> `decide_read(Open)` over current rows and
  resident facts (I1, `W/operations/native_read.rs:191-201`) -> INSERT `file_handle`, INSERT `lease` (7),
  `FILE_OPENS_ADD`, INSERT `native_file` (L4), one transaction. Reply carries the descriptor's owner id as
  today (`F/request/reply.rs:95-108`).
- **Per request:** jobs 4 -> 1; attempts 58 -> 8 (+T 3 = 11, was 46 + 12); transactions 4 -> 1; service
  169 -> about 30 µs. For a base file: 5 jobs + 1 grant -> 1 job, 0 grants when the kind is resident (the
  inode object is; an OPEN needs the kind, not the length).
- **C12:** Source jobs 355 -> 126, Lifecycle 1189 -> 731, transactions 2868 -> 2181, Lease attempts
  16296 -> 9426, service -31.9 ms.
- **Growth:** constant per request; every statement is a point seek or a single-row write.
- **Files:** `O/lifetime/native_visit.rs` (new `open_native_visit`), `W/operations/native_visit.rs` +
  `native_read.rs`, `D/overlay/native_job.rs`, `D/service/filesystem_port.rs`, `F/ports.rs`,
  `F/operations/lookup.rs` (OPEN joins the `visit()` branch at `:240-246`), `F/request/reply.rs`.
- **Risk:** low. It is the CREATE custody (`O/lifetime/native_mutation.rs:80-122`) without the publication.

### 4.2 READ of local bytes as one visit

- **Job** `NativeJob::ReadVisit { mount, serial, handle, offset, length }` (class Read): `FENCE_FILE` (W1,
  the statement WRITE visits and RELEASE already use) -> the descriptor-addressed layers (orphan-aware, as
  `orphan.rs:144-176`) -> `LAYERS` (I1) + `CELL_RANGE` (P1) -> `LocalRead` copied in the job (it is today:
  the reply of `FileRead` owns the bytes, `filesystem_port.rs:255-278, 539-548`).
- **On top of the base-READ visit of the other agent, local READ needs only two things:** (1) the job returns
  the composed window with its `span`; (2) when `span` is `None` the request replies on the thread it is on,
  with no `LeaveReceiver`, no reader grant and no release. Today `read.rs:76-77` leaves the receiver and takes
  a grant unconditionally. OPEN needs nothing from READ: the descriptor row is the same one.
- **Per request:** jobs 5 -> 1; attempts 52 + 12 -> 3; transactions 4 -> 0; grants 1 -> 0; hand-offs 1 -> 0;
  service 189 -> about 11 µs.
- **C12:** Source 126 -> 16, Read jobs -110, Lifecycle -220, grants 121 -> 11, transactions -440, service
  -19.5 ms.
- **Risk:** low for local bytes. The clamp at file size comes from the local layer row as today.
- **A stage-0 form** inside the current path, three lines: skip `LeaveReceiver` and `immutable()` in
  `read.rs:74-90` when the completed window has no span. Counter: C12 grants 121 -> 11. Subsumed by 4.2.

### 4.3 OPENDIR, READDIR, RELEASEDIR

**OPENDIR** = 4.1 with `retain_native_directory` (`native_directory.rs:31-68`): W1 I1 L4, T1; 4 jobs -> 1;
132 -> about 28 µs.

**RELEASEDIR** one job: `FENCE_HANDLE` (W1) + `close_native_directory_inner` (`native_directory.rs:118-138`);
2 jobs -> 1. With page rows (below) the close deletes the handle's cookie pages inline when there are at most
a fixed few (I suggest 8 rows) and enqueues the existing maintenance item otherwise: the cost just past the
bound is today's indexed, windowed retirement. That removes the 223 maintenance jobs of C05.

**READDIR.** The cursor semantics stay exactly as they are: enumeration is in binary name order over the
merged local and base view, and a cookie means "resume strictly after this name"
(`native_directory_read.rs:8-33`, `native_cookie.rs:25-28`) [S]. That is what makes concurrent create and
unlink safe, and nothing below changes it: a name that exists throughout two pages sorts either at or before
the cursor (it was in an earlier page) or after it (it will be in a later one), so it is returned exactly once;
a name removed and created again in between may or may not appear, which POSIX permits.

- **Cookies: one row per page.** `native_cookie(ns, owner, cookie, name)` plus its index
  `native_cookie_name(ns, owner, name, cookie)` (`sql/schema.sql:274-282`) becomes one row per published page:
  `(ns, owner, first_cookie, names)` where `names` is the accepted names, length-prefixed, in order. A cookie
  `c` resolves by one seek (`first_cookie <= c ORDER BY first_cookie DESC LIMIT 1`) and an index into the
  blob, so a resume from the middle of a page works. The blob is bounded by the reply the names were sent in
  (`READ_WINDOW`, 4 KiB with today's kernel request).
- **Cookie numbers from engine memory.** A page takes a contiguous range from the engine's owner counter
  (`file_owners.rs:10-19`: never reused, never returned on rollback), so the `next_cookie` column, its SELECT
  and its UPDATE go away and reservation is not a write.
- **Reuse of an existing name's cookie is dropped** (the per-name SELECT, `native_cookie.rs:48-50`). It bounds
  the rows of one handle to its distinct names when a long-lived handle rewinds and reads again. To keep a
  bound without it: a READDIR at offset 0 on a handle that already has pages retires those pages (inline up
  to the fixed few, else through the maintenance item). POSIX makes positions obtained before `rewinddir`
  unspecified afterwards. **This is a visible contract change and needs the owner's word**; the alternative
  that needs none is to keep reuse per page (look up an existing page by its `after` name and equal content),
  at one more SELECT per page.
- **Kinds in one statement.** The `Page` job runs one `source_inode` per local name (3 statements each). The
  window statement returns the kind itself: `SOURCE_NAMES` with a correlated `(SELECT kind FROM inode ...
  ORDER BY gen DESC LIMIT 1)`, one program, N index seeks. A bound name has a link, so its inode has no
  orphan-domain row [I, verify]. Kinds of base names come from resident inode objects inside the job, or in
  one `lookup_inodes` batch during the hand-off when they are not resident.
- **How many visits per data page.** The reply buffer is filled before the send (`reply.add` only appends;
  `ok()` sends: `F/request/directory.rs:153-218`), but fuser does not tell the callback the buffer size
  (`V/request.rs:299-310`). Therefore:
  - **Strict form, two visits (recommended first):** visit 1 reads (fence, cursor seek, parent for `..`,
    parent inode, window with kinds; no write) and returns names, kinds and the reserved cookie range; the
    request fills the reply buffer and knows the accepted prefix; visit 2 re-fences the handle and inserts the
    page row with exactly the accepted names in one transaction; then the reply is sent. "Only accepted
    entries become valid offsets" (`overlay/tests/native_directory.rs:81`) holds unchanged.
  - **One visit:** publish the whole listed page in the visit and correct the row in a post-reply job only when
    the buffer took fewer names. Zero extra jobs in the common case, but an offset the kernel was never given
    is valid for a short time. I would not start with it.
- **End page:** one read-only visit (fence, cursor seek, parent inode, window): 4 statements, 0 transactions.
- **Grant only when needed.** `directory.rs:263-264` leaves the receiver and takes a Store reader for every
  page. A visit decides a purely local directory, or one whose base page is resident, in the job; an
  undecided visit hands off exactly as LOOKUP does (`lookup.rs:228-237`).
- **Per request:** data page (N = 10) 6 jobs / 114+ attempts / 4 T / 1 grant / LR + NT -> 2 jobs / about 12
  attempts / 1 T / 0 grants / 0 hand-offs, 246 -> about 36 µs. End page 4 jobs / 44 / 2 T / 1 grant -> 1 job /
  5 / 0 T / 0 grants, 99 -> about 11 µs. The per-name term falls from 5 statements to 0.
- **C05:** jobs 7264 -> 5999 (Source 344 -> 0, Lifecycle 1933 -> 1123, Read 2877 -> 2766); grants 222 -> 0;
  transactions 4476 -> 3605; attempts 72142 -> about 50600; maintenance jobs 233 -> 10; service -44.5 ms,
  wait and maintenance about -10 ms, hand-offs about -7 ms: **-55 to -62 ms** [E]. C12: about -3 ms.
- **What `native_directory_read` protected:** (i) cookies outliving RELEASEDIR while a read is in flight;
  (ii) the identity of prepare and publish, published once; (iii) one base root across page, listing and
  cookies. After: (i) visit 2 re-fences the open handle and a closed handle publishes nothing; the kernel
  does not release a directory while a READDIR on it is in flight [I]; (ii) the page row's primary key
  refuses a second insert of the same range; (iii) one job reads one Workspace row, and carried facts are
  tagged with their base root (`W/operations/native_visit.rs:143-159`).
- **Files:** `sql/schema.sql` + `accounting.sql` (cookie table; schema version), `O/lifetime/native_cookie.rs`,
  `native_directory.rs`, `native_directory_read.rs`, `O/maintenance/native_directory.rs`,
  `O/namespace/directory_entry.rs`, `O/database/statements.rs`, `W/operations/native_directory.rs` +
  `namespace/list.rs`, `D/overlay/native_directory_job.rs`, `D/service/filesystem_port.rs`, `F/ports.rs`,
  `F/operations/directory.rs`, `F/request/directory.rs`.
- **Risk:** medium (schema change, a rewritten test file, the reuse decision).

### 4.4 SETATTR

Already one visit: `callbacks.rs:303-361` -> `F/request/mutate.rs:139-195` -> `mutate_visit`
(`F/operations/mutation.rs:236`); the handle is passed only for a size change (`callbacks.rs:346-348`) [S].
W2 I3, one transaction, about 29 µs in C04. Nothing to convert; its remaining cost is the COMMIT (10.8 µs).

### 4.5 FORGET

- **Batch:** override `Filesystem::batch_forget` (`V/lib.rs:430-436`): one admission and one job
  `Forget { units }` per window of at most `PAGE_ROWS` (64) units, one transaction per window, instead of one
  job and one transaction per inode. The kernel sends a batch only when several forgets are queued when the
  daemon reads [I]; C03 received 1000 single frames and no batch [R 667], so this is for bursts (cache
  pressure, a busy daemon), not for these cells.
- **A plain inode:** a fence variant that returns `(owner, nlookup, implicit)` replaces `state` +
  `native_mount_state` + `native_lookup_row` (W1 L2 -> W1), and `queue_closed_at` reuses the fenced row
  (-W1): 7 -> 4 attempts. When the last reference of an inode with no orphan row goes, delete its
  `file_custody` row in the same transaction rather than enqueueing the orphan item (verify against
  `O/maintenance/orphan.rs` before doing it).
- C12: 8 requests; no measurable effect.

### 4.6 LOOKUP / GETATTR of a base object with zero grants

- **Not a cost in these three cells:** every grant in 703 and 675 is a READ or a READDIR, and
  `length_batches` is 0 in all three [R]. It matters for C09/C10 and for real trees.
- **Cause:** inode and directory objects are read from the resident cache inside the job
  (`W/operations/native_visit.rs:149-159, 231-246`), but a regular file's length comes from the Store length
  port (`W/base/view.rs:139`) and the resident client has none (`W/base/client.rs`, `resident()`), so the
  first visit of every base regular file is undecided [S].
- **Design:** a length memo in the existing per-daemon `CanonicalCache` state, beside the directory-count memo
  and built the same way (`W/base/links.rs:16-52`): key = the file's content root (`ObjectId`, 32 bytes),
  value = length. I prefer that key to (base root, serial): a length is a pure function of the content root,
  so the entry survives an install and is shared by every Workspace whose base contains the same content.
  Capacity 16,384 entries, 88 logical bytes each, 1,441,792 bytes for the whole daemon, least-recently-used
  eviction; independent of the number of mounts W and of concurrent Execs E.
- **A miss** costs what every base-file LOOKUP costs today: an undecided visit, a hand-off, one reader grant,
  one length batch, a second visit; the batch fills the memo. Past capacity the oldest entry is lost and its
  next demand is a miss: it degrades to today's path and never fails.
- **Per request, resident:** 2 jobs + 1 grant + 1 length batch + LR + NT -> 1 job. C09: length batches
  514 -> 1 or 2.
- **Files:** `W/base/cache.rs`, `client.rs`, `view.rs`, `W/mutation/facts.rs`. **Risk:** low.

### 4.7 Tests that pin the old shapes (search of `core/crates/*/tests` at 63c48d8dc)

| Test file | What it pins | Affected by |
|---|---|---|
| `layerfs-overlay/tests/native_directory.rs` (`only_accepted_entries_publish_offsets_and_concurrent_aliases_remain_valid`, `cookies_are_reused_and_retire_in_bounded_indexed_live_turns`) | prepare/publish API, per-name cookies, reuse, retirement turns | 4.3 (rewrite; the first test's property is kept by the strict form) |
| `layerfs-workspace/tests/native_directory.rs` (three tests) | read/page/cookie sequence, parent retained after FORGET and rmdir, moved parent | 4.3 |
| `layerfs-daemon/tests/native_jobs.rs` | `NativeJob::Source`, `Observe`, `NativeDirectoryJob::*` on the real owner | 4.1 - 4.3 |
| `layerfs-overlay/tests/native_lookup.rs`, `native_revocation.rs` | `observe_native*`, `acquire_native_source`, revoke refused while a source exists | 4.1, 4.2 (keep while the old path exists; retire with it) |
| `layerfs-workspace/tests/native_read_plan.rs` | `NativeReadPlan` for Open and Data | 4.1, 4.2 |
| `layerfs-daemon/tests/fenced_port.rs` (14 port calls), `cold_failure_scope.rs`, `mounted_parking.rs` | the fence and the failed-base-demand scope per port method | 4.1 - 4.3 (new port methods need the same cases) |
| `layerfs-daemon/tests/mounted_drain.rs` (`fp21_*`, `fp29_a_parked_read_spans_the_release_of_a_sibling_handle_and_the_removal_of_its_name`, `fp31_*batched_forget*`) | a parked RELEASE/READ under unmount; exact FORGET units | 4.2 (a local READ no longer parks), 4.5 |
| `layerfs-daemon/tests/native_coherence.rs` (`removed_references_keep_exact_state_without_open_custody`, opcode counts at `:154-166, 517-539`) | removed-file state, ENOSYS-once counts | 4.1, 4.2, section 5 |
| `layerfs-daemon/tests/native_custody.rs` (`kernel_forget_arrives_on_a_live_connection_with_its_exact_decrement`) | FORGET decrement | 4.5 |
| `layerfs-overlay/tests/native_visit_fence.rs` | fence statements and their plans | all (add the new fences) |
| `layerfs-workspace/tests/native_visit_cost.rs` (`the_five_jobs_of_one_created_file_cost_exactly_this_at_any_directory_size`) | per-family counts of LOOKUP, CREATE, GETATTR, WRITE, RELEASE | unchanged by this report; the place to add OPEN, READ, READDIR cost rows |
| `layerfs-daemon/tests/visit_port.rs`, `install_slots.rs` | `base_readers` stays 0 for visits; install not held back | must also hold for the new visits |
| `layerfs-daemon/tests/mounted_install.rs`, `native_mutation.rs`, `job_cost.rs` | behaviour through a real mount; completion cost | behaviour must not change; no count pinned that I found |

I listed these by search and by their names; I did not read every assertion.

### 4.8 Memory, storage, concurrency

- **Storage:** falls. No transient `native_source` / `base_source` / `lease` / `native_read` / `file_read` /
  `native_directory_read` rows; one cookie row per page instead of a row and an index row per name (C05:
  2220 b-tree entries -> 111). No new index, column or preallocation.
- **Memory:** nothing resident is added by 4.1 - 4.5. A READDIR visit's reply is at most `PAGE_ROWS` names
  with kinds, which the `Page` job already charges (`D/overlay/native_directory_job.rs:77-85`); a READ
  visit's reply is the window `FileRead` already charges (`commands.rs:410-417`). Both are per in-flight
  request and bounded by the existing request slots. 4.6 adds 1.44 MB per daemon.
- **Concurrent callers:** a visit is one exclusive owner turn; two requests interleave only between jobs, and
  every visit re-reads its fence. Two readers of one directory handle each get their own cookie range (the
  counter is engine-wide), as today's aliases do.
- **Several mounts:** all rows are keyed by namespace; `native_mount` is one row per namespace
  (`sql/schema.sql:197-203`). W mounts share one owner and one length memo; nothing per mount is added.

---

## 5. Request elimination

What the product already answers so the kernel stops asking [S]: FLUSH, FSYNC, FSYNCDIR
(`callbacks.rs:258-269`), the xattr family (`:279-300`), ACCESS (`:606-608`), READDIRPLUS, locks, BMAP, IOCTL,
POLL, LSEEK, FALLOCATE, COPY_FILE_RANGE (`:574-683`) are `ENOSYS`; STATFS is inline. The receipts confirm it:
one Flush per cell and no xattr or access request [R]. Negotiated capabilities are `FUSE_ASYNC_READ`,
`FUSE_BIG_WRITES` and `FUSE_MAX_PAGES` only (`F/mount/profile.rs:33-40`). Lifetimes are 60 s
(`F/request/reply.rs:18`); nothing below lengthens them.

What each command triggers, and whether the product could make the kernel not ask (every kernel rule is [I]):

| Request | Who triggers it | Kernel rule | Removable? |
|---|---|---|---|
| LOOKUP before CREATE / MKDIR (C04: 1110, C12: most of 751) | `touch`, `mkdir`, shell redirection, git | a new dentry is looked up before the create; MKDIR and `O_EXCL` force a fresh lookup (`LOOKUP_EXCL`) | no |
| LOOKUP of an existing parent by `mkdir -p` (C04: 90) | `mkdir -p a/b` when `a` exists | `fuse_dentry_revalidate` revalidates under `LOOKUP_EXCL` whatever the lifetime | no |
| GETATTR of the parent after each change in it (C04: about 1010; C12: up to 544) | every process that walks through a directory just changed | a directory change invalidates its attributes; `default_permissions` refreshes them before the next permission check | no (dropping `default_permissions` would change permission checks: forbidden) |
| GETATTR of a file after WRITE (C12: about 100, `git add` stats each file) | `stat` after a write | a non-writeback WRITE invalidates mtime/ctime | no (the writeback cache is forbidden) |
| SETATTR from `touch` (C04/C05: 1000) | `utimensat` on the new file | always sent | no |
| RELEASE of a created file (C04/C05: 1000; C12: 231) | `close` | sent for every descriptor the daemon issued | no |
| second READDIR of every directory (C05: 111) | `getdents` until empty | the kernel asks until it gets an empty reply | no |
| OPENDIR with no READDIR (C04: 10; C05: 11) | `open(dir, O_DIRECTORY)` to `fchdir` | – | only with no-opendir (below) |
| **repeated LOOKUP of a missing name** | git probing optional files in each of its three runs | an error reply caches nothing; a reply with node id 0 and a lifetime is a cached negative entry, consulted except under `LOOKUP_EXCL`/`LOOKUP_REVAL` | **yes** |
| **OPEN + its RELEASE** (C12: 229 + 229) | every `open` of an existing regular file | with `FUSE_NO_OPEN_SUPPORT` negotiated, one `ENOSYS` to OPEN ends OPEN and RELEASE for files not opened by CREATE | **yes, owner decision** |
| OPENDIR + RELEASEDIR (C05: 122 + 122) | `opendir` | `FUSE_NO_OPENDIR_SUPPORT`, the same for directories | later; needs inode-scoped cookies |
| repeated READDIR of an unchanged directory | a second listing | `FOPEN_CACHE_DIR` keeps entries in the page cache | later; the kernel then resumes on another handle with a cached offset, so it also needs inode-scoped cookies |
| READLINK repeats | – | `FUSE_CACHE_SYMLINKS` | yes, trivially safe (a symlink's target never changes); 0 requests in these cells |
| READ of a small file just written (C12: 100) | `git add` reads `f1..f100` | a write shorter than a page does not leave the page up to date | no |

- **Negative entries.** Reply a refused LOOKUP of kind Missing as an entry with node id 0 and the entry
  lifetime instead of `ENOENT` (`reply.rs:69-74`; `V/reply.rs:236-244` shows the node id is `attr.ino`).
  No kernel reference is taken, so no row and no FORGET. Soundness needs exactly what the product already
  states for its 60 s positive lifetime and for `FOPEN_KEEP_CACHE`: "every change to a mounted view arrives
  as a request on this connection" (`F/coherence/pages.rs:8-10`). A name created outside the kernel would be
  hidden for up to 60 s; `Command::Namespace` exists (`commands.rs:179-181`) and I did not establish that it
  cannot reach a mounted Workspace. **Value here is small:** each name in C04/C05 is probed once and then
  created (0 saved); in C12 I estimate 20 to 60 of 751 [E, unverified]. It is for build and status loops.
- **No-open.** C12: 3436 -> 2978 requests. After 4.1 it is worth about 458 x 49 µs + 229 x 54 µs = about
  35 ms [E] (66 ms against today). The engine work is what the earlier research listed: READ and WRITE
  addressed by inode when the handle is 0, and the routing of an unlinked file's reads and writes, which
  today keys on the descriptor. The kernel keeps its lookup reference for as long as the file is open [I], so
  custody can key on `native_lookup`. The daemon's own "descriptor was opened for writing" check
  (`O/namespace/compound.rs`) cannot be made for handle 0; the VFS makes it. Whether dropping that second
  check is a "changed permission check" is the owner's call. Medium-high risk; do it only after 4.1 - 4.2.

---

## 6. Ranked changes

| Rank | Change | Cause, as a count | After | µs per request | ms per cell | Risk |
|---|---|---|---|---|---|---|
| 1 | OPEN one visit (4.1) | 4 jobs, 58 attempts, 4 T per OPEN | 1 job, 11, 1 T | 169 -> 30 | C12 -31.9 service | low |
| 2 | READ local one visit (4.2) | 5 jobs, 64 attempts, 4 T, 1 grant, 1 hand-off | 1 job, 3, 0 T, 0, 0 | 189 -> 11 | C12 -19.5 service, -110 hand-offs | low |
| 3 | READDIR visits + page cookies + kinds in one statement (4.3) | 6 jobs and 64 + 5N attempts per data page, 4 jobs per end page, 1 grant each | 2 jobs and about 12; 1 job and 5; 0 grants | 246 -> 36; 99 -> 11 | C05 -33 service, about -17 wait/maintenance/hand-off | medium |
| 4 | OPENDIR / RELEASEDIR one visit (4.3) | 4 jobs / 2 jobs | 1 / 1 | 132 -> 28; 40 -> 36 | C05 -12; C04 -1 | low |
| 5 | no-open (5) | 458 of C12's 3436 requests | 0 | 49 + 27 each | C12 about -35 | medium-high, owner |
| 6 | length memo (4.6) | 2 jobs + grant + length batch per base file | 1 job | – | 0 here; C09/C10 and real trees | low |
| 7 | negative entries (5) | an `ENOENT` is never cached | repeats not sent | 55 each | C12 up to -3 | low once the basis is confirmed |
| 8 | FORGET batch + fence (4.5) | 1 job + 1 T per inode | 1 per 64 | – | 0 here; bursts | low |

Combined on C12 for ranks 1 - 4: jobs 4622 -> 3439, transactions 2868 -> 1701, grants 121 -> 0, attempts
48646 -> about 28500, service 133.7 -> about 79 ms, **command about 272 ms [E]**; with rank 5 about 237 ms.
The target (189.3) is not reached by this subject alone.

---

## 7. Staged plan

Each stage is one commit that leaves the tree green; the old path stays until its last user is gone (it is
already unreachable for mutations and still present, ledger step 6).

| Stage | Crates, in order | Change | Proving counter (one sample, same cell) |
|---|---|---|---|
| 0 | fuse | no `LeaveReceiver` and no grant for a READ window without an inherited span (`read.rs:74-90`) | 703 -> reader grants 121 -> 11; `mount_work` wakes -110 |
| 1 | overlay, workspace, daemon, fuse | OPEN as one visit | C12 Source jobs 355 -> 126; transactions 2868 -> 2181 |
| 2 | overlay, workspace, daemon, fuse | READ of local bytes as one visit (shares the job with the base-READ work) | C12 Source jobs 126 -> 16; transactions 2181 -> 1741 |
| 3 | overlay, workspace, daemon, fuse | OPENDIR and RELEASEDIR as one visit each | C05 Lifecycle jobs 1933 -> 1567; Source 344 -> 222 |
| 4a | overlay | cookie pages (schema, retirement, inline delete at close), kinds in the window statement | overlay cost test: statements per listed name 5 -> 0; equal counts at 10 and 4,000 names |
| 4b | workspace, daemon, fuse | READDIR as a reading visit + a publishing visit | C05 reader grants 222 -> 0; Source jobs 222 -> 0; maintenance jobs 233 -> 10 |
| 5 | fuse, daemon, overlay | `batch_forget` override; FORGET fence | `fp31` batch test: transactions per batch of n = ceil(n / 64) |
| 6 | workspace | length memo | C09 `length_batches` 514 -> at most 2 |
| 7 | fuse (after owner confirmation) | negative entries | a second `stat` of a missing name: Lookup count +0 |
| 8 | all (owner decision) | no-open | C12 Open 229 -> 1, Release 460 -> 231 |
| last | overlay, workspace, daemon | retire the source-holding native read path and its tables | production LOC falls; `native_source` rows written in any C cell = 0 |

---

## 8. Not verified

- Every kernel rule in section 5 and the `LOOKUP_EXCL` explanation of C04's 90 positive lookups: recollection,
  not read locally.
- The split of Inode and DirectoryEntry statements among CREATE, MKDIR and SETATTR (a fit), and all of C12's
  Inode, DirectoryEntry, Payload and Reclaim per opcode; `k`, the number of C12 mutations that published
  nothing.
- The microsecond rows of 3.2: built from per-family averages of one sample per cell; class totals agree
  within 1 %, single rows may be off by 10 to 20 %. Command-time savings are estimates; the receipts do not
  divide time outside the owner.
- That a bound name's inode never has an orphan-domain row (needed for kinds in the window statement).
- That fuser accepts `INodeNo(0)` in a `FileAttr` for a negative entry, and that no control-plane mutation can
  reach a mounted Workspace.
- Whether the kernel can send RELEASEDIR while a READDIR on the same handle is in flight (I assume not).
- The assertions inside the tests of 4.7 beyond the lines quoted.
- Nothing was built or run; every "after" count is a prediction from the model in section 2.
