# Branches and publication

| Field | Value |
| --- | --- |
| Status | Draft |
| Scope | Ephemeral AI FS 0.1 |

This document defines private branch views, branch lifecycle, optimistic
conflict detection, and atomic publication. It uses the normative terms from
[`SPEC.md`](../../SPEC.md). Filesystem operation details not changed here are
defined by [`filesystem-api.md`](./filesystem-api.md), and durable revisions,
objects, transactions, and garbage collection are defined by
[`storage-and-data-model.md`](./storage-and-data-model.md).

## Model and terminology

- **Main** is the authoritative mutable workspace view.
- A **revision** is an immutable, durable snapshot identifier in the linear
  history of main. Revision identifiers are opaque to callers.
- A **branch** is a durable private overlay on exactly one immutable base
  revision.
- An **entry slot** is one child name in one directory. It has a version token
  even when the child is absent. This token detects create-delete and other
  ABA changes.
- A **node** is a regular file, directory, or symbolic link. Hard links are
  multiple entry slots that refer to the same regular-file node.
- A **node token** identifies a node version. A content or metadata change
  produces a new token even when the final bytes equal older bytes.
- A **subtree token** identifies the namespace state below a directory. It is
  used by recursive removal and directory rename; ordinary changes to
  independent child names do not conflict merely because they share a parent.
- A **branch generation** is a monotonic value changed by every successful
  mutation that changes the branch view.
- An **overlay** is the branch-private set of namespace replacements,
  tombstones, metadata, content manifests, copy-on-write pages, and ordered
  structural edits.

The terms `merged` and `published` describe the same successful terminal
state. The API outcome and lifecycle state are named `merged` in version 0.1.

## Branch identity and creation

The library MUST create a branch in one database transaction. Creation MUST
record the branch identifier, base revision, state, generation, and creation
time before returning success.

By default, the base revision MUST be the main head observed inside the branch
creation transaction. A caller MAY request an older retained main revision.
The implementation MUST reject an unknown or unretained base revision; it MUST
NOT silently substitute the current head. Version 0.1 MUST NOT create a branch
directly from another unmerged branch.

A branch identifier:

- MUST contain between 1 and 200 UTF-8 bytes;
- MUST be treated as an opaque, case-sensitive value;
- MUST NOT be normalized, parsed as a path, or used directly as a table name;
- MUST be unique for the lifetime of the filesystem; and
- MUST NOT be reused after merge, discard, retention expiry, or garbage
  collection.

An implementation that removes a branch record MUST retain a compact durable
uniqueness marker or equivalent protection against identifier reuse. The
library MAY generate identifiers when a caller does not supply one. Generated
identifiers MUST contain at least 128 bits of randomness or equivalent
collision resistance.

The initial branch generation MUST be `0`. The base revision MUST never change.
Rebase, merge-from-main, and branch-from-branch are future operations and are
not part of version 0.1.

## Lifecycle

```text
                    publish conflict
                   +------------------+
                   |                  |
                   v                  |
create --------> active --------------+
                   |  \
          publish  |   \ discard
          success  |    \
                   v     v
                 merged  discarded
```

`active`, `merged`, and `discarded` are the only version 0.1 states.

- Only an active branch MAY be read or mutated as a filesystem view.
- A publication conflict MUST leave the branch active and its generation and
  overlay unchanged.
- Successful publication MUST atomically change the branch from active to
  merged.
- Successful discard MUST atomically change the branch from active to
  discarded.
- `merged` and `discarded` are terminal states.
- A terminal branch MUST reject filesystem reads and mutations with
  `BranchNotActive`.
- Branch metadata and any retained publication result MUST remain queryable
  according to the retention rules below.

No lifecycle transition MAY make an uncommitted branch overlay visible in
main. A failed transition MUST leave both the branch view and main unchanged.

## Branch read view and isolation

An active branch view is:

```text
view(branch) = snapshot(branch.baseRevision) + overlay(branch)
```

All reads, `stat` calls, link resolution, and directory listings MUST resolve
against this view. Main revisions committed after branch creation MUST NOT
appear in the branch unless they are reproduced by the branch's own overlay.
This rule applies even to paths the branch has never read or changed.

An acknowledged branch mutation MUST be visible to later operations on the
same branch and MUST survive database reopen. It MUST NOT be visible through
main or another branch before successful publication. Two branches created at
the same revision MUST start with equal views and then diverge independently.

A filesystem operation MUST observe one consistent branch generation. It MUST
NOT combine a base lookup from one generation with overlay rows from another.
An implementation MAY achieve this with a transaction, snapshot, lock, or an
optimistic generation check.

