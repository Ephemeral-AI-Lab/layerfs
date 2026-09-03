# LayerFS 0.1.2 verification

> **Status:** Draft verification record. Exact-candidate benchmark evidence is
> complete; final documentation, native CI, tag, archives, and GitHub Release
> checks remain pending.

## Exact measured identity

| Field | Value |
| --- | --- |
| Commit | `c6c14d5a5a740665f5efbce439493f681bd7dd95` |
| Tree | `7c8b843c354fa49f4afa344d66c358a776bfd0d0` |
| Source seal | `6b3c039e4237a8ab27eebc5ea4752bc8ad9f58039725ac9b2e3230119b171ec9` |
| Product seal | `438253c10b6b33ae33e6b81113390f0d06d5b98fb2c0fc6c0e0438e0d483431f` |
| Harness seal | `4c68f918828036082c7110e28bfb2a2e88983d46d404fc1de3899335ad15694c` |
| Workload SHA-256 | `c07029d3bf95c187ded2899f3e6840449301a1495c8a51fc694fbbca63fbf6d9` |
| Candidate image | Clean commit/tree/source labels; no bind mount |
| Frozen count baseline | Same commit/tree/product; frozen workload and distinct source seal |

The measured commit is the final code/harness candidate. Release-only generated
documentation and artifact metadata may advance the eventual tag commit without
changing these seals; final native checks must run again on the tagged tree.

## Sealed benchmark and conformance evidence

| Evidence | Raw rows / receipts | Status | Manifest SHA-256 |
| --- | ---: | --- | --- |
| Universal edit-engine conformance | 36 Workspace + 11 file-edit + 2 reconciliation + seed/diagnostic/Clippy + 3 real-FUSE | pass | `deca3578ce3aabbad6ff61c41c5d42297e6d8f02fbd699a4b523194193b2aa4b` |
| Owner-side timing supplement | 9 measurements | pass | `0494d0d9c33ea79e488b3078e18714e86b17995df27e5123c11ecc285861f9e3` |
| Same-count | 84 performance + 6 proofs + 1 timing/status | target-pass | `07a17444ac938abbe27d3955fd6cb3eeca92f2a87ca10770a61777608e06cc05` |
| Count-changing | 150 primary + 45 controls + 18 scaling + 7 primary verification + 18 scaling verification | tolerated-pass | `491da0d15babd56b38eef00e85f282f318e0f44a847ee5a0a7b289733d979e97` |
| Store footprint | 9 performance + 3 verification | baseline complete; primary footprint no-go | `7907b11fa3db15cca13fda6a99a949c3ee0b984cb743270ba182cc0ef586271b` |

The owner timing receipt includes nine exact command records, bundled fixtures,
and a complete nested conformance bundle. Same-count and count-changing receipts
also carry a self-verifying nested #14 custody directory. Every final top-level
manifest and nested conformance manifest has been independently rehashed.

The third real-FUSE proof covers the direct-I/O create policy: same-handle and
concurrent-handle reads observe exact writes, close/reopen is exact, mmap on the
still-open create handle returns `ENODEV`, and read-only mmap succeeds after an
ordinary reopen.

## Binding benchmark decisions

- Same-count gates the symmetric aggregate identical-source A/A wall, not noisy
  per-case A/A ratios. Its exact ratio is `1.004258171`, target-pass.
- Count-changing gates every primary scenario directionally. Its worst
  candidate/baseline ratio is `1.096620770`, tolerated but below the `1.10`
  no-go boundary. Every absolute gate is target-pass.
- Every candidate sample in the 256 KiB count-changing temp-copy cohort has a
  batch-average mutation time below 10 ms/op. The admission gate is
  `median(inner_edit_ns) <= operation_count * 10,000,000 ns`, with no tolerance
  band; individual operations are not timed separately.
- The scaling 100 MiB/10 MiB copied-rate ratios are `1.257938569` for delete
  and `1.205767997` for shrink, both above the `0.90` floor. Scaling is
  candidate-only and does not claim CDC/ObjectId generalization.
- Store primary durable footprint is `662,831,104` median bytes, exactly
  `62,831,104` above the `600,000,000`-byte goal. This remains an explicit
  blocker; physical packs stay deferred to issue #18.
- Store metadata verification takes `63.356` seconds in its verification phase,
  tolerated by the 60/66-second phase policy. Its `69.756`-second external wall
  is not compared with that phase gate.

The complete tables, ranges, unit rules, timing boundaries, family walls, and
historical no-go separation are generated in
[benchmark-results.md](benchmark-results.md).

## Final native and publication commands

Before tagging, the final source must pass:

```bash
cargo fmt --all -- --check
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
tools/test-fast.sh

bash -n benchmark/fs-bench-pro/run.sh
bash -n benchmark/fs-bench-pro/run-namespace.sh
bash -n benchmark/fs-bench-pro/run-edit-same-count.sh
bash -n benchmark/fs-bench-pro/run-edit-count-changing.sh
bash -n benchmark/fs-bench-pro/run-store-footprint.sh

benchmark/fs-bench-pro/run.sh --self-check
benchmark/fs-bench-pro/run-namespace.sh --self-check
benchmark/fs-bench-pro/run-edit-same-count.sh --self-check
benchmark/fs-bench-pro/run-edit-count-changing.sh --self-check
benchmark/fs-bench-pro/run-store-footprint.sh --self-check

python3 release-notes/0.1.2/generate_benchmark_tables.py --check --verify-all
git diff --check
```

## Current acceptance record

| Gate | Status |
| --- | --- |
| Public/API/Store compatibility | Pass in exact-candidate conformance |
| Direct-I/O create/reopen/mmap policy | Pass in real Linux FUSE |
| Same-count family | Target-pass; exact nested custody |
| Count-changing family | Tolerated-pass; all absolute/scaling/verifier gates pass |
| Store footprint | Exact baseline complete; primary 600 MB goal remains no-go |
| Benchmark table regeneration | Pass; independently recalculated from sealed raw data |
| Final native/CI checks | Pending on the documentation-complete tree |
| Artifacts/checksums | Pending; no GitHub Release or `v0.1.2` tag is published |
