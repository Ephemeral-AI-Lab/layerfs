# File content: small and large files, CDC, immutable COW and storage handoff

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Component 2 within [Cluster 1](canonical-content.md).
Tracking: [#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).
Three read-only reviews covered edit inputs, COW finalization, and reads/transitions/
physical hints against the [pinned reference source](content-io-memory-audit.md#1-source-provenance-and-evidence-levels).
No implementation, build or benchmark was performed.

Read with [canonical objects](canonical-objects.md),
[finalized-object handoff](finalized-object-handoff.md),
[content I/O](content-io.md) and [policy/tables](content-storage-policy-and-tables.md).
This document owns the proposed file-operation contract and its cut list. Detailed
handoff lifetimes and C2 codec/pack/session rules remain in their linked owners.

## 1. Decision and scope

Keep two explicit write modes, complete construction and known-edit construction,
plus bounded reads. Preserve CDC, extent slicing, split/concat/coalescing and
canonical partition choices. Replace encoded intermediate structural objects with
owned unfinished nodes inside the same file algorithm. This is an algorithm
representation change to prove, not a new generic COW engine or plugin framework.

```text
stable complete input OR immutable base + normalized known edits
                              |
                       C1: File content
                 /            |             \
              empty          small          large
                |              |              |
          defined state   whole-file      CDC / extents
                          object          immutable COW
                 \            |             /
                  +-----------+------------+
                              |
                    finalized canonical objects
                    + bounded predecessor hints
                              |
                       C2: physical storage
                       exact CAS reuse first
                       FULL / eligible DELTA
                       compression / packs / SQL
```

Small and large files are both in scope. Whole-file and chunk delta encoding are
co-designed through the handoff; their implementation stays in C2. Defaults remain
cutoff T = 128 KiB, whole-file delta depth 8 and chunk delta depth 4, configurable
within supported capacities. The task is configurable transparency, not tuning.

The size selector below applies to regular files. Shared rope primitives also
serve metadata values, which retain their existing extent-only representation.
Preserve that use through explicit purpose/profile inputs; applying the regular-
file cutoff to every metadata rope would change its canonical format.

Live Workspace COW, raw write capture/normalization, backing, freeze lifecycle,
FUSE and transport are later owners. The input contract below is independent of
their implementation. Input acquisition and normalization costs remain visible
in complete-operation measurements wherever they are required.

## 2. Minimal operation inputs and results

These are semantic requirements, not finalized Rust types or new services.

| Operation | Inputs | Result |
| --- | --- | --- |
| Complete construction | Stable sequential source, optional exact length, explicit canonical policy/profile, bounded output | File root ObjectId and logical byte length |
| Known-edit construction | Immutable base, declared final length, normalized ordered edits, stable replayable replacement ranges, explicit policy/profile, authenticated reads and bounded output | New file root ObjectId and logical byte length, or the preserved base root for an established no-op |
| Range read | Authenticated file state held by the operation, requested range(s), bounded output sink | Requested bytes in logical order, or an error |

Use ordinary Read/Write-like capabilities and a small range-opening callback where
replay is required. No source framework, whole-file cache, mandatory Vec of edits,
or collection of all replacement bytes. An operation-local file view reuses an
authenticated small object or decoded file state and root; it is not a new cache
service. Storage completion remains separate from construction completion.
Tree counts/levels needed for parent construction stay in the internal builder
state; no generic logical-summary result is required. The returned root is a
content address, not a checkpoint or history record.

Complete input without a length keeps the existing bounded threshold probe.
Do not pre-scan merely to learn the length. Known lengths select the route directly;
verify actual byte counts and EOF where a complete source or declared replacement
span ends. A bounded range provider validates its span, not EOF of the entire
underlying file. Errors are not EOF, and length checks do not establish stability
against concurrent mutation. The source must refer to the same immutable input.

## 3. Edit coordinates and normalization

Preserve the current coordinate convention. Each edit describes:

```text
start            offset in the CURRENT result after preceding edits
delete_len       bytes removed at that position
replacement_len  exact number of replacement bytes
replacement      stable source whose ranges can be reopened
```

Example:

```text
base:   abcdefghij

edit 1: start 2, delete 2, insert XYZ
        abXYZefghij

edit 2: start 7, delete 1, insert Q
        abXYZefQhij
```

The [reference batch](../../../../../crates/layerfs-content/src/file/rope/edit.rs#L104)
applies each replacement to its current root. The
[reference caller](../../../../../crates/layerfs-workspace/src/changes.rs#L1935)
tracks base and final cursors, passing the final cursor to the batch. This is
source evidence for semantics, not a requirement to import Workspace pieces.

The target normalized stream is monotone, including deletions, and cannot replace
or remove an already-finalized inserted span. Do not silently merge adjacent edit
streams: CDC restarts per replacement operation, and segmentation/order can affect
canonical roots. Conversion from other coordinates must be explicit, checked and
measured; byte equality alone does not establish identical canonical structure.

The current finalized_through guard advances only after nonempty replacement.
It therefore admits some delete-only sequences beyond the proposed monotone
contract. Verify required callers and compatibility before tightening admission;
do not claim the old guard already proves monotonicity or silently reorder an
accepted sequence. The existing normalized product caller provides the starting
contract. Private API permissiveness need not become another execution mode.

Check range bounds, arithmetic and actual replacement lengths. Declared final
length chooses the final representation once; accumulated edits must agree with
it before success. This avoids constructing intermediate small/large versions
within one normalized operation. Across separate published operations, each
operation still applies its own final-size rule.

## 4. Empty, small, large and transition paths

```text
validated final logical size
           |
           +-- 0 ---------> defined empty file state
           +-- 0 < n < T -> one whole-file canonical object
           +-- n >= T ---> chunked file and extent mappings
```

| Case | Required construction |
| --- | --- |
| Small -> small | Fill and hash the complete resulting whole-file object; retain eligible whole-file predecessor information |
| Large -> large, known edits | Reuse unchanged extents/subtrees; CDC replacement bytes and rebuild affected mapping paths |
| Small -> large | Stream retained old bytes and replacements through the existing complete CDC builder |
| Large -> small | Fill one final whole-file object from retained ranges and replacements; do not read discarded ranges solely for conversion |
| Any -> empty | Validate the edit/length contract and construct the defined empty state |
| Complete input | Consume all supplied bytes through the selected complete builder |

Small-file construction is generally proportional to final file size. Physical
delta can reduce storage but does not remove canonical construction/hash work.
For known final small results, target one final canonical allocation instead of
separate head, tail, replacement-probe and inner-envelope allocations. The old
authenticated object/read wave may coexist; memory is not exactly T.

The [current replace wrapper](../../../../../crates/layerfs-content/src/file/content.rs#L245)
probes replacement bytes, then can materialize head/tail and feed another build
probe. Known lengths remove classification-only work. Preserve one bounded probe
for explicit unknown-length complete input; this is an input mode, not fallback.

At T-1/T/T+1, exactly T is chunked. Crossing the cutoff requires representation
conversion, even for a one-byte edit. Preserve empty semantics and both transition
directions. Large -> small may acquire whole chunks, groups or delta bases for
tiny retained slices; selected logical output length is not a physical-I/O bound.

## 5. Immutable file COW and final construction

### Preserve the actual edit algorithm

```text
old: [A] [---------------- B ----------------] [C] [D]
                         edit

new: [A] [B prefix slice] [X] [B suffix slice] [C] [D]
          same old payload IDs + offsets reused

X: newly constructed replacement payloads
Only affected mapping paths and necessary join boundaries are rebuilt.
```

The reference [replacement path](../../../../../crates/layerfs-content/src/file/rope/edit.rs#L49)
scans replacement bytes, splits the old mapping around deletion, and concatenates
left/replacement/right. Unchanged extents keep payload IDs/offsets; unchanged
subtrees keep summaries. Preserve coalescing, root collapse, half partitioning,
fanout/height limits and root/non-root validation. Known edits must not silently
switch to full-file CDC or a full extent-tree rebuild.

### Accurate baseline: structural overlays, already bounded

The current FileMutationBatch already streams replacement payloads and sealed
interior mapping nodes. Its deferred map holds structural candidates, not the
entire file payload. It prunes above 4 MiB and rejects charged deferred storage
above 8 MiB minus 1 byte. See [payload/sealed output](../../../../../crates/layerfs-content/src/file/rope/edit.rs#L166)
and [structural limits](../../../../../crates/layerfs-content/src/file/rope/edit.rs#L198).
Those thresholds apply specifically to DeferredFileObjects.charged_bytes, not
the inner DeferredNodes map or combined CDC, output and operation memory.

Two mechanisms overlap: per-replacement
[DeferredNodes](../../../../../crates/layerfs-content/src/file/rope/state.rs#L64)
and batch-wide DeferredFileObjects. The cut is their encoded lookup, cloning,
rehashing, decoding and graph-pruning work. Generic candidate payload staging
elsewhere is a separate handoff removal target; do not credit it to every file edit.

### Proposed private representation

```text
Existing subtree                       Unfinished subtree
  ObjectId + known summary               owned decoded entries + summary
                   \                    /
                    split / concat / coalesce
                              |
                  retain unresolved join boundaries
                              |
                  no later permitted edit/join changes node
                              |
                     seal children before parent
                              |
                     encode/hash once -> emit
                              |
                         keep summary
```

Use one private unfinished-node representation inside file code. Read an existing
node only when an affected path needs it. Update decoded entries directly, drop
replaced drafts through ownership, and encode the final file-state wrapper once.
Do not build a general hash-addressed draft store or preserve intermediate roots
solely to make unfinished nodes addressable.

Finality must include both sides of joins. Two valid leaf roots with non-coalescing
adjacent extents need not keep their identities after concatenation:

```text
80-entry left leaf root + 100-entry right leaf root
                     |
             existing concat partitions
                     |
              90-entry + 90-entry nodes
```

Thus valid occupancy or immutable encoded bytes alone do not prove finality.
Preserve the [existing join/partition choices](../../../../../crates/layerfs-content/src/file/rope/edit.rs#L526).
Similarly, a later edit deleting an earlier emitted replacement would orphan its
output; reject unsupported overlap through the normalized contract.

The target is bounded active paths, replacement-building levels and unfinished
join boundaries, independent of total file bytes or edit count. A bound proportional
to fanout times supported height needs a proved constant and a hard allocation
budget. Decoded nodes may be larger than encoded nodes. This finality/memory proof
is required before deleting either overlay; no spare full-rebuild fallback remains.

## 6. No-op preservation and replay

An empty edit stream can retain the base root after checking declared length and
compatible profile/representation. Explicit policy/format conversion follows its
qualified conversion contract; do not promise unconditional root reuse alongside
a conflicting representation change or reclassify stored content silently.
For small results, the completed canonical identity supports exact reuse without
an obligatory preliminary byte comparison. Large-file byte-identical replacements
can otherwise change CDC boundaries or mapping layout, so preserve the reference's
same-position/equal-length changed-range comparison where it applies.

```text
candidate unchanged-range edit
             |
bounded compare: replacement vs immutable base
             |
         +---+---+
         |       |
       equal   different
         |       |
   retain old   reopen replacement range
   references   and construct using original edit segmentation
```

The [current comparison](../../../../../crates/layerfs-workspace/src/changes.rs#L1953)
and [bounded comparison loop](../../../../../crates/layerfs-workspace/src/changes.rs#L2009)
demonstrate this semantic requirement. C1's edit operation owns it; callers need
not assert that bytes changed. Its applicability includes original/current-position
correspondence, not merely equal deletion and insertion lengths after a shift.

A non-replayable replacement may contain a long equal prefix before a mismatch.
Keeping that entire undecided prefix would violate bounded memory. Therefore
known-edit replacements provide stable range replay, for example an ordinary
range-opening callback. Replay offsets address the replacement source; edit offsets
still use current-result coordinates. Reopening a mutable pathname is insufficient.

Comparison may consume replacement bytes once, followed by construction reading
them again. Count both passes, base reads and waits. Do not promise single-pass
no-op detection. Complete construction can accept a sequential non-replayable
source; another caller must explicitly arrange stable edit input if needed. No
automatic materialization/spill or silent switch to complete reconstruction.

Cut automatic whole-file equality scans from trusted known-edit routes where
changed-range/provenance checks establish the required behavior. Do not remove
necessary equality checks or label explicit whole-content equality as cheap.
The reference already avoids its general whole-file comparison for matching-base
known edits through structural checks. Preserve that behavior; an absent scan is
not a new performance saving. Reusing read plans in required changed-range checks
is a concrete remaining reduction.

## 7. Bounded reads and delta cooperation

### Read from authenticated file state

```text
authenticate/open base once per operation
                 |
requested ordered ranges -> affected mapping traversal
                 |
bounded payload-demand wave -> grouped authenticated C2 reads
                 |
emit borrowed slices in requested logical order
```

Reuse [read_plan_from_state](../../../../../crates/layerfs-content/src/file/rope/read.rs#L62),
subtree skipping and bounded payload waves. Avoid repeated root/plan acquisition
for each retained prefix, suffix or equality block. Do not materialize all extents.
Repeated slices of one object within a wave can share one authenticated owner;
remove duplicate-demand payload clones while preserving order, cardinality and
bounds. Actual SQL/codec copies and cross-wave reacquisition remain accounted for.

### Carry useful delta hints with final output

```text
whole-file object + eligible whole predecessor --+
chunk object + bounded correspondence hints -----+--> C2 exact reuse
                                                      |
                                              FULL / eligible DELTA
                                              compression / packs / DB
```

Keep the existing metadata-only
[PredecessorCursor](../../../../../crates/layerfs-content/src/file/rope/read.rs#L365)
and its bounded search. Attach hints during final output instead of requiring
deferred-candidate replay solely to compute them. Share established traversal
results when equivalent; do not replace the cursor with a complete extent scan.

The cursor can supply up to four IDs, while current native chunk admission uses
the first. Preserve actual candidate order/selection and work limits; widening
search is a separately qualified change. The existing small predecessor locator
lookup is already batched. Batch distinct native predecessor locations within the
admission wave where semantics permit, preserving dependency and race checks.
These hints are advisory delta-base candidates, not stored dependencies. The
[C2 selection design](physical-encoding-and-packing.md#4-payload-selection-and-delta-base-candidate-quality)
also retains its bounded admitted-FULL cache for WHOLE_FILE when no explicit anchor
is usable. Selected bases become mandatory dependencies. A failed codec/read step
fails the operation; there is no retry or error-to-FULL recovery.

Current-result edit offsets, replacement-source offsets and predecessor spans
have different meanings. Hints are physical correspondence, not proof that the
same logical offset names the same original bytes. Keep regular-file provenance
explicit; metadata ropes can share the canonical chunk format.

Both whole-file and chunk DELTA paths remain. At representation transitions, no
cross-role base or chain continuity is promised: C2 reuses exact objects or selects
an eligible representation. FULL can be the correct result of its policy. Required
missing/corrupt dependencies remain errors. The handoff carries no codec, SQL,
Workspace, host or daemon handle.

## 8. Memory and time ownership

| Working state | Bound/ownership requirement |
| --- | --- |
| Small final object / unknown-length probe | Derived from supported T and exact framing; separate authenticated-base and conversion overlap is charged |
| CDC scanner / replacement builder | Existing chunk/window and per-level limits; no whole-replacement collection |
| COW unfinished nodes | Account both join boundaries, active old paths, decoded container capacity, summaries and recursion; prove a hard bound before overlay removal |
| Equality and retained-range reads | Bounded replay/read windows and authenticated payload waves; no retained undecided whole replacement |
| Hints / direct references | Bounded per object/wave, released after consumption; no operation-sized graph/index |
| Output / C2 / adapter buffers | Apply the handoff's simultaneous-allocation ledger and backpressure across all files |

Reuse one authenticated file view/read plan within its valid operation lifetime.
Every retained allocation still needs a reason; do not retain decoded old paths
after their last use merely to avoid a future read. Large -> small fragmented
ranges can require many distinct chunk/group/base reads while memory stays bounded.
Work follows required acquisition and touched structure, not final size alone.

Use existing coarse timer scopes for complete construction, known edits, range
reads and actual storage calls. Equality, replay, base acquisition and blocked
output time remain inside their required scopes. Construction wall time is not
pure CPU time, and overlapping spans are not additive. No per-chunk trace nodes,
extra workers or new telemetry runtime. Operation-wide concurrency and the
namespace-init exception follow the existing contract.

## 9. What to cut and what to preserve

| Target cut | Source evidence | Replacement / gate |
| --- | --- | --- |
| C2-owned checked file construction and broad Store-driven policy | [checked build](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L3234), [format access](../../../../../crates/layerfs-content/src/object/access.rs#L64) | C1 complete/edit bodies with explicit profile and selected consumer; no duplicate constructor |
| Classification probes for already-known lengths; head/tail and inner-envelope copies | [replace](../../../../../crates/layerfs-content/src/file/content.rs#L245), [encode_small](../../../../../crates/layerfs-content/src/file/content.rs#L69) | Direct final small allocation or streaming composed input; keep bounded unknown-length support |
| Repeated root/plan acquisition | [inspect and retained reads](../../../../../crates/layerfs-content/src/file/content.rs#L252) | Reuse authenticated file state/read plan held by the operation; preserve trust/context checks |
| Two encoded structural overlays | [DeferredFileObjects](../../../../../crates/layerfs-content/src/file/rope/edit.rs#L198), [DeferredNodes](../../../../../crates/layerfs-content/src/file/rope/state.rs#L64) | One owned unfinished-node representation; finality, exact-root and memory proof required |
| Reachability prune/final clone-decode traversal of draft nodes | [prune](../../../../../crates/layerfs-content/src/file/rope/edit.rs#L235), [commit](../../../../../crates/layerfs-content/src/file/rope/edit.rs#L300) | Drop overwritten drafts immediately; seal remaining frontier child-first after proof |
| Intermediate FileState encoding and rereads between edits | [replacement root](../../../../../crates/layerfs-content/src/file/rope/edit.rs#L49) | Carry current root summary internally; encode final wrapper once |
| Duplicate structural validation immediately after checked decode | [load_node](../../../../../crates/layerfs-content/src/file/rope/edit.rs#L746), [decoder](../../../../../crates/layerfs-content/src/file/extent_codec.rs#L119) | Reuse checked decoder result; retain distinct summary/root-context checks |
| Repeated-ID read payload clones | [read_object_rows](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L3869) | Shared authenticated owner within bounded wave; preserve demanded output order |
| Candidate replay solely for hint attachment; per-target native locator probes | [hint replay](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2812), [native predecessor](../../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L1312) | Final-output hints and bounded batched C2 lookup; retain actual selection and all required reads |
| Broad Workspace-shaped input routing and generic equality entry points in the construction context | [routing](../../../../../crates/layerfs-workspace/src/changes.rs#L1865), [whole-file comparison](../../../../../crates/layerfs-workspace/src/changes.rs#L2034) | Explicit complete or normalized-edit mode; preserve existing known-edit scan avoidance, bounded no-op semantics and visible adaptation costs; errors never trigger a full rebuild |

Some rows are C1/C2 integration cuts. Existing Workspace locations identify
semantics/coupling to extract; they do not prescribe the replacement Workspace
model or establish a Workspace speedup. Overlays can only be deleted together
with their proven replacement. No generic fallback keeps the old algorithm active
in the replacement product.

Preserve existing CDC/hash/profile behavior, exact extent slicing, split/concat/
partition/root-collapse rules, bounded no-op checks, authentication, EOF/arithmetic
checks, required predecessor information, FULL/DELTA/compression, packing efficiency
and old-root immutability. Keep active supported formats under the compatibility
contract. These are the optimizations and correctness obligations being carried
forward, not overhead to delete.

## 10. Qualification and implementation order

First extract complete empty/whole/chunked construction through real C2 save and
authenticated readback, with isolated construction using the same bodies. Then
add known single edits/transitions and the decoded multi-edit implementation.
Reference code stays separate until qualification; no runtime old/new fallback.

Use the existing [extent model oracle](../../../../../crates/layerfs-content/tests/extent_model.rs#L479)
for batch-versus-sequential root identity, unchanged old roots and no unreachable
new output. Preserve the [overlap rejection](../../../../../crates/layerfs-content/tests/extent_model.rs#L548)
and [streamed replacement](../../../../../crates/layerfs-content/tests/extent_model.rs#L571)
cases. Replacement tests belong outside product src/. Extend proof to:

- Empty, T-1, T and T+1; every transition and accepted larger cutoff.
- Current-result insertion/deletion coordinates, adjacent edit segmentation,
  delete-only monotonicity and required-caller compatibility.
- Equal-byte edits, long equal prefix then mismatch, and stable range replay.
- Mapping boundaries around 63/64/127/128/129 and flush 192/193; height growth/
  collapse, coalescing, repeated IDs and many length-changing edits.
- Fragmented large -> small retention and repeated chunk slices in read waves.
- Whole/chunk delta opportunities, missing optional hints, required dependency
  failures and predecessor-coordinate drift after insertions/deletions.
- Slow source/storage, blocked emission, sink failure and late length mismatch.

Match canonical roots for the same base/profile/normalized edit sequence and
route semantics. Equal final bytes alone are insufficient. Record actual source
passes, mapping/payload/base reads, hash/copy/encode work, final DB/index/pack size,
allocation coexistence and end-to-end time. Preserve the current algorithms'
complexity and existing-or-better speed/storage/memory under the
[I/O qualification contract](content-io.md#7-measurement-and-completion).
No worker, cache-state, workload or safety-limit changes can manufacture a pass.

Open proofs are decoded-boundary finality/memory, exact supported-caller mapping,
no-op applicability under coordinate changes, supported-capacity end-to-end behavior
and measured gains. The [filesystem-tree proposal](filesystem-tree.md) owns logical
tree integration and compact reference ordering. Live Workspace COW remains later
and may use a completely different shape; this component requires only its neutral
stable-input contract. No new crate or public API is frozen by this document.