`readStream` MUST select one immutable snapshot when its promise resolves. If
the selected bytes still depend on mutable page, patch, or namespace overlay
rows, the implementation MUST materialize them into immutable content or hold
a snapshot pin or durable read lease over the exact selected representation.
Successful publication, discard, and garbage collection MUST NOT abort the
stream or change its bytes. The stream MUST remain readable until it is fully
consumed, canceled, or errors, and then release every pin or lease.

Publication or discard MAY detach pinned overlay rows from the branch, but it
MUST NOT physically reclaim them while a selected stream needs them. Closing
the branch handle that created the stream or closing its owning filesystem
MUST error the stream with `FilesystemError` code `EBADF` and release its pin.

Conflict detection is based on the branch write set, not its read set. Reading
a path does not make a later main change to that path a publication conflict.
Hosts that need read-set validation MUST implement it above this contract.

## Overlay behavior

The physical overlay schema is not public, but the following behavior is
required:

- Each branch mutation MUST be atomic.
- The overlay MUST retain the expectation from the base revision, not from the
  main revision current when the path is first mutated.
- Ordered structural edits MUST be replayed in their acknowledged order.
- The default branch representation MUST use 4,096-byte logical copy-on-write
  pages for small equal-length overwrites. A final page MAY be shorter.
- Repeated equal-length writes to one copy-on-write page MUST replace that
  branch-local page state instead of appending full file copies.
- Renames and hard links MUST preserve node identity and MUST NOT copy file
  bytes merely to change a name.
- A branch-created entry that is removed before publication SHOULD collapse to
  no net namespace change when no retained branch operation result requires
  the intermediate state.
- Overlay compaction MUST preserve the exact observable branch view and all
  base expectations used during publication.

Each successful mutation that changes the observable view MUST increment the
branch generation exactly once. A semantic no-op, such as renaming a path to
itself, MUST NOT increment it. Generation values exposed as JavaScript numbers
MUST remain safe integers. A branch at the maximum generation MUST reject a
further mutation with `LimitExceeded` before changing state.

### Files and metadata

Creating a regular file MUST add a private entry and file node to the overlay.
Replacing or fully writing a file MUST preserve the base expectation for that
file while replacing its branch-visible content. Changes to file bytes or mode
MUST be node changes for conflict purposes. Version 0.1 has no ownership or
caller-set timestamp operation. Timestamps produced by filesystem mutations
are derived effects, not independent caller changes.

Writing an existing regular file through any hard-link alias MUST update the
same branch-visible node, so all aliases in that branch observe the new bytes.
It MUST NOT split the hard link into independent files unless an API operation
explicitly requests copy semantics.

### Directories

Creating a directory MUST create a private directory entry. Adding or removing
one child MUST update only that child slot for ordinary conflict detection;
independent child names in an otherwise unchanged directory MAY merge.

The parent timestamp updates implied by namespace mutations are mergeable
effects, not whole-directory conflict keys. The overlay MUST retain the one
filesystem-clock sample used by the branch mutation. At publication, each
derived parent `mtimeMs` and `ctimeMs` MUST be:

```text
max(current main value, branch-visible operation timestamp)
```

Publication MUST NOT sample the clock again for that derived field. This rule
makes independent sibling changes produce the same parent timestamps in either
publication order and prevents time from moving backward. An explicit metadata
operation on the directory, such as `chmod`, remains a node change and
conflicts with another explicit or incompatible change to that node.

Removing a directory without recursive `rm` MUST require it to be empty in the
branch view. Publication MUST also fail with a conflict if main added a child
after the base revision. Recursive removal MUST record a subtree expectation
so a concurrent change anywhere below the removed directory conflicts rather
than being silently deleted.

A namespace mutation MUST retain identity anchors for each traversed parent
directory. Replacing, deleting, or moving an ancestor in main after the base
revision MUST conflict even if the final child slot itself is still absent.
Creating independent sibling entries under the same unchanged parent MUST NOT
conflict.

### Symbolic and hard links

Creating or replacing a symbolic link MUST store the link target as link data;
it MUST NOT copy or resolve the target into file content. Publishing a symbolic
link creation checks its destination entry slot. Replacing or deleting an
existing symbolic link also checks the base node token.

Creating a hard link MUST:

1. follow a final source symbolic link and resolve the source in the branch
   view exactly as `link` requires in the filesystem API;
2. require the resolved source to be a regular file;
3. add a private destination entry referring to that same regular-file node;
4. check the destination entry slot during publication; and
5. check the source node token when the source came from the base revision, so
   the link cannot silently attach to content changed in main after the base.

A missing or dangling source MUST fail with `ENOENT`, a non-directory
intermediate component with `ENOTDIR`, and an over-limit link traversal with
`ELOOP`. A resolved directory or symbolic-link inode MUST fail with `EPERM`.
An existing destination of any type, including a symbolic link, MUST fail with
`EEXIST`; the destination is not followed. Other destination-parent failures
use the exact filesystem API error.

