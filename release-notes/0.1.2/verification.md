# LayerFS 0.1.2 verification

> **Status:** Developer Preview source verification record for `v0.1.2`.

## Release identity

| Field | Result |
|---|---|
| Git tag | Annotated `v0.1.2` |
| Git commit/tree | The commit and tree resolved by `v0.1.2^{commit}` |
| Workspace version | `0.1.2` for all eleven local packages |
| Clean source proof | Clean tagged checkout and deterministic archives |
| CI on exact commit | Required successful `ci` workflow check |
| Source archive verification | Release asset `SHA256SUMS` |

## Mandatory terminal commands

The release source must pass:

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

cargo test -p layerfs-layerstack-store \
  layerstack::tests::benchmark_initialization_seed_is_exact_lower_hex \
  -- --exact --nocapture
git diff --check
```

The diagnostic-seed test proves lowercase decoding and exact length, rejects
uppercase/short input, rejects an override without the diagnostic gate, and
retains ordinary LayerStackId-derived behavior when no override exists.

Final issue #14 conformance additionally runs the 36-test Workspace unit suite,
all 11 tests spanning its seven file-edit groups, both reconciliation tests,
scoped warning-denying Clippy for Store/Workspace/SDK/harness, and both
established real Linux FUSE tests. It verifies the compatibility repair:
v0.1.1 `WorkspaceCommitResult`, `WorkspaceState`, `OperationFamily`, and
`WorkspaceCommitReceipt` shapes remain exact, while edit diagnostics and
detailed presentation status are additive types.

Final conformance directory:
`benchmark-results/fs-bench-pro/edit-engine-acceptance/final-v012-issue14-19af57ef`.
Its manifest SHA-256 is
`9e18afc5ccafba5434b10044b9dec0a79842b51234513ad9ef3f178e08564f4e`.
The measured release-candidate identity is source seal
`14842002c48af00e38061529d835b55c447c18cd46fbcefd7f5bbb34a88e703a`,
product seal
`7559be73d672b9922ad7913e70f8afe0cd21a06ca3f18a90215fc7be4adfd924`,
harness seal
`6bb76a1968f7c0217e10324f1285b951161be73547048c6ba08b8f1fe272e88d`,
and workload SHA-256
`a2b39fb7b4773c97423760e3d1daa538ea759af3c915decd7031c272cabcb62e`.
The final custody bridge is
`benchmark-results/fs-bench-pro/v012-release/final-custody-19af57ef`, manifest
`516b436eca0b73f30bc3d15cfd6f93eb0308938ea4124be5582d04efb3c8473d`.

## Retained benchmark proof

The authoritative paths, formulas, counts, medians, and manifest hashes are in
[benchmark-results.md](benchmark-results.md). The binding decisions are:

- Same-count: 84 rows, seven separate fragmentation receipts, target-pass
  symmetric aggregate identical-source A/A ratio `1.027103819`, exact anchor
  custody, zero swap/OOM, and cleanup pass.
- Count-changing: the 150-row directional issue #15 evidence plus 45 controls
  and seven verifier receipts is authoritative. Maximum ratio-of-medians is
  `1.0746733575`; all three results above `1.05` have complete dispositions.
- Final count-changing A/A: all 150 performance rows are sealed, but the run is
  diagnostic/no-go because five absolute throughput medians miss. It has no
  verifier and never replaces issue #15.
- Store: nine baseline performance Stores and three exact verifier Stores pass
  custody/resource gates. The final metadata supplement separately verifies
  content, file and directory metadata, roots, reconnect, census, and cleanup.
- Store blocker: the owner accepts `661,061,632` bytes at the retained
  ObjectId/SQLite layout as the exact patch-compatible result. The
  `562,513,789`-byte physical-pack number is a conservative incompatible
  object-storage lower bound deferred to open issue #18.

Identical-source family repeatability gates the owner-approved symmetric
aggregate arm wall only. Directional baseline/candidate comparison continues
to gate every member. Performance and verification streams remain separate.

## Acceptance record

| Gate | Status |
|---|---|
| Public/API/Store compatibility | Pass; existing exhaustive public shapes and v0.1.x Store format retained |
| Version and lockfile | Pass; every local package resolves to `0.1.2` |
| Native workspace checks | Pass on the release source |
| Linux FUSE/Docker checks | Pass on the final production seal |
| Same-count family | Pass; accepted aggregate A/A rule and separate verifier |
| Count-changing family | Pass; authoritative directional evidence; final A/A explicitly diagnostic/no-go |
| Store footprint | Accepted exact blocker; complete census and final metadata verifier |
| Documentation audit | Pass; active manual, roadmap, registry, limitations, and local links agree |
| Artifacts/checksums | Published with the GitHub Release and verified by `SHA256SUMS` |
