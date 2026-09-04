# LayerFS 0.1.2

> **Status:** Completed v0.1.2 release checklist; source-bound evidence linked below.

LayerFS 0.1.2 includes the completed
[issue #20](https://github.com/Ephemeral-AI-Lab/layerfs/issues/20) SDK campaign
and the [release refresh](release-refresh.md) under parent
[#12](https://github.com/Ephemeral-AI-Lab/layerfs/issues/12).

The [diagram-led architecture history](architecture_shift.md) traces the
pre-v0.1 foundations through v0.1.2, including edit/Commit time and space
complexity, storage authority changes, and measured optimization boundaries.
For a focused visual explanation, see
[Extent trees: before/after and complexity](extent_tree.md).
The publication-ready [X Article editions](x-article/) contain English and
Simplified Chinese articles, localized diagrams, benchmark charts, Big-O tables,
and the accompanying long-form reply sequences.

## Ordered work

| Issue | Disposition |
| --- | --- |
| #17 family-local harness format | complete supporting work |
| #14 universal regular-file edit engine | complete supporting work |
| #13 prior same-count POSIX family | closed; archival evidence only |
| #15 prior count-changing POSIX family | closed; archival evidence only |
| #16 Store-footprint evidence | retained supporting evidence |
| #19 earlier rebuild draft | closed as superseded |
| #20 SDK-only edit benchmark rebuild | 560 samples and 112 proofs; final admission recorded by evidence selector |

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
receipts with 112 passing source-arm subproofs. The measured candidate is
`3337728e9846a200d7a5cc08d076de18f1d5436c`, with baseline
`dc7aeff9a7e4f9e849a48022142f86801273f0bd`. Both use the identical harness.

The [full timing and memory tables](../../../../release-notes/0.1.2/sdk-edit-benchmark-results.md)
retain nominal versus tolerance-only results, three approved Edit-parity
exceptions, and all original strict findings. Accepted median ceilings are
20/20/30 ms; nominal targets are 10/10/20 ms. Commit spreads are diagnostic,
not proof of size-independent Commit. Memory uses approved `ack-window-v1`
observations and native lifetime bounds, not exact-phase proof.

## Historical disposition

[Same-count edits](same-count-file-edits.md) and
[count-changing edits](count-changing-file-edits.md) document the superseded
POSIX/FUSE families. Their raw evidence is immutable but cannot serve as an
active member, baseline, paired arm, or release claim.

The namespace and Store supporting families have fresh release-source
measurements and separate passing verification, recorded in the
[supporting report](../../../../release-notes/0.1.2/supporting-benchmarks.md).
Empirical edit claims stop at 500 MiB, with no 100 GiB synthetic or extrapolated
claim. #18 remains far-future unscheduled storage-alternative exploration.

## Completion

- [x] Benchmark policy and issue #20 specification frozen before code changes.
- [x] Three SDK-only family definitions and runners implement exactly 56 IDs.
- [x] 560 terminal performance rows and all 56 verifier receipts pass the approved final policy.
- Final repository-gate status and checked source bridge: [evidence selector](../../../../release-notes/0.1.2/sdk-edit-evidence.json).
- #20 is closed. Parent #12 closes after final release verification, push and publication.
