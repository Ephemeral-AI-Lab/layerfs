# Phase 4 algorithm-complexity analysis

Status: WP4-C companion analysis; candidate format, not compatibility authority

Date: 2026-08-17

Scope: CAS + CDC + COW + canonical persistence, SQLite integration, reads,
writes, reconstruction, and future native materialization

## 1. Purpose and authority

This record states the time, resident-memory, and durable-space complexity of
the candidate in `PHASE_4_LOGICAL_PERSISTENCE_MAPPING.md`. It distinguishes:

- unavoidable semantic work from removable implementation amplification;
- full capture/scrub/materialization from incremental and range operations;
- peak live allocation from cumulative streamed work;
- the current fixed-radix candidate from a promoted final format; and
- asymptotic bounds from measured throughput.

This document grants no format or compatibility authority. The file constants
`K` and `F` and the directory page ceiling remain WP4-M measurement candidates.
WP4-P may freeze exactly one winning profile only after the required 100-MiB
and 512-MiB comparisons. Until then, Big-O notation describes the candidate
family and the exact provisional profiles; it is not evidence of 200 or
300 MiB/s.

The controlling invariants remain:

- Phase 1 canonical `Object::Bytes` and `Object::Directory` identity;
- Phase 2 CDC at 8/16/32 KiB minimum/target/maximum;
- Phase 3 COW, root, delta, and ordered mutation semantics;
- complete canonical authentication of every object actually fetched;
- immutable CAS objects and atomic visible-head publication;
- checked `u64` sizes, counts, offsets, and cumulative work;
- streaming operation with bounded resident memory; and
- SQLite as the authoritative Phase 4A disk engine.

The deleted append-only/packed carrier is not an alternative in this analysis.

## 2. Notation

| Symbol | Meaning |
|---|---|
| `S` | raw source or reconstructed payload bytes for one file |
| `S_u` | total raw bytes in unique chunks newly stored in CAS |
| `N` | ordered CDC chunk-reference occurrences in one logical file |
| `U` | unique canonical chunk objects among those `N` occurrences |
| `K` | maximum references in one file leaf |
| `F` | maximum children in one file branch or root |
| `P = ceil(N/K)` | number of file-reference leaves |
| `H` | number of branch levels between the file root and a leaf |
| `R_b` | raw bytes returned by one range request |
| `C_v` | complete chunk-object bytes authenticated for one range |
| `B_v` | maximum complete canonical bytes in one visited branch/root object |
| `L_v` | maximum complete canonical bytes in one visited reference leaf |
| `V_b` | branch/root object occurrences visited by one range |
| `V_l` | reference-leaf object occurrences visited by one range |
| `X_b` | raw bytes in an edited or appended CDC/resynchronization region |
| `X_c` | changed/new chunk-reference occurrences after CDC resynchronization |
| `L_c` | adjacent reference leaves changed by one contiguous edit |
| `Z` | reference occurrences in a suffix invalidated by fixed-ordinal repacking |
| `d` | logical namespace depth of the changed node, at most 256 |
| `E` | entries in one logical directory, at most 100,000 |
| `B_d` | complete canonical directory-page ceiling |
| `P_d` | directory entry-page count |
| `T` | ordered entries in one delta, at most 100,000 |
| `V` | strong-edge occurrences visited by a closure operation |
| `A` | complete canonical bytes authenticated by an operation |
| `Q` | peak live mapping-owned allocation |
| `W` | cumulative checked work/input bytes; not resident memory |
| `D` | cumulative decoded/output bytes; not resident memory |
| `J` | native filesystem entries produced by workspace materialization |
| `B_sql` | bounded number of rows handled by one later SQL batch |

For a nonempty file, branch counts are derived by:

```text
P  = ceil(N / K)
B1 = ceil(P / F)       when P > F
B2 = ceil(B1 / F)      while the preceding count is > F
...
Bh <= F
```

The exact candidate branch height is the smallest `H` satisfying
`P <= F^(H+1)`, equivalently:

```text
H = max(0, ceil(log_F(P)) - 1) for P > 0
```

`K`, `F`, canonical object maxima, and streaming-window maxima are bounded
format or operation constants. As `N` grows with those constants held fixed,
`H = Theta(log_F(N/K)) = O(log N)`. Candidate comparison changes constants and
physical I/O, not the asymptotic class.

## 3. Executive complexity matrix

