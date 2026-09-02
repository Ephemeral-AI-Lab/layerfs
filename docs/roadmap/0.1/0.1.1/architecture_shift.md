# LayerFS v0.1.1 namespace architecture shift

> **Status:** Architecture and evidence record. The namespace-v2 direct
> admission path is implemented and measured. The current-seal selected
> evidence meets every tier-specific binding median when the supplemental
> 100-file run is combined with the 1,000/10,000/100,000-file matrix, but no
> release candidate exists. Numbers labeled namespace-v2 below are retained
> development evidence, not a release claim.
>
> **Compatibility target:** The measured and fixed-seed equivalence paths
> preserve the released canonical form, Store schema, eager initialization
> semantics, public SDK/CLI behavior, and visibility-last publication. An
> apparent greater-than-1,000-top-level-task direct-path boundary remains a
> pre-release finding, so this record does not claim universal compatibility
> before that case is contained or proved.

## Purpose

LayerFS v0.1.1 began with a released initialization path that became
catastrophically superlinear at large namespace sizes. The work progressed
through two architectural corrections:

```text
v0: released incremental-mutation importer
    -> repeated immutable-tree reconstruction
    -> candidate-spool index cliff
    -> worst-case quadratic candidate lookup

v1: deferred final-state importer
    -> scan once
    -> discard obsolete transient structural nodes
    -> emit only final reachable directory and inode state
    -> indexed, bounded Store admission

v2: bounded direct-admission pipeline
    -> construct and admit concurrently
    -> reuse exact metadata results
    -> move owned canonical slabs
    -> remove the object-segment write/read pass
```

This document explains the algorithms, their complexity, the simplifications
that created the gains, and the value and limits of every retained mechanism.
It intentionally uses ASCII diagrams only.

## Nomenclature

`v0`, `v1`, and `v2` in this document name benchmark and implementation
generations. They are not three LayerFS releases.

| Name | Product path | Fixture | Meaning |
| --- | --- | --- | --- |
| `v0` | Released LayerFS 0.1.0 | Uniform 2,500-byte files | Original public initialization algorithm |
| `v1` | First v0.1.1 optimized path | Same uniform fixture | Deferred final-state construction and bounded admission |
| `v2` | Current development candidate | Mixed-size, small-heavy fixture with 100-MB anchors | Eligible empty-Store direct pipeline and synthetic byte distribution |

Only v0 and v1 are a strict same-fixture comparison. V2 changes both the
algorithm and the data distribution, so its MB/s must not be presented as a
pure v1-to-v2 product speedup.

## Symbols and lower bounds

The complexity discussion uses:

```text
N = regular-file count
D = directory count
B = total logical source bytes
C = content chunks produced by CDC
E = all canonical-object emissions, including duplicates and intermediates
K = total canonical bytes encoded and hashed across all emissions
A = SQLite row attempts after current-batch dedup
U = unique canonical objects ultimately stored
S = canonical bytes transferred through temporary object storage
k_d = child count of directory d
H = canonical tree height, approximately O(log N)
W = bounded import-producer count
Q = bounded slab-queue capacity
```

Under eager initialization and the unchanged canonical form, LayerFS has the
following unavoidable lower bounds:

```text
discover N paths                  Omega(N)
read B source bytes               Omega(B)
chunk and hash content            Omega(B + C)
construct final file/inode state  Omega(N)
store U unique canonical objects  Omega(U)
```

With the current SQLite B-tree Store, `A` submitted rows require approximately
`O(A log U)` primary-key work, where `U <= A <= E`. LayerFS therefore cannot
honestly make initialization depend
only on total bytes without lazy initialization, a trusted prebuilt manifest,
a pack format, a Store-layout change, or a canonical-form change. Those are
explicitly outside v0.1.1.

The target is instead to remove avoidable work so the implementation approaches
the lower bound:

```text
O(B + K + N + A)
```

while retaining the current tree and Store costs:

```text
O(B + K + sum(k_d log k_d) + N log N + A log U)
```

For the benchmark shape, each data directory has 100 files, so:

```text
sum(k_d log k_d) = O(N log 100) = O(N)
```

The global inode structure and SQLite object index remain logarithmic.

## Performance evolution

### Same-fixture v0 to v1

V0 and v1 use exactly the same deterministic 2,500-byte-per-file fixtures.

| Scenario | Logical bytes | v0 initialization | v0 throughput | v1 initialization | v1 throughput | Improvement |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `namespace-100` | 0.25 MB | 37.651--38.268 ms | 6.53--6.64 MB/s | 7.732 ms | 32.33 MB/s | about 4.9x |
| `namespace-1000` | 2.5 MB | 780.515 ms | 3.20 MB/s | 38.232 ms | 65.39 MB/s | 20.4x |
| `namespace-10000` | 25 MB | more than 524.79 s | less than 0.0476 MB/s | 489.426 ms | 51.08 MB/s | more than 1,070x |
| `namespace-100000` | 250 MB | not attempted | unavailable | 6.799 s | 36.77 MB/s | unavailable |

The v1 values for 100, 1,000, and 10,000 files are audited three-sample
medians. The 100,000-file v1 value is one additional audited safety sample.
V0's 10,000-file process was deliberately stopped while still CPU-bound; its
lower bound is retained rather than retried away. V0's 100,000-file tier was
not attempted after the 10,000-file defect was already established.

### Current namespace-v2 evidence

