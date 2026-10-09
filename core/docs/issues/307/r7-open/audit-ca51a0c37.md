# R7 product audit: 63c48d8dc..ca51a0c37

Scope: `core/crates/layerfs-{overlay,workspace,fuse,daemon}` `src/` and `sql/`.
Method: read-only, `git show` / `git grep` / `git diff` at the two commits; nothing built or run.
All `file:line` are at ca51a0c37, paths relative to `core/crates/`.
Product diff: 71 files, +3574 / -2412 physical lines. No change to any `Cargo.toml`, `Cargo.lock`,
`core/vendor`, `core/patches` or `.cargo` in the range.

Workload note: the assignment describes C06 as "bs 128 KiB"; `r7/workloads.py` has
`dd if=/dev/zero of=big bs=1M count=64` (the kernel splits it into 128 KiB WRITEs). C09/C10/C11 run
on a preconditioned root that, as I read `runner.py:470-471`, already holds that 64 MiB all-zero
file as committed base content; I did not trace the harness further.

## Verdicts

| Section | Verdict | Item that decides it |
| --- | --- | --- |
| A benchmark recognition | CLEAN (product); CONCERN (workload content) | nothing in the diff or source recognises a workload; C06-C11 content is all zeros and a pre-existing zero path exists in Commit construction |
| B constants and bounds | CONCERN (low) | `RUN_BYTES` raises three pre-existing bounds 8x; needs explicit owner acceptance |
| C memory | CLEAN | nothing new grows with files, bytes, directories or mutations |
| D storage | CONCERN | an index and a table with no reader/writer left are still stored; reply rows of a rewound handle are unbounded per open handle |
| E semantics | CONCERN | request custody of READ/OPEN/OPENDIR/READDIR is no longer in SQL; one unmount window is untested by the author's own statement |
| F forbidden mechanisms | CONCERN (minor) | test-only surface added in `src`; nothing else |
| G simplicity | CONCERN | three additions have a visibly simpler form; unreachable source-holding path kept |

No VIOLATION found.

---

## A. Benchmark-string search

Searched: the added lines of the product diff, and the full product source of the four crates at
ca51a0c37, for `/dev/`, bench/harness/workload words, `env::var`/`getenv`/`env!`, `cfg(test)`/
`cfg(feature`/`debug_assertions`, 67108864 / `64 * 1024 * 1024` / `1 << 26`, 1048576, 1000,
131072 / `128 * 1024`, 32768, 16384, file names and extensions (`big`, `f<N>`, `.git`, `node_modules`,
`.ts`, ...), command names, zero-content tests (`all(|b| b == 0)`, sparse, hole, zero-fill), and
words such as fast path / special case / heuristic.

| Hit | file:line | New? | Class |
| --- | --- | --- | --- |
| `"/dev/fuse"` | `layerfs-daemon/src/application/filesystem.rs:81`, `layerfs-fuse/src/mount/syscalls.rs:19` | pre-existing | legitimate: the FUSE device |
| `std::env::args_os()` | `layerfs-daemon/src/application/cli.rs:22` | pre-existing | legitimate: CLI arguments. No `env::var` in the four crates |
| `RUN_BYTES = 32 * 1024`, CHECK `32768` | `layerfs-overlay/src/contract/types.rs:10`, `sql/schema.sql:56-57` | new | legitimate, see B. Divides 128 KiB, 1 MiB and 64 MiB only because it is a power of two; it is not the fitted value (128 KiB, one row per WRITE) |
| `length(names)<=16384` | `sql/schema.sql:263` | new | legitimate: `PAGE_ROWS` (64) x 256 |
| `128 * 1024` windows, `65536` record bound, 8/16/32 MiB allowances | `types.rs:16-20`, `workspace/src/base/client.rs:7-8`, `daemon/src/install_types.rs:25`, `daemon/src/overlay/owner.rs:29` | pre-existing, unchanged | legitimate |
| `bytes.iter().all(\|byte\| *byte == 0)` -> `FileRun::Zero` | `layerfs-workspace/src/construction/captured/scan.rs:241` | pre-existing (a08bbe39d, 2026-10-07); untouched lines | see below |

Not found anywhere: 1000, 64 MiB, any file name, extension or command name, any environment
variable, any `cfg(test)`/feature flag, any identifier of harness, cell or receipt.
The overlay write and read paths have no zero-content case: `Window::trim`
(`overlay/src/payload/cells.rs:81-108`) trims by validity mask, never by byte value, and
`write_whole` (`overlay/src/payload/stream.rs:76-141`) stores the caller's bytes as given.

