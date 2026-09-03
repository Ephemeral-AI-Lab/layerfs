# Namespace initialization, scale, and CAS/CDC deduplication

## Status

Draft v0.1.3 family contract: 7 timed scenarios and 2 proof-only scenarios.
Four timed rows are inherited from v0.1.1; three timed and two proof-only rows
are new CAS/CDC deduplication admission work. Nothing in this document is
registered until v0.1.1 reaches a terminal profile decision and each new
source, fixture, schema, oracle, runner, and evidence identity is frozen.

## Problem statement

Large-namespace performance and deduplication answer different questions.
The unique-content v0.1.1 rows expose file/object overhead without letting CAS
collapse the payload. They do not prove that independently materialized files
with exact shared regions produce the expected chunk reuse, that CDC
resynchronizes after an insertion or deletion, or that a Workspace Commit
reuses chunks already durable in its base Layer.

Dedup-friendly logical bytes can also exaggerate physical write throughput.
For example, ten 100 MB files may contain only about 100 MB of unique payload.
The family therefore needs exact chunk and Store evidence while keeping its
dedup rows separate from the inherited unique-content performance gate.

The current v0.1.3 draft also described the historical uniform 2,500-byte
namespace-v1 fixture while v0.1.1 is evaluating a separately identified
namespace-v2 profile under the same four scenario IDs. Scenario ID alone is no
longer sufficient custody, and v0.1.3 must not pre-empt the v0.1.1 admission
decision or pool the two profiles.

## Goal

Rerun the four namespace rows with the complete profile ultimately admitted by
v0.1.1, then add one nested 1/10/100-file locality curve and two proof-only
cases that establish:

- exact within-initialization CAS coalescing;
- localized same-length sharing;
- CDC resynchronization after insertions and deletions;
- common-body reuse despite unique prefixes and suffixes;
- the negative case where high byte similarity has little exact-chunk reuse;
- frozen CDC boundary behavior; and
- reuse of chunks already present in a Workspace's base Layer.

Use the existing `fs-bench-pro` binary, namespace command, workload helper,
`run-namespace.sh`, real-FUSE lifecycle, custody, and cleanup machinery. Do not
create another benchmark crate, family, or runner.

## Files to read

- [Append-only benchmark contract](../benchmarking.md)
- [v0.1.3 parent plan](README.md)
- [v0.1.1 lifecycle checklist](../0.1.1/README.md)
- [v0.1.1 namespace optimization specification](../0.1.1/namespace-optimization-spec.md)
- [`fs-bench-pro` namespace command](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Namespace runner](../../../../benchmark/fs-bench-pro/run-namespace.sh)
- [Benchmark workload helper](../../../../benchmark/fs-bench-pro/workload.rs)
- [FastCDC profile](../../../../crates/layerfs-content/src/file/cdc/gear.rs)
- [File rope builder](../../../../crates/layerfs-content/src/file/rope/build.rs)
- [Existing-directory importer](../../../../crates/layerfs-layerstack-store/src/layerstack.rs)
- [Canonical object admission](../../../../crates/layerfs-layerstack-store/src/objects.rs)
- [Workspace Commit planner](../../../../crates/layerfs-workspace/src/changes.rs)
- [Current dedup analysis](../../../../crates/layerfs-monitor/src/dedup.rs)

## Identity and v0.1.1 dependency

Bind every namespace result to the complete tuple:

```text
scenario ID
result schema
fixture profile
fixture digest profile
CDC profile ID
source seal
product and harness revisions
container and cache identities
```

Historical namespace-v1, provisional namespace-v2, and v0.1.3 deduplication
evidence remain separate even where a display name shares a prefix. v0.1.3
reruns only the namespace profile that v0.1.1 ultimately admits. It does not
copy provisional v0.1.1 measurements into a release claim.

Use separate draft identities for the new rows:

```text
fixture_profile = namespace-dedup-locality-v1
fixture_digest_profile = namespace-dedup-digest-v1
result_schema = fs-bench-pro-namespace-dedup-v1
```

The existing `all` selector retains its frozen four-tier meaning. The new IDs
are individually selectable; any future group selector is additive and must
not change `all` or registered totals.

## Shared lifecycle and timing boundary

The four inherited rows retain the exact lifecycle and timing boundaries of
the v0.1.1 profile that owns them. Each new timed deduplication row uses:

```text
independently materialized existing directory
  -> LayerStack initialize
  -> genesis Layer and exact chunk analysis
  -> Branch fork
  -> real-FUSE Workspace create
  -> existing ten-byte positional edit
  -> one Commit
  -> End
  -> fresh Store reconnect
  -> exact canonical and real-FUSE verification
```