V2 uses a different, mixed-size fixture and therefore stands in a separate
comparison lane.

| Scenario | Files | Logical bytes | Selected init median | Throughput | File rate | Create | Commit |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `namespace-100` | 100 | 125 MB | 220.820 ms | 566.1 MB/s | 452/s | 14.742 ms | 2.730 ms |
| `namespace-1000` | 1,000 | 200 MB | 269.757 ms | 741.4 MB/s | 3,707/s | 12.394 ms | 2.532 ms |
| `namespace-10000` | 10,000 | 300 MB | 414.729 ms | 723.4 MB/s | 24,112/s | 18.750 ms | 3.448 ms |
| `namespace-100000` | 100,000 | 500 MB | 2.766 s | 180.7 MB/s | 36,149/s | 12.556 ms | 3.857 ms |

These are current-seal, three-subsequent-sample medians under the declared
`generated-subsequent-sample-uncontrolled` cache profile. Every LayerFS
process and Store is fresh; the fixture is generated once and reused without
clearing or deliberately warming the host page cache. They are effective
logical-throughput results, not physical-disk-throughput claims.

The selected evidence uses `issue9-v3-final-create-100-r001-20260903` for the
100-file row and `issue11-v3-terminal-all4-composite-r003-20260903` for the
other three rows and runner-owned composite proof. Both carry source seal
`f6a2c969ca9245b0394c91643d6c24a2f56180975fad537c10fb5360358d4170`.
The all-tier `r003` report by itself retains a 15.223-ms 100-file Create miss;
the supplemental same-seal 100-file median is 14.742 ms. Neither raw report is
rewritten or relabeled. The selected rows meet every tier-specific binding
median, while the 100,000-file row remains below the preferred nonbinding
2.5-second/200-MB/s outcome. This split evidence is not a release claim.

The earlier source-sealed one-sample init-only screen remains retained as
historical evidence: 251.905/295.101/457.022/3,040.053 ms and
496.2/677.7/656.4/164.5 MB/s. Its 100,000-file row was captured as a miss
under the then-current 200-MB/s binding contract and is never retroactively
relabeled.

At 100,000 files, v2 reads twice the logical bytes of v1 in 45 percent of the
wall time:

```text
v1: 250 MB / 6.799 s; about 14,708 files/s
v2 selected: 500 MB / 2.766 s; about 36,149 files/s
```

The file-rate improvement is about 2.46x. The MB/s improvement is larger partly
because the 100-MB anchors amortize per-file work over more bytes.

## V0: incremental persistent mutation

### Algorithm

Released v0.1.0 reused the ordinary filesystem-mutation path for bulk import.
For every source entry, it updated an already-immutable namespace and then
applied metadata through another mutation.

```text
Source directory
      |
      v
 Read next entry
      |
      v
 Resolve path in current immutable namespace
      |
      +-----------------------------+
      |                             |
      v                             v
 Build file content           mkdir/link/symlink
      |                             |
      +--------------+--------------+
                     |
                     v
            Update directory tree
                     |
                     v
            Update global inode tree
                     |
                     v
            Emit new namespace root
                     |
                     v
             Apply mtime separately
                     |
                     v
             Resolve the path again
                     |
                     v
       Rebuild directory and inode paths
                     |
                     v
        Retain intermediate canonical objects
                     |
               more entries?
                 /       \
               yes        no
                |          |
                +--repeat  v
                       candidate planning
                              |
                              v
                 128-object membership pages
                              |
                              v
                 <=127 objects per transaction
                              |
                              v
                    publish LayerStack
```

### Repeated immutable-tree work

One update to a persistent balanced tree creates a new leaf-to-root path:

```text
one tree update:   O(H) = O(log N)
N entry updates:  O(N log N)
```

V0 performed multiple logical updates for many entries, including a separate
mtime update. The file content was necessary; the repeated resolution and
structural reconstruction were not.

The emitted-object count was consequently closer to:

```text
E_v0 = O(C + N log N)
```

instead of the final reachable closure, which is closer to `O(C + N + D)`.

### The candidate-spool index cliff

The decisive worst-case defect appeared after candidate metadata exceeded its
bounded in-memory index. The object index was capped at 8 MiB and charged about
64 bytes per entry:

```text
8 MiB / 64 bytes ~= 131,072 indexed objects
```

Before overflow, a candidate lookup used an ordered in-memory index. After
overflow, the implementation discarded the index and scanned the spool file
from its beginning for an exact object lookup.

```text
Canonical object
      |
      v
 Is spool index available?
      /                 \
    yes                  no
     |                    |
     v                    v
 indexed lookup       scan spool from byte zero
 O(log E)             O(E), or O(S) in bytes
                          |
                          v
                    repeat for later objects
                          |
                          v
                       O(E^2)
```

The exact worst-case expression is `O(E * S)` byte inspection. With bounded
average canonical-object size, `S = O(E)`, so the fallback is `O(E^2)`.
A related exact candidate-ID set also had a linear spill-scan fallback.

The v0 worst case was therefore:

```text
T_v0 = O(B + N log N + E*S + U log U)

with bounded average object size:
T_v0 = O(B + N log N + E^2 + U log U)
```

This is a proved code-path upper bound. The observed 1,000-to-10,000-file cliff
is consistent with crossing it, but the retained evidence does not assign
every one of the 524.79 seconds to one function.

### Why v0 collapsed

```text
100 -> 1,000 files
file count:          10x
initialization:    20.4x

1,000 -> 10,000 files
file count:          10x
initialization:    >672x
```

