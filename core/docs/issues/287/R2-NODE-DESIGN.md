# R2 per-file interval nodes, cursors and custody

> **Status: Current planning checklist; no release candidate exists.**
> Read-only design audit, 2026-09-30. Audited checkout:
> `1d2fc8c2987a46acb906a1b9e720bb4cac116c7e`, branch
> `codex/issue287-implementation`, owned worktree
> `/Users/yifanxu/.codex/worktrees/issue287-implementation/layerfs`.
> Product paths below remain the R0 baseline; this audit changes no product,
> runs no build/test/benchmark and qualifies no provider.

This note supplies an implementation choice for the [R0 interfaces](R0-FROZEN-INTERFACES.md)
and [Workspace packet](../../architecture/proposal/bounded-workspace-implementation-20260930/WORKSPACE.md).
It covers SC-01/02/04/06/07 and unrelated-inode progress required by SC-08.
Root retains source, format, Bridge, integration and gate ownership. The R1a
worker owns its separate Server admission change; its edits are preserved.

## 1. Reuse source algorithms, replace population authority

| Actual source/API | Reusable part | Part that cannot serve the bounded target unchanged |
| --- | --- | --- |
| [active/page.rs](../../../crates/layerfs-workspace/src/backing/active/page.rs), `Page::new/verify`, `PageRef` | 4 KiB/128-byte authenticated framing, SHA-256 field zeroing, exact incarnation/ref checks, zero padding. | Current kinds 1–4 and zero header112..128 reject v3; extend explicitly for the frozen new kinds/scope. |
| [active/extents.rs](../../../crates/layerfs-workspace/src/backing/active/extents.rs), `Extent::cut` | Checked retained-range offsets and length/Zero semantics. | `ExtentPlan::replace/resize` accumulates all affected extents, a BTreeMap and final update Vec. A scan batch of128 is not an operation bound. |
| [active/index.rs](../../../crates/layerfs-workspace/src/backing/active/index.rs), `Index::prepare_file`, `IndexCandidate::publish` | Prepare complete pages before selected-root publication and retain failures. | Whole update slice, global pending bit/revision/generation check, created/replaced Vecs, clone at publish and retirement loop. |
| [active/keyed.rs](../../../crates/layerfs-workspace/src/backing/active/keyed.rs), `Node::decode` | Checked finite fences, ordering, reserved-field discipline. | Decoding owns per-cell Vecs; v2 hot targets need one selected directory, not direct v3 per-file roots. |
| [active/resolve.rs](../../../crates/layerfs-workspace/src/backing/active/resolve.rs), `Resolver::floor/scan` | Exact floor boundary behavior and bounded one-call scanning. | Each scan starts a new Resolver/root descent and returns a Vec; it is not a retained monotone cursor. |
| [extent/cursor.rs](../../../crates/layerfs-workspace/src/backing/binary_plus_tree/extent/cursor.rs), `Cursor::seek/next/advance` | Retained branch position, exact nonzero seek offsets and sibling descent without restarting at root. | Old length-indexed grammar, u32-based PageRef, decoded leaf Vec and old PieceStore ownership. |
| [keyed/cursor.rs](../../../crates/layerfs-workspace/src/backing/binary_plus_tree/keyed/cursor.rs), `Arena::key_cursor`, `KeyCursor::next` | Persistent ordered path and explicit reads/leaves accounting. | Holds decoded PageData/cell populations for its path and clones each result; adapt to borrowed page bytes. |
| [extent/splice.rs](../../../crates/layerfs-workspace/src/backing/binary_plus_tree/extent/splice.rs), `replace`, `Walk::descend`, `lift/pack_level` | Carry untouched subtree refs/lengths; cut boundaries; equal-level joins and root collapse. | Replacement::declare, Walk replacement/leaves and Level.pages grow with the operation. Its32-byte records and old implicit Base cannot be adopted as v3. |
| [ownership.rs](../../../crates/layerfs-workspace/src/backing/ownership.rs), `Arena::change_refs`, `RootOwner::write_raw_page` | Owner admission before exposure, verified physical identity, edge-install progress, exact ref accounting. | Old64-byte/u32 ledger and resident ledger identity/temp/custody lists. |
| [ledger_batch.rs](../../../crates/layerfs-workspace/src/backing/ownership/ledger_batch.rs), `change_refs_run/add_page_edges` | Advance a known contiguous ledger prefix, recording next edge and stopping exactly when a child reaches zero. | It takes a slice and keeps progress in a RootOwner; target uses bounded page-edge iterator and paged job/candidate facts. |
| [metadata_reclaim.rs](../../../crates/layerfs-workspace/src/backing/metadata_reclaim.rs), `RootOwner::cleanup_step` | Separate edge release, physical unlink, slot/free/refund phases; retained progress. | RootState cleanup/temp/custody and Arena ledger identities are resident lists. Its depth/temporary constants do not become target population caps. |
| [active/pages.rs](../../../crates/layerfs-workspace/src/backing/active/pages.rs), `PageStore::create_from/read/release` | Reserve, O_EXCL create, allocation readback, authenticated read, checked identity/blocks before unlink/refund. | State.entries is one BTreeMap entry per physical page, with128 B per entry; wrapping it in a new manager does not remove population memory. |
| [active/generation.rs](../../../crates/layerfs-workspace/src/backing/active/generation.rs), `ActiveBacking::read_file/capture` | Exact selected pack/extent and ordinary source callbacks. | Deferred span Vec, large/retired-large maps, capture's shared mutable owner and old index authority. |

