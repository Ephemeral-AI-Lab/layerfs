# CDC locality across independently written files

> **Status:** Draft family `dedup_cdc_locality`: 20 new timed cases and one
> proof-only case. No implementation or evidence is admitted by this document.

## Question and shared rules

How much exact payload survives localized changes, shifted content, a shared
middle, and scattered damage when each file is independently imported?
Measure real CDC work and stored chunks rather than infer reuse from textual
similarity. This full-import family does not measure SDK partial-edit borrowing.

The [shared testing rules](testing-rules.md) own common lifecycle, preparation,
sampling, caps, and timing. Reuse the fresh-import operation, independent file
materialization, complete payload accounting, and Store/resource receipts from
[cross-file exact deduplication](dedup-cross-file.md). Exact-copy and unique
controls belong to that family; do not register duplicate controls here.

## Exact membership

Every timed case imports one **1 MiB reference** plus `N = 1, 10, 100, 500`
independently written variants. N counts variants, so the N=1 case already
compares two files. Each row below expands to four IDs with suffixes
`-1`, `-10`, `-100`, and `-500`.

| Scenario prefix | Variant of reference B | File bytes |
| --- | --- | ---: |
| `dedup-cdc-overwrite` | Replace 64 interior bytes with a nonzero deterministic XOR mask | 1,048,576 |
| `dedup-cdc-insert` | Insert 4,096 deterministic bytes at an interior offset | 1,052,672 |
| `dedup-cdc-delete` | Remove 4,096 bytes at an interior offset | 1,044,480 |
| `dedup-cdc-common-body` | Unique 128 KiB prefix, B's middle 768 KiB, unique 128 KiB suffix | 1,048,576 |
| `dedup-cdc-scattered` | Change one byte in every 4 KiB block, using distinct nonzero masks | 1,048,576 |

There are **20 timed IDs and 60 timed samples**, plus exactly
`dedup-cdc-boundaries-proof`. The reference is fixed inside each profile and
seed; lower tiers use exact prefixes of its sealed 500-variant schedule.
Profiles use fresh Stores and separate domain labels. Reject duplicate variant
digests and unintended equality with B during fixture qualification.

## Fixture and execution

Reuse the existing bounded namespace stream and materialization machinery.
Seed framing must unambiguously bind shared seed, family, profile, reference,
variant ordinal, and byte/offset role. Interior offsets are at least 64 KiB
from each end and are chosen before collection; never select offsets using a
candidate's observed CDC behavior. Base and unchanged bytes are regenerated
for each output file; no copying, reflinking, hard-linking, sparse files, or
precomputed roots manufacture sharing.

The largest cohort is insertion at N=500:
`1,048,576 + 500 × 1,052,672 = 527,384,576 bytes` (502.953125 MiB).
Its largest file is 1 MiB + 4 KiB. All other cohorts are smaller. Stream
fixture generation and verification; do not materialize another complete
cohort or accumulate output copies inside the logical workload. Shared caps
apply at every intermediate state, not just after import.

Performance calls `Client::initialize_layerstack` from the independently
materialized directory into a fresh output Store, scanning every logical byte.
No edit follows merely to obtain a Commit. Time import acknowledgement and
declared product phases; collect exact canonical/FUSE reopen proof separately.
Reuse namespace runner, source sealing, cached pristine inputs, and result
machinery without copying a prepared Store over the measured import.

## Exact locality oracle

Seal every reference/variant byte digest and its independent ordered FastCDC
transcript before candidate work. Read each completed file from its root and
compare every actual ObjectId, logical boundary, and decoded payload length.
Matching bytes are necessary but cannot replace chunk evidence.

For each variant report:

- exact reference/variant shared chunk IDs and logical coverage;
- candidate, inserted, reused, and unique file-payload bytes;
- common-prefix and common-suffix chunk counts/bytes;
- for insertion/deletion, base and variant logical positions where the common
  suffix resumes, and distance from the respective edit boundary; and
- `not-found` when no shared suffix exists, never a misleading zero distance.

Report per-variant and whole-cohort equations separately: repeated chunks among
variants can improve whole-cohort reuse even when reference coverage is worse.
The common-body case must establish sharing inside the middle, not merely
reuse of prefixes accidentally identical between variants. Scattered edits are
the negative control for fuzzy similarity; use the exact qualified transcript
rather than claim "no sharing" solely from the mask schedule.

No historical 5 MB-file 85%/95% thresholds carry over. Freeze percentage and
resynchronization gates from fixture qualification/untouched baseline before
candidate optimization. Retain a reference limitation if a seeded input does
not resynchronize within the hoped-for window; do not search for a friendlier
seed after a candidate miss. The frozen CDC profile itself is unchanged.

## Boundary proof

`dedup-cdc-boundaries-proof` executes once and includes all three seed cohorts,
separately attributed. For lengths **0, 1, 8,191, 8,192, 16,384, 32,768,
32,769 bytes**, independently materialize an exact pair; for nonempty lengths
also materialize a one-byte mutation at `length / 2`.

Require exact bytes, matching transcripts for each pair, all chunk lengths
nonzero and at most 32,768, total lengths equal file lengths, no chunks for the
empty input, and at least two chunks for 32,769 bytes. One-byte mutations of
inputs no longer than 8,192 bytes cannot retain that input's sole payload
chunk. Other sharing follows the sealed oracle rather than a guessed bound.
Fresh canonical and FUSE reopen must preserve every cohort.

Reuse the existing CDC unit checks for fragmented reads and frozen profile
boundaries. Extend their evidence receipt into this public import/reopen proof;
do not create a new timed family or one latency row per input fragmentation.
The proof has correctness/resource results and its own wall, no latency
distribution or extra timed membership.

## Evidence and completion

Retain full regular-file payload transcripts, canonical namespace/metadata
bytes, logical scan versus unique insertion throughput, actual SQLite
length/allocation/live-page deltas, CPU/memory/I/O/spool peaks, transactions,
and cleanup under the shared rules. Neither monitor `saved_fraction` nor
`physical_bytes` replaces these receipts. No verification, digest construction,
failure injection, or FUSE readback runs inside the import timer.

One fresh performance sample per seed gives 60 rows. Use a small selected case
for a 1–5 second development aspiration after cached preparation; qualify all
four tiers with honest complete import work and frozen family budgets. No new
runner framework, Store schema, CDC profile, or object identity is authorized.

## Source references

- [Existing generator and namespace import](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Frozen FastCDC profile](../../../../crates/layerfs-content/src/file/cdc/gear.rs)
- [Existing boundary and fragmentation checks](../../../../crates/layerfs-content/src/file/cdc/mod.rs)
- [File rope builder](../../../../crates/layerfs-content/src/file/rope/build.rs)
- [Canonical object admission](../../../../crates/layerfs-layerstack-store/src/objects.rs)