`complete_product_ns` retains its existing boundary. Fixture and oracle
generation, Store and Client construction, container preparation, cache
preconditioning, report generation, and post-run chunk analysis are excluded
and recorded as setup or proof.

Deduplication itself occurs during LayerStack initialization or Workspace
Commit, not during Workspace Create. Create must reference the genesis CAS root
without producing another payload copy.

Every proof-only case records wall, CPU, RSS, I/O, Store growth, and cleanup,
but has no latency distribution and never enters a performance total.

## Inherited unique-content timed scenarios

| Scenario ID | Regular files / data directories | Authoritative fixture |
| --- | ---: | --- |
| `namespace-100` | 100 / 1 | Exact profile and byte total admitted by v0.1.1 |
| `namespace-1000` | 1,000 / 10 | Exact profile and byte total admitted by v0.1.1 |
| `namespace-10000` | 10,000 / 100 | Exact profile and byte total admitted by v0.1.1 |
| `namespace-100000` | 100,000 / 1,000 | Exact profile and byte total admitted by v0.1.1 |

These four rows remain the binding initialization-performance and
file-count-scaling control. Dedup-friendly rows never replace them, satisfy
their throughput gates, or enter their adjacent-ratio calculation.

## New timed locality curve

Each file contains exactly 5,000,000 bytes and is far larger than the frozen
32,768-byte maximum CDC chunk. Lower tiers are exact path/content prefixes of
the 100-file fixture for the same seed.

| Scenario ID | Files | Logical bytes | Relationship | Required payload oracle |
| --- | ---: | ---: | --- | --- |
| `namespace-dedup-locality-1` | 1 | 5,000,000 | Common base only | Exactly 0% intra-import payload deduplication |
| `namespace-dedup-locality-10` | 10 | 50,000,000 | Base plus nine localized variants | At least 85%; exact seed-bound chunk transcript matches |
| `namespace-dedup-locality-100` | 100 | 500,000,000 | Base plus 99 localized variants | At least 95%; exact seed-bound chunk transcript matches |

Use one data directory and exact paths:

```text
d0000/f000000
...
d0000/f000099
```

The percentage floors are admission expectations, not substitutes for exact
chunk IDs, lengths, counts, and byte equations. A valid slower or lower-dedup
baseline is retained as product evidence rather than relabelled as a harness
failure.

## Seeds and deterministic content

Use the three parent seed labels, one fresh timed sample per seed:

```text
layerfs-v0.1.3-seed-1
layerfs-v0.1.3-seed-2
layerfs-v0.1.3-seed-3
```

Reuse the efficient SHA-256-seeded xoshiro256** stream already implemented by
the namespace workload. Define an unambiguous framed seed:

```text
H(seed, role, ordinal) =
  SHA256(
    "layerfs/fs-bench-pro/namespace-dedup/v1\0"
    || len(seed)_be_u64 || seed
    || len(role)_be_u64 || role
    || ordinal_be_u64
  )

X(seed, role, ordinal, length) =
  first `length` bytes of the existing xoshiro256** stream seeded by H
```

Do not perform one cryptographic hash per output block. The common base is:

```text
B = X(seed, "locality-base", 0, 5,000,000)
```

For file zero, `F_0 = B`. For file `i >= 1`, calculate an interior offset:

```text
o_i = 65,536
      + little_endian_u64(H(seed, "locality-offset", i)[0..8])
        mod (5,000,000 - 131,072 - 10)
```

Create a nonzero ten-byte XOR mask from
`H(seed, "locality-mask", i)`, setting the low bit of every selected byte, and
define:

```text
F_i = B[0..o_i]
      || (B[o_i..o_i+10] XOR mask_i)
      || B[o_i+10..]
```

Reject duplicate final file digests or an identical offset-and-replacement
pair. Paths remain in namespace and fixture identities but never enter the
common-payload seed.

## Independent materialization and metadata custody

Every output is an independently created regular file. Regenerate common
bytes from the deterministic stream for each file; do not copy another file.

Freeze:

```text
regular-file mode = 0640
data/root directory mode = 0750
mtime seconds = 1700000000
mtime nanoseconds = 0
```

The generator uses one bounded reusable buffer, writes every byte through
ordinary sequential file writes, closes every file, sets exact metadata, and
sets directory timestamps only after their children exist. It performs no
per-file `fsync`. Atomic fixture publication remains required.

Fixture custody proves:

- unique `(device, inode)` for every regular file;
- `st_nlink == 1` for every regular file;
- no hard links, sparse holes, reflinks/clones, owner-side Workspace file-range edit, compression,
  shared backing file, or precomputed product ObjectId;
