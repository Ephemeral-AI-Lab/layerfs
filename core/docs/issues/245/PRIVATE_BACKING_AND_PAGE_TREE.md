# #245 private backing and file page tree: algorithm audit

> **Status:** Research; informative and not a product contract.
>
> Source reviewed: `74ac30e28bcfaef180f405c9bc9045970f1678eb` (2026-09-26).
> The complexity bounds below are derived from source, not a measured speed or
> release claim. The [E/F checkpoint](evidence/phase1-e-f-final/REPORT.md)
> records functional evidence on earlier named source identities; its latency
> cells are ineligible. [Issue #248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248)
> proposes removal of the remaining fixed edit-run budget.

## 1. Three different structures, with different owners

```text
shell syscalls -> FUSE -> Workspace's visible inode/version
                              |
                              +-- namespace keyed-cell pages (private)
                              |
                              +-- file length-indexed extent pages (private)
                                      | Base  -> immutable canonical file root
                                      | Local -> immutable private payload ID
                                      ` Zero  -> logical zeros; no payload
                                                |
                       private-backing/<workspace-id>/
                       |  m-page-<slot>-<epoch> : 4 KiB immutable page
                       |  m-ledger-<index>      : 4 KiB ownership ledger
                       `  p-<payload>-<index>   : 4 KiB header + <=1 MiB data

Commit later consumes a pinned Workspace root and writes canonical C1/C2
objects. The private file page tree is not the canonical C1 content tree.
```

The namespace and file trees use the same private metadata arena and custody
machinery, but their page bodies are distinct formats. The namespace uses
ordered keyed `Cell` pages; one inode cell points to a file's `PageRef`.
The file tree uses packed `PieceRecord` leaves and `ChildRef` branches. Each
page declares its kind and format; an old absolute-key file page is refused by
the new reader rather than reinterpreted. See `PageKind::of`,
`PieceRecord`, and `ChildRef` in
[`metadata_pages.rs`](../../../crates/layerfs-workspace/src/backing/metadata_pages.rs),
and `edges_raw` in
[`metadata_index.rs`](../../../crates/layerfs-workspace/src/backing/metadata_index.rs).

The backing lives under the execution host's `private-backing` path; each
Workspace gets an incarnation-scoped directory. The Linux implementation
requires owned ext4 with 4 KiB blocks, direct I/O, aligned reads/writes, and
checked names/identities. This is a supported deployment profile, not a
generic filesystem fallback. See `WorkspaceHost::new` in
[`runtime/host.rs`](../../../crates/layerfs-workspace/src/runtime/host.rs)
and `segments::linux` in
[`segments.rs`](../../../crates/layerfs-workspace/src/backing/segments.rs).

### File sequence and a structural insertion

The logical start of an extent is the sum of lengths before it; the record
stores its **source** offset, not a logical start key. Each branch stores the
length of every child subtree. For example:

```text
old root                               new root after insertion near the front
 +----------------------------+         +-----------------------------+
 | [A: 8 KiB] [B: 8 KiB]      |         | [A': 9 KiB] [B: 8 KiB]       |
 +-----+----------+-----------+         +------+----------+-----------+
       |          |                            |          |
    A leaf(s)   B subtree                   A' pages    B subtree
       |          |                            |          |
       `---- old generation -------------------+----------+
                (B's PageRef is unchanged)

logical suffix start: 8 KiB -> 9 KiB; B's encoded page need not change.
```

The format has 4,096-byte pages with a 128-byte header, 32-byte extent records
(`E = 124` per full leaf), and 16-byte child references (`F = 248` per full
branch). A `PageRef` is `(slot, reuse epoch)`. `MAX_EXTENT = 2^24-1` bytes;
longer logical ranges split into adjacent records. The declared branch level
ceiling is 7. A file is still limited by `MAX_FILE = 4 GiB`.
[`metadata_pages.rs`](../../../crates/layerfs-workspace/src/backing/metadata_pages.rs)
defines the codec and
[`metadata_pieces.rs`](../../../crates/layerfs-workspace/src/backing/metadata_pieces.rs)
defines `LEAF_RECORDS`, `BRANCH_CHILDREN`, `MAX_HEIGHT`, `replace`, and `pack`.

## 2. Cost model and comparison

Let `P` be the file's stored extent count, `K` the extents in or at the
replacement boundary, `Q` the newly supplied replacement extents, `L` the
leaf pages visited by a sequential cursor, `H` the tree height, and `B` the
replacement input bytes. Treat `E = 124` and `F = 248` as fixed page-format
capacities. `H <= 7` is an admission ceiling, not proof that every permitted
shape is currently handled. For a well-filled tree, `H` grows approximately
as `log_F(ceil(P/E))`; sparse pages make `P/E` an estimate, not a guaranteed
page count.

| Operation | Former absolute-key index | Current source at pin | Required scalable contract |
| --- | --- | --- | --- |
| One local range write | Materialize/splice `P` pieces, rebuild the whole fixed-height index: `Theta(P + B)` CPU and `Theta(P)` resident piece descriptors | `replace` skips untouched *descendant* pages; visited-branch scans and copied paths are `O(FH + K + Q + B)` for a valid narrow shape. `Replacement::declare` still holds `Theta(Q)` descriptors. High-tree correctness is unproved (see §5). | `O(FH + K + Q + B)` work, `O(FH + q_frame)` scratch with streamed replacement, and only touched leaves plus their ancestor paths written; `q_frame` is a fixed descriptor frame. |
| Offset lookup | Could visit materialized index/piece list | `Cursor::seek`: `O(FH)` child inspection, `O(H)` page reads and `O(H + E)` cursor records | Same asymptotic bound; no full-file vector. |
| Sequential cursor | Whole-file piece vector was needed on the old path | `O(P + FHL)` CPU and `O(HL)` page reads in the worst case: `step_in` re-reads ancestors and sums prior child lengths on every leaf advance. Memory remains `O(H + E)`. | `O(P + FL)` CPU and `O(H + L)` page reads by retaining each active branch page and its running prefix in a bounded stack. |
| Immutable root capture | Materializing/copying a complete file would be proportional to `P` | Root reference/custody capture is constant-sized; later writes create pages | Constant-sized root capture, with all retained pages/payloads charged until their last reader or generation releases them. |

The current narrow-write formula is a **source-level work bound conditional on
a valid output shape**. It does not certify production latency or say that a
shell's prepend costs only `B` bytes: an application may actually rewrite its
entire suffix through POSIX, in which case those submitted bytes are real
input work. It also does not erase canonical chunk reconstruction cost at
Commit. The former path and its 1,024-piece ceiling are described in the
[earlier #245 design](ARCHITECTURE.md); the current source is the authority
for the implemented path.

`Cursor::next` consumes one leaf's records, but its `advance` calls
`step_in`, which loops over the branch stack and re-reads each ancestor;
`children[..index]` is summed again. A single small read stays path-local,
while a complete file walk can multiply branch I/O by height. Retaining
at most seven decoded branch pages would cost under `7 * 4 KiB = 28 KiB`
of branch bodies, still independent of `P`. The current
[`metadata_cursor.rs`](../../../crates/layerfs-workspace/src/backing/metadata_cursor.rs)
documents a stronger `O(H + touched leaves)` I/O claim than `step_in`
implements; that discrepancy needs a count-based gate.

## 3. Space, allocation, and custody

The payload and metadata accounts are separate from logical file length.
For one nonempty immutable payload of `b` bytes, the current backing reserves
and allocates, in bytes:

```text
A = 4,096          (direct-I/O alignment and header size)
S = 1,048,576      (data per segment)
n = ceil(b / S)
D(b) = A*n + A*ceil(b / A)

D(1) = 8,192;  D(1 MiB) = 1 MiB + 4 KiB.
```

The `Local` extent points to an immutable payload plus an offset and length;
multiple extents may refer to one payload. `Base` extents retain a canonical
read and allocate no private payload bytes. `Zero` extents store length only.
The physical allocation is `sum D(b_i)` over **live payload records**, not
the size of the final logical file; superseded payloads remain charged while
old roots/readers/frozen Commit still own them. Repeated one-byte acquisitions
therefore cost at least 8 KiB each before metadata. This is a capacity and
write-amplification fact, not a reason for a fixed edit-run ceiling. See
`segments::planned` and `PayloadHost::acquire` in
[`segments.rs`](../../../crates/layerfs-workspace/src/backing/segments.rs)
and [`payload.rs`](../../../crates/layerfs-workspace/src/backing/payload.rs).

Each live immutable metadata page occupies one 4 KiB file. Ownership records
are kept in 4 KiB ledger files with 62 slots per ledger; the ledger high-water
allocation may outlive a reclaimed page. If `M` data pages and `J` allocated
ledger files are retained across all roots/arenas, their tracked physical
allocation is `4 KiB * (M + J)`, plus payload allocation and reservations.
For well-filled trees a single root needs about
`ceil(P/E) + ceil(ceil(P/E)/F) + ...` file-index pages. The safe worst-case
statement is `O(P)` pages because leaf occupancy is not given a strict
half-full invariant. Across generations the cost is the **union** of reachable
pages plus unreclaimed temporary pages: shared suffix pages are charged once,
new path copies are charged until retired. It is neither always one tree nor
always one full copy per generation.

```text
active G2 root ----+----> shared branch/leaf ----> Local payload custody
                   |
frozen G1 root ----+          reference edges in ledger
                   |
read handle --------+          release only after last owner is gone

reclaim: root ref -> page ref count -> child/custody edges -> payload owner
```

`edges_raw` extracts branch-child and `Local`-custody edges from sealed page
bodies. `RootOwner` owns candidate pages, and reclamation walks their edges
when a page's reference count reaches zero; known partial cleanup retains
progress. See
[`ownership.rs`](../../../crates/layerfs-workspace/src/backing/ownership.rs),
[`metadata_reclaim.rs`](../../../crates/layerfs-workspace/src/backing/metadata_reclaim.rs),
and [`reclaim.rs`](../../../crates/layerfs-workspace/src/backing/reclaim.rs).
The source does not call `fsync` on Workspace backing or promise crash
durability.

| Current resource admission | Source | Consequence |
| --- | --- | --- |
| Explicit positive private disk quota; `allocated + reserved` charged before payload acquisition | `PayloadHost::reserve`, `MetadataHost::reserve` | Real physical allocation can refuse a write even when logical file length is small. |
| Four aligned 128 KiB payload I/O windows, a charged 640 KiB metadata writer reservation, and retained metadata accounting | `PayloadHost::new`, `MetadataHost::writer` | Deliberate memory bounds exist, but the configured budget alone is not an RSS or cgroup proof; `Replacement` and branch vectors still scale with descriptor count. |
| **4,096 live payload records across the host** | `payload.rs::MAX_PAYLOADS`, `PayloadHost::reserve` | Distinct still-live tiny writes can hit a second 4,096 count ceiling independently of the edit descriptor ceiling. Reclamation can free dead records, but cannot free payloads referenced by the current file or a frozen generation. |
| **65,536 metadata page slots across the host**, up to 11 arenas and 32 roots | `metadata.rs::MetadataHost`, `ownership.rs::Arena::allocate_slot` | Finite page/root capacity still exists after a fixed edit count is removed. Slots carry reuse epochs, and free slots can be reused after safe reclamation. |
| 4 GiB logical file and 7 declared branch levels | Bridge `MAX_FILE`, `metadata_pages::LEVEL_LIMIT` | Structural bounds are explicit. A count-free edit API is still limited by bytes, storage, addressability, and time. |

The 4,096 **payload-record** bound is distinct from
`MAX_EDITS_PER_OPERATION = 4,096` in the
[`Bridge request contract`](../../../crates/layerfs-bridge/src/contract/request.rs)
and `metadata_pieces::replace`. Removing only the latter would still refuse
4,097 distinct retained payload acquisitions. A truly count-free final-delta
contract needs a bounded-memory ownership index or packing/compaction scheme
that admits by actual bytes/resources instead of an arbitrary record count.
Such a change must preserve immutable payload identity and the custody graph;
its precise implementation remains an #248 design decision.

## 4. Why the target can remain bounded

Bounded **resident** memory and bounded **total** work are different claims.
A fixed 128 KiB I/O window, one leaf, and at most seven branch pages give
`O(A*H + window)` cursor scratch independent of `P` and of the number of edits.
A streamed replacement descriptor frame gives a similarly bounded splice
scratch target. Disk use is bounded by the configured quota, which can cause a
clear capacity refusal; work for a file of length `N` still has a necessary
`Omega(N)` lower bound when the shell submits `N` new bytes or Commit must
construct those bytes. The relevant optimization is to avoid `Theta(P)`
**extra** index work for every small callback.

The current code has checked arithmetic, file/height/slot limits and operation
deadlines, but its `Replacement::declare` materializes `Q` extents and the
cursor repeats ancestor reads. Therefore **bounded-memory replacement and
linear-page Commit traversal are targets, not established properties** of
this source. A configured host memory budget cannot substitute for observing
process and cgroup memory under a declared cache state.

## 5. Source-derived correctness risk and acceptance gates

`Walk::descend` returns untouched child references at their existing level,
while a touched child can return references to its children. At height 2,
those can appear in the same returned vector. `pack` then encodes that vector
as one level-1 branch, without checking or normalizing the child levels.
`Cursor::seek` requires successive branch levels to descend by exactly one.
Thus a **narrow splice on a height-2 tree may publish a mixed-level root that
later reads reject**. This is a source-derived risk, not yet an externally
reproduced failure; the existing external `pieces_sequence` tests exercise
at most one branch level with nonmergeable extents. The same packing code may
also refuse an otherwise representable 249-leaf tree because a final
one-child branch is invalid. See `Walk::descend`, `pack`, and
`Cursor::seek` in the files above, plus
[`pieces_sequence.rs`](../../../crates/layerfs-workspace/tests/pieces_sequence.rs).

```text
old height-2 root
  L2 -> [ L1 subtree A (touched), L1 subtree B (untouched) ]
          |                             |
          +-> L0 leaf references        +-> L1 page reference
                    \                     /
                     returned together to pack(...)
                       -> new L1 parent   <-- mixed child levels
```

The following deterministic gates make the intended algorithm checkable:

1. Build and read files at the height transition: 248, 249, and 250 leaf
   pages, with **nonmergeable** extents. A full leaf holds 124 records, so
   249 leaves first appears at 30,753 records and 250 at 30,877. Exercise
   narrow edits in the first, middle, and last subtree, then seek/walk every
   boundary, verify page-level descent, and verify the old root still reads.
2. For fixed-size replacement on progressively fragmented files, record
   index page reads/writes, child references scanned, allocation and wall
   separately. Assert that narrow-edit page visits do not grow with untouched
   suffix extent count; explicitly measure the height transition. This is a
   count-driven structural check, not a repeated performance sample.
3. Walk the same file once with a cursor and count branch reads. To claim
   `O(H + L)` page I/O, each branch should be read only as needed to enter it,
   with an amortized bound proportional to distinct visited pages. The current
   `step_in` implementation does not pass that expectation by inspection.
4. Hold G1, edit G2, and retain an open reader; verify exact bytes, shared
   page IDs, payload custody and quota accounting before and after G1/read
   release, failed publication, and cleanup. Force slot reuse and reject a
   stale `(slot, epoch)` reference.
5. After #248 removes count ceilings, accept 4,097 distinct tiny writes with
   sufficient physical quota, then Commit the final delta. Assert no fixed
   edit-run or live-payload-record threshold fails first. Also test a real
   quota refusal and bounded resident/cgroup memory under a declared cache
   contract. Keep all nonpassing evidence.

These gates separate page-tree correctness, asymptotic index work, private
physical cost, and end-to-end Commit semantics. None can be inferred from a
single small-file PASS or from a warm elapsed-time number.
