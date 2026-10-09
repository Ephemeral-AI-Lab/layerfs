# Native read observation and lookup custody

> **Status:** Implemented R2 component on parent78374ced6,2026-10-08.
> Native Fuse activation, Ready and normal drain remain open.

R7 update, 2026-10-09 (READ and READLINK): **file data and symbolic-link
data are served by one read-only owner visit that records nothing.** The job
is `NativeJob::ReadVisit` (class Read) over
[`Overlay::read_native_visit`](../../crates/layerfs-overlay/src/lifetime/native_visit.rs)
and Workspace's
[`NativeDataVisit`](../../crates/layerfs-workspace/src/operations/native_data.rs).
It runs no transaction and writes no row:

1. The fence statement, on the descriptor for READ (`FENCE_HANDLE`) and on the
   kernel's lookup reference for READLINK (`FENCE_LOOKUP`), with the outcomes
   of every other visit.
2. Once the engine has created its first orphan, one probe for an orphan row
   of the inode. An orphan is read from its own generation and names the base
   root the orphan retained; a removed regular file reached without a
   descriptor is `Missing`.
3. Otherwise the inode's current layers (`read_layers`, the ordinary read):
   the local row and the local cells of the requested window, at most the
   existing 128 KiB read window, are copied inside the job. Parts the local
   state does not cover are reported as inherited spans, not read. The window
   names the Workspace's base root of that moment.
4. When no local row exists and the memory-only client is bound to that same
   root, the job also takes the inode's base value from resident canonical
   objects and, for a regular file, the length the cache remembers
   ([file lengths](24-file-lengths.md)). A fact that is not resident is left
   absent; it is not an error and nothing is asked of a provider.

The result is a
[`NativeWindow`](../../crates/layerfs-workspace/src/operations/native_data.rs):
the base root, the local window if a local row exists, and the resident base
value and length otherwise. After the job the request holds nothing in the
owner. [`NativeData::read`](../../crates/layerfs-fuse/src/operations/read.rs)
then replies in one of three ways:

| Window | Hand-off | Store reader | Bytes |
| --- | --- | --- | --- |
| A local regular file with no inherited span | none | none | replied from the job's completion, not copied again |
| No local row, and the offset is at or after a length known from memory | none | none | empty |
| Anything inherited: a local window with a span, a base file below or without a known length, a base link | one `LeaveReceiver` | one, through `RequestServices::base` | `NativeWindow::finish` over that reader |

In the third row the request first takes its own copy of the window and
drops the completion, so no owner credit is held across the reader wait or
the Store read. `finish` binds the base to the root the visit named
(`BaseView::at`: the current view when the roots are equal, a rebind
otherwise), reads the inode value if the job did not have it, and composes
the window with the unchanged Workspace algorithm (`file_window`,
`link_target`, `BaseView::plan_file`). A kind the operation does not serve is
the same refusal as before: `EISDIR` for READ of a directory, `EINVAL` for a
READ of a link or a READLINK of anything else, `ENOENT` for a serial the base
does not have. No job follows the reply.

What a READ no longer records, and what stands in its place:

- No `native_source`, `base_source`, `lease`, `file_read` or `native_read`
  row and no `base_readers` count. A READ therefore does not hold back a
  prepared-base install or a revoke, and there is nothing to release after
  its reply or after a failure.
- The bytes are those of one base root. The local part was copied inside one
  job; the inherited part is read from the immutable content of the root that
  job named, whatever is installed before the read. An install between the
  visit and the Store read costs a rebind of the view and never a mix.
- A file unlinked while open still reads its own bytes: its orphan generation
  and the root the orphan retained are found by step 2, through the
  descriptor's own engine rows, which RELEASE alone removes.
