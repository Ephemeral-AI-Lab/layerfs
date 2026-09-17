# Filesystem tree and metadata: updates, references and immutable COW

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Component 3 within [Cluster 1](canonical-content.md).
Tracking: [#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).
Three read-only reviews covered directory/inode COW, reference ordering, and
logical/physical metadata against the [pinned reference source](content-io-memory-audit.md#1-source-provenance-and-evidence-levels).
No implementation, build or performance measurement is claimed.

Read with [canonical objects](canonical-objects.md), [file content](file-content.md),
[finalized-object handoff](finalized-object-handoff.md), [content I/O](content-io.md)
and [physical storage](object-storage.md). This document owns component 3's proposed
inputs, ordering decision, reuse/cut list and qualification. The benchmark family
init_namespace remains an operation name, not this component's name.

## 1. Decision and scope

Reuse the existing bounded sorted-tree engine. Feed final directory bindings and
typed inode values directly into it. Keep portable attributes and bounded generic
attribute storage; remove platform-specific semantics from C1/C2. Use one private
inode-effects reducer for reference accounting and ordered
final records; retain compact ordering records where arbitrary change order needs
them. Cut encoded staging and repeated conversions around these algorithms.

This is one component with ordinary internal directory, inode and attribute code.
No new filesystem framework, generic scratch service, resolver service or crate
inventory is introduced. Live Workspace COW, mounts, daemon protocols, publication
and raw command capture remain later owners. Logical diff/conflict/resolution is
[deferred to v0.2.0](diff-conflict-deferral.md); required lookup, equality and update
validation remain.

### Independence is a design requirement

Existing Workspace code supplies evidence of semantics, resource costs and
algorithms. It does not define the replacement input shape, lifecycle, checkpoint
model, temporary-file layout or process placement. Do not recreate its context
object with a generic name.

```text
any future caller
  local application / importer / FUSE integration / remote service / Workspace
          |
          | stable logical inputs + policy + authorized identities/resources
          v
C1: directly callable logical construction and reads
          |
          | finalized objects / authenticated object access
          v
C2: directly callable object admission, encoding, packing and persistence
          |
          v
selected storage backend
```

C1 can run with supplied immutable reads and a bounded consuming destination;
C2 can save/read supplied canonical objects without C1 construction, a Workspace,
mount or history entities. Each can be measured independently and composed through
the same contract. Required provider/ordering work remains in its declared scope.
Host, daemon, container and cloud placement belong to adapters. Ordinary explicit
capabilities provide extensibility; no runtime plugin registry is required.

## 2. Representation to preserve

The supported compact profile already uses scoped identities and inline records:

```text
Filesystem root
  profile + allocation scope + root-directory serial + inode-table root
                                      |
                                      v
Inode table: serial -> INLINE record
                        kind / namespace reference count
                        content root / metadata root
                             |               |
             +---------------+               +--> attribute tree
             |                                    key -> value file root
             |                                              |
       kind selects meaning                           extent-only rope
       +-- directory: name -> inode serial
       +-- regular file: File content root
       +-- symlink: target object
```

Sources: [compact root](../../../../../crates/layerfs-content/src/tree/compact.rs#L249),
[inline values](../../../../../crates/layerfs-content/src/tree/compact.rs#L291),
[inode fields](../../../../../crates/layerfs-content/src/tree/inode/record.rs#L44).
Identity is scope plus serial, not a globally meaningful serial alone. Keep the
existing canonical profile, page partition rules, size/height limits and name/path
validation. Inline records are an existing optimization, not a new format proposal.

Directory bindings reference inode identities, not child file-content ObjectIds.
Consequently a content or metadata edit does not rewrite every ancestor directory:

```text
file edit / attribute edit                 cross-directory rename
         |                                         |
new content / metadata root               source + target binding changes
         |                                         |
updated file inode value                  affected directory pages
         |                                         |
         |                                updated parent-directory inode values
         +----------------------+------------------+
                                |
                      affected inode-table pages
                                |
                       new filesystem root
```

Preserve the [existing update indirection](../../../../../crates/layerfs-content/src/filesystem/apply.rs#L92)
and [rename behavior](../../../../../crates/layerfs-content/src/filesystem/apply.rs#L157).
Noncompact supported formats may require separate inode-record objects. Select
the declared write profile explicitly; do not delete necessary old-format handling
or silently convert a Store to make all inputs look compact.

## 3. Minimal operation contract

| Operation | Explicit inputs | Output |
| --- | --- | --- |
| Resolve / stat | Checked immutable root/profile/scope and canonical path | Inode identity/value and relevant checked root fields |
| List | Directory root, cursor/range, count and byte limits | Bounded page of bindings |
| Read attributes | Metadata root and requested keys | Checked values, with bounded shared acquisition |
| Build/update directory | Optional base directory root; strictly sorted unique name -> final inode/absence changes | Directory root and known count and level, plus finalized objects |
| Build/update inode table | Optional base table; strictly sorted unique identity -> final typed record/removal | Table root and summaries, plus finalized objects |
| Complete filesystem construction | Checked bindings and inode changes, derived reference effects and authorized identities | Filesystem root and required count/level summaries; C2 completion remains separate |

Conceptual final-state inputs:

```text
directory: (name, Some(inode)) = final binding
           (name, None)        = final absence

inode:     (identity, Some(typed record)) = final value
           (identity, None)               = removal
```

No all-files result map or whole-tree manifest is required by these interfaces.
File content supplies completed roots and lengths; this component does not run
CDC or choose physical delta bases. Share the handoff's operation-wide batches
and backpressure rather than committing once per file, directory or inode.

### Checked inputs precede finalized output

Checked logical inputs are a native core entry point for any caller. A public
path-command front end is optional and shares the same validation bodies; qualified
identity-based input need not pass through a Workspace-like command planner.
Preserve name-collision, identity, single-parent and rename-ancestry/cycle rules
against the valid base and effective bindings. Establish final binding and retained-
membership for each output before promising that output as final. Validated bindings and inode changes can
arrive incrementally; no all-files plan or reference Workspace graph is required.

Internal sorted-tree primitives consume checked bindings and inode changes as a precondition and remain
independently callable/measurable. Raw caller assertions or serialized retained/count
flags are insufficient. If an input cannot establish retention incrementally, account
for its necessary prepass, ordering records or retained state explicitly. Sorting,
topology checks, repeated reads and identity acquisition remain charged where they
occur. Invalid or mutating input fails the unpublished attempt. Arbitrary graph
imports need separately qualified validation, not a scan-free correctness promise.

New identities arrive as authorized values in the declared scope; C1 does not
allocate them. The existing SQLite [reservation](../../../../../crates/layerfs-layerstack-store/src/schema.rs#L198)
durably burns exposed serial ranges even if later work fails, with that cost owned
by the allocator. Other allocators must preserve uniqueness, scope and safe exposed-
identity reuse rules without importing SQLite transactions or the current Workspace
reservation lifecycle into C1. Remove hidden allocation from the broad ObjectStore
interface; keep the selected allocator's required effects visible.
The durable-burn behavior above is reference evidence. It is not an instruction
to introduce durability into replacement C1/C2, which consume authorized identities
under the current no-WAL/no-added-durability scope.

## 4. Reuse sorted immutable COW

```text
old subtree IDs + next ordered change
                    |
       active decoded pages / unresolved outside siblings
                    |
       neighboring input establishes final partition
                    |
       child-first final emission + parent summaries
                    |
       encode final filesystem root once
```

The [sorted tree engine](../../../../../crates/layerfs-content/src/tree/batch.rs#L1)
already reserves bounded working memory, retains unresolved siblings, skips
unchanged descendants and emits final pages. Preserve its 4-MiB internal budget,
count/byte checks, format-specific partitioning, canonical page identity reuse
and required sibling/summary validation. Its budget is not total operation memory.

Avoid repeated point insertion/removal with encoded DeferredDirectory/DeferredInodes
maps on supported final-state paths. Initial sorted directory construction should
use the engine's existing optional-base-root pattern rather than wrap it in another
deferred object graph or emit a provisional empty seed. A genuinely empty result
still emits its required canonical representation.

Return count, level and root already known to the engine. Keep only unfinished
boundaries and necessary summaries. A local directory page being final does not
prove its directory survives the entire filesystem operation; checked retained
membership is also required. No silent pruning of output already promised final.

## 5. Reference accounting and the temporary-record decision

### Required identity semantics

The [record checks](../../../../../crates/layerfs-content/src/tree/inode/record.rs#L53)
require the root to be a directory with count zero, regular files to have at least
one reference, and non-root directories/symlinks exactly one. Existing counts
include aliases outside the changed paths; touched names are not the whole count.
These are final record invariants. Provisional addition/removal counts stay private
until reduction completes; they are not emitted as finalized records.

For new inodes, derive initial counts from checked final retained bindings and
exclude their additions from subsequent effects, preserving the existing
[optimization](../../../../../crates/layerfs-workspace/src/changes.rs#L833).
Do not initialize every new inode at zero and journal every alias merely to unify
the code. The derived count is an internal result, not an unchecked caller assertion.

### Why ordering remains real work

```text
directory A changes -> inode 90, 4, 300
directory B changes -> inode 4, 90, 8
directory C changes -> inode 300, 4
                            |
                    combine effects by inode
```

Name order does not give inode order. A later directory removal can generate
further descendant effects. Exact accounting needs unresolved state, external
ordering, or additional passes/lookups. Moving this cost to the caller does not
eliminate it.

Decision: retain a narrowly scoped bounded inode-effects reducer with explicitly
owned compact record backing where needed. Reuse the existing edge journal and
tiered merge algorithm initially; shrink semantic records and remove redundant
passes. Do not replace them with an unbounded map, repeated whole-tree rewrites
or a general canonical-object scratch store.

```text
sorted directory merge observes old/new bindings
                       |
             compact reference events
                       |
             complete additions first
                       |
             removals / zero-count release
                       |
          bounded current inode values + ordered runs
                       |
             sorted final typed records
                       |
                  inode-table COW
```

The reference [edge journal](../../../../../crates/layerfs-workspace/src/changes.rs#L59)
stores 65-byte before/after records and replays additions then removals. Its
[tiered inode runs](../../../../../crates/layerfs-workspace/src/changes.rs#L2224)
already avoid repeated rewriting of the full accumulated prefix. Preserve bounded
merge buffers and that work bound; these are useful algorithms, not arbitrary bloat.

Backing selection, memory/disk allowances and cleanup belong to the operation's
declared resources, with no Workspace NodeId, spool pathname convention or mount
dependency in the logical contract. Record ordering is a planned algorithm with
explicit capacity/error behavior, not a hidden failure fallback. An environment
must supply the required capability or an explicitly qualified alternative input
arrangement; unsupported resources fail explicitly. Qualify its actual local or
remote costs. No new distributed scratch protocol is prescribed.

### Add before releasing, and preserve unseen aliases

```text
move: /old/name -> X       becomes       /new/name -> X

account +1 before -1 -> X never reaches a spurious zero count
```

Preserve [additions before release](../../../../../crates/layerfs-workspace/src/changes.rs#L3125).
When a directory really reaches zero, traverse its entries in bounded pages and
release descendant references, stopping where aliases retain an inode. Latest
pending records override base-prefetched records. Removing a directory can require
work proportional to the released subtree; unrelated subtrees remain untouched.

This updates membership of the new filesystem root. It does not delete canonical
objects used by retained versions; C2 failed-attempt cleanup is a separate concern.
Also, canonical graph reachability is insufficient to find orphan inode records:

```text
filesystem root -> inode table -> orphan record -> content
                                 ^ canonically referenced,
                                   but no filesystem name reaches it
```

Keep semantic reference accounting. Do not replace it with a final object-graph
walk. The [current caller](../../../../../crates/layerfs-workspace/src/changes.rs#L752)
already skips known disconnected dirty inodes; preserve that behavior through the
neutral input contract. Whole-operation zero-temporary-storage is not established.

## 6. Attributes and physical metadata are different responsibilities

### C1: logical attributes

**Owner scope decision, 2026-09-17:** remove Apple-specific metadata from the
replacement core. APFS materialization is outside v0.1.7. C1/C2 contain no Apple
ACL codec, BSD flag interpretation, platform xattr whitelist or native filesystem
metadata calls. Platform metadata interpretation/enforcement belongs to a future
adapter only when that adapter actually needs it; no adapter is added here.

Keep portable mode and mtime with their checked value grammar. Do not add implied
uid/gid/atime semantics. Keep one generic attribute tree:

```text
attribute key: domain + key bytes -> extent-only value root
                       |
             +---------+--------------------+
             |                              |
       portable mode / mtime          other domains
       checked typed values           opaque values
```

Validate framing, ordering, lengths, references and resource limits for all entries.
Retain the existing structural bounds (nonempty UTF-8 domain <=64 bytes, key <=255
bytes, neither containing NUL), with the reserved portable domain limited to its
mode/mtime keys. Other domains use that generic grammar without an OS whitelist;
their values are data, not permissions C1/C2 enforce. There is no special handling
based on an Apple name. Bounded generic read/set/remove/patch is sufficient.

The [reference validator](../../../../../crates/layerfs-content/src/tree/metadata/portable.rs#L53)
has a platform-specific whitelist. Removing it is an intentional acceptance-contract
change, not unchanged platform support. Record the accepted profile in Stage 5;
compare canonical bytes/partitions with v0.1.6 for common supported inputs and
verify new generic-domain behavior separately. Do not claim preservation of the
reference's platform-specific validation or access-control enforcement.

Patches preserve untouched keys and value roots opaquely, without decoding those
values or stripping them. Existing platform-labelled values, when structurally
accepted, follow the same generic rule as any other opaque data. An unsupported
required profile fails explicitly; no silent conversion, automatic migration or
compatibility fallback is added. Values use existing extent-only ropes; the
regular-file small/large cutoff must not reclassify metadata values.

Keep the streaming MetadataTreeBuilder, but stop cloning/encoding a candidate
page on every push merely to test fit:

```text
CURRENT                                  TARGET
append key/value                         append key/value
clone entries                            checked exact size accounting
encode candidate just to test size       keep unresolved tail
discard bytes, repeat                    final partition established
encode final node                        encode once -> emit
```

Use shared checked size/invariant logic with the encoder so partition boundaries
remain identical. Unexpected encoding errors propagate rather than being treated
as ordinary page-full results. Replace decode/re-encode validation only after all
ordering, reserved-field, size and summary invariants are explicitly covered.

### C2: physical inode-value pooling

```text
canonical compact inode leaf
             |
C2: reuse/store 73-byte inode values and assign ordinals
             |
       +-----+-------------------------+
       |                               |
pooled leaf representation       new value groups
FULL / eligible DELTA            FULL / group compression
       |                               |
       +---------------+---------------+
                       |
                  packs / SQLite
```

The table called metadata_value_groups catalogues ordinal ranges, pack/group
locations and decoded-body digests. Actual compact inode values live in pack
groups; the catalogue is not a separate payload BLOB table or xattr store.
Keep physical pooling, ordinals and expansion in C2. Its
metadata delta bounds are distinct from whole-file/chunk payload limits 8/4.
Preserve shared bounded decoded-value reuse and per-chain checks.
Pooling [precedes delta selection](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L929);
the [delta matcher uses the pooled target](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1805).
Reconstruction preserves the original canonical leaf identity.

The existing [physical value index](../../../../../crates/layerfs-layerstack-store/src/objects/metadata.rs#L209)
uses a private SQLite fingerprint index with its own entry/disk/cache bounds.
It is not a C1 candidate payload spool, and removing it is not part of this
component's claimed cuts. The [C2 design](physical-encoding-and-packing.md#replace-the-disposable-sql-fingerprint-index)
selects a bounded Store-owned BTreeSet preserving the same window, authenticated
comparison and chosen ordinals. Its resource/speed proof remains separate; no
temporary-storage-free C2 or cloud proof follows from C1 wrapper removal.

## 7. Read paths and operation-local reuse

```text
checked filesystem root fields
            |
path / inode-key / attribute-key demands
            |
bounded shared tree traversal
            |
authenticated records and requested values
```

Reuse validated canonical names/paths, bounded listing and the existing grouped
directory reader. Make compact inode batches share ancestor acquisition instead
of cloning the root and descending independently for every key. Preserve order,
duplicate demands, child summaries and count/byte limits. Loading a filesystem
root does not load its entire inode table.

Read mode and mtime with shared metadata paths and bounded value acquisition,
using their fixed-size destinations where possible. Carry checked attribute values
within the operation; avoid constructing metadata and reading it back solely to
recover them. Preserve the existing small exact construction-local metadata cache;
do not add a global cache. Previously unknown storage bytes still require checks.

## 8. Source-backed cut list

| Cut | Source | Replacement and qualification |
| --- | --- | --- |
| Repeated per-name/per-inode mutations plus deferred structural graph processing on final-state paths | [directory loop](../../../../../crates/layerfs-content/src/filesystem/apply.rs#L518), [inode loop](../../../../../crates/layerfs-content/src/filesystem/apply.rs#L340) | One sorted merge over affected tree regions using the existing engine; preserve final partitions and required checks |
| DeferredDirectory around an already sorted final-only initial build; provisional empty seed | [initial build](../../../../../crates/layerfs-content/src/filesystem/apply.rs#L475) | Reuse optional-base-root construction; emit actual empty output only when it is the final result |
| Input-size route guessing and sorted-attempt -> rewind -> point-mutation fallback | [preview](../../../../../crates/layerfs-content/src/filesystem/apply.rs#L432), [directory retry](../../../../../crates/layerfs-workspace/src/changes.rs#L959), [inode retry](../../../../../crates/layerfs-workspace/src/changes.rs#L3036) | One declared qualified route; invalid input/errors propagate. Preserve supported budgets/workloads before removing retries |
| Compact-profile record encode -> temporary object -> journal ID rewrite -> read/decode | [finalization](../../../../../crates/layerfs-workspace/src/changes.rs#L2946), [typed API](../../../../../crates/layerfs-content/src/tree/batch.rs#L1153) | Feed sorted typed records/tombstones directly into inline leaves; keep necessary noncompact record encoding |
| Workspace checkpoint fields and appended record IDs in reference ordering rows | [192-byte rows](../../../../../crates/layerfs-workspace/src/changes.rs#L2425) | Extract only inode-state/effect fields; caller owns checkpoint receipts. Final compact row layout remains to qualify |
| Synthetic legacy record allocation/hash used only by generic leaf plumbing | [inode_value_id](../../../../../crates/layerfs-content/src/tree/batch.rs#L1147) | Typed compact leaf path needs no synthetic object ID; keep real page IDs, origin hints and observer semantics. Prove all consumers before removing it |
| Root/profile rereads to recover known count/level or dispatch | [directory wrapper](../../../../../crates/layerfs-content/src/tree/batch.rs#L1364), [new-root reread](../../../../../crates/layerfs-content/src/tree/batch.rs#L1404) | Return known summaries and carry the checked profile |
| Compact inode batch root clones and repeated point descent; envelope reconstruction | [inode reads](../../../../../crates/layerfs-content/src/tree/inode/table.rs#L75), [payload rewrap](../../../../../crates/layerfs-content/src/tree/inode/table.rs#L101) | Bounded shared traversal and checked payload decoding; no global cache |
| Metadata trial encodings/clones solely for fit and tail rebalance | [leaf fit](../../../../../crates/layerfs-content/src/tree/metadata/tree.rs#L83), [branch fit](../../../../../crates/layerfs-content/src/tree/metadata/tree.rs#L178) | Exact checked sizing, identical partitions, final encode once |
| Decode then re-encode nodes solely to validate canonical structure | [metadata](../../../../../crates/layerfs-content/src/tree/metadata/codec.rs#L149), [compact inode](../../../../../crates/layerfs-content/src/tree/compact.rs#L406) | Checked decoding with all encoder-enforced invariants preserved; use fixture/malformed-input proofs |
| Separate portable-key traversals and fresh metadata rereads in the bridge | [portable reads](../../../../../crates/layerfs-workspace/src/cow_tree.rs#L540), [constructed metadata readback](../../../../../crates/layerfs-workspace/src/changes.rs#L226) | Neutral typed bounded attribute read/result, no Workspace types; preserve exact small-cache reuse |
| Physical metadata-group encode -> clone -> decompress merely to hash original body | [C2 preparation](../../../../../crates/layerfs-layerstack-store/src/objects/admission/metadata_values.rs#L112) | Compute the same body digest during original encoding; preserve physical framing and selection |
| Reconciliation-only metadata helpers and unnecessary full-collection convenience helpers | [collection helper](../../../../../crates/layerfs-content/src/tree/metadata/tree.rs#L199), [reconciliation](../../../../../crates/layerfs-content/src/tree/metadata/tree.rs#L442) | Keep bounded visitors/enumeration and ordinary equality; omit deferred reconciliation, move test-only collection into external tests after caller audit |

These are source-visible removal candidates, not proof that every old path pays
every cost. Existing compact inline records, sorted final emission, grouped directory
reads, bounded caches and tiered ordering are reused optimizations. Source locations
in Workspace identify semantics/bridges to extract, not a prescribed replacement
Workspace model. Detailed storage-index changes remain C2 decisions.

## 9. Memory, complexity and independent measurement

Count simultaneous owners: checked input/topology state, current file results,
active directory/inode pages, attribute buffers, edge-event buffers, inode pending
map/run readers, subtree-release cursors, C2 batches/codecs/packs, DB/adapter state
and bounded timer output. Ordering disk and OS-cache residency count too. No entire
inode table, all-file result map or all-candidate payload collection is justified.

Sorted tree state has a separate internal reservation; decoded pages, caller input
and the reference reducer have additional costs. Preserve reserve-before-growth,
page/depth limits and bounded read waves. Required sibling validation can acquire
more than the single changed path. Subtree deletion must inspect released entries,
and full initialization must consume all supplied entries. No universal O(changes)
or constant-RSS claim is established by these diagrams.

Independent scopes use the same production bodies:

```text
filesystem_tree.update
  checked input / supplied authorized identities
  attributes.read_or_build
  directories.apply
  references.reduce
  inodes.apply
  root.encode

storage: admission / metadata encoding / packing / persistence
```

Timer labels are illustrative. Use existing injected coarse scopes/bounded waves,
not a trace node per inode. Pure tree diagnostics can supply sorted bindings and inode changes and a
bounded consumer; full operations pay required normalization, identity reservation,
ordering, source/storage reads and backpressure. Storage calls may nest/overlap
construction; do not add durations into fictional CPU time. The init_namespace
benchmark remains a broader workload and keeps its declared worker exception.

Expected gains are fewer page acquisitions, allocations, temporary encodings,
synthetic hashes, graph traversals and record rewrite passes. Most cuts reduce
constants and actual I/O around retained algorithms; source review proves neither
latency improvement nor a lower asymptotic bound for every workload.

## 10. Qualification and next work

Prove independence through native entry points: C1 with supplied immutable roots, bindings and inode changes,
authorized identities, explicit ordering resources and bounded output; C2 with
supplied canonical objects and real storage, without a Workspace or filesystem
workflow. Compare canonical results under equivalent local and adapter-supplied
inputs when those adapters are implemented. Check imports and public signatures
for leaked Workspace/checkpoint/transport types. These are required proofs, not
completed extraction or remote-provider qualification claims.

Preserve exact roots under matching profile, scope, identities and normalized
changes; name/order validation, checked record/reference invariants and unchanged
old roots remain required. Existing evidence includes
[sorted final-only pages](../../../../../crates/layerfs-content/src/tree/batch.rs#L2037),
[canonical partitions](../../../../../crates/layerfs-content/src/tree/batch.rs#L1776),
[moved child and unseen aliases](../../../../../crates/layerfs-workspace/src/changes.rs#L4061),
[unrelated subtree avoidance](../../../../../crates/layerfs-workspace/src/changes.rs#L4408)
and [cross-run updates](../../../../../crates/layerfs-workspace/src/changes.rs#L5628).
Extract needed proofs outside replacement product source; do not import old
implementation files or preserve hidden fallback routes.

Cover empty/wide/deep trees, create/rename/remove, alias updates across batches,
new-inode counts, child moved out of deleted subtree, disconnected dirty records,
cycle/name-collision rejection, metadata patches preserving untouched domains,
large attribute values, supported profiles, tiny supported resource budgets,
slow output and failure during record merge or partial admission.

Freeze matched v0.1.6 comparisons under the [I/O qualification contract](content-io.md#7-measurement-and-completion).
Require existing-or-better complete-operation performance, final storage efficiency
and resource behavior alongside exact correctness. Count tree reads, encoded/hash/
copied bytes, event/run I/O, physical pool work, final DB/index/pack footprint and
simultaneous memory. Do not credit previously optimized paths with invented cuts,
move ordering outside a timer to claim a saving, or change workers/cache/workloads
to obtain a pass.

First extract sorted directory and direct typed-inode construction with the real
handoff, then reference reduction and attribute improvements. Open proofs are
checked final-plan/topology integration, semantic-only ordering record layout,
exact format/decoder equivalence, supported capacity and full-operation measured
gains. C2 physical index/pack/session proposals are recorded in the
[encoding](physical-encoding-and-packing.md) and [persistence](admission-and-persistence.md)
documents; concrete compatibility/resource qualification remains.
Live Workspace COW stays later; no new crate or public API is frozen here.

### Remediation note (2026-09-17): what an operation owns and what it charges

The first Stage 5 implementation reported its ordering work from the merge path
alone. An independent review measured the record-backed path at 3.65x-3.83x per
doubling (quadratic) while the reported `rows_read` grew linearly, and found that
the declared `ordering_bytes` ceiling did not bound the bytes the operation held
while a spill merged its inputs into an output. Both are corrected here, and the
contract they follow is stated rather than implied.

- **Lookup work is charged.** The reducer's run store keeps each tier's scan
  position, so an ascending sweep reads a run once instead of re-reading it per
  serial, and the rows it reads are added to `MergeWork::rows_read`. A request
  below the scan position restarts that tier, because a scan that resumed past a
  row never compared it; correctness before speed.
- **The ceiling covers every simultaneous owner.** A spill's older inputs stay
  charged to the operation until the handle that owns their bytes is dropped, and
  the merge output is reserved before it is written. `RunStore::owned_bytes`
  reports live runs, unmerged inputs, the pending rows and the reserved output
  together, and a backing whose own ceiling cannot hold the declared ceiling is
  refused once at operation entry.
- **The touched-serial vector is bounded.** It is one `u64` per touched inode,
  taken from the same declared ordering budget:
  `FilesystemResources::maximum_touched_serials()` is `ordering_bytes / 8`, and an
  operation whose touched set does not fit is refused with `ObjectLimitExceeded`
  instead of allocating past its own ceiling. A bounded collection that the
  caller declared is the contract; an unbounded one was not.
- **Validation is charged.** The checks read the base they are about to change:
  parent records, the bindings of the directories they walk and the entries of
  every directory they inspect for a cycle. Those reads and pages are reported in
  `FilesystemUpdateCounters::validation` (`ValidationWork`), because "repeated
  reads ... remain charged where they occur" is a requirement of this document,
  not an option.
- **A read wave is bounded.** One authenticated wave is one grouped query and one
  decode workspace, so its size is a declared capacity on both sides of the seam:
  `StorageCapacities::read_objects` for the Store and
  `MAXIMUM_READ_DEMANDS` for the filesystem boundary, both 4,096. A longer slice
  is refused before a connection is opened, and a batch lookup
  (`Store::contains`) applies the same ceiling rather than paging whatever it is
  handed.
- **The release frontier reads in waves.** The traversal that releases a
  zero-count subtree used to ask for one inode record per serial it popped, after
  the page wave that had just read that record, so a released subtree paid the
  record twice. The starting serials are read in `base_batch`-sized waves and a
  child that reaches zero keeps the record its own page wave already read; the
  frontier therefore adds no read of its own. The retained records are bounded by
  the same ordering budget as the touched set, and
  `filesystem_bounds::the_release_frontier_reads_one_wave_per_batch_not_one_record_per_demand`
  pins it (193 records for 128 released inodes before, 129 after).
