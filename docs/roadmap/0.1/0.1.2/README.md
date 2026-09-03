# LayerFS 0.1.2

> **Status:** Current planning checklist; no release candidate exists.

LayerFS 0.1.1 is the active released Developer Preview. LayerFS 0.1.2 is
unreleased and publication-blocked by
[issue #20](https://github.com/Ephemeral-AI-Lab/layerfs/issues/20). Parent issue
[#12](https://github.com/Ephemeral-AI-Lab/layerfs/issues/12) remains open.

## Ordered work

| Issue | Disposition |
| --- | --- |
| #17 family-local harness format | complete supporting work |
| #14 universal regular-file edit engine | complete supporting work |
| #13 prior same-count POSIX family | closed; archival evidence only |
| #15 prior count-changing POSIX family | closed; archival evidence only |
| #16 Store-footprint evidence | retained supporting evidence |
| #19 earlier rebuild draft | closed as superseded |
| #20 SDK-only edit benchmark rebuild | open release blocker |

## Active edit registry

The binding specification is
[SDK-only file-edit benchmark rebuild](sdk-only-edit-benchmark-rebuild.md).

| Family | Definition | Runner | IDs |
| --- | --- | --- | ---: |
| `edit_length_preserving` | `families/edit_length_preserving.rs` | `run-edit-length-preserving.sh` | 12 |
| `edit_length_changing` | `families/edit_length_changing.rs` | `run-edit-length-changing.sh` | 32 |
| `edit_canonical_chunk_count` | `families/edit_canonical_chunk_count.rs` | `run-edit-canonical-chunk-count.sh` | 12 |
| **Total** | — | — | **56** |

Every operation/outcome has exact 1/10/100/500 MiB siblings. Every registered
row performs one logical edit, one `WorkspaceFileRangeEdit`, one singular public
SDK call, one Commit, and one End. Batch and composite sparse performance are
outside issue #20.

The terminal directional campaign contains 280 baseline and 280 candidate
performance rows in the frozen alternating order, plus 56 aggregate verifier
receipts with 112 source-arm subproofs. Strict latency, parity,
no-amplification, phase-local process/cgroup memory, verification, cleanup, and
custody gates must all pass on one exact clean candidate.

## Historical disposition

[Same-count edits](same-count-file-edits.md) and
[count-changing edits](count-changing-file-edits.md) document the superseded
POSIX/FUSE families. Their raw evidence is immutable but cannot serve as an
active member, baseline, paired arm, or release claim.

The universal engine and Store evidence remain supporting work. Completion of
#20 makes #12 eligible for a later release-finalization step; it does not tag,
publish, or close #12. Empirical edit claims stop at 500 MiB, with no 100 GiB
synthetic or extrapolated claim.

## Completion

- [x] Benchmark policy and issue #20 specification frozen before code changes.
- [ ] Three SDK-only family definitions and runners implement exactly 56 IDs.
- [ ] 560 terminal performance rows and all 56 verifier receipts pass.
- [ ] All repository and evidence-custody gates pass on the exact clean candidate.
- [ ] Issue #20 is closed with exact evidence; parent #12 remains open.