V0 combined four amplifiers:

1. repeated path resolution and persistent-tree updates;
2. intermediate structural-object retention;
3. exact candidate lookups that could become linear spool scans;
4. at most 127 admitted objects per transaction.

This was not a slow storage device or a simple constant-factor issue. It was a
bulk operation implemented as thousands of increasingly expensive point
mutations, followed by a threshold-triggered worst-case candidate path.

## V1: bottom-up final-state construction

### Architecture

V1 changed the question from "how do I replay every file creation?" to "what
is the final canonical namespace?"

```text
                     Source root
                         |
                         v
              Sort top-level directory tasks
                         |
          +--------------+--------------+
          |              |              |
          v              v              v
       worker 1       worker 2       worker W
          |              |              |
          | scan assigned directories   |
          | read file contents once      |
          | collect final children       |
          | collect final inode records  |
          |              |              |
          +--------------+--------------+
                         |
                         v
                deterministic ordered merge
                         |
                         v
          update deferred local directory B-trees
          and prune obsolete transient nodes
                         |
                         v
             emit final reachable directory nodes
                         |
                         v
        insert compact pairs into deferred inode B-tree
                         |
                         v
             emit final reachable inode nodes
                         |
                         v
              construct one namespace root
                         |
                         v
                is Store provably empty?
                   /                \
                 yes                 no
                  |                   |
                  v                   v
        skip impossible membership  exact bounded membership
                  |                   |
                  +---------+---------+
                            |
                            v
                <=8,191 objects or <4 MiB
                    per transaction
                            |
                            v
                publish Layer + LayerStack last
```

### Complexity shift

V1 retained the unavoidable source and tree work:

```text
source scan and content:       O(N + D + B)
canonical encode/hash work:    O(K)
directory sorting/building:    O(sum(k_d log k_d))
final inode construction:      O(N log N)
indexed candidate handling:    O(E log E) in the measured range
SQLite B-tree admission:       O(A log U)
```

It removed the v0 `E*S` spool-scan term and reduced emitted structural objects
toward the final closure:

```text
v0 emissions:  E_v0 = O(C + N log N), including intermediates
v1 emissions:  E_v1 ~= O(C + N + D), final reachable objects
```

The measured-range v1 class became:

```text
T_v1 = O(B + K + sum(k_d log k_d) + N log N + A log U)
```

V1 still documented a larger-scale linear-spill fallback beyond roughly 1.4
million unique IDs. The audited 100,000-file v1 sample had about 808,000
objects and did not cross it. Therefore `O(U log U)` describes the measured
regime, not an unlimited global guarantee.

## V0-to-v1 maneuvers and their value

### 1. Emit only final reachable directory state

```text
Before:
empty -> add child 1 -> rebuild
      -> add child 2 -> rebuild
      -> ...
      -> add child N -> rebuild

After:
sorted final children
-> insert through a deferred local B-tree
-> prune obsolete transient nodes
-> emit only final reachable directory nodes
```

Value:

- removes growing-prefix directory reconstruction;
- eliminates obsolete intermediate directory objects;
- converts global mutation replay into bounded local deferred construction;
- preserves the exact final canonical directory nodes and ordering.

### 2. Emit the inode table from final pairs

```text
Before:
for every inode -> mutate persistent table -> emit a new root

After:
collect compact (InodeId, record ObjectId) pairs
-> insert pairs into a transient deferred tree
-> prune obsolete transient nodes
-> emit only the final reachable table
```

Value:

- removes repeated global inode-root replacement;
- retains compact identity information instead of complete file objects;
- emits final structural nodes instead of every historical state;
- preserves the same inode IDs, records, and canonical root.

### 3. Import independent root directories in parallel

The fixed fixture has many independent top-level data directories. V1 imports
them with a bounded worker count and merges results in deterministic task
order.

Value:

- reduces source-read and canonical-construction wall time;
- does not introduce nondeterministic canonical order;
- detects any hard link and falls back to the canonical final-state path;
- changes wall time, not required total work.

If `P` is parallelizable construction work and `R` is serial work, the useful
model is Amdahl's law:

```text
T_parallel ~= P/W + R + scheduling overhead
```

The worker count is bounded because more workers eventually trade CPU and
context switches for negligible wall-time gain.

### 4. Skip membership checks for a provably empty Store

```text
Before:
for every candidate page -> ask whether each ID already exists

After:
prove Store empty once -> no candidate can be preexisting
```

Value:

- removes an impossible-case database lookup pass;
- keeps the nonempty-Store path exact;
- retains normal collision handling during admission;
- adds no cache or persistent state.

### 5. Increase bounded admission batches

```text
Before: <=127 objects per transaction
After:  <=8,191 objects or <4 MiB per transaction
```

If average admitted payload permits the object limit, transaction count moves
from approximately:

```text
ceil(U / 127)
```

to:

```text
ceil(U / 8,191)
```

The 4-MiB byte cap remains authoritative for larger objects.

Value:

- removes thousands of BEGIN/COMMIT and statement-lifecycle costs;
- preserves bounded dirty pages and candidate ownership;
- avoids a single unbounded transaction;
- preserves visibility-last final publication.

### 6. Remove the unused initialization reference graph

Generic mutation and reconciliation candidates need object-reference tracking.
Final-only initialization already produces the complete reachable closure and
does not consume that graph.

Value:

- removes `O(E)` reference parsing and bookkeeping from initialization;
- reduces memory and hash-map work;
- changes only the proved final-only initialization buffer;
- leaves Workspace and reconciliation tracking intact.

### 7. Localize a ten-byte Commit

Released Commit built complete base and final namespace manifests even when
one file changed. The v1 fast path operates on the touched inode and rebuilds
only affected canonical paths.

```text
Before:
scan base namespace O(N)
-> scan final namespace O(N)
-> compare complete manifests
-> materialize complete captured state

After, topology unchanged:
resolve touched inode O(log N)
-> write changed range O(delta bytes)
-> rebuild affected inode path O(log N)
-> publish eight small candidates in the 100k measurement
```

Value:

- changes localized Commit from namespace-sensitive `O(N)` planning toward
  `O(log N + changed bytes)`;
- keeps Commit around 2--5 ms across the measured namespace tiers;
- the selected 100,000-file samples emit eight candidates totaling
  11,724--12,172 bytes; the field median is 11,788 bytes (11.51 KiB), and the
  Commit-time median is 3.857 ms;
- retains the complete-manifest fallback for rename, link, type, and topology
  changes;
- does not weaken concurrent Branch or Workspace publication checks.

### 8. Remove redundant FUSE lookup round trips

Directory enumeration had already computed child attributes, but the normal
response returned only name, node, and kind. Linux then issued another lookup
for attributes.

```text
Before:
readdir -> child list
       -> one additional lookup per child

After:
READDIRPLUS -> child list + attributes
            -> populate directory and attribute caches together
```

At 10,000 files, retained OS message counts changed from:

```text
sent:      40,452 -> 20,252
received: 120,752 -> 90,452
```

Value:

- removes one redundant synchronous host round trip per child;
- reduced audited 10k exact reopen from 6.343 s to 3.639 s;
- uses standard real-FUSE behavior rather than bypassing FUSE;
- keeps normal `readdir` as a fallback.

### 9. Store directory parents explicitly

Directory `..` resolution previously scanned materialized nodes. The Workspace
now maintains an exact parent relationship across materialization, creation,
rename, and reclaim.

```text
before parent lookup: O(materialized nodes)
after parent lookup:  O(1)
```

Value:

- removes a repeated namespace scan from traversal;
- preserves rename and reclaim correctness;
- uses a direct relation already implied by the materialized tree.

## V2: bounded direct-admission pipeline

V1 removed the catastrophic algorithm, but it still separated construction
and persistence into sequential passes:

```text
construct complete worker segments
-> wait for workers
-> read segments
-> admit into SQLite
-> build final structural root
```

For the eligible measured shape, v2 turns those phases into one bounded
pipeline. The direct path is selected only for the first LayerStack in a
proven-empty Store when the nonempty source root contains only top-level
directories, no hard link is detected, and direct structural limits hold.
A root-level regular file or symlink, any hard link, or a nonempty Store uses
the canonical final-state fallback. That fallback has different worker,
spool, memory, and performance characteristics and is not represented by the
direct-path measurements below.

The namespace-v2 fixture deliberately qualifies: its root contains only
independent data directories and each directory contains 100 files. Its byte
distribution is synthetic and small-heavy; it is not a claim that the root
topology reproduces a typical repository.

### Architecture

```text
               Eight existing import producers
               ===============================

 worker 1       worker 2       ...       worker 8
    |              |                         |
    v              v                         v
 read/stat/open source files in parallel
    |              |                         |
    v              v                         v
 unchanged CDC and canonical construction
    |              |                         |
    v              v                         v
 exact bounded metadata-root reuse
    |              |                         |
    v              v                         v
 fill owned slab: <=256 KiB and <=512 objects
    |              |                         |
    +--------------+------------+------------+
                                  |
                                  v
                      synchronous queue: 4 slabs
                                  |
                                  | move ownership
                                  | parent payload copy = 0
                                  v
                    calling thread / sole SQLite owner
                                  |
                                  v
                  carry one exact-dedup admission batch
                     <4 MiB or <=8,191 objects
                                  |
                                  v
                       SQLite object insertion
                                  |
                                  v
                      bounded transaction commit
                                  |
                                  v
        compact pair streams and final deferred inode B-tree
                                  |
                                  v
        final transaction: remaining objects + Layer + LayerStack
                                  |
                                  v
                  public structural visibility begins
```

The implementation uses the existing calling thread as admission owner. It
does not create a database worker or a second product worker pool. The active
thread peak on the measured direct path is nine: eight producers plus the
caller. The fallback can use up to 16 producers, so eight is a direct-path
measurement and limit, not a universal initialization bound.

Object-only transactions commit during production and final construction.
Only the final transaction adds the genesis Layer and LayerStack. Publication
is therefore visibility-last, but the complete import is not one atomic
SQLite transaction. A handled failure clears objects admitted to the
previously empty Store; an abrupt process failure can leave unreachable
object rows without exposing a partial LayerStack.

### Critical-path shift

If producer work takes `P`, SQLite admission takes `A`, and finalization takes
`F`, sequential v1 behaves like:

```text
T_v1_wall ~= P + A + F
```

Pipelined v2 approaches:

```text
T_v2_wall ~= max(P, A) + F + pipeline stalls
```

