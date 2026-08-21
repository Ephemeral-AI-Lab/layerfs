# Phase 4 CDC and locality optimization directions

Status: research only; no implementation or profile authorization.  Local
performance is primary.  Distributed mechanisms are excluded.

## Current algorithm and measured cost

- **Observed:** production `FastCdc::scan` reads through one 32,768-byte stack
  input window and owns one reusable `Vec<u8>` chunk buffer
  (`crates/layerfs-core/src/cdc/mod.rs:31-56,60-75`).
- **Observed:** the frozen profile is 8/16/32 KiB min/target/max, normalization
  shift 2, seed zero, fixed gear table and four masks
  (`crates/layerfs-core/src/cdc/mod.rs:11-20` and
  `crates/layerfs-core/src/cdc/gear.rs:1-260`).
- **Observed:** it skips rolling work before the minimum, then evaluates two
  bytes at a time, preserving a one-byte pending value across fragmented reads
  (`crates/layerfs-core/src/cdc/mod.rs:77-149`).  Fragmentation-independent
  boundaries and size edges are tested at `:183-273`.
- **Observed:** accepted full create synchronously sends every emitted chunk to
  the FileBuilder inside the same source traversal (accepted F2 source
  `target/wp4m-f4a-residual-attribution-k64-20260820-v1/custody/sources/phase4_create_edit_benchmark-f2-accepted.rs:5437-5458`).
- **Observed:** the retained 100-MiB fixture emits 5,284 chunks, average
  19,844.36 bytes (`implementation-detail/phase-4/wp4m/f-series/f4/report.md:21-32`).

**Observed bottleneck:** F4-A measures CDC-exclusive scan at 128.723 ms,
24.56% of mapping (`implementation-detail/phase-4/wp4m/f-series/f4/report.md:272-294`).
F4-A2 then tested a borrowed-window/carry scanner and found only 3.702 ms median
directly removable materialization wall, with 0/5 rows reaching 33 ms
(`implementation-detail/phase-4/wp4m/f-series/f4/a2-cdc-materialization.md:535-550,
626-630`).  The copy-removal direction is closed.

F4-A2 also observed that 3,200/5,284 chunks cross a read window and require a
bounded carry, representing 67,072,778 bytes, yet their direct copy wall is only
about 1.907 ms (`.../a2-cdc-materialization.md:552-577`).  More buffer machinery
would optimize the wrong cost.

## Locality contract

- **Observed:** CDC must examine every source byte; full create is `Theta(S)`
  (`implementation-detail/phase-4/algorithm/spec.md:364-370`).
- **Observed:** content boundaries enable unchanged raw chunks to rejoin after
  insertion/deletion, but the retained fixed-ordinal K64 mapping still rewrites
  a shifted mapping suffix for count-changing edits.  A faster CDC algorithm
  cannot by itself fix mapping suffix locality.
- **Observed:** same-count local edit, append/truncate, raw IDs, canonical IDs,
  deterministic fragmentation behavior, bounded 32-KiB raw chunks, and exact
  typed errors are protected contracts
  (`implementation-detail/phase-4/algorithm/spec.md:109-125,440-545`).

## Amdahl ceiling

**Derived from the five sealed F4-A rows:** deleting all CDC-exclusive time—a
physically impossible ideal—would leave durable rows at 489.894-510.379 ms.
Four of five remain above 500 ms.  Making CDC 2.5x faster (removing 60% of its
wall) leaves about 542-562 ms.

Therefore CDC optimization is strategically useful but cannot reliably reach
200 MiB/s alone.  It must combine with identity/hash or storage improvements.

## Primary-source findings

