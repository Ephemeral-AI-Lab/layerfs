# Independent review: pooled physical-group reuse

Status: source review and independent final raw-run reconciliation complete.
Reviewer changed only this document. No product/harness edits, builds, tests,
benchmark invocations or Git mutations were performed by the reviewer.

## Source verdict

No blocking correctness finding in the reviewed candidate. This verdict covers
the source diff and test design, not an independent execution of those tests.

The implementation borrows the existing session `GroupCache` through
`Resolver::resolve_charged` into `PoolReader::leaf_canonical_with_groups`, then
passes the optional mutable borrow through the two physical-record passes.
There is no second physical-group cache field and no ownership transfer on the
error path. Public `PoolReader::leaf_body`, `leaf_canonical` and `stored_base`
continue using the uncached path, including the save-side trial callers.

Inspected source references (repository-relative, candidate line numbers):

- `core/crates/layerfs-storage/src/encoding/delta/read.rs:140`: a fresh
  `PoolReader` still owns each requested pooled leaf; the existing group-cache
  borrow enters here. Final canonical identity authentication remains here.
- `core/crates/layerfs-storage/src/encoding/pool/read.rs:340`: physical record
  acquisition validates canonical-length limit, current pack header/lane, group
  directory and selected extent before consulting decoded compressed groups.
  Raw groups preserve their prior owned-copy path. Cache hits still check
  decoded length, record framing/ordinal and `METADATA_RECORD_LIMIT`.
- `core/crates/layerfs-storage/src/encoding/pool/read.rs:418`: the private cached
  entry checks the root's publication ceiling before reuse. Dependency locations
  still use the captured ceiling; chain-role and chronology checks remain.
- `core/crates/layerfs-storage/src/encoding/pool/read.rs:286`: reverse-pass
  canonical and encoded work charges execute for every element whether its
  physical group came from the cache or decompression. There is no policy change.
- `core/crates/layerfs-storage/src/encoding/pool/read.rs:148`: the value-group
  cache and `decoded_work` charging remain separate from physical-group reuse.
  Fresh per-leaf value readers preserve their prior lifetime and refusal behavior.
- `core/crates/layerfs-storage/src/encoding/decode.rs:23`: the reused cache keeps
  its existing 512 KiB retained-body bound and whole-cache eviction discipline.
  Ordinary group decoded length remains bounded by 64 KiB through the existing
  pack validator. Insertion may temporarily coexist with the next decoded group,
  so retained-byte capacity is not a promise of a 512 KiB total process peak.
- `core/crates/layerfs-storage/src/cas/pool_lane.rs:329` and `:360`: save-side
  stored-base and physical-body readers remain the public uncached methods.
  `cas/placement.rs:181` retains its existing pack invalidation call.

The provider cache may now contain pooled and ordinary physical groups, but both
use the same `(pack_id, group_number)` key for identical decoded physical bytes.
This changes cache competition, not the retention ceiling. Ordinary and pooled
decode counters should therefore both be reported; eviction can affect either.

## Counter interpretation

`PoolReadCounters` separates leaf requests, dependency edges, physical record
calls, actual successful Zstandard group decompressions, decoded bytes, cache
hits, value-group materializations and pack BLOB fetches/bytes. This closes the
previous omission of the pooled branch from ordinary decompression counters.

Actual-decompression counters are assigned after successful decompression;
`record` accumulates that local work even if later framing fails. Provider and
read-wave results expose successful waves only, as documented. They are not a
complete account of failed-wave work. Value-group materialization includes raw
groups; pack bytes are SQLite BLOB acquisitions, not measured disk traffic.

For an all-compressed successful chain, the independent invariant is
`physical_record_calls = 2 * (leaf_requests + chain_edges)` and
`physical_record_calls = physical_group_decodes + physical_group_cache_hits`.
Raw-group chains retain the first identity but need not satisfy the second.
These are work identities, not estimates of elapsed time.

## Test design reviewed

The added external tests exercise the actual public provider and Resolver APIs:

