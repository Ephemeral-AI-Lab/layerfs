# Phase 4 G6 fast research and benchmark plan

Status: **PROSPECTIVE / NO G6 MEASUREMENT AUTHORIZED**

Research disposition: **`G6_SPEC_READY_PENDING_G5_BASELINE`**

This plan contains three distinct stages:

1. a metadata-only research shadow that runs only after terminal G5 PASS under
   the updated responsibility boundary;
2. a `<20 s` product mechanism screen that remains blocked by the shadow and
   sealed G5 baseline;
3. one `<=150 s` complete measured gate that remains blocked by a passing
   mechanism screen.

The 20- and 150-second limits are **total complete-wall limits**, never per row
or per operation.

## 1. Authority and prerequisites

### Metadata-only shadow prerequisites

- terminal G5 PASS sealed; until then G6 remains research/specification only;
- read-only review accepts this research disposition;
- no benchmark/product source is modified;
- the exact reference manifests and expected roots for A/current are frozen;
- shadow source, schedule, cut predicate, limits, and analyzer hashes are
  frozen before any shadow result;
- the shadow operates on reference metadata only and acquires no global
  measured benchmark lock;
- immediately before execution, fail closed if the global benchmark lock, a
  G5 measured process, or a competing Cargo process is active; abort/wait
  rather than perturb the active G5 owner.

### Product screen prerequisites

All must exist before G6 Rust implementation or screen preparation:

```text
shadow terminal PASS
exact selected cut/profile bytes
sealed G5 terminal PASS
G5 source/diff/executable/fixture/input/manifests
reusable engine/schema boundary selected
segmentation-witness codec/schema/reopen/reconciliation lifecycle frozen
shared raw-mutation frontend and independent full-FastCDC oracle frozen
fixed-radix A reference consumes the same oracle occurrences as candidate C
G6 product source implemented once in that reusable boundary
reusable virtual-root install endpoint consumes that same engine resolver
native materializer consumes that same engine resolver
focused promotion-readiness audit PASS for both consumers
focused tests PASS
workspace/clippy/fmt/diff closure PASS on frozen source
```

If final G5 does not provide those reusable consumer boundaries, Stage C/D
waits for a separate prospective integration amendment. A benchmark-local
virtual endpoint or copied native materializer is not admissible.

### Measured gate prerequisites

- one complete `<20 s` mechanism screen passed;
- exact product source and release binary frozen once;
- exact retained control binary and candidate binary frozen;
- zero-row gate schedule/budget proof passed;
- no other measured campaign/Cargo process/global lock owner exists;
- inputs, process shapes, cache classes, trust modes, and durability match.

## Raw mutation input contract

Raw positions derive only from old logical length, never candidate chunk/tree
boundaries:

```text
early(L)  = L / 8
middle(L) = L / 2
late(L)   = 7*L / 8
```

| Fixture | Length | Early | Middle | Late |
|---:|---:|---:|---:|---:|
| 1 MiB | 1,048,576 | 131,072 | 524,288 | 917,504 |
| 10 MiB | 10,485,760 | 1,310,720 | 5,242,880 | 9,175,040 |
| 100 MiB | 104,857,600 | 13,107,200 | 52,428,800 | 91,750,400 |

Stable raw cases are:

| Case | Old-coordinate mutation |
|---|---|
| `RAW-I1-E/M/L` | Insert 1 byte at the corresponding scheduled fixture position |
| `RAW-D1-E/M/L` | Delete `[position, position+1)` on the scheduled fixture |
| `RAW-I4K-M` / `RAW-D4K-M` | Insert 4,096 bytes at 1-MiB middle / delete `[524288,528384)` |
| `RAW-I64K-M` / `RAW-D64K-M` | Insert 65,536 bytes at 10-MiB middle / delete `[5242880,5308416)` |
| `RAW-I1M-M` / `RAW-D1M-M` | Insert 1,048,576 bytes at 100-MiB middle / delete `[52428800,53477376)` |
| `RAW-APPEND1M` | Insert 1,048,576 bytes at old EOF 104,857,600 |
| `RAW-TRUNCATE1M` | Delete `[103809024,104857600)` |

Inserted bytes are frozen before rows:

```text
preimage =
  b"layerfs/g6/edit-payload/v1\0"
  || base_raw_blake3[32]
  || u32be(case_id.len) || case_id
  || u32be(island_ordinal)
  || u64be(old_start)
  || u64be(replacement_length)

replacement_bytes = BLAKE3-XOF(preimage, replacement_length)
```

Retain each preimage, declared length, SHA-256, and BLAKE3. A and C receive
byte-identical old bytes and replacement streams.

### Atomic 100-MiB net-zero case

`RAW-NZ4K-E-L` is one request over the pinned old root:

```text
island 0: insert 4,096 bytes at [13,107,200, 13,107,200)
island 1: delete old bytes [91,750,400, 91,754,496)

derived new starts: 13,107,200 and 91,754,496
DeltaB = 0
transactions = 1
publication COMMITs = 1
```

Equal length is not “same-count”: the fresh FastCDC oracle supplies `DeltaE`.

### Atomic 10-MiB mixed case

`RAW-MIX4-10M` is one four-island request in old coordinates:

```text
island 0: insert 4 KiB at [1 MiB, 1 MiB)
island 1: delete 4 KiB at [3 MiB, 3 MiB + 4 KiB)
island 2: insert 64 KiB at [6 MiB, 6 MiB)
island 3: delete 64 KiB at [9 MiB, 9 MiB + 64 KiB)

derived new starts:
  1,048,576
  3,149,824
  6,291,456
  9,502,720

DeltaB = 0
transactions = 1
publication COMMITs = 1
```

The independent full raw oracle streams old spans and replacement sources,
runs frozen FastCDC from byte zero, and freezes the target occurrence
commitment. A maps that sequence with current canonical-v2 K64/F64; C maps it
with the G6 tree. Edit-built C must equal a fresh C build. A/C roots are
profile-specific and need not match; logical bytes, output digest, requested
mutation, trust, durability, and endpoint do.

The main raw performance ladder, including its payloads and positions, remains
fixed regardless of the observed `DeltaE` distribution. It is never replaced
or tuned to obtain favorable occurrence deltas.

A separate correctness-only sign fixture proves `DeltaE<0`, `=0`, and `>0`.
Before product rows, an independent oracle evaluates the predeclared sequence
`SIGN-0000` through `SIGN-0255` on the pinned 1-MiB base at the fixed middle
position `M=524,288`. For integer case ID `n`, let `r=floor(n/4)+1`, so
`1<=r<=64`, and select the frozen shape by `n mod 4`:

```text
0  insert r generated bytes at [M,M)
1  delete old bytes [M,M+r)
2  replace old bytes [M,M+r) with r generated bytes
3  replace old bytes [M,M+r) with (65-r) generated bytes
```

Generated bytes use the same BLAKE3-XOF contract above with the sign case ID.
Retain every evaluated case and freeze the first lexicographic case for each
sign. If the bounded sequence lacks a sign, the focused semantic fixture is
REVISE; the performance matrix remains unchanged. These selected cases enter
only focused correctness/fault evidence, never wall-time inference,
representative-locality claims, or a 10x gate.
CP-0008's synthetic occurrence insertion is not a raw oracle or wall-time
control.

## 2. Stage A — metadata-only A/B/C shadow

Purpose: decide whether the candidate can be canonical, local on ordinary
LayerFS sequences, resource-bounded under adversarial sequences, and small
enough to justify product implementation.

It performs no payload read/write, SQLite work, durability, native projection,
or OS integration.

### Arms

```text
A  exact current canonical-v2 K64/F64 grouping
B  exact Xet-style 3–9 negative/reference arm
C  provisional bounded CD32–64 measured sequence tree
```

B is explanatory only and cannot be selected if it violates the metadata/
object bounds. It is pinned to
[`huggingface/xet-core@af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7`](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_core_structures/src/merklehash/aggregated_hashes.rs):
the shadow supplies `(ObjectId interpreted as the pinned MerkleHash byte
layout, raw_length as u64)` tuples; `next_merge_cut` returns the remaining
length for final tails of one or two, otherwise examines zero-based child
indices 2 through 8, closes inclusively at the first hash with `hash % 4 == 0`,
and forces the inclusive cut at 9. Parent hashes use the pinned
`merged_hash_of_sequence` byte format. Independent vectors freeze empty,
unary/two-child tails, natural index 2/8, forced 9, repeated hashes, and
multi-level aggregation before rows.

### Frozen C predicate

The first shadow uses the provisional rule in the G6 specification:

```text
preimage = b"layerfs/g6/cd32-64/cut/v1\0"
           || role_u8
           || output_level_u8
           || u32be(entry_bytes.len)
           || entry_bytes
digest = BLAKE3(preimage)
marker = (digest[0] & 0x1f) == 0

role 0x01 = leaf occurrence; role 0x02 = internal descriptor
leaf output level 0; internal output level = child level + 1
positions 1..31 ineligible
first marker at positions 32..63 closes inclusively
position 64 closes inclusively regardless of marker
complete top stream <=64 embeds directly in root regardless of marker

mapping codec role bytes (a separate domain):
root 0x01; leaf 0x02; internal 0x07
```

Before row 1, independent golden vectors must agree for empty-root, one-leaf,
and one-internal canonical bytes and ObjectIds. A role-byte or vector mismatch
is a zero-row failure.

Any predicate change creates a new shadow method before new rows. It is not a
post-observation tuning knob.

### Exact shadow schedule

Forty-three ordered records:

