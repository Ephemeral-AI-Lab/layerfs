# Phase 4 algorithm-complexity analysis

Status: WP4-C companion analysis; candidate format, not compatibility authority

Date: 2026-08-17

Scope: CAS + CDC + COW + canonical persistence, SQLite integration, reads,
writes, reconstruction, and future native materialization

## 1. Purpose and authority

This record states the time, resident-memory, and durable-space complexity of
the candidate in `../mapping/logical-persistence.md`. It distinguishes:

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

## 21. M4.5 repaired same-count changed-spine analysis

Status: source-linked terminal analysis for the prospectively amended private
K64/F64 XOR same-middle measurement path and the retained `v3-terminal` campaign.
This section does not promote K64/F64, complete WP4-P, or describe production
`Engine` integration.

### 21.1 Exact edited-stream CDC and rejoin

The repaired operation replaces the retained middle chunk's 18,854 bytes with
the deterministic bytewise transform `old_byte XOR 0x5a`
(`same_middle_replacement`, `phase4_create_edit_benchmark.rs`). The controlling
spec was prospectively amended on 2026-08-19 before the accepted build/timing,
and `require_amended_m45_expectations` rejects drift in the exact
operation, source, base, CDC sequence, before/after file, root, transition, and
closure. An exact full edited-stream scan proves that it has 5,284 references.
The prior uniform-`0x5a` operation is not the same
workload: exact FastCDC produces 5,283 references, so it is count-changing and
is ineligible for the same-count path. Its old 5,284-reference expectation was
created by substituting bytes after the old chunk boundary had already been
chosen and is superseded.

The local algorithm is in `same_middle_rejoin_references`:

1. authenticate the old file root and exact reference count;
2. choose ordinal `position - 1` as the safe predecessor and authenticate that
   reference/path;
3. read at most predecessor + removed bytes + the frozen 1-MiB rejoin ceiling;
4. run the frozen 8/16/32-KiB FastCDC scanner over
   `predecessor || exact replacement || old suffix`;
5. compare complete `(start, raw length, raw ChunkId)` observations and stop at
   the first two exact old-suffix confirmations
   (`tail_exact_rejoin`);
6. store only chunks before that proven rejoin and reuse the authenticated old
   suffix; and
7. require `old_count == new_count`, which is exactly the final-reference-count
   equality for a local replacement. Otherwise return the typed bounded-rejoin
   or length mismatch and never enter same-count COW.

The retained row inspected 143,709 CDC bytes, emitted five replacement chunk
references, and stopped well below the 1-MiB failure ceiling. The full oracle
independently scans the complete edited `Reader`, not the old callback stream.
`same_middle_expectations_use_the_exact_edited_cdc_stream`
asserts both exact-oracle equality and inequality with the withdrawn callback-
substitution sequence. Failure to establish this rejoin falls back by typed
classification; the fixed-ordinal same-count verifier is never faked.

For changed CDC bytes `X_b` and changed/new reference occurrences `X_c`, the
successful mutation is:

```text
T_mutation = O(X_b + X_c + K + F*H)
```

For the retained topology, `N=5,284`, `K=64`, `P=ceil(N/K)=83`, `F=64`, and
there is one branch layer between root and leaf (`H=1`). The direct measured
inputs were `X_b=143,709` scanned bytes and `X_c=5` replacement references.
The 1-MiB bound is a failure ceiling, not work performed after early rejoin.

### 21.2 Ordinal-local rewrite and redistributed lengths

`rewrite_same_root_by_ordinal` replaces
the consecutive ordinal run and rewrites only its affected leaf/leaves,
ancestor branch union, file root, singleton namespace root, and transition.
It does not require individual changed references or intermediate subtree
cumulative ends to retain their old lengths. Same-count means:

```text
final reference count equal
final total raw length equal
```

Each prior and replacement subtree independently recomputes and validates its
actual cumulative lengths. The paired verifier accepts different prior versus
replacement child totals when their separate declared/actual values agree.
`changed_spine_accepts_same_count_length_redistribution` directly
covers a two-reference length redistribution across the leaf boundary.

The retained edit created eleven canonical objects: five changed chunks plus
one leaf, one branch, file root, namespace root, delta page, and transition.
It rewrote 7,382 canonical mapping bytes and wrote 110,745 new canonical bytes.
Unchanged leaves, the other root branch, and unchanged chunk occurrences keep
their exact ObjectIds.

### 21.3 C0/C1 equivalence and witness-covered skips

C0 and C1 share the same executable, prepared base, edit, CDC/rejoin, object
creation, root/transition construction, COMMIT, reopen, scrub, reconstruction,
and range verification. The sole changed variable is pre-COMMIT qualification:

- C0 fully authenticates the requested transition and complete new file
  closure (`qualify_same_middle_full_closure`).
- C1 first consumes the exact private transaction-owned same-open permit,
  authenticates both nodes at each changed spine position, and treats an edge
  as covered only when the authenticated prior/replacement parent descriptors
  carry the byte-identical immutable child ObjectId
  (`verify_same_count_changed_spine`).

No merely equal count, length, ordinal, row key, or receipt byte string permits
a skip. Every different edge is followed, and every new chunk is read in full,
canonical-ObjectId authenticated, length checked, and raw-ChunkId checked.
For the retained topology the exact edge equation is:

```text
covered equal edges = 1 unchanged root child
                    + 63 unchanged branch children
                    + 59 unchanged leaf references
                    = 123

new/different edges = namespace->file root
                    + root->changed branch
                    + branch->changed leaf
                    + 5 changed chunk edges
                    = 8
```

The five new chunks total 103,363 authenticated canonical bytes. C1 removed
exactly 5,358 statement-cache acquisitions, queries, returned rows, and row-
BLOB reads relative to C0 while writes, rewrites, executes, changed rows,
transactions, COMMITs, Q, roots, transitions, closure, and endpoint storage
remained invariant.

The transaction-owned witness is established only by exact receipt/transition
checks plus `scrub_file`, whose namespace entry uses the exact singleton
resolver (`establish_same_open_file_witness`). Reopen, mutation,
tuple mismatch, single-use consumption, failed rollback, publication, and
unresolved durability invalidate authority. The direct complete-namespace and
failed-rollback tests are `witness_requires_the_exact_complete_namespace_closure`
and `failed_rollback_invalidates_an_unconsumed_witness`. A persisted receipt alone
cannot mint cross-reopen authority.

### 21.4 Qualification time and memory

Let `A_delta` be complete canonical bytes authenticated on the two changed
spines and `V_delta` be canonical bytes in fully traversed new/different
subtrees. C1 qualification is:

```text
T_qualify = O(K + F*H + A_delta + V_delta + H^2)
```

The `H^2` term is the bounded active-ancestry membership scan; no global
visited map is present. Parent canonical/decoded child vectors are explicitly
dropped before recursive changed-child descent in `verify_changed_file_pair`, so the
active resident shape is:

```text
M_resident = O(H + K + F
               + bounded CDC/chunk/canonical/page/SQL/range buffers)
```

For this contiguous edit, the pending changed-pair frontier is bounded by the
changed reference run (`O(K)` under the admitted local row). The final retained
campaign reports the same exact logical-Q high-water in every C0 and C1 row:

```text
base live state                       38,959
old authenticated CDC window       1,085,490
old-window RejoinChunk slots          12,864  (= 134 * 96)
edited scan input                   1,085,490
                                      -------
Q high-water                        2,222,803 bytes
```

The implementation records every term and checks the sum at the allocation
site; `q_cdc_overlap_current == q_high_water == 2,222,803` in all 12 rows and
`q_current=0` after the charged report output is delivered. The base-live term
is exactly `38,311` prepared-expectation bytes plus three simultaneous
216-byte witness/permit/prior-head receipts (`38,311 + 648 = 38,959`). The old window and scan
input are admitted before allocation, the exact 134 slots use the governing
96-byte file-reference charge, and the old window is explicitly dropped after
the edited input is built. Canonical builders, borrowed SQLite-to-owned copies,
decoded nodes, file references, tree nodes, DFS frames, delta paths, generated
SQL, eager ranges, range measurements, fixed receipts, prepared expectations,
phase/range JSON, and the final report output use the same checked RAII charge.
No already-allocated vector is adopted into Q. `read_prepared_expectations`
preflights the 128-KiB file and result capacity before allocation; `row_json`
counts the exact output before reserving it.

The exact-boundary/error test admits exactly 1,073,741,824 bytes, rejects the
next byte before allocation, leaves the prior charge unchanged, and returns to
zero. `real_sqlite_read_precharges_canonical_and_decoded_overlap`
independently computes a real SQLite canonical+decoded overlap. The complete
real-path equation is asserted by
`measured_edit_starts_from_an_already_published_base`.

W and D remain `Unavailable` because the private benchmark does not implement
the governing cumulative definitions exactly; canonical new-write,
authenticated-nonnew, rewrite, CDC, SQL, BLOB, and output counters are reported
under their precise narrower names. No alternate W/D meaning is invented.

### 21.5 BEGIN, publication, and ambiguous durability

`Store::begin` checks the next transaction identity, SQL execute
count, and transaction count before dispatching `BEGIN IMMEDIATE`; after SQLite
accepts BEGIN, installing counters and `active_transaction` is infallible.
`begin_counter_overflow_precedes_sql_and_leaves_connection_usable` proves the
typed `LengthOverflow`, unchanged head, absent writer,
and immediate connection reuse.

`transaction_attempt` is the single post-BEGIN/pre-COMMIT cleanup
path and preserves the first exact `FailureCause`, including a concrete missing
`ObjectId`, separately from cleanup and reconciliation. `Store::publish`
returns `PublicationOutcome`: normal `Committed`, or
`RequestedVisible` plus retained diagnostic provenance. The normal API test is
`normal_publish_retains_requested_visible_diagnostic`.

The dispatch matrix uses the production fresh-connection reconciliation path:

- `RequestedVisible`: real COMMIT succeeds, then acknowledgement is lost;
- `PriorVisible`: SQLite commit hook rejects the real COMMIT;
- `DifferentHead`: real COMMIT succeeds, then a separately committed complete
  successor head is visible; and
- `Ambiguous`: real COMMIT succeeds, then the fresh authoritative database path
  is genuinely unavailable during reconciliation.

`real_commit_dispatch_boundaries_cover_requested_different_and_ambiguous` and
`actual_commit_error_uses_fresh_reconciliation` prove one
counted COMMIT dispatch and exact first/cleanup/reconciliation/dominant slots.
After successful dispatch or requested-visible reconciliation, later failures
are wrapped as committed-publication failures and cannot relabel visibility.