| Area | Time | Peak mapping memory | Live durable space | Improvement status |
|---|---:|---:|---:|---|
| New-file CDC capture | `Theta(S)` plus `O(N)` object/row work | bounded chunk/object/page windows | `Theta(S_u + N)` | Same necessary class; constants remain optimizable |
| Canonical encode + identity | `Theta(canonical bytes)` | `O(max object)` or smaller when streamed | included in CAS/mapping | Same necessary class; duplicate passes removable |
| Existing-object immutable reuse | `Theta(object bytes)` per unaided authentication | `O(max object)` | `O(1)` new live bytes when reused | Same security lower bound; a future exact operation-local locator receipt may remove duplicate backend reads |
| File mapping construction | `Theta(N)` | `O(K + F*H)` working records | `Theta(N)` | Streaming and scalable; no giant manifest |
| One-leaf range with valid receipt | `O((H+1)*B_v + L_v + C_v + R_b)` | `O(max(B_v,L_v) + one chunk/output window + H)` | none | Improved from `O(N)` or `O(N/K)` mapping work |
| Same-count file edit | `O(X_b + X_c + K*L_c + F*(L_c+H))` | bounded edit/CDC/page windows | canonical bytes of `O(L_c+H)` new mapping objects plus new chunks | One-leaf case improves mapping work toward `O(log N)` |
| EOF append | `O(X_b + X_c + K + F*H)` | bounded edit/CDC/page windows | proportional to new chunks/leaves plus spine | Path-local apart from new data |
| EOF truncate with authenticated base | `O(H + K)` mapping work after locating the cut | bounded path/page windows | one rewritten leaf/spine; old CAS objects remain | Path-local; no GC implied |
| Early/middle count-changing edit | `O(X_b + Z)` worst case | bounded windows | `O(Z)` unreachable/new mapping history | Not asymptotically fixed; mandatory rejection gate |
| Fast unchanged reopen | `O(1)` fixed head/receipt/root work, then lazy access | bounded fixed state | none | Improved from full-closure replay |
| Fresh full closure scrub | `Theta(A + V)` | bounded object/spool/active stack | none | Intentionally not reduced |
| Full streamed reconstruction | `Theta(A + S + N)` | bounded object/spool/output windows | none | Same necessary time, bounded memory |
| Clean native materialization | `Theta(A + S + J)` | bounded traversal/output plus native operation state | `Theta(S + J)` destination | Future work; payload write lower bound remains |
| Incremental native materialization | target `O(changed paths + changed bytes + changed mapping paths)` | bounded per changed path/file | proportional to destination changes | Enabled by roots/deltas; not implemented by WP4 |
| Directory create/encode | `Theta(total encoded entry bytes)` | `O(B_d + index builder/spool)` | `Theta(total encoded entry bytes)` | Bounded pages prevent giant resident object |
| Directory same-size child replacement | `O(sum_i(page_i + index_i + wrapper_i))` over `d` ancestors | `O(B_d + index window)` | one page, index, and wrapper per ancestor | Large constant reduction; current core clone may remain `O(E)` |
| Directory leading count-changing insert | `O(E)` worst-case greedy suffix repack | bounded page/index windows | `O(E)` mapping history in worst case | Known fixed-partition weakness |
| Delta encode/replay | `Theta(total encoded delta bytes)` plus referenced COW work | bounded page/spool state | `Theta(total encoded delta bytes)` | Necessary ordered work; no sort/dedup shortcut |

## 4. CDC complexity

### 4.1 Initial or full replacement capture

CDC must inspect each input byte to determine canonical boundaries:

```text
T_cdc(S) = Theta(S)
M_cdc    = O(MAX_CHUNK_BYTES + rolling-state)
         = O(32 KiB + constant state)
```

Fragmentation-independent CDC cannot be sublinear for a previously unseen
source. A 100-GiB source therefore requires `Theta(100 GiB)` byte inspection,
but it does not require source-sized resident memory.

### 4.2 Small edit and resynchronization

When the base is authenticated and the Phase 2 edit path can prove an exact
rejoin, the intended work is:

```text
T_edit_cdc = O(X_b)
M_edit_cdc = O(MAX_CHUNK_BYTES + bounded rejoin state)
```

`X_b` includes the changed bytes and the bounded bytes inspected before exact
CDC resynchronization. If no safe rejoin is found or the caller supplies only
a complete replacement stream, the honest fallback is `Theta(S)`. Reusing
unchanged chunk identities must not bypass base authentication or rejoin
validation.