| Group | Population | Rows |
|---|---|---:|
| S0 | A/B/C fresh build for retained 1/10/100-MiB reference sequences | 9 |
| S1 | C-only raw 1-MiB `RAW-I1/D1-E/M/L`, `RAW-I4K-M`, `RAW-D4K-M` oracle manifests | 8 |
| S2 | C-only raw 10-MiB `RAW-I64K-M`, `RAW-D64K-M`, atomic `RAW-MIX4-10M`, inverse roundtrip, divergent-history equality | 5 |
| S3 | A/C pairs for 100-MiB `RAW-I1M-M` and `RAW-D1M-M`; C-only `RAW-NZ4K-E-L` | 5 |
| S4 | A/C pairs for `RAW-APPEND1M` and `RAW-TRUNCATE1M` | 4 |
| S5 | A/C structural occurrence tests `OCC-I1-E` and `OCC-D1-M`; never called raw-byte edits | 4 |
| S6 | C only: S6-01 no-cut; S6-02 every-cut; S6-03 repeated IDs plus legal zero-length occurrences; S6-04 marker-at-min; S6-05 forced-max; S6-06 singleton tail; S6-07 parent no-cut plus direct-root adversaries for top counts 32–64; S6-08 parent every-cut plus the same 32–64 root adversaries | 8 |
| **Total** |  | **43** |

A/C pair order is exactly `AB,BA,AB,BA,AB,BA` for
`RAW-I1M-M`, `RAW-D1M-M`, append, truncate, `OCC-I1-E`, and `OCC-D1-M`:
3 AB and 3 BA. Raw rows consume frozen full-FastCDC manifests; structural
occurrence rows manipulate occurrence sequences only.

No 500-MiB payload or reference population is run. The retained 500-MiB
reference count is used only in the analytical cost model.

### Shadow timers and counters

Observed for each row:

- fresh/rebuild wall;
- leaf/internal/root counts and bytes;
- height and occupancy by level;
- natural/forced cuts by level;
- entries replayed until rejoin by level;
- nodes read/reused/rewritten;
- unchanged suffix subtrees and logical extents reused;
- full fresh root and edit root;
- current/high-water/terminal exact shadow Q;
- maximum node and buffer capacity.

Raw rows additionally retain `DeltaB`, oracle `DeltaE`, island/cluster counts,
old/derived-new coordinates, replacement extent count, CDC restart/rejoin
cursors and bytes, per-level replay, union-path nodes, and fresh/edit occurrence
commitments. Metadata-only rows read no payload; these values come from the
frozen oracle manifests and simulated occurrence streams.

The complete shadow wall begins before record 1 and ends only after both
independent analyzers, result fsync, cleanup, and terminal marker. It must be
strictly less than 20 seconds.

### Shadow retain rule

Hard semantic gates:

- A reproduces every current expected root and byte count;
- every raw A/C edit root equals a fresh build from the same frozen full-
  FastCDC occurrence manifest under its own profile;
- C fresh and every edit history produce the same root for equal final ordered
  occurrence sequences; this does not by itself prove raw-byte canonical CDC
  segmentation;
- exact decoder/validator rejects every alternate encoding;
- all 43 rows return terminal Q zero;
- no node exceeds 64 entries or the derived encoded maximum;
- adversarial rows remain bounded and honestly report suffix fallback.

Required structural-occurrence signal:

```text
OCC-I1-E file-mapping work   <= floor(A row / 10)
OCC-D1-M file-mapping work   <= floor(A row / 10)
live mapping            <= 205,857 bytes
one-path structural bytes    <= current path + 1,024 bytes
```

For raw A/C cells, apply 10x only when the frozen oracle has `DeltaE!=0`, the
route is non-tail and locally rejoins, and A actually performs suffix mapping
work. If `DeltaE=0`, protect the A changed-spine class plus exact format delta.
Append/truncate use EOF-local and protected gates, never a blind 10x rule.
File mapping, fixed non-file mapping, and complete operation counters remain
separate.

If semantic/root/canonicality fails: **`REVISE_EXTENT_TREE_THESIS`**, stop.

If canonicality passes but ordinary locality or live/path gates fail: preserve
the shadow as negative evidence and either revise the exact structure or reject
G6 product implementation. Do not run unchanged for favorable noise.

Adversarial suffix fallback alone is not a failure because it is the declared
hard limitation; an unbounded node/Q/path or hidden fallback is a failure.

## 3. Stage B — zero-product-row product preflight

This stage constructs and verifies the eventual screen/gate without invoking
the product binary.

It must prove:

- exact branch, HEAD, status, source diff, toolchain, SQLite, host/filesystem,
  fixture, input, control, and candidate hashes;
- the G5 terminal baseline is sealed and its manifests rehash;
- control and candidate use the same input bytes, requested operations, trust
  mode, durability, process shape, and cache/preconditioning class;
