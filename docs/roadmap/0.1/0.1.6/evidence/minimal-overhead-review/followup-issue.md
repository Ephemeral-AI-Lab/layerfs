## Scheduling: implement only after the existing seven steps finish

**Owner-requested deferred follow-up. Blocked by completed #124 and #125.**

Start implementation only after all seven Snapshot-Isolated Workspace execution
steps are actually complete, including required correctness/capacity evidence,
the full existing benchmark campaign excluding #122, evidence publication, and
verified completed closure of both #124 and #125.

This issue must not interrupt, replace, expand, or become an additional completion
gate for that seven-step campaign. It does not permit deferring a mandatory
correctness, capacity, performance, resource or cleanup failure from #124/#125:
those issues must first satisfy their own existing terminal contracts.

Do not execute this optimization in parallel with the earlier steps merely
because its analysis is ready. The present issue records the subsequent work.

## Objective

Make a **fresh Workspace per agent tool call** affordable in latency, host memory,
temporary disk, I/O and cleanup, approaching the documented v0.1.5 cost profile
where the operation and environment are comparable.

Each call must retain a fresh Workspace ID, branch lease, mutable root, replay
scope, FUSE session and handle ownership. Reusing/resetting one mutable Workspace
is not a substitute. Reuse existing immutable Store data and physical services
(Store handle, bound container, in-process daemon and runtime) where their
existing ownership contracts allow it.

The desired cost shape is a shared immutable base plus a small, lazy delta.
Distinguish base file count N from changed files D, touched bindings B, live
pieces P, replacement bytes U and separately retained old state R. A Workspace
over 100,000 existing files that changes one file should not build an ownership
index for every base file.

## Source-backed findings to recheck after the prerequisite campaign

The three-agent review examined `3f04d4146ebe508a240076e4540de5123b9142d1` and
recorded subsequent changes through `737592356485ab4384e2339a81b9d923fa1fede0`.
These are historical review checkpoints, not a claim that the defects or costs
will remain unchanged when this issue starts. Inspect the delivered #124 source
and retained evidence before modifying or rerunning anything.

- The reviewed factory eagerly opened six index/payload backing files and
  preallocated root/release queues even for a small Workspace.
- After `ff7098929` correctly charged the ordinary SnapshotReader cache, the
  fixed memory reservation was 14,234,624 B (13.575 MiB), with 7,784,693,760 B
  of disk quota and eight FD slots reserved per Workspace. Reservations are not
  measured RSS or physical allocation. Keep the cache-accounting correction;
  reduce/share actual demand rather than making it uncharged again.
- Canonical reads could promote immutable files into mutable inode, binding,
  canonical-ID and range records before any edit.
- The generic index allowed only seven leaf records and eight branch children
  per 4-KiB page, even for tiny records.
- A singleton file range occupied a dedicated 4-KiB leaf. A singleton canonical
  predecessor description occupied three additional 4-KiB pages.
- A small ordinary payload used six general ownership/location/coverage rows;
  a one-byte independent arena allocation rounded to 4 KiB.
- Related mutations repeatedly copied index paths and updated disk reference
  metadata. Better packing alone can still leave excessive write traffic.
- The new candidate builder entered a 96-MiB/128-FD construction allowance,
  journals and private object machinery even for unchanged captured input.
  Its reviewed private-index/cache configuration could force first-row SQLite
  spill. Recheck this against subsequent construction-capacity fixes.

Preserve corrections already made: `ca46e2793` repaired resident-token accounting
and payload-index reclamation. Its recorded 8,193-file component rerun passed
with 71,593,984 B of payload-index allocation and 33,611,776 B of arena allocation.
The earlier ~16-pages-per-file growth is historical failed behavior, not the
corrected steady ratio. Neither that component result nor the review proves
final public-path performance.

## Proposed implementation order

1. **Cheap fresh Begin/read/End.** Use the existing in-process daemon mount route;
   avoid per-call helper/container startup where services are already bound.
   Make private backing and resource admission demand-driven with recovery
   headroom. Resolve unchanged canonical files directly and promote only when
   necessary. Preserve stable IDs, hidden hardlink aliases, deletion masks,
   readdir cookies and active kernel reference ownership. Arbitrary legacy
   256-bit inode IDs require a valid bounded mapping; never truncate/hash them
   into assumed collision-free IDs.