### 21.6 Complete phases remain linear

The path-local result does not change the necessary complete phases:

```text
same-open witness establishment = Theta(complete reachable authenticated closure)
fresh full scrub                = Theta(complete reachable authenticated closure)
full reconstruction             = Theta(source bytes + references + mapping auth)
complete first-open lifecycle    = same-open authority + durable edit
                                 + reopen + scrub + reconstruction + ranges
```

Final `v3-terminal` C1 medians were 237.833 ms for same-open authority,
9.134 ms for the durable edit, 694.629 ms for post-COMMIT verification,
703.764 ms for the same-open complete lifecycle, and 940.827 ms for the
derived first-open edit lifecycle. C0/C1 durable medians were
440.023 ms / 9.134 ms (`-97.924%`, 5/5 wins). Only the durable edit and
pre-COMMIT qualifier are path-local.

### 21.7 Count-changing bound and explicit limitations

Fixed ordinal grouping remains suffix-linear for `+1`, prepend, insert,
delete, append-with-count-change, or any edit that cannot prove equal final
reference count:

```text
T_count_change = O(changed CDC bytes + suffix references/objects/bytes)
S_history      = O(rewritten suffix mapping bytes)
```

The honest retained S1-100 whole-suffix ceiling remains 5,285 references,
86 mapping objects, and 365,211 canonical mapping bytes. M4.5 did not run or
improve that row. The discovered uniform-`0x5a` 5,283-reference result is a
concrete example that must take the count-changing/fallback classification.

There is no source-sized or all-reference staging vector, unbounded
expectations input (128-KiB hard limit), unbounded visited map, unbounded
cache, or extra serialized metadata. The changed operation retains a bounded
old-range/scan window only; the database schema, 216-byte receipt, candidate
profile bytes, and authority sidecar format are unchanged.

### 21.8 Final checkpoint-quality evidence

The 2026-08-19 post-measurement §13.5A clarification records the actual
terminal comparison without changing the experiment: one release executable,
C0 complete-closure qualification, and C1 changed-spine qualification. CDC,
CAS/COW mutation, copied pair base, authority, expectations, COMMIT, reopen,
scrub, reconstruction, ranges, counters, and reporting are common. Retained
M3 is historical continuity evidence only. The accepted v3 measured-spec
SHA-256 remains
`55980c049e5e3ce824664070c11c358428c69ad1fb4f3a4fc0af925ce941756b`.

`ChargedVec::from_exact_builder` now accepts a separately built
vector only when both `len == declared` and `capacity == declared`. A larger
returned capacity is rejected as typed `AllocationFailed`; the precharge and
the rejected vector then drop together and Q returns to zero. The focused
`exact_builder_rejects_excess_capacity_and_cleans_q` regression
constructs `len=4, capacity=5` explicitly. This is the smallest shared fix for
all file, delta, and directory canonical builders and does not change the
authoritative 96/256/64 semantic charges. This is a safe typed-failure
portability limitation: if a platform cannot return the exact requested
capacity, the operation fails rather than adopting uncharged allocator excess.

The synthetic `build_deep_uniform_base` fixture constructs a
canonical K64/F64 tree by reusing immutable leaf/branch objects; it allocates
no source-sized buffer. Its exact topology is:

```text
N = 64 * 64 * 64 + 1 = 262,145 references
leaf occurrences             = 4,097
level-1 branch occurrences   = 65
root level / H               = 2
root children                = 2
```

`deep_changed_spine_proves_height_union_and_bounded_qualification` changes
ordinals 63-64 across a leaf boundary, ordinal 4,096 in
the first leaf of a second inner branch, and ordinal 262,144 in the final
partial leaf. The final prior/replacement comparison therefore spans both root
children and has the exact changed union:

```text
changed leaves                 = 4
changed branches               = 5  (three level-1 + two level-2)
prior spine objects            = 11 (namespace + file root + union)
replacement spine objects      = 11
receipt-covered equal edges    = 376
new/different edges            = 14
fully authenticated new chunks = 4
```

The direct C0 path performs 266,309 complete file-closure occurrences and
266,318 SQL queries. The C1 path performs zero complete-closure occurrences,
34 SQL queries, and no leaf-batch query. Its derived two-sided active-ancestry
charge is `(H + 3) * 64 * 2 = 640` bytes, its exact qualification Q high-water
is 43,488 bytes, and terminal Q is zero. A malformed cumulative summary at the
deep level-1 branch is rejected by both C0 and C1 as typed `LengthMismatch`.
This turns the `F*H`, `H^2`, and active-ancestry terms from code inspection into
direct H=2 evidence; it does not change their bounds.

Because the capacity rejection is release-path code, a fresh versioned v4
campaign was required. Its independently recomputed durable medians are
446.457 ms for C0 and 8.541 ms for C1 (`-98.087%`, 5/5 wins). Exact campaign Q
remains 2,222,803 bytes with terminal zero. C1 RSS arm median is 0.175% lower;
peak-footprint arm median is 0.129% higher, so §13.6 does not trigger the
15-pair extension. The accepted path-local complexity and limitations above
are unchanged.

## 22. F2 bounded full-create construction-proof analysis

Historical F2-v1 analysis only. Section 23 supersedes its runtime-authority,
exact-Q, redundant-hash, and protected post-COMMIT interpretations while
preserving the v1 measurements as immutable **FAIL / REVISE** evidence.