- exact 1/10/100-MiB fixture roots and reference manifests;
- exact raw mutation cases, old-coordinate islands, deterministic inserted-byte
  digests, fresh full-FastCDC occurrence commitments, `DeltaB`, and oracle
  `DeltaE`;
- exact root/profile-bound segmentation-witness bytes, same-transaction
  publication, reopen lookup, and requested/prior/different/ambiguous vectors;
- exact `NativeCapabilitySetV1` schema, product-wrapper probe command,
  conditional schedule, expected environment envelope, and hash procedure;
  the actual receipt is captured only inside each locked screen/gate wall;
- the complete screen schedule has 48 product rows;
- the complete measured schedule has 100 product rows;
- balanced order assertions;
- total screen forecast `<20,000,000,000 ns`;
- total measured forecast `<=150,000,000,000 ns`;
- one lock intent/token and fail-closed release plan;
- no existing result root is overwritten;
- no product row, lock acquisition, or measured campaign occurs in dry run.

Forecast values are feasibility bounds, not observed product timing.
Capability probing uses tiny disposable files and is not a performance row,
but its execution, cleanup, and receipt fsync remain inside complete screen/
campaign wall. The conditional route population is frozen before the probe;
unsupported cells emit NotApplicable and are never substituted.

## 4. Stage C — `<20 s` product mechanism screen

A frozen release build and immutable shared-fixture creation may occur once
outside the screen wall and are timed separately. The `<20 s` complete screen
wall begins before attempt-local adoption/preconditioning, lock acquisition,
and product child/session startup. It includes all 48 rows, analysis, cleanup,
lock release, and terminal evidence fsync. Use long-lived product children and
prepared deterministic bases; do not create one process per tiny operation
when a matched session can preserve exact process shape.

```text
complete_screen_wall
  = attempt_local_adoption_and_preconditioning
  + lock_and_process_startup
  + native_capability_probe_cleanup_and_receipt_fsync
  + all_48_product_rows
  + analysis
  + cleanup
  + lock_release
  + terminal_evidence_fsync
  < 20,000,000,000 ns
```

### Exact 48-row schedule

| Group | Rows | Operations |
|---|---:|---|
| M1 — 1 MiB candidate semantic | 12 | same-size middle; raw `I1/D1` early/middle/late; raw `I4K/D4K` middle; 4-KiB range; whole range; sequential traversal |
| M2 — 10 MiB candidate semantic | 12 | same-size middle; raw `I1/D1` early/middle/late; raw `I64K/D64K` middle; atomic mixed four-island; 1-MiB range; sequential traversal |
| M3 — primary/endpoints | 21 | A/C pairs for six raw 100-MiB `I1/D1` positions (12); candidate-only virtual `I1M`, `D1M`, net-zero, mixed (4); candidate-only native TailAppend, TailTruncate, APFS +1-MiB shift, frozen Linux +1-MiB aligned route, forced +1-MiB FullFallback (5) |
| M4 — history | 2 | candidate 1-MiB 100-edit and 1,000-edit retained sequences |
| M5 — concurrency | 1 | candidate 10-MiB pinned reader across one writer COMMIT |
| **Total** | **48** | |

M3 raw one-byte A/C order alternates by operation: 3 AB and 3 BA. The four
virtual and five native cells are separately keyed candidate-only operations;
they never enter paired percentage claims.

### Immediate stop conditions

Stop after the failing row, preserve all prior output, release the lock, and
do not measure the gate if any occurs:

- canonical payload/root/transition/output mismatch;
- fresh/edit builder root mismatch;
- raw edit occurrence commitment differs from its frozen fresh FastCDC oracle;
- `DeltaB`/`DeltaE`, island/new-coordinate, final-length, or one-COMMIT equation
  mismatch;
- missing/wrong-role/identity/length/cut validation failure;
- unchanged suffix payload fetch/write nonzero on a claimed local route;
- mapping work misses the 10x gate;
- one transaction/one COMMIT mismatch;
- timer equation mismatch;
- Q overflow or terminal nonzero;
- buffer/descriptor/storage/residue failure;
- native full fallback reported as virtual visibility;
- fast native cell silently falls back or reports a combined/unfrozen route;
- complete wall reaches 20 seconds.

### Mechanism signal

Advance only if the complete raw ladder and all structural/native/virtual cells
satisfy:

- canonical ACK `<20 ms` for every mutation/publication row; read-only range
  and traversal rows classify canonical ACK `NotApplicable` and use their
  applicable first-range or traversal endpoint target;
- exact local or honestly classified fallback route;
- structural occurrence and eligible raw `DeltaE!=0` local rows meet their
  prospectively applicable 10x gate; `DeltaE=0`/tail rows meet protected/local
  equations;
- zero unchanged suffix payload fetch/write;
- arbitrary replacement work scales with replacement bytes, unique CDC scan,
  and union tree replay rather than base-file suffix;
