# G6 bounded content-defined extent-tree cost model

Status: analytical research model; no G6 implementation or measured row exists.

This document separates **Observed**, **Derived**, and **Estimate** values. It
does not project wall time from bytes or object counts.

## 1. Symbols and model profile

| Symbol | Meaning |
|---|---|
| `B` | Logical payload bytes |
| `E` | Ordered CDC chunk occurrences / canonical leaf extents |
| `L` | Leaf-node count |
| `I_j` | Internal nodes at level `j` |
| `H` | Root-to-leaf edge count |
| `R` | Returned range bytes |
| `C_R` | Payload chunks intersecting a range |
| `J` | Retained small-edit revision count |
| `Q` | Exact simultaneously live owned capacity |
| `k` | Normalized old-coordinate mutation islands, `0..=64` |
| `c` | CDC influence clusters after deterministic coalescing, `c<=k` |
| `R_mut` | Total streamed replacement bytes |
| `C` | Unique CDC bytes scanned across replacement/restart/rejoin input |
| `D_l` | Leaf occurrences replayed before exact tree rejoin |
| `D_j` | Child descriptors replayed at internal level `j` |
| `B_s` / `E_s` | Raw-byte / occurrence suffix after earliest unresolved cluster |
| `DeltaB` | Final raw-byte-length delta |
| `DeltaE` | FastCDC occurrence-count delta; independent of `DeltaB` |

Observed inputs:

```text
CDC minimum / target / maximum  8,192 / 16,384 / 32,768 bytes
canonical-v2 leaf occurrence    36 bytes
current child descriptor        40 bytes
current K/F                     64 / 64
reference counts                53 / 531 / 5,284 / 26,533
                                at 1 / 10 / 100 / 500 MiB
```

Candidate research profile:

```text
leaf occurrence                 raw_length:u32 + ObjectId[32] = 36 bytes
internal descriptor             subtree_raw_length:u64
                              + subtree_extent_count:u64
                              + child_id[32] = 48 bytes
provisional grouping            natural cut after 32; forced cut at 64
```

`raw_length` retains the current legal `0..=32,768` range; zero-length
occurrences count as descriptors and identity inputs but contribute zero
logical bytes. The 32/64 rule is a shadow variable, not a product profile. The
shadow predicate is frozen byte-for-byte in the specification and benchmark
plan; selecting it for a durable product profile still depends on shadow
evidence.

Under an explicitly illustrative iid-digest model for the low-five-bit
predicate:

```text
q = 31/32
expected occupancy = 31 + sum(k=0..32, q^k) = 51.7763 entries
forced cut at 64   = q^32                 = 36.2055%
```

Both values are **Estimates**, not observations or guarantees.

## 2. Exact encoded-node equations

The current Phase-1 canonical `Bytes` framing plus mapping envelope gives these
derived candidate sizes:

```text
leaf(n)     = 28 + 36*n
internal(c) = 29 + 48*c
root(c)     = 49 + 48*c
```

For the current v2 40-byte descriptor, replace `48*c` with `40*c`.

Candidate maximums at 64 entries/children:

```text
leaf_max     = 28 + 36*64 = 2,332 bytes
internal_max = 29 + 48*64 = 3,101 bytes
root_max     = 49 + 48*64 = 3,121 bytes
```

These are canonical mapping bytes, not SQLite page bytes, filesystem allocated
bytes, or resident memory.

## 3. Live topology and metadata

Two topology columns are modeled:

- **packed 64**: the current reference counts grouped at 64, useful as the
  minimum ordinary candidate footprint;
- **32-entry envelope**: every nonfinal group closes at 32, the maximum node
  count under the provisional CD32–64 rule.

The root remains minimal and may contain up to 64 children.

| Logical size | `E` | Height | Packed candidate topology | Packed candidate bytes | 32-entry topology | 32-entry candidate bytes |
|---:|---:|---:|---|---:|---|---:|
| 1 MiB | **Observed 53** | **Derived 1** | 1 leaf, root | **Derived 2,033** | 2 leaves, root | **Derived 2,109** |
| 10 MiB | **Observed 531** | **Derived 1** | 9 leaves, root | **Derived 19,849** | 17 leaves, root | **Derived 20,457** |
| 100 MiB | **Observed 5,284** | **Derived 2** | 83 leaves, 2 internal, root | **Derived 196,735** | 166 leaves, 6 internal, root | **Derived 203,351** |
| 500 MiB | **Observed 26,533** | **Derived 2** | 415 leaves, 7 internal, root | **Derived 987,316** | 830 leaves, 26 internal, root | **Derived 1,020,319** |