The one item to raise (not a product change of this run):
`scan.rs:241` classifies an all-zero captured window as `FileRun::Zero`, and Content then
"reus[es] full zero subtrees instead of processing a hole's length"
(`layerfs-content/src/file/construction/runs.rs:50-53`). By that contract the root equals the one
obtained by streaming the zeros, so it is canonical hole-aware construction, not a cheat. But
C06-C11 write and read only zeros (`dd if=/dev/zero`). For those cells every Commit runs on the
zero-subtree path, the Store holds a handful of distinct chunk objects for 64 MiB, and the base
file that C09/C10/C11 read is served from those few cached objects. Speed and storage numbers of
C06-C11 for Commit, Store bytes and base reads therefore describe the best case.
Resolution: add a pseudorandom-content variant of C06-C11 (same sizes) before any Commit, Store
storage or base-read claim is made from these cells. Overlay-side numbers (WRITE statements,
overlay pages) are unaffected: the overlay stores zeros verbatim.
R7 side effect on that path: the classification unit grew from one cell to one row of up to
32 KiB (`scan.rs:145-147`), so a 32 KiB row with one non-zero byte is now all `Data`. Same root by
contract; more bytes hashed. Not tested against row shape in this run as far as the diff shows.

## B. New or changed constants, capacities, bounds

