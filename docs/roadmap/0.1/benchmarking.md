# LayerFS 0.1.x benchmark contract

> **Status:** Current maintainer contract for 0.1.x evidence; not a released
> benchmark result or product contract.

All benchmark work follows the repository-wide
[benchmark rules](../../general/benchmark_rules.md). Historical evidence is
append-only: a corrected family receives a new identity and never relabels or
pools old rows.

## Release sequence

| Release | Benchmark state |
| --- | --- |
| v0.1.0 | Frozen 32 MiB payload lifecycle |
| v0.1.1 | Active released Developer Preview; namespace admission history |
| v0.1.2 | Unreleased; blocked by the SDK-only edit rebuild in issue #20 |
| v0.1.3 | Draft single-history workload families |
| v0.1.4 | Draft multi-Layer and multi-Branch history families |

## Active v0.1.2 edit admission

The canonical contract is the
[SDK-only rebuild specification](0.1.2/sdk-only-edit-benchmark-rebuild.md).
Exactly three active families contain 56 singular public-SDK edit scenarios:

| Family | IDs | Exact size tiers |
| --- | ---: | --- |
| `edit_length_preserving` | 12 | 1/10/100/500 MiB |
| `edit_length_changing` | 32 | 1/10/100/500 MiB |
| `edit_canonical_chunk_count` | 12 | 1/10/100/500 MiB |

Every row performs exactly one `WorkspaceFileRangeEdit`, one
`Client::edit_workspace_file_range` call, one Commit, and one End. No active
row calls the batch API or mutates through Exec, POSIX/FUSE writes,
temp-copy/rename, Store internals, or another editor. A real FUSE projection
remains attached and edit-caused FUSE payload and Workspace spool traffic are
both exactly zero.

Five repetitions per ID and two source arms produce 560 terminal performance
rows. Separate verification produces 56 aggregate receipts containing 112
source-arm subproofs. The absolute latency gates are strict medians of at most
10 ms for Edit, 10 ms for Commit, and 20 ms for Edit plus Commit. File-size,
matched-operation, no-amplification, process RSS, daemon-native cgroup, cleanup,
and custody gates are independently release-blocking.

Empirical claims stop at 500 MiB. No synthetic, measured, or extrapolated
100 GiB claim is permitted.

## Historical edit evidence

The prior `edit_same_count` and `edit_count_changing` definitions, runners, and
results measured POSIX/FUSE mutations, including temp-copy/rename. They remain
immutable reproducibility evidence only. They are not active members,
comparators, baselines, paired arms, or v0.1.2 release claims.

Selected development runs are always admission-ineligible. Complete family
execution requires explicit `--all`; a shared-path change requires one final
rerun of every affected member and verifier on the exact clean candidate.

## Existing registered lanes

The frozen v0.1.0 payload runner and v0.1.1 namespace runner keep their original
IDs and meanings. They are not silently redefined by issue #20. Later releases
rerun admitted earlier rows without changing their operation, fixture, timing,
oracle, or schema identity.