Link counts reported in a branch MUST reflect its view. They become durable in
main only on publication. `stat(existingPath).id` and
`lstat(newPath).id` MUST be equal after creation, after reopen, and after
publication. Editing either hard-link alias MUST preserve that ID and remain
visible through the other alias.

### Delete and unlink

Deleting or unlinking an entry that existed at the base MUST create a private
tombstone or equivalent expectation. Main remains unchanged. Deleting a
branch-created entry MAY remove its overlay directly.

Deleting an existing regular file or symbolic link MUST conflict with a main
content or metadata change to that node after the base, even if its entry slot
is unchanged. Unlinking one alias of an otherwise unchanged hard-linked file
MUST preserve the other aliases.

### Rename

Rename MUST be one atomic branch mutation. A successful rename MUST make the
source absent and the destination present in the same branch generation. It
MUST preserve inode ID, content, mode, birth time, regular-file modification
time, and hard-link relationships. Using the mutation's one filesystem-clock
sample, it MUST update both affected parent directories' `mtimeMs` and
`ctimeMs` and the moved inode's `ctimeMs`, without updating a regular file's
`mtimeMs`. Publication MUST NOT take a new clock sample for those effects. If
the destination is replaced under the filesystem API's rename rules, the
replacement and source removal MUST still be atomic.

For publication, rename MUST be treated as at least:

- a conflict-checked removal of the source entry;
- a conflict-checked creation or replacement of the destination entry;
- a source node check for a regular file or symbolic link; and
- a source subtree check for a directory.

The parent-directory identity anchors of both paths MUST also be checked. A
rename conflicts if, after the branch base:

- main changes, removes, or replaces the source;
- main creates, changes, removes, or replaces the destination relative to the
  base expectation;
- main changes any descendant of a directory source; or
- main replaces or moves an ancestor required to resolve either path.

Two branches renaming the same source MUST NOT both merge. If both rename the
same source to the same destination, the second publication reports every
mismatched source and destination key in deterministic order. A rename whose
source equals its destination is a no-op.

### Truncate and range edits

Truncation and range edits MUST operate on branch-visible bytes.

`truncate(path, size)` MUST reject a negative or unsafe integer size. Shrinking
a file removes its suffix. Growing a file inserts zero bytes as required by the
filesystem API. An explicit truncation to the existing size is a no-op.

The public range-edit operation is:

```ts
replaceRange(path, offset, deleteLength, insertBytes)
```

It MUST follow the public `replaceRange` contract from the filesystem API. In
particular, it requires an existing regular file, interprets the range against
branch-visible bytes, rejects an unsafe or negative offset or delete length,
and rejects a range beyond the branch-visible file size. It removes
`deleteLength` bytes at `offset` and inserts a copied `Uint8Array` named
`insertBytes` at the same offset. An empty deletion and insertion is a
validated no-op. The final size MUST remain within persisted `maxFileBytes`.
Equal-length edits MAY use copy-on-write pages. Length-changing edits MAY use
ordered patches and content-defined rechunking. These representations MUST
produce identical bytes and identical publication conflict behavior.

The public `writeRange` operation remains distinct: an offset beyond end of
file extends the file with the required zero-filled gap, as specified by the
filesystem API.

Multiple edits to one file MUST apply in call-commit order. Offsets in a later
edit refer to the file produced by all earlier acknowledged edits.

## Optimistic conflict contract

Publication uses conservative file-level optimistic concurrency. It MUST NOT
perform a byte-range, line, syntax-tree, or semantic merge.

For every branch change, the implementation MUST retain the base tokens of all
affected entry slots, nodes, subtree guards, and ancestor identity anchors.
Inside the final publication transaction, it MUST compare them with main. A
change conflicts when any required current token differs from its base token.

Consequences include:

- Changes to independent regular-file nodes and independent entry slots MAY
  merge even when main advanced after the branch base.
- Two branches that create the same path conflict.
- Any two content or metadata changes to the same file node conflict, even
  when they edit disjoint byte ranges or use different hard-link aliases.
- Edit-versus-delete and delete-versus-delete on the same base node conflict.
- A main change followed by restoration of identical bytes still conflicts;
  equality of hashes does not erase revision history.
- Read-only paths never conflict by themselves.
- A branch final state identical to its base MAY be removed from the write set
  before publication, but an implementation MUST NOT remove a change when
  doing so would discard namespace or node-identity semantics.

Conflict paths MUST be absolute canonical paths. A publication MUST return all
detected conflict paths, with no path duplicated, sorted by UTF-8 byte order. A
rename MAY therefore report both source and destination. If several internal
checks fail for one path, the result MUST choose one reason using this
precedence: `subtree-changed`, `ancestor-changed`, `source-changed`,
`destination-changed`, `node-changed`, then `entry-changed`. Conflict detection
MUST NOT depend on SQL row order, request arrival order, locale collation, or
hash-map iteration order.