Status: source-linked terminal analysis for the private K64/F64 F2 candidate.
The candidate is `FAIL / REVISE` because its prospectively protected COMMIT
wall regressed; this section records the proven mechanism and honest bounds,
not acceptance, profile promotion, production integration, or F3 authority.

### 22.1 Algorithm and authority

For each canonical object constructed during the existing source pass, the
private Store returns a move-only evidence value only after either:

```text
new row inserted with the exact canonical ObjectId/kind/length
or
conflicting incumbent fetched, fully ObjectId-authenticated, and byte-equal
```

Evidence is transaction/open/authority/mutation ordered. The existing
`FileBuilder` consumes chunk evidence immediately, folds exact count/length/ID
facts into each leaf, then folds leaf proofs into its bounded branch frontier.
File, singleton-workspace, and genesis-transition construction finish one
proof bound to the exact store/open/authority/epoch/profile/transaction,
source fingerprint, CDC sequence/count, root, transition, and separately
prepared full-verifier expectation. Mutation, rollback, COMMIT, reopen,
replay, second use, or any mismatch invalidates it.

The ordered closure digest remains the existing flat root-first transcript.
It is not derived by composing bottom-up subtree digests. A separately prepared
full verifier freezes the expected scalar outside the row; fresh post-COMMIT
reconstruction recomputes it independently. No linear construction-event list
is retained.

### 22.2 Time complexity

Let `S` be source bytes, `N` references, `K` leaf capacity, `F` fanout, and
`H` canonical file-root branch level. The proof adds:

```text
source fingerprint during the existing source read = Theta(S)
CDC sequence accumulator                           = Theta(N)
leaf/branch/file/workspace/transition folds        = Theta(N/K) objects/edges
single-use pre-COMMIT consumption                  = O(1) + one head query
```

The removed control replay was:

```text
Theta(reachable canonical/raw bytes + edge occurrences)
```

Therefore both before and after remain:

```text
T_full_create = Theta(source bytes + references)
```

F2 is pass elimination, not an asymptotic improvement to full creation. The
retained row moves pre-COMMIT from 5,373 queries/BLOB reads/authentications and
105,291,608 canonical plus 104,857,600 raw hash bytes to one empty-head query
and zero replay authentication. It adds one 104,857,600-byte source
fingerprint during the source pass. Measured mapping/proof-construction wall
rose `403.402 -> 606.564 ms`; pre-COMMIT fell `386.637 -> 0.068 ms`.

### 22.3 Exact live memory

Target sizes are:

```text
PutEvidence=80, ConstructionNodeProof=64,
FileReference=68, FileChild=40, Vec=24, Hasher=1,920 bytes.
```

Let `P=ceil(N/K)`, let `H` be canonical height, and let `R` be the root child
count after exactly `H` ceiling divisions by `F`. The existing streaming
builder temporarily creates and later collapses one extra unary full level
when `R=F`, so:

```text
L = H + 1 + usize(R == F)

Q_proof(K,F,H)
  = 4,096
  + K*68
  + L*(24 + F*40)
  + L*8
  + L*(24 + F*64)
  + 80

M_proof = O(K + F*H)
```

For retained K64/F64 `N=5,284`, `P=83`, `H=1`, `R=2`, `L=2`, so exact
proof-owned charge is 21,952 bytes. Measured total Q is 55,325 bytes, under
the preregistered 73,728-byte cap, and every exit returns to zero. No
all-reference/object/event vector, visited set, cache/map, source-sized spool,
table, sidecar, or serialized metadata exists.

### 22.4 Durable space and work

The candidate and F1-v3 control are exact on 5,372 created objects,
105,291,554 new canonical bytes, 365,262 mapping bytes, SQL writes/changed
rows, BLOB writes, one transaction/COMMIT, schema, logical/apparent DB bytes,
root, transition, closure, reconstruction, and ranges. Hence:

```text
S_live_F2 = Theta(S_u + N)  # unchanged
```

APFS allocated-store-delta median decreases
`118,042,624 -> 109,248,512` bytes, but allocation is not physical I/O.
VFS read/write bytes, sync calls/wall, and byte-level media I/O remain
Unavailable.

### 22.5 Measured terminal interpretation

Durable capture improves `929.420 -> 786.868 ms` (`-15.338%`, paired median
`-15.629%`, 5/5), while complete lifecycle improves
`1,615.793 -> 1,476.144 ms`. CPU/RSS/peak pass. COMMIT regresses
`135.886 -> 176.823 ms` (`+30.126%`, paired median `+28.184%`, 0/5). Because
COMMIT was prospectively protected at 5%, F2 is `FAIL / REVISE` despite the
correct mechanism and material durable gain. F3 remains ineligible. Full
create, fresh scrub, reconstruction, and complete lifecycle retain their
previous honest linear bounds.

## 23. F2-v2 standalone construction-proof correction

Status: prospective/source-linked correction to historical §22. Section 22
describes immutable F2-v1 **FAIL / REVISE** evidence and is not standalone
publication authority: v1 bound proof consumption to an externally prebuilt
root, transition, and closure. V2 removes that runtime dependency.

### 23.1 Before/after authority and time

The F1 control constructs every canonical object once, then replays the full
reachable transition/file closure from SQLite before COMMIT. V2 instead issues
private move-only evidence after each successful canonical insertion or fully
authenticated, role/length/byte-equal incumbent. The existing streaming
builder folds exact occurrence, edge, count, raw-length, ObjectId, level, and
cumulative-end facts through chunk, leaf, branch, file, singleton workspace,
and Genesis transition summaries.