### 4.3 What can improve

CDC's Big-O class cannot improve below `Theta(bytes inspected)`. Useful work is
constant-factor work only:

- one streaming pass;
- reuse of fixed buffers;
- no source-sized staging;
- no byte copying between unnecessary intermediate vectors; and
- exact measurement of inspected, reused, and rejoined bytes.

## 5. Canonical encoding and ObjectId hashing

For canonical object size `b`:

```text
T_encode(b) = Theta(b)
T_hash(b)   = Theta(b)
M_hash      = O(1) hasher state
```

If encoding first creates a complete `Vec`, peak encoder memory is `O(b)`.
A streaming prefix/body encoder can reduce auxiliary memory toward `O(1)` plus
the bounded input/output windows, but the resulting canonical bytes and hash
must remain identical.

For a complete new-file capture:

```text
T_raw_chunk_hashes      = Theta(S)
T_canonical_chunk_hashes = Theta(S + 13*N)
T_mapping_hashes         = Theta(M(N))
```

The raw `ChunkId` and canonical chunk `ObjectId` are different identities and
cannot substitute for one another. The semantic lower bound is at least one
complete pass for each required hash domain. Additional passes over the same
canonical bytes are implementation amplification, not required complexity.

The optimization target is therefore:

```text
one canonical encode -> one identity hash -> one validated storage handoff
```

not a weakened or partial authentication rule.

## 6. CAS creation, reuse, and authentication

### 6.1 New object

For a new canonical object of `b` bytes:

```text
T_put_new(b) = Theta(b) authentication/validation + backend lookup/write
M_put_new(b) = O(min(b, MAX_OBJECT_BYTES))
S_put_new(b) = Theta(b)
```

Across a file capture:

```text
T_new_chunk_bytes = Theta(S_u)
new chunk rows     = O(U)
```

The logical reference stream still has `N` occurrences even when `U < N`.

### 6.2 Existing immutable object

An object ID or SQL row-existence result is not proof that the incumbent bytes
are authentic and equal. Reuse of a persisted `b`-byte object requires
`Theta(b)` byte authentication and semantic validation unless a separately
bounded operation-local verified-work receipt exactly covers that immutable
store identity, validation authority, integrity epoch, mapping profile,
generation, authenticated root/transition, locator/row/range, and object ID.
The 216-byte publication snapshot receipt is not such a locator receipt and
cannot authorize incumbent equality.

The current shape can therefore pay:

```text
T_reuse = sum Theta(bytes of authenticated reused occurrences)
```

This can be material when repeated chunks are submitted many times. A future
bounded WP10 locator receipt may remove duplicate backend reads only when its
authority exactly covers them. It does not permit trusting a bare key or
partial bytes.

Receipt reuse improves constants and redundant I/O; complete authentication
of a newly fetched whole-object BLAKE3 value remains `Theta(b)`.

### 6.3 SQL operation count

Without batching, a capture can perform `O(N)` object lookups and writes and
`O(mapping objects)` mapping-row operations. Two statements per new object are
still `O(N)`, but the constant is expensive.

A bounded batch of `B_sql` rows changes execution count approximately from:

```text
O(number of rows)
```

to:

```text
O(ceil(number of rows / B_sql)) executions)
```

while row work, authentication bytes, and result classification remain
linear. Batching is an engine optimization, not a format field.

## 7. File mapping construction and durable space

Each file reference has exactly 68 bytes:

```text
32-byte raw ChunkId
4-byte raw length
32-byte canonical chunk ObjectId
```

For the fixed-radix candidate:

```text
P = ceil(N/K)

mapping objects:
O_map(N) = P + sum(B_i) + 1

canonical mapping bytes:
M(N) = 68*N + 68*P + 69*sum(B_i) + 49
```

With fixed bounded `K` and `F`:

```text
O_map(N) = Theta(N/K) = Theta(N)
M(N)     = Theta(N)
```

This linear space is fundamental: every logical occurrence must preserve
order, raw identity, length, and canonical locator identity. The radix adds
small envelope/descriptor overhead in exchange for bounded objects and local
access.

The live snapshot's approximate CAS space is:

```text
S_live = Theta(S_u + 13*U + M(N))
       = Theta(S_u + N)
```

SQLite B-tree pages, indexes, journals, WAL files if any, and APFS allocation
are additional physical bytes and must be observed rather than inferred.