Reuse the sound geometry and transition rules in focused `backing/active/nodes`,
`files` and `custody` implementation. No reference-root crate, new dependency,
universal pager or second mutable provider is needed. The existing older extent
tree is an algorithm donor, not a fallback when v3 fails.

## 2. Root-selected interval layout erratum

R0's initial prospective48-byte branch record lacks both persisted level and
the scalar totals needed for incremental SourceCoverage. A full delta scan on
every WRITE would reintroduce accumulated work. Root selected the following
erratum before any v3 product implementation: **interval node body layout2**,
56-byte node header and72-byte branch records. Prior prospective layout1 is
Unsupported; page kind/version/incarnation/scope/ref/checksum remain v3 as frozen.
Root updates the central freeze/checkpoint before producer/consumer enablement.

The fixed node header is:

```text
layout:u16=2 | level:u8 | flags:u8=0 | local_count:u16 | reserved:u16=0
lower:u64 | upper:u64 | intervals:u64 | replacement_bytes:u64
zero_bytes:u64 | captured_spans:u64
```

A leaf row is destination-start8 plus the frozen64-byte interval value.
A branch row is lower8/upper8/intervals8/replacement8/zero8/captured8/
provenance-bitmask8/PageRef16. Provenance uses the closed source-kind mask;
unknown bits fail. Summaries match the referenced child, including its expected
level. Node-header replacement includes Zero; zero_bytes is its checked subset.
Exact FileSet run/body totals still come from one monotone lowering pass against
the immutable construction/source context. An orphan/terminal canonical source
cannot gain Base eligibility merely because its node counter is zero.

```text
page body       = 4,096 - 128 = 3,968 B
cell space      = 3,968 - 56  = 3,912 B
leaf/branch max = floor(3,912 / 72) = 54 cells
used at max     = 56 + 54*72 = 3,944 B; unused tail = 24 B
```

Page header record count equals local_count. Leaf intervals=sum1, branch
intervals=sum(child intervals); lower/upper and length arithmetic are checked.
Empty roots use the explicit empty-root descriptor. Nonempty root height comes
from one authenticated header read; there is no inode flag hack, tag13 aggregate
table or mutable height cache. Retain the declared active structural ceiling7.
Root collapse and height refusal are explicit; never grow a chain of unary roots.
Select22..54 cells for each non-root leaf/branch, root leaf1..54 and root
branch2..54, with explicit empty root. This preserves a2/5 minimum body fill:
`56 + 22*72 = 1,640 B >= ceil(3,968*2/5) = 1,588 B`. Merge adjacent boundary
nodes when their combined count≤54; otherwise divide the bounded pair as evenly
as possible, giving the first node the extra cell. Lower-level joins handle
underfilled boundary results before installing a parent. This is a deterministic
initial format rule, requiring root's freeze adoption before encoding, not a
page-size/fill tuning campaign.

## 3. Narrow types and real I/O boundaries

Use concrete internal structs and only the supplied page/owner I/O capability.
An interval tree cannot independently allocate files, refund quota or adopt a
physical page named by untrusted bytes.

```text
SelectedFile {
  incarnation, scope_epoch, serial, logical_version, selected_generation,
  length, interval_root, immutable_source_context, root_owner
}
IntervalRoot { explicit_empty | PageRef, authenticated_level, scalar_coverage }
FileMutationTicket { exact SelectedFile, inode_conflict_lease, admitted_windows,
                     candidate_owner, publisher_credits, cancellation_state }
PreparedFileCandidate { ticket, complete_interval_root, payload/location_roots,
                        bounded_effect, paged_candidate/abort_owner, coverage }
CurrentCatalogGuard { current RevisionContext + admitted fixed path scratch }
```

