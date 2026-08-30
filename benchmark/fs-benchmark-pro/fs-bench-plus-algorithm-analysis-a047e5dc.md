# LayerFS `fs-bench-plus` algorithm and wiring audit

Status: **review complete; implementation intentionally not started**

Audit timestamp: `2026-08-31T00:00:42+08:00` (Asia/Shanghai)  
LayerFS HEAD: `a047e5dc48483f5b8189e19470ebdda37d4b8840`  
HEAD subject: `feat: replace LayerFS with two-store V2`  
Measured smoke: `smoke-20260830-a047e5dc-03`  
Computer pin: commit `de87919a4fd37242e960e13b7b3ba802d1eef0a0`, tree `4fb409d7e1356e1098439293d77d2fdc2dbf2190`

In this repository the durable benchmark package is named
`benchmark/fs-benchmark-pro`. This report uses **fs-bench-plus** as the
user-facing name for that same durable create/edit/persist/reopen comparison.
It is not the older FUSE-only `benchmark/fs-bench` suite.

## 1. Scope and custody

This was a read-only algorithm and wiring audit performed before implementation.
Three independent reviews covered:

1. `layerfs-content` file, namespace, and canonical-object algorithms;
2. V2 storage, transfer, Push, completeness, batching, and transaction behavior;
3. the real FUSE-to-Workspace-to-Commit-to-Push path measured by
   `fs-benchmark-pro`.

No production source was changed by the audit. The only new file is this report.

There is no `crates/layerfs-store/src` directory in the current V2 tree. The
relevant storage implementation is split across:

```text
crates/layerfs-storage/src
crates/layerfs-branch-store/src
crates/layerfs-layerstack-store/src
```

The valid one-pair smoke was produced from the same HEAD and the current
production-crate bytes. Four benchmark-harness files changed after that sealed
smoke (`Dockerfile.computer`, `README.md`, `compare.py`, and `run.sh`), so the
smoke is valid diagnostic evidence for the production algorithms but is not a
formal current-harness population. The formal 30-pair campaign remains blocked
by the previously recorded Docker Desktop Debian ARM64 proxy failure; see:

```text
benchmark-results/fs-benchmark-pro/computer-sealed-build-blocker-20260830.txt
```

## 2. Executive verdict

The premise is substantially correct: LayerFS already has an efficient,
persistent, bounded-memory small-range content algorithm. The present
Workspace Commit path does not use it.

The storage side is more nuanced. CAS admission, deduplication, bounded spill,
owned-Commit-suffix selection, missing-only payload transfer, and visibility-last
publication already work. A complete previous-root-aware canonical dependency
delta walker does **not** yet exist. Existing paired tree diffs provide most of
the alignment logic, but they emit logical changes rather than the complete set
of new canonical objects required for transfer and authority verification.

The concise diagnosis is:

| Concern | Efficient primitive exists | Used by measured path | Verdict |
|---|---:|---:|---|
| Fixed-buffer FastCDC | Yes | Yes, but over the full final file | Algorithm good; caller wrong |
| Persistent file range replacement | Yes | No | Main Commit wiring defect |
| Multi-range `FileMutationBatch` | Yes | No | Reuse it |
| Batched inode and directory COW mutation | Yes | No, not as the Workspace final builder | Reuse it |
| CAS deduplication | Yes | Yes | Working extremely well |
| Missing-only object payload | Yes | Yes | Working extremely well |
| Owned local Commit suffix for Push | Yes | Yes | Correct |
| Operation-wide Seen set and disk spill | Yes | Yes | Correct and bounded |
| Previous-complete-root Push discovery | Partial diff primitives only | No | One new typed visitor is required |
| Previous-root-aware authority verification | No complete production path | No | Must be fixed with Push discovery |
| Ordinary Commit without FUSE teardown/remount | Current worker can plausibly support it | No | Must be proven and wired |
| Exact hot-path observability | Counters partly exist | No | Add before optimized claims |

The correct work is not a new chunker, object format, cache, schema, database,
refcount system, or overlayfs path. It is:

```text
reuse the existing range/inode/directory algorithms in Workspace
+ add one missing typed old-root/new-root dependency-frontier visitor
+ reuse the current bounded transfer/admission machinery
+ stop rebuilding the ordinary FUSE projection after every Commit
```

## 3. Current measured result

The valid diagnostic smoke compared the pinned upstream Computer implementation
with LayerFS Reference placement on the same Docker daemon and constrained
single-CPU Linux/ARM64 envelope.

### 3.1 Headline latency

| Workload | Computer durable | LayerFS authority checkpoint | LayerFS speedup |
|---|---:|---:|---:|
| Create 32 MiB | 2,589.713 ms | 2,660.925 ms | 0.973x |
| 16 separately durable 10-byte edits | 2,378.320 ms | 40,028.546 ms | 0.059x |
| 10-byte prepend | 2,600.951 ms | 1,710.925 ms | 1.520x |
| Full read and sync | 172.758 ms | 190.114 ms | 0.909x |
| Complete registered workload | 7,741.742 ms | 44,590.510 ms | 0.174x |