| Name = value | file:line | Commit | Bounds | How chosen (as documented) | Equals/divides a cell parameter | Just past it | Raises a pre-existing limit |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `RUN_BYTES` = 32768 | `overlay/src/contract/types.rs:10` | e555b771f | widest payload row; aligned slot | comment: bounds the overflow-page walk of one random access / one maintenance row to 8 pages | divides 128 KiB (x4), 1 MiB, 64 MiB as any power of two does; not equal to any | next slot gets its own row (`stream.rs:47-53`); unaligned or partial data stays cell rows (`stream.rs:143-185`) | yes: see next three rows |
| payload CHECK `length(data) BETWEEN 1 AND 32768 ...` | `overlay/sql/schema.sql:56-57` | e555b771f | stored row shape | repeats `RUN_BYTES` | as above | constraint failure; writers never exceed a slot | yes, 4096 -> 32768 |
| captured window bound `RUN_BYTES`, `RUN_BYTES/8` | `workspace/src/construction/captured/scan.rs:145-147`, `overlay/src/payload/captured_types.rs:43,107` | e555b771f | one captured-scan window | follows row width | - | wider row is rejected as invalid record (cannot be stored) | yes, 4096/512 -> 32768/4096 |
| captured reply charge `+ RUN_BYTES + RUN_BYTES/8` | `daemon/src/overlay/commands.rs:393-394` | e555b771f | owner admission charge of one captured-run reply | follows row width | - | charged against the unchanged 8 MiB owner allowance | yes, 4608 -> 36864 bytes |
| `PAGE` = {cells 14, rows 64} | `overlay/src/maintenance/ready.rs:29-32` | 989bfa887 / e555b771f | one maintenance step and the releasing job's inline budget | the former literals `LIMIT 14` / `LIMIT 64`, now one value; static assert that the widest row fits (`ready.rs:34`) | no | rest is left for the next step (`ready.rs:39-51`) | no. Consequence: 14 is not a multiple of 8, so with 32 KiB rows a step drops one row (32 KiB), where it dropped 56 KiB before |
| `RELEASE_CALLS` = 6 | `overlay/src/maintenance/orphan.rs:10` | 989bfa887 | step calls of one inline release | 2 for the orphan + 2 for each of at most 2 lower layers | no | remainder queued ready or parked (`orphan.rs:126-146`) | new |
| `release_step` (one inline finish per job) | `overlay/src/database/connection.rs:35,243`, `lifetime/file_owners.rs:75`, `maintenance/ready.rs:189` | 989bfa887 | inline reclamation per atomic job | "at most one maintenance step's rows" | no | later orphans of the job are queued as before (`file_owners.rs:78-81`) | new |
| `INLINE_PAGES` = 8 | `overlay/src/lifetime/native_directory.rs:160` | 39e4ce766 | reply rows RELEASEDIR deletes itself; also rows per maintenance turn | decision D-e; no derivation given beyond "a window of the same statement" | no (8 replies = up to 512 names; the 1000-name directory of C01-C03 takes the queued path, the 10-name directories of C04/C05 the inline one) | header marked closed, indexed item retires 8 rows a turn (`native_directory.rs:132-139`, `maintenance/native_directory.rs:22-37`) | lowers the turn window from 64 name rows to 8 reply rows (up to 512 names) |
| `LIMIT 9` | `overlay/src/database/statements.rs:101-102` | 39e4ce766 | `COOKIE_PAGES` probe | literal `INLINE_PAGES + 1`, not derived from it | no | - | new; the two values must be changed together by hand |
| `names` CHECK 2..16384 bytes; `after` <= 255 | `overlay/sql/schema.sql:262-263` | 39e4ce766 | one published READDIR reply | 64 names x (1 + 255) | no | a reply never lists more than `PAGE_ROWS` names (`workspace/src/operations/namespace/list.rs:72`, unchanged); `publish_native_cookies` refuses more (`lifetime/native_cookie.rs:81-83`) | replaces per-name rows |
| fresh cookie range = `PAGE_ROWS` (64) identities | `overlay/src/lifetime/native_cookie.rs:41`, `file_owners.rs:22-30` | 39e4ce766 | offsets one reply may hand out | one window | no | counter exhaustion at i64::MAX is an error (2^57 visits) | new: every READDIR reading visit that can list a name consumes 64 owner identities, published or not |
| `JobRows` places = `COMPOUND_INODES` (4) | `overlay/src/namespace/job_rows.rs:20` | dc0c0a6f7 | inode rows one job remembers | serials one compound job can publish | no | not recorded; read and classified by the indexed statements (`job_rows.rs:39-43`, `inode.rs:201-222`) | no (constant unchanged) |
| `LENGTH_CHARGE` = 264 | `workspace/src/base/cache.rs:52` | dbaad86a5 | charge of one remembered length | 8 + the 256 bookkeeping bytes an object is charged | no | LRU eviction; allowance < 264 remembers nothing (`cache.rs:108-118`) | no new allowance |
| `NativeDirectoryWindow::CHARGE` | `workspace/src/operations/native_directory.rs:136-142` | 39e4ce766 | reply charge of one READDIR reading visit | 3 windows of `PAGE_ROWS` entries + one reply's names | no | charged against the unchanged owner allowance | replaces two smaller charges; see C |
| `NativeDataVisit::charge` = length + length/8 | `workspace/src/operations/native_data.rs:86-88` | 16fa316ee | reply charge of one READ visit | window bytes + one inherited bit each | length <= `READ_WINDOW` (unchanged) | - | no |
| `PHASES` (11 tables), `ORPHANS` = 7 | `overlay/src/maintenance/reclaim.rs:41,110` | 9f7037e04 | terminal reclaim cursor | table order of the old `match` | no | - | no. `ORPHANS` is a positional index into `PHASES` kept by hand |
| lease CHECK `kind BETWEEN 1 AND 6 OR kind=8` | `overlay/sql/schema.sql:75` | 77a2c137a | storable lease kinds | kinds 7 and 9 no longer written | - | constraint failure | narrows |
| `file_handle.mount >= 0`, `request` CHECK `(mount>0 OR request>0)`, `UNIQUE(ns,mount,request)` | `overlay/sql/schema.sql:132-140` | e62685c59 | one descriptor row | native request is "any 64 bits" | - | duplicate request refused by the key | replaces `native_file` |
| inode / maintenance `kind BETWEEN` | `overlay/sql/schema.sql:23,107` | dc0c0a6f7 | same sets as `IN (...)` | equivalent on STRICT INTEGER | - | - | no |
| `user_version` 29 | `overlay/sql/schema.sql:268`, `database/startup.rs:136` | merges | schema identity | - | - | - | header comment still says v25 (`schema.sql:1`) |

Unchanged in the range (verified by diff): `PAGE_ROWS` 64, `READ_WINDOW`/`WRITE_WINDOW` 128 KiB,
`COMPOUND_INODES` 4, `OPERATION_RECORD_BYTES`, statement cache capacity 256 (`startup.rs:144`),
`OwnerConfig` (8 MiB, 16 namespaces, 16 jobs each; `daemon/src/overlay/owner.rs:27-33`, file not in
the diff), `HANDOFFS` 16 (`fuse/src/dispatch/types.rs:4`, not in the diff), `StoreSettings`
(`daemon/src/install_types.rs:22-28`, not in the diff). No queue, slot, timeout or thread constant
was touched.

Paths that fail rather than degrade, all invariant violations, none a capacity:
`INODE_INSERT` over an existing row / an update that changes no row -> `Invalid("inode row of the
running job")` (`overlay/src/namespace/inode.rs:300-302`); an in-place write before the job's first
writing statement -> `Invalid` (`overlay/src/payload/runs.rs:166-168`); a second publication of one
offer fails on the row key (`native_cookie.rs:88-99`).

