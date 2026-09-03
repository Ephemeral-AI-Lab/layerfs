# SDK-only file-edit benchmark rebuild

> **Status:** Current planning checklist; no release candidate exists.

Tracked by [GitHub issue #20](https://github.com/Ephemeral-AI-Lab/layerfs/issues/20).

This specification replaces the v0.1.2 edit-performance admission model. It
does not reinterpret or delete earlier evidence. The earlier POSIX/FUSE rows
remain immutable historical records, but they are not baselines, members, or
admission evidence for the SDK-only families below.

The repository-wide [benchmark rules](../../../general/benchmark_rules.md)
apply without exception.

## Decision

Rebuild exactly three complete file-edit families:

| Family ID | Semantic axis | File-size proof |
| --- | --- | --- |
| `edit_length_preserving` | Final logical byte length equals initial length | Every operation at 1/10/100/500 MiB |
| `edit_length_changing` | Final logical byte length differs from initial length | Every operation at 1/10/100/500 MiB |
| `edit_canonical_chunk_count` | Canonical CDC extent count is preserved, increased, or decreased by a fixed local replacement | Every outcome at 1/10/100/500 MiB |

All three families mutate only through exactly one call to:

```text
Client::edit_workspace_file_range
```

Shell scripts orchestrate. They never mutate files. No benchmarked mutation
uses container Exec, a POSIX write, FUSE write, temporary-copy/rename,
`copy_file_range`, reflink/clone, direct Store mutation, internal test hook, or
another canonical editor.

The measured environment remains a MacBook host, Docker Desktop, one managed
Linux container, a real LayerFS FUSE projection, host-resident Store and SDK,
explicit Commit-return acknowledgement, and explicit End. The container
presents FUSE but runs no mutation workload.

## Claims and non-claims

The new evidence may support only these claims:

1. A bounded SDK edit does not read, copy, scan, spool, or allocate memory in
   proportion to untouched base-file bytes through 500 MiB.
2. For the same local mutation span, the measured SDK edit and Commit latency
   remains within the frozen size-parity envelope across 1/10/100/500 MiB.
3. Core overwrite, insert, delete, append, prepend, grow, shrink, truncate, and
   zero-extension operations have comparable latency when their edit spans and
   public-call topology match.
4. A bounded SDK edit remains resistant to canonical chunk-count changes
   produced by a 64 KiB replacement larger than the 32 KiB maximum CDC chunk.
5. Process and container memory remain within absolute ceilings and do not
   exhibit a file-size-correlated spike through 500 MiB.

The evidence does not claim:

- performance above the measured 500 MiB tier;
- constant-time full-file reads, digests, materialization, initialization, new
  file construction, or full-file replacement;
- performance for POSIX or shell reconstructions of insert/delete/prepend;
- exact nanosecond equality between different edit shapes; or
- a synthetic or extrapolated 100 GiB result.

## Why the earlier admission is replaced

The earlier `edit_same_count` and `edit_count_changing` runners enter a
container workload that mutates through POSIX/FUSE. Several length-changing
members implement a range edit by copying the original prefix and suffix into
a temporary file, synchronizing it, and renaming it. Those rows correctly
measure work proportional to the copied file, but they do not measure the
public SDK range-edit operation.

The earlier 100 MiB delete/shrink rows therefore performed approximately
100 MiB of reads, FUSE/spool writes, and Commit CDC work. Their roughly
112 MiB container peak is a cgroup lifetime peak dominated by the file-sized
spool/page-cache path, not proof that the SDK structural edit needs file-sized
RSS.

The new families remove the unrelated process, Exec, FUSE write, spool, copy,
and full-file CDC round trips. Historical raw evidence remains unchanged and
must be labeled archival and non-admission in new reports.

### Archival disposition

| Existing path | Disposition |
| --- | --- |
| `benchmark/fs-bench-pro/families/edit_same_count.rs` | Reproducibility-only; excluded from active SDK admission |
| `benchmark/fs-bench-pro/families/edit_count_changing.rs` | Reproducibility-only; excluded from active SDK admission |
| `benchmark/fs-bench-pro/run-edit-same-count.sh` | Reproducibility-only old runner |
| `benchmark/fs-bench-pro/run-edit-count-changing.sh` | Reproducibility-only old runner |
| `workload.rs::{same_count_edit,count_changing_edit,rewrite_file_range,...}` | Historical workload code; unreachable from active SDK runners |
| `benchmark-results/fs-bench-pro/edit-same-count/**` | Immutable archival evidence |
| `benchmark-results/fs-bench-pro/edit-count-changing/**` | Immutable archival evidence |

The active release-table generator sources edit claims only from the three new
family manifests. Old POSIX and new SDK rows are never pooled or paired. Raw
historical files are never edited or relabeled in place.

## Shared exact size tiers

Every size-independence cohort uses exact binary units:

| Tier | Exact bytes |
| --- | ---: |
| 1 MiB | 1,048,576 |
| 10 MiB | 10,485,760 |
| 100 MiB | 104,857,600 |
| 500 MiB | 524,288,000 |

Reports must say MiB, never MB. Larger fixtures are exact prefix extensions of
the smaller fixture so shared-offset content remains identical across tiers.

### Standard fixture profile

`sdk-edit-standard-content-v1` is a deterministic streaming SplitMix64 byte
stream. Initialize `state = 0x4c41594552465331`. For every eight output bytes:

```text
state = state + 0x9e3779b97f4a7c15 (mod 2^64)
z = state
z = (z xor (z >> 30)) * 0xbf58476d1ce4e5b9 (mod 2^64)
z = (z xor (z >> 27)) * 0x94d049bb133111eb (mod 2^64)
z = z xor (z >> 31)
emit z as eight little-endian bytes
```

Generation uses a fixed bounded block and never materializes the 500 MiB file
in memory. The fixture file is created with directory mode `0750`, file mode
`0640`, and mtime `1700000000`. These qualified values are frozen before
performance collection:

| Tier | Fixture SHA-256 | Canonical file root | Canonical mapping root | `C0` |
| --- | --- | --- | --- | ---: |
| 1 MiB | `d7dfe3d2828aceb85177e6efbeb600f23672a326c902e525e401c1545bb05bdc` | `8fafdf06fac9dbdffb7ccb6b1bde3b2460c387ef1abc55717dee8be401ff6078` | `dcea0efdabc05e8cc5634505601cf682ee362c7e16e6a6c228b4b699b16b3eea` | 54 |
| 10 MiB | `29c89128c748e4404f31b0147d447bd524d7b75afc98d56ac4debac762ee4b79` | `dd79a6666e83927d787c8a7679b06f4c98ca5f80b6abd48d94b5e8f84aad1c85` | `85cc86ea39b444756034d586424a0575b325aabea5beaf7c249a20b4aadb1638` | 544 |
| 100 MiB | `1bb2d79d54f72ae15eb0bb76ad715b9aafeba8ff8f9aa4f47bad3e3f101885bd` | `bbee7155df021324495d88954be4db125eca49442b50aadc16439f61f6c32efe` | `113604cbe8daefa95427d079c28e82bc6c1a78fd353bc7839b5e72c78cbd84b2` | 5,394 |
| 500 MiB | `bd782f202ec4c40a2070a1d08b78f5135a0ac604b871e4907846740bde906157` | `e4ab3cdbf81fe421e6bd2df0b34e57639845dcf244d127507cf15d6ebe01e9a3` | `9fe7687518d436f9b88c2e983566dd4110fd118b749becf9b087fb60fcc33ee3` | 26,995 |

The preparation manifest also freezes the initialized Layer/Branch root and
Store identity for each prepared Store. A Branch root is scoped to that Store
because inode allocation is intentionally Store-local; both source arms copy
the same prepared Store and must begin with the same frozen Branch root.

### Reusable preparation profile

`sdk-edit-prepared-store-cache-v1` persists only the initialized pristine Store,
Branch identity, fixture manifest, and producing-source provenance. The raw
fixture is removed after initialization; a duplicate 500 MiB input is not
retained. Entries are shared across the three families and selected development
invocations, and only requested exact tiers are prepared.

The key binds the fixture profile, generator/seed, exact tier and digest,
directory/file mode and mtime, Store format/configuration, and a conservative
preparation compatibility digest. That digest covers fixture-preparation code
and initialization/content/Store/SDK dependencies; unrelated run IDs, family
IDs, source-arm names, and whole candidate revisions are not cache keys.
Producing revision, binary digest, and preparation command remain provenance.
Relevant initialization or format changes invalidate the entry; unknown
compatibility fails closed. Both measured source arms use the exact same
qualified Store SHA-256 and initial Branch root.

Build into a sibling staging directory under per-key coordination, validate
after all preparation handles/processes exit, then atomically publish a sealed
immutable entry. The current Store uses MEMORY journaling and rejects WAL;
unexpected sidecars or incomplete entries are rejected, not recovered through
invented WAL handling. Qualification runs on a disposable clone and never
opens the cached master writable.

Every performance or verifier worker still receives a fresh writable APFS
copy-on-write clone or explicitly recorded independent byte copy, never a hard
link. Master integrity is checked once on acquisition/run; its sealed digest
is reused for comparison with every admission clone digest. No per-sample
source rehash is required. Normal cleanup removes ephemeral sample clones, not
the shared prepared input. Corrupt entries fail closed and may be quarantined
before a fully validated replacement is published.

Cache key/hit/miss/provenance and build, validation, clone, qualification, and
container-start walls are setup evidence, separate from all edit/Commit
metrics. OS-cache effects remain declared: neither a cloned nor a hashed Store
is called cold, and changed conditioning profiles are never pooled. The cache
cannot contain edited state, Commit results, live Workspaces, product reader
caches, or previous benchmark/verifier receipts. Expected roots remain
verification-only. Latency, resource, correctness, isolation, and custody gates
are unchanged.

Focused checks must prove a second selected run reuses preparation, each
sample remains pristine, sample mutation cannot alter the master or next
sample, cross-family reuse preserves the artifact identity, and invalidation,
corruption, concurrent builders, and interrupted publication fail safely.

Before performance, a separate preparation-only canonical-path qualifier runs
on disposable pristine Store clones and seals exact expected Branch, file,
mapping roots and extent counts for every plan in `qualification.tsv`. It does
not execute inside the performance or verifier process. Each source-arm
verifier consumes these presealed expectations and independently checks its
streamed byte oracle; agreement between the two measured arms alone is not an
expected-root proof. These expected values are never input to the mutation.

### Replacement profile

All ordinary Inline replacement bytes are generated before timing from a
separate SplitMix64 stream. Its initial state is the first eight bytes,
interpreted as a big-endian `u64`, of:

```text
SHA-256(
  "layerfs/fs-bench-pro/sdk-edit-replacement-seed/v1\\0"
  || family_id || "\\0" || operation_key || "\\0"
)
```

It then emits the standard fixture profile's SplitMix64 little-endian byte
stream. Empty Inline replacements use the standard SHA-256 of empty input;
Zero replacements hash their logical zero bytes. The edit-plan digest is:

```text
SHA-256(
  "layerfs/fs-bench-pro/sdk-file-edit-plan/v1\\0"
  || family_id || "\\0" || scenario_id || "\\0"
  || initial_len_be_u64 || start_be_u64 || delete_len_be_u64
  || replacement_kind_byte   # I or Z
  || replacement_len_be_u64 || replacement_sha256_raw_32
)
```

The exact ordered scenario registries, per-scenario edit-plan digests, payload
seeds/digests, and final lengths are constants in the family definition
modules and are emitted byte-for-byte to `scenario-registry.tsv`. Their frozen
family-manifest SHA-256 values are:

| Family | Definition manifest SHA-256 |
| --- | --- |
| `edit_length_preserving` | `daa3bcb8ba94da6dc28f7ca87dc2b27612c9988cf42fe5398cdddb3a5b386324` |
| `edit_length_changing` | `b6e8d0ab87a2ed72234623198994a460484bd950a04bb81a99a9aecda06c4390` |
| `edit_canonical_chunk_count` | `e76f9b08f7312abf0f30447765e9ff734cecd6c41210788bd4917286059158bf` |
| Combined ordered registry | `1773c7b82f739eaf1c2b8a2877f56baaa7e72b26ac8980802bdb82c80e270af6` |

The five performance repetitions use the same plan; repetition changes run
order, not benchmark semantics. Baseline, candidate, and verifier consume the
exact same byte-identical plan.

The ordinary Inline payload seeds and SHA-256 values are:

| Operation key | Seed | Replacement SHA-256 |
| --- | ---: | --- |
| `overwrite-head-4k` | 3,686,764,519,212,284,394 | `faca857b3e7f8b1b8f46c19b1625a1b9995248ae9ac85eae672b85fbc9932375` |
| `overwrite-middle-4k` | 10,800,757,348,883,211,881 | `f2ebe7d18fdbdd17c8aaff760ad8eeb0dfd82dadf874c41dad175ff916b2a6c5` |
| `overwrite-tail-4k` | 3,866,307,116,232,060,780 | `37cf5837649769db2eccb504f2af96c69b9e514304374ed7d74c3b1299c2f385` |
| `insert-middle-4k` | 6,313,238,748,831,594,097 | `568c5408a3f292d4a593d5ffa43736b790b6a5dac749427b0ad53c765e672616` |
| `delete-middle-4k` | 14,631,710,363,380,426,233 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `append-tail-4k` | 10,524,769,729,031,953,950 | `50495bfeedccb8983ead82f1fc3a55b7a45bb5741ecc79970b8a846616f95d22` |
| `prepend-head-4k` | 11,539,886,650,128,519,955 | `a675326972c1cb5168b42d324036fab260ccb91b9df982eaace85cd05682cbdb` |
| `replace-grow-middle-2k-to-4k` | 6,297,716,278,452,303,078 | `5d682316189ab7e945b298632c929e67f90a2b1aa13987181aef3f501421d93e` |
| `replace-shrink-middle-4k-to-2k` | 1,824,427,086,451,703,536 | `5e24d5a26d23669833f39b2c5b3c9f6a3620ba16d3505c710020d31704a8744b` |
| `truncate-tail-4k` | 9,706,727,036,258,497,900 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `zero-extend-tail-4k` | 7,852,731,424,507,589,290 | `ad7facb2586fc6e966c004d7d1d16b024f5805ff7cb47c7a85dabd8b48892ca7` |

## Shared operation vocabulary

For current logical length `L`:

| Operation key | Start | Delete | Replacement | Final length |
| --- | ---: | ---: | --- | ---: |
| `overwrite-head-4k` | 0 | 4 KiB | 4 KiB Inline | `L` |
| `overwrite-middle-4k` | `L/2 - 2 KiB` | 4 KiB | 4 KiB Inline | `L` |
| `overwrite-tail-4k` | `L - 4 KiB` | 4 KiB | 4 KiB Inline | `L` |
| `insert-middle-4k` | `L/2` | 0 | 4 KiB Inline | `L + 4 KiB` |
| `delete-middle-4k` | `L/2 - 2 KiB` | 4 KiB | empty Inline | `L - 4 KiB` |
| `append-tail-4k` | `L` | 0 | 4 KiB Inline | `L + 4 KiB` |
| `prepend-head-4k` | 0 | 0 | 4 KiB Inline | `L + 4 KiB` |
| `replace-grow-middle-2k-to-4k` | `L/2 - 1 KiB` | 2 KiB | 4 KiB Inline | `L + 2 KiB` |
| `replace-shrink-middle-4k-to-2k` | `L/2 - 2 KiB` | 4 KiB | 2 KiB Inline | `L - 2 KiB` |
| `truncate-tail-4k` | `L - 4 KiB` | 4 KiB | empty Inline | `L - 4 KiB` |
| `zero-extend-tail-4k` | `L` | 0 | 4 KiB Zero | `L + 4 KiB` |

## Frozen scenario-ID grammar

The definition self-check expands these grammars in the operation order shown
below and compares the resulting exact ordered registry and cardinality:

```text
edit_length_preserving size cohort:
  <operation>-on-<1|10|100|500>mib-ops-1

edit_length_changing size cohort:
  <operation>-on-<1|10|100|500>mib-ops-1

edit_canonical_chunk_count:
  overwrite-fixed-64k-chunk-count-<preserve|increase|decrease>
    -on-<1|10|100|500>mib-ops-1
```

No alias is a second ID. IDs must be unique across all three registries.

## Family 1: `edit_length_preserving`

### Ownership

```text
benchmark/fs-bench-pro/families/edit_length_preserving.rs
benchmark/fs-bench-pro/run-edit-length-preserving.sh
benchmark-results/fs-bench-pro/edit-length-preserving/<run-id>/
```

### File-size cohort

Each of these three operations has `on-1mib-ops-1`, `on-10mib-ops-1`,
`on-100mib-ops-1`, and `on-500mib-ops-1` IDs:

```text
overwrite-head-4k
overwrite-middle-4k
overwrite-tail-4k
```

Exact size-cohort membership: `3 operations * 4 sizes = 12 IDs`.

Total registered family membership: **12 IDs**. Every scenario uses exactly
one `Client::edit_workspace_file_range` call with one edit, one Commit, and one
End.

## Family 2: `edit_length_changing`

### Ownership

```text
benchmark/fs-bench-pro/families/edit_length_changing.rs
benchmark/fs-bench-pro/run-edit-length-changing.sh
benchmark-results/fs-bench-pro/edit-length-changing/<run-id>/
```

### File-size cohort

Each of these eight operations has `on-1mib-ops-1`, `on-10mib-ops-1`,
`on-100mib-ops-1`, and `on-500mib-ops-1` IDs:

```text
insert-middle-4k
delete-middle-4k
append-tail-4k
prepend-head-4k
replace-grow-middle-2k-to-4k
replace-shrink-middle-4k-to-2k
truncate-tail-4k
zero-extend-tail-4k
```

Exact membership: `8 operations * 4 sizes = 32 IDs`.

Total registered family membership: **32 IDs**. Every scenario uses exactly
one `Client::edit_workspace_file_range` call with one edit, one Commit, and one
End.

Growth and shrink members remain in this one family. No favorable subset may
complete while a sibling is deferred.

## Family 3: `edit_canonical_chunk_count`

This family is orthogonal to logical file length. It isolates the cost of
canonical CDC layout changes without changing file length, edit size,
position, SDK call topology, or base content around the edited range.

### Ownership

```text
benchmark/fs-bench-pro/families/edit_canonical_chunk_count.rs
benchmark/fs-bench-pro/run-edit-canonical-chunk-count.sh
benchmark-results/fs-bench-pro/edit-canonical-chunk-count/<run-id>/
```

### Canonical count definition

For this family only:

```text
canonical_chunk_count = FileStateV3.extent_count
```

The verifier also reports, but never substitutes:

```text
referenced_extent_count
unique_payload_object_count
mapping_node_count
mapping_tree_level
candidate_object_count
inserted_object_count
reused_object_count
```

Extent count and unique payload ObjectId count are different metrics. Reports
must never call them the same thing.

### Fixed operation

Every member performs one same-length 64 KiB overwrite at the fixed logical
range `[147,456, 212,992)`. Because all larger fixtures extend the 1 MiB prefix,
the base bytes and local edit position are identical at every tier. A 64 KiB
replacement exceeds the frozen 32 KiB maximum CDC chunk and exercises multiple
replacement chunks.

Qualification proved that the earlier prospective range
`[491,520, 557,056)` overlapped only four canonical extents and removed only two
whole extents. A 64 KiB replacement requires at least two extents under the
frozen 32 KiB maximum, so a negative delta was mathematically impossible. The
revised range is the first nearby 64 KiB range that removes three whole extents
and supports all three outcomes with the same per-outcome bytes at every tier.
This correction was frozen before harness implementation or performance
collection, as the prospective stop-and-revise rule requires.

### Outcomes and membership

Prepare and freeze exactly three deterministic 64 KiB replacement payloads:

| Outcome | Payload definition | Payload SHA-256 |
| --- | --- | --- |
| `chunk-count-preserve` | replacement SplitMix64 seed `4` | `6403e9f46c8e5034759add37d4d64ecffbeee1f26b719809d4f04a1e02864978` |
| `chunk-count-increase` | replacement SplitMix64 seed `2` | `ba71e3adc4ce9f1645d8f622f6c2600ca8236146f1d6817fce183f80170dade0` |
| `chunk-count-decrease` | 65,536 zero bytes (seed `0`) | `de2f256064a0af797747c2b97505dc0b9f3df0de4f489eac731c23ae9ca9cc31` |

The nonzero canonical payload stream initializes
`state = 0x4348554e4b434e54 xor seed` and otherwise uses the standard SplitMix64
algorithm above.

Each payload is byte-identical at all four file-size tiers and has one frozen
digest. Each outcome must verify both its exact frozen `(C0, C1)` pair at every
tier and its declared relationship:

```text
preserve: C1 == C0
increase: C1 > C0
decrease: C1 < C0
```

A sign-only result is insufficient. The exact qualified outcomes are:

| Outcome | Tier | `(C0, C1, delta)` | Final file SHA-256 | Canonical file root | Canonical mapping root |
| --- | --- | --- | --- | --- | --- |
| preserve | 1 MiB | `(54, 54, 0)` | `a3374b0be7c654cf87f2b8d411d657e170c821837dcce8661759aa5fe1fc7070` | `644ab1b651adc897f95da15461e32e587565b7e2789b377524c8e88aaf03e4a6` | `4c3514314a7daf61074e6b3a3093fb3beb07699a296d87b30e5e7ec316dea714` |
| preserve | 10 MiB | `(544, 544, 0)` | `231b62f873d1c1b498809d40bd92235a5cdf08150abaf802422e109fee490fcc` | `feef5c7528ffb92220caca77fdd89d2d1cce257e977cc204cef1e676ade6e493` | `d38ce6c671b657acee99b8a8848efd653a91dc5bc67005e6e38e9f2e42b96ec1` |
| preserve | 100 MiB | `(5,394, 5,394, 0)` | `f8e906873405662688d8c8add82abef06155c084f84957f0043103fc55d909f3` | `edfd0588251dddcb7fbbd4993a18d9d10e96d43c2ba07f3fa9c11389cd2e88cc` | `6a6ffdee08de34fd91ff016ec43309913930612585695f6ffc7ca40667a5c82b` |
| preserve | 500 MiB | `(26,995, 26,995, 0)` | `5d8205919a2abe3f7c51f1592ceed0977b39fa2c7e6a4568b541e6fa9ed51437` | `728adcbeb98753983afe97b0c2fde4d92251e044e6330d7919012879bc9a1c39` | `1fbf938ddc01cf94487c897c1e001268412f42cafec46e16488554d1178f6afc` |
| increase | 1 MiB | `(54, 55, +1)` | `b320ec162166c71532c93f1013e42b6d0beb9f194c467e043c07057f55101055` | `3756ef696b53388b234cc2c5877240c82a2ed660e9a462e86168bf4ebd9c4fcd` | `98eb01c06366048bf9e53da461bd01da68f7f2af182fc7420f6b1befab5b3c22` |
| increase | 10 MiB | `(544, 545, +1)` | `a5afa52cc6527313281971d1bb816d593cb173abf024a09689406a9b7afe01b5` | `e24afbe82dcb11d8d6084b77d519cf342127d29c4cad7db74fd275ae6ca3fc4d` | `2a5fc94a51cbeef53d196210d5118735f3a8d016f5c0bec198afdf3703965ad6` |
| increase | 100 MiB | `(5,394, 5,395, +1)` | `b95945fead470121e03fa4d1e640582a5d2a2ae63d66ff66b537ce407e408c7d` | `1b5c0ee5c643aaea649b7a76f1b325093f4887ce0e6fddfce1e221f2d5826567` | `1edb462942b53fbf3fb8353ec6f9bced7f23d7ba7628b067a03655a657e3b450` |
| increase | 500 MiB | `(26,995, 26,996, +1)` | `3accd704e6596ce90622d134a35591efe89f6ccb6e84a3d88b5e4aea6378bfd4` | `95fc3c70f8ea88cb3bee08ed9aa9c0fc4aae28eeac480f9d323c0665ddd9dd67` | `db2bebd5ac72048df698a4f6d4b42de4e07c66275397695fa5ee117639fe13b0` |
| decrease | 1 MiB | `(54, 53, -1)` | `dab7df0938e609ac80dddfc7fb6c0ed0a3e2643d5eb9181197cdd0e185920ed6` | `d0b2112fb3ce304634515ec0126759d4d54ce41ddd6ec6772646286445af35dc` | `5d10305e079c3d9c458a25bcfdf159df9d6a2aeb459262d16d852b3170370b85` |
| decrease | 10 MiB | `(544, 543, -1)` | `94d1d712c610775c38ae1be46233ff351e9104ce6af67543d4be3b6c5b8f0d4d` | `3a36a0494a2ad58411826e27d75bfaaa1970594efc06bc165496323256f71552` | `4610f5bfef022392b9edabd15b66bf70bf6f03f35ed0480e3e29d2378b28e238` |
| decrease | 100 MiB | `(5,394, 5,393, -1)` | `0865e1e1cdef049bfb49b5888808fdf87b1add29c45c7da55ff0c5d1f43db961` | `e7405b58a6e108d5cb4f7949766cb43a25b755a71233fb39e5694331950d41f3` | `6acf193d7fe65d835a09dbe2576015c1d14d8a2814cb599a31666727390e520f` |
| decrease | 500 MiB | `(26,995, 26,994, -1)` | `af536c25d5ee02671afa6eb0194534973d7f1595b6a84cf82acbf5305e7a145d` | `b3c43de62a637318f417a091cc82fe3b9c5bccf9d61d33127207182614e14e2e` | `3d81624ed5a969b832d45fd97fc39a8acad32863a8fead9a6dbc62ad6239be15` |

Preparation freezes these values before performance collection. Candidate
output must never be used to retroactively choose or rename the outcome.

The 12 canonical per-scenario edit-plan digests, in preserve/increase/decrease
outcome order and 1/10/100/500 MiB size order, are:

```text
98670b9838650ba214b4d03a0a17c07bee03aaaa0cf98620653e1ac26ad498dc
c711f36b9c0007beb924f7281baddc20e6c638f65ed921ce8f89a31643d69ed8
8ae7d9acf7a126c6ced5b50f472590b12186a9c86f4c1f78d94c7ee02d16cbb3
38b01ee197b5d7ebda259af756e91717b5e60fa81f3c72ecd6a354d1b456a4b0
79d0ec79d07f8a0d3734e1a666c6bc6f3d8e7495b60380a4adafd49b0aac4008
8313dc5ebc5ee365a2d4446f0b62d4a24ba7285afcbcb294729a88eae3677b01
62fd7352d7faf0829ce7794fda7f263da51f9051cd5692059ec29e5a2b108382
331528a1ee71328f87cab410eb5302a0f083bdeee9986ddce61cc3d2db0bc2fd
e5446d891dde9d5ae6db3e2b577e2932b1facbe78c8c1d9c6cfe068dbb5b05c3
029a74fa27a49b21982fb61387da2917a52d0a71957034e9c4263b6ef7f4c63d
8fc6cf7a01c72f01878cb4a89e8d001005f7c2ce41ed8fa8c9150efc709faee8
a3ac2301c04c3a040b448e05d078df1bbc499cfb38bc71f32ec6e2ccdf57bb1d
```

Each outcome has `on-1mib-ops-1`, `on-10mib-ops-1`, `on-100mib-ops-1`, and
`on-500mib-ops-1` IDs.

Exact registered family membership: `3 outcomes * 4 sizes = 12 IDs`.

Fixture qualification uses the real frozen FastCDC and ordinary public
initialization path. It must not directly assemble a noncanonical extent tree.
If deterministic replacement profiles cannot satisfy all exact outcomes at all
four tiers with the same three payloads, stop before performance collection and
revise this prospective specification. Do not choose different per-tier bytes,
weaken the gate, or infer an outcome after seeing candidate performance.

### Paired outcome controls

At each tier, the registered `chunk-count-preserve` row is the shared control
for its `increase` and `decrease` siblings. The two primary descriptors include:

```text
paired_control_id = same-tier preserve scenario ID
pair_fixture_match = exact
pair_initial_root_match = exact
pair_range_match = exact
pair_replacement_length_match = 65,536
pair_call_topology_match = exact
pair_timing_boundary_match = exact
```

The only treatment difference is replacement content and the resulting frozen
canonical extent-count outcome. For each repetition, execute preserve/increase/
decrease in the frozen balanced order recorded by the campaign. Preserve is
already one of the 12 registered IDs; no duplicate control row or fourth
payload is created.

## Total registry and samples

| Family | Registered IDs | Final repetitions per ID | Final-candidate rows | Aggregate verifier receipts |
| --- | ---: | ---: | ---: | ---: |
| `edit_length_preserving` | 12 | 5 | 60 | 12 |
| `edit_length_changing` | 32 | 5 | 160 | 32 |
| `edit_canonical_chunk_count` | 12 | 5 | 60 | 12 |
| **Total** | **56** | — | **280** | **56** |

Five identical-plan repetitions make a 10% parity claim more defensible than
three-sample medians. Repetitions 1–5 use a frozen balanced order so large tiers
and operation shapes do not always run last. No valid sample is discarded or
replaced. Invalid samples remain retained with a failure class; rerun the
complete affected cell.

The table counts **280 final-candidate rows**. An optimization comparison adds
280 comparator rows for 560 total and freezes adjacent order as:

```text
1: baseline -> candidate
2: candidate -> baseline
3: baseline -> candidate
4: candidate -> baseline
5: baseline -> candidate
```

Issue #20 terminal admission uses the directional 560-row form: 280 authentic
baseline rows and 280 candidate rows in the frozen adjacent order above. The
old POSIX rows are never a baseline.

Before either source arm runs, write and seal `sample-order.tsv`. Registry
order is operation/outcome-major with sizes `1, 10, 100, 500` MiB. Repetitions
1–5 rotate that registry left by these frozen offsets:

| Family | Rotation offsets |
| --- | --- |
| `edit_length_preserving` | `0, 5, 10, 3, 8` |
| `edit_length_changing` | `0, 13, 26, 7, 20` |
| `edit_canonical_chunk_count` | `0, 4, 8, 1, 5` |

Within each resulting scenario/repetition cell, source arms stay adjacent and
use the baseline/candidate direction sequence shown above. The unique row key
is `(family, scenario_id, repetition, source_arm)`.

Every ID uses one frozen edit-plan digest across its five performance
repetitions, so one verifier receipt per ID is sufficient. The receipt records
that plan digest and the identities of all five bound performance rows. It must
not imply that five different semantic inputs were verified. In a final-only
campaign it contains one candidate proof. In a directional campaign it
contains independent baseline and candidate subproofs and exact root/digest
equality: 56 aggregate receipts containing 112 source-arm subproofs.

## Pure SDK timing contract

All edit plans and replacement buffers exist before `T0`:

```text
T0 = immediately before public SDK edit call
T1 = immediately after SDK edit returns
T2 = T1 exactly; reuse the same timestamp value before public Commit
T3 = immediately after Commit returns with its public acknowledgement
T4 = immediately after Workspace End
```

Metrics:

```text
edit_call_ns       = T1 - T0
commit_call_ns     = T3 - T2
edit_commit_ns     = T3 - T0
workspace_end_ns   = T4 - T3
```

The receipt must satisfy exact integer equality because every term uses the
same stored timestamps and `T2` reuses `T1`:

```text
edit_commit_ns
  == edit_call_ns + commit_call_ns
```

The primary metric is `edit_commit_ns`. Workspace Create and End are reported
separately and may form an honestly named complete-lifecycle metric, but they
do not replace SDK mutation latency.

Nothing occurs between T0 and T3 except the declared SDK edit and Commit.
Specifically excluded:

- edit-plan construction or cloning;
- fixture checks, stats, or final-length checks;
- process/container/resource queries;
- monitor and receipt collection;
- Store/Commit inspection and `pin_branch`;
- payload-ID or extent enumeration;
- digest, oracle, or canonical-root construction;
- reconnect, reopen, materialization, or cleanup; and
- report generation.

Commit return is the only timed acknowledgement boundary. After End and after
all performance timers stop, exactly one read-only Branch-head query validates
the returned Commit ID. It may invalidate a wrong row and reports a separate
`visibility_validation_ns`, but it never enters a performance distribution or
mutation/lifecycle metric.

## Latency and parity gates

### Absolute candidate gates

Every registered row contains one logical operation, one SDK call, and one edit
member. Report all three nominal median targets independently:

```text
edit_call_ns   <= 10 ms
commit_call_ns <= 10 ms
edit_commit_ns <= 20 ms
```

The values above are nominal targets. Before further acceptance classification,
the user approved an absolute +10 ms tolerance on 2026-09-04. Accepted median
ceilings are therefore 20 ms Edit, 20 ms Commit, and 30 ms combined. The combined
ceiling applies independently: it is not 40 ms. Reports must distinguish
`nominal-pass` from `accepted-with-tolerance`; meeting only the accepted ceiling
must not be described as meeting the nominal target. A row fails if any one
metric exceeds its ceiling, even when the other two or their sum pass.

This tolerance changes no size-parity or matched-operation rule, and no
correctness, memory, sampling-coverage, no-amplification, preparation-cache,
cleanup, or source-identity requirement. Prior selected diagnostics remain
non-admission evidence; all three complete families must use the prospectively
updated, identical harness and policy. Localized edits must remain independent
of untouched file size; optimizing already-accepted millisecond values is not
a prerequisite to proceeding with the complete matrix.

Every sample additionally requires:

```text
logical_operation_count == 1
sdk_edit_member_count == 1
public_sdk_edit_call_count == 1
```

### File-size parity

**Final user-approved interpretation, 2026-09-04:** the cross-size parity
formula below remains binding for `edit_call_ns` only. Cross-size Commit and
combined spreads are reported diagnostics, not admission blockers. Accepted
absolute medians remain 20/20/30 ms (nominal 10/10/20). Describe size-stable
localized edits with bounded Commit latency; do not claim size-independent
Commit. This supersedes the older all-three-metrics requirement below, without
changing matched-operation parity, numerical memory limits under the approved
observation scopes, correctness, mutation semantics, or source integrity.
Subsequent explicit user review accepts exactly these recorded discrepancies:
length-changing delete-middle Edit cross-size spread 2.571958 ms;
replace-shrink Edit cross-size spread 2.111083 ms; and delete versus truncate
at 1 MiB Edit spread 2.484458 ms. Preserve their original 2 ms strict-rule
failures as diagnostics, not nominal passes. No other Edit exception is
authorized. Matched-operation Commit/combined spreads are also diagnostic,
consistent with bounded Commit acceptance; matched-operation Edit remains
binding except for the one reviewed delete/truncate case.
Final classification records both the immutable collection policy/source and
this explicitly approved acceptance policy. Original raw rows and prior
classifications remain unchanged.

For a fixed family, operation/outcome, and source arm, apply this formula
independently to the medians of `edit_call_ns`, `commit_call_ns`, and
`edit_commit_ns`. Let `m_N` be the chosen metric at tier `N` and
`m_min = min(m_1, m_10, m_100, m_500)`:

```text
max(m_1, m_10, m_100, m_500) - m_min
    <= max(2 ms, 0.10 * m_min)
```

Report `m_10/m_1`, `m_100/m_1`, and `m_500/m_1`. Above the envelope is no-go.
The 2 ms allowance applies only to a local phase/parity comparison; it does not
relax an absolute latency ceiling or any hard resource gate.

Every final-candidate operation/outcome in all three families must pass this
gate. A partial size matrix cannot support a family-level size-independence
claim. A comparison baseline must be authentic, correct, resource-valid,
topology-matched, and source-bound, but may retain the performance no-go that
the candidate is intended to fix.

### Cross-operation parity

Hard parity cohorts contain only byte- and topology-matched work. At each
file-size tier compare:

```text
Inline insert cohort:
  insert-middle-4k, append-tail-4k, prepend-head-4k
  deleted bytes = 0, Inline bytes = 4 KiB, one edit member, one live run

Delete cohort:
  delete-middle-4k, truncate-tail-4k
  deleted bytes = 4 KiB, replacement bytes = 0, one edit member

Overwrite-position cohort:
  overwrite-head-4k, overwrite-middle-4k, overwrite-tail-4k
  deleted bytes = Inline bytes = 4 KiB, one edit member, one live run
```

Use the same `max - min <= max(2 ms, 10% * min)` envelope independently for
each cohort and each of the three latency metrics. Zero extension, grow, and
shrink retain their absolute and file-size gates where they lack a byte-matched
peer.

The report also shows one broad table for all one-operation shapes. Its
nonbinding diagnostic target is a maximum `edit_commit_ns` median spread of
5 ms, with an alert above 7 ms that requires a phase/counter explanation. This
broad table expresses the product desire that local edit shapes feel similar,
but it has no admission consequence and does not replace the hard matched-work
parity cohorts.

## No-amplification hard gates

Every performance sample, not only its median, must satisfy:

```text
operation_surface == public-sdk
mutation_executor == fs-benchmark-pro-sdk
workspace_execution_count == 0
timed_call_graph_manifest_status == pass
operation_route_manifest_status == pass

capture_mode == Live
captured_files == 0
captured_bytes == 0

all edit-caused FUSE kernel/client/frame/host payload bytes == 0
spool_write_bytes == 0
spool_allocated_bytes == 0
spool_live_bytes == 0
spool_superseded_bytes == 0
physical_spool_high_water_bytes == 0

commit_cdc_bytes_scanned == final_live_non_base_bytes
candidate_bytes <= final_live_non_base_bytes + 8 MiB
inserted_bytes <= candidate_bytes
max_transaction_objects <= 127
max_transaction_bytes < 4 MiB

swap_bytes == 0
oom == false
oom_kill_delta == 0
timeout == false
cleanup_status == pass
active_execution_count == 0
active_workspace_count == 0 after End
```

The two manifest statuses are sealed static proofs, not fabricated numeric
counters. The isolated timed module forbids shell/POSIX/FUSE mutation and
alternate edit entrypoints. Runtime CDC, payload-read, candidate-byte,
Workspace-execution, FUSE, and spool tripwires reject a hidden full-file or
fallback path. If later product instrumentation exposes a real fallback
counter, it becomes a required exact-zero field through a schema revision.

`final_live_non_base_bytes` is the Inline/Zero content reachable after overlap
and supersession normalization, not total supplied bytes and never total file
length. A whole-file rebuild would make CDC/candidate work approach the file
size and fail these gates.

For one-edit 4 KiB rows:

```text
piece_count <= 3
piece_logical_charge_bytes <= 1 KiB
```

For 64 KiB chunk-count rows, the family self-check freezes exact expected
piece/count/height/charge bounds. It must not use only the much larger product
maximum as a benchmark target.

Length-changing rows require `commit_payload_bytes_read == 0`.
Length-preserving replacements deliberately differ in their first byte and
require:

```text
commit_payload_bytes_read
    <= 64 KiB * maximal_live_replacement_runs
```

Do not enumerate old payload IDs during performance. Verification uses
operation-specific retention expectations; delete and overwrite are not
required to retain objects that their ranges completely remove.

## Memory-spike hard gates

The 500 MiB tier must not recreate the earlier file-sized spool/page-cache
curve. A latency pass cannot override a memory failure.

Heavy fixture generation, hashing, initialization, and Store preparation run
in a separate process that exits before the fresh timed worker. Each sample
uses one fresh worker, Branch, Workspace, and prepared pristine Store state so
its memory is not inherited from fixture construction or an earlier edit.

The supervisor observes the worker's current RSS only while T0–T3 is active and
records:

```text
rss_baseline_bytes
rss_phase_peak_bytes
rss_incremental_peak_bytes
rss_final_bytes
process_lifetime_peak_rss_bytes
rss_sample_interval_ns
rss_sample_count
rss_first_sample_ns
rss_last_sample_ns
rss_maximum_sample_gap_ns
```

Endpoint RSS is not a peak. Lifetime high-water is reported separately and is
never called incremental memory. Define:

```text
rss_incremental_peak_bytes =
    saturating_sub(rss_phase_peak_bytes, rss_baseline_bytes)
```

A native external supervisor samples without allocating or querying resources
inside the measured worker. The interval and maximum observed gap must be at
most 1 ms. Coverage requires observations at the T0 and T3 boundaries plus at
least one interior sample. Missing boundaries, fewer than three observations,
or a gap above 1 ms makes phase RSS unavailable and the row
admission-ineligible.

Process gates, per sample unless stated otherwise:

```text
rss_phase_peak_bytes target <= 112 MiB
rss_phase_peak_bytes hard   <= 128 MiB
rss_incremental_peak_bytes  <= 32 MiB
process_lifetime_peak_rss_bytes <= 128 MiB
swap_bytes == 0
```

Compute the 16 MiB median phase-peak spread independently for each fixed
`(family, operation/outcome, logical operation count)` candidate size cohort.
Do not pool edit shapes, counts, families, or source arms.

The container cgroup measures the projection/daemon, not the host SDK process.
Before performance collection, implement and demonstrate one daemon-native
sampler in the existing control daemon. Arm it before T0 and disarm it after T3
without spawning a process inside the cgroup. It records total and domain
maxima, boundary timestamps, sample count, interval, and maximum gap. Do not
poll with repeated `docker exec`; the current helper creates work inside the
measured cgroup. A fresh/reset cgroup lifetime peak remains a conservative
sample guard, not the T0–T3 phase metric.

The daemon-native sampler records `memory.current`, `memory.peak`, and the
relevant `memory.stat` domains:

```text
anon
file
shmem
file_dirty
file_writeback
kernel
slab
sock
```

Define the cgroup fields exactly:

```text
cgroup_phase_peak_bytes =
    max(memory.current(t)) for T0 <= t <= T3

cgroup_phase_incremental_peak_bytes =
    saturating_sub(cgroup_phase_peak_bytes, memory.current(T0))

dirty_writeback_incremental_peak_bytes =
    max(saturating_sub(
        file_dirty(t) + file_writeback(t),
        file_dirty(T0) + file_writeback(T0))) for T0 <= t <= T3

cgroup_lifetime_peak_bytes = memory.peak(T3)
```

`cgroup_lifetime_peak_bytes` is a separate conservative sample guard and never
substitutes for the phase peak.

Cgroup gates:

```text
cgroup_phase_peak_bytes target <= 112 MiB
cgroup_phase_peak_bytes hard   <= 128 MiB
cgroup_phase_incremental_peak_bytes <= 32 MiB
dirty_writeback_incremental_peak_bytes <= 8 MiB
cgroup_lifetime_peak_bytes <= 128 MiB
swap.current == 0
OOM and OOM-kill deltas == 0
```

Compute the 16 MiB median cgroup-peak spread using the same fixed candidate
cohorts as process RSS. The cgroup sampler uses the same maximum 1 ms interval,
T0/T3 boundary observations, interior-observation requirement, and coverage
failure semantics.

### Clock-domain and boundary implementation

**Superseding user-approved observation policy, 2026-09-04:** the active profile
is `ack-window-v1`. Exact host/VM phase attribution, the 400 microsecond offset
cutoff, calibration retries, guaranteed interior witnesses, and one-millisecond
memory-boundary/gap rules below are historical diagnostics, not admission gates.
Do not run clock probes for this profile. SDK Edit/Commit timestamps remain in
the same host clock with their unchanged timing equation.

After setup, start the daemon sampler and receive its ready acknowledgment
before T0. Request its final observation after T3 (after End in performance
mode). Those causal acknowledgments bracket a broader observation window;
report its actual native timestamps, duration, sample count and gaps. Exact
cgroup T0–T3 attribution is `unavailable`. Name the category/total periodic
maxima `sampled` window observations, not exact phase or continuous peaks.
The daemon's native `memory.peak` is a whole-container lifetime upper bound;
the worker's native RSS high-water value is a whole-worker bound. Neither is
renamed an exact edit-phase peak. Report conservative incremental upper bounds
using these lifetime peaks minus the respective observed starting baselines.

Keep 128 MiB native peak ceilings, 32 MiB incremental upper-bound ceilings,
16 MiB cross-size native-peak spreads, zero observed swap and zero OOM/OOM-kill
events, and all no-copy/spool/route/correctness gates. The 8 MiB dirty/writeback
ceiling applies to available sampled observations; continuous category peaks
and transient swap between observations cannot be strictly proven and must be
explicitly disclosed. Sampling gaps/precision are diagnostic, not another
abort gate. Missing/unreadable observations are unavailable, never synthesized.
Do not pool prior exact-phase attempts with this profile or relabel failures.
All three fresh families must use the same prospectively frozen profile.

Host T0/T1/T3/T4 and external RSS observations use the same native
`CLOCK_MONOTONIC_RAW` domain. T2 reuses T1; no wall-clock query or resource RPC
is inserted between T3 and End. The daemon sampler uses an `Instant`-relative
monotonic epoch, not an assumed host/VM wall-clock alignment.

Prospective calibration correction, 2026-09-04: establish the authenticated
owner/Workspace-bound sampler connection and warm the sampler before the clock
bracket. Retain that connection for at most five bounded pre-edit clock probes.
Each probe supplies daemon receive/send times in the same sampler epoch;
together with host send/receive times they bound offset uncertainty after
subtracting daemon processing time. This excludes connection/authentication and
sampler-start setup from the clock bracket, not from reported setup cost:
`clock_sampler_start_ns` reports that wall time separately. A rejected probe is
not a rejected operation sample: no edit or performance sample has run yet.
Exhausting all five probes fails before editing. Do not retry the operation,
select a favorable timed result, or reset the sampler between probes.
Offset uncertainty must be at most 400 microseconds, followed by a fixed
untimed 2 ms sampler-settle interval. The receipt declares a conservative
1000 ppm clock-rate allowance, applied to the observed calibration-to-T3 age;
that age is capped at two seconds. Total uncertainty is carried into the
existing one-millisecond coverage gate, never added as a tolerance to it.

For mapped boundaries `M0`, `M3` and total uncertainty `U`, choose baseline
`B <= M0-U` and final sample `F >= M3+U`. Require worst-case distances
`M0+U-B <= 1 ms` and `F-(M3-U) <= 1 ms`. An interior witness must lie strictly
between `M0+U` and `M3-U`. Domain, dirty/writeback, swap, and total sampled peaks
cover the expanded possible phase `[M0-U, M3+U]` and are labeled conservative
uncertainty-bounded phase peaks. The final lifetime guard uses the first
boundary sample at or after `M3+U`. Raw records retain calibration operands,
mapped bounds, worst distances, and an interior witness so the report
generator independently recomputes coverage. Missing bracketing sides fail.

If the required phase/sample scope or attribution is unavailable, the row is
admission-ineligible. Do not substitute a campaign-lifetime peak, subtract
process RSS from cgroup memory, or classify physical spool disk as RSS.

The evidence claim stops at the measured 500 MiB tier. Complexity analysis may
explain the absence of a term proportional to untouched bytes, but it is not a
measurement above 500 MiB.

### Container build-cache integrity

Before compiling the daemon/helper image, clean the cached Cargo artifacts for
the three locally compiled product packages (`layerfs-content`, `layerfs-daemon`,
and `layerfs-fuse`) for the selected target. External dependency downloads and
build artifacts may remain cached. Docker can restore an older source layer
whose timestamps precede a shared target cache; source labels alone therefore
do not prove that Cargo rebuilt the corresponding product. Preserve build logs
and verify actual helper identities across the source arms. The incomplete
`terminal-6690b6e6` campaign exposed this stale-helper failure and is invalid,
not an operation-latency or calibration failure. Do not overwrite or reuse it.

## Performance and verification separation

**User-approved execution order, 2026-09-04:** collect performance for all three
families first (`--stage performance`), review their complete elapsed-time
tables, and only then run each saved family's `--stage verification`. The
verification stage validates the sealed performance collection and its original
preflight/qualification/identity, does not rerun performance or regenerate its
oracle, and binds all original performance row IDs. Performance completion is
not final admission; a numerical no-go is retained while independent final
verification may still collect its proof. Do not run family A's full verifier
before family B's performance. Qualification is input/oracle preparation, not
the full byte/reopen/materialization verifier.

After complete performance and verification, generate the unpublished draft
tables, commit documentation/evidence, then run the complete repository gate
once on that documentation-complete checkout, binding the measured candidate
through the unchanged-source documentation bridge. This one final gate may
serve both repository and documentation-gate selectors. Final terminal checking
still requires that gate, all proofs, and the evidence-only final bridge. A
draft table generated before this gate is not issue-closure evidence.

Performance emits timings, counters, resource data, cleanup status, and:

```text
verification_status = not-run-performance-mode
performance_distribution = true
```

It performs no full-file digest, extent walk, payload-ID enumeration, root
oracle, reconnect, reopen, materialization, or failure injection.

Every registered ID receives one separate verifier receipt. Verification uses
the same SDK mutation call and proves:

- exact initial and final length;
- exact final bytes and streaming SHA-256 from an independent range oracle;
- exact expected canonical file and Branch roots;
- fresh Client/Store reconnect;
- fresh read-only FUSE reopen;
- materialized equality;
- expected inode behavior;
- operation-specific payload-object retention;
- exact chunk counts and map digest where applicable;
- failure atomicity and no duplicate edit after retry;
- structural and memory/resource ceilings; and
- cleanup.

Verifier rows set `performance_distribution = false` and never alter
performance summaries. There is no synthetic 100 GiB verifier.

## Fast development and timeouts

```text
run-edit-*.sh --self-check
    no Docker or product execution; target under 2 seconds

run-edit-*.sh RUN_ID CONTAINER --case ID --repetition 1 --mode performance --source candidate
    exactly one final-source row; no verifier; admission_eligible=false

run-edit-*.sh RUN_ID CONTAINER --case ID --mode verify --source candidate
    exactly one verifier; admission_eligible=false

run-edit-*.sh RUN_ID CONTAINER --all --mode admission
    complete indivisible family; verification only after performance passes
```

Development sequence:

1. self-check;
2. one 1 MiB operation;
3. its 10/100/500 MiB siblings;
4. all operation shapes at one tier;
5. the smallest failing chunk-count cell;
6. optimize the smallest shared root cause;
7. rerun only affected selected cells; and
8. run complete terminal families once selected gates are green.

Short supervisor ceilings:

```text
SDK edit call                 2 seconds
Commit call                  2 seconds
fresh timed worker          10 seconds
one 500 MiB preparation     30 seconds
one 500 MiB verifier        30 seconds
```

The family wrapper computes its ceiling from registered workers and prepare /
verify budgets. It does not use one arbitrary 150-second operation timeout.
Setup walls are always reported separately.
The external performance supervisor additionally caps the whole
edit/Commit/End region at two seconds. This stronger development tripwire
enforces the individual two-second call ceilings without adding phase-marker
work between the public calls; the complete fresh-worker ceiling remains ten
seconds.

## Static and runtime route enforcement

Place timed SDK mutation in one small shared Rust module used by all three
families. Its call graph is restricted to:

```text
prebuilt edit plan
-> public SDK range edit
-> public Commit
-> End
-> post-timing read-only Branch-head validation
```

The timed module self-check rejects imports/calls for filesystem mutation,
`File::create`, `OpenOptions`, `set_len`, write, rename, copy, remove, shell or
`Command`, Workspace execution, FUSE mutation, container workload execution,
`copy_file_range`, reflink, or clone. Preparation and verification live outside
the timed module so the allowlist remains meaningful.

Runtime admission independently requires zero Workspace executions,
shell/POSIX/FUSE mutation, spool allocation, and fallback; exact bounded CDC;
bounded base reads and candidate bytes; and complete cleanup.

## Evidence layout

The exact JSON schema identifiers are frozen as:

| Artifact | `schema` value |
| --- | --- |
| Performance row | `fs-bench-pro-sdk-edit-performance-v1` |
| Verification aggregate receipt | `fs-bench-pro-sdk-edit-verification-v1` |
| Performance/verification summary | `fs-bench-pro-sdk-edit-summary-v1` |
| Run status | `fs-bench-pro-sdk-edit-status-v1` |
| Fixture manifest | `fs-bench-pro-sdk-edit-fixture-v1` |
| Daemon cgroup sample | `fs-bench-pro-sdk-edit-cgroup-v1` |
| Supervisor process RSS sample | `fs-bench-pro-sdk-edit-process-rss-v1` |

Schema changes require a new identifier; missing required fields are failures,
not implicit zeroes.

Each family writes:

```text
benchmark-results/fs-bench-pro/<family>/<run-id>/
├── environment/
│   ├── command.txt
│   ├── source-identity.json
│   ├── image.json
│   ├── fixture-manifest.json
│   ├── scenario-registry.tsv
│   └── sample-order.tsv
├── performance/
│   ├── raw.jsonl
│   └── summary.json
├── verification/
│   ├── raw.jsonl
│   └── summary.json
├── scenarios/<scenario>/<repetition>/
│   ├── raw.jsonl
│   ├── supervisor.txt
│   └── exit-status.txt
├── run-status.json
├── report.md
└── evidence.sha256
```

One shared non-executable shell library may own argument handling, custody,
container lifecycle, timeouts, and evidence sealing. It is not a fourth family
runner. One shared Rust SDK executor owns call topology and receipts. Family
modules own only definitions, schedules, fixture identities, expected counts,
and self-checks.

## Files the implementer must read

- [Benchmark rules](../../../general/benchmark_rules.md)
- [`fs-bench-pro` family format](fs-bench-pro-format.md)
- [Universal edit engine](universal-file-edit-engine.md)
- [Existing length-preserving specification](same-count-file-edits.md)
- [Existing length-changing specification](count-changing-file-edits.md)
- [`Client` SDK](../../../../crates/layerfs-sdk/src/client.rs)
- [Workspace lifecycle](../../../../crates/layerfs-workspace/src/lifecycle.rs)
- [Workspace file I/O](../../../../crates/layerfs-workspace/src/file_io.rs)
- [Workspace piece tree](../../../../crates/layerfs-workspace/src/file_edit.rs)
- [Workspace Commit planning](../../../../crates/layerfs-workspace/src/changes.rs)
- [Persistent rope editing](../../../../crates/layerfs-content/src/file/rope/edit.rs)
- [FastCDC profile](../../../../crates/layerfs-content/src/file/cdc/gear.rs)
- [Existing benchmark host](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Existing workload to remove from active edit routes](../../../../benchmark/fs-bench-pro/workload.rs)

## Required documentation and issue order

Before implementation or collection:

1. commit this specification and the general benchmark rules;
2. update the v0.1.2 README, family-format document, earlier edit-family
   documents, release notes, and parent issue to mark old edit admission as
   superseded and the release as blocked;
3. freeze the chunk-count fixture/replacement manifests and exact `(C0, C1)`
   values in this specification;
4. update the tracking issue with the exact frozen registry and cardinalities;
5. only then edit the harness, definitions, or runners.

## Completion gates

### Documentation-only finalization bridge

**Final approved-policy bridge:** the terminal producer remains pinned to
`3337728e9846a200d7a5cc08d076de18f1d5436c` and its original contract. The final
documentation commit records the explicitly approved parity interpretation
above; its contract hash is separately pinned. A separately identified final
consumer may recognize the producer's unavailable-attribution field alias and
apply only those approved classification changes, without modifying raw rows,
source proofs, or the frozen producer report. It must retain every original
finding and fail on any other resource/correctness/identity failure. Product,
compiled source, harness, lockfile, workload, preparation, and frozen report
and custody helpers remain byte-identical. The performance-first amendment
requires one full repository-gate sequence on the documentation-complete
checkout, not the two sequences described historically below. A subsequent
evidence-only commit may add gate/terminal artifacts and the selector JSON.

Retain the exact measured candidate revision and its repository-gate receipts.
After its family evidence passes, generated human reports and evidence may be
committed at a later revision only through a checked documentation-only bridge:
product, source, harness, Cargo lock, contract, workload, preparation
compatibility, and both report generators plus the custody helper must remain
byte-identical. The intervening diff is limited to the release-note documents,
the v0.1.2 roadmap README, optimization history, and the explicitly named new
SDK family/build/repository-gate/terminal evidence directories. The frozen
specification and benchmark rules are not editable through this bridge.

Run a second complete repository-gate sequence on that documentation-complete
commit, recording both its revision and the measured candidate plus the exact
approved diff. A final evidence-record commit may add only those gate/terminal
artifacts and the release evidence-selector JSON; it cannot change human claims
or code. The terminal evaluator requires both gate sets and validates this
last evidence-only diff. This avoids self-referential report/commit hashes
without reusing stale product evidence or relaxing any benchmark gate.

### Terminal checklist

- [ ] All active edit mutations use the public SDK and zero forbidden routes.
- [ ] One definition and one runner own each of the three complete families.
- [ ] All 56 registered IDs and five repetitions per ID exist exactly once.
- [ ] Every operation/outcome has complete 1/10/100/500 MiB evidence.
- [ ] All three absolute latency gates plus file-size and matched-operation
  parity gates pass.
- [ ] Every per-sample no-amplification and no-fallback gate passes.
- [ ] Phase/sample-scoped process and cgroup memory gates pass with no
  file-size-correlated spike through 500 MiB.
- [ ] All 56 separate verifier receipts pass.
- [ ] Reports show elapsed-time and memory tables with medians, min-max ranges,
  sample counts, exact units, statuses, and raw-evidence links.
- [ ] Exact source/product/harness/fixture/image/environment/report custody and
  manifests verify.
- [ ] The v0.1.2 tag and GitHub Release remain absent until all gates pass.
