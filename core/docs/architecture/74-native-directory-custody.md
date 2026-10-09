# Native directory handles and reply offsets

> **Status:** Implemented R2 component on parent1da897903,2026-10-08.
> Native kernel dispatch, mount Ready, permissions and full drain remain open.

R7 update, 2026-10-09 (implemented; lead decisions for batch 3b). **Every
directory request is owner visits that record no request source: OPENDIR one
visit, READDIR a reading visit and, when it returns names it has not returned
before, a publishing visit, RELEASEDIR one job.** Overlay schema 29 (23 on its branch) replaces
the per-name cookie rows with one row per published reply. The text below
describes this implementation; the R2 flow it replaced (a class 2 directory
source held across page, cookie plan, publication and release jobs) is in the
history of this file and in the R2 evidence linked at the end.

Owner decision applied, 2026-10-09 (overlay schema 32; supersedes D-a's last
sentence and ledger decision L4-9). **A READDIR at offset 0 on a handle that
has published replies is a rewind: its reply is published afresh and retires
every earlier reply of the handle, and an offset handed out before it is
answered `EINVAL` from then on.** POSIX leaves a `telldir` position
unspecified after `rewinddir`. Offsets handed out since the rewind are
unchanged, including a seek back to one of them and to offset 2. See
"Rewind" below.

## Decisions recorded for batch 3b

| | Decision | Where it is implemented |
| --- | --- | --- |
| D-a | An offset means "resume strictly after this name". Only names the reply buffer accepted become offsets. A handle that seeks back is answered with the reply it was given: a reply listed after the same name, whose names the present listing begins with, is returned again at its offsets, for one more SELECT per reply. Superseded in one point by the owner decision above: replies are retired at offset 0. | `offer_native_cookies`, `NativeDirectoryWindow::finish` |
| D-b | READDIR is strictly two visits: the reading visit, the request fills its reply, the publishing visit records the accepted names in one transaction, then the reply is sent. | `DirectoryStream`, `DirectoryBatch::accept` |
| D-c | No new resident structure. Offsets come from the engine's existing owner counter. Memory is bounded per in-flight READDIR and nothing is resident per open handle. | "Memory" below |
| D-d | Storage does not rise: a directory listed once stores fewer rows and bytes than before at 10 and at 4000 names. | "Storage" below |
| D-e | Past the inline bound RELEASEDIR degrades to the queued indexed retirement; nothing fails. | `close_native_directory_inner`, `retire_native_directory` |

## OPENDIR

OPENDIR is the visit LOOKUP, GETATTR and OPEN use (`NativeJob::ObserveVisit`,
class Read), made by `Workspace::native_opendir_visit` and run by
[`Overlay::opendir_native_visit`](../../crates/layerfs-overlay/src/lifetime/native_visit.rs)
in one transaction: the fence on the kernel's lookup reference
(`FENCE_LOOKUP`), the unchanged decision (`decide_read` for `Opendir`) over
current rows and facts carried or resident in memory, and, when it decides the
directory, the parent it was reached through (`native_parent`), the
`native_directory` row for the kernel request that receives it, its lease and
the open count. It records no request source and no processing read, so
nothing is released after the reply and a fenced or failed OPENDIR holds
nothing. An undecided visit writes nothing; the request reads the fact outside
the owner and visits again. A regular file is refused `ENOTDIR` by the
decision, with nothing written.

[Directory ownership](../../crates/layerfs-overlay/src/lifetime/native_directory.rs)
reuses the FileHandle lease and reference mechanism with one backed header per
open. The root's parent is itself. A positive directory LOOKUP records its
observed parent in one indexed `native_parent` row, retained by the existing
`file_custody`. Directory moves emit `Changes::moved_directory` only when the
parent changes; Overlay updates the retained row inside the namespace
publication transaction. A reading visit at offset 0 or 1 observes the parent
of its own moment; nothing retains an earlier observation.

## READDIR

An offset denotes a name boundary, independently of the current membership.
Offset 0 starts enumeration, 1 follows dot and 2 follows dotdot; the native
adapter composes the dot entries from the directory and the parent the visit
read. Offsets of 3 and above are positions in a published reply of this one
open. Removing or renaming a name does not invalidate its offset; resumption
is strictly after that name. An offset of another handle or Workspace, one that
was reserved but never accepted, and any offset of a released handle are Stale.
An offset from before the handle's last rewind is refused `EINVAL`.

**The reading visit** is `NativeDirectoryJob::Visit` (class Read), made by
`Workspace::native_directory_visit` and run by
[`Overlay::read_native_directory_visit`](../../crates/layerfs-overlay/src/lifetime/native_directory_read.rs)
with no write transaction:

