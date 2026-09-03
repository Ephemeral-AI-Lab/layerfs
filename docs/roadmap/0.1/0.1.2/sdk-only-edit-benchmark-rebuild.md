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
in memory. The preparation manifest freezes each tier's SHA-256, initialized
Layer/Branch root, canonical file root, extent count, and Store identity before
performance collection.

### Replacement profile

All ordinary 4 KiB replacement bytes are generated before timing from a
separate SplitMix64 stream seeded by the family, scenario, and operation index.
The definition module freezes the domain encoding and edit-plan digest. The
five performance repetitions use the same plan; repetition changes run order,
not benchmark semantics. Baseline, candidate, and verifier consume the exact
same byte-identical plan.

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
range `[491,520, 557,056)`—64 KiB centered at 512 KiB. Because all larger
fixtures extend the 1 MiB prefix, the base bytes and local edit position are
identical at every tier. A 64 KiB replacement exceeds the frozen 32 KiB maximum
CDC chunk and exercises multiple replacement chunks.

### Outcomes and membership

Prepare and freeze exactly three deterministic 64 KiB replacement payloads:

```text
chunk-count-preserve
chunk-count-increase
chunk-count-decrease
```

Each payload is byte-identical at all four file-size tiers and has one frozen
digest. Each outcome must verify both its exact frozen `(C0, C1)` pair at every
tier and its declared relationship:

```text
preserve: C1 == C0
increase: C1 > C0
decrease: C1 < C0
```

A sign-only result is insufficient. Preparation freezes replacement digest,
base/final extent counts, exact delta, final file digest, canonical file root,
and ordered chunk-map digest before performance collection. Candidate output
must never be used to retroactively choose or rename the outcome.

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
member. It must satisfy all three hard median ceilings independently:

```text
edit_call_ns   <= 10 ms
commit_call_ns <= 10 ms
edit_commit_ns <= 20 ms
```

There is no tolerance band on these absolute ceilings. A row fails if any one
metric exceeds its ceiling, even when the other two or their sum pass.

Every sample additionally requires:

```text
logical_operation_count == 1
sdk_edit_member_count == 1
public_sdk_edit_call_count == 1
```

### File-size parity

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

If the required phase/sample scope or attribution is unavailable, the row is
admission-ineligible. Do not substitute a campaign-lifetime peak, subtract
process RSS from cgroup memory, or classify physical spool disk as RSS.

The evidence claim stops at the measured 500 MiB tier. Complexity analysis may
explain the absence of a term proportional to untouched bytes, but it is not a
measurement above 500 MiB.

## Performance and verification separation

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

run-edit-*.sh RUN_ID CONTAINER --case ID --repetition 1 --mode performance
    exactly one final-source row; no verifier; admission_eligible=false

run-edit-*.sh RUN_ID CONTAINER --case ID --mode verify
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
