# Workspace Commit integration

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> Written 2026-09-21 against merged source
> `152b9c3a2e8ec2536a1d63601b681e1f7ef34455`. The shared service/history APIs
> below exist at that pin. Section 7's successor-reconciliation rules were
> refreshed in the issue-179 documentation round against product source
> `f802cc124` to state the implemented reconcile carry, declaration ledger,
> rename replacement semantics and canonical link-count re-baseline. No
> performance result, crash-durability guarantee or completed qualification is
> claimed.

Read the [packet overview](README.md), [Workspace/FUSE contract](01-workspace-fuse-contract.md)
and [overlay/snapshot design](02-overlay-snapshot.md) for runtime behavior.
[Implementation and verification](04-implementation-and-verification.md) owns
the implementation rounds and checks. This document specifies how that runtime
consumes the existing operation surface; it does not redefine its wire grammar,
catalog schema or algorithms.

The current backing target is [explicit daemon-local disk backing with a small
RAM working set](01-workspace-fuse-contract.md#134-npm-install-and-low-memory-backing),
including the owner's mixed large/tiny-file and npm-install workload. The earlier
RAM-only payload prototype is not the general solution. The 8 MiB target applies
to accounted Workspace allocations, while disk payload/index capacity has an
independent declared quota. This audit changes neither shared API limits nor
concurrency settings; workload blockers remain explicit below.

## 1. Owners and acknowledged boundaries

```text
 APPLICATIONS / LOCAL KERNEL
             |
        FUSE adapter                         DAEMON
             |
        Workspace <---- explicit SDK edit/control binding
        +-----------------------------------------------+
        | live inode/namespace state, open handles      |
        | frozen generation G + live successor G+1      |
        | disk extent/index references and revisions   |
        | exact request/result and visibility context  |
        +----------------------+------------------------+
                               |
                       existing bridge Client
                               |
              +----------------+------------------+
              |                                   |
     operation profile 1                 operation profile 2
     ReadFile / Inspect                  HistoryQuery (opcode 6)
     ConstructFile / EditFile            HistoryCommand (opcode 7)
              |                                   |
              +----------------+------------------+
                               |
                 authenticated service admission         SERVICE
                               |
              +----------------+------------------+
              |                                   |
        C1 construction                    C5 history catalog
        C2 save/read                       exact stage / Commit /
        content Store                      Branch / Layer metadata
              ^                                   ^
              +----- service composition ----------+
                    separate acknowledged steps
```

The daemon owns pending bytes, handles, the frozen snapshot and the live
successor. C5 knows a stage's opaque producer incarnation and compact saved-root
context; it does not own the mounted Workspace, its pending bytes or per-inode
revisions. Service handlers compose C1/C2 with C5. Canonical-object reads and
emissions remain local to the service. Workspace receives no SQL, SQLite
credentials, C2 private save IDs or pack-layout knowledge.

The same service handlers serve direct and transported operations. Workspace
uses the existing bridge client rather than another framing protocol, catalog
client or mutation-replay mechanism. The [service history composition][service-history]
performs C1/C2 construction outside short history transactions. One composite
request does not turn several persistence boundaries into one atomic save.

There are six distinct acknowledgements:

1. A local write becomes visible in the bounded overlay.
2. An exact file-object save finishes and its terminal result is received.
3. The filesystem-root C2 save finishes inside StageChanges.
4. C5 inserts and acknowledges an exact stage.
5. C5 commits that stage, conditionally advances the Branch and consumes it.
6. A separately requested AddLayer publishes a Layer and advances the stack.

None implies crash durability. Ordinary FUSE write, flush, release and close do
not automatically perform steps 2-6. Their callback contract is defined in the
[Workspace/FUSE document](01-workspace-fuse-contract.md).

## 2. Profiles, grants and identity

Three unrelated uses of version/profile information must remain separate:

| Value | Meaning |
| --- | --- |
| Native HELLO version 1 | The current transport handshake/framing version |
| `Request.profile = 1` | Existing content/read operations, opcodes 1-5 |
| `Request.profile = 2` | History query/command operations, opcodes 6 and 7 |
| `BranchSnapshotWire.profile: [u8; 32]` | The canonical filesystem profile identity validated against its root |

Do not put the filesystem profile bytes into `Request.profile` or switch a
ReadFile/EditFile request to profile 2. History query permission is `0x20`;
history command permission is `0x40`. A legacy mask of 31 grants neither.
The existing service checks authenticated public-key possession, expiry,
logical Store selection and the operation grant. The grants are Store-wide;
they do not imply a per-Branch or per-Workspace ACL. A hash, token or Workspace
ID is not an access grant. [Profiles and permissions][history-contract]
[Service authorization][service-owner]

| Identity | Owner and use |
| --- | --- |
| `/workspace-id` and its display ID | Managed mount/lifecycle identity. The root is immovable; history operations do not rename it. |
| Workspace producer incarnation | Explicit nonzero 32-byte identity supplied to the runtime by its authority; identifies the producer of a C5 stage. It is not inferred from a path, PID, clock or connection. |
| Branch / LayerStack identity | Explicit authority-supplied 16-byte body becomes its typed 17-byte identity. Name uniqueness and ownership are checked by C5. |
| Commit / Layer identity | Typed 33-byte history identity. Workspace consumes it rather than deriving publication identity from a file root. |
| C1 root / allocation scope | A 32-byte logical identity; no native path or handle crosses the bridge. |
| Local generation and inode revision | Workspace's frozen/live bookkeeping. Keep them associated with the exact request and captured context. |
| `PreparedChanges.generation` | Inner producer generation persisted in StageWire. It must fit C5's signed 64-bit storage representation. |
| `Request.generation` | Outer carried request metadata. Service logic does not enforce equality with the inner generation or use it as a persistent replay key. |
| Request correlation ID | Nonzero and increasing within a connection; not a cross-restart operation receipt. |
| Catalog incarnation | C5 authority/cursor context, distinct from all runtime generations. |

Set outer and inner generations consistently in the proposed caller and retain
the association locally, while preserving the fact that equality is not checked
by the current service. Do not mint another producer incarnation to evade an
existing or unknown stage. A lost in-memory producer state does not establish a
new writable incarnation or authorize reuse of its serials.

## 3. Acquire a validated base

### Existing Branch

`HistoryQuery::GetBranch { branch }` returns a coherent
`BranchSnapshotWire`: Branch/stack identity, base Layer, optional head Commit,
head/base/effective roots, allocation scope, canonical profile and
`root_serial: Some(...)`. After the C5 snapshot, the service opens the effective
C1 filesystem root, checks its scope/profile and obtains its actual root serial.
There is no whole-filesystem materialization in this descriptor read.
[Validated GetBranch path][get-branch]

```text
 GetBranch(B)
      |
      +--> C5 coherent snapshot: head K0, base L0, effective R0
      |
      +--> C1 opens R0 and checks profile/scope
      |
      `--> descriptor Ctx0: (B, K0, L0, R0, scope, profile, root_serial)
                         |
                         `--> captured unchanged with generation G
```

The result is coherent captured context, not a lease preventing later Branch
changes. A writable Workspace freezes this descriptor with G and accepts a
later conflict rather than silently updating the expected tokens.

### Fork or initialization

```text
 already published Layer or authorized ancestor Commit
                         |
                     Fork(Bnew)
                         |
        acknowledged metadata-only BranchSnapshot
                  root_serial = None
                         |
                    GetBranch(Bnew)
                         |
                 validated mount descriptor

 no existing stack:
   acknowledged file roots + bounded manifest
                         |
                  InitLayerStack
                         |
       StackCreated { stack, root, actual root_serial }
                    NO Branch created
                         |
                 Fork --> GetBranch
```

Fork deliberately does not perform a later C1 root read that could turn an
acknowledged Branch creation into a reported creation failure. Its absent serial
is expected, not zero or an assumed one. Native response matching requires a
present serial for GetBranch and an absent one for Fork. [Client matching][client]

InitLayerStack accepts at most 128 pathless manifest entries including the root,
reserves the required serials, builds prerequisite metadata/symlink objects,
saves the filesystem tree and publishes genesis/stack metadata. File roots in
the manifest must already be saved. It creates no Branch, imports no host tree,
and supplies no general live create/mkdir operation. Its root serial is the
actual reservation result. [Bootstrap composition][bootstrap]

A read-only explicit-root mount remains possible without Branch creation.
History-backed mutation needs the Branch context above. The manifest and new
inode reservation capabilities do not widen the prepared-update path described
next.

## 4. Freeze G, save prerequisites, build PreparedChanges

The overlay document owns how G is captured without copying the entire
Workspace and how G+1 remains writable under the same total allocation budget.
Commit integration retains G's original roots, namespace changes, inode
revisions, stable replacement bytes and immutable Branch context.

Maintain the changed frontier while accepting mutations. Capture selects that
frontier and immutable view/index roots; it does not enumerate every path,
export the mounted directory, copy the spool, or normalize every file while
holding the state mutex. Once a prior Commit is acknowledged, the next frontier
contains only the remaining successor changes relative to that known result.
The retained file-save association is keyed by captured inode/version, so two
hard-link names do not create two saves of the same captured file. Metadata-only
or name-only changes retain acknowledged file content roots without invoking
EditFile solely because the inode/path appeared in a dirty set.

Save G's changed file content sequentially through profile-1 `EditFile`, or
through `ConstructFile` for an explicitly selected complete-file construction.
Do not retry an unsupported edit as complete-file construction. File lowering
uses the final piece state, not the chronological FUSE write log. Newly created
files still require the missing live new-inode attachment operation before
their namespace update can use this history path.

Record each exact successful `Saved { root, length, inserted, reused }` terminal
against its frozen inode/revision. A root seen elsewhere, a successful readback,
or another writer publishing the same ObjectId does not prove this operation's
finish succeeded. A failed or unknown file save stops the chain.

### 4.1 Disk-backed frozen input uses the existing stream capability

```text
 frozen G
   changed-inode/version frontier
   immutable local extent/index references
                |
         bounded lowering cursor
                |
         existing bridge Source::read(buffer, deadline, cancel)
                |
        bounded BODY frames --------------------------> service
                                                        |
                         EditFile: bounded replacement acquisition
                         ConstructFile: complete logical input stream
                                                        |
                                          local C1 -> local C2 -> terminal
```

The proposed source adapter retains the exact file/extent owner, offsets,
lengths and version selected by G. It reads into the caller-provided bounded
buffer, checks exact declared length/EOF and respects cancellation/deadline
between local I/O steps. An extent later appended for D1 cannot replace G's
source, and a reused native filename or descriptor number cannot identify it.
G/read ownership prevents truncation, overwrite, deletion or physical reuse of
referenced extents. The source cursor, open-descriptor budget, frame buffers and
any local read scratch remain charged. It does not collect a complete file or
all replacement bytes in a daemon vector before calling the bridge.

This is an implementation of the existing native `Source` input capability,
not another transport or a daemon-facing canonical-object provider. ReadFile
requests are the existing logical way to obtain required immutable bytes; C1
canonical traversal and content/storage algorithms stay local to the service.
Do not reproduce the reference's reverse per-object snapshot protocol. Ordinary
local disk I/O may block; the selected backing must qualify bounded admission,
progress and cleanup rather than claiming the Source signature preempts every
filesystem syscall. [Source and client deadline][client]

Daemon streaming does not remove service replay cost. At this pin EditFile
acquires its replacement parts into service-owned `Replacements` before starting
the save, with an 8 MiB aggregate limit. A larger local disk spool does not make
a larger EditFile legal. ConstructFile accepts a complete stream, but using it
after an edit refusal would be an unapproved alternative operation. Large new
files still need a live new-inode attachment contract; arbitrary large rewrites
need an explicitly selected supported shared input contract. No automatic
per-file multi-save sequence, changed operation or increased limit is hidden
inside the source cursor. [Service replacement acquisition][file-write]

### 4.2 Prepared filesystem input and missing prerequisite operations

Metadata roots must also already be valid and retained. General live portable
metadata construction is still missing from the shared operation surface;
bootstrap support does not supply it. This includes mtime changes required by
ordinary file writes. Reusing old metadata in a content-only diagnostic is not
POSIX write support. The first supported writable mount must have its selected
metadata operation/policy implemented and verified. Workspace must not fill the
gap by encoding canonical metadata itself.

The public `PreparedChanges` fields are fixed by the [shared contract][prepared]:

| Field | Source in frozen G / validation |
| --- | --- |
| `workspace: [u8; 32]` | Exact nonzero producer incarnation that will own the stage |
| `branch: [u8; 17]` | Captured typed Branch ID |
| `expected_head: Option<[u8; 33]>` | Captured head Commit; None is meaningful for a Branch with no Commit |
| `expected_base: [u8; 33]` | Captured base Layer |
| `generation: u64` | Inner frozen generation; caller keeps it within `i64::MAX` |
| `base: Root` | Original captured effective filesystem root, not a newly constructed candidate |
| `scope: Root` | Captured and validated allocation scope |
| `root_serial: u64` | Actual validated root serial, not a default or guessed allocator start |
| `directories: Vec<DirectoryChange>` | Sorted unique parent records; each contains sorted unique final bindings for changed names only, with removal represented by None |
| `inodes: Vec<InodeChange>` | Sorted unique existing serials and their final kind/content/metadata roots; file roots are tied to known prerequisite results |

The filesystem profile is not another PreparedChanges field. The service gets
it from the captured Branch/stack and validates the filesystem context. It also
validates inode content/metadata roles, rather than accepting a hash merely
because some canonical object with that identity exists.

At this pin the builder requires every referenced/updated serial already to
exist, preserves an existing inode's kind and refuses wholesale replacement of
a directory root. It passes `new_inodes: &[]`. ReserveInodes now allocates
nonrecycled ranges, but cannot make a new serial accepted by this builder.
[Shared existing-identity builder][filesystem-update]

The disk-backed target also requires scalable dirty metadata, but disk placement
alone cannot bypass the bridge's 128 changed-name/inode/directory limits or
32 KiB complete request metadata limit. One user Commit cannot secretly become
several 128-record Commits: that changes atomic visibility, expected Branch
context, stage lifetime and failure meaning. Nor can the caller prebuild an
arbitrary root and ask StageChanges to register it. Before promising a saveable
npm-sized frontier, agree and implement a bounded shared contract that preserves
the intended one-user-Commit semantics, or keep that capability explicitly
unavailable. New-inode attachment, live mode/mtime and symlink construction,
oversized edit input and large prepared frontiers are separate unresolved
requirements; this document invents no private operation names for them.

The current `Source` capability streams file payload; it does not stream the
PreparedChanges record set. History requests declare zero BODY input and decode
their complete bounded directory/inode vectors from metadata, then the service
passes prepared slices to C1. Transporting a larger future record set in pages
would not make it bounded if the receiver collected every page into one growing
vector before construction. A large-G revision must specify bounded delivery,
ordering/index ownership, service construction input and finalization together,
including disk/scratch quotas and failure retention. Paged transport alone is
not that new prepared-input contract and does not justify intermediate Commits.

## 5. StageChanges owns filesystem construction and save

```text
 Workspace                                      Service
 ---------                                      -------
 freeze G/Ctx0
 keep live G+1
 file-save terminals ----------------------->   C1 file work + C2 finish
 retain exact content roots <----------------   Saved(...)

 PreparedChanges(G, original R0, changes) ---->   C5 snapshot and early checks
                                                role validation
                                                C2 begin_save
                                                C1 filesystem update
                                                C2 finish
                                                C5 stage insertion
 StageWire(T, candidate R1, captured Ctx0) <---   acknowledged exact stage
```

The service first rejects stale expected head/base and mismatched construction
base/scope. It then validates supplied inode roles, builds and saves the
filesystem tree through the shared C1/C2 path, and only after successful finish
records the stage. C5 does not hold its transaction over construction, upload
or C2 finish. A Branch can advance while that work runs; the resulting saved
stage can retain its original expectations and subsequently lose at Commit.
[StageChanges implementation][stage-body]

**Do not call UpdatePreparedFilesystem before StageChanges.** StageChanges
accepts original base plus prepared changes and constructs its own candidate;
it is not an API to register an arbitrary already-built filesystem root. The
profile-1 root-only operation remains usable for an explicitly root-only caller,
but is not the final step of a history-backed Workspace Commit.

Stage success is also not local snapshot capture, file-save success, Branch
publication or permission to clear G. Earlier acknowledged file/tree objects
may remain unreferenced if a later stage/publication step fails. There is no
automatic deletion, serial refund or promotion of an unknown save based on
another operation's matching root.

## 6. Composite Commit and explicitly staged workflows

Recommend `HistoryCommand::Commit(PreparedChanges)` for an ordinary explicit
Workspace Commit when the caller did not request a pause after staging. It
calls the same staging body followed by the same `commit_staged` body under
one service admission. It does not add another history implementation or merge
the C2/C5 failure boundaries. [Composite command][history-commands]

```text
 ordinary explicit Commit:
   file save 1 ... file save F --> Commit(PreparedChanges)
                                    |
                                    +--> filesystem save
                                    +--> stage T
                                    `--> commit T / consume T

 explicit stage pause:
   file save 1 ... file save F --> StageChanges(PreparedChanges)
                                    |
                                    `--> stage T retained
                                          ... caller decides ...
                                          CommitStaged(workspace, T)
```

With F file saves and no additional prerequisite operations, these require
F+1 and F+2 logical request/result exchanges respectively. Required base reads,
initial descriptor queries and future metadata prerequisite operations are
additional work, not omitted cost. Composite Commit still needs no wire payload
body: the stable file bytes were submitted in prerequisite content operations.

One stage may exist per Workspace producer incarnation. Another StageChanges
does not overwrite it; it is refused. Do not evade this by changing producer ID.
An explicit stage pause retains exact context and blocks another save for that
incarnation until its disposition is known. G+1 may remain live only within the
same charged budget; retaining a stage does not authorize unbounded generations.

The proposed runtime additionally admits **one retained frozen submission per
consumer**, across that consumer's Workspaces. A per-Workspace submission slot
and the consumer admission are distinct from the service's two active-operation
capacity. Known failure, unresolved staging and unknown outcome continue to own
the required G admission until a valid disposition accounts for G, D1 and all
references. Another Workspace in that consumer cannot silently capture a second
G merely because the network call returned. An idle retained stage releases its
active client/transport permit; it does not hold a state mutex or socket forever.
Local supported work can continue within the remaining RAM/disk budgets. The
[overlay admission rules](02-overlay-snapshot.md#33-local-commit-serialization)
own the state transitions; there is no background retry queue.

## 7. Known completion and live successor reconciliation

On StageChanges success, compare StageWire with the request/G association:
Workspace, Branch, generation, expected head/base/root, construction base,
intended Commit base, scope/profile and candidate context. Retain its exact
token. Current client response matching checks result shape and bounds, not
every association needed by the runtime; that association belongs to Workspace.

CommitStaged takes only Workspace+token. Its success does not echo a generation
or provide a per-inode completion stream. Composite Commit likewise returns its
history outcome, not a live-state patch. Retain the request-to-G association,
the known file results and per-inode revisions until reconciliation completes.

| Known result | Meaning and local transition |
| --- | --- |
| `Stage(StageWire)` | Candidate/root/token are acknowledged; Branch is unchanged. Retain G, its descriptor and any G+1 data. |
| `Committed(Committed(CommitWire))` inside `HistoryResult` | A Commit was inserted/verified, Branch head conditionally advanced and exact stage consumed. Record returned own head/root/base and acknowledge only G. |
| `Committed(UpToDate { head, root })` | Exact stage consumed because candidate root and intended base already match the unchanged expected Branch. Record returned head/root; invent no Commit. Acknowledge only G. |
| `Discarded { removed }` | Exact stage removal/absence result only. It neither reverts the local Workspace nor discards newer revisions. |

The nested names above describe `HistoryResult::Committed(CommitOutcomeWire)`;
they are result variants, not another operation or wire format.

```text
 at capture:             during save:                  known completion:

 inode A revision 4 ----> unchanged revision 4 -------> base may become saved A
 inode B revision 9 ----> write -> revision 10 -------> KEEP revision 10 changes
                          ^                            retain dependencies still
                          |                            used by B/G+1/readers
 frozen G keeps rev 9 ----+--> save only rev 9

 Branch context Ctx0 -------------------------------> own acknowledged Ctx1
 next snapshot uses Ctx1, retaining/re-expressing G+1's remaining changes
```

An unchanged live inode can adopt the acknowledged saved base. An inode changed
since capture must keep its successor edits and old roots/segments they still
reference. Advancing the Workspace's known Branch context must not orphan those
dependencies. The next snapshot must be expressed against the actual root/head
established by this known own completion. Reconciliation may need to normalize
the remaining successor state; changing expected tokens alone is insufficient.

This is reconciliation of acknowledged own edits, not an automatic rebase onto
someone else's later Branch. Do not use a current GetBranch result to overwrite
the descriptor of an in-flight or losing G. Stage success alone cannot establish
the next acknowledged Branch base.

### 7.1 Repeated incremental Commits in the same Workspace

A successful Commit does not discard the Workspace, unmount it, recreate the
namespace, or require another Fork. Keep its identity, handles, local successor
and acknowledged Branch context. Each next capture contains the dirty frontier
since the previous captured state, expressed against the last **known own**
acknowledged filesystem root. Advance that root and expected head together only
after the corresponding Commit outcome is known.

```text
 same Workspace W, same Branch B, same mount throughout

 acknowledged:  root R0 / head K0 / base Layer L0
                      |
 local edits:         +-- change file A -> capture G0
                      |                   save changed A + Commit(PreparedChanges)
                      |                   base R0, expected K0/L0
                      v
 acknowledged:  root R1 / head K1 / base Layer L0
                      |
 local edits:         +-- change file B -> capture G1
                      |                   save changed B + Commit(PreparedChanges)
                      |                   base R1, expected K1/L0
                      v
 acknowledged:  root R2 / head K2 / base Layer L0

 K2.parent = K1; K1.parent = K0 when K0 exists
 unchanged A in the second capture keeps its acknowledged canonical content root
 no full Workspace export, fresh initialization or Layer publication between cuts
```

The current service validates that PreparedChanges.base is the Branch's effective
root and checks the expected head/base Layer before construction. StageChanges
performs the C1/C2 filesystem update itself. Do not first construct a complete
filesystem with UpdatePreparedFilesystem, and do not send an arbitrary candidate
root as a substitute for PreparedChanges. [Base validation and staging](https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/history.rs#L403)

| Scenario | Next capture and service work | Reuse / limit |
| --- | --- | --- |
| First Commit changes A; second changes B | G0 saves A; after known K1/R1, G1 saves B and changed filesystem records against R1 | A's content root and unchanged namespace structure remain referenced; no resubmission of A's full bytes |
| Change the same file several times before capture | Lower its final piece sequence into bounded edits against the current acknowledged file version | Overwritten local bytes need not appear as intermediate saved file versions; retained read/snapshot owners still pin any bytes they use |
| Change A again while G0 saves A | G1/D1 keeps its G0-relative immutable file base and newer edits; after exact G0 success resolve that base to its saved root, then save G1 against R1/K1 | Saving G0 cannot clear G1; changing an old root without correcting old coordinates is invalid |
| Rename a descendant without changing file bytes | Submit changed source/destination bindings and required inode/metadata changes through PreparedChanges | No file-content EditFile is required solely for the rename; C1/C2 filesystem construction, topology checks and history work still occur |
| All canonical content, namespace and metadata return to the acknowledged state | Normal explicit Commit may return UpToDate after exact stage/context checks | File-byte equality alone is insufficient; mtime or another metadata change can make a new root |
| Another Workspace advances the same Branch | Preserve the captured root/head and report the stale/conflict outcome | No automatic refresh, replay, merge or adoption of the competitor's root |

New-inode attachment, live portable metadata and the mounted write path are
implemented and evidenced at `f802cc124`; [01 §7.2/§7.4](01-workspace-fuse-contract.md#72-selected-w-operations-implemented-and-evidenced)
record the per-operation receipts and reproduction commands. A rename's
file-content reuse still does not excuse POSIX timestamp handling: the mounted
`setattr` evidence covers chmod, `UTIME_OMIT` mtime and mode preservation.

#### Successor-root content after a known own Commit

Four implemented rules govern what the successor root carries and how live
state re-baselines when the Commit outcome is known (all in
`commit/reconcile.rs`, `commit/directories.rs` and `runtime/state.rs`):

- **The reconcile carry with its live-owner filter.** A replaced or unlinked
  committed identity — one whose last local name went away while the attached
  base still owns it — is recorded at replacement/removal time and carried into
  the successor root as an `Unbound` frontier record (no dirty key, no
  declaration), but **only while a live local owner can still address it** (an
  open handle or a lookup reference). An identity whose handle and lookup
  references are both gone has no owner left to serve, so nothing is carried
  for it and the arena can be reclaimed. Carrying unconditionally keeps the
  arena unreclaimable after its last owner is gone — that regression is why
  the filter exists.
- **The declaration ledger.** The submission records the exact directory
  declarations it sends; a completed Commit forgets exactly those, so a
  directory a later generation created while the submission was in flight
  stays undeclared and the next Commit declares it. Clearing the ledger
  wholesale loses such a directory.
- **Rename replacement semantics.** The replaced identity's record is
  preserved for its open handles and moves to the successor root, so a held FD
  keeps reading its own inode and bytes across the Commit instead of asking
  the service for a name that is gone; the moved identity's own record is
  published when the live root holds none ([02 §2.3](02-overlay-snapshot.md#23-precedence-and-tombstones)).
- **The canonical link-count re-baseline.** The successor root republishes an
  identity's canonical namespace link count, and the resident node refreshes
  to it rather than refusing: `cache_lookup`'s references comparison is
  baseline-gated exactly like the adjacent original/roots staleness check, and
  `serial_original`'s stale-baseline query checks serial and kind only
  (`filesystem/namespace.rs`, `filesystem/original.rs`). The bounded corner —
  a re-resolved base identity whose delta link-count adjustment was lost with
  its resident node presents the canonical count until the next Commit — is
  declared in [01 §8.1](01-workspace-fuse-contract.md#81-attribute-projection).

These rules are evidenced by `round57/final3-ns-unlink-01` (open-orphan record
across Commit), `round57/final3-ns-rename-01` (the replaced FD readable after
its Commit), `round57/final3-ns-generation-01` (a later generation's namespace
and metadata changes survive G completion),
`round54/final3-mkdir-successor-01` (a directory the successor generation
created is declared by the next Commit) and the mounted durability selection
`round57/final-ns-mounted-durability-01`.

Incremental input does not mean that every operation touches only the modified
bytes or costs O(number of changes). The existing algorithms provide these
specific reuse properties:

| Level | Existing reuse | Work still charged to the operation |
| --- | --- | --- |
| File with no edits / identical replacements | Existing file root can be returned | Base validation and, for equality detection, actual comparison reads |
| Whole-file representation | Existing root survives a true no-op | A changed whole-file result is assembled and constructed as a complete canonical object |
| Chunked file with chunked base | Retained mapping subtrees are joined with newly constructed replacement regions | Boundary reads, byte comparisons, replacement construction and joins |
| Whole-file base becoming chunked | New chunked result | Combined retained/replacement bytes stream through complete construction; no old mapping tree exists to reuse |
| Directory and inode trees | Sorted update engine reuses untouched subtrees; an empty change stream returns the existing root | Immediate child-page authentication at visited branches, affected leaves, sibling redistribution, encoding/hashing |
| C2 canonical storage | Existing exact objects can be reused | Membership lookup, authentication/reconstruction and exact byte comparison |

The default C1 whole-file construction cutoff is 128 KiB exclusive; it is a
content representation policy, unrelated to the reference's 8 KiB FUSE prefetch
cutoff. [Construction policy](https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/policy.rs#L13),
[file edit paths](https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/file/edit/apply.rs#L48),
[inode update](https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/filesystem/inode/update.rs#L49),
[sorted reuse](https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/filesystem/sorted/finish.rs#L19),
[sibling work](https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/filesystem/sorted/merge.rs#L93),
and [C2 reuse checks](https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-storage/src/cas/save.rs#L21).

Directory/symlink parent-alias validation can walk the base namespace, and release
of unreferenced descendants can require traversal. A small namespace change may
therefore perform broad bounded validation or hit its explicit work ceiling.
Neither subtree reuse nor dirty tracking removes that requirement. Operation-local
C1/C2 caches do not establish free predecessor reads across successive Commits.
[Alias validation](https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/filesystem/validate.rs#L304)

Only a later matched measurement can establish the cost of repeated Commit. It
must include those predecessor reads, construction, save and history boundaries
from the declared cache state. No performance result is inferred here.

### 7.2 UpToDate is deliberately narrow

The exact-token check and stale head/base check precede candidate equality.
An equal root on a newly advanced Branch does not rescue a stale stage. The
UpToDate path consumes the exact stage without creating a Commit; it does not
turn a repeated request into an idempotent replay. Resending a consumed token
returns StageChanged: the actual stage is absent if no stage exists, or names the
different current token if a later generation has since been staged for that
Workspace incarnation. Neither observation is the earlier success or permission
to adopt that other stage. [Commit ordering][catalog-commit]

An unchanged explicit Commit is not automatically free: StageChanges still uses
the normal filesystem construction/save path before C5 can decide UpToDate.
Empty sorted updates may reuse roots, but filesystem-root emission and C2 checks
remain real work. A local status of clean is a local observation, not a service
Commit receipt or proof that another producer has not advanced the Branch.

### 7.3 End-to-end work ledger

This ledger identifies work and ownership that the implementation must preserve
and expose for later verification. It gives no latency, throughput or asymptotic
guarantee. COW avoids copying payload at capture and preserves unchanged data; it does
not eliminate validation, authentication, physical reads or acknowledged saves.

| Phase | Input / work actually owned | Reuse and required observation |
| --- | --- | --- |
| Accepted local mutation | Reserve indexed metadata and backing space, write new payload ranges, prepare COW versions, publish one coherent local revision | Reuse inherited canonical ranges and immutable extents. Account payload written, metadata pages copied, working allocation and retained disk ranges; never copy up an unchanged complete file. |
| Capture G | Acquire the selected Workspace/consumer submission admission and retain immutable view/frontier roots | No payload copy, spool clone, whole-namespace walk, complete dirty-map export or service request during the cut. Count retained descriptors/frontier roots; defer traversal and large destruction outside the state lock. |
| Enumerate/lower G | Stream only captured changed identities and their final changes from owned indexes; normalize supported file edits and changed-name records | Do not rescan every base path to rediscover dirty state. Count changed inode versions, names, edit records, local index pages read, replacement bytes and encoded/decoded input sizes. Required topology/base queries remain separately charged. |
| Acquire/send file input | Read immutable G extents into bounded Source buffers and deliver exact declared bytes once per selected file operation | Count useful local bytes, physical backing read bytes, transmitted BODY bytes/frames and held source references. Zero spans count as delivered replacement bytes even if they use no local payload extent. |
| C1 file operation | Open/authenticate base, compare applicable replacements, assemble whole-file result or update chunk mapping, emit final objects | Count actual base/mapping reads, compared bytes, replacement bytes scanned, retained subtrees and emitted objects. Retained mapping identity is reuse, not evidence that every validation or physical read was avoided. |
| C2 file save | Membership/dependency checks, exact-reuse comparison, selected encoding/placement and finish | Count inserted/reused objects and actual storage reads/writes/work. Accept only this operation's successful terminal; a matching root published by another writer cannot replace it. |
| C1/C2 filesystem stage work | StageChanges validates original Branch context/roles, merges changed bindings, derives references, updates the inode tree, emits filesystem root and finishes one save | Count directory/inode pages read, reused and created; changed keys; alias/cycle entries examined; descendant release work and root-save result. No separate pre-staging UpdatePreparedFilesystem. |
| C5 stage and Commit | Short stage insertion, then exact-token/context check, optional Commit insertion, conditional Branch advance and exact stage consumption | Distinguish committed, UpToDate, stale, refused and unknown outcomes. Count operation/transaction boundaries separately from C2; an UpToDate result still followed required preceding work. |
| Local reconciliation | Validate request/G/result association, install known own acknowledged context, preserve D1 and release only unreferenced state | Count covered versions, remaining dirty versions, retained old extent/index pages and completion-map records. No whole-workspace clear, root-only reinterpretation of old coordinates or unbounded deferred-free queue. |

Existing C1 `FilesystemUpdateCounters` distinguish directory/inode sorted work,
validation, reference reduction and descendant release. File edit counters and
C2 save outcomes expose other parts. These counters do not all cross the bridge
as terminal result fields today: any additional observation must use the actual
operation's existing telemetry/public result boundary or an explicitly selected
production telemetry extension, not a private canonical-object protocol or product
test-only path. [Filesystem counters][filesystem-counters]

For an admitted captured generation, define:

- **F**: distinct captured inode versions whose file payload is submitted. Two
  hard-link names for one version count once; unchanged payload is not another
  file save merely because metadata or a pathname changed.
- **Q_setup**: required descriptor/lifecycle queries or commands charged to this
  selection, rather than assumed free from an earlier setup.
- **Q_read**: actual logical base-range/inspection calls required by this Commit
  on the caller side. Service-local C1 canonical reads are separate work, not
  extra bridge operations.
- **Q_meta**: additional agreed metadata prerequisite operations, if the selected
  future writable contract introduces them. Such operations are not supplied by
  the current general live-metadata API; use zero only where no such work is
  required, not to conceal missing POSIX timestamp handling.

```text
 current supported shape, with all prerequisite semantics present:

 composite Commit exchanges = Q_setup + Q_read + F + Q_meta + 1
 explicit stage + Commit     = Q_setup + Q_read + F + Q_meta + 2

 file input bytes = sum(EditFile replacement bytes, including zeros)
                  + sum(ConstructFile declared complete lengths)

 immutable bytes preserved by reference are not resent as EditFile BODY bytes;
 their service-side comparison/reconstruction reads remain in the work ledger.
```

These are logical request/result counts, not TCP RTT or latency estimates.
Connection/TCP/Noise/HELLO work and failed/refused attempts are recorded
separately. For N positive input bytes, `ceil(N / 16 KiB)` is a lower bound on
BODY frame count; smaller source reads may produce more frames, and framing,
END_INPUT and terminal metadata add bytes. Native record batching does not change
logical operation or frame semantics. An empty history input still requires
its exact empty-input boundary and terminal result.

Disk capacity is not an efficiency substitute for a missing operation. The
current bound applies to each whole prepared request, not independently to
successive hidden chunks. F+1 describes one admitted generation only after its
complete PreparedChanges fits the 128-record and 32 KiB bounds; it is not a
recipe for an arbitrarily large npm Commit. A future streaming or composite
shared operation must explicitly settle resource ownership, final-root
construction, one-user-Commit semantics and partial/unknown outcomes before its
different counts can be claimed. Until then, report the blocker rather than
calling repeated partial Commits one efficient atomic Commit.

## 8. Failure context is part of the lifecycle result

Preserve `Failure.code`, `unknown`, `cleanup` and the complete optional
`HistoryFailure { conflict, stage }`. A FUSE errno alone cannot represent a
Commit's outcome. The proposed explicit Workspace lifecycle result must expose
this bounded typed state together with its request/G association.

| Code family | Interpretation |
| --- | --- |
| `InvalidInput`, `Unsupported`, `Denied`, `Capacity` | Preserve the original class and phase; do not turn validation/capability/authorization/resource refusal into missing content. |
| `Ownership`, `Busy` | Storage ownership versus immediate history-provider admission. Preserve the difference. A known Busy refusal is not permission for automatic retry. |
| `MissingObject`, `PathNotFound`, `NotFound` | Missing canonical content, missing filesystem name, and missing history record are separate failures. |
| `Provider`, `Integrity`, `Io`, `Deadline` | Preserve available cause; never infer a successful save or namespace absence from them. |
| `HeadMoved` | Typed conflict may be BranchMoved, StackMoved or BaseMismatch. Expected/actual context explains the failed publication precondition. |
| `StageChanged` | Named exact token differs from the observed token, or no stage was present. It is not a replay receipt. |
| `ContinuityUnavailable` | This provider has no writable history authority; block history mutation and allocation. |
| `Unknown`, or `unknown = true` on another code | Outcome cannot be established. Retain input/context; no replay, guessed rollback or cleanup. |

Failure context carries `HistoryConflict::BranchMoved`, `StackMoved`,
`StageChanged` or `BaseMismatch`. Preserve the exact expected/actual values from
the deciding operation; do not replace them with a later mutable reread.
[Failure types and mapping][history-failures] [Repaired failure contract][repaired-contract]

| StageObservation | What it proves / what Workspace retains |
| --- | --- |
| `Unobserved` | No stage observation was made. It does not prove absence. |
| `Absent(workspace)` | The deciding boundary observed no stage for that incarnation. It does not prove an earlier request succeeded or rolled back. |
| `Retained(StageWire)` | After known refusal and successful cleanup, the deciding transaction retained the observed row. First compare its token/context with G: it may be the **different actual stage** encountered by a stale-token request. |
| `AcknowledgedUnknown(StageWire)` | This stage was acknowledged earlier, but its final disposition is not established by the later failure. It is not proof that the row remains present. |

Stage observation and the outer unknown flag are independent. Composite Commit
can acknowledge staging and then fail to enter the Commit transaction with Busy;
its context can be AcknowledgedUnknown even though the outer failure is a known
Busy refusal. Preserve both facts. Conversely, a stale CommitStaged request can
return an actual retained stage belonging to another local attempt. Record that
observation separately; never attach its candidate or token to G without matching
Workspace/token/generation/context.

### Worked failure A: Branch movement around tree save

```text
 caller G captured K0/L0/R0        service / competing writer
 ------------------------        --------------------------
 StageChanges(G) ----------------> early check sees K0/L0
                                  C1/C2 builds candidate R1
                                  competitor commits K2
                                  this C2 finish succeeds
 <------------------------------ stage T retains K0/L0 expectations

 CommitStaged(T) ----------------> sees Branch K2, not expected K0
 <------------------------------ HeadMoved + retained exact stage T

 keep G/G+1 + T; no token refresh, no automatic rebase or resend
```

If the early check already sees K2, StageChanges refuses before its tree work;
there need not be a stage for that attempt. Those two failures must not be
collapsed into one invented rollback story.

### Worked failure B: file content has the same hash as another save

```text
 A submits file bytes -----------> A finish fails / reply becomes unknown
 B submits identical bytes ------> B finishes and publishes root F
 caller can now read F

 read F / root equality != evidence that A finished successfully
 A retains frozen input and does not manufacture StageChanges from this fact
```

No success, receipt or completion status may be transferred between the two
operations merely because canonical identity is equal.

### Worked failure C: lost composite terminal

```text
 Commit(G) ----------------------> filesystem save succeeds
                                  stage T inserted
                                  Commit K1 + consume T may have completed
          X terminal connection loss

 caller: unknown, retains G/G+1 and exact request association
 optional explicit observation on healthy continuing authority:
     GetStage / GetBranch / history -> current state, NOT original receipt

 absent T or root == R1 alone cannot establish the lost request outcome
```

If C5 itself encounters unknown persistence outcome, its provider is
quarantined and refuses subsequent queries as well as mutations. An explicit
reread through that provider cannot repair it. If only the transport was lost
and authority remains healthy, explicit queries are available but still do not
constitute a replay or an original-operation status API.

## 9. Commit, Layer publication and discard

```text
 LayerStack S: head L0
                    \
 Branch B: base L0, head K0, effective R0
                    |
        StageChanges(G) -> T, candidate R1       B unchanged
                    |
        CommitStaged(T) -> K1, B head K1          B base remains L0
                    |
        AddLayer(B, K1, expected stack L0/base L0)
                    |
 LayerStack S: head L1                           B base STILL L0
                    |
        explicit Fork(B2 from L1) -> GetBranch(B2)
                    |
        deliberate clean/new Workspace binding for next publication cycle
```

AddLayer is a separate explicit action, not part of ordinary Workspace Commit
or FUSE flush/close. It returns `Added(LayerWire)`, exact-source
`UpToDate { layer }`, or `NoChanges { head }`. UpToDate names an existing
publication; it need not be the current stack head after later publications.
Publication does not advance the source Branch's base. A further Commit on B
is allowed and does not require a new Fork, but it does not remove that old-base
constraint for publishing another Layer. Establish a later publication cycle
through an explicit fork from the published Layer or future rebase support.
Never silently retarget G+1 or change `/workspace-id` to perform it.

DiscardStage removes only the named exact stage row. It does not delete
objects, revert live files, erase G+1, release bytes still referenced by readers,
recycle serials, delete a Branch or implement GC. A mismatched token cannot
discard a different row. A successful absence result is likewise a stage
observation, not permission to clear unrelated local state. Local revert/discard
needs its own explicit semantics and snapshot/reference accounting.

## 10. Complete history operation/result map

The following names are the current [public union][history-contract]. Queries
are profile 2/opcode 6; commands are profile 2/opcode 7. No new opcode or local
catalog method is introduced by this document.

| Query | Main inputs | HistoryResult |
| --- | --- | --- |
| `GetStack` | stack | `Stack` |
| `ListStacks` | cursor, limit | `Stacks { continuation, records }` |
| `GetBranch` | branch | `BranchSnapshot` with validated root serial |
| `ListBranches` | stack, cursor, limit | `Branches` |
| `GetCommit` | commit | `Commit` |
| `CommitHistory` | branch, optional start, cursor, limit | `Commits` |
| `GetLayer` | layer | `Layer` |
| `LayerHistory` | stack, optional start, cursor, limit | `Layers` |
| `GetStage` | producer incarnation | `Stage`; absence is history NotFound |
| `ListStages` | branch, cursor, limit | `Stages` |

| Command | Main inputs | HistoryResult / ownership |
| --- | --- | --- |
| `InitLayerStack` | authority stack body, name, scope seed, bounded manifest | `StackCreated`; constructs/saves genesis, creates stack, no Branch |
| `Fork` | stack, authority Branch body, name, Layer or authorized ancestor Commit source | `BranchSnapshot` without validated root serial; metadata only |
| `StageChanges` | PreparedChanges | `Stage`; owns filesystem construction/save then stage insertion |
| `Commit` | PreparedChanges | `Committed(CommitOutcomeWire)`; same stage and Commit bodies, one admission |
| `CommitStaged` | producer incarnation, exact token | `Committed(CommitOutcomeWire)`; metadata only |
| `AddLayer` | stack, branch, commit, expected stack head, expected Branch base | `Published(LayerOutcomeWire)`; metadata only |
| `DiscardStage` | producer incarnation, exact token | `Discarded { removed }`; metadata only |
| `ReserveInodes` | scope, count | `Reservation { scope, start, count }`; metadata allocation only |

Queries and commands use the same service authorization. A history operation
requires a configured catalog; missing catalog configuration is Unsupported,
not an implicit fresh catalog. Reservation success consumes the range even if
unused or followed by failure/discard. It does not grant permission to attach
new inodes through the existing-only prepared builder.

The history cursor is an opaque authenticated continuation. Return it unchanged
with the same query context; do not decode it, synthesize it or replace its
immutable ancestry anchor with a moving head. Paginated history is not an
unbounded Workspace history cache.

## 11. Connection lifetime, restart and uncertainty

| Situation | Available behavior |
| --- | --- |
| Another request on the same healthy connection | Uses a new increasing correlation ID and the same authorized service/catalog. No implicit replay. |
| New authenticated connection, same continuing service/catalog | Can explicitly consume a previously acknowledged stage token or issue bounded queries. Stage identity is not connection-local. |
| Lost mutation terminal | Retain unknown caller state. A new connection does not make resubmission safe. |
| C5 unknown transaction outcome | Provider quarantines; queries/mutations refuse. No reread/repair or guessed rollback. |
| Service process restarts and opens existing catalog | Read-only open is supported. Writable continuity is not re-established; mutations/reservations return ContinuityUnavailable. |
| Daemon loses live Workspace state | Saved C5 records and leftover disk segments do not reconstruct arbitrary pending pieces, successor writes or open handles. No general runtime recovery is supplied. |

The current C5 creator owns writable continuity while that authority continues.
Reopen cannot prove exposed serials were not already consumed. No clean-exit
flag, assumed-clean input, root scan or new scope supplies that proof. Workspace
must not silently downgrade a requested write, create replacement authority or
pretend an empty overlay means safe recovery. [Catalog continuity][catalog-open]
[Provider quarantine][catalog-rows]

`GetStage`/history queries make saved state inspectable, not offset-resumable
uploads or receipt-based mutation replay. Transfer resumption, general conflict
merge/rebase, writable restart, GC and recovery remain outside this integration.
The only supported live continuation proposed here is G+1 alongside frozen G,
with exact known-result reconciliation and bounded references.

## 12. Bounds consumed by Workspace

These are **implemented API/resource limits at the pinned source**, not measured
throughput, mounted acceptance or authorization to enlarge Pair 1's budgets.

| Shared boundary | Current limit |
| --- | --- |
| Declared file/input/read-response allowance and accepted inspected file length | 4 GiB; old 64 MiB observations are superseded |
| Edit replacement bytes / edits | 8 MiB / 256; base/final lengths obey the file limit |
| Prepared changes | 128 directory records, 128 inode records, 128 total changed-name bindings |
| Body/result-data frame / request metadata | 16 KiB / 32 KiB |
| Frame budget | `ceil(bytes / 1024) + 257`, with direction/terminal checks; no old fixed 8,192-frame claim |
| Filesystem path / component name | 4,096 / 255 bytes |
| Filesystem list page | 128 entries and 16 KiB; names/serials only |
| History name / cursor | 63 bytes in its checked name grammar / 160-byte authenticated continuation |
| History page / complete encoded result | 128 records maximum / 16 KiB including tags and continuation |
| Profile-2 failure / legacy failure | At most 482 bytes / exactly three bytes |
| Init manifest / symlink target | 128 entries including root / 4,096 bytes per target, also within total request metadata |
| ReserveInodes | 1-65,536 serials; checked nonrecycled half-open range within signed integer bounds |
| New explicit history ancestry membership proof | At most 4,096 examined rows; authenticated bounded-page continuation is a different operation |
| Complete service operations | 2 service-wide, including reads; no waiting service queue |
| C2 private saves | 2 per Store, including quarantined/failed-cleanup ownership |
| Admitted sessions / accept-refusal resource | 4 / 1 |
| Operation maximum / native bidirectional I/O progress | 600,000 ms / 5,000 ms |
| Connect / handshake / idle | 5 seconds |
| Native encrypted-record batch | 256 KiB flush-after-append threshold; one appended record can cross it. Not a strict allocation ceiling or change to frame size |

[Request limits][request] [History limits][history-contract]
[C2 save slots][save-slots] [Native connection][connection]

Record counts and encoded-byte limits both apply. For example, 128 individually
valid large records need not fit the total metadata/result allowance. Encoded
sizes are not decoded allocation costs: charge vector capacities, byte arrays,
StageWire/failure boxes, descriptors, per-file completion maps and retained
cursors/results to the proposed Workspace ownership budget.

Keep the proposed **8 MiB accounted Workspace-allocation target per consumer**,
including G/G+1/read references, resident index/frontier pages and integration
state. Payload uses the explicitly selected disk backing, with its own declared
quota; it is not an automatic spill after a RAM-only attempt fails. Scalable
metadata may need owned indexed local backing, but retaining an unlimited RAM
map or rebuilding one at Commit defeats the target. The overlay document owns
the backing layout, reservation/retirement and paging algorithm; no disk quota
or new backend is selected here.

The service's 4 GiB file limit increases neither local RAM nor disk quota.
Charge bridge allocations, native batches/stacks, kernel/page-cache state and
service C1/C2/C5 separately; do not call the 8 MiB target total RSS. Frozen,
unknown, old-reader and D1 extents remain charged to their actual owners until
release. Low heap usage plus file-size-proportional kernel cache is not success.
Refuse unavailable local reservations before visibility, while reporting shared
operation limits as unresolved capability boundaries; do not shrink a registered
selection or silently split one requested Commit to obtain acceptance.

`Client::call_until` now accepts an absolute caller-local deadline and sends the
remaining duration. Keep the earlier **10-second FUSE callback budget** as a
separate proposed policy; the 600-second protocol maximum does not increase it.
An explicit lifecycle operation needs one declared complete caller budget across
all prerequisite saves and history calls, not a new budget per child operation.
These protocol values never relax benchmark command budgets. [Client deadline][client]

Service admission 2 includes reads; C2's two private save slots do not guarantee
fairness, mounted neighbour progress, two simultaneous SQL writers or successful
history admission. C5 uses short nonblocking provider admission and can return
Busy. Preserve the single construction producer, one local in-flight save and
the proposed one-retained-frozen-submission admission per consumer.
This document changes no concurrency configuration or limits.

Authenticated caller evidence now resides in bridge `contract/caller.rs`, but
current transport delivery still maps read errors to Io and mutation delivery
errors to Unknown. Generic I/O conversion loses the socket timeout kind; typed
history errors do not repair that POSIX errno gap. The current fuser/client
combination also supplies no immediate external interrupt cancellation here.
Reuse bounded delivery, preserve the known distinctions and qualify lifecycle
cleanup without asserting cancellation or rollback that is unavailable.

## 13. Implementation ownership and verification status

Under `core/crates/layerfs-workspace/src/`, `commit/lower.rs` creates the stable
operation inputs; `commit/save.rs` orchestrates descriptor/file/stage/Commit/
publication actions; `overlay/snapshot.rs` owns frozen/live generation
transitions; `runtime/state.rs` records the single local state machine.
The library is assembled by the daemon and consumed by `layerfs-fuse`; its
`commit` folder does not own service execution or C5 SQL. These are proposed
responsibilities, not files already implemented by this document. Do not add
parallel stage registries, a daemon-owned C5 catalog, or a second transport client.

The audit resolves the document's former ambiguity about RAM-only payload,
dirty-frontier discovery, per-consumer frozen admission and the cost hidden by
the word incremental. It preserves the source-backed reuse and exact-result
rules. The following implementation questions remain open, with proof work
assigned to [04](04-implementation-and-verification.md):

| Open implementation requirement | Proof needed before claiming it |
| --- | --- |
| Low-RAM disk source and index/frontier traversal | Peak accounted resident working state remains bounded while a complete selected large/tiny-file input is retained on disk; source/index I/O and kernel residency are observed separately. No complete-file or complete-dirty-map materialization at capture/submission. |
| One save per captured changed payload version | Hard-link aliases and metadata-only changes do not duplicate file submissions; each generation takes only one StageChanges/Commit filesystem construction path. |
| Repeated own Commit with concurrent D1 | Next input is based on the exact acknowledged R1/K1 and exact captured file roots, with successor bytes/metadata/names unchanged across reconciliation; old reader sources remain valid. |
| Incremental work accounting | Existing subtree reuse is distinguished from child-page reads, repartition, comparisons, broad validation, release and C2 reuse authentication. Empty/UpToDate work is retained in the ledger. |
| Large single-user-Commit contract | More than the current prepared-record/metadata or edit-byte limit is handled only by an implemented agreed shared operation with its explicit failure boundaries; no hidden chain of partial Commits. |
| Writable file/namespace semantics | Live mtime/mode, new inode and symlink operations exist and their roots are valid before the corresponding input is admitted; disk backing alone is not sufficient. |
| Frozen backlog and failure retirement | One unresolved G retains its consumer admission and actual disk/RAM ownership; blocked captures, active network permits and ordinary local operations have distinct observable outcomes. No request failure silently frees or replays G. |

These are proof requests, not another aggregate test wrapper or a claim that a
new performance campaign has been selected. Functional work/identity observations
must use the real implementation; numeric performance/resource conclusions still
require the separately declared measurement protocol.

The [implementation plan](04-implementation-and-verification.md) must verify:
descriptor provenance; exact-save prerequisite association; metadata correctness;
G/G+1 retention and reconciliation; stale versus UpToDate behavior; different
actual-stage context; lost terminals; quarantine/continuity; publication/base
separation; and both encoded and decoded resource limits. Tests exercise public
production operations, with actual mounted behavior where claimed.

Pair 2 implementation is merged, but [#210](https://github.com/Ephemeral-AI-Lab/layerfs/issues/210)
retains H04 real overlapping/reverse-completion saves, H06 independent-stack
overlap, H08 boundary/identical-root failure schedules and H14 unchanged-consumer
substitution qualifications. Those are not completed by reading source or by
this design. [Pinned qualification status][qualification]

No build, benchmark, mount, issue closure or new passing result is produced by
this document. MEMORY journal, synchronous OFF, no fsync and no WAL remain;
neither the name Commit nor a retained stage introduces durability. The
`init_namespace` multi-worker exception remains separate from ordinary
Commit/snapshot construction and is not redefined here.

[history-contract]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/contract/history.rs
[prepared]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/contract/history.rs#L154
[request]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/contract/request.rs
[client]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/adapters/native/client.rs
[connection]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/adapters/native/connection.rs
[service-owner]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/owner.rs
[service-history]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/history.rs
[get-branch]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/history.rs#L126
[history-commands]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/history.rs#L318
[stage-body]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/history.rs#L403
[history-failures]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/history.rs#L34
[filesystem-update]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/filesystem.rs
[bootstrap]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/history_bootstrap.rs
[catalog-commit]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-history/src/sqlite/commit.rs#L83
[catalog-open]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-history/src/sqlite/open.rs
[catalog-rows]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-history/src/sqlite/rows.rs#L73
[save-slots]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-storage/src/sqlite/ownership.rs#L12
[repaired-contract]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/docs/architecture/proposal/commit-history/remediation-contract-20260921.md
[qualification]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/docs/architecture/proposal/03-history.md
[file-write]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/write.rs#L19
[filesystem-counters]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/filesystem/update.rs#L32
