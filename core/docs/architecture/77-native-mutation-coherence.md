# Native mutation and kernel coherence

> **Status:** Current general guide.

This record describes the implemented R3 composition: ordinary mutation through
a mounted Workspace and how the kernel's caches stay exact without any
notification. It builds on the [native request service](75-native-request-service.md)
and the [native mount session](76-native-mount-session.md). Mounted Commit,
forced unmount and timing qualification are not part of it. Scope, receipts,
failed attempts and open decisions are in the
[R3 completion record](../issues/307/R3-COMPLETION-20261008.md).

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
   nothing reserved.
3. [`NativeMutation`](../../crates/layerfs-fuse/src/operations/mutation.rs) drives
   the plan. Rounds that need immutable base facts read them through the shared
   reader and publish nothing. The deciding round is
   [`NativeMutationJob`](../../crates/layerfs-workspace/src/operations/native_mutation.rs):
   `NamespaceJob::decide` computes the changes, and
   [`Overlay::apply_native`](../../crates/layerfs-overlay/src/lifetime/native_mutation.rs)
   publishes them, marks the request source decided, and acquires the kernel
   custody the reply hands over, all in one transaction.
4. The reply is composed only from that job's result, attempted once, and only
   then are the publication ticket and the processing source released.

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
  releases the ticket, then the source, only after the single attempt.

A link count is the engine's namespace reference count for a file. A
directory's count is projected: 2 while it has a name, 0 once removed.

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

FLUSH, FSYNC and FSYNCDIR answer success inline with no engine or durability
work. An engine reservation refusal is `ENOSPC` with no effect.

## Accounting

Every mutating opcode is counted by opcode and disposal like the read path.
`store_units` counts WRITE units that carried the page-cache flag. The counts
are reported in the connection's drain receipt.
