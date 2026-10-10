# Native mutation and kernel coherence

> **Status:** Current general guide.

R7 update, 2026-10-09 (cleanup): `acquire_native_open_source`,
`apply_native` and `NativeMutationJob` named below are deleted; a native
mutation is one `NativeMutationVisit` (`Overlay::mutate_native_visit`) with
no request source. See [native read custody](73-native-read-custody.md).

This record describes the implemented R3 composition: ordinary mutation through
a mounted Workspace and how the kernel's caches stay exact without any
notification. It builds on the [native request service](75-native-request-service.md)
and the [native mount session](76-native-mount-session.md). Mounted Commit,
forced unmount and timing qualification are not part of it. Scope, receipts,
failed attempts and open decisions are in the
[R3 completion record](../issues/307/R3-COMPLETION-20261008.md).

R7 update, 2026-10-09 (inode statements): the mutation visit also publishes
over the inode rows its evaluation read. Each is read once in the job, a
created serial is inserted without a read, and a row the job read is
updated in place with no layer probe. See the
[overlay note](19-daemon-overlay.md).

R7 update, 2026-10-09 (statement diet): the mutation visit evaluates and
publishes over the Workspace row its fence read, and a handle-addressed
mutation over the descriptor that fence read; a read-only descriptor is
still refused with `Invalid("read-only descriptor")` before anything is
evaluated. A creating operation states its reserved serial in
`Changes::created`: the kernel lookup row of that inode is inserted without
a read (a duplicate fails and rolls back the whole job), and CREATE's lookup
reference and descriptor are one `file_custody` row write. LINK still reads
the count the kernel already holds. See the
[overlay note](19-daemon-overlay.md).

R7 update, 2026-10-09: a native mutation no longer acquires a processing
source. Steps 1 and 3 to 5 below are one owner visit
([`NativeMutationVisit`](../../crates/layerfs-workspace/src/operations/native_visit.rs)):
it resolves the kernel reference or the handle, decides over current rows
with base facts read from resident objects, and publishes with the reply's
kernel custody in one transaction. An undecided visit changes nothing; the
request reads the needed facts outside the owner and visits again, holding
nothing in between. After the reply only the publication's ticket is
released, in the engine's memory and without an owner job; a capture or a
terminal cleanup that waited for it gets one `ReplySettled` job. The serial of a creating operation is reserved
before the first visit. See
[native read custody](73-native-read-custody.md) for the visit rules.

## One engine, one publishing job

There is no native mutation engine. Every mutating kernel request becomes one
Workspace [`MutationPlan`](../../crates/layerfs-workspace/src/mutation/mod.rs)
over the existing [operations](30-namespace-operations.md), decided by one
publishing Overlay owner job:

1. The request acquires an independent processing source under the kernel
   reference that protects it (the parent directory, the target inode, or a
   handle resolved in the same job by `acquire_native_open_source`).
2. A creating operation reserves its inode serial from the Store through the
   existing allocator. A contended allocator writer is a definite `EAGAIN` with
   nothing reserved. Since R8b (2026-10-10) a create that leaves fewer local
   serials than the daemon's explicit low-water also makes one early
   reservation; see "Early serial refill" below.
3. [`NativeMutation`](../../crates/layerfs-fuse/src/operations/mutation.rs) drives
   the plan. Rounds that need immutable base facts read them through the shared
   reader and publish nothing. The deciding round is
   [`NativeMutationJob`](../../crates/layerfs-workspace/src/operations/native_mutation.rs):
   `NamespaceJob::decide` computes the changes, and
   [`Overlay::apply_native`](../../crates/layerfs-overlay/src/lifetime/native_mutation.rs)
   publishes them, marks the request source decided, and acquires the kernel
   custody the reply hands over, all in one transaction.
4. The reply is composed only from that job's result, attempted once, and only
   then are the publication ticket and the processing source released. Both
   are released by one owner job in one transaction
   (`Overlay::reply_attempted_and_release`): either both are recorded or the
   request keeps both. A mutation that published nothing releases its source
   alone.