The initial create is approximately tied, prepend already favors LayerFS by
1.52x, and read is close. The entire loss comes from the separately durable
small-edit row.

### 3.2 Exact LayerFS edit decomposition

| Phase | Sixteen-edit total | Mean per edit | Share of authority checkpoint |
|---|---:|---:|---:|
| Real FUSE shell write plus `fsync` | 969.945 ms | 60.622 ms | 2.42% |
| Workspace Commit API | 35,754.761 ms | 2,234.673 ms | 89.33% |
| Push API | 3,303.841 ms | 206.490 ms | 8.25% |
| **Authority checkpoint** | **40,028.546 ms** | **2,501.784 ms** | 100% |

Computer averaged `148.645 ms` per durable edit. LayerFS already spends
`60.622 ms` in the real FUSE write/fsync portion, leaving only:

```text
148.645 - 60.622 = 88.023 ms/edit
```

for **Commit plus Push combined** merely to tie Computer.

For a 10% per-edit latency win, the combined Commit plus Push budget is:

```text
Computer target: 148.645 * 0.90 = 133.781 ms/edit
Commit + Push:    133.781 - 60.622 = 73.159 ms/edit
```

### 3.3 Exact Commit deduplication equation

Across the sixteen edits:

```text
candidate IDs       27,792 = 112 inserted + 27,680 reused
candidate bytes 538,576,944 = 396,146 inserted + 538,180,798 reused
```

Byte reuse was:

```text
538,180,798 / 538,576,944 = 99.9264%
```

Per edit the Workspace generated a 1,737-object, 33,661,059-byte candidate,
while only seven objects and 16,151–29,430 bytes were new.

This proves that the CAS and object identity design are not the small-edit
problem. The system discovers excellent deduplication only after unnecessarily
constructing, hashing, buffering, spilling, and admitting a full-file candidate.

### 3.4 Exact Push equation

Across the same sixteen edits:

```text
announced IDs        27,968
missing/sent IDs        112
preexisting IDs       27,856

announced bytes  538,593,088
missing/sent bytes     396,146
preexisting bytes  538,196,942

membership pages           432
payload batches              16
known roots pruned             0
```

Only `0.0736%` of announced bytes were actually sent. Missing-only payload is
working; root-closure discovery is not incremental.

### 3.5 Current physical storage result

At the comparable authority checkpoint:

| Metric | Computer | LayerFS Reference |
|---|---:|---:|
| SQLite database bytes | 72.34 MiB | 66.80 MiB |
| WAL bytes | 0 | 10.42 MiB |
| SHM bytes | 96 KiB | 64 KiB |
| Durable allocated bytes | 80.19 MiB | 80.19 MiB |

The final physical allocated result is a tie, not a demonstrated total-space
win. LayerFS retains immutable Commit history and deliberately places locally
committed objects in BranchStore and pushed objects in LayerStackStore.

The more useful semantic observation is the edit delta:

```text
LayerFS new canonical bytes per Store for 16 edits: 396,146 B
Computer fixed-chunk semantic growth for 16 edits: approximately 8 MiB
```

LayerFS therefore already has a strong semantic incremental-growth advantage,
but SQLite page allocation, WAL state, two durable placements, and different
retention semantics hide it in this small final allocated-footprint snapshot.

Direct final Store inspection after the excluded Add showed:

| Store/set | Object IDs | Canonical object bytes |
|---|---:|---:|
| BranchStore | 1,866 | 34,084,010 |
| LayerStackStore | 1,875 | 34,084,837 |
| Cross-Store intersection | 1,866 | 34,084,010 |
| Cross-Store union | 1,875 | 34,084,837 |
| Physical placement sum | 3,741 placements | 68,168,847 |

The unique canonical union is only about 1.58% above the final 33,554,442-byte
logical file despite immutable history. The roughly 2.03x semantic placement
sum is the intentional two-database placement, not failed object deduplication.

## 4. Content algorithm analysis

### 4.1 Existing FastCDC is streaming and bounded

`crates/layerfs-content/src/file/cdc/gear.rs` freezes:

```text
minimum chunk:   8 KiB
target chunk:   16 KiB
maximum chunk:  32 KiB
input buffer:   32 KiB
chunk buffer:   32 KiB
```

A full build is linear in input bytes but does not allocate a complete file.
That is the correct algorithm for a new file or genuine complete rewrite.

### 4.2 Existing range replacement is the right small-edit algorithm

`crates/layerfs-content/src/file/rope/edit.rs` implements this persistent edit:

```text
scan replacement bytes only
split the old extent tree at the edit start
split the remaining tree at the delete length
join shared old-left + new replacement + shared old-right
rewrite only changed boundary spines
publish a new FileStateRoot
```

