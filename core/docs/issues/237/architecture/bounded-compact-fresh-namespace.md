# #237: build a fresh namespace from bounded, compact input

> **Status: Proposal; target LayerFS 0.1.7; not a released contract.**
> Source inspected at `00c677489f813d6157a72e245203e74123e732f6`.
> No streaming fresh-builder implementation, memory saving or speed gain is
> claimed. The companion [import-frontier note](bounded-import-frontier.md)
> covers native discovery and file jobs. The
> [implementation spec](bounded-import-implementation-spec.md) starts with a
> smaller treatment of the measured namespace peak.

## Goal and measured reason

Build the same Core canonical filesystem root for a fresh native import while
holding only bounded namespace input and construction state in process memory.
Keep the public `Client::init_project` call, four file constructors, C2 private
Save ownership, C5 inode allocation and History publication, and the existing
Workspace update path. This is a fresh-build design (`base: None`), not a new
general update algorithm.

[`build_namespace`](../../../../crates/layerfs-service/src/save/import/namespace.rs)
currently keeps all `PreparedEntry` values, then allocates full serial,
metadata-root, content-root, inode and directory inputs before calling
[`build_filesystem`](../../../../crates/layerfs-content/src/filesystem/update.rs).
The [compact-Job diagnostic](../c3-job-metadata-memory-result-20260924.md)
measured 14,680,064 B of entry-vector reservation and 18,838,440 B of
namespace-input vector reservation on 101,001 paths. Process peak RSS rose
from 105,857,024 B after the file loop to 144,703,488 B by namespace Save.
These are overlapping reservations and high-water observations, not additive
components or a prediction of recoverable RSS.

```text
Current fresh import

source -> Entries[N] -> file roots -> metadata roots[N] + content roots[N]
            |                            + serials[N] + inodes[N]
            |                            + directory updates
            +-----------------------------------|
                                                v
                                      FilesystemInput slices
                                                |
                                      C1 validate + fresh run
                                                |
                                      C2 tree Save -> C5 root
```

Core already has a direct fresh-build count path inside C1; its earlier
[fixed-identity 10k proof](../c1-fixed-identity.md) observed a lower raw
caller time with equal canonical roots and object IDs. That optimization did
not remove the whole-tree inputs, and its sampled RSS did not fall. This
proposal addresses retained state, not the same direct-build work again.

## Candidate boundary

Feed a fresh-build-only C1 entry point with ordered compact records under a
declared byte budget. A record supplies the final parent/name binding, typed
inode value and source-derived metadata/content roots needed for the Core
canonical format. The producer may pause when C1's bounded window is full;
one Save owner continues to drain canonical objects. C1 validates the stream
and emits directory and inode pages without retaining a second full-width
copy of all entries.

This boundary is **not yet an implementation recipe**. Current C1 validates
the complete input before it emits the first tree object. A safe streaming
candidate needs operation-owned ordered records that survive the file Save,
prerequisite Save, validation pass and construction replay. It must define
their byte limit, page-cache behavior and cleanup. Changing C1 to validate
while emitting would instead change the failure-side-effect contract and
requires its own approval and proof.

```text
Service: native source + published file roots
                    |
                    v
       bounded, ordered compact-record producer
                    |   (count/serial assignment is an open design point)
                    v
       +-----------------------------------------+
       | C1 fresh-build-only input boundary      |
       | validate order, bindings and identities |
       | derive counts; build directory pages    |
       | build sorted inode pages                 |
       | retain at most a declared work window   |
       +-----------------------------------------+
                    |
             one C2 tree Save -> C5 initialization

Workspace Commit -> existing FilesystemInput(base=Some)
                 -> existing C1 update -> its own C2 Save
```

The current [`FilesystemInput`](../../../../crates/layerfs-content/src/filesystem/input.rs)
requires complete sorted slices. Its shared `run` path also allocates a
fresh-build `initial_counts` vector, a map of directory content roots, and
validation maps/sets for binding counts and reachability
([C1 build](../../../../crates/layerfs-content/src/filesystem/update.rs),
[validation](../../../../crates/layerfs-content/src/filesystem/validate.rs)).
Merely replacing Service's vectors with an iterator would leave these
`O(N)` or `O(directories)` owners live. A claimed bounded design must account
for each of them, including high-fanout directories and worker completions
that arrive out of order. It may use bounded work windows or operation-owned
backing, but it must state where every displaced record lives.

## Correctness constraints that shape the design

