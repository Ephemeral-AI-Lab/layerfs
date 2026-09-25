# Workspace overlay and coherent snapshot

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> Dated 2026-09-23. This specifies the proposed daemon runtime algorithm and its
> required proofs. The records below are conceptual design notation, not existing
> Rust APIs. Section 2.3's precedence rules were refreshed in the issue-179
> documentation round against product source `f802cc124` to state the implemented
> binding-versus-removal, shadowing, listing and re-anchor behavior; the
> binding-versus-removal rule and the metadata key bound below were extended in
> the issue-245 repair round, whose implementation commit is `a0258cd6f`; the
> design notation elsewhere remains pinned to
> `152b9c3a2e8ec2536a1d63601b681e1f7ef34455`. No performance or memory result is
> claimed.

Packet: [overview](README.md), [Workspace/FUSE contract](01-workspace-fuse-contract.md),
[Commit integration](03-commit-integration.md), and
[implementation/verification](04-implementation-and-verification.md).
This paper owns local visibility, snapshot and resource semantics. It consumes
the existing service operations; it does not restate or implement the C5 catalog.

The current target is **explicit local disk backing for pending payload**, with
bounded resident metadata and I/O buffers. It serves the owner's `npm install`
and mixed large/tiny-file workload direction. The earlier RAM-only 8 MiB pending
payload candidate is historical; it cannot satisfy that larger dirty-payload
envelope. This revision preserves its useful version/ownership rules while
replacing its backing assumption throughout the active design.

The **8 MiB aggregate Workspace allocation** remains an unqualified RAM target,
not a chosen disk quota or proof that a whole changed namespace fits in memory.
Disk quotas, segment layout, residency enforcement, metadata paging and the
required scalable shared operations remain explicit implementation decisions.
Nothing here silently changes an upstream bound, introduces automatic spill or
claims that `npm install` is already supported. See §10 and [04](04-implementation-and-verification.md).

## 1. Target and ownership

The full writable target allows ordinary supported filesystem operations to
continue while a previously captured state is being saved. It retains one frozen
generation G and one live successor, called D1 or G+1 below. A prototype that
blocks writes until save finishes is a bring-up step, not this target and not an
equivalent replacement for the reference's continued live work.

The daemon assembles `layerfs-workspace`, which owns the live Workspace and its
private local disk backing. `layerfs-fuse` delegates to its public semantic API;
no second copy of namespace, file lifetime, revisions or
pending bytes belongs to the adapter. RAM holds admitted buffers, resident
metadata pages and small version/operation descriptors; accepted bulk payload
lives in referenced local disk extents. The same ownership can serve a real
later projection through the same Workspace library. There is no additional
runtime facade crate or provider registry.
Service C1/C2 constructs and saves canonical content; C5 records stages and
history. Neither receives the live overlay, kernel handles, native paths or
daemon memory references.

```text
  POSIX applications                       SDK controller
       |                                        |
  kernel / layerfs-fuse                  SDK edit binding
       | callbacks/replies                     |
       +--------------------+-------------------+
                            |
  +------------------ layerfs-workspace ----------------------+
  | visible head, identities, revisions, semantic handles    |
  | immutable base B + versioned local overlay               |
  | frozen G + live D1 + older pinned read/directory versions |
  | bounded resident metadata + I/O buffers                  |
  | private disk extents + versioned metadata backing        |
  | shared RAM/disk reservation and accounting owner         |
  +-------------------------+-------------------------------+
                            | existing logical operations
                            v
                    existing bridge client
                            |
  ----------------------- boundary ---------------------------
                            |
               authorized service handlers
                    /                 \
               C1 + C2                C5
        canonical file/tree saves  exact stages/history
```

The managed `/workspace-id` is an immutable lifecycle root, not a renameable
entry of this overlay. Only descendant names participate in namespace changes.
The display/mount identity, authority-supplied Workspace producer incarnation,
Branch identity, inode scope, local revision, generation, stage token and
connection request ID remain distinct. [Commit integration](03-commit-integration.md)
defines their use at the boundary.

## 2. One visible state, several immutable views

### 2.1 Terms and conceptual records

The following names describe responsibilities. They do not declare public
types, prescribe a crate API or imply an implementation already exists.

| Concept | Required information and ownership |
| --- | --- |
| Base B | Acknowledged immutable filesystem root; for a Branch-backed Workspace, the coherent expected head/base Layer, scope, canonical profile and root serial returned through the service |
| Visible head | One immutable descriptor of the current namespace/inode roots and overlay ancestry, with a checked local revision; published by the Workspace owner |
| Dirty overlay D | Final changed-name entries and immutable inode versions relative to B; indexes/counts needed for capture are maintained as mutations are accepted |
| Frozen generation G | Captured visible view, generation, exact Branch expectations, changed-item frontier and retained versions/bytes; immutable after capture |
| Live successor D1 | Final changes relative to the captured G view; an empty D1 inherits G, and later writes replace only the versions they change |
| Namespace entry | Key `(parent inode identity, name)`; value `Bind(inode identity)` or `Tombstone`; no entry means inherit |
| Inode version | Identity, version/revision, kind, logical length, portable metadata, local link/lifetime information and immutable content representation published together |
| File representation | Immutable ordered pieces referencing canonical file ranges, a captured file version, immutable local disk extents, or logical zero spans; its metadata is resident only within the declared page budget |
| Local disk extent | Checked `(backing owner/incarnation, segment identity, offset, length)` plus retained ownership; a published extent contains complete immutable bytes |
| Resident buffers/pages | Bounded write/read/serialization buffers and metadata cache pages, independently reserved and charged; they are not a whole-file payload cache |
| Semantic open handle | Inode identity and accepted open flags/access, plus owned lifetime references; it does not permanently freeze content at open time |
| Read plan | Exact immutable inode/file version, length, requested range and source spans pinned for one callback |
| Directory view | Immutable namespace/inode view plus directory identity, stable base locator, merge positions and bounded cookie state |
| File-save result | Exact `(G, inode identity, captured file version)` to acknowledged canonical root/length association |
| Submission observation | Original request/G association, exact stage/token when known, typed result/failure and cleanup/uncertainty context |

The inode identity includes its allocation scope and serial and is qualified by
the Workspace/mount context. A pathname is a lookup key, not that identity.
Canonical mode/mtime and the declared projection metadata from
[01](01-workspace-fuse-contract.md) must remain consistent with the selected inode
version. Unsupported metadata mutations remain refused until their shared
operation exists.

### 2.2 Immutable means content cannot change through another alias

The visible descriptor points to immutable namespace/inode versions and immutable
byte ranges in private disk backing. A mutex may protect publication of the
descriptor and resource reservations; it must not turn an old captured view into
an alias of mutable live records.

```text
  INVALID: copying a handle to mutable state

      G -----+
             +----> shared mutable inode/pieces ---- write changes both views
      live --+

  REQUIRED: share immutable versions, replace the live reference

      G ----------> inode version v7 ----> immutable disk extents A, B
                        ^
                        | old read still owns v7

      live --------> inode version v8 ----> shared A, new extent C, shared B
```

An `Arc<Mutex<...>>` shared between G and live state would not provide a snapshot.
Reference counting protects lifetime; it does not establish immutability.
An extent's bytes become immutable before publication. Append into a disjoint,
reserved tail may share a segment file; it may not overwrite, truncate, punch out
or recycle a range still reachable from live state, G, a read or a directory
view. If reclamation works only at segment granularity, every pinned extent keeps
that segment's unreclaimable allocation charged.

The capture descriptor references versioned metadata roots, not a mutable map
visible through two aliases. A small prototype can use owned in-memory maps,
with admitted copy-on-write metadata. A map containing every dirty file/name or
an entire large directory is not the general low-RAM target. Metadata page/range
copies happen during mutation preparation, with reservation, before the short
publication section. A full base-tree copy or whole-map clone at capture is
not an implementation option. §2.4 states the still-open metadata-index gate.

### 2.3 Precedence and tombstones

Before capture the logical view is D over B. During save it is D1 over G over B.
G may internally hold its full captured descriptor; the diagram separates G's
changes from B to make precedence visible.

```text
  lookup(parent, name)
           |
           v
       D1 entry? --- Bind(i) ----> inode i in the same selected view
           |  \
           |   `-- Tombstone ---> absent; STOP
           | none
           v
        G entry? --- Bind(i) ----> inode i in the same selected view
           |  \
           |   `-- Tombstone ---> absent; STOP
           | none
           v
      B directory lookup -------> binding or confirmed absent
```