Let:

```text
D = replacement bytes
B = extent-tree fanout, between 64 and 128 entries
H = tree height
```

The expected work is:

```text
CPU:                    O(D + B*H)
unchanged payload read: 0 bytes
new payload bytes:      O(D)
new structural bytes:   O(B*H)
```

For the 32 MiB fixture, the extent tree should normally be shallow. A ten-byte
overwrite is therefore small, bounded tree surgery, not a 32 MiB operation.

Existing tests prove:

- no unchanged payload reads during splice;
- fewer than 32 structural node reads in the relevant splice fixture;
- a seven-byte filesystem replacement scans exactly seven CDC bytes;
- retained old roots remain byte-exact.

### 4.3 Existing `FileMutationBatch` handles normalized multi-range edits

`FileMutationBatch` accepts sorted, nonoverlapping final ranges, retains only
final-reachable structural objects, and guarantees the same root as applying
the same sequence of direct `replace` operations.

Its structural scratch bounds are:

```text
prune watermark: 4 MiB
hard ceiling:     8 MiB - 1 byte
```

The existing 64 MiB streaming replacement test stays below that ceiling.

### 4.4 Existing namespace primitives are also persistent

The content layer already provides:

- `filesystem::replace_range`;
- `filesystem::apply_inode_mutations`;
- batched directory mutations;
- inode-preserving rename;
- hard-link mutation through one shared inode record;
- paired inode, directory, file-rope, and filesystem diffs that prune equal
  immutable IDs.

These should be the one canonical implementation. Workspace should translate
its overlay state into these operations instead of rebuilding a second
architecture from complete path manifests.

### 4.5 Correct canonical-root oracle

An incremental edit root must **not** be required to equal a from-scratch
`build(final_bytes)` root.

The file model is operation-history canonical. Range replacement chunks the
replacement bytes and reuses old extent slices; a full rebuild chunks the
complete final stream. Those representations can have different IDs while
representing the same final bytes.

The binding V2 rule is:

> Operation-history-equivalent final filesystems produce identical canonical roots.

The correct oracle is therefore:

```text
same base root
+ same deterministic normalized ordered range operations
+ same replacement bytes
= same root as sequential public range-replacement semantics
```

Tests must compare optimized Workspace output with that direct ordered-range
oracle, not with the current full-rebuild result.

## 5. Where Workspace defeats the algorithm

Workspace already records the necessary edit information:

```rust
FileData::Overlay {
    base: Option<(FileStateRoot, u64)>,
    spool: PathBuf,
    len: u64,
    dirty: BTreeMap<u64, u64>,
    charged: BTreeMap<u64, u64>,
}
```

Writes coalesce overlapping and adjacent dirty ranges. Payload lives in a
disk-backed sparse spool. Reads correctly merge base bytes, spool bytes, and
implicit zeros.

`Workspace::build_candidate`, however, discards the useful range representation:

```text
enumerate complete base manifest
enumerate complete final manifest
for a candidate file:
    hash the complete final file
    separately read and hash the complete base file
if different:
    stream the complete final file through write_file
    FastCDC the complete final file
    encode/hash a complete candidate graph
```

For file size `N`, current dirty-file construction is approximately:

```text
O(N) final-file read/hash
+ O(N) base-file read/hash
+ O(N) full rebuild/FastCDC
+ O(N / 16 KiB) candidate object work
```

This matches the measured `2.235 s` Commit and 33.66 MiB candidate for a
ten-byte overwrite.

`ObjectBuffer` then discovers that almost every generated object already
exists. Because the complete candidate exceeds 8 MiB, it also uses its
file-backed SQLite spill path. The candidate path is memory-safe but performs
unnecessary CPU, hashing, SQL lookup, and scratch I/O.

## 6. Correctness and safety issues found by the audit

These are acceptance blockers, not optional performance cleanup.

### P0: clean truncate-shrink can be lost

For a clean base file, `truncate(node, smaller_size)` creates an overlay,
changes `len`, and clips dirty ranges. If there were no earlier writes, the
dirty map remains empty.

`file_matches`, however, returns `true` for a matching base root plus an empty
dirty map without checking final length. Commit can therefore classify the
truncated file as unchanged.

Required invariant:

```text
final_len != base_len => changed
```

A clean shrink must be committed as an EOF deletion and survive process reopen.

### P0: Workspace can confuse old and newly created inode identity

Workspace nodes retain `canonical: Option<InodeId>`, but `build_candidate`
matches final groups to base groups using paths, kind, and group shape without
requiring that the final node still owns the base canonical inode.

Consequences visible from the code include:

- unlink plus recreate at the same path can be treated as an update to the old
  inode;
- pure rename becomes remove plus recreate/full file build;
- hard-link add/remove causes a full group rebuild;
- a temporary-file rename-over can lose the new node identity;
- directory rename can rebuild a complete subtree.