2. **Proven-unchanged Commit shortcut.** After valid supported-surface capture,
   bypass content construction and unnecessary scratch when the captured state
   is already represented by the canonical predecessor. Preserve exact stage,
   expected head/base, coverage and authoritative V4 publication receipts,
   including uncertain UpToDate. Host generation alone is not a dirty-mmap proof.
3. **Compact simple ranges and correspondence.** Reuse existing compact edit
   representations where suitable. The reviewed singleton correspondence can
   fit in 106 bytes inside the existing parent row instead of three private
   pages. Keep a single reader/planner interface with a packed indexed form for
   genuinely fragmented cases; preserve C2 origins and zero-anchor matching.
4. **One bounded resident/spill representation.** Use compact immutable pages,
   byte-aware packing and bounded prepared batches. Keep small workloads in
   charged host-owned buffers; evict individual pages incrementally under
   pressure without a whole-map conversion at capture or on the next write.
   Root ownership must not pin every page body in RAM. Preserve independent
   ownership, failure-atomic location changes and physical reclamation.
5. **Reduce duplicated record inventory where justified.** Evaluate primary-row
   effect sequences/subtree summaries instead of dual change indexes, cookies
   embedded in bindings, and carefully promoted singleton alias/source forms.
   Explicitly revise affected internal specifications before implementing a
   different selected representation. Preserve deletion markers, bounded old
   record cleanup and exact incremental C2 enumeration.
6. **Tiny/shared payload allocation.** Avoid a full allocation block per tiny
   source where ownership can be preserved. Ordinary writes remain ordinary
   Payload tokens with ordinary-spool charging, not SDK-inline inputs. Crossing
   a tiny-storage threshold must preserve sustained append coalescing, Origin
   coordinates and old readers. Promote a compact whole-source form before any
   operation creates a second independent token interval, including append/join
   or a full-range duplicate unless it reuses the same token correctly. Do not
   simply change allocation rounding or retain large mostly-dead segments.

Prefer deletion/reuse of existing machinery over another framework, database per
Workspace, storage hierarchy, or independent small/large encoding pipeline.
Validate the less invasive changes first; do not require every exploratory
representation idea if a simpler measured implementation achieves the objective.

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
requirements already belong to #124 and the earlier seven steps; the deferred
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

## Contracts that optimization must preserve

- Non-pausing supported FUSE/SDK behavior, mount/inode/open-descriptor identity,
  generic V1 visibility, exact C1/C2 and leased-source installation.
- Bounded snapshot acquisition with no full-tree/map scan, global freeze,
  writer-finish requirement, cache drain or delayed whole-overlay copy.
- Same CAS/authentication, FULL/DELTA, CDC/extents, packing/compression,
  construction/admission pipeline, branch leases and conditional publication.
- No mandatory checkpoint reset, copied snapshot tree, per-update canonical
  construction or unbounded operation/temporary history.
- Active readers, snapshots, replay and cleanup retain real ownership and
  charges; dropping cache entries or acknowledging logical End does not free
  still-owned data.
- Published-page in-place mutation is forbidden by the reviewed frozen spec.
  It is not part of the initial proposal and cannot be added silently.

## Performance reference and measurement boundaries

Read the [v0.1.5 benchmark closeout](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.5/release-notes/0.1.5/benchmark-closeout.md)
and [machine-readable performance rows](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.5/release-notes/0.1.5/benchmark-performance.csv).
The release record transcribes #120 evidence; the relevant fresh rows were
produced by binary `c55daf13e372331a5ab6dbd465ece351a55923831c45864325ac46b1508fa295`
on source `1ff1f2dddeb60493953311de316fa5bec4634a1a`.