LayerFS currently has no GC. Across edit history:

```text
S_store_history = live reachable bytes + unreachable immutable residue
```

and may grow with cumulative newly created chunks and mappings. COW reduces
per-edit creation; it does not reclaim historical CAS objects.

### 7.1 Construction memory

A streaming builder need retain only:

- one reference leaf;
- at most one partial branch per active level;
- one chunk/canonical-object window; and
- checked counters.

Therefore:

```text
M_mapping_build = O(K + F*H + max object window)
```

not `O(N)`. At 100 GiB under the retained density, K64/F64 needs only two file
branch levels.

## 8. Range-read complexity

### 8.1 Flat manifest

A flat manifest can binary-search offsets after decode, but whole-object
authentication still hashes the complete manifest:

```text
T_flat_range = Theta(N) mapping bytes + O(log N) routing + O(R_b)
M_flat_range = O(N) if eagerly decoded
```

### 8.2 One-level page table

A root containing every leaf descriptor reduces leaf scanning but still makes
root authentication proportional to the number of leaves:

```text
T_one_level_range = Theta(N/K) descriptor bytes + O(L_v + C_v + R_b)
```

It also creates an artificial maximum when the single descriptor object hits
the Phase 1 field/object limit.

### 8.3 Radix path with a valid receipt

The new candidate authenticates the visited root/branches, intersected leaves,
and selected complete chunk objects:

```text
T_radix_range = O(V_b*B_v + V_l*L_v + C_v + R_b)
M_radix_range = O(max(B_v,L_v) + max chunk + output window + H)
```

Since `H = O(log_F(N/K))`, mapping navigation is logarithmic in file-reference
count. Cross-leaf and cross-branch ranges add only the intersected siblings.
Zero-length references remain semantic but are not fetched for nonempty range
data. For one leaf, `V_b = H+1` and `V_l = 1`, reducing the formula to
`O((H+1)*B_v + L_v + C_v + R_b)`.

Without a valid receipt or equivalent integrity authority, skipped cumulative
summaries cannot be trusted. The operation must first perform a complete scrub
or fail with the exact validation-authority error. The fast-path complexity is
conditional, not an authentication bypass.

### 8.4 Numerical anchors

For the retained 100-GiB K64/F64 projection:

- `N = 5,410,816` references;
- `P = 84,544` leaves;
- branch counts are 1,321 and 21;
- a cold one-leaf mapping path authenticates about 10--12 KiB before chunks.

A one-level 40-byte descriptor table for 84,544 leaves alone is about
3.23 MiB. The radix therefore reduces selected-path mapping authentication by
hundreds of times at that scale, before duplicate current-engine BLOB passes.

## 9. File-write and COW complexity

All incremental bounds in this section require an authenticated base snapshot
and exact CDC rejoin where CDC reuse is claimed. Without that authority, a
full scrub or full replacement path may dominate.

### 9.1 New file or full replacement

```text
T_create = Theta(S) CDC
         + Theta(S) required hash-domain work
         + O(N) CAS/mapping row work
         + durability/publication cost

M_create = bounded chunk/object/page windows
S_create = Theta(S_u + N)
```

The radix does not improve the `Theta(S)` lower bound. SQL batching, fused
encoding/hash/validation, and removal of duplicate BLOB passes improve the
constant only.

### 9.2 Same-count local edit

If one contiguous CDC edit changes `X_c` references across `L_c` adjacent
leaves without changing total `N`, every unaffected leaf and branch retains
its canonical identity. The touched leaves rewrite their union of ancestor
spines:

```text
T_same_count = O(X_b + X_c + K*L_c + F*(L_c+H))
mapping objects created = O(L_c + H)
```

For `L_c = 1`, this reduces to `O(X_b + X_c + K + F*H)` time and
`O(H)` mapping-object work. Scattered edits use the exact union of their
changed leaves and ancestor paths rather than this adjacent-leaf bound. For
fixed `K` and `F`, a one-leaf edit has `O(log N)` mapping-object work.
The candidate's retained-density exact maxima are:

| Case | File-mapping rewrite |
|---|---:|
| 100 MiB | 7,098 canonical bytes / 3 objects |
| 512 MiB | 7,298 canonical bytes / 3 objects |
| 100 GiB analytical projection | 10,447 canonical bytes / 4 objects |