The terminal Workspace builder must preserve canonical inode identity for
existing nodes and assign deterministic creation identities to genuinely new
nodes. Incidental lazy lookup order must not affect those identities.

### P0: root-object presence is not a completeness receipt

`PushRootRequests` currently maps target root-row presence to
`known_complete`. V2 explicitly says that a complete-root receipt is an
authenticated full-closure claim and must not be inferred from the root object
alone.

Final authority verification currently prevents silent publication of an
incomplete closure, but the signal is still structurally wrong and can turn an
interrupted residue into a late verification failure instead of repairing it.

Only an authority-visible, previously verified Layer or Commit root may be a
trusted delta baseline.

### P1: dirty interval metadata is not charged

The Workspace policy charges materialized spool bytes but not the in-memory
`dirty` and `charged` `BTreeMap` entry overhead. Many disjoint one-byte writes
can grow metadata independently of the one-GiB spool limit and eight-MiB
final-delta limit.

The smallest safe correction is to conservatively charge both maps, including
temporary clone/rollback overhead, against `max_final_delta_memory_bytes`.
Only add a file-backed interval table if a measured legitimate workload later
needs more intervals.

### P1: zero writing has a request-sized allocation path

When a zero write overlaps a materialized spool interval, Workspace uses
`vec![0; byte_len]`. A large request can therefore allocate request-sized RAM.
It should write a fixed zero block repeatedly.

### P1: ordinary Commit includes projection teardown and recreation

A successful normal FUSE Commit reloads the Workspace, destroys the old
projection/proxy/helper, creates a new projection, and remounts read-only.
The Commit receipt does not split this lifecycle from candidate construction.

For an ordinary non-reconciliation Commit, the existing mounted view already
dereferences the Workspace behind the worker. Replacing the committed Workspace
state and transitioning the same projection read-only appears sufficient, but
kernel-cache correctness must be proven. Keep refresh as a fallback for
materialization, reconciliation results that differ from the visible tree,
failed reload, or an unproven cache state.

### P1: monitoring cannot yet prove the optimized mechanism

`BuiltRoot` contains `cdc_bytes_scanned` and a candidate-count-derived
`encode_hash_invocations`, but these are not published in the local admission
receipt. Current receipts also do not split candidate build, admission,
verification, Workspace reload, and projection refresh.

Before optimized claims, add exact counters for:

```text
Workspace:
  manifest paths visited
  dirty intervals and dirty bytes
  final/base bytes compared
  overlay/base bytes read
  CDC bytes scanned
  payload bytes constructed
  candidate/inserted/reused IDs and bytes
  FileMutationBatch deferred peak/prunes
  ObjectBuffer peak/spill

Commit timing:
  pause/quiesce
  candidate build
  local admission
  completeness verification
  Commit/head CAS
  Workspace reload
  projection transition

Push:
  source objects and canonical bytes traversed
  traversal authentications
  equal subtrees pruned
  membership pages
  announced/missing/sent IDs and bytes
  Seen spill and transfer-buffer peak
```

Do not reinterpret `known_roots_pruned` as equal-subtree pruning; today it
counts only complete top-level root requests.

## 7. Storage and Push analysis

### 7.1 What already works

The following production mechanisms should be retained:

| Mechanism | Current implementation |
|---|---|
| Operation-wide union deduplication | spillable `SeenIds` |
| Candidate-object spill | `DeferredObjectStore` |
| Final-reachable candidate filtering | `reachable_from` |
| Postorder transfer | `ObjectTransfer::visit` |
| Bounded ID membership | 512-ID pages |
| Bounded payload admission | 128 objects or 4 MiB |
| Bounded fact admission | 128 facts or 64 KiB |
| Bounded transfer memory | strictly below 34 MiB |
| Large-history handling | disk-backed `FactSpool` |
| Push ownership boundary | `owned_commit_page` suffix only |
| Publication order | objects, facts, then Branch head |

The current transaction separation is sound:

- object/fact validation happens before bounded SQLite writer transactions;
- history enumeration, network calls, hashing, and closure traversal happen
  outside publication transactions;
- authority name/head publication is one final small CAS transaction.

### 7.2 Why Push still scans everything

`RootTransferRequest` carries `root_id` and `known_complete`, but no trusted
previous complete root.

For a new Commit root, the authority does not have the root object, so generic
transfer walks the complete dependency closure. It announces every child and
sends only missing ones.

`SnapshotReader::prune_existing_subtree` prunes only authority-fallback objects
that are absent locally. Reused objects created or stored locally are present
in BranchStore, so the source descends through them even when the authority
already has them.

After transfer, the LayerStackStore verifies every new suffix root with another
complete closure walk before final publication. The small edit therefore pays:

```text
full sender closure read/authentication/membership discovery
+ full receiver closure read/authentication verification
+ missing-only payload admission
```