- native request/selection/outcome and shifted/reflinked/fallback counters close;
- protected same-count/range/history/concurrency semantics;
- no material source/static/resource defect.

The screen is retain/revise/revert evidence only, never a G6 performance PASS.

## 5. Stage D — `<=150 s` measured gate

One complete campaign, no selective reruns and no per-row cap interpretation.

### Exact 100-row population

#### D1 — 1-MiB semantic breadth: 14 candidate rows

```text
same-size middle
RAW-I1 early / middle / late
RAW-D1 early / middle / late
RAW-I4K-M / RAW-D4K-M
4-KiB range
1-MiB/whole range
sequential traversal
exact reopen
latest reopen
```

#### D2 — 10-MiB scale smoke: 14 candidate rows

```text
same-size middle
RAW-I1 early / middle / late
RAW-D1 early / middle / late
RAW-I64K-M / RAW-D64K-M
atomic RAW-MIX4-10M
1-MiB range
sequential traversal
exact reopen
latest reopen
```

#### D3 — 100-MiB adjacent primary: 36 rows

Every pair uses the same raw mutation frontend and full FastCDC oracle. A uses
current canonical-v2 K64/F64 plus the sealed G5 variable-size `FullFallback`
native endpoint; C uses the G6 tree and selected G6 native route. Canonical
`t4`, native `t10/t11`, and complete `t13` remain separate, so component and
whole-solution claims are both visible. A/C roots are profile-specific; raw
output bytes/digest and native endpoint are exact matches.

Six raw one-byte positional operations receive one adjacent pair each:

```text
RAW-I1-E / RAW-D1-E
RAW-I1-M / RAW-D1-M
RAW-I1-L / RAW-D1-L

6 operations * 1 pair * 2 arms = 12 rows
```

Orders alternate AB/BA: 3/3.

The two middle one-MiB magnitude operations receive three pairs each:

```text
RAW-I1M-M  AB / BA / AB
RAW-D1M-M  BA / AB / BA

2 operations * 3 pairs * 2 arms = 12 rows
```

Append, truncate, and the atomic net-zero shift receive two pairs each:

```text
RAW-APPEND1M    AB / BA
RAW-TRUNCATE1M  BA / AB
RAW-NZ4K-E-L    AB / BA

3 operations * 2 pairs * 2 arms = 12 rows
```

D3 is exactly 18 pairs and 9 AB / 9 BA. The one-byte cells are semantic/current-
scaling controls (`n=1` per arm); one-MiB cells use `n=3`; tail/net-zero use
`n=2`. Every base and replacement stream is byte-identical across its pair.

#### D4 — variable projection diagnostics: 9 candidate rows

Four virtual cells, one observation each:

```text
100-MiB RAW-I1M-M
100-MiB RAW-D1M-M
100-MiB RAW-NZ4K-E-L
10-MiB  RAW-MIX4-10M
```

They require `VirtualNoNativeFile`, `t7/t8`, and native output/calls/bytes
`NotApplicable`; CAS/SQLite I/O remains separately classified.

Five native route cells, one observation each:

```text
TailAppend:       100 MiB -> 101 MiB
TailTruncate:     100 MiB ->  99 MiB
APFS:             RAW-I1M-M, expected shifted suffix 50 MiB
AutoLinuxAligned: RAW-I1M-M, exact route frozen by capability precedence
FullFallback:     forced RAW-I1M-M exact comparator
```

Unsupported APFS/Linux cells retain their scheduled `NotApplicable` receipt
and are not replaced. Candidate-only D4 establishes semantics, direct work,
resources, and absolute single-observation timing—not a population percentage
or protected no-regression claim. D3 supplies matched whole-solution A/B.
Forced fallback uses a reusable explicit product diagnostic policy, never
`cfg(test)`, a benchmark-only semantic copy, or a fixture/root special case.

#### D5 — protected 100-MiB operations: 24 rows

Ten non-compound operations receive one adjacent pair each. The compound first-
after-reopen operation receives two pairs, one `AB` and one `BA`. The ten
single pairs alternate, making D5 exactly 6 AB / 6 BA:

```text
durable full create
same-size early edit
same-size middle edit
same-size late edit
1-MiB range
sequential traversal
exact reopen/head
reopen -> first same-size one-byte edit
authenticated reconstruction
same-size incremental native projection
full native materialization with observed cache class; controlled cold only
when independently established
```

The 4-KiB range and latest-reopen cells remain in D1/D2 semantic breadth; they
are the two substituted protected pairs. D5 retains the expensive and
integration-sensitive full-create/edit/range/reopen/reconstruction/projection/
materialization boundaries.

#### D6 — history and concurrency: 3 candidate rows

```text
1-MiB 1,000 retained edits
10-MiB pinned reader across one writer
10-MiB two independent stores
```

Total:

```text
14 + 14 + 36 + 9 + 24 + 3 = 100 rows
```