A conflict is an expected result, not a storage exception. It MUST NOT create a
main revision, update main, clear the overlay, change the branch generation,
or make the branch terminal.

## Atomic publication

Publication MAY prepare hashes, chunks, manifests, and an immutable candidate
change set before opening the final write transaction. Preparation MUST capture
the branch generation. Prepared data MUST either remain in memory until the
transaction or be protected by a durable staging lease from concurrent garbage
collection.

The final publication transaction MUST perform the following logical steps:

1. Look up the operation identifier and replay a prior result when required.
2. Verify that the branch exists, is active, and still has the prepared
   generation. If the generation changed, restart preparation or reject with
   `BranchChanged`; it MUST NOT publish an incomplete generation.
3. Re-read the current main head and every required conflict token.
4. If any token differs, construct the complete deterministic conflict result,
   durably record it when an operation identifier was supplied, and commit
   only that result record.
5. Otherwise, make all referenced immutable objects and manifests durable.
6. Allocate exactly one new revision whose parent is the current main head,
   not necessarily the branch base.
7. Apply the complete branch change set to that parent and record the revision
   delta, author or branch identifier, and changed paths.
8. Advance the main head to the new revision.
9. Change the branch state to merged and release its mutable overlay.
10. Durably record the successful result when an operation identifier was
    supplied.
11. Commit once.

The transaction boundary MUST include steps 2 through 10. The order above is
logical; physical writes MAY be reordered when foreign keys or adapter details
require it, but no observer may see a partial outcome.

Publishing an active branch with an empty net change set MUST still create one
durable revision with an empty changed-path list. This keeps successful branch
publication auditable and satisfies the rule that a successful publication
has one stable revision identifier.

A successful response MUST be returned only after the transaction commits.
Exactly one durable revision MUST correspond to one successful publication,
regardless of response loss or retries.

## Operation identifiers and result replay

A publication operation identifier is optional in the low-level API and
REQUIRED for hosts that retry requests after timeouts, disconnects, or process
restarts. It MUST contain between 1 and 200 UTF-8 bytes and is opaque and
case-sensitive. An empty, over-limit, or otherwise invalid operation identifier
MUST reject with `InvalidOperationId` before publication preparation.

When a publish call durably records a result for an operation identifier, the
implementation MUST bind the identifier to the branch identifier and branch
generation published by that attempt. A later call with the same identifier:

- MUST return the exact recorded merged or conflict result without creating a
  revision or repeating conflict detection;
- MUST work after database close, process restart, or lost response;
- MUST return `OperationBranchMismatch` if the supplied branch differs; and
- MUST NOT be interpreted as a request to publish later edits on that branch.

Callers MUST use a new operation identifier after changing a conflicted branch.
Reusing the old identifier intentionally replays the old conflict.

The operation lookup MUST occur both before expensive preparation and inside
the final transaction. Concurrent calls with the same operation identifier
MUST converge on one stored result. A uniqueness constraint or equivalent
serialization MUST prevent two successful revisions.

The recorded result MUST include every field returned by the public API that
is needed for exact replay. A conflict result MUST be recorded atomically even
though it leaves the branch active. A call without an operation identifier has
no replay guarantee, but it retains all atomicity and conflict guarantees.

`branches.replay(operationId, branchId?)` is the authoritative
handle-independent replay path. It MUST work after process restart and after
the branch overlay, branch payload, or retained branch metadata has been
pruned. It MUST NOT open, read, publish, or otherwise mutate a branch. If
`branchId` is supplied and differs from the recorded branch, it MUST reject
with `OperationBranchMismatch`. If the full result is retained, it MUST return
that exact result. If only the lifetime tombstone remains, it MUST reject with
`OperationResultExpired`. An identifier with neither a result nor a tombstone
MUST reject with `OperationNotFound`.

`BranchChanged` is a rejection, never a `PublishResult`. It MUST leave main and
the branch unchanged. A transient operation reservation for an attempt that
rejects before recording a merged or conflict result MUST be rolled back or
released, so retrying that identifier cannot be mistaken for result replay.

## Retry and crash recovery

Database rollback is the publication recovery mechanism. The implementation
MUST NOT require callers to repair partially advanced main state.

- A crash or error before the final commit MUST leave main, branch state, and
  the operation result as they were before the attempt.
- A crash after commit but before response delivery MUST be resolved by
  operation-result replay.
- A failed publication MUST leave an active branch retryable with its complete
  overlay.
- Reopening the database MUST require no in-memory branch state to reconstruct
  active views or recorded results.
- Orphaned immutable objects created during preparation MAY remain, but MUST
  be unreachable from main and MUST be reclaimable by garbage collection.
- Recovery MUST NOT guess whether an operation committed from branch state
  alone; it MUST use the durable operation-result record when an identifier was
  supplied.

