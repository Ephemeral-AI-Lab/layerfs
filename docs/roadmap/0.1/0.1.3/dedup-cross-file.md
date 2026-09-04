# Cross-file exact-content deduplication

> **Status:** Draft family `dedup_cross_file`: 10 new timed cases, no
> proof-only cases. No implementation or evidence is admitted by this document.

## Question and scope

When independently written files contain unique bytes, identical bytes, or a
mixture, does one fresh import store exactly the required distinct payload?
Measure the CPU, reads, canonical objects, and durable Store bytes required to
obtain that reuse. This is an initialization family, not an SDK edit family.

The [shared testing rules](testing-rules.md) own custody, size caps, timing,
sampling, preparation reuse, and admission. The [parent inventory](README.md)
owns release totals. [CDC locality](dedup-cdc-locality.md),
[preexisting Workspace reuse](dedup-workspace-reuse.md), and
[retained Branch history](dedup-branch-history.md) own different dedup questions.

The four released `init_namespace` cases and three `store_footprint` controls
remain inherited evidence with their exact released identities and sizes.
They are not members of this new family or replacements for its controls.
Retain `synthetic-small-heavy-v2`, `namespace-file-digest-tree-v2`, and
`fs-bench-pro-namespace-v3` when rerunning those namespace cases; historical
namespace-v1 and these new dedup rows must not be pooled with them.

## Exact membership

One unit is one independently materialized **1,048,576-byte file**. Use the
explicit tiers `1, 10, 100, 500`; smaller fixtures are prefixes of the same
500-file fixture for a seed. Each profile runs in its own fresh output Store.

| Scenario IDs | N | Relationship | Required result |
| --- | --- | --- | --- |
| `dedup-cross-file-anchor-1` | 1 | One common first file, shared by all three curves | One payload, no unintended internal duplicate chunks |
| `dedup-cross-file-unique-10`, `-100`, `-500` | 10 / 100 / 500 | Every file has unrelated deterministic bytes | No unintended inter-file payload-ID intersection |
| `dedup-cross-file-identical-10`, `-100`, `-500` | 10 / 100 / 500 | Every file has the anchor's exact bytes | Identical ordered chunk transcripts and file content roots |
| `dedup-cross-file-mixed-10`, `-100`, `-500` | 10 / 100 / 500 | Repeated four-file block: pair, pair, unique, unique | Exactly the frozen mixture of duplicate and distinct payload |

The mixed block uses one new base for its first two files and distinct bases
for its last two; no base is shared between blocks. The first base is the
anchor. Thus distinct complete file payloads number 7 / 75 / 375 at the three
mixed tiers. Qualify no unintended chunk intersections within or between
distinct bases before admission. The identical profile has exactly one MiB of
distinct file payload, and its intrinsic payload savings are `(N - 1) / N`.
The mixed profile's exact fractions follow its sealed chunk set, not a rounded
label such as "50% duplicates."

There are **10 distinct timed IDs and 30 timed samples**. The N=1 anchor is
collected once per seed and referenced by all three curves; it is not three
separate samples or three family members. The shorthand suffixes in the table
expand only with their row's full prefix.

## Fixture, operation, and bounds

Reuse the existing namespace generator's bounded stream and fixture writer,
`sdk_edit_common.rs` size labels/fixture helpers where applicable, namespace
runner, custody, result adapters, and prepared runtime. Freeze one
domain-separated seed for each distinct base using the three shared seed
labels. Do not replace the existing stream with per-block cryptographic hashes.
The maximum fixture is **500 MiB**; files are always exactly 1 MiB.

Create every input file independently with ordinary writes, regenerating its
deterministic bytes. Do not hard-link, reflink, clone, create sparse holes,
compress, reuse a backing file, or supply precomputed product roots. Record
distinct source inodes, link count one, logical bytes, bytes written, allocation,
exact modes/mtimes, path manifests, and hashes. Payload buffers are bounded;
fixtures and oracles are prepared outside product timing.

The measured entrypoint is `Client::initialize_layerstack` with
`LayerStackInitialization::Directory`. It must read and CDC-scan every source byte
into a **fresh output Store**; cached input files cannot replace measured
import with a copied prepared output Store. Record expected and observed source
reads and scanned bytes. No artificial post-import mutation is needed.

Performance mode times initialization acknowledgement and named product
phases only. Separate verification opens the exact genesis root from a fresh
Client, creates one Branch and real-FUSE Workspace for readback, verifies every
path, and ends it cleanly. A verification `UpToDate` acknowledgement is not an
extra initialization timing sample. No Add, competing Branch, or history loop
belongs here.