```text
Sequential
time ------------------------------------------------------------>

producer  [================ 2.1 s ================]
SQLite                                                [== 1.5 s ==]
final                                                              [.25]

total = 2.1 + 1.5 + 0.25 = 3.85 s


Pipelined target
time ------------------------------------------------------------>

producer  [================ 2.1 s ================]
SQLite          [============ 1.5 s ============]
final                                                [.25]

total = max(2.1, 1.5) + 0.25 = 2.35 s
```

Big-O notation alone hides this gain because both sums are asymptotically
linear in the same inputs. The critical-path equation captures the
architectural value.

## V1-to-v2 maneuvers and their value

### 1. Use a synthetic small-heavy mixed-size fixture

The original uniform fixture exposed namespace scaling but underrepresented a
workspace-like size distribution. V2 keeps the same four scenario IDs and
100 files per directory while using empty, tiny, small, medium, and exact
100-MB anchor files. It deliberately does not reproduce a live repository's
root topology, extensions, links, or directory-depth distribution.

Value:

- separates byte throughput from namespace-operation throughput;
- exposes the 100k small-file cliff while retaining large-file behavior;
- prevents a tiny-file-only result from being mistaken for bulk throughput;
- remains deterministic and fully materialized.

This is a benchmark-contract improvement, not a production optimization.

### 2. Intern exact portable-metadata results

Many files share exactly the same canonical metadata input:

```text
(inode kind, permission mode, mtime seconds, mtime nanoseconds)
```

Each importer holds an eight-entry operation-local cache:

```text
first exact tuple
-> cache miss
-> use unchanged canonical metadata builder
-> emit canonical graph
-> remember tuple -> root ObjectId

later identical tuple
-> exact cache hit
-> reuse deterministic root ObjectId
```

The cache is destroyed with initialization. It is neither persistent nor warm
across runs.

At 100,000 files, the measured effect is:

| Counter | Before exact reuse | Current direct path |
| --- | ---: | ---: |
| Canonical emissions | about 1,132,000 | about 439,070 |
| Pending duplicate candidates | 708,845 | about 15,200 |
| Canonical payload | about 601.8 MB | 544.3 MB |
| Exact metadata cache hits | 0 | 99,000 |
| Exact metadata cache misses | unavailable | 2,000 |

Value:

- removes roughly 693,000 redundant encode/hash/transfer operations;
- changes repeated construction into an `O(1)` bounded lookup;
- uses constant memory because both worker count and cache capacity are fixed;
- preserves canonical bytes and IDs because only exact canonical inputs reuse
  a root produced by the unchanged builder.

The earlier isolated cache experiment was not a decisive wall-time win while
the object spool remained. Its value appears in the direct pipeline by
removing objects before handoff and admission.

### 3. Move coarse owned slabs

Each producer fills a slab until either bound is reached:

```text
payload <= 256 KiB
objects <= 512
```

The byte cap bounds payload memory. The object cap bounds headers when the
slab contains many tiny objects. Ownership of each `Vec<u8>` moves through the
queue and admission batch; the parent does not copy its payload.

At 100,000 files, retained counters report:

```text
slab handoffs:             2,147
slab objects:       about 439,070
slab payload:        544,309,172 bytes
queue peak:                       4 slabs
queue payload peak:       1,048,576 bytes
parent payload copies:            0 bytes
```

Value:

- replaces object-granular synchronization with coarse ownership transfer;
- prevents a queue from becoming an unbounded in-memory spool;
- reduces wakeups and context switches;
- permits producers and SQLite to run concurrently;
- requires no custom allocator or new dependency.

### 4. Remove the object-segment write/read pass

The pre-direct namespace-v2 baseline transferred approximately:

```text
647 MB -> temporary object-segment write
647 MB -> temporary object-segment reread
543 MB -> final SQLite canonical payload
```

The current direct path reports:

```text
object_segment_write_bytes     = 0
object_segment_raw_read_bytes  = 0
object_segment_passes          = 0
parent_payload_copy_bytes      = 0
```

Value:

- removes about 1.294 GB of intermediate object traffic at 100k;
- removes a complete sequential phase boundary;
- lowers temporary storage rather than trading memory for speed;
- leaves the final reachable canonical encoding and public Store schema
  unchanged.

This is specifically the large canonical-object segment. The direct path
retains a compact 64-byte `(InodeId, record ObjectId)` pair stream. At 100,000
files it writes and rereads 6,464,000 bytes, and the final SQLite Store grows
by roughly 662 MB for the 500-MB fixture. Zero object-segment traffic is not a
claim of zero temporary I/O, zero storage amplification, or zero durable
Store writes.

### 5. Keep one carried 4-MiB admission batch

The sole admission owner carries one batch across slab, directory, and worker
boundaries:

```text
slab A ----+
slab B ----+--> same pending batch --> flush before 4 MiB or at 8,191 objects
slab C ----+
```

Value:

- avoids one transaction per slab or directory;
- keeps exact duplicate and collision checks local to bounded memory;
- retains about 131 bounded transactions at 100k;
- delegates global uniqueness to the existing Store instead of adding a
  complete in-memory ID set or temporary database.

### 6. Keep one SQLite owner

SQLite admission stays on the calling thread.

Value:

- avoids lock contention between database writers;
- preserves the Store's operation gate and publication ordering;
- avoids a new worker, connection, or acknowledgment protocol;
- keeps failure handling local and deterministic.

The SQLite profile shows that database time is a write path, not a payload-read
path:

```text
primary-key B-tree probe
-> primary-key index insertion
-> objects table insertion
-> page allocation/balance
-> bounded commit pwrite
```