If an adapter cannot provide rollback across all authoritative tables, that
adapter does not conform to this specification.

## Discard

Discarding an active branch MUST be one transaction that:

1. changes its state to discarded;
2. removes or detaches all mutable overlay records;
3. releases its base revision and overlay objects as garbage-collection roots;
   and
4. preserves the terminal branch metadata required by retention.

Steps 2 and 3 MUST preserve any detached immutable snapshot or durable read
lease selected by an already-open branch stream. The stream, rather than the
terminal branch, remains the root until consumption, cancellation, or error.

Discard MUST NOT change main or create a main revision. A second discard of an
already discarded branch SHOULD return the original terminal information as an
idempotent success. Discarding a merged branch MUST return `BranchNotActive`.
Publishing a discarded branch MUST return `BranchNotActive` unless a supplied
operation identifier replays a result recorded before discard.

Content made unreachable by discard MAY remain physically stored until a later
garbage-collection pass.

## Retention and garbage-collection roots

Active branches MUST NOT expire automatically in version 0.1. Hosts MAY alert
on or explicitly discard abandoned branches.

`BranchRetentionOptions` MUST set `terminalBranchRetentionMs` and
`publicationResultRetentionMs` to 30 days by default. Each has a minimum of 7
days. The effective values are exposed through filesystem capabilities as
specified below.

Retention time starts at the terminal transition or recorded result commit.
Terminal branch metadata referenced by a retained operation result MUST be kept
until both retention periods have elapsed. Before deleting a replayable result,
the implementation MUST retain a compact operation-identifier tombstone for
the filesystem lifetime. A retry after the replay payload expires MUST return
`OperationResultExpired`; it MUST never execute as a new publication.

Garbage collection MUST treat all of the following as roots:

- the current main namespace and its revision;
- every revision retained by history policy;
- the complete base snapshot of every active branch, including files the
  branch has never changed;
- all namespace and content state reachable only from active overlays;
- every revision referenced by a retained successful publication result; and
- every snapshot pin or durable read lease held by an active stream, including
  a stream opened before its branch became terminal; and
- durable publication staging protected by an unexpired lease, if staging is
  used.

Copy-on-write pages and ordered patch bytes of active branches are live branch
payload. Terminal branch overlays are not roots after their transition
transaction releases them, except for detached data protected by a stream
snapshot pin or lease. Conflict results do not root overlay data; the
still-active branch does.

Garbage collection MUST serialize with publication finalization or use an
equivalent snapshot protocol. It MUST NOT delete a candidate object after
publication has validated it but before the publication transaction makes it
reachable. In particular, rooting only changed base files is insufficient:
the whole base snapshot of an active branch MUST remain readable.

## Concurrency and ordering

All successful main mutations MUST have one total commit order. Each published
revision's parent MUST be the immediately preceding main head in that order.
The implementation MUST NOT assume a single-threaded Durable Object host; the
Node.js adapter and future hosts require the same behavior.

Concurrent mutation of one branch and publication of that branch MUST be
serialized or guarded by branch generation. The result MUST contain all
mutations that committed before the captured generation and none that committed
after it. It MUST never contain a partial mutation.

Concurrent publications from the same branch MUST create at most one revision.
After one succeeds, another call without the same recorded operation identifier
MUST observe a non-active branch. Concurrent publications from different
branches MAY finish in any order, but each MUST test conflicts against the main
state immediately preceding its own commit.

Therefore, when 50 branches share one base revision:

- if each changes an independent file or entry slot, all 50 publications MUST
  merge and create a 50-revision parent chain in some order; and
- if all 50 change the same file node, exactly one publication MUST merge and
  the other 49 MUST return a conflict for that file.

The winning branch in the same-file case is determined by transaction order
and is not otherwise guaranteed.

## Public API and result shapes

The following TypeScript is normative in meaning; exported names MAY change
before the first release candidate only with a corresponding specification
update.