1. the fence on the open descriptor (`FENCE_DIRECTORY`: Workspace row open,
   mount attached and not revoked, this handle open on this inode), which
   also returns the handle's floor;
2. the offset's name: nothing for 0, 1 and 2; a number below the floor ends
   the visit as `NativeDirectoryCursor::Rewound`, with no further statement;
   otherwise one seek to the reply row at or above the floor whose range
   holds it (`COOKIE_PAGE`);
3. the parent, only for offsets 0 and 1;
4. the directory's row and one window of at most 64 local names with their
   inode kinds (`SOURCE_NAMES_KINDS`), on the active generation and, during a
   capture, on the captured one;
5. the merge with inherited names and kinds, inside the job, when the
   canonical cache holds every object it needs. The job asks the provider
   nothing;
6. unless the job found that no name is listed, the offer of offsets: the
   latest reply of this handle at or above the floor listed after the same
   name (`COOKIE_PAGE_AFTER`) and a fresh range of 64 numbers from the
   engine's owner counter. Reserving a range writes nothing and makes no
   offset valid. At offset 0, finding such a reply makes the offer a rewind
   that reuses nothing.

When memory did not hold an inherited name or kind, the request leaves the
receive loop, takes one Store reader and merges the same window outside the
owner at the base root the visit named
([`NativeDirectoryWindow::finish`](../../crates/layerfs-workspace/src/operations/native_directory.rs)).
Kind demand uses the canonical inode record alone, without file length or
portable attributes. A window of whiteouts lists nothing and names its
continuation: the request yields and makes another reading visit, not an end.
An owned removed directory with zero entries lists nothing; READDIR acquires
no kernel lookup reference.