For example, if G renamed `a` to `b` and D1 unlinks `b`, D1's tombstone hides
G's binding. Falling through a tombstone would resurrect `b`. An absent D1 key
merely inherits G. Complete inode versions use the same newest-version
precedence; fields from different versions must not be assembled into one attr
reply or save input.

The implemented delta records add three rules to this precedence:

- **One name owns a binding or a removal, never both.** Every publication that
  binds a name clears the removal record that name carried
  (`overlay/directories.rs::keep_name` rebuilds the directory's removal page
  without the name, and both `filesystem/rename.rs` and `filesystem/create.rs`
  call it before writing the binding), so a name is listed and resolvable
  instead of being hidden by its own stale tombstone. The rule is shared
  because one generation reaches it through more than one ordinary syscall
  sequence: a POSIX overwrite the kernel resolves as `UNLINK` followed by
  `CREATE` binds a name whose removal record the same generation has just
  written. `keep_name` advances its cursor by the cell it visited rather than
  by the cells it kept, so a name that is the first removal record on the page
  is still visited exactly once.
- **A metadata page key's length bound belongs to its kind.** A `D` key is 17
  bytes and an `I`, `N` or `R` key is 9, while a name kind (`E`, `T`) is one
  kind byte plus up to 255 name bytes (`backing/metadata_index.rs::key_limit`).
  A cursor bound may carry the kind byte alone; a key written into a page must
  carry its identity (`stored_key_limit`). `backing/metadata_build.rs` validates
  every page it writes with that rule, so rebuilding a name page accepts an
  ordinary long file name: a 17-byte `package-lock.json` produces an 18-byte
  removal key.
- **A rename destination binding shadows an inherited binding.** The
  publication writes the destination binding whatever the destination parent's
  origin held (`filesystem/rename.rs`), so an inherited base binding is
  shadowed rather than skipped and the replacement is resolvable from the very
  next lookup.
- **A listing merges its origin through the tombstone filter.** The origin
  still lists every name it holds, including the ones this delta removed; the
  merge drops a tombstoned name and reads the next origin page instead of
  returning a short listing, because a mounted caller reads a page that is not
  full as the end of the directory (`filesystem/namespace_view.rs::list_view`).