Payload ratios:

| Size | Packed candidate | 32-entry envelope | Provisional `<1%` gate |
|---:|---:|---:|:---:|
| 1 MiB | **Derived 0.1939%** | **Derived 0.2011%** | PASS by model |
| 10 MiB | **Derived 0.1893%** | **Derived 0.1951%** | PASS by model |
| 100 MiB | **Derived 0.1876%** | **Derived 0.1940%** | PASS by model |
| 500 MiB | **Derived 0.1883%** | **Derived 0.1946%** | PASS by model |

At exactly the 16-KiB CDC target, occurrence bytes alone are:

```text
36 / 16,384 = 0.2197%
```

Node envelopes and internal descriptors keep the ordinary analytical total
near 0.23%, still below 1%. SQLite pages, indexes, receipts, transitions,
journals, and allocation granularity are additional and must be observed.

### Current exact anchor

The retained canonical-v2 100-MiB mapping is:

```text
5,284 references
83 leaves
2 branches
1 root
86 mapping objects
196,055 file-mapping bytes
```

The candidate packed model is 680 bytes larger because every one of the 85
non-root child descriptors adds an 8-byte subtree extent count:

```text
196,055 + 8*(83 + 2) = 196,735 bytes
```

This is a **Derived** codec cost, not a measured database result.

## 4. Tree height and range routing

With min 32 and max 64 grouping, nonfinal levels have bounded fanout and:

```text
H = O(log_32 E)
```

The final tail and root have explicit exceptions but do not invalidate the
height bound.

Observed/derived path counts for the retained reference populations:

| Size | Packed path | 32-entry-envelope path |
|---:|---:|---:|
| 1 MiB | root + leaf | root + leaf |
| 10 MiB | root + leaf | root + leaf |
| 100 MiB | root + internal + leaf | root + internal + leaf |
| 500 MiB | root + internal + leaf | root + internal + leaf |

Range complexity with bounded linear search inside each node is:

```text
T_range = O(64*H + C_R + R)
Q_range = O(H*node_max + bounded chunk batch + bounded output)
```

The factor 64 is fixed profile work, not `O(E)`.

With `subtree_extent_count:u64`, minimum fanout 32, and the minimal-root rule,
the provisional codec accepts at most internal output level 11, 12
root-to-leaf edges, and 13 simultaneously active root/path nodes. This is a
**Derived** bound that the shadow must mechanically reverify before rows.

### Range chunk counts

- A 4-KiB request ordinarily intersects one chunk and at most two interior
  chunks because the CDC interior minimum is 8 KiB. A short final chunk is a
  separately tested boundary.
- On the retained 100-MiB fixture, average chunk length is:

```text
104,857,600 / 5,284 = 19,844.36 bytes
```

So a middle 1-MiB range estimates:

```text
ceil(1,048,576 / 19,844.36) + boundary slack ~= 54 chunks
```

CP-0009 **Observed** 60 authenticated objects and 1,090,255 canonical bytes to
return 1,048,576 bytes. The G6 resolver should preserve or reduce that work.

Hard range bound from the 8-KiB minimum is approximately 129 intersecting
chunks for 1 MiB plus explicitly handled EOF behavior. With a 64-object batch,
payload acquisition needs at most three bounded batches.

## 5. Full construction and reconstruction

Fresh construction remains:

```text
time       Theta(B + E)
resident   O(CDC window + one leaf + one partial node per level + SQL batch)
mapping    Theta(E)
```

Full logical reconstruction and cold native export remain:

```text
logical reconstruction    Theta(B + E)
native destination writes Omega(B)
```

An extent tree cannot honestly improve those lower classes. Its opportunities
are fewer statements/copies, bounded cursors/batches, and avoiding unrelated
work for partial reads or edits.

## 6. Structural count-change and raw mutation cost

### Current observed work

Current canonical-v2 100-MiB **structural occurrence-insertion** counters:

```text
                        total operation   file mapping only
early +1                  196,375 B          196,091 B
middle +1                 100,763 B          100,479 B
same-count                  5,334 B            5,050 B
fixed non-file component                         284 B
```

CP-0008 v1 scale evidence shows suffix references and bytes grow about 5x from
100 to 500 MiB. Its `+1` operation inserted one standalone occurrence; it was
not a raw one-byte FastCDC edit. These observations prove fixed-radix suffix
scaling only and cannot predict `DeltaE` for the raw mutation ladder.

### Candidate ordinary local path

If one node changes at each level:

```text
height 1: leaf_max + root_max
        = 2,332 + 3,121
        = 5,453 bytes

height 2: leaf_max + internal_max + root_max
        = 2,332 + 3,101 + 3,121
        = 8,554 bytes
```

If one split rewrites two nodes at each non-root affected level:

```text
height 1 split envelope
  = 2*2,332 + 3,121
  = 7,785 bytes

height 2 cascading split envelope
  = 2*2,332 + 2*3,101 + 3,121
  = 13,987 bytes
```

Corresponding new mapping-node counts are **Derived**:

```text
height 1 normal / one split       2 / 3 nodes
height 2 normal / cascading split 3 / 5 nodes
```

All exactly converged later mapping subtrees are reused by `ObjectId`. All
unchanged payload objects are reused; only the bounded CDC replacement window
may create new payload objects. Exact reused counts remain Observed shadow/
product counters because cut convergence is sequence-dependent.

These are **Estimates** contingent on local content-defined rejoin. They do not
include payload chunks, namespace wrapper, transition, receipt, or SQLite page
amplification.

### Position and operation envelope

Raw insertion/deletion changes byte length, but frozen FastCDC independently
determines whether `DeltaE<0`, `=0`, or `>0`. The fixed-radix suffix envelope
applies only when the frozen full oracle shows an occurrence-count change. A
`DeltaE=0` raw edit instead protects the current changed-spine class.

| Size | Current early/middle/late suffix occurrences (**Derived**) | Candidate ordinary normal/split mapping bytes (**Estimate**) | Candidate hard fallback |
|---:|---:|---:|---:|
| 1 MiB (`E=53`) | 53 / 27 / 1 | 5,453 / 7,785 | through remaining 53/27/1 occurrences |
| 10 MiB (`E=531`) | 531 / 266 / 1 | 5,453 / 7,785 | through remaining 531/266/1 occurrences |
| 100 MiB (`E=5,284`) | 5,284 / 2,642 / 1 | 8,554 / 13,987 | through remaining 5,284/2,642/1 occurrences |
| 500 MiB (`E=26,533`) | 26,533 / 13,267 / 1 | 8,554 / 13,987 | through remaining 26,533/13,267/1 occurrences |

The table describes structural occurrence positions, not a raw-byte oracle.
The 500-MiB row is analytical only.

### Raw mutation-magnitude ladder

All positions are derived from old logical length, never chunk boundaries:
`early=L/8`, `middle=L/2`, and `late=7L/8`. Exact byte spans and inserted-byte
digests are frozen in the benchmark plan.

| File | Raw mutation cases | Necessary changed-input work | Expected tree behavior |
|---:|---|---:|---|
| 1 MiB | `+/-1 B` early/middle/late; `+/-4 KiB` middle | replacement stream plus local CDC restart/rejoin | ordinarily one local cluster/path; exact `DeltaE` from oracle |
| 10 MiB | `+/-64 KiB` middle; one atomic four-island mixed edit | about four target chunks for 64 KiB plus boundary/resync; four union paths | bounded multi-island, shared ancestors counted once |
| 100 MiB | `+/-1 MiB` middle; append/truncate 1 MiB; two-island net-zero 4-KiB shift | about 53 target chunks for 1 MiB at observed average density plus boundary/resync | multiple local leaves; tail is `EofLocal`; middle/net-zero may fallback |

Chunk estimates are illustrative. The independent full frozen-FastCDC oracle
supplies exact old/new occurrence counts and target roots. A 1-MiB mutation is
not expected to match a 1-byte mutation's latency.

Exact compound accounting:

| Case | `k` | Inserted stream `R_mut` | Deleted logical bytes | `DeltaB` | Ordinary cluster bound |
|---|---:|---:|---:|---:|---:|
| 100-MiB net-zero early insert/late delete | 2 | 4,096 | 4,096 | 0 | `c<=2` |
| 10-MiB mixed two-insert/two-delete | 4 | 69,632 | 69,632 | 0 | `c<=4` |
| 100-MiB middle insert 1 MiB | 1 | 1,048,576 | 0 | +1,048,576 | `c<=1` |
| 100-MiB middle delete 1 MiB | 1 | 0 | 1,048,576 | -1,048,576 | `c<=1` |
| Tail append/truncate 1 MiB | 1 | 1,048,576 / 0 | 0 / 1,048,576 | +/-1,048,576 | EOF-local |

For multiple islands, mapping nodes are counted over the union of changed
paths:

```text
new_mapping_nodes
  = one root
  + unique changed/split leaves
  + unique changed/split internal nodes
```

Shared ancestors are not charged once per island.