A 5,284-reference flat 100-MiB mapping is about 359 KiB, so the candidate
reduces this same-count mapping rewrite by roughly fifty times before database
overhead.

### 9.3 Append at EOF

Appending `X_b` bytes creates `X_c` references, completes or replaces the last
partial leaf, creates any additional leaves, and rewrites only the rightmost
branch/root spine:

```text
T_append = O(X_b + X_c + K + F*H)
new mapping objects = O(ceil(X_c/K) + H)
```

The operation is proportional to appended data plus logarithmic metadata, not
existing file size.

### 9.4 Truncate at EOF

With an authenticated root and path, a cut can retain complete prefix
subtrees by identity and omit complete suffix subtrees without reading their
payloads. It rewrites the boundary leaf and ancestor spine:

```text
T_truncate_mapping = O(H + K)
new mapping objects = O(H)
```

Locating a byte cut uses the range path. The removed objects become unreachable
from the new root; truncate does not perform physical CAS deletion or GC.

### 9.5 Early/middle count-changing insert or delete

Fixed ordinal leaves are canonical but shift when reference count changes.
If `Z` suffix references change page membership:

```text
T_count_change_mapping = O(Z)
S_new_mapping_history  = O(Z)
worst case Z = Theta(N)
```

This is the candidate's principal unresolved asymptotic weakness. Conservative
whole-suffix ceilings are:

| Case | Mapping invalidated/created ceiling |
|---|---:|
| 100 MiB, 5,284 -> 5,285 refs | 365,211 bytes / 86 objects |
| 512 MiB, 27,162 -> 27,163 refs | 1,876,516 bytes / 433 objects |
| 100 GiB retained-density | 373,777,332 bytes / 85,889 objects |

WP4-P must reject fixed ordinal grouping if the forced `+1` row fails the
measurement gate. Only then is a deterministic, history-independent
content-defined/prolly reference tree justified. Its target expected behavior
would be local page changes plus `O(log N)` ancestors, but no such format or
claim exists yet.

### 9.6 Namespace ancestor propagation

Changing a file `NodeId` changes each containing directory node up to the root.
For logical depth `d`:

```text
durable ancestor objects = O(d)
durable ancestor bytes   = sum over ancestors(page_i + index_i + wrapper_i)
```

The durable logical-depth bound is 256. With bounded directory pages, one
same-size child-ID replacement creates a page, index, and wrapper at each
ancestor. Current in-memory `TreeNode` mutation may still clone and rehash a
complete `BTreeMap` at an ancestor, producing `O(E_i)` CPU/allocation for that
ancestor. The durable mapping does not by itself repair that core data
structure.

## 10. Directory complexity

Directory entries are canonically ordered and packed into bounded Phase 1
Directory pages. A separate authenticated index routes by page boundary.

Let total encoded entry bytes be `S_e`. Approximately:

```text
P_d = O(S_e / B_d)
directory live space = Theta(S_e + index descriptor bytes)
```

### 10.1 Create and full validation

```text
T_directory_create = Theta(S_e)
T_directory_scrub  = Theta(S_e + child closure work)
M_directory_build  = O(B_d + bounded index builder/spool)
```

### 10.2 Point lookup

After authenticating the complete index and selected page:

```text
CPU routing = O(log P_d + log entries_in_page)
authenticated mapping bytes = O(index bytes + B_d)
```

Whole-object authentication means the byte-I/O bound is not merely
`O(log E)`: the complete bounded index and page are hashed. The A/B chooses the
best physical constant.

### 10.3 Same-size child replacement

One replacement changes one entry page, the index containing its new page ID,
and the wrapper containing the new index ID:

```text
T_directory_replace = O(B_d + index bytes)
new mapping objects = 3 per changed ancestor directory
```

For 100,000 maximum-name entries:

| Page ceiling | Max pages | Max index | Same-size replacement ceiling |
|---:|---:|---:|---:|
| 64 KiB | 447 | 131,003 bytes | 196,628 bytes / 3 objects |
| 256 KiB candidate | 112 | 32,848 bytes | 295,081 bytes / 3 objects |
| 1 MiB | 28 | 8,236 bytes | 1,056,901 bytes / 3 objects |

The earlier near-16-MiB page preference could rewrite approximately 16.8 MiB
for one child. The 256-KiB candidate reduces that ceiling by about 57 times,
but it is not called optimal before the physical-I/O A/B.

### 10.4 Count-changing insertion/removal

