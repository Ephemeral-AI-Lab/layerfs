# Disjoint-edit stale publication proof

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Parent product commit: `f2021367e11f6433d1d9e055198cac5ad280f6c9`.
This follow-up adds an external service regression only; production source and
runtime SQL are unchanged. Two stages independently change different files from
one Branch snapshot. After the first Commit, the second native CommitStaged
returns exact BranchMoved and its retained stage. Readback proves the second
file stayed unchanged; there is no implicit merge. Combined with the overlapping
edit and multiple no-change tests, this covers H05's distinct semantic cases.

Focused command (repository root, own CARGO_TARGET_DIR):
`cargo +1.85.1 test --manifest-path core/Cargo.toml --offline --locked -p layerfs-service --test history disjoint_file_edits_still_refuse_stale_publication_without_merging`
passed. The later validation record gives the committed-head full workspace check.

Production LOC: 96344 -> 96344 (delta +0).
Core 30927 -> 30927 (+0); reference 65417 -> 65417 (+0).
Same exact-snapshot counter and scope as the implementation record:
`python3 tools/production_loc.py --root <export> --detail --json`, using first
parent/final staged trees and excluding tests, docs, tooling and legacy inline
tests. Counter SHA-256
`c6e853c2280e2ee96caa6cafff4b221fa9201e1f5bf40d12e8251f70387210fc`.