**Early serial refill (R8b, 2026-10-10; owner ruling P-1).**
`DaemonLimits.serial_low_water` is an explicit value of the private
configuration record; startup refuses one at or above the 1,024-serial refill
window, and 0 makes no early attempt. Application assembly gives it to the
Store once, before any Workspace is served. `reserve_serial` calls
[`StorePorts::take_serial`](../../crates/layerfs-daemon/src/store/ports.rs),
which passes it to `Workspace::next_serial_with_low_water`
([operations](30-namespace-operations.md)). The early attempt runs on fresh
ports over the same scope, created only when it is made, so its failure is
never retained as the request's own first Store failure and the request's
later base demand is still admitted. Its outcome is decided in
[`reserve_serial`](../../crates/layerfs-daemon/src/service/filesystem_port.rs):
writer contention is that attempt's before-effect refusal and the create
continues with the serial it took; any other early failure is that create's
original error, its serial consumed, and, exactly as for the exhausted path,
the request service retains it, so the mount then serves nothing further and
its unmount stops at retained requests. Nothing is replayed, waited for
or timed: a later create is a new operation with its own single attempt, and a
create that finds no local range and whose one attempt is contended is still
`EAGAIN` with no effect. A Workspace's first create always finds no range.
While a peer holds the Store writer, each create below the low-water makes one
refused write attempt on its request thread. Proofs:
`layerfs-workspace/tests/namespace.rs` (range arithmetic),
`layerfs-daemon/tests/mounted_low_water.rs` (real mount, external writer) and
`layerfs-daemon/tests/observed_application.rs` (configured value through the
executable). No deployed value is selected here; harnesses and examples set 0.

`Overlay::apply` is the same checked body inside its own transaction;
`apply_native` adds the native effects to it. A second publication through one
request source is refused. The attempt is never replayed: an uncertain or failed
publishing job retains the request, its source and any ticket as
`RequestDisposition::Retained`, which fences that mount's admission.

| Reply carries | Acquired in the publishing transaction |
| --- | --- |
| An entry (MKNOD, MKDIR, SYMLINK, LINK) | One kernel lookup reference; for a directory, its retained parent |
| An entry and a descriptor (CREATE) | The lookup reference and an open handle of the requested access mode |
| Attributes, a count or nothing | Nothing beyond the publication |

Because custody is acquired with the publication, an unobservable reply never
needs a guessed compensating release: the kernel either learned of the
reference and will FORGET/RELEASE it, or the connection's drain-qualified
revocation retires it.

## Operations

| Kernel request | Workspace operation | Notes |
| --- | --- | --- |
| OPEN for writing | open handle with write access | `O_TRUNC` arrives as OPEN followed by a size-0 SETATTR without a handle |
| WRITE | `WriteOpen` at `Position::At(offset)` | Always the kernel's offset; the kernel resolves `O_APPEND` under its inode lock. The reply is the full count or an error |
| WRITE with the page-cache flag | `StoreOpen` | A shared-mapping store. Clipped to the current size and never an extension; still answered in full |
| SETATTR | `SetAttributes`, or `SetOpenAttributes` for a size change through a handle | Mode, size and modification time. Access time is not stored |
| CREATE, MKNOD (regular) | `Create` | CREATE replies its descriptor from the same transaction |
| MKDIR, SYMLINK, LINK | `Mkdir`, `Symlink`, `Link` | A symlink target is at most one page minus one byte |
| UNLINK, RMDIR | `Unlink`, `Rmdir` | Exact emptiness; an inherited name becomes a whiteout where needed |
| RENAME, RENAME2 | `Rename` | Replacement and no-replace |

A cross-parent directory rename must prove the destination is not inside the
moved directory. A kernel request carries no path, so the publishing job reads
the connection's retained parent chain
([`native_ancestors`](../../crates/layerfs-overlay/src/lifetime/native_mutation.rs)):
indexed point reads of rows recorded at LOOKUP, MKDIR and directory moves,
bounded by the canonical depth. A missing step is `Stale`, never an assumed
root. The non-native path still requires the caller's destination path.

## Coherence without notifications

No notification or invalidation is sent, and kernel writeback caching is not
negotiated. Every change to a mounted view arrives as a request on its own
connection, so the kernel updates its caches from the reply:

- **Publish before reply.** A reply is attempted only after its publication is
  known. A lost reply does not undo the mutation.