`bounded_effect` is exact overwrite/shrink/extend coordinates and replacement
source ownership, not all touched keys. A root owner is an exact ledger
capability, not `Arc<OldFileVersion>` with a recursive parent chain.

The page boundary supplies:

```text
read_selected(PageRef, expected_kind/scope/level, admitted_aligned_window)
  -> authenticated fixed Page
write_candidate(candidate_owner, checked_node_bytes, bounded_edge_cursor)
  -> OwnedNode { PageRef, checked header/summary }
hold_root(owner, exact PageRef) -> fixed RootLease
enqueue_release(owner, exact root, charged_cleanup_job) -> checked job identity
```

These are responsibility/signature sketches for the root's implementation;
they do not create public methods solely for tests. The production node boundary
can remain independently usable where it is the actual supplied I/O/resource
contract, as the existing PieceStore is today. All allocation and physical effects
belong to the same v3 candidate/owner authority. No algorithm accepts PageStore's
old entries registry or an unrestricted alloc/free callback as a substitute.

Keep selected source resolution separate from node traversal. `CheckedRange`
names exact source token, authorized length and offset. It is canonical, captured
parent, owned payload/pack slot or Zero under the frozen grammar. A reader consumes
one range into caller-sized output, resolves the exact immutable location context
and releases expendable scratch. It never creates the active read_file span Vec
or asks latest-by-serial state to interpret an old token. Canonical remote reads
use the existing finite Bridge Source/delivery contract; tree code owns no channel.

## 4. Borrowed monotone cursor

Adapt the existing cursor's retained path: one authenticated page per level,
checked child slot/fence and one current leaf slot. The page owns its raw4KiB
bytes; borrowed key/value views remain valid until that cursor advances. Parsing
one72-byte interval copies only fixed scalar/token fields when a caller needs
to retain the result. Variable catalog keys are separately byte-bounded.

Seek descends once to the interval whose start is≤offset and whose end is>offset;
EOF is separately checked against selected length. Advancing beyond a leaf
increments the deepest branch with an unvisited child and descends from there.
Completed ancestors/leaves are not reopened for every interval or transport byte.
Whole passes cost O(height + reached pages + rows), with actual branch sibling
visits counted. A deliberate second descriptor/replacement pass reopens once.

The cursor checks checksum, page reference/epoch, scope/file serial, node level,
local count, row order, source bounds, child fences and zero tail before exposing
a row. Adjacent rows must cover the selected logical interval exactly. `finish`
checks full declared coverage/scalars and exact EOF. Integrity, I/O and deadline
errors preserve that selection and cursor owner; they do not root-refresh/retry.
Any borrowed cache admission counts encoded page plus actual retained metadata,
decode scratch and clone overlap before insertion.

## 5. Persistent split/join without removed-key patches

An overwrite is `split(V,a) -> left/rest`, then `split(rest,b) -> removed/right`,
followed by `join(left, replacement, right)`. Only the two boundary paths are
decoded. A whole covered child is carried or dropped by its root descriptor;
neither action visits its interval rows. A boundary interval is cut with checked
source-offset arithmetic adapted from `Extent::cut`. Appending beyond EOF adds
one Zero span; shrink keeps the prefix and owns the discarded suffix root.

Replacement construction is a bounded producer, not Replacement::declare:
at most two builder pages per level hold current rows/children. A full page is
sealed and handed to candidate custody, and only its fixed descriptor rises to
the next level. Boundary joins merge/rebalance only the admitted sibling pages,
preserve equal child levels, carry unchanged subtrees and collapse a one-child
root. Scalar coverage is checked addition/subtraction of emitted/cut/carried
facts. No whole-file normalization, repeated old delta scan or E(serial,start)
delete list appears. Body/key-width constraints and chosen fill policy govern
all split and join choices deterministically.

Each accepted WRITE publishes final interval state for that generation. It
replaces overwritten runs and shares immutable subtrees; it does not append a
checkpoint linking to the previous WRITE. A disjoint edit retains real surviving
fragmentation and pays its structural/ownership cost. True release work on a
discarded subtree is later O(actual zero-ref pages/edges), in the included cleanup
owner, not zero work or an uncharged background backlog.

Two boundary paths and two builder pages per level, including leaves, use at
most `4*(7+1)*4096 = 131,072 B` of raw node storage in this conservative layout.
Existing segments::Window is another aligned131,072 B when retained for I/O.
Contexts, frames, owner/candidate scratch, payload/source buffers, publisher
paths, cache and simultaneous operations are additional actual charged terms.
This is an allocation inventory for implementation review, not a physical/RSS
PASS or a licence to replace whole populations with another uncounted Vec.