- exact path, type, size, mode, mtime, SHA-256, logical bytes, bytes written,
  and physical allocation; and
- fixture generation and independent CDC-oracle CPU, RSS, wall, logical I/O,
  and physical I/O separately from product timing.

## Proof-only scenarios

| Scenario ID | Purpose | Execution |
| --- | --- | --- |
| `namespace-dedup-mechanisms-proof` | Exact CAS, CDC locality/resynchronization, negative behavior, and boundary semantics | One lifecycle containing separately attributed, domain-separated cohorts for all three seeds |
| `namespace-dedup-preexisting-proof` | Reuse of chunks already durable in a base Layer during Workspace Commit | One lifecycle containing independently attributed base/variant cohorts for all three seeds |

### Combined mechanisms proof

Use separate top-level directories and domain labels. Require no unintended
chunk-ID intersection between cohorts.

1. **Unique control:** ten independently generated 1,000,000-byte files. Freeze
   the exact chunk set and require no unplanned duplicate payload IDs.
2. **Exact copies:** ten independently written 1,000,000-byte files regenerated
   from one stream. Their ordered chunk transcripts and file content roots are
   identical, unique payload is exactly 1,000,000 bytes, logical payload is
   exactly 10,000,000 bytes, and intrinsic payload deduplication is exactly
   90%.
3. **Insertion/deletion:** one 1,000,000-byte base plus nine variants that
   alternately insert or delete 4,096 deterministic bytes at interior offsets.
   Freeze exact transcripts; require a nonempty common suffix, at least 80%
   shared payload coverage, and measured resynchronization no later than four
   maximum chunks (131,072 bytes). A miss is retained CDC limitation evidence.
4. **Common body:** ten 1,000,000-byte files containing a unique 125,000-byte
   prefix, common 750,000-byte body, and unique 125,000-byte suffix. Require an
   exact frozen oracle and 60%–67.5% intrinsic payload deduplication.
5. **Scattered edits:** one 1,000,000-byte base plus nine variants changing one
   deterministic byte at every 4,096-byte position. Require the exact frozen
   chunk set and at most 1% payload deduplication, proving CAS is exact rather
   than fuzzy.
6. **CDC boundaries:** for lengths 8,191, 8,192, 32,768, and 32,769 bytes,
   create an independently written exact pair and a one-byte middle mutation.
   Exact pairs have identical chunk transcripts and file roots; 8,191- and
   8,192-byte mutations share no payload chunk with their base; 32,769-byte
   inputs emit at least two nonempty chunks; all chunk lengths sum to the exact
   file size and never exceed 32,768 bytes.

Input fragmentation must not change CDC boundaries. The proof analyzes every
cohort independently even though one lifecycle amortizes setup and FUSE
cleanup.

### Preexisting-base Workspace Commit proof

Prepare the genesis Layer with one independently materialized 1,000,000-byte
common base per seed. Record the exact pre-Workspace chunk set and Store state.
Then use one fresh process through real FUSE to independently regenerate and
write ten new files per seed:

```text
new/f000 = exact base bytes
new/f001..f009 = distinct ten-byte localized variants
```

Do not read and clone the base file, use a hard link/reflink, borrow a range,
or pass precomputed roots to the product. Require all new logical bytes to be
written by the workload and captured by the ordinary Workspace path.

After one Commit, require:

- the exact-copy file has the preexisting content root;
- exact-copy incremental payload insertion is zero;
- incremental payload deduplication across the cohort is at least 90%;
- every reused or borrowed chunk ID existed before Workspace Create;
- within-Commit borrowed references remain separate from emitted preexisting
  reuse; and
- exact Commit root, End, reconnect, FUSE bytes/metadata, and cleanup proof.

## Exact chunk oracle and metric model

The existing `CandidateReceipt` and monitor `saved_fraction` are insufficient
for fresh-Store deduplication: candidate construction may coalesce duplicate
chunk emissions before the receipt, so ten exact copies can correctly report
zero preexisting reuse while achieving 90% intrinsic payload deduplication.

For the ordered multiset `E` of file payload chunk occurrences, record:

```text
path
chunk ordinal
logical start and end
canonical chunk ObjectId
raw payload length
```

Let:

```text
C = number of occurrences in E
L = sum of occurrence payload lengths
U = distinct ObjectIds in E
U_B = sum of payload lengths once per ObjectId in U
P = distinct payload chunk IDs present before the operation
R = U intersect P
I = U minus P
I_B = sum of payload lengths once per ObjectId in I
```

Every same-ID occurrence must have identical canonical bytes or the row fails
as an integrity collision. For full initialization:

```text
L = fixture logical bytes = LayerFS scanned bytes
C >= |U| = |R| + |I|
submitted chunk canonical bytes = L + 21*C
unique chunk canonical bytes = U_B + 21*|U|
```

The 21-byte envelope is the frozen 9-byte object header, 4-byte Bytes length,
and 8-byte `LFS4CHK` role marker. Validate the equations from actual records.

Report authoritative integer numerators and denominators for:

```text
intrinsic payload dedup = 1 - U_B/L
incremental payload dedup = 1 - I_B/L
payload sharing factor = L/U_B
preexisting logical coverage =
  final extent-occurrence bytes whose chunk ID is in P / L
```

Keep these distinct:

- **within-import duplicate:** a later occurrence of a chunk already emitted by
  the same initialization;
- **preexisting reused chunk:** emitted now but already durable at operation
  start; and
- **borrowed chunk:** referenced by the result and present at operation start
  without being emitted by this operation.

For complete existing-directory initialization, borrowed payload is zero
because every input file is read and rechunked. Borrowed chunks are expected in
localized Workspace Commit.

Also report, without calling them payload deduplication:

```text
candidate = inserted + preexisting reused canonical objects/bytes
SQLite amplification = allocated Store growth / logical bytes
canonical-to-SQLite amplification =
  allocated Store growth / inserted canonical bytes
logical ingest throughput = logical bytes / initialization wall
unique payload insert rate = I_B / initialization wall
unique canonical insert rate = inserted canonical bytes / wall
```

A high logical ingest rate never implies the same physical write rate.

## Oracle construction and post-run verification

Fixture preparation uses the frozen public FastCDC and canonical chunk encoder
to create two sealed digests before timing:

```text
chunk_transcript_digest = hash, in sorted path order, of
  domain || framed path || file length || chunk ordinal ||
  logical start || logical end || ObjectId || payload length

unique_chunk_set_digest = hash, in ObjectId order, of
  ObjectId || payload length || authenticated canonical digest
```

After the timed operation, a fresh process reconnects to the Store, resolves
every expected path from the exact genesis or Commit root, traverses each
regular file's content rope, authenticates every referenced object, and
compares the complete ordered extent/chunk transcript with the fixture oracle.
For full initialization, every extent must cover its complete payload chunk:

```text
source_offset = 0
logical_length = decoded payload length
```

The distinct reachable payload set must equal the expected set exactly. A
Store-wide search for `LFS4CHK` objects is not authoritative because portable
metadata values also use file ropes and chunk objects.

Fresh real-FUSE verification additionally proves exact path count, no extra or
missing path, type, size, mode, mtime, content digest, Branch head, root, and
cleanup. Roots are compared within a sample across End and reconnect; roots
from different random LayerStack seeds are not expected to match.

For shifted variants, freeze the longest common `(ObjectId, payload length)`
suffix and report the first matching suffix positions, shared suffix chunks
and bytes, and maximum base/variant distance from the edit boundary. Emit an
explicit `not-found` status rather than a numeric zero when resynchronization
does not occur.

## SQLite and physical-storage evidence

Record at operation start and after fresh reconnect:

```text
store file length and allocated blocks
PRAGMA page_size
PRAGMA page_count
PRAGMA freelist_count
live page bytes = (page_count - freelist_count) * page_size
canonical object count and bytes
Store file census
```

Retain signed, checked deltas for database length, allocated blocks, and live
pages. Do not use saturating subtraction to hide an unexpected shrink.
`1 - Store growth/logical bytes` is not a dedup rate because namespace objects,
keys, page fill, and indexes can make it negative.

The final Store census contains exactly `store.sqlite`. No persistent chunk
index, pack, side database, or dedup sidecar is allowed.

## Required resource evidence and anti-cheating rules

Every timed row and proof records:

- source open/read calls and complete logical bytes read;
- CDC profile, bytes scanned, chunk occurrences, unique and inserted chunks,
  payload and canonical bytes;
- canonical candidate, inserted, preexisting reused, and borrowed objects and
  bytes by relevant kind;
- initialization/Commit phase walls, process user/system CPU, baseline/peak/
  incremental RSS, cgroup memory/swap/OOM, workers, and logical/physical I/O;
- temporary segment writes, reads, passes, and peak bytes;
- SQLite statements, transactions, returned/skipped rows, bound bytes, page
  state, file length, allocation, and Store growth; and
- exact root, chunk transcript, FUSE reopen, and resource cleanup.

Missing fields are evidence errors, never silent zeros. Deduplication,
performance, resource, correctness, and cleanup receive separate pass fields.