### Expected and worst complexity

```text
ordinary/expected
  O(k*H + C + D_l + sum(D_j))

ordinary local form
  O(R_mut + sum(local_resynchronization_i) + k*H)

memory
  O(k + H*max_node_entries + one <=1MiB segment + bounded buffers)

bounded fail-closed
  O(R_mut + unique bounded probes + kH), then no publication

successful earliest-unresolved raw fallback
  Theta(R_mut_remaining + B_s + E_s)

mapping-only public-hash fallback after occurrences exist
  Theta(E_s)
```

No-cut/every-cut/duplicate/adversarial boundary streams can prevent local
rejoin. CDC influence windows that overlap the next logical island coalesce;
one earliest unresolved cluster falls back at most once. The hard maximum keeps
`Q` and node size bounded; it does not make edit work hard logarithmic. Deleted
logical bytes need not be fetched byte-for-byte when authenticated subtree
measures prove removal.

## 7. Retained-history model

Tree-only immutable history if every edit remains within the one-path or
one-split envelopes:

| Retained edits `J` | Height-1 normal | Height-1 split every edit | Height-2 normal | Height-2 cascading split every edit |
|---:|---:|---:|---:|---:|
| 1 | 5,453 B | 7,785 B | 8,554 B | 13,987 B |
| 10 | 54,530 B | 77,850 B | 85,540 B | 139,870 B |
| 100 | 545,300 B | 778,500 B | 855,400 B | 1,398,700 B |
| 1,000 | 5,453,000 B | 7,785,000 B | 8,554,000 B | 13,987,000 B |

Classification: **Estimate**. Add separately:

```text
sum(new local canonical payload objects)
+ namespace/root/transition/receipt objects
+ SQLite row/page/index overhead
+ unreachable objects retained before GC
+ journal/temp peaks
```

H11-v9 **Observed** a different, deterministic 1-MiB same-size workload:

```text
6 objects/revision
23,030 canonical bytes/revision
2,255 mapping bytes/revision
24,858.9069 logical/apparent SQLite bytes/revision
```

Those slopes are protected evidence, not a G6 prediction. G6 must measure its
own current-live, retained-union, unreachable, logical, apparent, and allocated
slopes.

## 8. Fragmentation

Canonical fragmentation is:

```text
tree_node_fill = actual entries / 64
mapping_fragmentation = actual mapping bytes / packed-64 mapping bytes
```

The provisional CD32–64 hard envelope at 100 MiB is:

```text
203,351 / 196,735 = 1.03363
```

or about 3.36% above the candidate packed model. At 500 MiB:

```text
1,020,319 / 987,316 = 1.03343
```

This is a codec/topology envelope, not APFS fragmentation or physical
allocation. Native extent fragmentation and allocated storage require
filesystem observations.

## 9. SQL and CAS crossings

The portable resolver should use one bounded cursor and batches rather than one
SQLite query per output extent.

Required derived equations:

```text
mapping_fetches
  = root_fetches + internal_fetches + leaf_fetches

payload_batch_queries
  = ceil(intersecting_payload_objects / frozen_batch_capacity)

range_amplification
  = authenticated_canonical_bytes / returned_bytes

retained_mapping_growth
  = sum(new_leaf_bytes + new_internal_bytes + new_root_bytes + wrappers)
```

G4 **Observed** full 100-MiB traversal at 170 SQL queries, 5,371 objects, 83
leaf batches, and 5,284 payload BLOB reads. G6 must preserve or improve that
shape. A statement-count drop alone is not a wall-time prediction.

## 10. Resident memory and resources

Provisional resolver ownership:

```text
root-to-leaf decoded nodes      <= H * about 3.2 KiB
64 maximum canonical chunks    <= 64 * about 32.8 KiB ~= 2.1 MiB
returned/output buffer          <= 1 MiB
CDC/edit/resync state           bounded separately
```

Initial target:

- no `O(B)` or `O(E)` resident mapping;
- additional G6 resolver `Q <=4 MiB` on the scheduled operations;
- individual owned buffer `<=1 MiB`;
- at most 64 normalized island descriptors, one live replacement segment
  `<=1 MiB`, one 32-KiB FastCDC buffer, and one coalesced CDC cluster;
- simultaneously live decoded descriptor count
  `O(max_node_entries * active_height + frozen batch capacity)`; total
  persistent descriptors remain `Theta(E)`;
- no unbounded pending ranges, extents, proof vectors, history vectors, or
  decoded-node caches;
- every capacity checked and charged to its live owner;
- terminal Q, scopes, transactions, descriptors, temps, and pending projection
  state exactly zero.