- The request is accounted by the dispatcher lane alone. It stays `admitted`
  from its receive until its future ends, whether it is parked for a reader
  or inside a Store read. Normal Unmount makes the kernel's reversible detach
  first, which answers busy while a caller is blocked in the READ, and then
  requires the lane's drain (`received == 0 && admitted == 0`) before
  `Revoke` is submitted; Force stops the fence, so a READ parked for a reader
  ends with `ENOTCONN` at the reader gate holding nothing, and one inside a
  Store read runs to its own result while the drain waits for it
  ([mount session](76-native-mount-session.md#revocation-after-detach)).
  `revoke_native_mount` still refuses on a remaining source or read
  association of the requests that have one; it cannot see a READ, and the
  drain before it is what covers a READ.
- A failed Store read is `BaseDemandFailed` for that request (`EIO`), with
  nothing to give back; the mount keeps serving
  ([request service](75-native-request-service.md#failed-base-demand)).

Per request, exact, from
[`read_cost.rs`](../../crates/layerfs-daemon/tests/read_cost.rs) (owner jobs
as Read/Lifecycle/Source, then reader grants, write transactions and
statements as attempted/executions):

| Request | Before | Now |
| --- | --- | --- |
| READ of base bytes | 3/2/1 jobs, 2 grants, 4 transactions, 68/83 | 1/0/0 jobs, 1 grant, 0 transactions, 2/2 |
| READ of local bytes | 2/2/1 jobs, 1 grant, 4 transactions, 63/78 | 1/0/0 jobs, 0 grants, 0 transactions, 3/3 |
| READ of a window with local and inherited bytes | 2/2/1 jobs, 1 grant, 4 transactions, 63/78 | 1/0/0 jobs, 1 grant, 0 transactions, 3/3 |
| READ at or after a length known from memory or the local row | as the first two rows | 1/0/0 jobs, 0 grants, 0 transactions, 2/2 |
| READLINK of a base link | 3/2/1 jobs, 2 grants, 4 transactions, 68/83 | 1/0/0 jobs, 1 grant, 0 transactions, 2/2 |
| READLINK of a local link | 2/2/1 jobs, 1 grant, 4 transactions, 63/78 | 1/0/0 jobs, 0 grants, 0 transactions, 3/3 |

Once the engine has created an orphan, every READ and READLINK attempts one
more statement, the orphan probe. A READ of an orphan reads its cells from
the orphan generation and, until background maintenance has moved them
there, from the generation they were written in: one or two cell statements.
With no canonical cache allowance a READ is still one job and one grant; a
read at the end of a base file then costs the grant too, because no length
is remembered.

Limits of this record. The copy in the third reply row is one bounded
allocation per in-flight request (the window, at most 128 KiB, and its span
list), released with the reply; no resident structure was added. The
in-job lookup of step 4 walks the base inode tree in resident objects on the
owner thread for every READ of a file with no local row. Nothing keeps the
objects of a replaced base root for a READ that has not finished: this
relies on the Store never deleting an object, which holds today because no
collection exists; a future collector needs its own rule for in-flight
readers. OPEN and OPENDIR still record a source and a processing read, as
described below, and `NativeReadOperation::Data` is no longer reached by a
filesystem request.

R7 update, 2026-10-09 (statement diet): the visit's three checks below (live
Workspace, attached mount, the kernel's reference) are **one statement**,
`Overlay::native_fence`, with the same outcomes: `Closed` for a closed
Workspace, `Stale` for a missing Workspace, a foreign or revoked mount, or a
reference that is not held. RELEASE (`close_native_file`) uses the same fence
without the `Closed` refusal, so a closed Workspace still releases its
descriptors, and it does not read the descriptor or the Workspace row a
second time. The orphan-domain probe before an inode read is skipped until
the engine has created its first orphan. See the
[overlay note](19-daemon-overlay.md).

R7 update, 2026-10-09: **LOOKUP, GETATTR and every native mutation are
served by owner visits that record no request source.** One visit is one owner
job ([overlay](../../crates/layerfs-overlay/src/lifetime/native_visit.rs),
[workspace](../../crates/layerfs-workspace/src/operations/native_visit.rs)):
it checks the live mount and the kernel's own reference on the inode or
descriptor the request names, decides over current rows, and, when it
decides, takes the reply's kernel custody or publishes in the same
transaction. The job is the request's whole window in the owner: install,
revoke, close and reclamation are owner jobs too and cannot run inside it, so
it writes no `native_source`, `base_source`, `lease(5)` or reader count and
there is nothing to release after the reply. Inside the job the base is a
turn-local source (class 3) that names no row; it is accepted only while the
Workspace still has the same base root and install frontier.

Base facts a visit needs are read inside the job from canonical objects
already resident in the Store's cache, through a memory-only client that never
asks a provider and copies no object above 64 KiB. A fact that is not
resident leaves the visit undecided and unchanged. The request then reads it
outside the owner, from the Workspace's current base over an admitted Store
reader, tags it with that base root and visits again. A visit uses carried
facts only of the base the Workspace has at that moment, so an install
between two visits costs another read and never a stale answer; a base that
answers the same needs twice is reported as `BaseChanged` rather than read
again. Between visits the request holds nothing in the owner. Consequences:
`base_readers` no longer counts these requests, so they do not hold back a
prepared-base install; a revoke between two visits is not refused on their
account, and the next visit fails `Stale`; the kernel's reference keeps the
parent or target inode as before. OPEN, OPENDIR and directory enumeration
still record a source, as described below; file data and symbolic-link data
do not (see the update above).

R7 update, 2026-10-09 (directory link counts): the fact of a base directory
now includes its child-directory count, which is derived, not stored
([effective view](29-effective-base-view.md)). Inside a visit the memory-only
client answers it only from the remembered counts; it never lists. A count
that is not remembered is one more fact that is not resident: the visit is
undecided and unchanged, the request derives the count outside the owner and
carries the complete inode fact to its next visit, so an eviction between the
two visits cannot make it ask again. An empty directory needs no count.

R7 update, 2026-10-09 (file lengths): the length of a base regular file is
one more such fact. The memory-only client has no length port and answers it
only when the canonical cache remembers the Store's answer for the file's
content root ([file lengths](24-file-lengths.md)). The first LOOKUP or GETATTR
of a base file in a daemon is therefore undecided once, reads the length in
the one reader grant of its fact round and visits again; every later one is
one visit with no reader grant. An evicted answer costs that second visit
again and nothing else.

Overlay schema17 adds one engine-minted NativeMount per Workspace and indexed
native_lookup, native_source and native_read associations in the existing Overlay.
The mount retains its authenticated root serial. Its implicit root lookup owner
is separate from explicit kernel nlookup; count-zero FORGET does not discard it.
Repeated positive lookups increment one checked count for the same inode and keep
one existing LookupOwner lease. Hard-link aliases therefore share inode custody.
There is no resident namespace/inode map or total lookup limit.

[Native sources](../../crates/layerfs-overlay/src/lifetime/native.rs) bind the
full engine/route/mount/request identity. Request keys preserve all64bits in an
eight-byte blob. Nonrecycled engine IDs occupy BaseSource kind2, distinct from
caller sources(kind0) and independent FileRead sources(kind1). Acquisition also
retains an independent FileReader lease on the lookup parent or getattr target.
That lease protects current metadata while immutable facts are demanded, even
when FORGET and unlink happen meanwhile. Exact source release removes both the
association and its lease. Copying a capability creates no ownership.

[Workspace NativeReadPlan](../../crates/layerfs-workspace/src/operations/native_read.rs)
uses the existing Eval/BaseFacts/Need semantics. Owner rounds perform bounded
current-row decisions and request missing immutable facts; Base rounds run those
demands outside SQL. Final positive lookup selects its inode and increments
nlookup in one atomic transaction. Getattr uses the same observation without
adding a kernel lookup. Both replies carry attributes only, so neither retains
a FileRead (`Overlay::observe_native_attributes`): the request's source and,
for a lookup, the kernel reference taken in the same transaction are the only
owners. The `Data` operation was the target of READ and READLINK until the
R7 update above; it runs the same decision and retains the independent
FileRead its bytes are served from, as open and opendir do. The existing source protects removed metadata,
so a previously received getattr can report nlink0. A final negative/refusal
is marked decided and acquires no FileRead. An original request cannot perform
a second final decision.

[NativeObservation](../../crates/layerfs-overlay/src/lifetime/native_observation.rs)
retains the original semantic decision separately from transaction completion.
A minted candidate is preserved even if association insertion or completion
fails. Only resultOk proves usable acquisition. A candidate from a failed or
uncertain attempt is evidence, never permission to adopt, release, replay or
send success. Retained-source/read point APIs observe original custody after
loss without resolving that uncertainty. Internal FileRead requests use negative
engine-owner keys; existing public positive request keys remain disjoint.

[NativeJob](../../crates/layerfs-daemon/src/overlay/native_job.rs) carries these
operations on the existing fair SQL owner. Observe returns the complete original
outcome in an Arc; input/facts/output capacity is charged with the original
completion. An asynchronous consumer must retain that Daemon Completion alongside
any projected Arc until output/consumer disposal. Owner execution performs no
provider I/O. NativeReadPlan::supply requires an already admitted immutable reader;
this component does not wire a native dispatcher or move blocking Store admission
onto its future worker pool.

After acknowledged detach and complete consumer drain, revoke_native_mount
refuses any remaining native source/read association before effect, then revokes
admission and schedules indexed retirement. Automatic maintenance first retires
file and directory handles the aborted connection never released, at most 64 per
turn, then removes at most
64lookup groups per turn, including the root group, using an ordered serial
cursor. Existing orphan and closed-Workspace cleanup follows exact last release.
The mount row itself fences whole-namespace cleanup until retirement ends. Open
file/directory owners and kernel detach/drain still need integration and cannot
be inferred from this source/read fence. Reply-send knowledge remains the
unavailable outcome selected in S8; no compensating FORGET is invented.

The [component receipts](../issues/307/checks/r2-native-lookup-20261008/35-results.md)
cover exact repeated counts, foreign/stale identity, independent held sources,
lookup races with unlink, hard-link identity, full-width request keys, rollback
decision retention, automatic bounded retirement and the real Daemon/Store route.
Actual indexed plans are paired with DatabaseWork; they do not establish native
kernel behavior, latency, cold cache eligibility, RSS bounds or full R2 acceptance.

Schema18 extension on parentcc0b9a06a retains regular native file handles through
the existing OpenFile capability. NativeReadOperation::Open uses the same fact
rounds and current evaluator; its final transaction validates a linked regular
inode, retains OpenFile and an independent processing read, and records the
mount/request association. Original open candidates are retained before later
association/read/commit failure, under the same no-adoption completion rule.

[Native file ownership](../../crates/layerfs-overlay/src/lifetime/native_file.rs)
validates the engine/route, mount incarnation, serial and encoded nonrecycled
OpenFile owner ID together. Separate opens have separate owners and retain their
own writable flag. After FORGET/unlink, an actual handle may acquire an independent
native request source. Later RELEASE does not invalidate that source or its read
window. No new file token hierarchy, resident inode map or second close algorithm
is introduced. NativeJob exposes File/FileSource/RetainedFile/CloseFile through
the same SQL owner and completion credit.

The native_file association references the exact file_handle row. Exact close
through either public path deletes the association atomically through a foreign
key cascade; its owner index prevents a namespace scan on ordinary file close.
Native file request keys occupy the negative internal engine-owner domain while
full64bit original kernel request keys remain in native_file. Public caller
open request IDs remain positive. Mount revocation no longer refuses remaining
native file owners: a detached connection may never deliver their RELEASE, so
they are retired in bounded indexed turns after revocation. The qualifying
precondition is complete connection drain, described in
[native mount session](76-native-mount-session.md#revocation-after-detach);
an empty source/read set alone is insufficient.

The [native-open component receipts](../issues/307/checks/r2-native-open-20261008/25-results.md)
extend the public engine, canonical Workspace and actual Daemon tests for these
transitions. Schema versions and earlier proof identities above are historical;
this in-process Overlay is freshly initialized, with no migration or reopen path.

Schema19 subsequently adds [native directory ownership and cookies](74-native-directory-custody.md).

The replacement [native request service](75-native-request-service.md) now drives
these plans through actual Owner futures and admitted direct Store readers.
Its initial callback adapter does not yet establish mounted application Ready
or normal detach/drain; the earlier component receipts retain their original scope.
The earlier component limitations above retain their original scope; kernel
integration and complete R2 acceptance remain open.
