# Native directory sources and indexed cookies

> **Status:** Implemented R2 component on parent1da897903,2026-10-08.
> Native kernel dispatch, mount Ready, permissions and full drain remain open.

R7 update, 2026-10-09 (OPENDIR and RELEASEDIR, implemented; lead decision
for batch 3b: directory requests are owner visits). **OPENDIR is one owner
visit and RELEASEDIR is one owner job.**

- OPENDIR is the visit LOOKUP, GETATTR and OPEN use (`NativeJob::ObserveVisit`,
  class Read), made by `Workspace::native_opendir_visit` and run by
  [`Overlay::opendir_native_visit`](../../crates/layerfs-overlay/src/lifetime/native_visit.rs)
  in one transaction: the fence on the kernel's lookup reference
  (`FENCE_LOOKUP`), the unchanged decision (`decide_read` for `Opendir`) over
  current rows and facts carried or resident in memory, and, when it decides
  the directory, the parent it was reached through (`native_parent`), the
  `native_directory` row for the kernel request that receives it, its lease
  and the open count. It records no request source and no processing read,
  so nothing is released after the reply and a fenced or failed OPENDIR
  holds nothing. An undecided visit writes nothing; the request reads the
  fact outside the owner and visits again. A regular file is refused
  `ENOTDIR` by the decision, with nothing written.
- RELEASEDIR is `NativeDirectoryJob::Close { mount, serial, handle }` over
  `Overlay::close_native_directory`: the fence statement on the open
  directory descriptor (`FENCE_DIRECTORY`: Workspace row, mount attached and
  not revoked, this handle open on this inode), then the unchanged close in
  the same transaction. A file's descriptor, another inode's handle and a
  closed or unknown handle are Stale and change nothing; a closed Workspace
  still releases its descriptors. The port call `close_directory(mount,
  serial, handle)` is a disposal call: unlike before, a stopped fence does
  not refuse it, exactly as RELEASE. Cookie retirement is unchanged by this
  step (the queued, indexed item below).

Per request, from
[`directory_cost.rs`](../../crates/layerfs-daemon/tests/directory_cost.rs)
(jobs as Read/Lifecycle/Source; statement attempts/executions):

| Request | Before | Now |
| --- | --- | --- |
| OPENDIR of a local directory | 1/2/1 jobs, 0 grants, 4 transactions, 58/75 | 1/0/0 jobs, 0 grants, 1 transaction, 9/11 |
| OPENDIR of a base directory the daemon has seen | 2/2/1 jobs, 1 grant, 4 transactions, 64/81 | 1/0/0 jobs, 0 grants, 1 transaction, 10/12 |
| RELEASEDIR | 0/2/0 jobs, 1 transaction, 15/17 | 0/1/0 jobs, 1 transaction, 9/11 |

The description of OPENDIR's "independent reply-processing read" and of the
`directory` lookup before RELEASEDIR below is the earlier flow.

Overlay schema19 adds NativeDirectory ownership through the existing FileHandle
lease/reference mechanism, with one backed header per actual directory open.
NativeReadOperation::Opendir reuses Workspace's current evaluator and immutable
fact rounds. One final owner transaction validates the directory and acquires
its descriptor plus independent reply-processing read. Original candidates are
retained under the [native observation completion rule](73-native-read-custody.md).
No second file owner, resident visited-tree map or per-directory database appears.

[Directory ownership](../../crates/layerfs-overlay/src/lifetime/native_directory.rs)
checks engine/route/mount/serial/encoded handle together. The root's parent is
itself. A positive directory LOOKUP records its observed parent in one indexed
native_parent row, retained by existing file_custody. Directory moves emit
Changes::moved_directory only when the parent changes; Overlay validates that
value against the same final binding and updates the retained row inside the
namespace publication transaction. It does not enumerate open handles or add a
parent query to ordinary file/name changes. A new read observes the current
parent; an already received read retains its original parent observation.