## 6. Exact edges, candidate custody and retirement

Reuse the older ledger's transition ordering, with frozen128-byte OwnerFact and
96-byte CleanupJob records and u64 PageRefs. For each newly emitted page:

1. Reserve physical/page/ledger and failure credits before file creation.
2. Persist exact candidate owner/phase and pending native identity before exposure.
3. Create/preallocate/write/readback/authenticate the immutable page using the
   existing private-directory/no-follow/direct-I/O/identity checks.
4. Install each actual outgoing child/source edge from its bounded page iterator;
   persist next-edge progress. A partial failure records exactly the installed
   prefix. Page bytes being present do not make incomplete edges selectable.
5. Seal the page's owner and checked summaries, then allow a parent/root to name it.

A candidate's temporary root owns one reference. Each selected parent owns its
actual outgoing refs; sharing adds that new parent edge, not a pin on every
descendant. Selection transfer holds the new root before releasing the old root.
An aborted/unused intermediate root releases through its candidate job; there is
no in-memory list of every page created by the operation. Every possibly exposed
ID remains burned or retains its exact allocation owner on failure.

Dropping a root decrements that root's exact reference. Nonzero means stop at
that child; zero schedules its own bounded DFS edge cursor. The job stores next
edge and phase on disk. It decrements known installed outgoing edges once,
validates native device/inode/epoch and allocated blocks, unlinks once, records
absence, and only then refunds/free-slots. Partial ledger/unlink/refund failure
retains job/fund/physical custody. The old selecting-cohort data structures and
full inverse-ref scans are replaced, rather than hidden behind this job API.

Ledger pages/files themselves remain charged through a fixed bootstrap owner
with exact native identity and actual allocation high-water. Ledger growth is
admitted before another dependent owner becomes visible. Reuse indexed bounded
ledger-page updates and exact progress; do not recursively create an in-memory
owner for every ledger page or add a journal/recovery service. Mutable ledger
acknowledgement uncertainty quarantines its exact authority. No fsync/WAL,
automatic replay, adoption of unknown pages or guessed refund is introduced.

## 7. CURRENT publication and capture-crossing normalization

[publish_active_file_mutation](../../../crates/layerfs-workspace/src/filesystem/active_file.rs)
currently holds State throughout private mutation and refuses a changed baseline.
The target takes only an inode conflict lease through bulk preparation, pins exact
logical version V, and acquires resource/source/publisher credits before entering
State/current-catalog ownership. It cannot hold old MetadataHost::writer, active
State or Index.pending across preparation and still prove unrelated progress.

The short publisher verifies incarnation/lifecycle and current inode version==V,
then applies a statically bounded inode/dirty/location/cleanup-root change to the
**current** catalog. Capture or canonical installation may have changed the
global generation, revision or physical resolver while V's bytes remain exact.
Those events do not stale the leased logical V or authorize rebuilding against
another file. A real version mismatch under its exclusive lease is coherence
failure. There is no whole detached-root CAS/refresh/retry of all file changes.
Catalog path copying still costs O(height) for each fixed key; physical latency
remains observable. Accepted revision comes from that one publication point.

On first touch in a successor generation, inherited V is one exact opaque parent
span. Splitting it emits at most prefix/suffix parent ranges plus the current
replacement and any EOF gap. Neither candidate normalization nor later lowering
enumerates V's predecessor interval tree merely to reproduce those ranges.

For a candidate that crosses capture:

```text
A pins immutable published V and prepares effect [a,b)
capture selects V into G1 and opens G2
B may publish another inode into CURRENT G2
A enters publisher and observes new generation but same logical V
A selects retained prefix V[0,a), replacement, retained suffix V[b,len(V))
  plus checked Zero/EOF effect, under exact captured V token
publication updates CURRENT G2; B's selected state survives
```

The same rule covers shrink/extend and capture after candidate pages were staged.
Prepared pages retain their immutable creation/birth generation; accepted dirty
generation is the current catalog fact. Never retag/copy every page after capture.
Unused pre-normalization candidate pages stay owned and are released through
their paged abort job; exact source/payload refs survive the normalization.

Known installation binds V to its proved canonical result without changing V's
logical identity or revoking A's source pin. Ordinary ranges terminate at that
immediate known root. A token selecting an older/orphan version absent from the
new root keeps its terminal immutable context/exception owner; offsets are not
rebased by serial or byte equality. One pending captured-parent edge is allowed;
completed Commit/WRITE history cannot become a recursive resolver chain.

## 8. Smallest coherent R2 delivery and independent proof