Measured roles are exactly 30 A/control arms and 70 C/candidate rows. Paired
positions across D3+D5 are exactly 15 AB and 15 BA; candidate-only D1/D2/D4/D6
never enter paired percentage calculations.

### Complete-wall budget partition

The zero-row preflight must fit the exact schedule into:

| Bucket | Ceiling |
|---|---:|
| Lock/source/input verification | 10 s |
| Prepared-base adoption and process/session startup | 20 s |
| 1/10-MiB semantic rows | 10 s |
| 100-MiB raw-mutation and variable projection rows | 25 s |
| Protected 100-MiB rows | 25 s |
| History/concurrency | 20 s |
| Analysis, custody, cleanup, lock release, terminal fsync | 25 s |
| Reserve | 15 s |
| **Total** | **150 s** |

An overrun terminates the campaign honestly. It does not authorize row removal,
parallel measured work, relaxed cleanup, or a 150-second allowance per bucket.

## 6. Direct counters

Every applicable row reports:

- cell key `(old file size, raw magnitude, position/island list, DeltaE,
  canonical route, projection route)` and separate `DeltaB`/`DeltaE`;
- input/normalized island count, CDC cluster/coalescing counts, per-island old
  and derived-new coordinates/lengths, cumulative deltas, replacement source
  bytes/digests, and final-length equation;
- CDC restart boundary/predecessor, old prefix/replacement/suffix-probe bytes,
  unique/summed scan, old/new cursor rejoin, and rejoin/EOF/coalesced/fail/
  fallback class;
- old/new/local/replacement occurrence counts;
- new/reused/authenticated/deleted-logical/deleted-fetched payload objects/bytes;
- unique leaf/internal/root reads/writes/reuses, shared ancestors, and encoded
  bytes by level;
- height and occupancy;
- natural/forced cuts, split/merge, occurrence/descriptor replay to rejoin by
  level;
- unchanged subtree reuse and covered logical bytes;
- suffix payload fetch/write and suffix mapping rewrite;
- range mapping nodes, fragments, CAS fetches/batches, authenticated/returned
  bytes;
- workload/instrumentation SQLite acquisitions, queries, executes, rows,
  BLOBs;
- transactions, COMMIT dispatch, return, error, reconciliation;
- segmentation-witness decode/authenticate/read/write and the fresh
  reconciliation tuple;
- exact/latest request population and root;
- requested/selected/outcome route, capability hash, eligibility/fallback
  reason, parent/target/splice geometry, and native logical read/write/clone/
  reflink/shift/full-fallback bytes;
- TailAppend fetch/write, TailTruncate calls, APFS shift read/write and
  `2*S+N`, Linux ioctl arguments/results/errno/shared/boundary bytes;
- sync/rename/directory-sync;
- Q components/current/high-water/terminal, RSS, buffers, descriptors, queue;
- logical/apparent/allocated DB/journal/authority/native endpoints;
- current-live/retained-union/unreachable objects and bytes;
- metadata/node fill/fragmentation and extents per 1-MiB range.

No logical, pager, allocation, Q, RSS, or wall counter substitutes for physical
I/O.

Every field is exactly one of `Observed(source/API)`, `Derived(equation)`,
`NotApplicable(reason)`, or `Unavailable(source/reason)`. Zero native output
bytes on the virtual route does not erase CAS/SQLite physical I/O: native
output-file I/O is `NotApplicable`, while store I/O is separately Observed or
Unavailable.

Hard per-edit equations include:

```text
new_length
  = old_length - sum(old_length_i) + sum(replacement_length_i)

DeltaE = new_occurrence_count - old_occurrence_count

mapping_bytes_written
  = sum(new_leaf_bytes)
  + sum(new_internal_bytes)
  + new_root_bytes

new_mapping_nodes
  = one root
  + unique changed/split leaves
  + unique changed/split internal nodes

transactions = 1
publication_COMMITs = 1
terminal_Q = 0
```

For native parent `B`, old span `[a,b)`, new span `N`, target `T`, and surviving
suffix `S=B-b`:

```text
signed_delta = N - (b-a)
T = B + signed_delta
shifted_suffix_bytes = S
CloneShiftPatch wrapper transfer = 2*S + N
FullFallback destination logical writes = T
```

Changed payload/CDC work may grow from one byte to one MiB. Only base-file
suffix independence for the same mutation shape is a locality claim.

## 7. Timer equations

Every applicable endpoint closes:

```text
canonical_ack
  = construction
  + precommit
  + commit_dispatch_to_return
  + postreturn_wrapper
  + reconciliation_if_needed

edit_to_virtual_visible
  = canonical_ack
  + projection_dispatch
  + queue_wait
  + virtual_root_install

first_range_return
  = edit_to_virtual_visible
  + resolver
  + CAS_fetch_and_authentication
  + caller_delivery

direct_range_return
  = resolver
  + CAS_fetch_and_authentication
  + caller_delivery

direct_traversal_return
  = resolver_traversal
  + CAS_fetch_and_authentication
  + caller_delivery

native_durable_ack
  = seed_validation
  + clone_or_create
  + patch_shift_reflink_or_stream
  + data_sync
  + metadata_sync
  + rename
  + directory_sync
  + fresh_reconciliation_if_needed
  + postpublication_descriptor_and_root_verification

native_projection_complete
  = native_durable_ack
  + successor_seed_installation
  + temp_and_descriptor_cleanup
  + residue_verification

complete_campaign_wall
  = lock_and_preflight
  + preparation_and_startup
  + every scheduled product row
  + analysis
  + cleanup
  + lock release
  + terminal evidence fsync
```

Standalone read-only range/traversal rows use the direct applicable equation;
their canonical-ACK, edit-to-virtual-visible, publication, transaction, and
COMMIT timers are `NotApplicable(ReadOnlyOperation)`.

Operation timers are exclusive endpoint differences (`canonical_ack=t4-t0`,
`virtual_visible=t7-t0`, `first_range=t8-t0`,
`native_durable_ack=t10-t9`, `native_projection_complete=t11-t9`,
`cold_full_complete=t12-t9`, `operation_complete=t13-t0`). Campaign wall is
`T1-T0`. Nested durations are never added twice.

CPU is whole-child unless a direct phase source exists. Cache class is exact or
`warm_or_unknown`; cold is never inferred.

## 8. Acceptance rules

### Hard gates

- every scheduled row present, ordered, parseable, and retained;
- source/binary/input/base/output/manifest hashes exact;
- canonical identities and output exact;
- every raw target equals its frozen full-FastCDC byte/occurrence oracle; A and
  C roots each match their profile-specific oracle;
- island normalization, old/derived-new coordinates, `DeltaB`, oracle
  `DeltaE`, final length, replacement source, and fresh-build/edit-build
  commitments exact;
- one ordered canonical occurrence sequence/one mapping root histories exact;
- frozen FastCDC witness rejects alternate segmentation of identical raw
  bytes, preserving raw-byte one-content/one-root publication;
- every edit base carries the exact-root CanonicalSegmentationWitness; a
  read-only legacy-preserved conversion is excluded from the product
  performance population and rejects splice before target writes;
- fault/error precedence exact;
- one writer transaction/one publication COMMIT for every scheduled
  state-changing atomic mutation; normalized-empty and semantic same-root
  `NoChange` cases close at exact `0/0` and rolled-back `1/0` respectively;
- atomic net-zero and mixed rows expose no partial-island visibility;
- fresh reconciliation exact;
- direct work equations exact;
- Q/overflow/terminal cleanup exact;
- resource/storage/residue/custody exact;
- both independent analyzers agree;
- complete wall within the stage limit.
- native capability receipt, requested/selected/outcome route, exact output,
  durability, fallback discipline, and NotApplicable population exact.

### Primary G6 gates

```text
structural occurrence mapping reduction >=10x on frozen A/C rows
raw local DeltaE!=0 mapping reduction   >=10x only when frozen A has suffix work
raw DeltaE=0 / tail route                protected/local equation, no blind 10x
ordinary multi-island work               O(kH + unique CDC/tree replay)
canonical ACK p50, mutation/publication rows <=20 ms
canonical ACK p95, mutation/publication rows <=30 ms
virtual-visible p50/p95               <=20/30 ms
100-MiB raw local competitiveness     <= same-semantic A arm +5 ms
unchanged suffix payload fetch/write  exactly 0/0
unchanged suffix mapping after rejoin exactly 0
live ordinary mapping                 <1% logical bytes
additional resolver Q                 <=4 MiB
individual buffer                     <=1 MiB
terminal Q                            0
```

Candidate-only D4 route cells cannot establish a percentage speedup or a
protected no-regression result. `FullFallback` is valid correctness evidence
but never a native variable-size fast result. APFS middle insertion remains
suffix-dependent, and cold/full native export remains `Theta(file bytes)`.

Compute p50/p95 separately for every
`(size, operation, position, route_class)` population. Never pool qualifying
local and explicit fallback rows. With two or three observations, retain and
report every value. With one observation, p50=p95=that value and the result is
labeled a single-observation semantic smoke, not population inference. For two
sorted observations, define p50 as the checked floor midpoint
`lower + (upper-lower)/2` and p95 as `upper`; for three, p50 is the middle value
and p95 is the maximum. Every primary 100-MiB operation passes independently.

### Protected regression

Freeze exact final controls after G5:

```text
control >=5 ms: candidate <= control * 1.05
control < 5 ms: candidate <= control +1.000 ms
```

