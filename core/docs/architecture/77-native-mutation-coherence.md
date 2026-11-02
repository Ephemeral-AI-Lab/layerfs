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
3. [`NativeMutation`](../../crates/layerfs-mount-ports/src/operations/mutation.rs) drives
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

**Known capacity exhaustion (issue 319, 2026-10-12).** A physical reservation
refusal before BEGIN and a direct SQLite Full returned by the atomic native
visit after confirmed rollback both answer `ENOSPC`, with no published
filesystem mutation or ticket. The latter still pays attempted SQL work and
actual transient allocation observations; rollback cannot erase the event
peak. The original completion remains owned through the single error reply,
then normal reply disposal releases it. The mount continues serving, and both
normal and forced terminal drain remain available. The Overlay error predicate
identifies direct SQLite Full only; it does not establish rollback knowledge.
The atomic visit supplies that knowledge. An uncertain COMMIT/rollback result,
a provider ambiguity or an existing publication is never reclassified from a
nested capacity cause: it retains the original failed request and `EIO` outcome.
This is current SQL-format capacity behavior, not qualification of the retired
remote ShortAppend or one-shot NoSpace instrumentation cases.

**Early serial refill (R8b, 2026-10-10; owner ruling P-1).**
`DaemonLimits.serial_low_water` is an explicit value of the private
configuration record; startup refuses one at or above the 1,024-serial refill
window, and 0 makes no early attempt. The runtime and the SDK examples deploy
256 (2026-10-10, owner decision C-4). Application assembly gives it to the
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

## Recognition before the first visit

