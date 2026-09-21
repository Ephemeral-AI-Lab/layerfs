# Attribute hierarchy prerequisite for R2

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> Implemented from `acb4558e3d6ca2fe0d2696d9865239c30dc201ee`, 2026-09-21.

R2 must update portable mode/mtime through the shared C1/C2 owners while
preserving opaque attributes. Reviewing that existing patch route found two
related defects. Grouped reads and the streaming patch/key cursor discarded the
parent level, so they accepted a child with a skipped or repeated level. Adding
the missing check exposed the builder's finalization error: its pending level
index already denotes the emitted branch level, but finalization added one
again. Streaming flush used the correct level.

The builder now uses that index directly. Both read routes share the existing
size/fill/maximum-key validation together with an exact child-level check.
Valid pages remain at most 8,192 bytes and tree level at most 31; each accepted
descent decreases the level by one. The patch cursor retains one leaf and
pending siblings by level, O(page entries × tree height), rather than a complete
attribute map. The 4,096-key limit belongs to `FilesystemRead::attribute_keys`;
it is not an undocumented total-tree limit on the generic streaming patch API.

Changed product files are `attributes/build.rs`, `attributes/read.rs` and
`attributes/patch.rs` in `layerfs-content`. External regressions are in
`tests/filesystem_attributes.rs`; [architecture 12](../../12-attributes.md)
records the algorithm and compatibility consequence.

**Compatibility:** the canonical grammar is unchanged, but the old core builder
could emit malformed wide trees. Corrected builds can therefore produce
different roots, and strict readers reject the malformed old hierarchy. No
migration, fallback or historical identity/receipt relabelling is introduced.
The existing sealed attribute goldens are leaf trees and remain unchanged.
This is a required correctness fix; no closed Stage 5 acceptance row is reopened.

## Verification and retained failures

The first focused test attempt retained two failures: the new multi-level test
observed level 4 where level 2 was required, and the existing 4,097-key fixture
hit the new hierarchy refusal before its expected operation capacity refusal.
Those failures led to the production builder fix; the existing fixture was not
rewritten to hide the defect. The corrected focused suite passes all 12 tests,
including grouped reads, complete key visitation, wide patches preserving 999
untouched value roots, malformed hierarchy refusal and the original key bound.

From this implementation worktree, using the repository ARM build configuration,
`CARGO_TARGET_DIR=$PWD/core/target` and `LAYERFS_CONSTRUCTION_WORKERS=1`:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked -p layerfs-content --all-targets -- -D warnings
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

All content tests, including existing canonical comparisons, pass. The first
Clippy attempt rejected a complex local tuple type; a private lookup-demand
alias addresses it without changing behavior. The repeated Clippy check passes.
Changed files are formatted and the guard plus six self-tests pass. Logs,
including both failed attempts, are retained in
[the prerequisite evidence](evidence/r2-prerequisite-20260921/).
Whole-workspace checks follow the R2 integration; this prerequisite does not
claim a mounted metadata operation, completed R2, performance or durability.

Next dependency: implement the single authorized typed metadata-save operation
through the existing bridge/service save boundary, then verify the native route.

## Production source comparison

Production LOC: **99,288 -> 99,276 (delta -12)**. Reference: 65,417 unchanged;
replacement core: 33,871 -> 33,859 (-12). Shared validation removes duplicated
checks; no legacy source is retired. The same `tools/production_loc.py`
(blob `b5b9617d08204977176302311e0b2c72a811b420`) counts first-parent and staged
Git archives of `crates` and `core/crates`, including runtime SQL and excluding
tests, examples, fixtures, tools, docs, manifests, comments and blank lines.
The commit message records the exact parent and final staged tree.