- A compressed FULL leaf and four-element chain assert canonical bytes, identity,
  two-pass record counts, first-wave decompression, second-wave physical reuse,
  and unchanged fresh value materialization.
- Warm and cold GroupCaches with canonical or encoded chain budgets reduced to
  one produce the same `Integrity("pooled chain work")` refusal.
- A warmed provider still refuses a damaged locator record ordinal and a newly
  lowered publication ceiling.
- Ten separate one-row chain elements are reframed using real pack/group
  assembly with unreferenced framed padding. Their decoded groups exceed 512 KiB
  while canonical chain work stays bounded. The expected 18 decompressions and
  two hits exercise whole-cache eviction rather than claiming all chains halve
  decompression work.

The fixture padding stays within ordinary decoded group limits and preserves
the original record ordinal and framing. It exercises physical cache pressure;
it is not a representative retained-history performance workload.

## Qualifications

- Reuse assumes published physical groups remain immutable, the existing ordinary
  GroupCache contract. It does not promise freshly detecting arbitrary external
  mutation to a compressed body already retained by the session. Current header,
  extent and length validation still runs, and returned canonical bytes are
  authenticated. Save readers retain their previous uncached behavior.
- Identical instrumentation in both arms does not measure instrumentation
  overhead independently. The fresh timing pair must retain that qualification.
- Cache residency is uncontrolled under the frozen protocol. Timing comparisons
  are diagnostics, not cold admission or proof of matching v0.1.6.
- The reviewer has not rerun tests or measurements. Arithmetic and file hashes
  were independently checked as described below. Root owns execution and its
  test/check logs; the source/test-design review does not substitute for them.

## Final evidence reconciliation

The reviewer used independent Python reads of each `raw/timing.json` and
`trace-perf.jsonl`, summing state children and per-state provider counters rather
than trusting `analyze.py` or its output. For every one of the 70 states in each
arm, both successful all-compressed counter identities stated above hold.

| Measured operation | Baseline ns | Candidate ns | Exact reduction ns |
| --- | ---: | ---: | ---: |
| Stride10 | 19,660,740,792 | 16,268,082,127 | 3,392,658,665 |
| Stride3 | 56,736,225,583 | 44,591,815,082 | 12,144,410,501 |

Reduction percentages are `100 * reduction / baseline`: **17.256006276124044%**
and **21.405037744771757%**. They describe these fresh diagnostic pairs only.
The prior campaign's baseline times differ materially, so compounding percentages
across the campaigns would not be a matched measurement.

| Actual work or overlapping time | Stride10 baseline → candidate | Stride3 baseline → candidate |
| --- | ---: | ---: |
| Provider elapsed ns, nested within operation | 9,856,060,754 → 6,648,561,878 | 37,227,856,889 → 25,319,737,028 |
| Pooled leaf requests | 11,268 → 11,268 | 41,958 → 41,958 |
| Pooled chain edges | 16,570 → 16,570 | 82,387 → 82,387 |
| Physical record calls | 55,676 → 55,676 | 248,690 → 248,690 |
| Pooled physical group decompressions | 55,676 → 2,723 | 248,690 → 20,959 |
| Pooled decompressed bytes | 2,522,862,738 → 123,780,247 | 10,302,425,176 → 877,790,656 |
| Pooled physical cache hits | 0 → 52,953 | 0 → 227,731 |
| Ordinary group decompressions | **513 → 1,163** | **3,892 → 5,285** |
| Pooled value materializations | 65,337 → 65,337 | 368,074 → 368,074 |
| Pooled pack BLOB fetches | 79,784 → 79,784 | 427,384 → 427,384 |
| Pooled pack BLOB bytes | 7,378,994,999 → 7,378,994,999 | 23,399,127,004 → 23,399,127,004 |