CONCERN, exact item: `RUN_BYTES` is a larger unit that raises three pre-existing bounds 8x. It is
bounded, documented in e555b771f, stores fewer pages, and is not tuned to the 128 KiB WRITE. It
sits inside the wording "larger limits/buffers", so the owner should accept it explicitly.
Resolved by: an owner line accepting 32 KiB rows and the 36864-byte captured reply charge.

## C. Memory

Concurrency facts used below (all unchanged by the run): one overlay connection and one owner
thread per daemon, so per-job buffers exist once per daemon whatever W and E are; at most 16
handed-off requests + 1 receive slot per mounted Workspace; owner admission 8 MiB per daemon.

### Resident (survives a job)

| Structure | Where | Capacity | Scope | W Workspaces x E Execs | Released | Grows with data? | Claim vs code |
| --- | --- | --- | --- | --- | --- | --- | --- |
| length memo `Cache.lengths` + `ages` entries | `workspace/src/base/cache.rs:59-61,98-118` | charged 264 bytes each inside the one `CanonicalCache` allowance (default 8 MiB -> at most about 31,700 entries if it held nothing else); real size about 100-130 bytes an entry (estimate from the two B-tree keys) | per daemon Store (`daemon/src/store/open.rs:63`), shared by all Workspaces, kept across mounts | one allowance, independent of W and E | LRU with objects (`cache.rs:79-96`) | no: bounded by the allowance; it competes with objects for it, so a stat sweep of many base files evicts object bytes at 264 per file | matches dbaad86a5. The `ages` key gained a `bool`: about +8 bytes per cached object, inside the existing 256-byte bookkeeping charge |
| `Overlay::release_step` | `overlay/src/database/connection.rs:35` | 1 byte | per engine | 1 | - | no | matches |
| `orphan_seen` (now exact) | `connection.rs:30,244-249` | existing flag | per engine | 1 | - | no | matches 3558fb2be |
| prepared statements | cache capacity 256 (`startup.rs:144`), unchanged | more distinct statement texts exist (22 terminal, 7 payload, 4 inode, 3 cookie, HELD, FENCE_DIRECTORY, minus the removed ones) | per daemon | 1 | LRU of rusqlite | no | not counted by any commit; whether the distinct texts still fit 256 is unverified |

No per-handle, per-offset, per-file or per-directory resident structure was added.

### Per job / per request

