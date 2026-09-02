# LayerFS 0.1.1 release-candidate verification

> **Status:** In progress. Results below are development evidence, not terminal
> verification of a tagged release.

## Release identity

| Field | Result |
| --- | --- |
| Git tag | Pending |
| Git commit/tree | Pending clean candidate |
| Workspace version | `0.1.1` complete |
| Clean source proof | Pending |
| CI on exact commit | Pending |
| Source archive verification | Pending |

## Mandatory terminal commands

Run from the clean candidate source and retain complete output:

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

- released 0.1.0 Store opening on the candidate;
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

## Pending acceptance record

| Gate | Status |
| --- | --- |
| Clean immutable source | Pending |
| Version and lockfile | Pending |
| Native workspace checks | Pass in namespace composite proof |
| Linux FUSE/Docker checks | Pass in namespace composite proof |
| Namespace performance/evidence | Pass on terminal namespace seal |
| Registered payload regression | All frozen gates pass on terminal payload seal |
| Documentation audit | Candidate links and whitespace pass; release-commit audit pending |
| Artifacts/checksums | Pending |
