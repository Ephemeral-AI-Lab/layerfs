# LayerFS 0.1.2 benchmark blocker record

> **Status:** Unreleased; active edit admission is incomplete and publication
> is blocked by issue #20.

## Active edit evidence required

The authoritative contract is
[`sdk-only-edit-benchmark-rebuild.md`](../../docs/roadmap/0.1/0.1.2/sdk-only-edit-benchmark-rebuild.md).

| Family | IDs | Exact tiers | Terminal performance rows |
| --- | ---: | --- | ---: |
| `edit_length_preserving` | 12 | 1/10/100/500 MiB | 120 |
| `edit_length_changing` | 32 | 1/10/100/500 MiB | 320 |
| `edit_canonical_chunk_count` | 12 | 1/10/100/500 MiB | 120 |
| **Total** | **56** | — | **560** |

Each row is exactly one `WorkspaceFileRangeEdit`, one singular
`Client::edit_workspace_file_range` call, one Commit, and one End. The terminal
campaign has five repetitions in each baseline/candidate arm. Separate
verification requires 56 aggregate receipts containing 112 source-arm
subproofs.

No active results exist yet. Admission requires strict 10/10/20 ms median
Edit/Commit/Edit-plus-Commit gates; size and matched-operation parity; zero
Exec, mutation-caused FUSE payload, and Workspace spool; bounded CDC/candidate
work; phase-local process RSS and daemon-native cgroup coverage; exact
verification; cleanup; and source custody. A memory failure independently
blocks release.

## Historical evidence disposition

The prior same-count and count-changing evidence at candidate `c6c14d5a`
measured container POSIX/FUSE operations, including temp-copy/fsync/rename. It
is immutable archival evidence and may be reproduced, but it is not an active
member, baseline, comparator, paired arm, admission result, or v0.1.2 claim.
Its former pass/tolerated-pass labels do not satisfy issue #20.

The universal edit-engine conformance and Store-footprint records remain
supporting historical evidence. They do not replace the three complete active
families.

Empirical edit claims stop at 500 MiB. No synthetic, measured, or extrapolated
100 GiB result is permitted.

The report generator remains bound to the archival evidence until issue #20
produces sealed raw JSONL. It must be rebuilt from that new raw evidence before
this document can become a final candidate report.