| Buffer | Where | Bound | Scope | Total for W x E | Released | Claim vs code |
| --- | --- | --- | --- | --- | --- | --- |
| `JobRows` | `overlay/src/namespace/job_rows.rs:20`; created at `lifetime/native_visit.rs:176,300` | 4 x (serial + inode row + 3 integers), about 0.5 KiB as claimed (not measured) | stack of the running atomic job | 1 per daemon | job end; each entry dropped by the write of its serial | matches dc0c0a6f7 |
| SQLite bind + record of one payload row | `stream.rs:125-139` (`CELL_PUT` bound from the caller's slice) | 2 x 32768 transient (was 2 x 4096) | the one connection | 1 per daemon | statement reset | consistent with the code (borrowed bind, no `to_vec`); not measured |
| in-place blob handle | `stream.rs:42,71-74`, `runs.rs:158-189` | one handle per write window | job | 1 per daemon | closed before the job commits | matches |
| `row_bytes` / `part` / `transfer_row` copy | `runs.rs:116-153,258-272` | <= 32768 | job | 1 | call end | matches |
| local read composition | `stream.rs:249-357` | window (<= 128 KiB) x2 (data + decided) + rows fetched; rows may start up to 28 KiB before the window and end up to 28 KiB after it, so about 184 KiB of row bytes per layer query instead of 128 KiB | job | 1 per daemon | job end | not stated in any commit; small, bounded |
| captured window + reply | `scan.rs:145-147`, `commands.rs:393-394` | 32768 + 4096 (was 4096 + 512) | one in-flight captured read | 1 per Commit producer | step end | matches e555b771f |
| READ visit reply `NativeWindow` | `workspace/src/operations/native_data.rs:31-41,86-88`; charge at `daemon/src/overlay/native_job.rs:124-127` | <= 128 KiB + 16 KiB, charged | in-flight READ job | <= 16 per Workspace, 8 MiB per daemon | completion drop | matches 16fa316ee |
| READ request copy | `fuse/src/operations/read.rs:74-91` | wholly local: borrowed from the completion, no copy. Otherwise the window is cloned (<= 144 KiB) and the completion dropped, then <= 128 KiB inherited: about 272 KiB held by the request outside owner credit | in-flight request | <= 17 per mounted Workspace, about 4.6 MiB each Workspace worst case; scales with W, not E | reply | same order as before (128 KiB local + 128 KiB data); the copy is no longer under owner credit |
| READDIR visit reply `NativeDirectoryWindow` | `workspace/src/operations/native_directory.rs:41-49,136-142`; `daemon/src/overlay/native_directory_job.rs:52-57` | `CHARGE` about 84 KiB (my arithmetic with estimated struct sizes); the former page reply was about 42 KiB and the cookie plan about 19 KiB | in-flight READDIR visit, then cloned by the request (`fuse/src/operations/directory.rs:158`) | <= 17 per mounted Workspace | reply | 39e4ce766 says "one window charged as CHARGE": true; it does not say the single charge is about twice the former largest one |
| `NativeCookieOffer` | `overlay/src/contract/native_directory.rs:98-140` | `after` <= 255 + one reply's names <= 64 x (24 + 255) | in-flight READDIR | as above | publish or reply | matches |
| inline release | `overlay/src/maintenance/orphan.rs:61-151` | `[(i64, Next); 6]` on the stack, <= 14 payload keys, <= 64 step rows | releasing job | 1 per daemon | job end | matches 989bfa887 / 36e2ec328 |
| terminal page keys | `overlay/src/maintenance/reclaim.rs:232-267` | <= 64 keys (names <= 255 bytes) | reclaim step | 1 per daemon | step end | matches 9f7037e04 |

Verdict CLEAN: every item is bounded per daemon, per mounted Workspace or per in-flight request,
none grows with files, bytes, directories or mutations, and the commit messages' memory claims
match the code. Two unreported facts: the READDIR reply charge roughly doubled, and the prepared
statement population is unverified against the 256 cap.

## D. Storage (schema 22 -> 29)

| Object | Change | file:line | Stored bytes |
| --- | --- | --- | --- |
| `payload.data` CHECK | 1..4096 -> 1..32768 with the dense/aligned shape | `sql/schema.sql:56-57` | per 64 MiB dense: 2048 rows / 16664 pages vs 16384 rows (e555b771f; its "before" for 64 MiB is computed, not measured). Lower |
| `payload_namespace_row` index | kept, no reader left | `sql/schema.sql:70`; only reference in `src`/`sql` | every payload row still writes one entry of it. Pure cost |
| `lease` | kinds 7 and 9 no longer stored | `sql/schema.sql:75` | per open handle and per kernel-referenced inode: -1 row, -1 `lease_resource` entry |
| `file_handle` | + `mount`; `UNIQUE(ns,mount,request)` | `sql/schema.sql:132-140` | per open handle 5 -> 2 B-tree entries; +1 header byte on a non-native row |
| `native_file` + 2 triggers | removed | - | folded into `file_handle` |
| `native_directory.next_cookie` | removed | `sql/schema.sql:244-252` | - |
| `native_directory_read` + index + 2 triggers | removed | - | READDIR records no read row |
| `native_cookie` | one row per published reply (`first_cookie`, `after`, `names`); index `native_cookie_after(ns,owner,after,first_cookie)` replaces `native_cookie_name` | `sql/schema.sql:258-267` | per listed directory: 10 names 20 -> 2 entries; 4000 names 8000 -> 126 entries (39e4ce766). `after` is stored twice (row and index) |
| `native_read` + 2 triggers | kept, no writer left | `sql/schema.sql:229`; read at `lifetime/native.rs:379`, `lifetime/native_observation.rs:26`; deleted from at `lifetime/file_owners.rs:393` | always empty; three statements probe or delete from an empty table |
| accounting triggers | 61 triggers write the namespace row only; orphan triggers still write ns 0 | `sql/accounting.sql`; `:75-82` | rows unchanged. `resources(None)` now sums one row per namespace (`database/accounting.rs:34`): O(namespaces), read by the daemon diagnostic only (`daemon/src/overlay/commands.rs:478-481`) |
| `payload_account_delete` | BEFORE DELETE, lengths by sub-select | `sql/accounting.sql:48-50` | none; one extra row seek per deleted payload row |

Where stored bytes can be higher than before:

1. Known: a directory handle that is rewound or re-seeked over a changing directory stores one
   reply row per reply that differs from the latest one listed after the same name
   (`workspace/src/operations/native_directory.rs:170-182`, `overlay/src/lifetime/native_cookie.rs:22-40`).
   The bound is the number of READDIR calls on that handle x up to 16384 + 2 x 255 bytes; nothing
   is retired before RELEASEDIR ("replies are not retired at offset 0", 39e4ce766). Before, the
   bound was distinct names ever listed. An alternating listing (A, B, A, B) stores a row every
   time, because only the latest row after a name is compared.
2. Same mechanism, no rewind needed: any seek back to an already answered offset after the
   window changed by one name stores a whole new reply (up to 64 names) for a one-name change.
3. A row never coalesces: a file written in 4 KiB appends stays one row per cell (64 rows per
   256 KiB, unchanged from before), and a wide row that a fold has to merge is rewritten as up to
   8 cell rows for good (`runs.rs:258-272`). Never above the old shape, but the 8x reduction
   applies only to aligned writes of two cells or more.
4. Terminal reclamation of a dense file frees 32 KiB a step instead of 56 KiB (`ready.rs:29-34`,
   9f7037e04 states it): stale bytes of a closed 64 MiB namespace live for 2048 steps instead of
   about 1171. Storage held longer, not more.
5. Not higher, checked: overwrite in place (`runs.rs:158-189`: same row, same length, no trigger);
   shrink inside a wide row (`layers.rs:128-170`: kept cells below the boundary, one tail part);
   shrink then regrow (a stale wide row is dropped whole by the first write that reaches its
   cells, `stream.rs:91-103,160-170`); a put into one cell of a wide row (`access.rs:33-45`,
   `runs.rs:192-208`: same bytes in at most three rows).

CONCERN, exact items and what resolves each:
- `payload_namespace_row` (`schema.sql:70`): drop the index (9f7037e04 says it "has no reader left").
- `native_read` (`schema.sql:229`) with its two triggers and three statements: remove
  (900231d0a says it "has no writer left").
- Reply rows of one open handle: either an owner statement accepting growth per differing reply,
  or a bound (for example replace the row listed after the same name instead of adding one).

## E. Semantics

Permission checks: none removed or reordered. The diff touches no mode/uid/gid/access code. READ's
kind refusals moved from the FUSE reply (EISDIR/EINVAL from the stat kind) to
`NativeWindow::refusal` (`workspace/src/operations/native_data.rs:127-133`) with the same mapping.
A handle's `writable` flag is still read by the fence (`statements.rs:224-226`).

Reply before commit: none found. OPEN/OPENDIR write the descriptor in the deciding transaction
and reply after the job returns (`lifetime/native_visit.rs:158-239`). READDIR publishes before
`reply.ok()` (`fuse/src/request/directory.rs:203-212`; `fuse/src/operations/directory.rs:234-262`).
READ runs no transaction.

Changed, in order of weight:

1. Request custody is no longer recorded for READ, READLINK, OPEN, OPENDIR and READDIR: no
   `native_source`, `base_source`, `file_read`, `native_read` row, no lease, no `base_readers`
   count (`lifetime/native_visit.rs:240-271`, `lifetime/native_directory_read.rs:58-66`).
   `revoke_native_mount` still fences only on those rows (`lifetime/native.rs:375-388`), so SQL no
   longer holds revocation back for these requests; the dispatcher's lane drain is the only fence.
   16fa316ee says so and states "NOT staged: a READ that is inside a Store read during an
   unmount; that case rests on the drain predicate, by source."
   A second dependency is implicit: the reply reads inherited bytes at the root the visit named
   (`workspace/src/base/view.rs:78-84`) after the visit, with nothing recording that reader.
   Safe today because Store objects are never collected; a future Store GC or base-root release
   keyed on `base_readers == 0` would not see an in-flight READ.
   Resolved by: a staged test of Unmount and Force with a READ parked inside its Store read, and
   a recorded contract line that in-flight visits hold no base reader.
2. A READDIR whose publishing visit races RELEASEDIR now fails and publishes nothing
   (39e4ce766, restaged `filesystem_port.rs`); before, its read source kept the cookie state.
   The kernel does not normally release a directory with a READDIR in flight.
3. RELEASEDIR is a disposal call: a stopped fence no longer refuses it (1fe0ed54b;
   `fuse/src/request/directory.rs:97-103`). Before, the handle row was left for revocation.
4. Dropped constraints, with `PRAGMA foreign_keys=ON` (`database/profile.rs:59`): the known
   descriptor -> `native_mount` foreign key (e62685c59, reason given: the child scan walked every
   descriptor); also `native_file -> file_handle ON DELETE CASCADE`,
   `native_directory_read -> native_source` (cascade) and `-> native_directory`, and the
   `request BLOB length = 8` CHECK. Ordering now rests on `retire_native` reading the mount's
   descriptor window empty before the mount row goes (`maintenance/native.rs`).
5. Retained longer: a file removed while only kernel lookups reference it is no longer migrated
   into its orphan domain; its orphan keeps its lower layers until the last lookup is released
   (`lifetime/orphan.rs:138-144`). A captured generation's rows of that serial cannot retire
   before the kernel's FORGET. 989bfa887 describes the queueing change, not this consequence.
6. Released earlier, inside the releasing job: the custody row of a live file at its last
   reference (`lifetime/file_owners.rs:66-72`); a whole small orphan (up to one page, 6 calls) in
   the FORGET or RELEASE that drops the last reference (`maintenance/orphan.rs:61-151`); a
   directory handle with at most 8 replies in its RELEASEDIR (`lifetime/native_directory.rs:119-156`).
   FORGET of a removed one-cell file goes 12 -> 29 statements while maintenance goes 4-7 steps -> 0
   (989bfa887, 36e2ec328): work moved into the request path, not out of it.
7. `StoredCounts::owner_rows` no longer counts descriptors and lookup references (77a2c137a);
   a reader of that diagnostic as "anything held" would now be wrong. `queue_closed_at` was
   moved to the six-table `HELD` probe (`lifetime/close.rs:14-19`).
8. The daemon aggregate is derived at read (D). Nothing in admission reads it.
9. An in-place overwrite keeps the row's stamp (`runs.rs:154-157`). I checked the staircase
   reasoning (a live row stays live under existing steps, and any later step has a newer epoch
   than any current stamp) and found no case where it differs from re-stamping; this is my
   reading of `layers.rs:68-91`, not a tested claim.

Warm state and caches:

- Length memo (`workspace/src/base/client.rs:242-251`): filled only by a provider answer to a
  LOOKUP/GETATTR/READ/OPEN-driven `file_length`, keyed by immutable content root, never by Commit
  construction (`client.rs:103`, `base/file.rs:21`). It lives in the daemon's one cache, so it
  persists across mounts and across Workspaces of one daemon. It is a legitimate product cache.
  Measurement consequence: before the run every stat/open/read of a base file paid a length
  demand even in a warm daemon; now a warm-up or setup command in the same daemon removes it.
  In cache class A the measured command pays its own first demand (C09: 514 length batches -> 1,
  all inside the command). In classes B/C (`runner.py:937-952` runs a warm-up Workspace in the
  same daemon) the saving is credit from the declared warm-up, and must be reported as such.
- `orphan_seen` is now exact (`lifetime/orphan.rs:57-67`, `connection.rs:244-249`): a used engine
  costs the same as a fresh one once its last orphan is gone. This removes a fresh-engine
  advantage; it does not add one.
- The orphan-free fast path is keyed on an engine-wide flag: one unlinked-but-open file in any
  Workspace adds one probe statement to every inode read, READ, OPEN and last-reference drop of
  every Workspace of the daemon (`namespace/compound.rs:389-391`, `lifetime/native_visit.rs:258`,
  `lifetime/file_owners.rs:51,66`). The benchmark cells never hold an orphan across commands, so
  they measure the path without it. Pre-existing in kind, and better than the sticky flag.
- `JobRows` and the READDIR window do not outlive a job or a request.

Verdict CONCERN on item 1 (untested unmount window, implicit base-reader contract); items 2-8
are documented behaviour changes for the owner to accept.

## F. Forbidden mechanisms

Checked every added line for thread/spawn/sleep/Duration/timeout/retry/spin/poll/park/Condvar/
Mutex/Atomic/channel/affinity/priority/`#[cfg`/writeback/fsync/sync_all/madvise/unsafe.

- Threads, sleeps, timers as control, spins, polling, retries, replay: none added.
- New loops, all bounded and none waiting: `'rounds` (<= 6 calls, `maintenance/orphan.rs:90-125`);
  empty-table skip (<= 12 tables, `maintenance/reclaim.rs:156-185`); slot loop of one write window
  (`payload/stream.rs:44-70`); `decode_names` (`lifetime/native_directory_read.rs:12-19`); LRU
  eviction (`workspace/src/base/cache.rs:81-94`). The fact-round loop in
  `workspace/src/operations/native_visit.rs` is the pre-existing one, moved.
- Queue, slot, buffer, timeout, lifetime constants: none raised, except the row-width items in B.
- `cfg(test)` / feature flags: none in the four crates at ca51a0c37. One `#[cfg(target_os = "linux")]`
  helper was removed (`ReadFailure::with_data_input`).
- Dependencies, vendor, patches: no change.
- fsync, kernel writeback cache, CPU pinning: nothing touched.
- Thread hand-off: a READ or READDIR decided from memory no longer leaves the receive loop
  (`fuse/src/operations/read.rs:73-87`, `fuse/src/operations/directory.rs:192-200`); it awaits the
  owner job and does no provider I/O there. No new thread.

Test-only or unread surface added to `src` (the CONCERN):
- `Overlay::explain_close_held` (`overlay/src/lifetime/close.rs:87-95`): only caller is
  `overlay/tests/close_custody.rs:134`. It follows 25 pre-existing `explain_*` functions.
- `PayloadWork::in_place_writes` / `in_place_bytes` / `in_place_ns`
  (`overlay/src/diagnostics/payload.rs:16-18`): not exported by the daemon diagnostic
  (`daemon/src/application/diagnostics/schema.rs:64-86` lists the other seven fields); the first two
  are read by `overlay/tests/costs.rs:27,161` only; `in_place_ns` has no reader at all, yet
  `write_in_place` reads the clock twice for it on every in-place write
  (`overlay/src/payload/runs.rs:169,184-186`).
- `ClientWork::file_lengths` (`workspace/src/base/client.rs:30`, `cache.rs:45`): read by tests only.
- `DirectoryFailure::offered` (`fuse/src/operations/directory.rs:68-70`): tests only; it replaces
  four removed test-only accessors.
None of these changes behaviour. Resolved by: exporting the counters through the daemon
diagnostic or deleting them, and deleting `in_place_ns` with its two clock reads.

## G. Simplicity

Net +692 production lines. The three largest additions (per-commit counts from the messages):

1. Payload run rows, e555b771f, +411 (`payload/runs.rs` 327 new lines, `payload/stream.rs` +134).
   Simpler form visible: drop the in-place branch. `write_in_place`, `writing()`, `failed()`, the
   blob handle threaded through `write_cells`/`write_whole`/`write_part`, and the three counters
   exist to turn an overwrite from 8 statements into 4 per 128 KiB window; the base case (range
   delete + one insert, `stream.rs:117-140`) already covers every overwrite correctly. Whether
   any cell exercises it is unclear: if C11's prepared file is base content (see the workload
   note), there is no local row to write into and the branch never runs. Keep it only with a
   receipt that needs it.
   Not simpler: one row shape (32 KiB cells) would enlarge partial-write merges and masks.