The proof is issued and consumed using only the live transaction's open,
store, validation authority, epoch, profile, transaction, authority serial,
mutation serial, empty head, one-pass source fingerprint, ordered CDC
sequence/count, total raw length, file root, workspace root, and transition.
It does not contain or require an external root/transition/closure oracle.
Optional golden values are compared only after fresh post-COMMIT root-first
verification. The flat closure transcript remains noncomposable and is not a
construction summary.

Let `B` be source bytes and `N` ordered chunk-reference occurrences. With
fixed profile bounds:

```text
source/FastCDC/raw ChunkId pass                     = Theta(B)
whole-source fingerprint in that same pass          = Theta(B)
ordered CDC accumulator                             = Theta(N)
canonical CAS writes/authenticated incumbent bytes  = Theta(B + N)
leaf/upper summary objects                          = Theta(N/K) + upper levels
authenticated occurrence and strong-edge folding    = Theta(N)
pre-COMMIT proof consumption                        = O(1) + one head query
fresh post-COMMIT closure/reconstruction             = Theta(B + N)
```

Therefore standalone full create remains exactly:

```text
T_full_create = Theta(B + N)
```

This is pass elimination and constant-factor work reduction, not a change in
the lower-bound class. V2 removes both the duplicate SQLite closure replay and
v1's unaccounted second raw `chunk_id(bytes)` over every just-derived chunk.
Required raw ChunkId hashing, the distinct whole-source fingerprint, and the
canonical ObjectId domains remain.

### 23.2 Exact live structures and Q

There is no source-sized or all-reference/object/event list, map, cache,
visited set, spool, table, sidecar, serialized metadata, or dependency. Live
semantic construction state consists of:

- one bounded chunk/canonical/SQL encoding window;
- one at-most-`K` leaf reference vector;
- one at-most-`F` child vector and one at-most-`F` proof-summary vector per
  active builder level;
- one cumulative total per active level;
- two fixed BLAKE3 hashers and scalar scope/counter fields; and
- one 80-byte per-put evidence slot.

For canonical height `H`, including the existing builder's possible temporary
unary-collapse level, the honest bound is:

```text
M_construction
  = O(K + F*(H+1)
      + bounded chunk/SQL/encoding/output buffers)
```

Let `R` be the root child count after exactly `H` ceiling divisions and
`L = H + 1 + usize(R == F)`. Target-layout exact charge is:

```text
Q_proof(K,F,H)
  = 4,096
  + K*68
  + L*(24 + F*40)
  + L*8
  + L*(24 + F*64)
  + 80
```

For retained `K=F=64`, `N=5,284`, `H=1`, `L=2`, the exact proof-owned peak is
`21,952` bytes. V2's `FileBuilder` owns the frontier charge for exactly as long
as the charged leaf/level/total/proof capacities exist. Unary collapse/root
finalization first drops moved children and all frontier allocations, then the
charge; the scan for nonempty levels does not allocate. The remaining fixed
proof plus evidence-slot charge is `4,176` bytes until proof drop. Checked
admission, allocation refusal, overflow, construction error, rollback,
successful consume, and report delivery all terminate at Q zero.

### 23.3 Durable work and space

For source unique-byte total `B_u`:

```text
summary objects = Theta(N/K) plus geometrically smaller upper levels
authenticated occurrence/edge work = Theta(N)
S_live = Theta(B_u + N)
```

Schema, serialized bytes, CAS identities, file/workspace/transition topology,
SQL write shape, BLOB writes, DELETE/FULL durability, one writer transaction,
one COMMIT, and independent post-COMMIT work are unchanged. Existing CAS
history and no-GC bounds remain unchanged.

V1 measurements are not exact-Q or standalone-authority acceptance evidence.
Its corrected audit also fails protected COMMIT, fresh-reopen pair-count, and
range pair-count gates. V2 may replace §22's candidate interpretation only
after all correctness, exact-Q, storage, M4.5, five-pair performance, manifest,
and final read-only audit gates pass. Until then F2 is **FAIL / REVISE** and F3
is ineligible.

### 23.4 F2-v2 measured closure

The frozen v2 source/executable are
`c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158` /
`68b599b819da9f05c76d35efd807c5d5f03266dfb7d4ed0cc78da269c4b891c0`.
Direct counters confirm exactly one required raw ChunkId pass over
104,857,600 bytes / 5,284 chunks, one distinct source-fingerprint pass, and
5,284 CDC accumulator entries. The former unaccounted third source-sized hash
is absent. Candidate pre-COMMIT is one query and zero rows/BLOB/authentication.

Measured medians are `398.408 -> 486.716 ms` for mapping/proof construction,
`386.597 -> 0.052 ms` for pre-COMMIT, `129.875 -> 164.052 ms` for COMMIT,
and `916.758 -> 652.573 ms` for durable capture. This is a 99.987%
pre-COMMIT wall reduction and 28.817% durable improvement with 5/5 wins, while
COMMIT regresses 26.315% with 0/5 protected pairs. Complete lifecycle improves
`1,608.325 -> 1,343.971 ms` (16.437%). These observations change no bound:
full create remains `Theta(B + N)` and independent scrub/reconstruction remain
linear.

