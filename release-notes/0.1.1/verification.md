# LayerFS 0.1.1 verification

> **Status:** Developer Preview source verification record for `v0.1.1`.

## Release identity

| Field | Result |
| --- | --- |
| Git tag | Annotated `v0.1.1` |
| Git commit/tree | The commit and tree resolved by `v0.1.1` |
| Workspace version | `0.1.1` |
| Clean source proof | Clean tagged checkout and deterministic archives |
| CI on exact commit | Required successful `ci` workflow check |
| Source archive verification | Release asset `SHA256SUMS` |

## Mandatory terminal commands

The following commands were required from the release source:

```bash
cargo fmt --all -- --check
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
tools/test-fast.sh
bash -n benchmark/fs-bench-pro/run.sh
bash -n benchmark/fs-bench-pro/run-namespace.sh
benchmark/fs-bench-pro/run.sh --self-check
benchmark/fs-bench-pro/run-namespace.sh --self-check
git diff --check
```

Terminal verification must also include:

- released 0.1.0 Store opening on the 0.1.1 release;
- canonical fixture and reachable-root compatibility;
- exact direct-path eligibility and canonical fallback coverage;
- real Linux FUSE/materialization equality;
- managed-container lifecycle, attachment failure, disconnect, and cleanup;
- fresh-process namespace-v2 samples at all four tiers;
- the registered LayerFS payload campaign; and
- local documentation links and artifact checksums.

## Terminal benchmark proof

The terminal namespace campaign completed every sample and passed every
setup, product, verification, performance, evidence, resource, correctness,
cleanup, quality, sample-shape, normal-overwrite, and composite gate. Its
source seal is
`c17e554ac21d53fd168a70bc492bf5342eb5a90a6f7a9c067f81c4148976cd7c`.

The terminal payload campaign passes every frozen performance and resource
gate at source seal
`dd219ed9e7942a42891ff14646ee3c54a4580e6aaeeee7a25a01b30d1453a805`.
See [benchmark-results.md](benchmark-results.md) for retained paths, exact
checksums, medians, and the bounded read-ahead tradeoff.

## Acceptance record

| Gate | Status |
| --- | --- |
| Clean immutable source | Pass; bound by annotated tag and release archives |
| Version and lockfile | Pass; every local package resolves to `0.1.1` |
| Native workspace checks | Pass in namespace composite proof |
| Linux FUSE/Docker checks | Pass in namespace composite proof |
| Namespace performance/evidence | Pass on terminal namespace seal |
| Registered payload regression | All frozen gates pass on terminal payload seal |
| Documentation audit | Pass; release links and whitespace verified |
| Artifacts/checksums | Published with the GitHub Release and verified by `SHA256SUMS` |