[NativeDirectoryRead](../../crates/layerfs-overlay/src/lifetime/native_directory_read.rs)
owns a distinct class2 source and a backed association to its open header. It
retains metadata and cookie access after RELEASEDIR, FORGET, removal and logical
Workspace close. Release of the existing exact source also removes this
association and wakes eligible directory cleanup. A copied token creates no
owner. Retained-source queries identify the original request without replay or
adoption. Callback dispatch must keep this source and original completion credit
until all page/reply consumers finish.

A directory offset denotes a name boundary, independently of the current
membership. Offset0 starts enumeration,1 follows dot and2 follows dotdot.
Published cookies>=3 map to exact byte names for this one open. Removing or
renaming that name does not invalidate its offset; resumption is strictly after
the retained name. Foreign, unpublished and released-handle offsets refuse.
Dot entry composition belongs to the native kernel adapter using the supplied
directory/parent identities; it is not implemented by this component alone.

[Cookie preparation](../../crates/layerfs-overlay/src/lifetime/native_cookie.rs)
accepts at most64ordered names from one result window. It reuses each name's first
published cookie and reserves distinct IDs for new names. Allocator reservation
changes no enumeration cursor and creates no usable offset. The caller submits
only the prefix accepted by the native reply buffer, then attempts the reply.
Only that prefix publishes new mappings. Unused reserved IDs remain invalid
holes. Concurrent proposals may publish distinct alias cookies for the same
name; both remain valid and later proposals reuse the first indexed mapping.
No compensating deletion follows an unavailable send outcome. Prepare/publication
are each one original attempt; their input/outcome remain in caller custody after
failure, and publication of a successfully completed plan cannot repeat.

[Workspace composition](../../crates/layerfs-workspace/src/operations/native_directory.rs)
uses the existing64-key name merge through SourceView::list_from_window. One
read-only owner job supplies the current local names and local inode kinds; the
caller demands immutable names/kinds outside SQL after reader admission. Kind
demand uses the canonical inode record alone, without file length or portable
attribute requests. READDIR acquires no kernel lookup reference. Empty whiteout
pages with continuation are intermediate windows of the same request, not EOF.
An actually owned removed directory with retained zero-entry metadata returns
EOF; ordinary unowned namespace listing still refuses its removed serial.

RELEASEDIR marks its header closed, releases the descriptor lease and refuses
new reads. Existing directory sources continue. Once the last source releases,
[automatic maintenance](../../crates/layerfs-overlay/src/maintenance/native_directory.rs)
deletes at most64cookie rows per turn, then the header. This runs during a live
mount and idle activity. A partial open-header index makes native revocation's
open selection independent of closed headers awaiting cleanup. Revocation refuses
live request sources. Directories still open at a drained detach are closed by
bounded maintenance through that index; closed cookie/header rows can outlast
logical revocation. They fence physical namespace deletion until cleanup ends.
Parent rows cascade only when existing file_custody is finally removed.

[Typed directory jobs](../../crates/layerfs-daemon/src/overlay/native_directory_job.rs)
run on the existing fair SQL owner. Page/prepare/publish jobs use ordinary read
credits; small observation/close jobs use lifecycle credits. Inputs, retained
original Arc results, bounded names/kinds/cursors and reply capacity are charged.
Projected result Arcs do not replace the original Completion's credit lifetime.
No job performs provider I/O or imports fuser types.

[Component evidence](../issues/307/checks/r2-native-directory-20261008/62-results.md)
covers concurrent aliases, partial/empty accepted prefixes, old offsets after
deletion, parent moves, removed directories, empty whiteout continuation, actual
indexed plans and SQL work, live64-row cleanup and the real Daemon/Store path.
The later [native consumer](75-native-request-service.md) now wires bounded pages,
accepted-cookie prefixes, READDIR/RELEASEDIR and directory-handle GETATTR. Its
component tests include sixteen offered batches and reads surviving descriptor
close. This does not establish kernel reply-buffer behavior, native Ready/permissions,
mounted memory bounds, cold-cache eligibility, latency or complete R2 acceptance.