The payload term is small; both closure terms are file-sized.

### 7.3 Existing diffs are necessary but not sufficient

The content crate already has paired persistent diffs for filesystem roots,
inode tables, directories, file ropes, and metadata structures. They correctly
cut equal ObjectIds and use bounded cursors for different partitions/heights.

They emit logical paths, inode entries, directory entries, or byte ranges. They
do not emit every new canonical object required to reconstruct the new root.

There is no drop-in function equivalent to:

```rust
visit_new_dependency_frontier(old_complete_root, new_root, visitor)
```

`dependency_order` must not be used; it materializes a complete closure in an
unbounded `Vec`. `collect_dependency_set` is spill-safe but still scans the
complete old closure and therefore does not solve the CPU problem.

### 7.4 Required typed dependency-frontier visitor

Add one streaming visitor in `layerfs-content` with this proof rule:

```text
old root is authority-visible and previously verified complete

if old ObjectId == new ObjectId:
    prune the immutable subtree without reading descendants
else:
    authenticate the new object
    pair old/new typed structure
    recurse through new or unequal children
    emit required new objects in postorder
```

Required typed alignment:

| Canonical role | Alignment rule |
|---|---|
| Namespace root | Pair inode-table roots |
| Inode table | Align persistent nodes by inode key/range |
| Inode record | Pair content and metadata roots by inode kind |
| Directory state/tree | Align name ranges and entries |
| File state/extent tree | Align logical spans and equal extent IDs |
| Metadata tree | Align metadata keys/ranges |
| Chunk/symlink leaf | Emit new leaf; no descendants |
| Incompatible shape/type | Full traversal of the new subtree |

Memory must be only:

```text
O(tree depth)
+ bounded child page
+ current spillable operation Seen set
+ current bounded payload batch
```

No complete old/new closure set and no schema table are required.

### 7.5 Every intermediate Commit root matters

For multiple unpushed Commits, Push must process every adjacent root transition
in ancestry order:

```text
trusted authority/origin root -> C1 root
C1 root -> C2 root
...
Cn-1 root -> Cn root
```

Comparing only the authority root with the final head can omit an object that
is reachable from an immutable intermediate Commit and deleted by the final
Commit. The current owned-suffix fact spool is already correct; the new object
visitor must preserve the same complete-history obligation.

The authority must validate the same adjacent proof chain before the final
head CAS. Optimizing only sender traversal leaves the second full verification
walk and will not make Push genuinely incremental.

## 8. Correct implementation path

The order matters because the audit exposed correctness defects and because
wall time alone cannot prove which mechanism improved.

### Stage 0: freeze proof and expose internal phases

Add the mechanism counters and timing split listed above. Freeze acceptance
ceilings from a correctness fixture before rerunning performance.

Do not change object identity, schema, or CDC for instrumentation.

### Stage 1: repair correctness and resource bounds

1. Add a failing clean truncate-shrink Commit/reopen test.
2. Make final length authoritative in same-file classification.
3. Make base matching require the exact `Node.canonical` relationship.
4. Add unlink/recreate, rename, hard-link, and rename-over inode-identity tests.
5. Charge dirty/charged interval metadata against the final-delta policy.
6. Replace request-sized zero allocation with a fixed streaming zero buffer.

### Stage 2: wire existing-file dirty ranges to `FileMutationBatch`

For an existing file whose canonical inode relationship is proven:

```text
base FileStateRoot
+ base and final length
+ sorted/coalesced dirty intervals
+ bounded Workspace range reader
    -> FileMutationBatch
    -> new FileStateRoot
    -> one shared inode-record upsert
```

Mapping rules:

| Workspace mutation | Range operation |
|---|---|
| Equal-length overwrite | Replace the dirty interval with final interval bytes |
| Several writes | Apply sorted/coalesced intervals in one batch |
| Append | Insert the final tail at old EOF |
| Write past EOF | Replace old overlap, then insert zero/materialized tail |
| Truncate shorter | Apply surviving dirty intervals, then delete old EOF tail |
| Truncate longer | Insert streamed zero/materialized tail |
| Same-byte write | Compare only dirty intervals and emit no content mutation |
| Metadata-only change | Preserve content root |
| Exact hard-link group | Update the shared canonical inode once |
| New file/new replacement inode | Keep one full streaming build |

The range reader must implement `Read` for `[start,end)` and fill caller-owned
fixed buffers. No dirty range becomes a range-sized `Vec`.

Use the existing `ObjectBuffer`. Candidate construction remains outside durable
writer transactions, stays memory-first below 8 MiB, and spills for genuine
large candidates.

Use a deterministic fallback policy based on total dirty bytes, interval count,
file length, and tree height. A dense rewrite may correctly choose one full
streaming build. The registered ten-byte edit must not silently fall back.

### Stage 3: remove full equality passes and ordinary FUSE refresh