An exact RSS ceiling is **Unavailable pending the sealed G5 control**. G4's
20,578,304-byte maximum and H11's 14,090,240-byte maximum describe different
processes and cannot be silently reused as the G6 gate.

## 11. Native projection model

| Route | Logical/native work | Native-output physical I/O classification |
|---|---|---|
| virtual resolver | Mapping/CAS range only; zero full native bytes | Native output-file I/O `NotApplicable`; CAS/SQLite physical I/O `Observed` when supported or `Unavailable(source/reason)` |
| same-size clone+patch | Clone metadata + changed ranges + sync/publication | Physical bytes Unavailable unless directly observed |
| TailAppend | Whole clone plus appended replacement bytes; shifted suffix 0 | Changed logical writes; physical bytes Unavailable unless directly observed |
| TailTruncate | Whole clone plus truncate; shifted suffix 0 | Metadata/COW dependent; physical bytes Unavailable |
| APFS CloneShiftPatch | For suffix `S` and new span `N`: shift read `S`, shift write `S`, patch `N`; derived transfer `2S+N`; logical work `Theta(S+N)` | Unavailable unless directly observed; `S`, `2*S+N`, allocation, RSS, and wall are not physical-I/O substitutes |
| Linux InsertCollapsePatch | Whole clone, aligned extent operation, boundary/changed patch | Unavailable unless directly observed by a supporting source |
| Linux RangeReflinkSplice | Shared aligned prefix/suffix extents plus boundary patch | Unavailable unless directly observed; reflinked logical bytes are shared mappings, not media-transfer bytes |
| FullFallback | Full authenticated target stream and destination write; `Theta(T+E)` and at least `Omega(T)` destination logical writes | Unavailable unless directly observed; the logical-write lower bound is not a physical-I/O lower bound |
| ColdFullExport | Full authenticated standalone export; `Theta(T+E)` and at least `Omega(T)` destination logical writes | Unavailable unless directly observed; the logical-write lower bound is not a physical-I/O lower bound |

APFS logical clone bytes, allocated blocks, RSS, or wall time are not physical
I/O. Linux reflinked bytes are shared logical mappings, not proof of stable
media transfer.

For the scheduled 100-MiB middle `+1 MiB` APFS cell, `S=50 MiB`; expected
wrapper counters are 50-MiB shift read, 50-MiB shift write, and 1-MiB patch.
The Linux cell uses the same aligned `+1 MiB` geometry so the forced
FullFallback cell is an exact-operation comparator. Stage-B capability
preflight selects exactly one Linux mechanism before rows; unsupported cells
remain `NotApplicable`.

## 12. Required direct observations

The later shadow/candidate must report or classify:

- tree height and node occupancy histogram;
- file size, raw mutation magnitude/position, input/normalized islands,
  `DeltaB`, independent oracle `DeltaE`, and CDC cluster count;
- old/derived-new coordinates, cumulative deltas, replacement/source digests,
  unique CDC scan, replacement extents, and per-level replay;
- natural and forced cuts by level;
- replay/resync entries by level;
- new/reused leaves/internal nodes and encoded bytes;
- unchanged suffix payload fetch/write count;
- unchanged suffix subtree reuse count and logical bytes;
- range mapping nodes, fragments, CAS fetches/batches, authenticated/returned
  bytes;
- split/merge/root grow/root shrink;
- Q component equation/current/high-water/terminal;
- RSS, buffers, descriptors;
- current-live and retained-history objects/bytes;
- SQLite workload versus instrumentation statements;
- transactions, COMMIT dispatch/return/reconciliation;
- logical/apparent/allocated database/journal/authority/native endpoints;
- requested/selected/outcome native route, capability hash, shifted/reflinked/
  fallback logical bytes, ioctl results/errno, and tail/truncate calls;
- physical I/O as Observed only when a supporting API exists, otherwise
  Unavailable with reason.

## 13. Cost conclusion

The model establishes three useful facts:

1. Whole-chunk canonical extents retain sub-0.25% ordinary metadata at the
   current CDC density, far below the 1% provisional limit.
2. A locally rejoining CD32–64 tree could reduce structural 100-MiB
   count-change mapping from 100–196 KiB to roughly 5–14 KiB for a one-path
   case; variable magnitudes legitimately add replacement extents and union
   paths.
3. That reduction is conditional. Raw mutation work grows with replacement
   bytes and unique CDC resynchronization. The hard successful fallback can
   scan a raw and mapping suffix, so the shadow/screen—not this model—must
   decide whether the architecture advances.