All candidate rows satisfy exact `Q_proof=21,952`, total
`q_high_water=55,325 <= 73,728`, and terminal zero. The separately
preregistered v2 control-relative Q gate fails (`37,301 -> 55,325`), but the
governing bounded-state equation and absolute cap pass. Durable space, schema,
write shape, and one transaction/COMMIT are exact control matches.

The diagnostic-only 200-ms idle demonstrates that the outer COMMIT phase timer
includes caller delay after precommit: its median rises by 199.892 ms and the
caller-wrapper component absorbs the idle. Nested SQLite dispatch-to-return is
`167.886 -> 160.304 ms` (paired `-1.082%`, one +6.629% pair), so the idle does
not reliably cure the acceptance regression. Physical writeback/sync causality
remains Unavailable.

Final F2-v2 disposition is **FAIL / REVISE** because COMMIT, fresh-reopen arm
median, ranges 4/5, and the extra v2 relative-Q gate fail. F3 is ineligible.

## 24. F2-v3 accepted bounded construction proof

Status: **PASS / retain; F3 eligible only as a separate task**. This section
supersedes only the v2 terminal acceptance disposition in §23.4. It does not
alter the historical v1/v2 measurements or loosen their frozen gates.

### 24.1 Exact time bound and eliminated amplification

For raw source bytes `B`, ordered CDC occurrences `N`, leaf capacity `K`,
branch fanout `F`, and canonical file height `H`, accepted full create does:

```text
one source/CDC/raw-ChunkId pass                         Theta(B)
one whole-source fingerprint nested in that pass       Theta(B)
canonical CAS insert or exact incumbent authentication  Theta(B + N)
authenticated occurrence/edge summary folding           Theta(N)
summary-object construction                              Theta(N/K) + upper levels
pre-COMMIT proof consumption                             O(1) + one head query
fresh post-COMMIT closure/scrub/reconstruction           Theta(B + N)
```

The two `Theta(B)` hashes have distinct required trust domains and share the
same single source read. There is no third source-sized hash. The former
5,373-row SQLite/BLOB/authentication closure replay is absent from candidate
pre-COMMIT. Therefore:

```text
T_full_create = Theta(B + N)
```

This is a proven constant-factor/pass elimination, not an asymptotic-class
change. Fresh ordered-closure verification remains root-first and linear; its
flat digest is neither assumed nor changed to be subtree-composable.

### 24.2 Exact live data structures and bound

The accepted private construction path retains only:

- one bounded chunk/canonical/SQLite encoding window;
- one at-most-`K` `FileReference` leaf frontier;
- one at-most-`F` canonical child frontier and one at-most-`F` proof-summary
  frontier for each active builder level, including possible unary collapse;
- one cumulative total per active level;
- two fixed BLAKE3 hashers and scalar scope/counter bindings; and
- one 80-byte move-only per-put evidence slot.

It retains no `Theta(B)`/`Theta(N)` resident list, event transcript, source
spool, object/reference collection, map, cache, visited set, table, sidecar,
public proof framework, dependency, or serialized metadata. The honest bound
is:

```text
M_construction
  = O(K + F*(H+1)
      + bounded chunk/SQL/encoding/output buffers)
```

With `R` the root child count after `H` ceiling divisions and
`L = H + 1 + usize(R == F)`, the exact target-layout proof charge remains:

```text
Q_proof(K,F,H)
  = 4,096
  + K*68
  + L*(24 + F*40)
  + L*8
  + L*(24 + F*64)
  + 80
```

At retained K64/F64 (`N=5,284`, `H=1`, `R=2`, `L=2`), `Q_proof=21,952`
bytes. The five measured candidate rows all have total `q_high_water=55,325`
bytes, satisfy the authorized `<=73,728` cap, and terminate at zero. Owner
charges overlap exactly with live frontier/proof capacities; checked
admission, overflow, allocation/fold/issuance error, rollback, consume,
reopen/replay rejection, and report cleanup all return Q to zero.

### 24.3 Durable space and unchanged format

Summary objects are `Theta(N/K)` plus geometrically smaller upper levels;
authenticated occurrence/edge work is `Theta(N)`. With unique raw bytes `B_u`:

```text
S_live = Theta(B_u + N)
```

Accepted v3 adds zero metadata, schema, table, sidecar, or endpoint and leaves
CAS/CDC identities, canonical bytes, file/workspace/transition topology,
SQLite write shape, DELETE/FULL mode, single transaction/COMMIT, publication,
reconciliation, and post-COMMIT verification unchanged. All row databases
retain schema SHA-256 `e83baa35…162`, 109,268,992 bytes, 5,372 objects, one
meta row, one visible head, a 32-byte authority endpoint, and no residual
journal/WAL/SHM.

### 24.4 Accepted measurement and phase-coupling interpretation

Against sealed F1-v3, measured medians are:

```text
mapping/proof construction      400.461209 -> 492.776500 ms
pre-COMMIT qualification        387.464834 ->   0.051458 ms
standalone outer COMMIT         126.053792 -> 168.425625 ms
qualification + COMMIT          512.861458 -> 168.477083 ms
durable capture                 916.310250 -> 659.592708 ms
complete lifecycle            1,607.986125 -> 1,353.840916 ms
```

The durable improvement is 28.016% (paired 27.725%, 5/5); the combined-tail
improvement is 67.150% (paired 67.513%, 5/5). SQL queries fall 5,373 to 1 and
row-BLOB reads/authentications 5,373 to zero. Total CPU falls 16.049%; RSS,
peak footprint, and allocated-store delta improve. Exact final dirty writes
remain 26,676/26,676 and spills 6,676/6,675.