| Existing case | Begin | Commit | End | Recorded boundary |
|---|---:|---:|---:|---|
| `workspace-clean-commit-1-compact-v2` (50 files / 1 MiB) | 6.731 ms | 1.501 ms | 2.167 ms | 10.495 ms public-call sum including visibility |
| `workspace-clean-commit-500-mixed-v4` (5,000 files / 500 MiB) | 13.144 ms | 1.634 ms | 2.929 ms | 17.795 ms public-call sum including visibility |
| `payload-random-read-1-compact-v2` | 10.824 ms | 2.179 ms | 3.174 ms | 20.460 ms including read/exec and visibility |
| `overwrite-head-4k-on-1mib-ops-1` | 11.843 ms | 9.054 ms | 3.071 ms | 10.948 ms edit + Commit; Begin/End excluded |

These are historical single samples, not forecasts or new gates. The v0.1.3
ratios have an undeclared reference cache profile and are not paired evidence.
Preserve original WARN/FAIL/waiver/reuse labels. A 100k-file Init is not a Begin
measurement; 500 Commits in one Workspace are not 500 fresh Workspaces. Cold
service startup and warm-service fresh Workspace lifetimes must be separate.

Use the existing benchmark contracts, runner entrypoints and measurement lock.
Keep Store/SDK/coordinator/construction/spool on macOS and daemon/FUSE/workload
in Linux Docker. Bind source, public dispatch, binary/image, fixtures, cache and
timer boundaries. A run on the legacy route does not measure the new snapshot
path. Do not run #122-owned scenarios or use `shared/v016_matrix.py` as a driver.

Start with existing clean-commit, random-read, localized SDK edit, tiny-create,
rename and Workspace-change-locality cases. Add only narrowly justified missing
fresh-lifetime/cost diagnostics, not a replacement benchmark framework. Preserve
large-file locality, successive Commit and capacity regressions. Reuse valid
passes; rerun only affected checks or those required by final source custody.

## Storage reporting and acceptance

Report both a 100k-file unchanged base with D=0/1/10 changes and a workload with
100k genuinely changed files. State names, directory shape, range fragmentation,
payload sizes, kernel pins, held snapshots and Commit phase. Neither base file
count nor a low final Store size describes all Workspace overhead.

The previously discussed ~105–245 MiB compact-layout estimates are **not approved
targets or an acceptable fixed per-call allowance**. Re-cost final encoded
records and actual ownership rather than guessing a smaller number. Report
actual RSS/cache demand, admission reservations, metadata/payload/snapshot-only
bytes, physical allocation, I/O, scratch peaks and cleanup separately. Repeated
bounded hot-set edits must not grow storage with unowned operation history.

- [ ] Verify prerequisites: all earlier seven steps complete and #124/#125 closed with valid terminal evidence.
- [ ] Recheck findings against the delivered source; preserve fixes/evidence already completed.
- [ ] Freeze the selected minimal representation and any explicit internal-spec amendments.
- [ ] Implement the selected improvements with focused ownership, spill, failure, append, hardlink, C1/C2 and publication checks.
- [ ] Demonstrate real fresh-Workspace identity per call and bounded setup/read/update/End costs.
- [ ] Prove ordinary operations complete during a held Commit build, live state/handles survive publication, C1/C2 coverage is exact, and conflict/reply-loss/reclamation paths preserve progress and ownership.
- [ ] Verify streaming lower/upper directory merge, bounded batched attributes, snapshot/live cursor semantics, changed-only Commit enumeration and direct range content scans; publish their separate cost counters.
- [ ] Publish matched before/after public timings and complete resource/storage accounting under applicable existing contracts.
- [ ] Preserve required large-file, repeated-Commit, capacity and supported-surface correctness; no unresolved required regression or cleanup failure.
- [ ] Publish source/evidence and close only when the selected implementation and verification are complete; no speed claim from arithmetic alone.

No release/tag, #122 execution, or closure of #123 is authorized by this issue.

Review artifacts currently exist under
`docs/roadmap/0.1/0.1.6/evidence/minimal-overhead-review/` in the working checkout:
`README.md`, `overlay-analysis.md`, `commit-analysis.md`, `lifecycle-review.md`,
baseline/source JSON and the extraction script. This issue is self-contained;
do not assume those local reports are already published immutable source. Locate
and publish the applicable review/source identities before implementation.
