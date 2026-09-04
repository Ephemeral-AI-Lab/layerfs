# PR title

`bench: prototype deterministic multiscale history anchors`

# PR body

## Summary

- add an isolated research benchmark for immutable Commit-history depth;
- compare the current parent walk, fixed 10-Commit checkpoints, and one
  deterministic multiscale anchor per eligible Commit;
- preserve two independent raw runs plus a dependency-free evidence verifier;
- change no Store schema, canonical identity, public API, product path, or
  v0.1.4 benchmark registration/evidence.

Author: **Wang Runyuan**

## Motivation

LayerFS already supports direct lookup of a known Commit and direct Merkle-root
content Diff. The remaining question is narrower: Branch membership validation
currently walks `parent_commit_id`, and `DiffRequest::BranchCommits` performs
that validation before content Diff. This prototype measures whether a tiny,
derived cross-scale sidecar can reduce that planning work as history grows.

This is inspired by established skip indexing, Git commit-graph, and Merkle DAG
lineage precedents. Spectral sparsification is only a design analogy; this PR
makes no LayerFS theorem claim.

## Design

For Commit ordinal `i`, the multiscale sidecar stores one ancestor at distance
`lowbit(i)` when the distance exceeds one. The rule is deterministic,
non-adaptive, and rebuildable from canonical history. A fixed checkpoint route
stores one 10-Commit jump at eligible positions.

The sidecars are in memory only. They are not authoritative and never enter the
Store. Deleting them leaves the original history fully usable.

## Evidence

Two formal runs used identical deterministic fixtures at depths 1, 10, 100, and
1000. Every result matched the current public operations exactly, and the Store
database bytes, counts, canonical storage, and Commit IDs remained unchanged.

| Depth | Route | Distant Diff nodes | Median latency across runs | Metadata |
| ---: | --- | ---: | ---: | ---: |
| 100 | baseline | 101 | 182.3–185.0 us | 0 B |
| 100 | fixed-10 | 20 | 109.0–109.2 us | 4,469 B |
| 100 | multiscale | 6 | 108.8–110.1 us | 6,109 B |
| 1000 | baseline | 1001 | 651.9–684.9 us | 0 B |
| 1000 | fixed-10 | 110 | 110.1 us | 45,059 B |
| 1000 | multiscale | 10 | 109.7–109.9 us | 61,459 B |

At depth 1000, the compact logical metadata budget is 0.957% of Store bytes.
Direct known-Commit lookup already visits zero parent nodes and is not improved.
Returning complete history is also still linear. The measured benefit is limited
to ancestry validation and Branch-Commit Diff planning.

## Validation

- `cargo fmt --check`
- `cargo test` — 4/4 prototype tests pass
- `cargo clippy -- -D warnings`
- two independent release-mode benchmark runs
- `python3 verify.py --output results/analysis.json` — all gates pass
- patch applied to a clean worktree at baseline `1e81e9b`
- applied tree matched the source tree
- no new runtime dependency

## Scope boundaries

This PR does not propose a production persistence format or threshold, modify
the formal v0.1.4 benchmark contract, optimize direct historical reads, add
changed-path Bloom filters, or claim that sparse anchors are universally better.
It supplies a small reproducible experiment for deciding whether a later Store
index design is worth discussing.