The prospectively run same-binary diagnostic establishes that full-verifier
activity changes named pager/filesystem state before COMMIT and shifts work
between qualification and dispatch without extra logical writes or durability
work. Standalone COMMIT's accepted-row +33.614% remains a reported engine-
phase diagnostic; it is not erased or claimed to be harmless generally. VFS
calls/bytes, xSync calls/wall, true journal/temp peaks, and physical media I/O
remain **Unavailable**, so physical causality is not claimed. The accepted
prospective contract hard-protects exact pager/write/durability equations,
combined tail, durable total, total/system CPU, storage, and post-COMMIT work.

These measurements do not alter the bound: accepted standalone full create
remains `Theta(B + N)`, accepted construction memory remains
`O(K + F*(H+1) + bounded buffers)`, and durable live space remains
`Theta(B_u + N)`.

## 25. F3 bounded SQLite CAS insertion grouping — terminal rejection

Status: **FAIL / revert; accepted F2-v3 bounds remain active; F4 ineligible**.

F3 tested one private fixed-cap insertion group under row cap 64 and total
owned canonical-byte cap 1,048,576. Each input remains completely canonical-
validated. Distinct first occurrences are inserted, conflicts are fully
authenticated, later occurrences retain exact created/reused classification,
and one ordered `PutEvidence` is issued only after the complete group succeeds.
The structure introduces no source/reference/object-sized resident state.

For source bytes `B`, occurrence count `N`, accepted frontier capacities `K/F`,
file height `H`, and fixed insertion caps `R=64`, `C=1,048,576`:

```text
T_F3 = Theta(B + N) + O(number_of_groups * R^2)
M_F3 = O(K + F*(H+1) + C + fixed R-sized SQL/result/evidence buffers)
S_F3 = Theta(B_u + N)
```

Because `R` and `C` are frozen constants, bounded duplicate/result scans do not
change the full-create class. The conservative all-path ceiling is at most
`16 * 64^2 = 65,536` ID comparisons per group. The exact logical-Q contract is:

```text
fixed/scalar/iterator envelope                         4,096
64 GroupInput descriptors                  64 * 72      4,608
owned pending canonical buffers                       1,048,576
maximum incoming canonical overlap                       32,781
64 pending construction results             64 * 16      1,024
maximum generated INSERT SQL                               767
64 bounded incumbent/result ObjectIds        64 * 32      2,048
64 ordered PutEvidence values                64 * 80      5,120
maximum decoded incumbent payload + charge                 33,024
maximum mapping encode-buffer overlap                       8,747
conservative analytical ceiling                        1,209,997
authorized absolute cap                               1,310,720
measured v3 high-water                                1,147,173
terminal current                                               0
```

The analytical sum is deliberately conservative; fast and fallback statement
buffers are sequential, not simultaneous. The preregistration's 1,210,008
figure retained a 778-byte conservative SQL term. V3's exact longest retry SQL
is 767 bytes, reducing the same equation by 11 bytes; both remain below the
unchanged cap. The measured external RSS/footprint costs are separately
protected and are not hidden inside logical Q.

### 25.1 Tested exact-classification shapes

The three immutable candidates cover three SQLite mechanisms actually tested
for learning which submitted IDs were created. They do **not** exhaust the
row/byte-cap cost curve, binding policies, statement policies, or all possible
SQLite implementations:

| Shape | Group write/read/result work on fresh fixture | Exact outcome |
|---|---|---|
| v1 `INSERT ... RETURNING` | 103 writes; 5,372 inserted-ID results | correct; wall and memory FAIL |
| v2 incumbent prequery then INSERT | 103 reads + 103 writes; zero INSERT results | performance FAIL; later-duplicate kind audit defect |
| v3 `INSERT OR ABORT`, query/retry only on uniqueness | 103 writes; zero reads/results/fallbacks | semantic PASS; wall and memory FAIL |

The v3 fast path removes classification reads/results on the fresh fixture: one
successful write statement per group, no classification query, no returned
row, and no fallback work. It is not a lower bound on total SQLite, parameter,
B-tree, journal, or memory work. Exact direct counters are:

```text
object occurrences / unique IDs                5,372 / 5,372
groups / optimistic writes                         103 / 103
fallbacks / reads / retries / result IDs           0 / 0 / 0 / 0
mapping acquisitions / executes / queries        103 / 104 / 0
INSERT bound rows / BLOB binds                 5,372 / 10,744
created / reused                               5,372 / 0
proof evidence / edges                        5,372 / 5,371
```

`INSERT OR IGNORE` plus a changed count does not identify conflict positions.
Post-query or savepoint variants add work and reduce to v2/v3. No-op update or
replace semantics weaken immutable CAS and can change trigger/write behavior.
Hooks, temporary markers/tables, sidecars, or cross-group history violate the
schema/state/one-variable bounds. The original campaign therefore accepted no
fourth classification primitive. The later D1 diagnostic below evaluates
whether any prospectively listed transport/cap revision has enough measured
budget to justify a new candidate; it does not infer universal exhaustion from
these three campaigns.

### 25.2 Measurement and retained bound

Against accepted F2-v3, v3 medians are:

```text
mapping/CAS       489.054042 -> 521.491917 ms  +6.632779%, 0/5
durable capture   653.848625 -> 693.110583 ms  +6.004747%, 0/5
complete lifecycle 1,345.911375 -> 1,385.097667 ms +2.911506%, 0/5
RSS               93,241,344 -> 98,304,000 B  +5.429626%, 0/5
peak footprint    92,045,744 -> 97,042,888 B  +5.428979%, 0/5
```

Candidate/control identities, logical writes, final dirty writes 26,676,
spills 6,675, sampled journal allocation 20,480, schema/endpoints,
FULL+DELETE, one transaction/COMMIT, reconstruction/ranges, and M4.5 remain
exact. VFS calls/bytes, xSync calls/wall, true journal/temp peaks, and physical
media I/O remain **Unavailable**.

F3 therefore establishes a negative constant-factor result, not a new retained
algorithm. After source reversion, accepted full create remains exactly §24:

```text
T_full_create = Theta(B + N)
M_construction = O(K + F*(H+1) + bounded accepted buffers)
S_live = Theta(B_u + N)
```

The intentional 1-MiB insertion group and its SQL/result buffers are absent
from the retained source. No F4 eligibility, profile choice, format change,
production extraction, or backend claim follows from F3.

### 25.3 D1 causal diagnostic and final retained interpretation

D1-v1 is sealed `REVISE` after one warmup control row exposed a surplus final
JSON brace. D1-v2 repaired only serialization/strict parsing, then completed
34 full-create rows and four M4.5 rows. Both independently implemented
analyzers accept the evidence and return `NO-GO`.

The affected light medians are:

```text
mapping/CAS       530.074166 -> 528.057708 ms  -0.380411% arm,
                                                  -1.004541% paired, 3/5 wins
durable capture   712.157750 -> 717.935708 ms  +0.811331% arm,
                                                  +1.407266% paired, 2/5 wins
throughput        140.418327 -> 139.288238 MiB/s
RSS                93,552,640 -> 98,369,536 B  +5.148862% arm,
                                                  +5.370889% paired, 0/5 wins
footprint          92,324,296 -> 97,092,040 B  +5.164127% arm,
                                                  +5.371336% paired, 0/5 wins
```

The 5,372-to-103 insertion-execution reduction remains exact, while 21,488
parameter binds, 10,744 BLOB binds, 105,463,458 requested BLOB bytes, and 5,372
logical B-tree insertions remain. Detail medians show:

```text
inferred cold prepares                  1 -> 11
VM steps                          118,184 -> 103,098
bind wall                      3.032563 -> 4.388413 ms
step/reset wall               92.198066 -> 102.612911 ms
post-bind statement MEMUSED      52,176 -> 1,256,000 B
DB STMT_USED sampled max          52,176 -> 1,514,944 B
```

The named proxy VFS directly observes that the control opens no VFS-backed
statement subjournal while every measured grouped row opens one and writes
8,216,400 logical bytes in 3,946 calls. Candidate median subjournal callback
wall is 16.127082 ms, nested in step/reset. This confirms spill on the runtime
system SQLite 3.51.0; it does not expose physical-media traffic or in-memory
journal CPU.

The separate mapping-reset MEMSTATUS pair records SQLite memory high-water
`88,257,960 -> 89,715,544` bytes. Logical Q remains bounded but rises
`55,325 -> 1,147,173` bytes. Both arms retain identical final pager/storage
equations, one transaction/COMMIT, roots, transition, closure, schema, and no
residual journal/WAL/SHM. All four M4.5 rows pass with grouping counters zero.

With fixed caps `R` and `C`, the grouping construction still has the same
asymptotic classes stated above. D1 changes the constant-factor interpretation:

```text
statement execution count reduction != total SQLite work reduction
fixed-cap O(1) memory                 != a small measured memory constant
```

The primary non-overlapping gross observations are prepare 0.700372 ms, all
candidate bind wall 4.388413 ms, candidate step/reset excess 10.414845 ms, and
VFS-backed subjournal callback wall 16.127082 ms. The largest is below the
prospectively frozen 60 ms no-go boundary before subtracting mandatory
replacement work. The independent analyzer more conservatively refuses to call
hidden SQLite CPU bounded by those observations, but also finds no positive net
removable budget because prequery/bind/safety replacement costs are not
measured. Both reach the same final decision.

Therefore the supported terminal claim is scoped:

- the three immutable R64/B1MiB F3 implementations failed;
- D1 did not justify an OR-FAIL/prequery, static-bind, fixed-SQL, or smaller-cap
  F3-v4 under the frozen Amdahl gate;
- accepted F2-v3 remains the retained implementation and bounds;
- F4 remains ineligible.

This is not a universal limitation of SQLite, all caps, other hosts, future
storage engines, or physical-layout work.

### 25.4 Terminal custody

The valid D1-v2 evidence root is sealed read-only. Its corrected 405-payload
mode/byte/SHA/path manifest is
`d1-v2-terminal-manifest-r2.tsv`, SHA-256
`f70dd3c87fcecab22fa2af8e5d6bc48cad06bf478581733ca25cfe9c66a9b905`;
the external verification attestation SHA-256 is
`84dc10435fdeefcc6ec4823c86f6d604a412e7c5db3036d364bcd36298fb3a61`.
The initially malformed delimiter inventory is preserved as historical
failed-closed evidence and is not used for custody. This adds no algorithm or
implementation change.