Share one range-aware equality implementation between `build_candidate` and
Workspace cleanliness checks:

```text
same base root + same length + no dirty intervals => unchanged
different length => changed
dirty intervals => compare only those intervals in fixed blocks
```

After an ordinary Commit whose candidate exactly represents the visible FUSE
tree, keep the mounted worker/proxy/helper and transition it read-only after
reloading the Workspace state. Retain the existing refresh path as a safe
fallback for cases requiring a different visible tree or unproven cache
invalidation.

### Stage 4: add typed previous-root-aware Push and authority verification

For each owned Commit, select the trusted baseline:

```text
parent Commit root, when present
otherwise the Commit base Layer root
```

The first baseline must terminate at the observed authority head, immutable
fork boundary, or authority Layer. Arbitrary object presence is not trust.

Run the typed dependency-frontier visitor on every adjacent transition, feed
its postorder new-object stream into the current membership/Seen/payload
pipeline, then send facts and publish visibility last.

Run the same delta-completeness proof at the authority before publication.
Retain a full-new-subtree fallback for incompatible shapes or no trusted
baseline.

### Stage 5: replace complete manifests with Workspace namespace deltas

The narrow existing-file fast path fixes the benchmark, but it is not the
terminal Workspace architecture. The complete `base_manifest`/`final_manifest`
hot path remains `O(number of paths)` and mishandles inode identity for several
namespace mutations.

Translate existing Workspace state directly:

```text
Node.canonical
+ DirectoryData.base/changes
+ FileData.base/dirty
+ node path/link groups
    -> changed content/metadata roots
    -> batched persistent directory mutations
    -> one bounded inode mutation batch
    -> one namespace root
```

Existing canonical inode IDs must survive rename and hard-link changes. New
inode identity must derive from frozen semantic creation history, not incidental
lookup or node allocation order.

Only after this stage should Workspace wiring be called structurally terminal.

## 9. CPU and memory safety after optimization

The optimization does not require a whole-file memory cache.

| Component | Existing/required bound |
|---|---:|
| FastCDC input plus current chunk | approximately 64 KiB |
| `FileMutationBatch` deferred structural objects | `< 8 MiB` |
| `ObjectBuffer` candidates | approximately 8 MiB, then file SQLite spill |
| Dirty/charged interval metadata | add explicit `<= 8 MiB` accounting |
| Directory/inode deferred batch | `< 8 MiB` while active |
| Workspace payload | disk-backed sparse spool |
| Transfer ID page | 512 IDs |
| Transfer object batch | 128 objects or 4 MiB |
| Transfer fact batch | 128 facts or 64 KiB |
| Seen IDs | approximately 8 MiB, then file SQLite spill |
| Combined active transfer buffer | strictly `< 34 MiB` |
| History | disk-backed fact spool |

Candidate construction and Push are sequential operations, so these maxima are
not one mandatory simultaneous allocation. Actual RSS additionally includes
SQLite and kernel caches and must be measured.

CPU remains safe in the worst case:

- a ten-byte overwrite performs dirty-byte plus tree-spine work;
- many disjoint edits use the bounded batch until a deterministic threshold;
- a dense or incompatible rewrite falls back to one full streaming build;
- an unrelated Push root falls back to full-new-subtree traversal;
- no correctness proof is weakened to obtain a fast path.

## 10. Expected `fs-bench-plus` result

These are engineering expectations and acceptance budgets, not measured
post-fix results.

### 10.1 Mechanism expectation for one ten-byte overwrite

| Work | Current | Correctly wired expectation |
|---|---:|---:|
| Final/base comparison | 32 MiB final + 32 MiB base | At most the dirty interval from each side |
| CDC scan | 32 MiB | Exactly 10 bytes |
| Commit candidate | 1,737 IDs / 33.66 MiB | Single digits to low tens / KiB to low tens of KiB |
| Push discovery | 1,748 IDs / 33.66 MiB | Changed canonical frontier only |
| Membership pages | 27 | One to single digits; target `<= 1` for this fixture |
| Push payload | 7 IDs / 16–29 KiB | Same order; already missing-only |
| Full-file dependence | Linear in file size | Approximately independent of file size |

A useful preregistered acceptance envelope is:

```text
cdc_bytes_scanned == 10
candidate IDs < 128
candidate bytes < 256 KiB
Push changed-frontier membership pages <= 1
Push sent bytes == exact authority-missing frontier bytes
FileMutationBatch deferred peak < 8 MiB
transfer peak < 34 MiB
```

Freeze the exact object/byte ceiling from a correctness fixture before formal
measurement.

### 10.2 Latency expectation

The content algorithm makes a `10–80 ms` durable Commit an engineering target,
not a current proof. A typed delta Push makes `40–100 ms` an engineering target,
not a guarantee. SQLite `synchronous=FULL`, object/fact admission, the final
authority CAS, and the current projection lifecycle can dominate after scan
work disappears.