- The original [FastCDC paper](https://www.usenix.org/conference/atc16/technical-sessions/presentation/xia)
  identifies Gear hashing, sub-minimum cut-point skipping, simplified judgment,
  and normalized chunking as its speed/locality tradeoff.  It explicitly warns
  that enlarging the skipped minimum can reduce deduplication, with about 15%
  worst-case decline in its study.
- The maintained [fastcdc-rs implementation](https://github.com/nlfiedler/fastcdc-rs)
  documents the 2020 two-byte algorithm and reports different variants can
  produce different cut points.  LayerFS already implements the two-byte form;
  merely naming “FastCDC 2020” is not a new direction.
- Google’s [FastCDC implementation](https://github.com/google/cdc-file-transfer/blob/main/fastcdc/fastcdc.h)
  shows a phase-structured, contiguous-buffer kernel and runtime size profile,
  but uses modified judgment/regression rules.  It is an implementation design
  reference, not boundary compatibility evidence.
- [VectorCDC, FAST 2025](https://www.usenix.org/conference/fast25/presentation/udayashankar)
  obtains very high throughput by vectorizing **hashless** CDC algorithms.  Its
  published implementation targets SSE/AVX, not Apple NEON, and changes the
  boundary algorithm.
- The official [Xet chunking specification](https://huggingface.co/docs/xet/chunking)
  uses GearHash with approximately 64-KiB target chunks and a minimum-size
  skip-ahead optimization.  That is evidence that larger local chunk profiles
  are practical elsewhere, not that they fit LayerFS workloads.
- Recent primary research on
  [CDC fingerprinting](https://eprint.iacr.org/2025/558.pdf) shows that bespoke
  keyed-CDC constructions can fail.  LayerFS seed zero is public; introducing
  a “secret” table is neither a speed optimization nor an adequate privacy
  design.

## Ranked directions

### 1. Exact-boundary hot-loop rewrite

- **Classification:** constant-factor and format preserving.
- **Hypothesis:** retain exact gear table, masks, pending-byte behavior, and
  earliest cut point, but scan contiguous slices with local scalar state and
  separate fixed-mask pre-target/post-target loops.  Emit offsets and append a
  complete chunk once rather than mutating the chunk on every pair.
- **Expected impact:** plausibly 10-25% of the 128.7-ms CDC lane (13-32 ms);
  only the high end meets the 33-ms decision threshold.  F4-A2 did not test
  this—it tested materialization while retaining the same pair state machine.
- **Risk:** a one-byte boundary drift changes the affected chunks and resulting
  mapping/root, and may cascade until the scanner resynchronizes.  The current
  two-byte implementation may already compile to nearly the same loop.
- **Decisive future question:** can an instrumentation-free candidate reproduce
  all 5,284 sealed boundaries under contiguous and adversarial fragmented reads
  while saving at least 33 ms in four of five full rows?
- **Kill direction if:** exact boundaries differ, or the full durable gain is
  below 5%.

### 2. Larger CDC profile, measured against edit locality

- **Classification:** constant-factor object-count reduction plus a format
  change; full work remains `Theta(S)`.
- **Hypothesis:** a 32- or 64-KiB target reduces chunk/object/reference/SQL
  counts and mapping bytes while retaining CDC rejoin behavior.
- **Expected impact:** object count may fall roughly with target size, but the
  dominant per-byte CDC and three BLAKE3 lanes remain.  A large full-create win
  is possible only if per-object SQLite/mapping cost is larger than current
  attribution suggests.
- **Risk:** changed boundaries and all roots; larger changed-byte amplification,
  coarser deduplication, larger range-read authentication, and worse small-edit
  storage growth.  The retained fixture alone cannot select this tradeoff.
- **Decisive future question:** across representative same-count, `+1`, append,
  truncate, repeated-content, compressed, sparse-like, and adversarial inputs,
  does 32/64 KiB improve full durable wall materially without losing more local
  reuse/storage than it saves?
- **Kill direction if:** full durable gain is below 10%, same-count latency or
  storage exceeds the protected gate, or post-edit unique bytes materially
  increase.

### 3. A new hashless/vector CDC profile

- **Classification:** disruptive algorithm/profile change.
- **Hypothesis:** a NEON-suitable hashless extrema algorithm can reduce most of
  the 128.7-ms scan while preserving useful probabilistic edit locality.
- **Expected impact:** CDC-only upper bound is 128.7 ms, but the exact F4 rows
  show even eliminating all CDC is insufficient in four of five rows.  Combine
  only with an independently justified hash/identity direction.
- **Risk:** no direct Apple-NEON evidence from VectorCDC; new boundary
  distribution, adversarial behavior, roots, dedup ratio, and portability.
- **Decisive future question:** can one portable scalar + NEON candidate beat
  FastCDC by at least 2x on real LayerFS corpora while matching or improving
  reuse/storage after local edits?
- **Kill direction if:** a scalar fallback is poor, locality/storage regresses,
  or end-to-end durable wall remains above the strategic target.

### 4. Fixed-size chunks

- **Classification:** disruptive simplification.
- **Expected impact:** removes almost all content-boundary work.
- **Anti-recommendation:** insertion/deletion shifts every subsequent chunk,
  destroying the core CDC reuse property documented by the FastCDC paper.  It
  is only defensible if real product workloads prove count-changing edits and
  cross-version deduplication irrelevant.

### 5. Larger read buffers or source `mmap`

- **Classification:** constant-factor I/O setup.
- **Observed ceiling:** source-read median is only 16.468 ms
  (`implementation-detail/phase-4/wp4m/f-series/f4/report.md:278-285`).
- **Recommendation:** do not prioritize; even perfect removal cannot meet the
  33-ms milestone, and mmap would complicate RSS/page-fault interpretation.

## Recommendations and anti-recommendations

1. Run one minimal exact-boundary kernel bake-off before changing the profile.
   It is the only low-format-risk CDC direction not already killed.
2. Treat larger chunks as a workload/locality study, not a 100-MiB benchmark
   shortcut.
3. Reserve VectorCDC/hashless CDC for a separately versioned profile after an
   Apple-NEON feasibility result.
4. Coordinate any new CDC profile with the identity report: collapsing hashes
   has more measured ceiling than CDC copying or source I/O.

Do not reimplement the rejected borrowed-window scanner, infer physical I/O
from logical reads, use a keyed gear table as a security claim, add a worker or
async pipeline under the synchronous caller-thread contract, or claim that a
CDC change fixes the mapping layer's count-changing suffix rewrite.