```ts
type RevisionId = string;
type BranchState = "active" | "merged" | "discarded";

interface BranchLimits {
  readonly maxBranchIdBytes: number;
  readonly maxOperationIdBytes: number;
  readonly maxActiveBranches: number;
  readonly maxChangedPathsPerBranch: number;
  readonly maxConflictsPerPublication: number;
}

interface BranchRetentionOptions {
  readonly terminalBranchRetentionMs: number;
  readonly publicationResultRetentionMs: number;
}

interface BranchConfiguration
  extends BranchLimits, BranchRetentionOptions {}

interface OpenFilesystemOptions {
  readonly branch?: Partial<BranchConfiguration>;
}

interface FilesystemCapabilities {
  readonly branch: Readonly<BranchConfiguration>;
}

interface BranchInfo {
  id: string;
  baseRevision: RevisionId;
  state: BranchState;
  generation: number;
  createdAt: number;
  terminalAt: number | null;
}

interface CreateBranchOptions {
  id?: string;
  baseRevision?: RevisionId;
}

interface PublishOptions {
  operationId?: string;
}

type ConflictReason =
  | "entry-changed"
  | "node-changed"
  | "source-changed"
  | "destination-changed"
  | "subtree-changed"
  | "ancestor-changed";

interface PublishConflict {
  path: string;
  reason: ConflictReason;
  expectedRevision: RevisionId | null;
  actualRevision: RevisionId | null;
}

interface MergedPublishResult {
  outcome: "merged";
  branchId: string;
  operationId: string | null;
  baseRevision: RevisionId;
  parentRevision: RevisionId;
  revision: RevisionId;
  changedPaths: string[];
  conflicts: [];
}

interface ConflictPublishResult {
  outcome: "conflict";
  branchId: string;
  operationId: string | null;
  baseRevision: RevisionId;
  headRevision: RevisionId;
  revision: null;
  changedPaths: [];
  conflicts: PublishConflict[];
}

type PublishResult = MergedPublishResult | ConflictPublishResult;

interface EphemeralBranch extends Omit<EphemeralFilesystem, "close"> {
  readonly id: string;
  info(): Promise<BranchInfo>;
  publish(options?: PublishOptions): Promise<PublishResult>;
  discard(): Promise<BranchInfo>;
  /** Releases this handle; it does not close the owning filesystem. */
  close(): Promise<void>;
}

interface Branches {
  create(id: string): Promise<EphemeralBranch>;
  create(options?: CreateBranchOptions): Promise<EphemeralBranch>;
  open(id: string): Promise<EphemeralBranch>;
  get(id: string): Promise<BranchInfo>;
  replay(operationId: string, branchId?: string): Promise<PublishResult>;
}

interface BranchCapableFilesystem
  extends EphemeralFilesystem, EphemeralFilesystemAdministration {
  readonly branches: Branches;
}

type BranchErrorCode =
  | "InvalidBranchId"
  | "InvalidOperationId"
  | "BranchNotFound"
  | "BranchNotActive"
  | "RevisionNotFound"
  | "BranchChanged"
  | "OperationBranchMismatch"
  | "OperationNotFound"
  | "OperationResultExpired"
  | "LimitExceeded";

interface BranchError extends Error {
  readonly name: "BranchError";
  readonly code: BranchErrorCode;
  readonly branchId?: string;
  readonly operationId?: string;
  readonly limit?: keyof BranchLimits;
}
```

`EphemeralFS` MUST implement `BranchCapableFilesystem`. `branches.open` MUST
return a handle for an active or retained terminal branch. A terminal handle
can inspect metadata and replay a retained publication, but its filesystem
methods fail with `BranchNotActive`. After terminal metadata expires,
`branches.open` rejects with `BranchNotFound`; callers use `branches.replay`
without a handle. Closing a branch handle MUST NOT close the owning filesystem
or database.

On a terminal handle, `publish` MUST validate and look up a supplied operation
identifier before rejecting the lifecycle state, so a matching retained result
can replay. A different or unknown operation then rejects with
`BranchNotActive`. The idempotent repeated-discard rule is the corresponding
exception for `discard`. `branches.replay` checks owner closure first, validates
its operation identifier and optional branch identifier, and then performs the
result or tombstone lookup without a branch-state check.

`createdAt` and `terminalAt` are Unix epoch milliseconds from the filesystem
clock contract. They are metadata, not ordering keys. Revision order,
transaction order, and branch generation define ordering.

### Branch handle lifetime and close

`EphemeralBranch.close()` MUST be idempotent. When its first call begins, that
handle MUST stop admitting operations. Calls started afterward, including
`info`, `publish`, and `discard`, MUST reject with `FilesystemError` code
`EBADF`.

Close MUST error every snapshot stream created by that handle with `EBADF`,
release its pins or leases, and wait for every already-admitted non-stream
operation to settle before resolving. It MUST NOT discard, publish, or
otherwise change the branch. It MUST NOT close the database or affect another
handle for the same branch; other handles and their streams remain usable.

Closing the owning `EphemeralFS` MUST invalidate every branch handle it issued,
error all of their active streams with `EBADF`, and wait for admitted
non-stream work under the filesystem close contract. Calling a handle's
`close()` before, during, or after owner close remains an idempotent success.

### Exact changed paths

`changedPaths` MUST contain, without duplicates, the canonical paths visible
immediately before or after publication whose entry binding changed or that a
branch mutation selected to change inode content or mode. It MUST be sorted by
unsigned UTF-8 byte order.

The exact rules are:

- create, link, unlink, and replacement include the affected entry path;
- a content write, `replaceRange`, truncate, or `chmod` includes the canonical
  path selected by that operation;
- rename includes source and destination;
- directory rename includes old and new paths for the directory and every
  moved descendant;