Combining the broad targets with the measured FUSE cost gives:

```text
60.6 ms FUSE
+ 10–80 ms Commit
+ 40–100 ms Push
= 110.6–240.6 ms per edit
```

For sixteen edits that is approximately `1.77–3.85 s`, versus Computer's
measured `2.378 s`. A LayerFS win is plausible but not guaranteed.

The non-negotiable acceptance budget is clearer than the forecast:

```text
tie Computer:     Commit + Push <= 88.0 ms/edit
10% edit win:     Commit + Push <= 73.2 ms/edit
```

An illustrative center case is:

```text
FUSE:    60.6 ms
Commit:  30.0 ms
Push:    40.0 ms
total:  130.6 ms/edit
```

That would make the edit row approximately `2.09 s`, about `1.14x` faster than
Computer. It remains a target until rerun.

### 10.3 Throughput interpretation

`500 MiB/s` is not a useful success metric for a ten-byte durable edit. The
correct mechanism requirement is that comparison, CDC, candidate, and transfer
discovery work stop scaling with the complete file size. Latency then consists
mostly of fixed FUSE, SQLite durability, and authority-publication costs.

For the operations that do move the full 32 MiB, the current end-to-end durable
rates are approximately:

```text
LayerFS create: 32 MiB / 2.661 s = 12.0 MiB/s
Computer create: 32 MiB / 2.590 s = 12.4 MiB/s

LayerFS read+sync: 32 MiB / 0.190 s = 168 MiB/s
Computer read+sync: 32 MiB / 0.173 s = 185 MiB/s
```

These include the registered durable/product boundaries. A higher number from
the older `fs-bench` FUSE-only overlay does not establish the same result,
because that suite excludes Workspace Commit, Push, authority checkpoint, and
process-reopen proof.

### 10.4 Final comparison-table projection

If the tiny-edit row merely ties Computer and the other measured LayerFS rows
do not change, the table would be:

| Workload | Computer measured | LayerFS target/projection | Projected LayerFS speedup |
|---|---:|---:|---:|
| Create 32 MiB | 2,589.713 ms | 2,660.925 ms | 0.973x |
| 16 durable edits | 2,378.320 ms | 2,378.320 ms | 1.000x |
| 10-byte prepend | 2,600.951 ms | 1,710.925 ms | 1.520x |
| Full read and sync | 172.758 ms | 190.114 ms | 0.909x |
| **Complete workload** | **7,741.742 ms** | **6,940.284 ms** | **1.115x** |

The arithmetic explains why the work is worthwhile: LayerFS does not need to
crush Computer on the edit row to win the complete registered workload, because
it already has a substantial prepend advantage.

At the broad engineering range, assuming the other rows remain fixed:

```text
LayerFS complete workload: approximately 6.33–8.41 s
Computer measured:          7.742 s
```

That spans an approximate `1.22x` win to a `0.92x` result. Only a fresh paired
campaign can choose the outcome.

### 10.5 Storage expectation

Push-only delta wiring, with current Commit roots held fixed, changes no
persistent object IDs or bytes. For the recorded sixteen edits it would still
insert exactly `112 objects / 396,146 bytes` at the authority; it would only
avoid full-root announcement, reading, hashing, and verification.

Commit range wiring can produce a different operation-history canonical root.
A ten-byte replacement stores a tiny replacement payload but may add extent
slices and structural nodes. Repeated edits can fragment the rope. The safe
expectation is:

```text
single-digit to low-tens new objects per edit
KiB to tens-of-KiB new canonical bytes per edit
no full-file-sized retained amplification
```

The exact persistent result can be lower, similar, or modestly higher than the
current 16–29 KiB per edit and must be remeasured.

Do not promise a dramatic reduction in final allocated directories. The likely
LayerFS superiority is in:

- semantic bytes added per tiny edit;
- candidate write amplification;
- object discovery/read/hash work;
- transfer announcement work;
- missing-only wire payload when the endpoint is remote.

Final SQLite allocation can remain tied because of page granularity, WAL,
immutable history, and two intentional placements.

## 11. Required acceptance matrix

### Content and Commit correctness

- one-byte and ten-byte overwrite at start, middle, and end;
- same-byte write returns `UpToDate` without full-file scanning;
- adjacent, overlapping, separated, and out-of-order writes normalize
  deterministically;
- optimized root equals the direct ordered-range oracle;
- do not compare it with a from-scratch build root;
- append, sparse write past EOF, truncate shorter, truncate longer, truncate
  then rewrite, and zero writing;
- retained old Commit remains byte-exact;
- content plus metadata publish atomically;
- registered ten-byte case never takes the full-file fallback.

### Namespace identity

- write through one hard-link alias updates every alias once;
- pure rename preserves inode and avoids content rebuild;
- hard-link add/remove preserves source inode;
- unlink/recreate at the same path gets the intended new identity;
- temporary-file rename-over preserves replacement identity;
- directory rename preserves descendant identities;
- metadata-only change never rebuilds file content.