## Exact payload and storage accounting

`CandidateReceipt` and monitor `saved_fraction` alone cannot measure fresh-import
dedup: duplicate emissions may be coalesced before the receipt. Monitor
`physical_bytes` describes encoded canonical bytes, not SQLite allocation.
Retain both existing fields, but derive the claim from the complete transcript.

For every regular-file payload occurrence record path, ordinal, logical
offset/length, payload chunk ObjectId and full payload length, and any source
offset. Let `L` be logical bytes, `U` the distinct reachable file-payload chunk
IDs, `P` those durably present before the operation, and `I = U - P`. Let `U_B`
and `I_B` sum each distinct chunk's raw payload length once. Record integers
before percentages:

```text
intrinsic payload savings = 1 - U_B / L
incremental payload savings = 1 - I_B / L
sharing factor = L / U_B
preexisting logical coverage = referenced logical bytes backed by P / L
```

For fresh import `P` is empty for regular-file payload and every file's full
input is scanned. File extents must cover full chunks; exact decoded payload
lengths sum to file sizes. Qualify source fixtures so accidental duplication
cannot falsify a unique control. Authenticate canonical bytes for matching IDs;
a collision is an integrity failure. Keep namespace and metadata chunks out of
the regular-file payload denominator even when they use the same object kind.

Also retain ordered chunk-transcript and unique-set digests, canonical file
roots, canonical metadata/namespace object counts and bytes, candidate,
inserted, emitted-preexisting-reuse, and borrowed object/byte counts. Distinguish:

- duplicate occurrences within this import;
- chunks emitted now that already existed before the operation; and
- unchanged extents borrowed without re-emission.

The last category is zero for full fresh import. For incremental families,
source offsets and referenced lengths matter: partial extents can reference a
larger stored chunk, so `U_B` need not equal logical referenced bytes. Report
that retained payload cost without pretending it is zero-copy CAS insertion.

At start and after completion/reconnect, record actual database length and
allocated blocks, SQLite page size/count/freelist and live page bytes, canonical
bytes by role, and the Store file census. Use signed checked deltas. SQLite
growth includes namespace, metadata, indexes, and page slack and is not itself
a payload dedup fraction. Temporary WAL/journal, spools, and fixture files are
reported separately; terminal durable inventory must obey the released Store
contract without an added persistent dedup sidecar.

## Verification, resources, and admission

Seal expected bytes, metadata and full CDC transcripts before candidate work.
The verifier independently walks each result file's rope from the exact root,
authenticates referenced objects, and compares the ordered transcript and
distinct set. Fresh FUSE readback verifies complete bytes, path cardinality,
modes/mtimes, no extra paths, and cleanup. A Store-wide search for chunk-shaped
objects is not a substitute for traversing regular-file payloads.

Record initialization phases, logical ingest and unique insertion rates,
CPU, RSS/cgroup peaks, swaps/OOM, source and Store I/O, CDC scans, transactions,
temporary passes and peak spool bytes, Store growth, and cleanup. A high logical
ingest rate is not a claim of equal physical write throughput.

Run one fresh performance sample per shared seed, with separate verifier
executions under the shared rules. Reuse pristine input/runtime preparation;
preserve independent measured Stores. A selected small case should target the
shared 1–5 second development loop after preparation, but qualification budgets
must account for reading all 500 MiB. Freeze exact correctness/resource gates
before baseline and performance budgets before candidate optimization. Retain
all valid baseline outcomes; no unqualified percent or copied earlier timing
can satisfy admission.

Implementation remains in the existing `fs-bench-pro` crate and namespace
runner where applicable. Preserve the existing runner's frozen `all` meaning;
add explicit new family selection rather than redefining inherited membership.

## Source references

- [Namespace fixture/runner implementation](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Released namespace family](../../../../benchmark/fs-bench-pro/families/init_namespace.rs)
- [Existing namespace runner](../../../../benchmark/fs-bench-pro/run-namespace.sh)
- [FastCDC profile](../../../../crates/layerfs-content/src/file/cdc/gear.rs)
- [Existing-directory importer](../../../../crates/layerfs-layerstack-store/src/layerstack.rs)
- [Canonical admission](../../../../crates/layerfs-layerstack-store/src/objects.rs)
- [Current monitor accounting](../../../../crates/layerfs-monitor/src/dedup.rs)