- **Attributes come from the publishing job.**
  [`coherence::attributes`](../../crates/layerfs-fuse/src/coherence/attributes.rs)
  refuses to compose a reply from request inputs or from a different inode's
  result. Change time is reported equal to the one stored modification time.
- **Pages.** Opens keep previously cached pages and never bypass the cache
  ([`coherence::pages`](../../crates/layerfs-fuse/src/coherence/pages.rs)).
  A size change makes the kernel drop the affected pages itself.
- **Reply order.** [`coherence::reply_order`](../../crates/layerfs-fuse/src/coherence/reply_order.rs)
  releases the ticket and the source, in one owner job, only after the
  single attempt.

A link count is the engine's namespace reference count for a file. A
directory's count is projected: 2 while it has a name, 0 once removed.

R7 update, 2026-10-09 (directory link counts, overlay schema 22): the
sentence above is superseded for a named directory. A directory reports
**2 plus the number of its child directories**, as a native filesystem does,
and still 0 once removed; the root, which has no namespace reference and is
never removed, follows the rule of a named directory.
[`Identity::attributes`](../../crates/layerfs-fuse/src/attributes.rs) adds 2
to `ViewStat::subdirs` with a checked conversion (`EOVERFLOW` beyond 32 bits),
so LOOKUP, GETATTR, MKDIR, SETATTR and every other reply that carries
directory attributes report one value. The count is the inode row's `subdirs`
column, moved by the transaction that changes a binding
([namespace operations](30-namespace-operations.md)); for a directory with no
local row it is derived from the base and remembered
([effective view](29-effective-base-view.md)). It is never the reference
count: a directory's `nlink` stays 1, the root's 0, as the canonical grammar
requires. No notification is sent: after mkdir, rmdir and rename the kernel
drops the attributes of the parents it changed by itself. The mounted proof is
[`mounted_links.rs`](../../crates/layerfs-daemon/tests/mounted_links.rs).

## Removed references

Kernel lookup custody, open handles and processing sources are independent
owners; a removed inode and its content are retained until the last of them is
released.

- An open-unlinked file keeps exact content and accepts writes and truncation
  through its handle.
- A removed working directory or `O_PATH` reference returns exact attributes
  with link count 0 from lookup custody alone, after every open handle is gone.
- **A removed file can still be opened** by a request the kernel protects with a
  live lookup reference. The kernel resolves a path to an inode and then sends
  OPEN for it; an unlink or replacement rename can land between the two. The
  open yields the old file, so a reader of an atomically replaced name sees the
  old or the new file and never an absent one. This reverses the R2 rule that a
  native OPEN required linked metadata; the general non-native `open_file`
  still refuses a removed inode.

A SETATTR that names no handle (for example `fchmod` on an open-unlinked file)
is still addressed by inode and is refused `ENOENT` once the inode has no name.

## Refusals that remain

Decided before any engine custody
([`unsupported`](../../crates/layerfs-fuse/src/operations/unsupported.rs)):

| Request | Answer |
| --- | --- |
| Extended attributes, ACCESS, READDIRPLUS, locks, BMAP, IOCTL, POLL, LSEEK, FALLOCATE, COPY_FILE_RANGE | `ENOSYS`; the kernel stops asking on this connection and uses its own fallback where it has one |
| MKNOD of a FIFO, socket or device | `EPERM` |
| Set-user-id, set-group-id; a sticky bit on a regular file | `EPERM` |
| Ownership other than the configured command identity | `EPERM`; naming that identity succeeds and changes nothing |
| Hard link to a directory or symlink | `EPERM` |
| Exchange and whiteout renames | `EINVAL` |
| A name over 255 bytes; a symlink target of a page or more | `ENAMETOOLONG` |

FLUSH, FSYNC and FSYNCDIR are answered `ENOSYS` once per connection (R7,
2026-10-09), with no engine or durability work; the kernel reports success to
the caller of that and every later `close`, `fsync` and `fdatasync` and sends
the request no more. An engine reservation refusal is `ENOSPC` with no effect.

## Accounting

Every mutating opcode is counted by opcode and disposal like the read path.
`store_units` counts WRITE units that carried the page-cache flag. The counts
are reported in the connection's drain receipt.