Added 2026-10-11 for [#316](https://github.com/Ephemeral-AI-Lab/layerfs/issues/316),
phase 2. A WRITE without the page-cache flag passes one request port,
`recognize`, before its first owner visit. The port returns the same bytes,
with [claims](31-payload-streams.md) where whole committed chunks of the
file's own committed content were recognized in them; the publishing job then
stores those ranges as reference pieces and every other byte as cells. The
kernel request, its reply and the operation are unchanged, and nothing but the
bytes of ordinary WRITE requests creates a claim.

The [recognizer](../../crates/layerfs-workspace/src/operations/file/recognize.rs)
reads the write's bytes and mapping pages of the committed file. It reads no
overlay row, holds no SQL writer and fetches no committed chunk.

- **Gate.** Nothing is done for a write below one minimum chunk (8 KiB), for
  a file with no committed content, or for committed content that is not in
  chunks. A committed length below the smallest whole-file threshold decides
  that without reading the content root. The answer is remembered for the
  base it was given for.
- **Follow.** Once a shift (file offset less source offset) is known, the
  committed extents that fall wholly inside the request under that shift are
  compared one by one: the bytes are hashed with Content's chunk identity and
  the identity is compared with the extent's. Equal identity is the only
  proof; an extent that is a slice of a chunk never matches. Two consecutive
  differing chunks end the run.
- **Search.** With no shift, or when the known one matched nothing in the
  request, the first 64 KiB are scanned once for cut candidates in both pair
  alignments. A committed extent is tried only where its length fits between
  two candidates; extents are taken from after the last claim, or from
  512 KiB before the written offset, 64 at a time. A chunk cut at the maximum
  size has no candidate at its end and is left to a followed run. At most
  eight identities are computed in one search and one search runs a request.
- **Forward sources.** A new shift is accepted only when it keeps the source
  moving forward after the last claim. Other bytes are stored.
- **Budget.** After 1 MiB of consecutive requests that claimed nothing, a
  file's writes are no longer examined while its state is remembered.
- **Carry.** Added in phase 3. When a followed run reaches a committed chunk
  that begins in the request and continues past its end, the identity of the
  chunk's first bytes is kept in the file's state: a streaming hash, never
  the bytes. The request that begins exactly where that one ended supplies
  the rest; if the finished identity is the committed chunk's, its claim
  begins before its own bytes, over the part the earlier request stored (and
  over the end of that request's claim that was not whole cells), at most
  one chunk (32 KiB) back and never before that request. The claim names the
  revision of the publication that stored those bytes, which the port
  records when the earlier write's job is applied; the engine honours the
  part before the request only while the file's content sequence is still
  that revision, and otherwise stores it as it was. So the pieces of an
  unbroken tail are contiguous in file and source and extend one row. A
  carried chunk serves the next request or none: a request elsewhere, a
  shorter one than the rest of the chunk, an unpublished or failed write and
  any other publication to the file all drop it, at the cost of one chunk
  stored. A write below 8 KiB is still never examined.
- **Carry in falling offsets.** A program that grows a file in place moves
  its tail from the end backward, so its requests arrive in falling offsets
  and the chunk between two requests is cut the other way round: its end is
  written first. A request whose followed run begins right after such a
  chunk keeps the chunk's last bytes, its own first bytes, at most one chunk
  (32 KiB). The request that ends exactly where that one began holds the
  chunk's beginning; the identity of both parts is the proof, and the claim
  then ends after the request, in the bytes the earlier request stored,
  under the same revision rule. A request completes a chunk at its beginning
  or at its end, not both, because a claim names one earlier publication.
- The claims of one request are at most 18 ranges and adjacent chunks share
  a range.

A pass first runs over the Store's memory-only client. When a page or the
file's length is not in memory it records nothing and is repeated once over an
admitted reader, in demand custody of its own: a failed read there is a
missed claim and never the request's failure. Hashing and page reads run
after the request has left the receive loop. A miss of any kind, a stopped
budget and an evicted state all store the write exactly as before; only a
stopped fence ends the request, as it would at the visit.

Per-file state is fixed in size and holds no bytes: the base it belongs to,
the gate's answer, the shift, the last claim's range and the unmatched
count, and at most one carried chunk in each direction: for rising offsets
its extent, the hash state and the revision, and for falling offsets the
chunk's last bytes as the earlier request wrote them, at most 32 KiB. These
are bytes of a write in flight between two requests, not a cache of
committed content. The daemon keeps it in
[64 slots shared by every Workspace](61-direct-store-adapter.md), least
recently used first out.

Checked by
[the Workspace test](../../crates/layerfs-workspace/tests/recognize.rs), by
[the daemon test](../../crates/layerfs-daemon/tests/recognizer.rs) through the
request port, the product Commit and a fresh bind, and by
[the mounted proof](../../crates/layerfs-daemon/tests/mounted_recognizer.rs),
in which a process launched outside the daemon shifts two tails with `dd`:
with the carry each tail is one or two piece rows. Before the carry every
request left one stored stretch between two pieces, and the Commit paid one
edit for each; both states are measured in the
[progress ledger](../issues/316/PROGRESS.md).

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

#327 update, 2026-10-15 (merged install): the one exception to "no
notification" is a Commit that installs a root other than the one the
Workspace showed (merge-on-commit, clean or with markers). That change does not
arrive as a request on the connection, so the daemon invalidates the changed
inodes and rebound names before the Commit's answer, as the FUSE contract
requires for an install that is not identical. See
[architecture 81](81-compare-and-merge.md#merge-on-commit), step 6. An ordinary
Commit installs the Workspace's own captured view and still sends nothing.

#359 update, 2026-10-18 (negative entries): a LOOKUP whose name is `Missing`
is answered with an entry of node id 0, attribute lifetime 0 and the usual
60 s entry lifetime ([`reply.rs`](../../crates/layerfs-fuse/src/request/reply.rs)),
which the kernel keeps as a negative dentry instead of asking again on every
probe. Other refusals stay errors (`ENOTDIR`, `EACCES`, …). A negative entry
takes no lookup reference, so there is nothing to forget or drain. It needs no
new notification:
- every create, mknod, mkdir, symlink, link and rename of the mount reaches the
  daemon as a request and replaces the negative dentry in the kernel;
- a merged install already notifies every name that differs in a directory
  present in both roots, added names included (`binding_changes`);
- an in-flight LOOKUP decided before that install holds the parent's `i_rwsem`
  shared until its dentry is hashed, and the kernel's entry invalidation takes
  it exclusively, so the notification drops the new negative dentry too;
- an ordinary or known install does not change the view.

The mounted proof is
[`negative_entries.rs`](../../crates/layerfs-daemon/tests/negative_entries.rs).

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
([`unsupported`](../../crates/layerfs-mount-ports/src/operations/unsupported.rs)):

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
`store_units` counts WRITE units that carried the page-cache flag.
`largest_read` and `largest_write` are the largest READ size and WRITE data
length the kernel has sent on the connection (2026-10-10, owner decision C-2):
the observed request maxima beside the negotiated 131072. The counts are
reported in the connection's drain receipt.

Issue 319 update, 2026-10-11 (after harness checkpoint `b97857b7f`): the first
follow established by a search now visits the committed extent holding the
request's first byte, before the whole match where following begins. It retains
that leading extent only when it crosses the request's beginning; it makes no
claim for it. This supplies the existing backward carry with the earlier
request's own tail bytes. The next adjacent request still has to complete the
whole chunk identity, and Overlay still checks the earlier publication revision
before accepting a claim beyond the request. A changed shift clears a previous
unclaimed leading extent. No committed payload is fetched, and the slot, search,
hash and claim budgets are unchanged.

The added traversal covers only the prefix of this bounded request before the
search's first match, at most the request window, plus its leading extent and
mapping path. It does not scan the file prefix or retain another chunk. Initial
search previously skipped that leading extent, leaving an unclaimed chunk at
the first backward request boundary far from the actual edit. The external
[retention regression](../../crates/layerfs-daemon/tests/recognizer_diagnostics.rs)
uses the exact benchmark fixture and request pattern, verifies every committed
byte and asserts that original chunks away from the edit remain named. This
source description is separate from the [issue 319 measurements](../issues/319/PROGRESS.md).