### Push completeness

- one tiny edit announces only the changed frontier;
- equal-subtree prune counter is positive after the first edit;
- repeated Push sends zero;
- multiple unpushed Commits retain every intermediate root;
- an object created in C1 and deleted in C2 is still transferred for C1;
- pulled ancestry is never retransmitted;
- interruption after objects but before facts/publication remains invisible and
  retryable;
- HeadMoved preserves authority state;
- arbitrary root-object presence never becomes completeness;
- sender and authority both avoid full closure scans under a trusted baseline;
- no network, history, hashing, or closure walk occurs inside SQLite writer
  transactions.

### Memory and scale

- ten-byte edit over 4 MiB, 32 MiB, 256 MiB, and 1 GiB has approximately flat
  controlled memory and candidate work;
- 10,000 disjoint one-byte writes hit the declared interval limit or spill;
- `FileMutationBatch` peak remains below 8 MiB;
- `ObjectBuffer` spills after its 8 MiB bound;
- Seen IDs spill in fixed pages;
- transfer buffer remains below 34 MiB;
- no complete old/new closure collection appears in memory.

### Failure and durability

- base read, spool read, candidate spill, bounded admission, fact publication,
  pointer CAS, and projection-transition failures;
- old visible head and old Commit remain readable;
- Workspace final state remains available for retry where the lifecycle
  requires it;
- immutable unreachable residue is permitted but never visible;
- a fresh process reopens the acknowledged root and reproduces exact final
  size and SHA-256.

## 12. Verification performed during the audit

All executed focused tests passed:

```text
cargo test -p layerfs-content
  97 passed, 0 failed

cargo test -p layerfs-workspace
  11 passed, 0 failed

cargo test -p layerfs-storage --all-features \
  multi_root_transfer_and_receipts_share_one_operation_state -- --exact

cargo test -p layerfs-storage --all-features \
  transfer_is_postorder_and_rejects_a_nested_payload_over_the_buffer_ceiling -- --exact

cargo test -p layerfs-storage --all-features \
  expensive_validation_precedes_every_publication_write -- --exact

cargo test -p layerfs-branch-store --all-features \
  branch_pull_is_read_only_complete_history_and_push_sends_only_owned_suffix -- --exact

cargo test -p layerfs-layerstack-store --all-features \
  an_advance_verifies_only_the_new_owned_suffix -- --exact
```

The smoke durability oracle passed for both implementations:

```text
final bytes:  33,554,442
final SHA-256: 7b86abcd0e9d2016bbb8b16722e1439475feff84e31fe9801a4ec74e99dc74c3
process reopen: PASS
```

The current suite has no regression that catches the clean truncate-shrink
classification defect or the same-path recreate inode-identity defect. Passing
the existing suite therefore does not clear those P0 findings.

## 13. Raw evidence

```text
benchmark-results/fs-benchmark-pro/smoke-20260830-a047e5dc-03/comparison.md
benchmark-results/fs-benchmark-pro/smoke-20260830-a047e5dc-03/comparison.json
benchmark-results/fs-benchmark-pro/smoke-20260830-a047e5dc-03/manifest.json
benchmark-results/fs-benchmark-pro/smoke-20260830-a047e5dc-03/pairs/001/computer-upstream/summary.json
benchmark-results/fs-benchmark-pro/smoke-20260830-a047e5dc-03/pairs/001/layerfs-reference/summary.json
benchmark-results/fs-benchmark-pro/smoke-20260830-a047e5dc-03/pairs/001/layerfs-reference/layerfs-reference.jsonl
benchmark-results/fs-benchmark-pro/smoke-20260830-a047e5dc-03/pairs/001/layerfs-reference/layerfs-reference-store/branch.sqlite.runtime/monitor/operations.jsonl
benchmark-results/fs-benchmark-pro/computer-sealed-build-blocker-20260830.txt
```

## 14. Final recommendation

Proceed with implementation, but keep the patch on the existing architecture:

1. fix truncate and canonical-inode correctness first;
2. expose exact phase and mechanism counters;
3. wire `FileMutationBatch` plus batched inode mutation for existing-file edits;
4. retain the ordinary FUSE mount when correctness permits;
5. add one typed streaming previous-root dependency-frontier visitor and use it
   for both Push discovery and authority verification;
6. finish by replacing full Workspace manifests with canonical directory/inode
   deltas;
7. rerun scale probes, the one-pair smoke, then the formal paired population.

Do not add a third database, per-Commit object-membership table, full-file RAM
cache, new content format, new CDC profile, object refcounts, overlayfs path, or
parallel benchmark architecture for this fix.

The efficient content algorithm is already present. The largest savings come
from routing the product through it and stopping complete-root work after an
immutable equal-ID proof.