Reject any result that obtains a favorable percentage or wall time by:

- skipping a source read or CDC scan;
- using sparse files, hard links, reflinks/clones, compression,
  owner-side Workspace file-range edit, repeated backing storage, or precomputed product roots;
- moving product work into fixture preparation, cache warm-up, another phase,
  or a background task;
- adding product workers, hidden Stores, sidecars, or durable formats;
- increasing CPU, physical I/O, temporary storage, Store amplification, swap,
  or memory outside the owning metric; or
- weakening the exact canonical, chunk, metadata, FUSE, reconnect, collision,
  acknowledgement, or cleanup oracle.

Fixture/oracle preparation is streamed with bounded scratch and reported
separately. Product initialization still reads and CDC-scans every logical
fixture byte, including bytes that CAS later coalesces.

## Sampling, rates, and family budget

Each new timed row has exactly three fresh samples, one per frozen seed. Every
sample owns a fresh fixture, Store, process, LayerStack, Branch, Workspace, and
evidence directory. Cache state is explicit and never pooled across profiles.
Proof fixtures contain all three seed subtrees and execute once per proof ID.

Report logical ingest, unique payload insertion, unique canonical insertion,
and physical Store growth side by side. Dedup rows do not satisfy the inherited
unique-content throughput or adjacent-scaling gate and remain outside
`registered_total_ns`.

Use the parent planning model for budgeting, not as a pre-baseline latency
admission gate. The family includes the four inherited rows, nine timed dedup
samples, and one execution of each proof:

- Target family wall: **55 seconds**.
- Hard family wall: **90 seconds**.

A family that exceeds the hard wall retains its valid evidence and revises its
matrix or implementation; it does not hide slow rows.

## Execution order

1. Wait for the terminal v0.1.1 namespace-profile and identity decision.
2. Freeze the five new scenario IDs, fixture/result profiles, three seed
   manifests, exact chunk transcripts, percentage numerators/denominators, and
   proof expectations.
3. Extend the existing benchmark binary, workload helper, namespace runner,
   source-seal closure, report validator, and one smallest runnable self-check.
4. Capture the unoptimized public-path baseline before changing product code.
5. Run the unique and exact-copy controls, localized curve, shifted/common-body
   proofs, scattered negative control, boundary proof, and preexisting-base
   proof.
6. Classify any failure from exact chunk, canonical, Store, CPU, memory, I/O,
   or lifecycle evidence. Do not change the CDC profile merely to improve a
   percentage.
7. Optimize only a measured compatible root cause and rerun its focused proof.
8. Rerun the four inherited unique-content rows to reject performance or
   resource regression.

## Acceptance criteria

- [ ] Keep exactly one namespace family, the existing benchmark crate and
  binary, and `run-namespace.sh`; add no new family, crate, or runner.
- [ ] Resolve the inherited v0.1.1 profile before freezing v0.1.3 and bind every
  result to scenario, schema, fixture, digest, CDC, source, container, and cache
  identities.
- [ ] Implement exactly the three timed locality IDs and two proof-only IDs
  above without changing the existing `all` selector.
- [ ] Prove the 1/10/100 paths and content are exact nested prefixes and every
  byte is independently materialized and scanned.
- [ ] Match every seed-bound chunk transcript, unique chunk set, payload and
  canonical equation, dedup numerator/denominator, root, and real-FUSE reopen
  oracle.
- [ ] Achieve exact 90% payload deduplication for the exact-copy cohort and the
  declared localized, shifted, common-body, scattered, and boundary gates.
- [ ] Distinguish within-import duplicate, preexisting reused, and borrowed
  chunk references; do not infer fresh-import dedup from `reused_objects`.
- [ ] Prove the preexisting-base Workspace Commit reuses the durable payload
  without cloning the source or adding another complete payload copy.
- [ ] Report logical ingest beside unique payload/canonical insertion and
  SQLite length/allocation/live-page growth; never present logical throughput
  as physical write throughput.
- [ ] Preserve the CDC profile, canonical bytes/ObjectIds, five-table Store,
  SDK/CLI, daemon/FUSE protocol, worker ceiling, collision checks, and public
  lifecycle semantics.
- [ ] Retain complete CPU, RSS, worker, logical/physical I/O, temporary storage,
  Store growth, swap/OOM, exact reconnect, and cleanup evidence with no hidden
  resource trade.
- [ ] Keep all dedup-friendly rows separate from inherited unique-content
  performance totals.
- [ ] Run three fresh samples per timed row and one complete execution per proof
  case, retaining every valid success or failure.
- [ ] Meet the 55-second target and never exceed the 90-second hard family wall
  without an explicit retained revision decision.