For each protected operation, `control` is the arithmetic mean of all its
control arms and `candidate` is the arithmetic mean of all its candidate arms,
computed with checked integer sums before conversion. Report every arm and
paired delta. A one-pair operation reduces to its direct two-arm comparison;
the compound first-after-reopen operation uses its balanced two-pair means.

No threshold is changed after product rows exist.

## 9. Fault population

All cases in specification §17 are mandatory. Cheap focused tests, not
100-MiB performance rows, cover:

- empty/one-byte, exact 31/32/33/63/64/65 leaf boundaries, and exact
  `32*32` / `64*64` internal boundaries with `-1/0/+1` cases;
- alternate cut, invalid tail/root, gap/overlap, legal zero-length occurrence
  preservation/range skip, and oversized extent rejection;
- identical raw bytes with alternate valid chunk segmentation rejected by the
  publication boundary or normalized only through the frozen FastCDC witness;
- 0/1/63/64/65 islands; unsorted/overlap/adjacent/same-offset insertions;
- one zero-effect island; 64 real islands plus discarded zero-effect islands;
  65 real normalized islands; inputs normalizing to an empty no-op or one
  island, with the empty result proving zero target writes/transactions/COMMITs;
- a nonempty semantic same-root plan proving one rolled-back writer
  transaction, zero publication COMMITs/transition/head/witness writes,
  zero retained objects, and exact terminal cleanup, including discovery
  failure;
- old/new coordinate overflow, final-length underflow, replacement length/
  digest/source mismatch, stream short read/error/cancellation;
- the separately frozen correctness-only sign fixtures with oracle
  `DeltaE<0`, `=0`, and `>0`, excluded from performance inference;
- standalone `64 KiB -> 4 KiB` shorter and `4 KiB -> 64 KiB` longer
  replacements;
- independent versus CDC-overlapping islands, deterministic cluster
  coalescing, bounded no-rejoin failure, and one earliest-unresolved fallback;
- range probes immediately before/inside/after every net-zero/mixed seam;
- missing/wrong-root segmentation witness and edit of a read-only
  legacy-preserved conversion rejected before target writes;
- wrong subtree length/count/level, role, identity, missing object, cycle;
- stale expected head;
- before-COMMIT and every reconciliation outcome;
- old reader across COMMIT and new reader after COMMIT;
- projection failure after canonical ACK;
- clone/reflink/alignment/unsupported fallback;
- capability receipt mismatch, frozen Linux precedence, TailAppend/
  TailTruncate, APFS shift directions, accelerator-temp discard, and no second
  publication after ambiguous visibility;
- before-rename, rename-lost-ACK, directory-sync-lost-ACK;
- restart/cancellation/cleanup and verified-after-trusted scrub;
- overflow and terminal Q/descriptors/queue/residue.

Use tiny deterministic 1-MiB or smaller fixtures. Do not consume campaign time
with repeated 100-MiB faults.

## 10. Evidence package

Retain:

- branch/HEAD/status/environment/toolchain/SQLite/filesystem;
- source/diff/binary/fixture/base/input/output hashes;
- schedule assertion and zero-row proof;
- raw mutation/island manifest, replacement-source hashes, independent full-
  FastCDC occurrence/root oracles, and `DeltaB`/`DeltaE` classifications;
- `NativeCapabilitySetV1`, conditional route schedule, probe artifacts,
  NotApplicable receipts, and route outcome/fallback receipts;
- commands, stdout, stderr, exit status, `/usr/bin/time` sidecars;
- raw JSONL, counters, timer equations, storage snapshots;
- primary and independently written analysis;
- protected/fault/static results;
- cleanup and lock-release receipts;
- payload and final manifests;
- read-only terminal rehash;
- every superseded or failed attempt.

Generated caches are non-authoritative and either excluded prospectively or
classified explicitly.

## 11. Retain / revise / revert

- **Shadow PASS**: permits a later G6-spec amendment after G5 seals; does not
  permit product implementation by itself.
- **Shadow REVISE**: preserve results, change one grounded canonicalization
  mechanism, and run a new version. No unchanged rerun.
- **Mechanism screen PASS**: permits exactly one measured gate.
- **Mechanism screen REVISE/FAIL**: preserve candidate/result and repair or
  revert before any gate.
- **Measured PASS**: possible only after every hard, primary, protected,
  resource, custody, and analyzer gate passes. It would make terminal G6 audit
  eligible, not automatically close Phase 4.
- **Measured REVISE**: preserve the exact failed population and repair only the
  failing group under a new prospective method/source when required.
- **Revert**: use when the candidate cannot meet canonical identity, locality,
  resource, or protected-operation requirements without changing the thesis.

Current execution stops before Stage A is run. Under the updated boundary,
Stage A itself waits for terminal G5 PASS. G5 supplies only the sealed trusted/
position-preserving/fallback baseline; it is not evidence for raw mutation,
multi-splice, virtual count-change, or native variable-size routes. No G6
implementation or measured campaign is authorized by this document.