**The publishing visit** is `NativeDirectoryJob::Publish` (class Read) over
[`Overlay::publish_native_cookies`](../../crates/layerfs-overlay/src/lifetime/native_cookie.rs):
in one transaction, the same fence and one `native_cookie` row holding the
names the reply buffer accepted, in order. Names are checked before the
transaction (1 to 64, ordered, after the reply's start, valid as names). A
reply that reuses an earlier one, and a reply whose buffer accepted nothing,
publish nothing and submit no job. A handle released between the two visits
makes the publication Stale: nothing is written and the reply fails. The
publication is one attempt; after a failure or an unknown outcome the request
ends and nothing is repeated or compensated. Nothing is held between the two
visits or after the reply, so a fenced or failed READDIR has nothing to
release.

Two requests on one handle that list the same window each publish their own
reply; both ranges stay valid. A later request at that position reuses the
latest.

### Rewind

Offsets are numbers of the engine's owner counter, which only rises, so one
number per handle separates the replies of before a rewind from those after
it: `native_directory.floor`, 0 until the handle is first rewound.

- **What a rewind is.** The reading visit at offset 0 reads the latest reply
  at or above the floor that was listed from the start. Every reply of a
  handle descends from one listed from the start (offsets 0, 1 and 2 are the
  only ones that need no earlier reply), so that one statement, which the
  visit ran before this change too, tells whether the handle has an offset
  left. If it has, the offer is a rewind: it reuses no reply.
- **When it takes effect.** In the publishing visit of that offer, in its one
  transaction: the floor is set to the offer's fresh range, then the reply's
  row is inserted. The request publishes a rewinding offer even when its
  buffer accepted no name; then only the floor moves. A reading visit alone
  changes nothing. A rewind whose reading finds no name to offer (the
  directory lists nothing from offset 0) publishes nothing, retires nothing
  and stores nothing.
- **What it retires.** Every reply row below the floor. A request at an
  offset below the floor is refused whether or not its row is deleted yet:
  the Fuse request answers `EINVAL`, completes and retains nothing
  (`DirectoryFailure::rewound`). Offsets 0, 1 and 2 are never below a floor.
- **How the rows go.** Every publishing visit on a handle whose floor is not
  0 deletes at most `INLINE_PAGES` (8) rows below the floor in one statement
  (`COOKIE_RETIRE`: one seek and at most eight index steps). There is no
  queue item, loop or timer for an open handle. Rows still left when the
  handle is released go with it: RELEASEDIR and the closed-handle item
  delete every row of the handle, as before.
- **Bound.** A publication adds one row and, while a row below the floor
  exists, deletes at least one. So the rows of one handle never exceed the
  largest number of replies published between two of its rewinds: one
  listing, however often the handle is rewound. Before this change K rewinds
  of a directory whose early names changed stored K listings.
- **Races.** A publication whose offer was made before the floor passed its
  range is Stale and writes nothing: its reply fails as a publication on a
  released handle does. The kernel serializes READDIR on one open file, so
  this needs two requests the kernel does not send.
- **Unchanged.** A handle that is never read from offset 0 a second time has
  floor 0 and runs no statement it did not run before. A seek back to an
  offset handed out since the last rewind, or to offset 2, reuses or
  publishes exactly as before and retires nothing.

## RELEASEDIR

RELEASEDIR is `NativeDirectoryJob::Close { mount, serial, handle }` over
`Overlay::close_native_directory`: the fence on the open descriptor, then in
the same transaction the lease and open count released and one bounded window
over the handle's replies (`COOKIE_PAGES`, at most nine rows). With eight or
fewer replies they and the header are deleted there and nothing is queued. With
more, the header is marked closed and the existing indexed item is queued;
[automatic maintenance](../../crates/layerfs-overlay/src/maintenance/native_directory.rs)
deletes eight replies per turn and then the header, during live and idle
activity. A file's descriptor, another inode's handle and a closed or unknown
handle are Stale and change nothing; a closed Workspace still releases its
descriptors. The port call `close_directory` is a disposal call that a stopped
fence does not refuse, exactly as RELEASE.

A partial open-header index keeps native revocation's selection independent of
closed headers awaiting cleanup. Directories still open at a drained detach
are closed by bounded maintenance through that index with the same inline
bound; closed headers and their replies fence physical namespace deletion
until cleanup ends.

## Counts

Per request, from
[`directory_cost.rs`](../../crates/layerfs-daemon/tests/directory_cost.rs)
(jobs as Read/Lifecycle/Source; statement attempts/executions). "Before" is
the R2 flow, measured by the same test before the change
([receipt](../issues/307/checks/r7-optimization-20261009/380-dir-before-attempt1-directory_cost.txt)).

| Request | Before | Now |
| --- | --- | --- |
| OPENDIR of a local directory | 1/2/1 jobs, 0 grants, 4 transactions, 58/75 | 1/0/0 jobs, 0 grants, 1 transaction, 9/11 |
| OPENDIR of a base directory the daemon has seen | 2/2/1 jobs, 1 grant, 4 transactions, 64/81 | 1/0/0 jobs, 0 grants, 1 transaction, 10/12 |
| READDIR returning 10 local names | 3/2/1 jobs, 1 grant, 4 transactions, 114/133 | 2/0/0 jobs, 0 grants, 1 transaction, 10/11 |
| READDIR returning 64 local names | 3/2/1 jobs, 1 grant, 4 transactions, 384/457 | the same 10/11 |
| READDIR returning 64 inherited names | 3/2/1 jobs, 1 grant, 4 transactions, 192/265 | the same 10/11 |
| READDIR at an offset already answered | 3/2/1 jobs, 1 grant, 4 transactions, 323/332 | 1/0/0 jobs, 0 grants, 0 transactions, 5/5 |
| READDIR after the last name | 1/2/1 jobs, 1 grant, 2 transactions, 44/53 | 1/0/0 jobs, 0 grants, 0 transactions, 4/4 |
| READDIR at offset 0 of a handle with replies (rewind) | did not exist: 1/0/0 jobs, 0 transactions, 5/5 when the first reply was unchanged, the ordinary 10/11 when it was not | 2/0/0 jobs, 0 grants, 1 transaction, 12 attempts (two more than an ordinary reply: the floor and the bounded delete) and one execution per deleted row, at most 8 |
| READDIR that returns names on a handle with a floor | the ordinary 10/11 | 2/0/0 jobs, 0 grants, 1 transaction, 11 attempts (the bounded delete) and one execution per deleted row |
| RELEASEDIR | 0/2/0 jobs, 1 transaction, 15/17 | 0/1/0 jobs, 1 transaction, 9/12 with one reply (one execution more per reply, at most nine) |
| Maintenance after listing and closing a 10-name directory | 1 queued item | 0 items, 0 turns |

Statements per listed name fell from 5 (local) and 2 (inherited) to 0: a
reply of 10 names and one of 64 cost the same. When the cache holds nothing of
the base, a reply takes one reader and the same jobs and statements. A handle
of 63 replies (4000 names) is retired in 9 maintenance turns.

Merge note, 2026-10-09: the "Now" column is the branch's, measured before it
met overlay schema 24 (a descriptor has no `lease` row; the "lease" of the
OPENDIR and RELEASEDIR text above is the handle's reference on the file's
custody row) and the single inode read of a visit. On the merged tree
(schema 29) the same test pins OPENDIR at 8/9, local and base, and RELEASEDIR
at 8/10 with one reply and 8/11 with two; every READDIR row is unchanged.

## Storage

One reply is one `native_cookie` row (`ns`, `owner`, `first_cookie`, the name
it was listed after, and the accepted names as one byte of length and the
bytes each) and one `native_cookie_after` index entry. Before, every listed
name was one `native_cookie(ns, owner, cookie, name)` row and one
`native_cookie_name` index entry, each carrying the name. For a directory listed once
(`a_directory_listed_once_stores_fewer_rows_and_bytes_than_a_row_and_an_index_entry_per_name`
in [`native_directory.rs`](../../crates/layerfs-overlay/tests/native_directory.rs),
which builds both layouts from their DDL;
[receipt](../issues/307/checks/r7-optimization-20261009/380-dir-c2-storage-attempt2-native_directory.txt)):

| Names | B-tree entries before | now | Name bytes before | now | 4096-byte pages added before | now |
| --- | --- | --- | --- | --- | --- | --- |
| 10 | 20 | 2 | 100 | 60 | 0 | 0 |
| 4000 | 8000 | 126 | 40,000 | 24,620 | 36 | 6 |

The `native_directory.next_cookie` column and the `native_directory_read`
association table with its index and accounting triggers are removed. Schema
32 adds `native_directory.floor`, one integer per open handle and no index.
A handle that is rewound stores one listing ("Rewind" above): after five
rewinds of a 1280-name directory, each after a name was added before every
other, the handle holds 21 reply rows, where the layout without the floor
held 20 + 5 × 21. A handle that seeks back without a rewind after its
directory changed still adds one row per reply that differs; every row is
deleted at RELEASEDIR or by its bounded retirement.

## Memory

No structure is resident per open directory or per offset: handles and
replies are SQL rows. One in-flight READDIR owns one
`NativeDirectoryWindow`, charged to the owner's admission as
`NativeDirectoryWindow::CHARGE`: two local windows of at most 64 names with
kinds, the merged window of at most 64 entries, and the names of one earlier
reply, at most 64 names of 255 bytes. A publishing visit carries at most 64
names of 255 bytes; one reply row is at most 16,384 bytes of names. Offsets
are taken from the engine's owner counter, which existed before. The floor
is a column of the handle's row; the page and the offer of one in-flight
READDIR carry it as one integer and one flag.

## Jobs and evidence

[Typed directory jobs](../../crates/layerfs-daemon/src/overlay/native_directory_job.rs)
run on the existing fair SQL owner: the two READDIR visits with ordinary read
credits, observation and close with lifecycle credits. No job performs
provider I/O or imports fuser types.

Component tests of this implementation: offsets, reuse, races with RELEASEDIR,
bounded retirement, and the rewind (exact rows after repeated rewinds, refused
earlier offsets, later offsets and seeks back, a rewind whose reply accepted
nothing, offers that predate it, a second handle) in
[overlay `native_directory.rs`](../../crates/layerfs-overlay/tests/native_directory.rs);
whiteout windows, removed and moved directories in
[workspace `native_directory.rs`](../../crates/layerfs-workspace/tests/native_directory.rs);
exact counts, a cold cache, names created and removed between replies, two
handles of one directory, two mounted Workspaces listing one inherited
directory, and the cost, stored rows and refusals of a rewound handle through
the Fuse port in `directory_cost.rs`; accepted prefixes and a reply racing
RELEASEDIR in
[`filesystem_port.rs`](../../crates/layerfs-daemon/tests/filesystem_port.rs).
These are host component tests without a kernel mount; they establish no
latency and no kernel reply-buffer behavior. On a real Linux mount,
[`mounted_rewind.rs`](../../crates/layerfs-daemon/tests/mounted_rewind.rs)
drives `telldir`, `rewinddir` and `seekdir` of libc: the position from before
the rewind is `EINVAL`, one taken since resumes after its name, and nothing
is retained for the refusal.

[R2 component evidence](../issues/307/checks/r2-native-directory-20261008/62-results.md)
covered concurrent aliases, partial/empty accepted prefixes, old offsets after
deletion, parent moves, removed directories, empty whiteout continuation, actual
indexed plans and SQL work, live64-row cleanup and the real Daemon/Store path.
The later [native consumer](75-native-request-service.md) wired bounded pages,
accepted-cookie prefixes, READDIR/RELEASEDIR and directory-handle GETATTR. That
R2 evidence describes the earlier source-holding flow, in which a read survived
descriptor close; it is retained as measured and is not evidence for the
implementation above. This does not establish kernel reply-buffer behavior, native Ready/permissions,
mounted memory bounds, cold-cache eligibility, latency or complete R2 acceptance.