Ordinary decompressions **increase by 650 / 1,393** because the shared bounded
cache has more competing groups. This cost is real and must remain visible.
The optimization removes physical group reconstruction work; it does not remove
pack acquisitions or pooled value decoding. Provider waves, requested/returned
objects, returned canonical bytes, failed-wave counts and connection-open counts
also match between arms. The provider intervals overlap the operation intervals
and cannot be added to them or interpreted as isolated codec CPU time.

All **17 + 53** state roots and saved work-counter maps match between arms.
Independent SHA-256 reads of all four Store files match their performance and
verification receipts, and each baseline/candidate pair is byte-identical:

| Artifact | SHA-256 |
| --- | --- |
| Both stride10 Stores | `ab64dab7aaed512ab93b7ccc46fd14f22e57791f642b39fdf2b94650a41a965f` |
| Both stride3 Stores | `cfb74bc9b1db6c9470129613283a8f4afa3b139c2819ae7bd7a4f14e548ecdb5` |
| Baseline executable | `a27b740fc7e92b61a600c9764b46ed3522c3ed229466c077daa2bad441b0f734` |
| Candidate executable | `0cfa938be11a61625da8fd4ef4306657860d6e9e548986983b9f9b3e34b375f8` |

The large-file hash pass acquired and released both shared TMPDIR and `/tmp`
measurement flocks plus the worktree harness lock in a quiet interval expressly
reserved by root after all eight runs. It did not overlap performance,
verification, builds or tests. Executable hashes match identity files; the
identity-file hashes match run receipts. Baseline and candidate harness-file
hash maps and lockfile hashes are identical. Only `encoding/pool/read.rs` and
`encoding/delta/read.rs` differ in the product identity maps between the measured
arms; the counters were already present in both.

## Verification and unresolved targets

The reviewer read raw phase files and complete-command receipts independently.
Every run completed, every verification trace reports **zero sampled
mismatches**, and the following distinction is preserved:

| Selection / arm | Internal verification ns | Complete verification command ns | Work target |
| --- | ---: | ---: | --- |
| Stride10 baseline | 6,185,391,917 | 6,241,705,625 | PASS ≤10 s |
| Stride10 candidate | 4,241,717,250 | 4,260,495,375 | PASS ≤10 s |
| Stride3 baseline | 20,766,917,666 | 20,803,914,708 | **TARGET_MISS >20 s** |
| Stride3 candidate | 14,987,183,125 | 15,003,333,792 | PASS ≤20 s |

All complete verification commands remain below the 60-second hard budget.
Complete performance command times are 33,741,919,208 / 30,502,824,583 ns
(stride10 baseline/candidate) and 74,713,341,541 / 61,657,848,334 ns (stride3),
within the prospectively declared 120/240-second diagnostic limits.

Every raw trace still contains `g1.o3-pinned-counters = INCOMPLETE` because the
history cases lack pinned counters. Receipts declare cache residency uncontrolled,
`admission_eligible = false`, and cold/performance status **INELIGIBLE**. A
verification work-target pass does not repair those admission gaps. Zero sampled
mismatches is the declared sampled proof, not an exhaustive byte-by-byte traversal
of all historical files.

The candidate measured operations **16.268082127 s / 44.591815082 s** remain above
the historical v0.1.6 Commit sums **11.370679212 s / 24.815 s**. The second
historical figure is rounded as recorded. Timer composition and historical cache
qualifications remain unresolved; this review does not close that tripwire.

Reported LOC receipts use the same production scopes: reference **65,417** in
both; instrumentation baseline core **20,250**, candidate core **20,306**;
combined **85,667 → 85,723 (+56)**. Relative to merged main's combined **85,582**,
the recorded instrumentation increment is **+85** and total increment **+141**.
The reviewer checked the receipt arithmetic, not a new exact-staged-tree counter
execution; any commit must still perform its required first-parent comparison.

Final verdict: the source preserves the bounded cache and required read checks,
and independent artifacts demonstrate the named reduction in actual decoding
work, with matching roots and Store bytes. Timing improvement is diagnostic;
ordinary cache competition, the baseline verification target miss, admission
gaps and the unmet historical tripwire remain explicitly reported.