The **re-anchor rule** governs how an edited parent's record is resolved: from
the live root and the record's own revision, never from a cached path plus a
service fallback (`filesystem/create.rs`, `remove.rs`, `rename.rs` treat a
directory delta this way). A node's cached path is not a valid live locator
after a Commit advanced the baseline, and a path-addressed `Inspect` against a
root that no longer contains that path answers `PathNotFound` — the mutation
itself must read the delta record the live overlay owns. The same rule is
stated from the lookup side in [01 §7.1](01-workspace-fuse-contract.md#71-required-r-operations-and-local-lifecycle).

Namespace overlays are keyed by parent identity. If a base directory moves, a
read of its inherited contents retains an immutable base-root/original-path
locator or another already-supported logical locator from that selected view.
It must not ask the service about a new live pathname inside an older root.
Current shared inspection operations are path-addressed; they do not provide a
private by-inode or per-canonical-object RPC. This is why the bounded profile
refuses to move a committed directory whose namespace record is absent: its
inherited children resolve through the canonical tree by path, and no
identity-keyed service query can re-anchor them after the move
([01 §7.3](01-workspace-fuse-contract.md#73-optional-kernel-owned-or-unsupported-operations)).
[Existing read handlers][service-read]

### 2.4 Many tiny files: metadata must scale independently of payload

A disk payload spool alone does not bound RAM for `node_modules`. Names, inode
versions, dirty indexes, piece records, directory cookies and per-file save
results can dominate when each file contains few bytes. The target therefore
requires **bounded resident metadata with owned versioned backing**, or a plainly
declared missing capability; it cannot claim scale by retaining all these records
in an unbounded in-memory HashMap/BTreeMap.

```text
 short resident descriptors
   visible version root / G root / dirty-frontier root / counts
                       |
              bounded resident page window
                       |
          private versioned metadata backing
           names, inode/piece records, dirty keys,
           directory resume positions, G file-result associations
```

The metadata format, index/page layout and update/merge algorithm are **not frozen
here**. Do not scaffold a new SQLite store, provider registry or generic storage
framework to fill that gap. A concrete bounded page/index design must establish:

- Point lookup, final changed-key update and ordered dirty-key/range iteration
  without loading the whole namespace or dirty set.
- Immutable roots that G/readers can pin while live updates replace only affected
  metadata. Disk records required by a visible root are complete before that
  root is published; no fsync/restart-durability claim follows.
- Maintained frontier counts and roots so capture performs no dirty-ID clone,
  directory scan, metadata-page fault or disk read/write in its state lock.
- Bounded resident pages, descriptor/pin counts, index scratch and decode/output
  buffers; nonresident records and abandoned/versioned pages consume disk quota.
- Paged completion associations and directory resume state as well as paged
  namespace data. Moving only one growing map to disk is insufficient.

Clean immutable metadata can be evicted only when it is reconstructible from its
owned backing or an immutable Store identity and the active operation holds what
it needs. Dirty/in-flight metadata without a complete backing representation
cannot be discarded as a cache entry. Older views keep their backing records
alive, even after those pages leave RAM.

Until that index is implemented and qualified, an in-memory bounded-map prototype
must refuse at its declared capacity and must not pass the many-file target.
Likewise, current 128-record prepared updates do not become a whole-installation
Commit merely because local metadata can be paged; §10.3 records that separate
shared-operation prerequisite.

## 3. Mutation acceptance and the atomic cut

### 3.1 Mutations publish a coherent descriptor

A supported mutation prepares its final effect against one observed revision:

1. Check access, inode kinds, flags, ranges and checked arithmetic. Resolve any
   required immutable information through existing operations outside the state
   lock, retaining the exact queried view.
2. Reserve disk capacity for new payload/metadata, bounded RAM for I/O and affected
   metadata, and the scratch/result headroom required to finish accepted work.
   Check resulting dirty-record counts and relevant shared-operation limits.
   Old pinned allocation is not free merely because it will be superseded.
3. Fill/write the reserved payload ranges completely through bounded buffers;
   prepare the new immutable pieces, inode version and changed-name records.
   Write required versioned metadata backing before publication. Namespace
   operations prepare all participating parents/inodes as one proposed change.
4. Under the Workspace lock, validate the observed head identity, generation,
   inode revisions and preconditions, and
   install one new visible descriptor. Its namespace, file version, length and
   metadata become visible together. A stale preparation is a bounded refusal
   or an explicitly scheduled new caller operation; do not add an internal
   unbounded retry/reprepare loop.
5. Release the lock, then drop retired resources and send the callback reply.
   Keep pending reply/error ownership explicit. Publication is the operation's
   local linearization point; it may precede delivery of the kernel reply.

The preparation phase can do bounded local disk I/O and metadata work; the
publication phase must not perform disk/network I/O, canonical construction or
large allocation/destruction. All bytes/metadata referenced by the new visible
version must already be completely readable from owned backing. Allocation or
short/failed writes before publication leave the old view intact. Unpublished
partial ranges remain reserved/charged until exact cleanup proves release; never
expose a partially initialized range or infer that a failed write wrote nothing.
An error before publication leaves the prior namespace/version intact. An error
after publication is not permission to claim that the local change never
happened.

```text
  prepare from revision r       publish under short lock       reply
  [reserve + disk write + COW] -> [r still valid? install r+1] ---->
       no visible effect             local effect exists

  capture before install => entire mutation belongs after G
  capture after install  => entire mutation belongs inside G
```

The cut is coherent across the filesystem state that one mutation installs. It
is not an application transaction spanning several syscalls: capture may fall
between `write(file A)` and `rename(temp, file B)`. The application must supply
its own higher-level coordination if those calls must be one transaction.

### 3.2 Capture algorithm

Dirty indexes, changed identities and conservative lowering counts are maintained
while accepting mutations. Their small root descriptors are already resident.
Capture does not discover dirty files by scanning the namespace, cloning dirty
IDs, faulting metadata pages or re-reading content. A paged backing index is not
permission to put a hidden index scan inside the capture critical section.

Before acquiring the state lock, reserve the small fixed descriptors needed for
G and the empty successor. Under the lock:

1. Refuse if a previous generation/stage still owns the submission slot, if the
   required reservations are unavailable, or if the generation counter would
   overflow its representable catalog range.
2. Retain/move the current immutable visible descriptor and dirty-index roots into
   G together with its exact baseline and current generation. A pending mutation
   cannot publish between the namespace and inode capture because they share
   the same descriptor/publication lock.
3. Install an empty D1 whose immutable parent view is G. The next accepted write
   is assigned to the successor generation. Mark the single submission slot as
   owned by G and advance the visible-head revision. A mutation prepared against
   the pre-capture head cannot publish its old-coordinate proposal into D1 without
   a new explicit preparation; its old head/generation guard must fail.
4. Release the lock. Paged dirty iteration, lowering, file saves and history
   requests begin afterwards. Move retired references out of the critical section
   before their final drop can recursively destroy a large retained graph.

```text
  BEFORE                        CAPTURE                     AFTER

  visible head H0         [one descriptor rotation]     visible head H1
       |                         under lock                  |
       v                                                     v
    D over B  ------------------------------->          empty D1
                                                            |
                                     G owns H0 <-------------+
                                         |
                                      D over B

  no full namespace traversal at this point
  no payload copy / hash / C1 construction / service I/O at this point
```

This is a structural requirement for a short capture critical section, not a
latency claim or a claim of lock-free execution. A capture implementation that
calls `clone` on every inode/map/payload, normalizes all files, or runs recursive
destruction while holding the lock does not satisfy the proposal.

All local operations published before the cut are in G, including coherent
length/metadata and hard-link/rename effects. Operations merely buffered by the
application or not yet accepted by the daemon are outside the cut. The kernel
implications are explicit in §10.

### 3.3 Local Commit serialization

The implementation must document one lock discipline before adding concurrent
control handlers. Registry lookup returns a retained handle and releases the
WorkspaceHost lock before entering a Workspace. Obtain budget/count/I/O admission
as owned reservations outside the Workspace state lock; reservation ownership
does not mean holding an accounting mutex through I/O. Complete backing access
and encoding outside the state lock, then briefly validate/publish. Move drops,
refunds and callbacks out of that lock before they can acquire another owner.
If an unavoidable nested lock remains, record its single order and test competing
control/data transitions; no path may reverse it. Kernel/service callbacks may
not re-enter locked mutable state. This is a required implementation discipline,
not proof that an unimplemented concurrent path is deadlock-free.

Every Workspace has a local **submission slot**, protected by its short state
mutex. This is proposed runtime behavior, not a lock already implemented by
Pair 3. One state field is sufficient; a second lock service, lock file or
distributed lock manager is unnecessary.

| Mechanism | Scope and lifetime | Purpose |
| --- | --- | --- |
| State mutex | Per Workspace; only coherent state publication, capture and reconciliation | Protect namespace/inode/version transitions without holding a lock during service I/O |
| Submission slot | Per Workspace; capture through exact generation disposition | Serialize Commit and explicit staging for this producer incarnation |
| Frozen-generation admission | Proposed one retained submission across the consumer's Workspaces | Preserve the current retention ceiling; per-Workspace slots do not increase it |
| Active remote-operation admission | Separate consumer/client admission while an operation runs | Bound transport work; an idle staged snapshot does not keep a client mutex forever |
| C5 expected-head/base checks | Service/catalog operation against the Branch | Detect another Workspace or process changing the same Branch; a local slot cannot protect other processes |

```text
 Workspace A state mutex                  Workspace A submission slot
 -----------------------                 ---------------------------
 short: capture G + reserve  ------------> Idle -> Submitting(G)
 release mutex                                  |
                                                +--> Staged(G, exact token)
 short: write D1; release                        |      slot remains owned
 short: read view; release                       +--> Failed/Uncertain(G)
                                                |      slot remains owned
 second commit(A) -----------------------------> Busy; no queue or new capture
                                                |
 known own Commit result ----------------------> reconcile only G -> Idle

 supported local D1 work never waits for the entire G save under the state mutex
```

Reserve admission before publishing a capture; failure to obtain it leaves the
visible state unchanged. Once G is captured, a request returning, a dropped
mutex guard or a lost connection cannot release its logical slot. Stage success
retains G; stale/failed/uncertain outcomes retain the necessary G/D1 context until
an explicit valid disposition. Do not make a second Commit an implicit retry of
the first. The slot is released only after known own completion is reconciled,
or a separately specified disposition has safely accounted for G, D1, readers
and any exact remote stage. DiscardStage alone does not perform that disposition.

The earlier one-frozen-submission proposal is **per consumer**. Consequently,
Workspace A retaining a staged or uncertain G can prevent Workspace B in that
consumer from capturing another submission. B's ordinary supported local work
can continue within its shared budget. This is an explicit initial isolation
limit, not a property of per-Workspace serialization. Allowing independently
retained frozen submissions for several Workspaces requires a separate admission
decision; this clarification does not make that change.

An idle retained G does not own an active network permit. Existing remote
admission, the service's two-operation capacity, C2's per-Store save limits and
the single construction worker remain separate constraints. One mount per
Workspace isolates mount/session and local state; it does not promise that a
shared consumer, service or Store cannot limit its neighbours.

### 3.4 Multiple operations continue during Commit

A Workspace is a continuing filesystem session: several applications/threads
can use its supported operations and make repeated explicit Commits without
recreating or unmounting it. Commit serialization applies to submission of a
captured generation, not to the complete lifetime of every filesystem operation.

Capture orders mutations by their coherent publication point. A mutation
published before the cut belongs to G; one published afterwards belongs to D1.
An operation's syscall start time alone does not decide inclusion, and a callback
may already have published before its reply reaches the caller. A preparation
that loses its revision guard must follow the explicit conflict policy, not
install a stale version. Reads retain the consistent view selected for that read.

```text
                     brief capture under state lock
                                  |
                    +-------------+--------------+
                    |                            |
                frozen G                    live D1 / G+1
                    |                            |
           construct / transfer              read / write
           save / history outcome            rename / unlink
                    |                            |
                    +------ known result --------+
                                  |
                      reconcile G, preserve D1
```

No Workspace state mutex spans G's hashing, construction, transfer, storage save
or history request. Supported local operations can make progress while that
work remains incomplete. Existing in-flight reads may finish on their pinned
versions; later operations observe the current live view. Snapshot capture is
not a drain of all application work or an application transaction across syscalls.

This is not lock-free execution or unconditional acceptance. Short state-lock
contention, exhausted allocation/saveability limits, and a necessary remote
read facing admission/deadline failure remain explicit outcomes. The runtime
must not block all event loops behind a busy client or retain an unbounded
operation queue. Verification requires a D1 write and live read to finish before
G's actual service save finishes; merely withholding an already-computed terminal
response is insufficient. See S-11/S-16 in [04](04-implementation-and-verification.md).

## 4. File versions and G-relative successor coordinates

### 4.1 Piece vocabulary

Each file version owns an ordered, non-overlapping logical sequence covering
exactly `[0, length)`. A piece describes one of:

| Piece source | Meaning |
| --- | --- |
| Canonical range | Range of one immutable canonical file identity and its known length |
| Captured-version range | Range of the exact immutable file version visible in G, with that version's length and source graph pinned |
| Local disk extent | Immutable initialized range of a daemon-owned segment file, with checked owner/segment/offset/length and retained backing lifetime |
| Zero span | Logical zero bytes introduced by extension/gaps; no materialized zero-filled spool |

Admitted callback/read/transfer buffers are temporary operation resources, not
another published file-piece source. New accepted payload must have its complete
owned disk extent before the visible file version references it.

Ranges are half-open and use checked integer arithmetic. References always point
to older immutable versions, never to a live head or to themselves. Adjacent
pieces may share backing, but saving uses an explicitly defined lowering policy;
canonical edit boundaries cannot be changed by the bridge.

### 4.1.1 Disk extent installation and retirement

```text
 write callback bytes
         |
 reserve disk extent + metadata + bounded input/scratch headroom
         |
 positioned write of complete reserved range
         |
 write complete versioned metadata required by the candidate
         |
 revalidate prepared version under short state lock
         |
 publish piece -> (owner, segment, offset, length) -> acknowledge

 before publication failure -> old version stays visible
                           -> partial/dead range charged until released
 after publication failure  -> preserve actual visible state/outcome
                           -> never announce guessed rollback
```

Disk is the selected backing, not a fallback triggered by a RAM overflow.
Local I/O may read directly from the callback slice while that borrow remains
valid. If work retains bytes asynchronously beyond that lifetime, use an admitted
owned buffer or an explicitly supported ownership transfer; never retain the
borrow itself. Complete the disk write before applying the referencing piece.
This does not require an extra owned copy when no retention occurs, and makes
no zero-copy promise. The reference already uses base/replacement pieces and write-before-apply;
these are preserved semantics, not new optimizations. [Reference preparation][v016-edits]
[Reference write-before-apply][v016-write]

Published ranges are immutable; future writes reserve new extents. Appending a
disjoint tail must not change earlier bytes or reuse an exposed segment identity.
Bounds and ownership are checked on every local read. A short read from a range
the runtime acknowledged as complete is an explicit I/O/integrity failure, not
EOF or permission to return zero-filled replacement bytes.

Segment size, rollover rule, disk quota and reclamation granularity need an
explicit profile before implementation. No value is selected by this paragraph.
The reference's 1 MiB segments demonstrate one way to bound dead slack; they are
not an automatically adopted target. Segment/page registries and open file handles
are also bounded metadata, not an uncharged entry per byte-range operation.

### 4.2 A successor file starts from its G version

On the first D1 mutation of an inode, select its complete file version in G as
the immutable logical source. The successor's offsets refer to those bytes,
including any overwrite or size change G made relative to B.

```text
  B file C0                  G file vG                  D1 file v1
  original bytes -----> G's final piece view -----> ranges of vG + new bytes
                             |                              |
                             +-- exact file save --> C_G ---+
                                  same vG bytes        usable source identity
```

When G's exact file save is acknowledged, record `vG -> (C_G, length_G)` in a
bounded result association. A new read/save plan may resolve that exact version
through C_G. Existing plans continue to own their original source references.
This association cannot change vG's bytes and is not a mutable alias to v1.
It is based on successful completion for that specific captured input, not on
root existence or the last file root returned by any operation.

Do not replace B's root in a B-relative edit list with C_G. That would reinterpret
coordinates and lose inherited changes. D1 must either already be expressed
relative to vG, as proposed here, or undergo a real validated normalization into
that coordinate system before using C_G. A metadata/root-pointer substitution
alone is not normalization.

The source association also prevents an indefinitely growing active chain of
unsaved file recipes: after confirmed own completion, the next save lowers
successor pieces against the canonical identity of their actual G base.
Older readers can still retain old recipe nodes; their memory remains charged.
No new snapshot requires recursively walking all file recipes at capture.

### 4.3 Worked overwrite example

```text
  B bytes:                  abcdefghij                   length 10
  G: overwrite [2,5) XYZ -> abXYZfghij                   length 10

  vG pieces:       C0[0,2) | DiskExtent(eXYZ,0..3) | C0[5,10)

  capture ----------------------------------------------------------

  D1: overwrite [3,4) q  -> abXqZfghij                   length 10
  v1 pieces:       vG[0,3) | DiskExtent(eq,0..1)   | vG[4,10)

  G save C_G = "abXYZfghij"
  next successor save: EditFile(C_G, overwrite [3,4) with "q")
```

Forwarding the chronological G and D1 writes as one C1 edit stream would make
the second write reach into an earlier replacement. C1 explicitly rejects that
shape. Saving G and later lowering D1 against the exact C_G base has the required
meaning. The local overlay may represent arbitrary successive overwrites; the
shared edit operation consumes a supported final stream. [C1 edit input][edits]

Within a single generation, normalize its final piece state before submission.
For base `abcdef`, writes `write(1,"XYZ")`, then `write(2,"q")` yield `aXqZef`.
The proposed deterministic final-state lowering emits one maximal changed span
`[1,4) -> "XqZ"`; it does not replay the second write into the first replacement.
This is an explicit Workspace input policy. It promises the canonical result of
that declared lowered stream, not equality to an invalid chronological stream
or a differently segmented edit policy. The transport must preserve it exactly.

### 4.4 Truncate, extend and zero example

```text
  B bytes:                    abcdefghij                 length 10
  G: truncate(4)           -> abcd                       length 4
  capture: vG = "abcd"

  D1: extend(7)            -> abcd 00 00 00              length 7
      write at 6, "Z"     -> abcd 00 00 Z               length 7

  v1 pieces:             vG[0,4) | Zero(2) | DiskExtent(eZ,0..1)

  save G:                C_G = "abcd"
  lower v1 from C_G:     insertion [4,4) -> bytes 00 00 Z
```

The successor must not expose B's old `efg` after extending the truncated file.
Truncation removed that tail from the logical version; extension creates zeros.
`seek` alone changes no file size. A write beyond current EOF adds an explicit
zero gap and new bytes in the same inode-version publication.

Zero spans reduce local storage, not shared-operation work. If a lowered
replacement contains zero bytes, their complete logical length counts against
the 8 MiB replacement-input ceiling and they must be delivered when the service
operation runs. A multi-gigabyte hole cannot evade the input limit by occupying
one local span record. Refuse an unsupported final submission before acknowledging
the mutation; do not silently flatten or split it into extra saves.

`O_TRUNC` is a mutation at successful writable open even if no write follows.
`O_APPEND` selects current live EOF and publishes its write as one protected
operation; an EOF sampled before another append cannot be reused blindly.
The [callback contract](01-workspace-fuse-contract.md) owns flag/permission/error
mapping. Local metadata and length change together with the pieces.

### 4.5 General final-state lowering

Use one deterministic walk over the captured final pieces, outside the capture
lock. The supported byte-edit subset preserves the relative order of inherited
base bytes; rename and hard links change names/identity, not byte order.

1. Identify maximal unchanged ranges of the exact file base version.
2. Between them, emit the final replacement/zero byte stream once in logical
   order, eliminating local bytes overwritten before this capture.
3. Express each edit in C1's current-result coordinates. For a removed base
   interval `[a,b)`, apply the checked accumulated prior length change `delta`
   to obtain `[a+delta,b+delta)`; update delta by inserted length minus removed
   length. The next edit cannot reach into bytes this stream already introduced.
4. Validate final length, monotone edit positions, exact replacement lengths,
   record/metadata limits and source lifetimes before service submission.

For example, deleting base `[2,4)` changes a later base `[6,8)` edit into current
coordinates `[4,6)`. A root substitution does not perform this arithmetic.
Maintain conservative prospective counts during local mutation so a known
256-edit/8 MiB-input or encoded-metadata refusal is not first discovered after
the user was promised a saveable mutation. Full final validation still runs
before submission. Do not split one captured file into a hidden multi-save
sequence or fall back from EditFile to whole-file reconstruction after refusal.
New-file construction remains conditional on the missing live-new-inode contract.

### 4.6 Incremental work must follow the changed frontier

Maintain separate content, metadata and binding-change facts as operations are
accepted. After capture, iterate only G's required records in bounded pages:

| Frozen change | Required work; work not implied |
| --- | --- |
| File content changed | Stream a supported final edit against its exact base, or the explicitly selected complete-file construction for a new file; no daemon copy of all inherited bytes |
| File metadata only | Construct/attach the selected metadata result; reuse its known content root, not a gratuitous file reconstruction |
| Rename/hard-link/binding only | Submit the affected final bindings and required metadata; do not reconstruct file bytes because a path changed |
| Unchanged file/directory | Preserve inherited identities; do not enumerate/reconstruct it merely because another item is dirty |
| No recorded local changes | Apply the explicit lifecycle policy without a whole-tree comparison. Local clean status is not a successful history Commit receipt or proof of the current remote head; [UpToDate still follows its exact context checks](03-commit-integration.md#72-uptodate-is-deliberately-narrow) |

This is a caller work-selection rule, not a guarantee that C1/C2 perform work
proportional only to changed bytes. C1 can read immutable mappings, authenticate
predecessors and reconstruct encoded objects; filesystem validation can have
its own bounded traversal. Charge and observe that actual service work. Do not
move it into setup or call it free because the runtime retained a root.

Do not cancel a content-dirty flag by guessing that writes restored the original
bytes. An exact local proof may eliminate a no-op; otherwise use the supported
C1 operation and its own outcome. Future optimization must preserve the declared
final edit policy and returned root, not switch construction semantics after a
refusal. The complete logical replacement stream is generated through bounded
disk read/zero/transport buffers, never a single vector holding all G payload.

### 4.7 Fragmentation and repeated generations

A 1-byte overwrite repeated many times can exhaust piece/index/disk resources
while the file's visible length remains constant. Reserve and bound the resulting
piece records, metadata page copies, disk reservations and abandoned tails before
each accepted operation. Coalesce adjacent compatible references when their
source identity/ranges permit it; this must not merge away distinct semantics or
silently rewrite pinned ranges.

```text
 same file, same logical length
   overwrite #1 -> extent E1   G/read may pin E1
   overwrite #2 -> extent E2   E1 becomes dead only for the live view
   overwrite #3 -> extent E3   E1/E2 may still consume disk allocation

 live byte count != piece count != reserved/allocated backing bytes
```

The chosen profile must set fragmentation/piece and metadata-work bounds and
state refusal behavior. If compaction is selected later, it reserves old and new
disk allocation, bounded copy buffers and metadata together, writes a replacement
fully, then atomically publishes new references. Old references remain valid
until their users release them. No unbounded background compactor, second
construction worker, hidden whole-file flattening or compaction under the capture
lock is introduced here. A simple first implementation may refuse its declared
fragmentation bound, but that refusal must be measured against the required
workload rather than described as full package-install support.

After known own Commit, resolve exact captured file versions to their saved
canonical roots so active successor recipes do not gain another unresolved
ancestor at every Commit. Do the corresponding bounded metadata-root/overlay
reconciliation so namespace reads do not walk the whole history of past dirty
maps. Old readers may retain older roots, but those retained graphs stay acyclic,
charged and independently reclaimable. Repeated Commit must not mean retaining
every old recipe/result association forever or rescanning every historic dirty
set to answer a current read. The exact bounded index compaction/reconciliation
algorithm is an implementation prerequisite, not a format frozen by this note.

## 5. Namespace, hard links and capture

### 5.1 Rename is one local namespace publication

Resolve source/destination and validate permissions, types, destination emptiness,
supported flags and topology against one view. Prepare source removal,
destination final binding, parent metadata and affected inode state together.
Publish them through one visible-head change.

```text
  before:  dirA/old -> inode 41       dirB/new -> inode 52

  rename preparation:
      (dirA,"old") = Tombstone
      (dirB,"new") = Bind(41)
      final parent/inode metadata and lifetime changes

  one publication --------------------------------------

  after:   dirA/old absent           dirB/new -> inode 41
           open handle to 52 still retains inode 52
```

Capture sees either complete side, never a source removal without the destination
binding or a destination pointing at mixed old/new inode metadata. A failed
precondition leaves both prior names intact. Replacement of a name does not
retarget an already-open handle to its old inode.

The final shared directory input contains sorted final bindings for changed
names; unchanged names remain inherited. It is not a copy of every child in both
directories. [C1 filesystem input][filesystem-input]

Directory/symlink rebinding still needs bounded cycle/alias validation before
local visibility. The current 4,096-work ceiling can be exceeded by traversal of
the base namespace even for a small moved subtree. The missing public typed
prevalidation/result path cannot be replaced by parsing an error string or
discovering refusal only at later Commit. Keep such callbacks disabled until
their required shared operation and error semantics are available. Immutable
mount roots do not remove this limitation for descendants.

### 5.2 Rename during G's save

```text
  G captured:      a -> inode 7, version vG
  D1 rename:       a = Tombstone, b = Bind(7)
  D1 write via b:  inode 7 -> version v1 based on vG

  before Commit:   D1 over G over B       visible b -> v1
  own G Commit:    R1 contains a -> vG
  after Commit:    D1 over R1             visible b -> v1
```

Completion cannot clear all dirty names or reinsert `a`. D1's tombstone and
binding remain the newest namespace facts. The known root R1 represents G only;
it does not represent D1.

### 5.3 Hard links and unlinked open files

```text
  names:  a ----+
                +---- inode 7 ---- current version v1
          b ----+
                        ^
                  open handle h

  unlink(a): b and h remain
  unlink(b): h remains; neither name is visible
  write(h): updates inode 7 for its open lifetime, creates no pathname
```

Regular-file hard links share one inode state. A write through either name is
visible through the other; they are not independent file copies. Namespace
reference counts and metadata changes publish with the corresponding binding
transaction. C1 derives saved reference counts from final bindings; the daemon
must not claim it can override them. Directory/symlink hard links are outside the
current supported rules.

An unlinked open file remains readable/writable for its accepted handle lifetime,
subject to budget. Its last name must not be resurrected by save completion.
A generation may retain an earlier named version while the live unlinked inode
has newer data. Local unlinked data with no retained namespace binding is not
made reachable by inventing a hidden saved name. Handle/reference release and
saved namespace retention are separate decisions.

## 6. Read plans, handles and late service results

### 6.1 Current reads pin a version for one callback

An ordinary writable handle pins inode identity, not a permanently frozen
open-time file. At each read's start, retain the current immutable view/version
descriptor under the Workspace lock. Load any nonresident inode/piece metadata
outside that lock from the pinned version, clamp to its logical length, reserve
bounded reply/source metadata, and retain the read plan. Local disk reads, byte
copying and remote acquisition happen outside the state lock. No missing metadata
page may trigger blocking disk I/O inside the short capture/publication lock.

```text
  time      read A                     write B                live
   t0       pin inode7/v8, len8                                v8
   t1       acquire first span          publish v9             v9
   t2       acquire second span of v8                          v9
   t3       return one coherent v8 reply                       v9
   t4       next read on same handle pins v9                   v9
```

A read begun before a concurrent write may return its older coherent version.
It must not splice a prefix from v8 with a suffix or EOF decision from v9.
Returning an old coherent result must also not overwrite newer data in the
kernel's cache. Qualify the selected kernel's reply/write/invalidation ordering;
the immutable read plan alone is not that cache-coherence proof.
At EOF return the correct empty/short logical result; current C1 `read_range`
rejects an end beyond the file's logical length, so clamping belongs to Workspace.
Hold a bounded response until successful terminal read completion before exposing
it as a successful kernel reply. Partial bridge data followed by failure is not
a successful file read. [C1 range read][file-read]

The proposed maximum FUSE reply reservation is 128 KiB per admitted callback,
inside the Workspace allocation cap. Several disjoint immutable-source spans
can require several logical reads; they use one complete callback budget and
one version, not one new timeout per span. Canonical traversal/reconstruction is
service-local. A requested range does not guarantee equally small physical I/O.

### 6.2 Late lookup or attribute result

Each query retains `(view revision, parent/inode identity, base locator, purpose)`.
The result belongs to that query view, not automatically to the current head.

```text
  t0  lookup("a") starts against view V: a -> inode7
  t1  live rename/unlink publishes V+1: a = Tombstone
  t2  Stat result for V arrives: inode7

  allowed: finish an explicitly pinned V read/iteration with V's result
  forbidden: insert a -> inode7 into live V+1
```

For a current namespace lookup whose result would populate the kernel dentry or
attribute cache, revalidate the relevant view/name revision before reply/cache
installation. If superseded, give the selected bounded refusal/re-resolution
disposition; never silently install stale names or enter an unbounded retry loop.
For an already-pinned read or directory view, the old result remains usable
within that view and retains its resources until reply completion.

Kernel cache publication needs ordering too: either the old reply precedes the
corresponding invalidation, or it is refused before a new mutation can make that
reply stale. Checking a revision and then allowing a competing publication
before an unordered kernel reply is insufficient. The adapter and Workspace
must implement/verify that reply-versus-invalidation ordering without holding a
global state lock over service I/O. The initial adapter's actual API limitations
are a capability gate, not a reason to assume the race cannot happen.

## 7. Directory views and bounded merge cookies

`opendir` retains a coherent view and directory identity. The proposed writable
iteration semantics are a stable view for that directory handle: later mutations
can affect fresh lookup/open calls without retargeting this iterator. A new
directory handle sees the newer view. This does not promise a transaction across
an application's separate directory and file calls.

Merge the selected view's ordered base pages, G changes and D1 changes by name.
For each equal name choose D1, then G, then base; tombstones emit nothing.
Advance all lower-precedence entries for the selected name. Supply attributes
and types from the same directory/inode view, not the latest live inode map.

```text
  B page:  a  b  c  f     G: b=remove, d=7    D1: c=9, d=remove, e=8
             \             |                  /
              +------------+-----------------+
                           |
                   ordered precedence merge
                           |
                    a, c->9, e->8, f
```

Acquire only the next bounded base page needed to fill a reply. Do not enumerate
the whole directory merely to return its first page. Current `Inspect::List`
returns name/serial without type, so missing entry types need the existing
metadata inspection in the same view. Richer `readdirplus` remains a separate
shared-result/callback capability.

Each exposed nonzero directory cookie identifies this handle/view and an exact
resume position. It must not be an array index into a changing live directory.
The implementation may retain a bounded mapping to last delivered name, pending
base-page position and overlay merge positions; it must reserve that mapping
before exposing cookies. Reusing a cookie cannot skip or duplicate entries in
the selected immutable view. Invalid/cross-handle cookies fail explicitly.

Cookie records, retained pages, pending entry attributes and pinned views count
against the consumer cap. They cannot be evicted while the handle still requires
their semantics. If another page/cookie cannot be admitted, report capacity;
do not return a false end-of-directory. Release the owned cursor graph on
`releasedir` once in-flight replies no longer reference it. Dot entries and the
immutable managed root's parent behavior follow [01](01-workspace-fuse-contract.md).

## 8. Reference ownership and more than two physical versions

One frozen generation plus one successor bounds active submission topology.
It does not imply only two inode, namespace or payload versions exist. Reads
and directory handles may retain older views across several successful Commits.

```text
  t0  read A pins v0; it remains in flight
  t1  capture G1 containing v1; live moves to v2
  t2  Commit G1 succeeds; active base becomes R1; v0 still owned by A
  t3  capture G2 containing v2; live moves to v3
  t4  Commit G2 succeeds; active base becomes R2; v0 still owned by A

  roots now owning resources:
      read A ------> v0 ------> old base/segments
      directory D -> old namespace/inode view
      live --------> v3 ------> R2 / current segments

  both earlier submissions finished; old read resources are still charged
```

The ownership graph is acyclic and references only immutable earlier versions.
Disk backing and RAM have separate accounts. A shared disk allocation is charged
once while any owner keeps it unreclaimable; new/reserved/temporary-copy disk
allocation is charged separately. Resident reference records, buffers, pages and
result associations consume RAM even when payload storage is shared. A tiny live
slice can pin a segment's otherwise dead disk allocation; selected slice length
is not the backing charge. §10 defines the required accounting categories.

| Owner | Release condition |
| --- | --- |
| Live head | New head replaces it and all readers/preparations holding the old head finish |
| Frozen G | Exact terminal lifecycle result permits reconciliation and no successor/read/source still needs its graph |
| D1 pieces | Version superseded and all readers/snapshots of that version finish |
| Read plan/reply | Read fails or kernel reply completes, including any queued source/result references |
| Directory handle/cookies | Released and all in-flight directory replies finish |
| Open-unlinked inode | Last semantic handle, lookup/reference obligation, snapshot and read release it |
| File-save association | No retained version or pending reconciliation needs the exact root/length mapping |
| Unknown submission/status | Explicitly resolved lifecycle ownership, never merely loss of a socket or moving the record into another container |

Resource release happens after the final owner, not at `save.finish` alone.
Logical unlink does not free payload still pinned by an open handle, snapshot or
read. Even OS unlink may leave physical blocks allocated while a file descriptor
is open; no disk refund is allowed merely because a pathname disappeared.
Failed delete/close/reclamation remains an explicit charged cleanup state.
Move disk cleanup and substantial destruction outside the short state lock.
Do not add a helper construction worker or an unbounded deferred-free queue;
resources awaiting actual release remain charged. [Reference extent lifetime][v016-backing]

## 9. Completion, successor preservation and C5 stages

Only the local transitions needed by this algorithm are specified here; see
[03](03-commit-integration.md) for the complete service lifecycle and typed errors.

```text
  local capture G        file roots saved       StageChanges       CommitStaged
        |                       |                    |                  |
  immutable input        exact per-vG roots    candidate R1 + T     Branch -> R1
  not a C5 stage         not a C5 stage        Branch unchanged     exact T consumed

  Composite Commit uses the same stage and commit bodies in one request.
```

Lower only G. Save its changed files sequentially through existing profile-1
operations, then send `Commit(PreparedChanges)` or explicit
`StageChanges`/`CommitStaged`. StageChanges constructs and saves the filesystem
tree itself; do not prebuild it through another `UpdatePreparedFilesystem` call.
Only one Workspace save is in flight and canonical construction remains one
producer. Independent service admission capacity does not create another local
construction lane. [Service staging body][service-stage]

### 9.1 Stage success is not a clean Workspace

Stage success records a saved candidate and exact token. It does not advance the
Branch, remove G from the overlay chain, or permit G+1 to use that candidate as
its acknowledged publication baseline. Retain G, D1 and the captured expectations.
File-version resolution to an acknowledged C_G means those same bytes have a
canonical identity; it does not imply Branch publication.

```text
  before stage:     visible = D1 over G over B
  stage succeeds:   visible = D1 over G over B    stage T names candidate R1
                     ^ unchanged                 Branch still names B
```

No second stage submission is admitted for this producer while the earlier
generation/stage remains unresolved. C5 checks exact tokens for commit/discard,
including a different observed actual stage; StageChanges does not replace a stage.
This is not permission for a local queue to overwrite a stage the user still
intends to commit.

### 9.2 Known own Commit result

On exact `Committed` or `UpToDate` success associated with G, validate the result
against the retained request/stage context. Install the returned own acknowledged
root/head as the next Branch baseline. Preserve the captured base Layer unless
the defined result says otherwise; `AddLayer` is a different operation.

The visible equivalence to establish is:

```text
  BEFORE:                    AFTER KNOWN OWN SUCCESS:

       D1                              D1
       | overrides                     | overrides
       G                               R1
       | overrides                     = exact filesystem G over B
       B

  names, inode identity, current file bytes/length/metadata: unchanged
```

D1 tombstones and changed inode versions remain. An inode unchanged since capture
can adopt its known canonical root directly. A changed successor keeps its
G-relative representation and exact vG source association; it is not given an
arbitrary root in place of old-coordinate pieces. Older reads/dir views keep
their own descriptors. Removing G from the active precedence chain does not
free every G allocation.

Prepare any required versioned metadata/reconciliation writes outside the short
state lock, then publish a coherent new baseline/overlay descriptor. Failure to
allocate or install this local result does not undo a known successful remote
Commit: retain that known outcome plus G/D1 and report incomplete local
reconciliation. Do not re-submit the remote Commit or clean the old graph on a
guess. Further submission remains blocked until exact local disposition is safe.
The completion path must not clear a global dirty map, retarget a handle by
pathname, refresh to someone else's latest Branch root, or drop a newer inode
version because its
serial appeared in G's file-save results. Reconciliation of known own changes
is not an automatic rebase against another writer.

### 9.3 Failure timelines

```text
  A. Known file-save refusal
     capture G -> save file1 succeeds -> file2 fails definitely
        live D1 continues within budget
        no invented stage; preserve G/D1 and acknowledged file1 association
        no guessed deletion of file1 objects; no automatic retry

  B. Branch moves during G's tree save
     capture (head K0, base B) -> tree saved -> stage T retained
     another writer moved head -> CommitStaged(T) reports HeadMoved
        preserve G/D1/T and original expected context
        changing expected_head is not content rebase

  C. Stage known, Commit outcome not established
     StageChanges -> known T -> Commit attempt loses terminal outcome
        retain T observation plus G/D1 and request association
        neither missing stage nor equal root proves the original outcome

  D. Read older than a known successful Commit
     read pins vG -> Commit G succeeds -> D1 publishes newer v1
     read completes from vG; next read sees current v1
        no version splice and no early release of old source bytes
```

Preserve original failure code, unknown flag, cleanup result, conflict context
and exact stage observation independently. `Unobserved` is not `Absent`;
`AcknowledgedUnknown` need not have an `Unknown` outer code; a retained stage in
a stale-token failure may belong to a different generation. Do not automatically
adopt or discard it. [History wire observations][history-wire]

Stale/unknown results stop another submission, not erase live edits. Existing
local reads and supported local writes may continue only while retained G/D1
state and all admission limits fit; capacity then refuses before visibility.
An explicit later disposition must account for both generations. C5 stage discard
deletes metadata only; it is not local-edit discard, content deletion, serial
refund or permission to forget open readers. There is no automatic replay,
refresh/rebase, GC, interrupted-transfer resumption or restart recovery.

## 10. Memory, backpressure and the kernel boundary

### 10.1 Separate RAM, disk capacity and kernel residency

Disk-backed payload and a low-RAM target need three accounts. A bounded process
heap does not establish a bounded cgroup when buffered local writes populate the
page cache. Disk quota does not establish residency, and logical file length
establishes neither.

| Scope | Current direction / outstanding decision |
| --- | --- |
| Workspace-owned RAM | **8 MiB allocated capacity total per consumer** remains an unqualified target, shared by its Workspaces; not an owner-approved measured result |
| RAM contents | Resident metadata/index pages, descriptors/pins, bounded input/read/serialization buffers, copy-on-write preparation and completion/status scratch; bulk accepted payload is disk-backed |
| Frozen retention | One unresolved frozen submission per consumer, plus the owning Workspace's live successor; per-Workspace correctness slots remain separate; older reader/view versions stay charged |
| Per-read reply reservation | Candidate at most **128 KiB**, inside the same RAM target; request/reply and multi-span delivery must enforce it |
| Immutable payload prefetch/cache | No new userspace payload cache or enlarged kernel policy selected |
| Disk backing | Explicit private execution-side backing, with ownership distinct from the mount path and service Store; not automatic fallback or tmpfs |
| Disk quota | Proposed startup setting recommends one aggregate consumer allowance; **unselected:** numeric value, per-Workspace subdivision, reservation granularity and full/cleanup-failure behavior before implementation qualification. See [configuration proposal](01-workspace-fuse-contract.md#323-proposed-pair-1-startup-variables-and-attach-inputs) |
| Segment/extent layout | **Unselected:** rollover size, extent/piece count, registry/FD bounds and maximum dead slack; no inherited numeric default is silently adopted |
| Resident metadata policy | **Unselected:** index format/update/iteration algorithm and resident-page/scratch budget within the RAM target; a full namespace map is not the target |
| Local disk residency | **Unselected:** supported buffering/cache-window/backpressure policy and observed kernel/cgroup limits; memory hints alone are insufficient |
| Ordinary callback deadline | Candidate 10-second complete deadline; child calls consume remaining time, never a reset budget |

```text
 RAM target (unqualified):
   resident index/piece pages + small visible/G/dirty-root descriptors
   + handles/read pins/cookies + bounded I/O and serialization buffers
   + preparation/lowering/result headroom + retained failure/status context

 disk charge, without double-counting overlapping ownership categories:
   allocated backing blocks not proven released
     [live + G + old readers + zero-link inodes + dead/failed ranges
      + metadata/index versions + temporary replacement/compaction output]
   + outstanding reserved capacity not already covered by those allocations

 separately observed kernel/process/service scopes:
   local disk page cache + FUSE/socket/kernel objects + bridge buffers/stacks
   + C1/C2/C5 working memory + allocator/process overhead
```

Count each shared allocation once, but keep it charged while any owner makes it
unreclaimable. A dead subrange inside a live segment is not freed capacity. Partial
failed writes, uninstalled metadata pages, unused reserved tails and unlinked
open files remain in the account until their reservation or physical ownership
is actually released. A failed cleanup never produces a successful quota refund.
The selected filesystem/backing design must define how physical allocation is
observed, including preallocation/sparse extents and the difference between
logical file length, promised reservation and allocated blocks; do not add the
same block twice merely because both G and a reader reference it.

Before exposing a write, reserve its worst-case disk growth and admitted RAM
buffers/metadata/scratch. If physical write/metadata installation fails, keep the
old visible descriptor; cleanup only the exactly owned unpublished resources.
If cleanup cannot finish, retain bounded failure/ownership records and their
charges. System-wide disk exhaustion can occur despite local quota reservation;
it is an explicit I/O/capacity failure, not proof that no partial bytes exist.
Do not assume ordinary safe Rust allocation makes every OOM recoverable: audit
fallible reservations/conversions on Rust 1.85.1 and do not claim graceful recovery
from every allocator/process/OS failure.

No disk-spool or metadata quota value is invented here. Before reporting large
file or package-install acceptance, the profile must state and verify its quota,
maximum dead allocation, resident metadata and kernel memory behavior. A prototype
that merely writes payload to disk while retaining all names/pages in memory, or
allows file-size-proportional page-cache growth, fails that target.

The exact reference used 1 MiB payload segments and a four-sealed-segment hint
window. Those are reference facts only. `POSIX_FADV_DONTNEED` can be refused or
ineffective, and dirty pages/kernel state can remain resident. Reusing that idea
requires actual declared residency/backpressure evidence; it is neither a hard
4 MiB cap nor a cold-cache proof. No fsync/WAL or third-party patch is added to
turn a hint into a guarantee. [Reference local spool][v016-spool]

### 10.2 Bounded RAM without whole-file copy-up

The current backing changes the replacement source, not the logical file model.
The reference already avoids ordinary whole-file copy-up. Preserve that property
for one large file and for many small files; do not flatten either a file or a
captured generation into a process-resident byte image.

```text
 existing C0, length 1 GiB; overwrite 4 KiB at p

 service Store: C0 ---------------------------------------------- remains there

 bounded input buffer -> complete local disk write -> extent E

 file pieces:   Base(C0,0..p) | DiskExtent(E,0..4096) | Base(C0,p+4096..L)
                 inherited       owned disk bytes      inherited

 capture G:     pin immutable metadata/frontier roots; no extent copy or scan
 D1 overwrite:  append another extent; retain E if G/readers still need it
 read/Commit:   range cursor -> bounded buffer -> result/source consumer

 new disk work: accepted replacement and affected metadata
 daemon RAM: admitted buffers/resident pages, not the 1 GiB inherited file
```

This is a structural example, not a measured peak or proof of service read
amplification. File/metadata inspection, authenticated reconstruction and C1/C2
construction still perform their real work. An existing canonical representation
may require more service-side bytes than the requested logical range.

For a new large file, stream incoming writes into disk extents and retain a
bounded representation of its logical order; do not concatenate the file before
construction. For `node_modules`, the corresponding problem is metadata:
creation/link/timestamp state, dirty keys and saved-root associations must be
paged/iterated within the same resident target. Both paths still require their
missing live namespace/metadata and scalable submission APIs before support can
be claimed. Local disk capacity alone supplies none of those operations.

| Boundary | Required behavior |
| --- | --- |
| Incoming bytes | Use a callback borrow only while valid; own any bytes retained beyond it within the admitted buffer bound; finish extent installation before publish |
| Disk pieces | Stable owner/incarnation/range identity; immutable published bytes; bounded registry and open-FD state |
| Read output | Range plan and bounded buffers only, including local disk reads; no whole-file read_to_end |
| Frozen file source | Stream its final pieces/zero bytes through bounded buffers from the exact G view; do not read all G files to prepare one request |
| Metadata/index | Bounded resident pages and disk-backed immutable records; no full dirty-name/inode/cookie/result map hidden behind a small payload buffer |
| Copy/compaction | Reserve old and new disk allocations and RAM scratch together; never overwrite pinned ranges or compact under the capture lock |
| Progress headroom | Accepted state retains capacity for bounded capture descriptors, serialization and completion; writes cannot consume that reserved headroom |
| Physical release | Drop only after the last live/G/read/open-unlinked owner and actual backing release; pathname unlink or logical overwrite is insufficient |

No unbounded persistent version chain, background retry/compactor, automatic
Commit, quota expansion or service-owned mutable overlay is introduced. A tmpfs
path would move disk-looking bytes into memory pressure and does not satisfy the
selected low-RAM disk-backing direction. Local disk files may outlive process
failure physically, but they are not a supported restartable Workspace image:
no recovery/identity/cleanup authority is inferred from discovering them.

### 10.3 Current service bounds remain independent prerequisites

At the reviewed pin, EditFile accepts at most 256 edits and 8 MiB of replacement
input; file/base/result limits are 4 GiB. Prepared filesystem updates carry at
most 128 changed names total, 128 inode records and 128 directory records inside
32 KiB encoded metadata. C5 history result/cursor bounds are also finite.
[Current request bounds][requests]

A disk-backed Workspace can retain a larger generation than these operations
currently accept, but that fact does not make it saveable through this surface.
The full target needs an agreed bounded/streamed shared operation or declared
selection that preserves **one frozen G and its intended logical Commit**. This
is a contract prerequisite, not permission to send a private opcode, change limits
silently, issue a hidden Commit every 128 names, or keep retrying whole-file
construction after an EditFile refusal. The public API and qualification owner
for those changes are described in [03](03-commit-integration.md) and [04](04-implementation-and-verification.md).

Until those capabilities exist, a prototype must validate its actual saveability
limits before acknowledging the mutation it claims it can save. A full
`npm install` with many new entries remains an explicit support gap rather than
a successful installation whose required Commit predictably fails. A separately
chosen complete-file operation is not an error-driven fallback. New inode and
portable metadata construction, bounded rename prevalidation and exact frozen
context are still required independently of file bytes.

Zero spans save local disk writes but not transport input allowance: the full
logical zero/replacement length must be delivered and charged by the selected
operation. Disk space and small buffers do not justify bypassing that ceiling.
No registered workload is shrunk, silently split or described as passing through
an unsupported operation.

### 10.4 Cached reads do not make mapped writes part of capture

The proposed initial executable-read profile uses kernel cached I/O with
writeback-cache disabled. Kernel page cache and readahead still exist and require
separate accounting; zero additional userspace payload cache does not mean zero
kernel caching. Cached mode supports mappings in Linux, but that fact alone
does not make a writable mapping coherent with a daemon snapshot.

Exact v0.1.6 also has a process-shared 32 MiB immutable range cache, independent
of its 8 KiB acquisition-prefetch cutoff. The zero-extra-payload-cache proposal
therefore changes a real userspace cache policy. It is not a claim that the
reference rereads every above-8-KiB file remotely or that the new profile will
win repeated executable reads. [Reference cache audit](05-v016-source-comparison.md)
[Linux FUSE I/O modes](https://docs.kernel.org/filesystems/fuse/fuse-io.html)

```text
  supported write callback:
      bytes -> daemon reservation -> immutable version publication -> reply
                                   ^ capture boundary can order here

  shared writable mapping:
      CPU store -> dirty kernel page -> later filesystem delivery
                  ^ outside the proposed daemon acceptance boundary
```

Before W is exposed, the chosen adapter/kernel profile must actually reject
unsupported shared writable mappings, or implement and qualify their coherent
visibility/capture/resource behavior. Merely placing them in an unsupported list
does not stop a kernel from accepting them. Do not assume a FUSE `mmap` callback
exists for this purpose. If the selected supported dependency cannot enforce the
required mode, record that as a W capability blocker; no local third-party patch,
hidden direct-I/O mode switch or unqualified mmap implementation is permitted.

Likewise, keep writeback-cache disabled through negotiated capability settings,
not just a comment. Atomic truncation/open ordering must ensure the daemon's
length/piece cut agrees with the kernel's accepted `O_TRUNC`; verify the actual
negotiated truncate path. Refuse unsupported `O_SYNC`/`O_DSYNC` guarantees before
accepting those opens. A later unsupported `fsync` reply cannot undo an earlier
false synchronous-write promise.

Local mutations originating outside ordinary FUSE writes need correctly ordered
kernel attribute/dentry/data invalidation so current cache hits cannot return a
stale view indefinitely. Known own Commit changes representation, not visible
bytes; it must not introduce an unrelated invalidation or cache warm-up merely
to manipulate a later timing. No kernel cache state is described as cold without
the separate measurement contract's evidence.

The design claims no crash durability. Disk write completion before local
publication is not fsync, a recovery protocol or writable restart authority.
Resident unpublished state and in-flight buffers can be lost with the daemon;
leftover backing files alone do not restore its identities or outcome context.
Existing MEMORY journal/synchronous OFF/no sync/no WAL remains. Local capture,
content save, C5 stage, logical Commit and Layer publication are distinct events,
none upgraded by a `write` acknowledgement.

## 11. Required external invariants and failure schedules

These are verification obligations for the implementation in
[04](04-implementation-and-verification.md), not tests run by this document.
Exercise the real public runtime/service path; do not introduce test branches,
timing sleeps or mutable test hooks into product source.

| Invariant or schedule | Observable proof required |
| --- | --- |
| Atomic cross-file/rename cut | Each accepted operation belongs wholly before or after G; namespace/inode/length/metadata agree; separate application syscalls may straddle the cut |
| Capture without pause-through-save | After capture, accepted writes enter D1 while G still owns the submission; success matches frozen G and current reads match D1 |
| Immutable versions | A later overwrite cannot alter G bytes or an already pinned read; reference sharing has no mutable payload alias |
| G-relative successor base | Overwrite-in-replacement and G-truncate/D1-extend examples return exact expected bytes before/after own Commit and after the next save |
| Lowering coordinates | Insert/delete/overwrite combinations satisfy current-result coordinates, exact lengths and declared canonical edit policy |
| Append and truncating open | Concurrent accepted appends do not reuse stale EOF; successful O_TRUNC has effect without a subsequent write |
| Sparse gaps | Truncated old bytes never reappear on extension; zero delivery is charged to shared input limits |
| Hard links | Aliases read the same current inode version; link-count/name updates and capture agree |
| Open unlink/replacement | Open handles keep the original inode; completion cannot restore removed names or retarget a replaced destination handle |
| Late immutable query | Result for old V cannot populate a newer live namespace/cache after removal/replacement; pinned old views remain readable |
| Read spans across completion | One reply uses one version/length even when G completes or a newer write publishes between source reads |
| Directory continuation | Base/G/D1 merge preserves precedence, tombstones and stable cookies; capacity is not reported as EOF |
| Old references after multiple Commits | An old reader/directory view keeps required bytes alive; every version/segment/cookie remains charged and eventually releases |
| Capacity at COW/capture/result boundary | The next non-fitting operation refuses before visibility; no implicit backing fallback, pressure save, lost reference or uncharged RAM/disk reservation |
| Stage-only success | Exact token/candidate retained; Branch baseline and dirty G are not treated as acknowledged Commit |
| Known own Commit with newer writes | R1 replaces only G's baseline contribution; D1 tombstones, pieces and metadata remain unchanged |
| Stale head/stage token | Typed original context survives; no refresh-and-resubmit or accidental consumption of a replacement stage |
| Unknown around file/tree/stage/Commit | Frozen/successor input and exact independent observations survive; no guessed cleanup/replay or root-existence inference |
| Kernel capability enforcement | Writable mmap/sync modes are genuinely refused or separately implemented/qualified; negotiated writeback/truncate/cache behavior matches the declared capture scope |
| Disk write-before-publication | Short/failed payload or metadata writes never install partial records; stale prepared writes leave the old descriptor and charge cleanup residue |
| Exact physical/reservation accounting | Live, G, reader-pinned, zero-link, dead, failed and temporary-copy allocation remains charged without double-counting shared blocks; OS unlink alone does not refund an open backing file |
| Many tiny files | Namespace, dirty frontier, pieces, cookies, segment registry and file-result associations use bounded resident metadata; full-map memory does not grow with file count |
| Short cut with nonresident metadata | Capture rotates already-resident roots/counts; no metadata fault, dirty-ID clone, directory traversal or disk/payload I/O occurs inside the cut |
| Incremental selection | A metadata/binding-only update reuses known file roots; unchanged files are not reconstructed by the daemon; real C1/C2 read/construction work remains attributed |
| Fragmented overwrites | Repeated tiny overwrites obey declared piece/index/disk bounds; pinned old extents and failed reservations cannot disappear from accounting |
| Repeated Commit graph lifetime | Active file/namespace recipe depth and result-index growth stay bounded by the selected policy; old readers retain only charged graphs and releases make them reclaimable |
| Local reconciliation after known Commit | Local metadata-allocation/I/O failure preserves the known remote result and G/D1; no repeated remote publication or premature cleanup |
| Kernel/cgroup disk residency | Sustained disk writes/reads meet the declared measured page-cache/resource envelope; a cache hint or bounded heap alone is not proof |
| Shared-operation scaling | Large generations/new files/large replacement streams use a supported explicit operation; existing 128/256/8 MiB limits are not bypassed by hidden repeated Commits |

Pair 2's remaining H04/H06/H08/H14 evidence gaps stay visible in the packet.
A passing local snapshot test cannot substitute for real cross-boundary loss or
history-overlap schedules, and a mounted read proof cannot qualify W.

This document adds no build, benchmark, dependency, runtime code, commit or issue
update. New implementation follows the repository's actual-workspace checks,
per-worktree resource rules, locked dependencies and single construction worker.
`init_namespace` retains its separately ruled multi-worker exception; it is not
implemented as an ordinary snapshot save.

[edits]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/file/edit/input.rs
[filesystem-input]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/filesystem/input.rs
[file-read]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/file/read.rs
[service-read]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/read.rs
[service-stage]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/history.rs#L403
[history-wire]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/contract/history.rs
[requests]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/contract/request.rs
[v016-edits]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace-core/src/file_edit.rs#L72
[v016-write]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs#L1048
[v016-backing]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace-core/src/backing.rs#L13
[v016-spool]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/local_spool.rs#L71