R2a is a named **interval/source/custody** delivery. It includes real v3 page
encoding/borrowed cursor, per-file overwrite/shrink/extend split/join, checked
immutable source reads and exact candidate/root-edge/abort/release ownership.
It uses the actual Linux private backing. Empty modules or a tree backed only
by an external in-memory test implementation do not pass its exit gate.
The independently usable production page/root boundary may be proved first;
ordinary Workspace behavior is claimed only after the native public
read/write_file/set_len route delegates to it with the CURRENT publisher.
Namespace catalog, live handle/cookie admission and full Commit remain separate
R2b/R3/R4 gates; partial delivery keeps their capability enablement disabled.

R2a's explicit proof matrix is:

| Proof | Independent expected result / exact observation |
| --- | --- |
| Borrowed codec/cursor | External parser/model checks every field, hash/fence/level/summary/zero padding and exact ordered intervals/EOF; nonzero seeks cross leaf/branch boundaries; count reads/advances rather than timing another arm. |
| Bytes and selected roots | Deterministic fixture manifest and separate byte/interval model for overwrite, disjoint/repeated overwrite, append gap, shrink-to-zero and boundary cut. Old root/pin bytes remain exact; unchanged carried child refs stay selected without copying their leaves. |
| Exact edge custody | Independent full graph/ledger walk sums actual selected/candidate/pin refs, verifies no dangling edge/double decrement and exact device/inode/blocks. Drop one shared root, then its final pin; only the final zero-ref closure refunds. |
| Refusal/failure | Small predeclared quota refuses before root visibility; real controlled identity/unlink/provider failures retain exact job and previous or accepted selection. No inline hooks, fake allocator or guessed cleanup. |
| Bounded preparation | Vary final interval population independently from WRITE count; raw node windows/owner cache remain fixed, while actual pages/disk/cleanup grow honestly. No created/replaced/root-owner population Vec or full-delta scan per small write. |
| Capture crossing | Deterministically hold A during its real selected-page read after V is pinned; capture and unrelated B publish before A resumes. Check G1 bytes, current G2, exact source tokens, next lowering and both old pins. |

For the overlap witness, an external Linux helper can hold a write file lease on
one selected **per-file interval** page. The kernel SIGIO lease-break notification
establishes A's conflicting open/read reached that page; only then capture/B run,
then the helper releases the lease. This avoids a sleep or launched PID as the
overlap assertion. A catalog page must not be leased, or the observer itself
would prevent the operation being tested. The [Linux file-lease interface](https://man7.org/linux/man-pages/man2/F_SETLEASE.2const.html)
defines this notification/blocking behavior. Check actual filesystem/permission
support first and release within the existing callback bound; do not extend
lease-break timers or product deadlines. If unsupported, record the missing
coordination capability and choose another proved provider barrier before claiming
progress. The helper is external test coordination, not a new product dependency.

The independent private-root proof authenticates and semantically validates the
selected graph and sharing; private PageRef allocation is not a canonical
ObjectId promise. New v2 expected canonical roots belong to the independent R0
oracle/R3 path. Candidate-generated IDs cannot become expected canonical pins.
One narrower component proof does not establish SDK/POSIX overlap or physical
memory qualification. Every gate retains its source/provider/route label.

## 9. Current capability blockers and remaining ownership

[segments.rs](../../../crates/layerfs-workspace/src/backing/segments.rs) is explicit:
Linux canonical private directories require euid-owned0700 paths, ext4 magic and
4 KiB filesystem blocks; page files use O_DIRECT/O_NOFOLLOW/O_EXCL, aligned4 KiB
I/O and fallocate plus exact block readback. Non-Linux open/read/write/allocation
functions return Unsupported. Darwin logic/source checks therefore cannot pass
the real-provider R2 gate. The R0 Docker inventory established only a possible
Linux provider, not its backing/lease/edge/progress qualification.

Actual PageStore/Arena owner registries and MetadataHost's one writer remain
replacement work. Reusing their physical I/O checks does not qualify bounded
owner memory or remove global contention. R1 resident/window admission must fund
simultaneous A, B, publisher and protected cleanup before the overlap witness;
capacity unavailable up front is a declared refusal, not an admitted progress
PASS. Node-array arithmetic also says nothing about kernel/file-cache residency.

Root must adopt the stated interval fill policy and integrate the explicit
layout2 erratum with the central freeze, then own implementing the node/owner/
publisher boundary. R2a proof and native route integration are still NOT_RUN. Larger
file/body/count profiles, WRITE10,240, mmap/durability and benchmark qualification
remain separately delegated/unrun. No codec/page-size/worker/timeout campaign,
third-party modification or new dependency is proposed here.