Actual cross-batch conflict reads at 100k were only about 151 KB and 13 ms.
Adding read prefetch, a reader worker, or an initialization database cache
would optimize the wrong side.

### 7. Separate content flow from structural publication

Path-independent content-addressed objects can be admitted while producers
continue. Inode and directory structure remains compact until ordering is
known; any detected hard link rejects direct completion and selects the
canonical fallback.

```text
content objects ---------------------> bounded direct admission

compact inode pairs
      |
      v
deterministic task ordering
      |
      v
any-hard-link fallback barrier
      |
      v
final inode table and namespace root
      |
      v
visibility-last LayerStack publication
```

Value:

- overlaps safe content admission with source construction;
- prevents partial structural state from becoming a visible LayerStack;
- retains deterministic canonical ordering;
- preserves the serial fallback for incompatible hard-link topology.

The compact pair stream is intentionally retained. At 100k it transfers about
6.46 MB in each direction, far below the removed 647-MB object spool. Removing
it before measurement identifies it as a bottleneck would add complexity with
little expected value.

### 8. Demand-load Workspace Create

Earlier Workspace Create loaded Store-wide small-object state. The retained
path loads authenticated objects when traversal actually requests them and
reuses the decoded root/bootstrap state.

```text
before: Workspace Create ~= O(Store objects)
after:  Workspace Create ~= O(bootstrap and requested roots)
```

Value:

- makes Workspace Create insensitive to unrelated Store population;
- avoids a Store-wide scan at 100k;
- leaves source initialization eager;
- does not prefetch 100-MB anchors.

The selected current-seal Workspace Create medians are 14.742, 12.394, 18.750,
and 12.556 ms for 100 through 100,000 files. The 100,000-file ceiling is 25 ms,
with a separate non-Attach target of 10 ms. The cache profile is uncontrolled,
not labeled warm.

### 9. Bound FUSE reads without read amplification

The earlier exact reopen path fetched overlapping large read-ahead windows and
amplified 500 MB of served data into multiple gigabytes of fetched data. The
retained read path uses bounded per-node entries, skips fully served responses,
and caps actual read-ahead at 8 MiB.

Value:

- 10k exact verification fetches exactly 300 MB to serve 300 MB;
- 100k exact verification fetches exactly 500 MB to serve 500 MB;
- unused fetched bytes are zero in the retained proof;
- exact verification remains outside `layerstack_init_ns` but inside the same
  sample's `complete_product_ns`.

This improves the full lifecycle but is not counted as initialization
throughput.

The same product sample records both clocks; verification is not discarded or
substituted with a different execution path:

```text
T0                                                             T7
|---------------------------------------------------------------|
| init | fork | Create | edit | Commit | End | reconnect | verify
|<-- layerstack_init_ns -->|
|                                              |<- reopen_verify_ns ->|
|<------------------- complete_product_ns ---------------------->|
```

| Selected scenario | Initialization | Exact reopen verification | Complete product |
| --- | ---: | ---: | ---: |
| `namespace-100` | 220.820 ms | 1.029 s | 1.281 s |
| `namespace-1000` | 269.757 ms | 1.581 s | 1.875 s |
| `namespace-10000` | 414.729 ms | 5.614 s | 6.062 s |
| `namespace-100000` | 2.766 s | 47.367 s | 50.165 s |

Per-tier fresh reopen proves the exact path set, content, metadata, expected
edit, digest, and cleanup through real FUSE. The separate 10,000-file
materialization/FUSE equality proof establishes equal logical state and equal
canonical root; the generic per-tier reopen row does not replace that proof.

## V2 complexity and memory

### Time

The current direct path retains:

```text
source scan and content:       O(N + D + B)
canonical encode/hash work:    O(K)
directory sorting/building:    O(sum(k_d log k_d))
final inode construction:      O(N log N)
batch-local dedup:             expected O(E) with a bounded hash map
SQLite admission:              O(A log U)
```

The broad work class is therefore still:

```text
T_v2_work = O(B + K + sum(k_d log k_d) + N log N + A log U)
```

The wall-time shift is:

```text
T_v2_wall ~= max(producer lane, SQLite lane)
            + structural finalization
            + measured pipeline stalls
```

This is the measured-path model for the fixed benchmark topology and object
range. `A`, rather than only `U`, matters because duplicates that cross batch
boundaries can still reach SQLite. It is not an unlimited guarantee for every
source shape or Store population.

### Explicit payload memory

For fixed:

```text
W = 8 producers
Q = 4 queued slabs
s = 256 KiB per slab
a = 4 MiB admission payload limit
```

the primary payload ownership is:

```text
M_payload = O(W*s + Q*s + a) = O(1) with respect to N and B
```

The selected current-seal 100k evidence reports:

```text
modeled named-buffer peak:              10,101,246 bytes, 9.63 MiB
initialization incremental HWM maximum: 103,006,208 bytes, 98.23 MiB
complete-lifecycle HWM maximum:         208,388,096 bytes, 198.73 MiB
```

These are different measurements. The 9.63-MiB value is instrumentation's
modeled sum of named buffers, not a complete allocator census of everything
LayerFS owns. Whole-process RSS includes SQLite's 32-MiB connection page-cache
target, thread stacks, allocator state, deferred-tree allocations, shared
code, and runtime state. Native process high-water remains the authoritative
aggregate memory measurement.

Not all transient state is globally `O(1)`:

- top-level task descriptors are `O(D)`;
- deferred directory and final inode construction have structural state tied
  to their inputs;
- compact inode pairs are `O(N)` but primarily file-backed and bounded in
  memory;
- the durable Store is necessarily `O(B + U)`.

The architectural guarantee is that the large canonical payload is not held
in a complete namespace-sized vector or unbounded queue.

## Why 10k to 100k still regresses

The current 10k-to-100k transition increases logical bytes only 1.67x but
increases file-count-sensitive work by roughly 8--10x.

| Counter | 10k | 100k | Ratio |
| --- | ---: | ---: | ---: |
| Logical bytes | 300 MB | 500 MB | 1.67x |
| Files | 10,000 | 100,000 | 10.00x |
| Directories | 100 | 1,000 | 10.00x |
| File opens | 10,000 | 100,000 | 10.00x |
| Metadata operations | 10,101 | 101,001 | 10.00x |
| Source reads | 28,205 | 210,687 | 7.47x |
| Canonical emissions after metadata reuse | about 56,160 | about 439,070 | 7.82x |
| SQLite rows submitted | about 55,150 | about 423,850 | 7.69x |
| Selected initialization median | 414.729 ms | 2.766 s | 6.67x |

The cost model is therefore:

```text
T(N, B, U) = source_syscalls(N)
           + content_processing(B)
           + canonical_work(U)
           + SQLite_insert(U log U)
           + inode_tree(N log N)
           + pipeline_stalls
```

At the first three tiers, the large anchors make byte processing dominant and
per-file work amortizes well. At 100k, the `N` and `U` terms dominate the
modest byte increase. This is no longer the v0 quadratic cliff; it is the
unavoidable file/object cardinality becoming visible, plus a remaining
pipeline-utilization gap.

## Architecture scorecard

| Concern | v0 | v1 | v2 |
| --- | --- | --- | --- |
| Import model | Replay point mutations | Build final state | Build and admit final state concurrently |
| Persistent-tree work | Repeated leaf-to-root reconstruction | Bottom-up final construction | Same final construction, pipelined |
| Candidate lookup | Indexed, then possible linear spool scan | Indexed in measured range | Bounded batch-local hash plus Store authority |
| Worst measured-path class | Contains `O(E*S)`, commonly `O(E^2)` | Measured-range final-state model | `O(B + K + N log N + A log U)` for the fixed shape |
| Wall composition | Sequential, with quadratic cliff | `P + A + F` | `max(P, A) + F + stalls` |
| Object transfer | Candidate spool and rescans | Worker segment write/read | Owned slabs; no object segment |
| Metadata construction | Repeated per file | Repeated per file | Exact bounded result reuse |
| Admission bound | 127 objects | 8,191 objects and less than 4 MiB | Same, carried across slabs |
| Database ownership | One writer | One writer | One writer, concurrent with producers |
| Payload memory | Bounded cache with pathological scan fallback | Segmented, but large completed state | Fixed slabs and bounded queue |
| Publication | Visibility last | Visibility last | Object-only commits, then visibility-last Layer/LayerStack transaction |
| Canonical/Store format | Released format | Unchanged | Unchanged |

## Simplification summary

The improvement came mainly from deleting stages and duplicate work:

```text
removed: replay every import as a public mutation
kept:    one source traversal and one final-state construction

removed: growing history of intermediate namespace roots
kept:    final reachable canonical closure

removed: post-index exact linear spool scan in the measured path
kept:    bounded exact dedup and collision checks

removed: impossible empty-Store membership queries
kept:    exact nonempty-Store behavior

removed: thousands of tiny transactions
kept:    less-than-4-MiB/8,191-object bounded transactions

removed: duplicate portable-metadata construction
kept:    exact canonical root from the unchanged builder

removed: 647-MB object-segment write and reread
kept:    direct canonical-payload movement plus a 6.464-MB compact pair stream

removed: second database writer or worker pool
kept:    the caller as sole admission owner

removed: Store-wide Workspace Create scan
kept:    authenticated demand loading

removed: redundant FUSE lookup and overlapping read-ahead
kept:    real Linux FUSE and exact reopen proof
```

The resulting design is smaller conceptually:

```text
scan once
-> build in bounded deferred structures
-> prune obsolete transient structural nodes
-> emit only final reachable structural nodes
-> move canonical payload through bounded owned slabs
-> publish once
```

## Invariants preserved through every shift

The performance architecture is not allowed to change:

- canonical encoding, directory order, or CDC behavior; fixed-seed reference
  and optimized builders must produce the same final reachable root and bytes;
- the released five-table Store schema;
- eager existing-directory initialization;
- LayerStack, Layer, Branch, Commit, Workspace, or Execution semantics;
- Store uniqueness and exact same-ID/different-byte collision rejection;
- hard-link, rename, mode, mtime, symlink, and open-unlink behavior;
- visibility-last Layer and LayerStack publication;
- SDK, CLI, daemon, proxy, or FUSE contracts except compatible additive
  internal behavior already proved by the roadmap;
- one Store operation gate and one SQLite admission owner;
- exact reconnect and cleanup proof outside `layerstack_init_ns` but retained
  inside the same sample's complete-product clock.

Independent public initializations are not expected to produce the same root
from source bytes alone: each allocates a new LayerStack identity, and the
inode-allocation seed derives from that identity. Canonical equivalence means
the same source metadata and bytes plus the same LayerStack-derived seed
produce the same final reachable canonical state. It does not mean v0 and v2
leave an identical physical Store population, because v2 deliberately omits
obsolete unreachable intermediate objects.

