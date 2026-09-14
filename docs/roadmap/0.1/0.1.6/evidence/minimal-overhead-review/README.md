# Minimal-overhead Workspace review

Status: source-backed proposal, 2026-09-14. Not an implementation, performance
qualification, amendment of the frozen contracts, or completion of #124/#125.

Scheduling update: [#130](https://github.com/Ephemeral-AI-Lab/layerfs/issues/130)
is promoted into the active migration, before final capacity qualification and
benchmarks. The owner's full 25,000-file lifecycle objective is <=2 seconds, with
no quadratic scaling. The [current implementation plan](../../overlay-minimal-overhead-implementation-plan.md)
supersedes this review's earlier order and selects the initial representation;
the remaining analysis below is retained context, not a mandatory pager/feature
bundle. #124/#125 retain all correctness, capacity and benchmark obligations.

The owner requires a fresh Workspace to be affordable for each agent tool call.
Reusing a mutable Workspace and resetting it is not a substitute for that
requirement. This review modifies no product source, runs no product tests or
benchmarks, and preserves the other task's active capacity edits and evidence.

## Decision

Optimize the common fresh-Workspace path first: a shared immutable canonical
base plus a small, lazy, snapshot-capable delta. Reuse existing Store, daemon,
container and runtime services while retaining a new Workspace identity, lease,
replay scope, FUSE session and handle ownership. Do not add a new pool of mutable
Workspaces, a new canonical encoder, a per-call SQLite database, or a second
small/large Workspace implementation.

The earlier 180–210 MiB estimate retained most of the current record inventory.
It is withdrawn as a recommendation: it was neither a minimum imposed by
snapshot isolation nor an acceptable fixed cost for a tool call. Total base
file count N must be distinguished from changed files D, touched bindings B,
live pieces P, replacement bytes U and separately retained old state R.

The immediate plan preserves the existing immutable-root and publication
contracts. More aggressive removal of dual change indexes is a separately
identified specification revision, not a silent implementation shortcut.

## Evidence and attribution

- Baseline: released v0.1.5 at
  `6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13`, specifically
  `release-notes/0.1.5/benchmark-closeout.md` and its performance CSV.
- Current code checkpoint: `3f04d4146ebe508a240076e4540de5123b9142d1` plus the
  explicitly recorded active working-tree changes. See `source-manifest.json`;
  it is a read-identity record, not an atomic candidate seal.
- `baseline-selection.json` retains nine exact historical rows, producing
  binary identities, raw receipt paths, original statuses and timer names.
- `current-capacity-arithmetic.json` reproduces source-derived reservations.
  Reservation is not measured RSS, physical allocation or elapsed time.
- `extract_baseline.py` regenerates those analysis files without running a
  product workload. It verifies release-row uniqueness and that its selected
  historical cases do not appear in the #122 exclusion manifest.
- Independent reviews: [overlay representation](overlay-analysis.md),
  [Commit construction and receipts](commit-analysis.md), and
  [fresh lifecycle and supported-surface constraints](lifecycle-review.md).

The public default lifecycle still uses the legacy path at this checkpoint.
HostRuntime/CommitCoordinator are component entrypoints. A green existing
benchmark on this HEAD does not establish new snapshot-path performance unless
the receipt proves actual dispatch into the new implementation.

### Source advanced during review

A parallel implementation task published two relevant local commits while this
read-only review was running. They are not work performed by this review:

- `ca46e2793f1a8c41260e97f3b04d10edaf2612fc` separates resident handles from
  persisted tokens and fixes payload-index reclamation/progress. Its retained
  `evidence/step1-capacity/README.md` records the 8,193-file reproduction PASS,
  payload-index physical allocation 71,593,984 B and payload arena allocation
  33,611,776 B. The earlier ~16-page/file growth is historical failed behavior;
  the corrected diagnostic is about two payload-index pages/file. The original
  failure remains preserved. This is component evidence, not a public benchmark.
- `ff7098929eda07d91fbfc2d1072b286231203325` correctly charges the ordinary
  SnapshotReader's 8-MiB demand-cache allowance. The fixed host reservation is
  now 14,234,624 B (13.575 MiB), not the original 5.575 MiB. The 128-MiB host cap
  therefore fits nine such full fixed reservations before construction/extra
  activity; the earlier 17 count concerned disk quota alone. Optimization must
  reduce/share actual cache demand safely, not remove this accounting correction.

Active uncommitted scratch-placement changes were also observed in Store scratch/
spill, candidate_capacity and snapshot_candidate. They preserve the reviewed
96-MiB construction envelope at this point; no correctness result is inferred
from their presence. `source-manifest.json` and `current-capacity-arithmetic.json`
retain the original read's identities/arithmetic; they must not be mistaken for
the newer source. See `followup-source-manifest.json` for the later read.
At the final status read, scratch placement had been committed separately as
`737592356485ab4384e2339a81b9d923fa1fede0`. This review did not perform or verify
that implementation; all proposal findings retain their stated source scope.

## Concrete avoidable costs

| Finding | Consequence | First change |
|---|---|---|
| Initial HostRuntime reserves 5,846,016 bytes of memory capacity, 7,784,693,760 bytes of disk quota and eight FD slots; the subsequent reader-cache correction makes memory 14,234,624 bytes | Fixed entitlement reservations constrain concurrency even when actual usage is small; reservations are not physical usage | Reserve actual admitted work and retained ownership plus bounded recovery headroom, rather than every Workspace's maximum entitlement |
| Six backing descriptors are opened eagerly | Per-call setup/cleanup even for an unchanged Workspace | Open private backing only when data or evicted metadata actually needs it |
| Canonical reads can promote records into the mutable index | A read-only traversal can create unnecessary overlay state | Direct immutable-base reads with stable identity and bounded handle/cache bookkeeping; mutation performs promotion |
| Leaf limit is seven records, branch limit eight children | Tiny records have poor page occupancy and deep paths | Byte-aware compact pages and bounded decoded-byte limits |
| One dedicated 4-KiB index leaf per range node | 100k single-range files need 390.625 MiB of range page bodies alone | Inline the simple range shape; pack complex range nodes together |
| A singleton published description owns offset, origin and header trees | Three additional 4-KiB pages per represented file | Store the singleton description directly in the existing correspondence row |
| A one-byte arena allocation rounds to 4 KiB and creates six ownership-index rows | Payload space and catalog work dominate tiny edits | Tagged small owned payload values, preserving ordinary-write charging; packed arena allocation for larger payloads |
| Related index changes are applied as separate COW updates | Immediately obsolete intermediate paths and reference-count traffic | One bounded prepared mutation for the affected records/pages |
| Snapshot candidate always enters construction admission | Clean input can claim 96 MiB/128 FD slots and initialize unnecessary scratch | Prove captured input is unchanged, then stage/publish the known canonical root through the same V4 protocol |
| CapturedChanges rebuilds a derived tree before construction | Extra change-key indexing per Commit | Reuse an existing bounded journal/cursor where ordering allows; retain necessary parent/refcount grouping |

The in-process daemon mount route already exists. Selecting it avoids the Docker
CLI/helper-launch fallback while still creating a new FUSE session. Cold service
startup remains a different measurement from the warm-service per-call path.

## Recommended lifecycle

1. **Begin:** acquire the existing branch lease and immutable base context;
   create only the new identity/root/replay/handle scope. Use existing shared
   services. Do not copy the namespace or preclaim the entire index quota.
2. **Read:** resolve unchanged canonical data directly. Cache entries and active
   kernel references are charged and bounded; untouched base files need no
   mutable records. Compact canonical inode identities are reusable only where
   scope and collision rules prove it; legacy identities need a valid adapter.
3. **Mutate:** reserve the actual prepared peak and cleanup headroom. Construct
   host-owned replacement bytes and immutable affected metadata before atomic
   source-root installation. Keep a single owned read interface for small
   buffered values and spilled data; never reclassify FUSE writes as SDK-inline.
4. **Capture:** retain the root and existing ownership without an index walk,
   cache flush, whole-map clone or deferred whole-map copy on the next write.
   The generic V1 kernel visibility obligation still applies.
5. **Construct:** when the captured state is proven unchanged relative to the
   canonical predecessor, bypass encoding/planning but retain exact expected
   head/base checks, stage/outcome ownership and publication receipts. Otherwise
   stream relevant changed records into the existing shared builder. Allocate
   journals/index spill only as their actual bounded workload requires.
6. **Publish:** retain the exact candidate and use current conditional
   publication. Update comparison/coverage, not live file contents. Store the
   latest compact predecessor description; never retain a temporary per-Commit
   history just to perform C2 matching.
7. **End:** verify consumer retirement and release the Workspace's owners.
   Existing readers/attempts retain only what they legitimately own. Closing the
   final private backing owner should not require materializing every disk row.
   Cleanup failures retain both ownership and accurate charges.

## Memory and spill are one representation

Use immutable compact pages with stable identities and a bounded host-owned
cache. Root ownership refers to the graph, not to an Arc for every resident page.
Pages and small payloads may remain in charged memory while small; eviction writes
individual already-owned pages/records and changes only their physical location.
Reads through an old snapshot keep the same logical identity before and after
eviction. Failed eviction leaves the authoritative resident bytes and charge
intact. Admission must reserve the eviction/publication/recovery peak first.

This is a pager/location change within one overlay, not a growing HashMap that
gets serialized at Commit or converted wholesale when a threshold is crossed.
There must be no unbounded PageId-to-location RAM map, epoch-wide retention of
intermediate edits, or synchronous whole-overlay drain. Small and spilled cases
must use the same mutation, snapshot, replay and canonical-construction semantics.

An implementation proof is required for cache eviction versus snapshot/read
leases, disk reference counts, partial I/O, physical relocation and End. The
diagrammatic existence of a cache is not that proof.

### Two adversarial corrections

Small ordinary bytes must remain an ordinary `Source::Payload` with
`inline_charge=false`, even when physically embedded in an owned token. They are
not SDK-inline input. A stream of 17-byte appends must not create a new lineage
and range piece every time a 64-byte storage threshold is crossed. A bounded
small-to-arena promotion must preserve origin coordinates, append continuity,
old readers and exact charging under failed I/O. Until that promotion is proved,
do not claim the small-token storage estimate for general append workloads.

Direct canonical reads can avoid full inode/range promotion, but a read-only
100k-file traversal can still cause the kernel to retain 100k lookup references
or directory-cookie entries. Those need compact bounded/disk-backed ownership;
zero mutable-file promotion is not a promise of zero handle bookkeeping.
Mapping `NodeId = canonical serial` blindly is unsafe: the root exception,
namespace scope, allocation domains and legacy 32-byte identities need explicit
handling. Never truncate/hash a canonical ID into an assumed collision-free ID.

## Sparse upper architecture and fast scans

This is a proposed sparse, snapshot-capable upper layer. It has upper-layer
lookup/override/deletion behavior, but its private backing need not be a native
directory tree or a separate host file for every logical file. Canonical lower
data remains shared; current upper records describe effective state, not an
operation log. Root/record ownership preserves old versions only while needed.

```text
                  Commands / FUSE / SDK
                           |
                 Host Workspace engine
                 fresh identity + lease
                 atomic current root
                           |
              +------------+-------------+
              |                          |
     Immutable canonical lower    Sparse current upper
     shared Store files/metadata  inode and binding overrides
                                  deletions, ranges, changes
                                         |
                                Owned compact backing
                                charged resident buffers
                                incremental private spill

     Commit: retain root -> snapshot reader -> existing builder
                         -> exact stage -> conditional publication
```

Small edits retain lower references and store replacement ranges, rather than
copying the whole file. A one-range file has a compact descriptor; fragmented
files use packed indexed ranges. Snapshot registration retains an already owned
root; the next mutation copies only affected paths, never the whole upper.

```text
After capture, before another write:     After a write affecting B:

Live ----+                              Live -> R8 -> B-new
         +-> R7 -> A, B                          |
Snapshot-+                                      +---- A ----+
                                                           |
                                        Snapshot -> R7 ----+
                                                      \-> B-old
```

Generic kernel-visible capture remains a separate V1 obligation. A host root
alone does not certify that dirty mmap bytes are included. None of the following
scan optimizations changes that requirement or promises measured latency.

### Directory enumeration: merge two sorted streams

For a selected directory, seek to its lower entries and its upper binding prefix.
Stream both by the same canonical name ordering. Upper entries override equal
lower names; deletion markers suppress the name; lower-only entries pass through.

```text
Lower entries                 Upper entries
-------------                 -------------
a.txt -> lower                b.txt -> modified
b.txt -> lower                c.txt -> deleted
c.txt -> lower                e.txt -> new
d.txt -> lower
          \                   /
           +-- ordered merge-+
                      |
                      v
          a.txt -> lower
          b.txt -> modified
          d.txt -> lower
          e.txt -> new
```

With L lower entries and K upper entries in that directory, full enumeration
does O(L + K) stream work after the initial index seeks. It does not point-lookup
every lower name in the upper, materialize the combined directory, or copy all
observed lower files into mutable inode/range trees. Traversal of many directories
accounts for each directory seek; this is not a claim that a full filesystem
scan performs only two seeks.

Use bounded cursors over packed leaves, returning batches through readdir or
readdirplus. Keep only admitted traversal/page/response buffers, not the entire
directory. A cursor advances through leaves with its bounded traversal state
rather than restarting a root-to-leaf lookup for every entry. Immutable page
caching and read-ahead may reduce I/O under their actual memory and ownership
limits. Do not promise sequential physical reads merely because keys are sorted:
physical locality and read amplification must also be observed.

For readdirplus, attribute resolution must likewise use bounded batches and
cached/packed inode pages where possible; directory names and inode values may
be different indexes. Count residual inode lookups and host RPCs rather than
hiding an expensive per-entry attribute path. Emitting entries and acquiring
kernel lookup references must preserve the existing atomicity, cancellation,
FORGET and unlink behavior.

### Commit enumeration: seek to effective changes after coverage

The initial implementation can use the existing latest-change indexes. Seek
after the captured predecessor's covered sequence and consume only qualifying
keys from the captured root. Repeated updates replace the superseded change
entry; they do not append an unbounded operation history.

```text
C1 covered through sequence 42

Captured latest-change index:
    57 -> inode A changed
    58 -> old binding removed
    58 -> new binding installed

C2: seek after 42 -> changed keys -> captured records
                  -> affected content and namespace construction
```

Required ancestor updates, link accounting, range traversal and genuinely large
namespace operations still contribute work. This is not a universal O(number
of files edited) claim. Covered tracking can be retired without removing lower
deletion masks or newer effects. Replacing the mandated dual indexes with
primary-row/subtree sequence summaries remains the explicit specification
revision identified in this proposal, not an assumed prerequisite for streaming
scans.

### Snapshot consistency, live cursors and content scans

A snapshot scan merges the lower reference and upper root belonging to the same
captured view, including across cursor batches, eviction and relocation. Hold
the required root/backing leases without pinning every page body in memory.
Ordinary live readdir must keep its existing numeric-cookie/resume and mutation
semantics; do not silently convert it into an atomic whole-filesystem snapshot.

A full name scan is at least proportional to the entries returned. A scan of
all file contents must read the requested visible bytes. Stream file ranges
directly from canonical backing and owned upper replacements; do not reconstruct
each file into a temporary copy before searching it. Reading and authentication,
fragmentation, cache misses and RPC work remain real costs.

### Scan checks and reporting

- Exact merge results for upper-only names, replacements, deletion masks,
  duplicate names, empty directories and batch boundaries. Deletion-only batches
  must still advance the cursor without falsely reporting end-of-directory.
- Snapshot scan remains exact across concurrent writes, rename/unlink, eviction
  and relocation. Live cookie resume, hardlinks and open-unlinked lifetime retain
  their existing contracts; quota/I/O/cancellation failure releases or retains
  the correct cursor and kernel-reference ownership.
- A large C1 followed by a small C2 enumerates its relevant changed frontier,
  including removals, while unrelated canonical subtrees remain reused.
- Measure entries examined/returned/suppressed, tree/page reads, inode lookups,
  host RPCs and batch sizes, cache behavior, overlay promotions/rows created,
  cursor memory, bytes read/written, physical allocation and retained references.
  Report names-only, attributes and content scans separately.
- Use applicable existing directory, namespace, Workspace-locality and read
  benchmarks. Add focused missing correctness/cost checks only where justified;
  no replacement framework, new numerical gate or #122 scenario is introduced.

## Continued operation during and after Commit

The live Workspace and a Commit attempt own separate views. These correctness
requirements already belong to #124 and the earlier seven steps; the promoted
#130 optimization must preserve them. This clarification neither postpones their
verification nor claims they are proved merely by the proposed representation.

### Independent live and captured roots

```text
Before capture:
    Live -> R10 -> a.txt = A, b.txt = B

Capture S1:
    Live ------+
               +-> R10 -> a.txt = A, b.txt = B
    Snapshot --+

While S1 builds, the application writes A2 and renames b.txt:
    Live -> R12 -> a.txt = A2, c.txt = B
    S1   -> R10 -> a.txt = A,  b.txt = B

After C1 publishes:
    Branch: C0 -> C1          C1 contains a.txt = A,  b.txt = B
    Live:         R12        Live still a.txt = A2, c.txt = B

Next capture:
    R12 -> S2 -> C2           C2 contains a.txt = A2, c.txt = B
```

S1 retains the old namespace and bytes it needs. Ordinary writes prepare new
owned replacement bytes and affected immutable metadata paths; they do not
overwrite S1's backing. Unchanged pages/ranges remain shared. Rename/link/unlink
must install their related effects atomically, so capture cannot observe a
half-applied namespace operation. Root retention must not pin all page bodies
in RAM or defer a whole-overlay copy to the next write.

### Descriptors continue to follow live inodes

```text
Application descriptor -> stable live inode ID -> live operation's version
Commit reader          -> captured root        -> captured inode/version
```

Ordinary operations use the current version appropriate to their existing
ordering contract. Rename changes a binding, not inode identity or an open
descriptor's offset. Open-unlinked content remains available to its handles
without being resurrected in committed namespace output. A retained snapshot
reader always resolves through its captured view, never substitutes newer live
bytes if its old backing is missing.

### Expensive Commit work does not hold live-operation synchronization

```text
Ordinary mutation                    Commit attempt
-----------------                    --------------
Admit actual resources               Acquire valid owned snapshot
Prepare bytes and metadata           Read captured input
Compare leased source root           Construct canonical objects
Briefly install new root             Admit and stage exact candidate
Acknowledge operation                Conditionally publish / resolve
```

Preparation, physical I/O and required cleanup run outside the brief root-install
boundary. If the leased source root changed, stale prepared work cannot overwrite
unrelated changes; revalidate/reprepare under the existing ownership and fairness
contract. The Commit-attempt gate serializes unresolved Commit attempts for that
Workspace, not its reads, writes, namespace operations or SDK edits.

Construction, admission and publication must not retain live-state/lifecycle
locks that exclude ordinary operations. Existing bounded Store publication
transactions and branch leases remain, without a global construction lock across
independent Workspaces. Normal operation ordering and genuine resource contention
are still real; neither may hide a wait-for-Commit freeze.

### Publication advances exact coverage, not live contents

```text
S1 captures effects through sequence 42
While C1 builds, effects 43, 44 and 45 install

After successful C1:
    published coverage = 42
    later effects      = still eligible for C2
    current live root  = unchanged by publication
```

Do not reinstall S1, reset the upper, remount, replace descriptors, globally
clear dirty state or discard later changes. Covered tracking may be retired
only where its exact sequence is covered; deletion masks needed to hide lower
entries remain independently owned. Keep the last canonical predecessor and
metadata-only correspondence separately from live range provenance, so C2 can
reuse C1 without rewriting live contents or rebuilding all changes since Begin.

### Retry and failure belong to the attempt

A retained stage, head conflict or uncertain reply must not mark filesystem
activity inactive. Retry the same candidate and expected context, not a new
candidate rebuilt from later live state. A successful publication whose reply
was lost requires its authoritative Created/UpToDate outcome; stage absence or
equal content is not sufficient evidence. Construction failure, quota failure
or definite abandonment must preserve all acknowledged live operations and
their ownership. An unresolved attempt may delay the next Commit, not turn into
a mandatory stop for ordinary operations.

### Reclamation and canonical backing preserve the live view

Storage becomes reclaimable only after the last relevant root, reader, prepared
operation or attempt releases it. Release snapshot ownership when the stage/
retry contract permits; held readers may continue retaining particular bytes.
Reclaim unrelated superseded versions incrementally rather than retaining every
post-capture operation under one epoch. Failed reclamation retains its charge.

Optional canonical substitution may replace temporary ranges with byte-identical
canonical references, using a leased source and exact correspondence. It must
preserve later writes and stable inode identity, and may defer on a race. Commit
does not require a global checkpoint installation to make the Workspace usable;
the Workspace must have remained usable throughout.

Held snapshots still consume real resources. Prospective admission includes
transient ownership and recovery headroom; exhaustion follows the existing error/
backpressure contract without corrupting data or disguising a global freeze.

### Required continuation checks

- Hold a test builder after capture and prove ordinary reads, writes, rename/
  unlink and SDK edits actually complete before the builder is released.
- Verify C1 excludes later changes and C2 includes their effective results;
  verify namespace atomicity, the same mount/inode/descriptor identities, file
  offsets and open-unlinked behavior before, during and after publication.
- Exercise stale prepared roots, failed construction, head conflict and lost
  Created/UpToDate replies. Verify exact candidate reuse and preserved live
  progress; no later input may leak into the retained attempt.
- Keep readers across capture, spill, relocation, canonical substitution and
  snapshot release. Check correct old/new bytes, bounded independent reclamation
  and charges after failed cleanup.
- Prove the actual supported FUSE boundary, including dirty writable mappings.
  Host generation/root ownership alone does not establish V1. Cheap host-root
  acquisition is not a substitute for that required kernel-visibility proof.

These use and extend the applicable existing correctness oracles; test-only
builder holds are not part of timed performance. No new numerical benchmark
gate, supported-surface waiver or #122 scenario is introduced.

## Specification boundary

The following changes can preserve the current selected contracts: lazy resource
allocation, bounded caching of immutable pages, compact page density, singleton
range/correspondence representations, classified small owned payloads, batched
private preparation and a publication-safe unchanged candidate shortcut.

The following require an explicit internal specification revision and focused
proof before use:

- Replace the mandated dual change indexes with latest-effect sequence stored
  in primary inode/binding/tombstone rows and subtree sequence summaries. A
  captured-root traversal can skip subtrees covered by the last publication,
  but maintenance must stop assuming chronological iteration and deletion
  markers must survive until their effects are covered.
- Embed a cookie in the binding instead of maintaining an additional
  name-to-cookie record; retain the ordered-cookie access needed by readdir.
- Represent aliases as zero/one/many with bounded promotion. An undiscovered
  canonical hardlink must never be incorrectly classified as the only alias.

Exclusive in-place mutation of a published page is explicitly forbidden by the
current spec, even when no snapshot owns it. It is not needed for the initial
recommendation. No V1–V4 semantics, authentication, fsync, FULL/DELTA, CDC,
lease/publication rule, input limit or benchmark criterion is relaxed here.

## Performance comparison and validation

The release report is historical evidence: one public sample per fresh case,
with some explicitly reused rows. Its v0.1.3 comparison has an undeclared cache
profile and is not a paired speedup claim. Match the producing source, binary,
fixture, timer and cache/topology contract when measuring a new implementation.

Prioritize the existing clean-commit, random-read, distributed SDK edit, tiny
create, rename and localized-edit cases. Also preserve namespace Init, large
localized CDC and full C1/C2 cases so a small-call optimization cannot conceal
large-file or successive-Commit regressions. All #122 scenarios remain excluded.

| Historical v0.1.5 case | Begin | Commit | End | Registered boundary |
|---|---:|---:|---:|---|
| `workspace-clean-commit-1-compact-v2` (50 files / 1 MiB) | 6.731 ms | 1.501 ms | 2.167 ms | 10.495 ms public-call sum including visibility |
| `workspace-clean-commit-500-mixed-v4` (5,000 files / 500 MiB) | 13.144 ms | 1.634 ms | 2.929 ms | 17.795 ms public-call sum including visibility |
| `payload-random-read-1-compact-v2` | 10.824 ms | 2.179 ms | 3.174 ms | 20.460 ms public-call sum including 4.188 ms read/exec and visibility |
| `overwrite-head-4k-on-1mib-ops-1` | 11.843 ms | 9.054 ms | 3.071 ms | 10.948 ms edit+Commit; Begin and End excluded from that timer |

For clean1, mount-ready takes 5.407 ms inside Begin's 5.625-ms attach subreceipt,
with zero Docker CLI calls. These clocks are nested, not additive savings.
Component costs suggest where avoiding disk/journal/ownership work can help,
but no current-source elapsed-time forecast follows. Cold process/container
startup and 100k-file fresh no-op timing are unavailable in this selected
historical evidence. Preserve each original row's WARN/FAIL/waiver/reuse status.

The intended cost signature is:

- Begin/no-op over a large unchanged base does not enumerate or index that base.
- One changed file adds bounded data/metadata work based on its actual pieces
  and affected namespace, not total base file count.
- Snapshot retention does not flush the small cache or copy the full overlay.
- Repeated writes to a bounded hot set do not grow storage with operation count
  when obsolete versions have no owner.
- A second Commit after a large C1 constructs the newly relevant files and
  preserves the exact canonical predecessor without rewriting live contents.
- Host and container resource domains, physical disk, retained bytes, cleanup
  backlog and I/O are all observed; reservations are reported separately.

## Storage estimate scope

The main estimate for a per-call Workspace must use the changed/touched set,
not every file in its base. With an untouched 100k-file base, D0 should hold only
its fresh context and base reference; D1/D10 should add a few charged compact
pages/owners, not a 100k-file ownership index. This is an implementation target,
not a measured zero-RSS or zero-total-disk claim. Active kernel references,
directory cookies, service state and canonical caches are separate real costs.

For 100k **new changed** single-range files, the overlay review gives a different
explicit arithmetic model: 16-byte names, one-byte ordinary writes, no held old
snapshot, no kernel pins, no prior canonical correspondence. Keeping the existing
record inventory but packing singleton ranges and pages yields roughly
135–166 MiB of metadata under a stated 65–80% fill assumption. Removing redundant
change/cookie records through the identified spec revision yields roughly
105–129 MiB, still retaining all six general payload-catalog rows. These are
intermediate layout estimates, not the final minimalist target or a capacity
PASS. Current 4-KiB payload rounding is another 391 MiB until tiny/shared-block
ownership is implemented. Commit/predecessor/snapshot/scratch costs are additional
according to their actual overlap and lifetime.

The review explicitly declines to promise a 20–50 MiB total by assigning an
unproved tiny record size to the allocator. Further compact tagged source/alias
records and singleton correspondence must be costed from their completed
encoded layouts, including ownership, free space, class, checksums and retained
versions. Fewer rows, better packing and a cheaper D0/D1 path must be validated
independently; a low steady page count can still hide excessive write traffic.

Use focused component checks while implementing; final full public-path
qualification and the complete included benchmark campaign still follow the
existing plan after correctness/V1 and source custody are established. A focused
diagnostic can count rows/pages/I/O without becoming a new benchmark family or
inventing a numerical acceptance gate.

## Execution order

1. Remove empty/clean-path eager work and oversized static admission claims;
   preserve the real fresh Workspace lifecycle and actual output ownership.
2. Inline singleton correspondence and simple file-range state; make ordinary
   tiny payloads byte-proportional and correctly classified.
3. Implement compact byte-aware pages, one bounded prepared mutation, and the
   shared bounded pager with failure-safe incremental eviction/reclamation.
4. Only if measurements justify the additional change, revise dual change/cookie
   indexes and compact whole-source payload ownership with explicit promotion.
5. Integrate default public dispatch, prove supported-surface capture and C1/C2,
   then compare the sealed candidate under the existing benchmark contracts.

Steps are implementation priorities, not roadmap-phase completion claims.

## Review completion

Three bounded subagents reviewed representation, Commit/publication and lifecycle
independently. Cross-review corrected the whole-source promotion trigger to
include append/join and any second independent token interval, not only partial
slices. Root integrated the correction. Local document links, JSON, selected
baseline identities and exact timer arithmetic were checked; see
`analysis-verification.json`. No new product/benchmark result, issue closure,
phase completion, release or tag is claimed.