2. READ and OPEN as owner visits, 16fa316ee + 8ac4b4c4c, +276 (`workspace/src/operations/native_data.rs`
   194 new lines). The form itself is simpler than what it replaced, and 900231d0a removed 250
   lines. What is left is not: by 900231d0a's own list, `observe_native_attributes` with
   `NativeReadPlan`, the ports `source`/`view`/`immutable`/`observe`/`release_source`, the
   `native_read` table and its statements are unreachable from FUSE and kept for component tests.
   Simpler form: delete that path and move its tests to the visit API.
3. Inline orphan release, 989bfa887 + 36e2ec328, +206 (`maintenance/orphan.rs` +220 net).
   `finish_release` runs a round loop over the orphan and up to two released layers with a shared
   page budget, a `Step`/`Next` protocol, partial-page accounting (`dropped`, `charge`, `spent`)
   and three ways to leave work queued. Simpler form: finish inline only when the orphan has no
   lower layer and its rows fit one page (the one-cell removed file the counters are about), and
   otherwise queue exactly as before. The partial case it adds (a 56-cell file: FORGET drops 14
   cells, 5 queued steps remain) saves one step of six.

Also: `Narrowed` with `INODE_RESIZE` / `INODE_RECOUNT` (dc0c0a6f7, `namespace/inode.rs:64-82,238-296`,
`statements.rs:117-123`): no counter in the commit message is attributed to the column-subset
updates (statement counts are the same as with `INODE_UPDATE`); about 45 lines that could go
unless a measured cost justifies them. `LIMIT 9` and `ORPHANS = 7` are hand-kept duplicates of
other values. `schema.sql:1` says v25.

## Limits of this audit

Nothing was built, run or measured. Struct-size arithmetic in C is mine. Correctness of the
payload row logic was read, not proved; I did not find a wrong case. The harness, receipts and
ledger were not audited beyond `workloads.py` and the cache-class lines of `runner.py`.