The code audit also found an apparent pre-release boundary that is not covered
by the current fixture: the direct path creates one compact-pair task block per
top-level directory, while the stream rejects more than 1,000 blocks. The
largest fixture has exactly 1,000 top-level directories. Until a focused test
proves otherwise, an otherwise eligible 1,001-directory source may enter
direct admission and fail instead of selecting the existing fallback. The
minimal intended resolution is an eligibility preflight before any direct
admission; this document records the finding but does not claim it is fixed.

## Current evidence and remaining release conditions

The retained v2 path has already achieved:

```text
object-segment writes/reads: 0
parent payload copies:       0
metadata cache:              exact and bounded
direct producer ceiling:     8
queue:                       4 fixed slabs
modeled named buffers:       <=10 MiB
selected init medians:       pass at all four tiers
selected Create medians:     pass at all four tiers
selected Commit medians:     pass at all four tiers
CPU / HWM / swap / OOM:      pass
```

At 100,000 files, the selected current-seal row is:

```text
initialization:           2.766279583 s
logical throughput:       180.748180 MB/s
file rate:                36,149 files/s
Workspace Create:         12.555708 ms
localized Commit:         3.856541 ms
binding outcome:          pass for each of those medians

preferred outcome:        <=2.500 s / >=200 MB/s / >=40,000 files/s
preferred wall gap:       approximately 266 ms

stretch outcome:          <=2.000 s / >=250 MB/s
```

The selected evidence is a same-source-seal combination, not a rewritten raw
campaign. The all-tier `r003` report retains its 15.223-ms 100-file Create miss
and therefore its `performance_pass=0` and `evidence_pass=0` markers. The
supplemental 100-file report provides a 14.742-ms median on the same source and
harness seals; it does not duplicate the runner-owned composite proof. The
documents may present the selected rows together only when both evidence
directories and this qualification remain visible.

Current selected resource maxima are 12.968067459 initialization CPU-seconds,
103,006,208 bytes of initialization incremental process high-water,
208,388,096 bytes of complete-lifecycle incremental high-water, and 10,101,246
bytes of modeled named buffers. The SQLite connection-cache target remains
33,554,432 bytes. Swap, OOM, large canonical-object-segment traffic, and parent
payload copies are zero. The direct path still writes and rereads a 6,464,000-
byte compact inode-pair stream and the durable Store grows by roughly 662 MB;
zero object-segment traffic is not a claim of zero temporary or durable I/O.

The remaining release work is not another performance experiment. It is to
resolve or explicitly contain the apparent greater-than-1,000 top-level task
boundary, reconcile the split selected evidence with the admission contract,
freeze a clean immutable release source, and rerun any comparison that will be
attributed directly to the released binary. The preferred 200-MB/s and stretch
250-MB/s outcomes remain visible misses rather than release blockers.

The remaining performance costs are the
100,000 POSIX file operations, approximately 422,000 unique canonical Store
objects, the global inode structure, and SQLite B-tree writes. Further major
reductions would then require one of the explicitly excluded representation or
semantic changes and belong outside v0.1.1.

## Evidence and implementation map

- [v0.1.1 roadmap and checklist](README.md)
- [Retained baseline and architecture evidence](baseline-2026-09-02.md)
- [Namespace-v2 optimization specification](namespace-optimization-spec.md)
- [Namespace-v2 execution handoff](namespace-v2-handoff-prompt.md)
- [Benchmark contract](../benchmarking.md)
- [Benchmark harness](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Namespace runner](../../../../benchmark/fs-bench-pro/run-namespace.sh)
- [LayerStack initialization implementation](../../../../crates/layerfs-layerstack-store/src/layerstack.rs)
- [Object buffering and admission implementation](../../../../crates/layerfs-layerstack-store/src/objects.rs)
- [Canonical inode-table construction](../../../../crates/layerfs-content/src/tree/inode/table.rs)
- [Canonical file construction](../../../../crates/layerfs-content/src/file/rope/build.rs)
- [Workspace Store access](../../../../crates/layerfs-layerstack-store/src/workspace.rs)
- [Real-FUSE proof](../../../../crates/layerfs-sdk/tests/live_fuse.rs)
- [Managed Docker and cleanup proof](../../../../crates/layerfs-sdk/tests/live_docker.rs)

Retained raw-report identities cited here:

```text
v0 baseline:
benchmark-results/fs-bench-pro/namespace/
  issue6-v010-exploratory-r001-20260902
  issue6-v010-exploratory-r002-20260902

v1 audited results:
benchmark-results/fs-bench-pro/namespace/
  issue6-audit-optimal-final-n100-r001-20260902
  issue6-audit-optimal-final-n1000-r001-20260902
  issue6-audit-optimal-final-n10000-r001-20260902
  issue6-audit-optimal-final-n100000-r001-20260902

v2 retained nonterminal init-only screen:
benchmark-results/fs-bench-pro/namespace/
  issue11-v3-retained-init-all-r001-20260903

current-seal selected product evidence:
benchmark-results/fs-bench-pro/namespace/
  issue9-v3-final-create-100-r001-20260903
  issue11-v3-terminal-all4-composite-r003-20260903

shared full source seal:
  f6a2c969ca9245b0394c91643d6c24a2f56180975fad537c10fb5360358d4170
```