Greedy canonical partitioning can move entries across every later page:

```text
T_directory_count_change = O(entries in affected suffix)
worst case = O(E)
```

This row must be measured alongside same-size replacement. The implementation
must not claim path-local directory insertion while using a suffix-repacking
format.

## 11. Delta and root/publication complexity

### 11.1 Delta encoding

Delta order and repeated paths are semantic. Sorting, deduplication, or
parallel reordering is not a valid optimization.

For `T` entries and total encoded delta bytes `S_delta`:

```text
T_delta_encode = Theta(S_delta)
M_delta_encode = O(delta page ceiling + bounded index/spool)
S_delta_live   = Theta(S_delta)
```

Delta replay is:

```text
T_delta_replay = Theta(T) dispatch/path work + referenced COW mutation cost
```

The referenced COW cost can dominate. A delta mapping does not make a current
full-map clone inside core tree mutation logarithmic.

### 11.2 Root identity

The durable root is exactly the canonical directory-wrapper `ObjectId`.
Parentage is a publication transition, not content identity. Calculating a
changed root costs only the changed file/directory ancestor mappings already
counted above; it does not require hashing unchanged payload subtrees.

### 11.3 Atomic publication

After all required immutable objects and closure evidence exist, visible-head
publication is one atomic transaction/conditional transition:

```text
T_publish = O(objects/rows staged for the capture) + one durability boundary
visible transitions = O(1)
```

Durability sync latency is a constant-number boundary but can dominate small
captures in wall time. It is not removed by Big-O notation. A valid base
receipt can allow incremental validation of reused immutable subtrees; without
it, complete closure validation may dominate publication preparation.

## 12. Closure traversal and authentication

For `V` visited strong-edge occurrences and `A` fetched canonical bytes:

```text
T_full_closure = Theta(A + V)
```

Every fetched object is fully hashed and semantically decoded. Shared sub-DAG
occurrences are valid. Without a valid receipt they may be reauthenticated per
occurrence, so time is stated in edge occurrences rather than only unique
object count.

Cycle detection requires only the active ancestry, not an unbounded global
visited-ID map:

```text
M_active_stack = O(physical graph depth)
```

The candidate's maximum derived physical strong-edge path is 781 edges, and
the 100-GiB cases are shallower. Wide work is handled with a bounded/file-backed
spool rather than source-sized RAM.

The spool's resident window is bounded, but its transient backing file may be
linear in queued edge records in the worst case:

```text
M_spool_resident = O(fixed spool window)
S_spool_temporary = O(queued edge records) <= O(V)
```

It is operation residue, not live CAS metadata, and must be released on
success or retained with exact typed custody after a cleanup failure.

Removing the global visited map changes memory from:

```text
O(unique closure objects)
```

to bounded object/spool windows plus:

```text
O(active depth)
```

The tradeoff is possible repeated authentication of shared completed
occurrences. A bounded exact receipt/cache may improve that constant without
becoming an unbounded object-ID map.

## 13. Reopen complexity

### 13.1 Fast unchanged reopen

With an exact valid `ValidatedSnapshotReceiptV1` and the required store/epoch
authority:

```text
T_fast_reopen = O(1) head + receipt + bounded root authentication
```

Subsequent access pays only for fetched paths:

```text
T_access_after_reopen = O(H + selected mapping/chunk bytes)
```

The receipt attests that the closure was valid at publication. It does not
authenticate bytes fetched later, prove continued presence, or detect an
out-of-band store rollback without the required monotonic authority.
Current SQLite qualification therefore permits same-open immutable-generation
reuse by default. Adversarial cross-reopen reuse is unavailable until the
validation key, store identity, epoch, and rollback authority have exact
custody; otherwise the operation performs a fresh scrub or returns
`ValidationAuthorityUnavailable`.

### 13.2 Fresh scrub reopen

When receipt authority is unavailable or a complete audit is explicitly
requested:

```text
T_scrub_reopen = Theta(A + V)
```

This distinction is semantic, not a benchmark trick. Fast reopen and a fresh
full integrity scrub are different named rows.

## 14. Reconstruction and materialization

### 14.1 Full streamed reconstruction from CAS

A full reconstruction must authenticate mapping objects and every referenced
chunk occurrence and emit every payload byte:

```text
T_reconstruct = Theta(A + V + S)
              = Theta(S + N) for the ordinary file closure
output lower bound = Omega(S)
```

Peak mapping-owned memory is independent of `S`:

```text
M_reconstruct = O(max canonical object
                  + spool window
                  + output window
                  + active depth)
```

The declared ordinary streaming shape is about 33.6 MB: a 16-MiB canonical
object window, 8-MiB spool window, 8-MiB output window, and small frames and
receipt state. `Q` is peak live allocation. `W` and `D` may reach 100 GiB
because they are checked cumulative counters, not resident allocations.

Radix mapping does not improve full reconstruction's asymptotic time and can
add small-object overhead if implemented as one SQL/BLOB operation per mapping
object. Batching, sequential leaf traversal, prepared statements, bounded
read-ahead, and single-pass authentication are required to keep the constant
small.

### 14.2 Partial or range reconstruction

With a valid receipt:

```text
T_partial = O(V_b*B_v + V_l*L_v + C_v + R_b)
M_partial = bounded path/chunk/output windows
```

For a range that visits exactly one reference leaf, this reduces to:

```text
T_partial = O((H+1)*B_v + L_v + C_v + R_b)
```

This is the principal materialization-side asymptotic improvement. A 64-KiB
request from a 100-GiB file need not authenticate or decode a file-sized
manifest or unrelated chunks.

### 14.3 Clean native materialization

Native materialization means producing actual destination files, directories,
metadata, and a publication boundary. For a complete workspace:

```text
T_native_clean = Theta(A + total payload bytes + J namespace operations)
destination bytes = Theta(total payload bytes + native metadata)
```

Writing a new `S`-byte native file has an unavoidable `Omega(S)` destination
write lower bound. WP4 does not implement native materialization, APFS clone or
reflink behavior, atomic destination publication, or its benchmark. Streamed
reconstruction is a prerequisite, not proof of native materialization speed.

### 14.4 Incremental native materialization

If the destination is proven to correspond exactly to the authenticated parent
root, the future target is:

```text
T_native_incremental = O(changed paths
                         + changed payload bytes
                         + changed mapping paths
                         + required native durability work)
```

Unchanged paths should require no payload rewrite. This requires a later
materialization authority/custody design; a delta alone does not prove that an
arbitrary native destination still matches its parent root.

### 14.5 Destination verification

A full byte-for-byte verification of newly materialized output is:

```text
T_destination_verify = Theta(total materialized bytes + entries)
```

Sampling or metadata-only checks cannot be reported as equivalent full
verification. A later benchmark must state whether destination verification
is inside its timer.

## 15. Resident memory versus cumulative work

The candidate explicitly separates:

```text
Q   = max live mapping-owned allocation at any instant
W   = cumulative input/authentication/work bytes
D   = cumulative decoded/output bytes
```

For streaming capture, scrub, range, and reconstruction:

```text
Q = O(MAX_OBJECT_BYTES
      + spool window
      + output window
      + K
      + F*physical depth)
```

while:

```text
W, D = O(total bytes processed)
```

Thus a 512-MiB or 100-GiB operation may have correspondingly large `W` and
`D` without allocating that amount. The 1-GiB durable live-allocation cap is
a pathological simultaneous-allocation guard, not a file-size, closure-work,
or streamed-output ceiling.

An eager API requesting one `Vec` containing a very large reconstructed file
has `Q = Theta(S)` and may correctly fail admission. The streaming API remains
the scalable contract.

## 16. Remote-backend complexity model

No remote backend is selected or implemented, but the mapping's dependency
depth is visible.

For a cold one-leaf range with a valid receipt:

```text
dependent request stages = O(H)
transferred mapping bytes = O(H*F + K)
transferred payload bytes = O(R_b plus complete overlapping chunk envelopes)
```

For capture:

```text
immutable PUT objects = O(U + mapping objects + directory/delta objects)
final dependent visible-head CAS = O(1)
```

Bounded batching changes network request count toward:

```text
O(ceil(objects / batch size))
```

but does not remove object bytes or final-publication dependency. RTT can
dominate wall time even when Big-O is unchanged. The format permits computing
immutable object IDs locally and uploading independent objects in bounded
batches; it does not require one network round trip per chunk.

## 17. Numerical durable-space anchors

The exact K64/F64 analytical 100-GiB mapping projections are:

| CDC density | References | Mapping objects | Mapping bytes including framing | Payload-relative overhead |
|---|---:|---:|---:|---:|
| 32-KiB chunks | 3,276,800 | 52,014 | 268,958,546 | 0.2505% |
| Retained measured density | 5,410,816 | 85,887 | 444,117,735 | 0.4136% |
| 8-KiB chunks | 13,107,200 | 208,051 | 1,075,833,899 | 1.0019% |

These are canonical mapping projections, not physical SQLite/APFS
measurements. They demonstrate linear, low-percentage metadata growth and no
arbitrary 100,000-page or file-size ceiling.

## 18. What improved, what did not, and what remains open

### 18.1 Asymptotically improved

```text
range mapping work:
O(N) or O(N/K) -> O(log_F(N/K) + selected leaves)

same-count COW mapping objects:
O(N) or O(N/K) -> O(log_F(N/K))

resident mapping memory:
O(N) or O(S) -> bounded windows + O(physical depth)

fast unchanged reopen:
O(full closure) -> O(1) initial authority + lazy selected paths
```

### 18.2 Constant-factor or bounded-amplification improvement

```text
maximum-name 100,000-entry directory same-size replacement:
about 16.8 MiB former page ceiling -> about 295 KiB candidate ceiling

100-MiB same-count file mapping rewrite:
about 359 KiB flat -> 7,098 bytes K64/F64 candidate
```

### 18.3 Unchanged necessary classes

```text
initial capture              = Theta(S)
CDC over inspected bytes     = Theta(S) or Theta(X_b)
required canonical hashing   = Theta(canonical bytes)
full reconstruction          = Theta(S + N)
fresh full closure scrub     = Theta(A + V)
clean native materialization = Theta(S + J)
live mapping disk space      = Theta(S_u + N)
```

### 18.4 Explicit unresolved costs

- Fixed-ordinal early/middle count changes remain `O(N)` in the worst case.
- Greedy directory leading inserts can remain `O(E)` in the worst case.
- Current in-memory COW directory mutation may clone/rehash a full map.
- Current SQLite paths can perform sequential per-object statements and
  duplicate BLOB authentication/validation passes.
- Full reconstruction can suffer small-object/query amplification without
  batching and sequential leaf traversal.
- No GC means unreachable immutable COW history consumes durable space.
- Native materialization and incremental destination authority are later
  phases, not WP4 results.

## 19. Measurement obligations

Big-O notation is a design filter, not throughput evidence. WP4-M must compare
K64/F64, K59/F101, and K256/F256 and directory ceilings 64 KiB, 256 KiB, and
1 MiB on identical 100-MiB and 512-MiB fixtures. It must include:

- full create/capture;
- full scrub/reopen;
- full streamed reconstruction;
- prefix, middle, EOF, and cross-boundary ranges;
- same-count middle edit;
- forced `+1` early/middle edit; and
- wide-directory create, lookup, same-size replace, and leading insert.

Every row must record at least:

- wall and CPU time;
- RSS and mapping `Q/W/D`;
- canonical objects and bytes encoded/hashed/authenticated;
- created, reused, and unreachable objects/bytes;
- SQL statements, rows, BLOB opens, and bounded batches;
- SQLite page, journal/WAL, database, and physical allocated bytes;
- file height and selected range path;
- closure occurrences and canonical bytes;
- durability boundaries; and
- cache conditioning and unavailable observations explicitly.

Source generation remains outside timers. The 100-GiB rows remain analytical;
the 100/512-MiB slopes test whether the measured per-byte/per-object model
supports the projection.

Native APFS materialization requires a later distinct benchmark that includes
destination namespace creation, payload writes, metadata application,
durability/publication, and explicitly timed destination verification. CAS
reconstruction must not be relabeled as native materialization.

## 20. Decision summary

The candidate improves the operations for which CAS+COW indexing should matter:

```text
small same-count writes  -> changed leaf + logarithmic ancestor spine
EOF append/truncate      -> rightmost/boundary path rather than whole file
range reconstruction     -> logarithmic mapping path + returned chunks
unchanged reopen         -> fixed authority check + lazy authenticated access
resident memory          -> bounded windows independent of file size
```

It does not pretend that byte-linear work is avoidable:

```text
new capture, full scrub, full reconstruction, and clean native materialization
remain linear in the bytes or closure they must process.
```

The fixed-radix candidate is promotable only if measured constants are good and
the count-changing-edit gate passes. Otherwise WP4-P must reject it and measure
the narrowly defined deterministic content-defined/prolly alternative rather
than freezing a known `O(N)` small-edit failure mode.