1. **Serials and order.** Today C5 reserves one contiguous range after
   `scan_and_save` returns `entries.len()`. Entry indices determine serials
   in the scanner's breadth-first, per-directory name order. A changed order
   can change the canonical root even when file bytes match. The new path
   must learn the count and reproduce the required serial/binding order, or
   explicitly treat a changed root as a format/identity change. A counted
   source pass, bounded external records or a different reservation contract
   are options, not selected implementations. A second live source pass also
   needs mutation and count/order checks; the current file checks are not a
   directory snapshot. Any extra work stays inside the public-call timer.
2. **Validation.** C1 must still reject invalid names, serials, duplicate or
   unsorted bindings, wrong inode kinds, multiple directory parents,
   disconnected directories/cycles, capacity overruns and failed providers.
   A native traversal's shape is useful evidence, but cannot silently replace
   C1's current checked-input contract. Fresh-build validation must use
   bounded state or declared backing; an unbounded reachability set would
   defeat the goal.
3. **Canonical output.** The final directory pages, inode table, derived
   reference counts, metadata roots, object roles and root ID must match the
   current Core profile for fixed source, stack and scope identities. The
   v0.1.6 compact task format is a work-organization reference, not a codec
   to copy into Core.
4. **Ownership and failure.** File, prerequisite and tree Saves remain
   sequential initially. The existing Store provider reads published roots;
   this proposal does not merge Saves or hold a SQLite write transaction
   across input waits. Backpressure, deadline, source-stability checks,
   definite abort and unknown-outcome ownership must retain their current
   behavior. Each *remote mutation* in a high-level Workspace Commit takes
   its own service writer permit; its C2 Save is private, and C5 retains the
   expected-head check. An import keeps one permit for its entire call.
5. **All memory domains.** Disk records or a second source pass are not free
   setup. Temporary bytes, page cache, Store growth, process RSS and cleanup
   must be counted separately. A file-size-proportional spool or resident
   page-cache growth cannot be called a memory win solely because the heap
   shrinks. A directory too large to sort within the declared budget must
   have a bounded method or an explicit refusal.

## Expected benefit and limits

The target is to remove the simultaneous whole-tree Service entry/input
representations and C1 per-entry scratch from the namespace peak. That should
make *namespace working memory* depend on a declared window rather than the
total path count. C2's per-Save codec workspaces, SQLite cache, source-page
cache and the persistent Store remain separate costs. No exact RSS target can
be derived by subtracting vector reservations from the observed 144,703,488-B
peak; allocator behavior and lifetimes were not measured as disjoint charges.

A smaller candidate made `build_namespace` own the existing
`Vec<PreparedEntry>` and dropped it before C1 validation/build. The
[matched diagnostic](../c3-entry-lifetime-memory-result-20260924.md)
found **753,664 B higher**, not lower, whole-call peak RSS. The treatment
was reverted. It was a lifetime change, not a bounded namespace algorithm;
the result does not predict what avoiding those allocations entirely would do.

Fewer allocations and sequential construction might improve speed, but
counting, ordering, validation or backing I/O might erase that gain. The
retained 100k [stage comparison](../c3-v016-exact-source-side-by-side-20260924.md)
records 719.056500 ms for Core prerequisites plus tree Save and 452.704542 ms
for v0.1.6's different canonical tree and Save boundaries. Their difference is **not** a
candidate speed prediction. C5 History publication was 0.275084 ms in that
Core diagnostic, so changing History storage is not the target here.

## Proof before adoption

Freeze the exact fresh-build treatment and input-budget definition before a
candidate sample. Use one real public SDK import per arm, the same source and
fixed stack/scope identity **and identical initial History allocator state**.
Record and compare the C5 reservation start/count. Separately verify every
path, portable metadata field, size and file SHA-256. Compare exact canonical
root/object IDs and dense/sparse Store geometry; retain every failure.
Record the high-water at scan, file Save, compact-input construction, C1
validation, directory and inode construction, tree Save and call return,
alongside explicit live capacities and all scratch/backing bytes. The
[benchmark rules](../../../../../docs/general/benchmark_rules.md) forbid
credit from source or backing pages left warm by setup or another phase;
source metadata cache state must be qualified before a speed admission claim.

Exercise an overlapping native import and Workspace Commit against one Store
for aggregate RSS, admission/refusal, bounded progress, Store ownership and
same-branch conflict behavior. If shared C1 `run` or validation changes, run
the existing base-root update coverage as well. A bounded file-job queue
alone, or a compact Service record followed by full C1 maps, does not satisfy
this proposal's namespace-memory claim.