- recursive creation or removal includes every created or removed path; and
- an empty publication has an empty list.

Derived parent timestamp changes, link-count and `ctimeMs` changes, and moved
inode `ctimeMs` changes do not add paths. A content or mode change through one
hard-link alias includes the selected alias but not untouched aliases, even
though they observe the same inode change. This choice keeps the list tied to
the authored branch write set; inode revision records remain the authority for
auditing alias-visible effects.

Branch-management and branch-state errors MUST reject with `BranchError` and
stable codes at least for:

- `InvalidBranchId`;
- `InvalidOperationId`;
- `BranchNotFound`;
- `BranchNotActive`;
- `RevisionNotFound`;
- `BranchChanged`;
- `OperationBranchMismatch`;
- `OperationNotFound`;
- `OperationResultExpired`; and
- `LimitExceeded`.

`BranchChanged` MUST reject the promise and MUST NOT be encoded as a merged or
conflict result. `LimitExceeded` is only for a `BranchLimits` bound. Inherited
filesystem content, materialization, and atomic-tree limits continue to use
`FilesystemError` code `EFBIG`.

Error precedence for a branch-handle filesystem call is: closed handle or
closed owner (`EBADF`), branch lifecycle (`BranchNotActive`), then ordinary
filesystem validation and semantics. Branch-limit checks follow filesystem
validation. Therefore a terminal branch with an invalid path reports
`BranchNotActive`, and an active operation that exceeds `maxFileBytes` reports
`EFBIG`, not `LimitExceeded`.

A normal optimistic conflict MUST be a `PublishResult`, not a thrown error.

## Limits and capability reporting

The resolved `BranchConfiguration` combines `BranchLimits` and
`BranchRetentionOptions`. Its version 0.1 defaults are:

| Field | Default |
| --- | ---: |
| `maxBranchIdBytes` | 200 |
| `maxOperationIdBytes` | 200 |
| `maxActiveBranches` | 10,000 |
| `maxChangedPathsPerBranch` | 100,000 |
| `maxConflictsPerPublication` | 100,000 |
| `terminalBranchRetentionMs` | 2,592,000,000 |
| `publicationResultRetentionMs` | 2,592,000,000 |

Every value MUST be a positive safe integer. Both retention values MUST be at
least 604,800,000 milliseconds. The two identifier limits MUST NOT exceed 200
UTF-8 bytes, and `maxConflictsPerPublication` MUST be at least
`maxChangedPathsPerBranch`.

`OpenFilesystemOptions.branch` MAY provide a partial configuration; omitted
fields use the defaults. The effective configuration affects persisted writer
behavior, MUST be stored with the filesystem, and MUST agree across writers.
Opening with incompatible persisted values MUST fail with `ESCHEMA`.
`EphemeralFS.capabilities.branch` MUST expose an immutable copy of every
effective field.

Hosts MAY configure lower identifier, active-branch, changed-path, and conflict
limits. Raising their version 0.1 defaults requires a format review. A branch
identifier outside its configured bound uses `InvalidBranchId`, and an
operation identifier outside its bound uses `InvalidOperationId`.

The exact expanded `changedPaths` list defines
`maxChangedPathsPerBranch` accounting. A recursive mutation or directory rename
that would exceed the bound MUST reject atomically with `LimitExceeded`.
Creating a branch above `maxActiveBranches` uses the same error. Conflict
results MUST never be truncated.

Implementations SHOULD process publication preparation and garbage collection
in bounded batches. A limit error or resource error MUST NOT partially apply a
branch mutation or publication. Storage optimizations such as a 4 KiB page,
64 KiB fast path, or 512 KiB patch segment are not semantic API limits and MUST
NOT change results.

## Required invariants

At every committed database boundary:

1. A branch refers to exactly one existing base revision.
2. A branch's base revision never changes.
3. Only active branches own mutable overlay rows.
4. The branch view equals its immutable base plus its complete overlay.
5. Main references only committed revisions and never a private overlay.
6. Main revisions form one parent chain with one head.
7. A merged branch corresponds to exactly one publication revision.
8. One publication operation identifier corresponds to at most one branch,
   branch generation, and result.
9. Replaying a recorded operation cannot change main, the branch, or the
   result.
10. A conflict changes neither main nor the branch view.
11. A failed transaction exposes neither half of a rename nor part of a
    revision.
12. Every object reachable from main, retained history, an active branch, a
    retained successful result, or protected staging survives garbage
    collection.
13. Same-node concurrent writes cannot both merge from the same base token.
14. Independent entry slots can merge without copying or rebasing the branch's
    untouched base snapshot.

Implementations SHOULD run invariant checks in test builds after injected
transaction failures.

## Conformance scenarios

Every supported database adapter MUST pass the same deterministic scenarios.
At minimum the suite MUST cover:

1. **Snapshot isolation:** create a branch, change main, and prove the branch
   still reads its original base plus its private edits.
2. **Private namespace:** create files, directories, symbolic links, and hard
   links in a branch; prove main and a sibling branch cannot observe them.
3. **Overlay ordering:** combine page writes, public `replaceRange` insertions
   and deletions, truncation, and later writes; verify reads and published bytes
   match a reference byte array.
4. **Hard-link identity:** edit one alias and observe the edit through another;
   publish without splitting the node.
5. **Independent paths:** publish two stale branches that change different
   files in both orders; both must merge.
6. **Same file:** edit disjoint ranges of one file in two branches; the first
   must merge and the second must conflict.
7. **Create collision:** create the same absent path in two branches; only one
   may merge.
8. **Delete versus edit:** publish delete then edit, and edit then delete; the
   second result must conflict and retain its private view.
9. **Rename source conflict:** rename a file while another branch edits or
   deletes the source; the stale rename must conflict.
10. **Rename destination conflict:** rename onto an expected destination while
    main creates, removes, or changes that destination; publication must
    conflict and rename must never be half-applied.
11. **Directory subtree conflict:** recursively remove or rename a directory
    while main changes a descendant; publication must conflict.
12. **Independent siblings:** create different child names below one unchanged
    directory in two branches; both must merge.
13. **ABA detection:** change a path in main and restore identical bytes or
    absence; a branch based before both changes must still conflict.
14. **Lost merged response:** publish with an operation identifier, reopen the
    database, call handle-independent `branches.replay`, and receive the exact
    result with no second revision.
15. **Lost conflict response:** record a conflict, reopen, modify main again,
    retry the identifier, and receive the original conflict unchanged.
16. **Operation mismatch:** use one operation identifier with two branches and
    receive `OperationBranchMismatch` without publishing the second.
17. **Injected failure:** abort each publication write step in turn; after
    reopen, main must be unchanged and the active branch must be retryable.
18. **Interrupted rename mutation:** inject failure between physical rename
    writes and prove the branch shows either the complete old state or complete
    new state, never both halves.
19. **Discard:** discard a branch with pages, patches, and branch-only objects;
    prove main is unchanged and later garbage collection reclaims the data.
20. **Active-branch GC:** retain an old active branch while main advances past
    history retention; collect garbage and prove every untouched and changed
    file in the branch base remains readable.
21. **Fifty independent writers:** create 50 branches at one base, change one
    distinct file or entry slot per branch, issue publications concurrently,
    and require 50 merged results, 50 new revisions in one parent chain, and
    every value present in final main.
22. **Fifty same-path writers:** create 50 branches at one base, change the same
    regular-file node, issue publications concurrently, and require exactly
    one merged result, 49 deterministic conflicts for that path, one new
    revision, and final bytes from the winning branch.
23. **Terminal handles and independent replay:** open retained merged and
    discarded handles, prune branch payload, replay without a handle, then
    expire the full result and require `OperationResultExpired` from its
    lifetime tombstone.
24. **Handle close:** prove close is idempotent, rejects later calls with
    `EBADF`, aborts its streams, waits for admitted non-stream work, leaves
    other handles usable, and is safely subsumed by owner close.
25. **Stream pins across terminalization:** open a branch stream, publish or
    discard the branch, run garbage collection, and consume the original exact
    bytes; also prove cancel, stream error, handle close, and owner close release
    the pin.
26. **Hard-link source resolution:** link through a final symbolic link,
    preserve `FileStat.id`, and assert `ENOENT`, `ENOTDIR`, `ELOOP`, `EPERM`,
    and `EEXIST` for the specified source and destination cases.
27. **Directory timestamp merge:** use a controlled clock for independent
    sibling mutations, publish them in both orders, and require the same
    component-wise maximum parent timestamps with no publication clock sample.
28. **Exact changed paths:** cover create, write, mode change, link, unlink,
    file and directory rename, recursive creation and removal, parent-only
    timestamps, and writes through one of several hard-link aliases.
29. **Errors and configuration:** verify defaults, partial overrides,
    capability reporting, persisted-configuration mismatch, every branch
    limit, `InvalidOperationId`, `BranchChanged` rejection, `EFBIG` separation,
    and closed-state error precedence.

Concurrency tests MUST use real concurrent requests or promises at the adapter
boundary and MUST NOT preselect the winning branch. Crash tests MUST recreate
the engine from the same durable database instead of reusing in-memory state.

## Explicitly out of scope

Version 0.1 does not perform automatic rebase, three-way byte merge, line merge,
syntax-tree merge, conflict markers, or model-assisted semantic merge. A future
semantic-merge layer MAY consume a conflict result, create a new branch or
explicitly update the active branch, and publish with a new operation
identifier. It MUST NOT weaken the conflict and atomicity rules defined here.
