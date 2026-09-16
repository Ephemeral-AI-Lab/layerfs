# v0.1.6 benchmark closeout — every measured selection

> **Status:** LayerFS 0.1.6 release record. Generated report-only from the two
> committed seed-1 matrices by [`generate_closeout.py`](generate_closeout.py);
> **no benchmark or proof was run to write this file.** Every cell is copied from
> [benchmark-performance.csv](benchmark-performance.csv) and
> [benchmark-verification.csv](benchmark-verification.csv), which are themselves
> derived from the receipts: dispositions are never recomputed, `N/A` never
> becomes zero, and a declared exception is printed with its measured wall.

## What was measured

| Quantity | Value |
|---|---:|
| Registered rows in this release record | 36 (33 regular + 3 extended) |
| Rows with a performance receipt | 28 |
| Rows verify-only by declaration | 8 |
| Rows with an independent verification | 36 — every one `PASS` |
| Performance gate `PASS` | 25 |
| Performance gate `EXCEPTION` (above the 15 s family target, inside the declared allowance) | 3 |
| Verification gate `PASS` | 33 |
| Verification gate `EXCEPTION` | 3 |
| Cleanup `FAIL` | 0 |
| Mode-level results above the 15 s family target | 8 |
| Rows carrying an `EXCEPTION` gate in either mode | 3 |

Seed 1, one sample per case and mode, repetition 1, `--setup clone` for every
post-initialization case, the same image and the same sealed producer for both
modes. All 28 performance receipts reused the closed prepared master
(`cache_hit: true`, `clone_method: closed-quiescent-byte-copy`; preparation
0.424–2.563 s, outside every timer). Cleanup ran 0.338–0.864 s and passed in all
64 driver records.

## Declared allowances

The 15 s family target is reported for every row and is **not** redefined by any
allowance below.

| Scope | Allowance | Source |
|---|---:|---|
| Regular v0.1.6 performance invocation (complete command) | 60 s | owner ruling, ledger L10 |
| Regular v0.1.6 verification (complete command) | 25 s (22 s worker stop + 3 s cleanup) | frozen `cases.json`; ledger L10 |
| `v016-branch-mixed-500mb-30000-k100-v1` verification | 30 s | owner ruling on #154, ledger L12 |
| Extended `v016-workspace-four-100mb-5000-k100-v1` | 60 s | frozen `V016_EXTENDED_WATCHDOGS` |
| Extended `v016-mixed-exhaustive-100mb-5000-k100-v1` | 120 s | frozen `V016_EXTENDED_WATCHDOGS` |
| Extended `v016-mixed-exhaustive-500mb-30000-k100-v1` | 300 s | frozen `V016_EXTENDED_WATCHDOGS` |

Each row's own column prints the allowance its receipt declared and enforced, so a
row can never be read against a ceiling its receipt did not carry. The 30 s and
the three extended watchdogs are wider than the rules' regular band and are
declared as such in [waivers](waivers.md); the measured walls (23.10 s, 9.882 s,
15.014 s, 59.326 s) are all inside the wider band **and** are reported as measured
rather than trimmed to fit a smaller one.

## Every family and case
### `file_size_transition`

The F1 boundary family: the seven control, above, below, exact and round-trip cases where a file crosses the 128 KiB small/large content boundary (131071/131072/131073 B). All seven complete in 1.774-2.127 s with a PASS verification, and every one carries its own declaration rather than an inference from the boundary neighbour.

| Selection | Performance | Perf wall (s) | Perf gate | Verification | Verify wall (s) | Verify gate | Declared allowance (s) | Cleanup | Evidence |
|---|---|---:|---|---|---:|---|---:|---|---|
| `v016-boundary-above-v1` | PASS | 1.843 | PASS | PASS | 1.820 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/file_size_transition` |
| `v016-boundary-alias-roundtrip-v1` | PASS | 2.127 | PASS | PASS | 1.758 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/file_size_transition` |
| `v016-boundary-below-v1` | PASS | 1.953 | PASS | PASS | 1.996 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/file_size_transition` |
| `v016-boundary-exact-v1` | PASS | 1.925 | PASS | PASS | 1.907 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/file_size_transition` |
| `v016-boundary-large-control-v1` | PASS | 1.848 | PASS | PASS | 1.956 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/file_size_transition` |
| `v016-boundary-roundtrip-v1` | PASS | 1.774 | PASS | PASS | 1.986 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/file_size_transition` |
| `v016-boundary-small-control-v1` | PASS | 1.927 | PASS | PASS | 1.882 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/file_size_transition` |
### `branch_development`

Four v0.1.6 branch cases plus the two F4 compact-branch controls. The 100 MiB K10/K100 and 500 MiB K10 rows sit at 1.979-7.373 s; the 500 MiB K100 row is the campaign's largest regular row at 16.492 s performance (above the 15 s family target, gate `EXCEPTION`) and 23.10 s verification under the owner-declared 30 s ceiling (ledger L12). The two F4 controls prove per-head compact branch roots and the distinct reference-versus-root accounting that the v0.1.6 oracle requires.

| Selection | Performance | Perf wall (s) | Perf gate | Verification | Verify wall (s) | Verify gate | Declared allowance (s) | Cleanup | Evidence |
|---|---|---:|---|---|---:|---|---:|---|---|
| `v016-branch-convergent-content-v1` | PASS | 2.118 | PASS | PASS | 2.086 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/branch_development` |
| `v016-branch-fork-descendant-v1` | PASS | 1.979 | PASS | PASS | 2.265 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/branch_development` |
| `v016-branch-mixed-100mb-5000-k10-v1` | PASS | 3.325 | PASS | PASS | 4.479 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/branch_development` |
| `v016-branch-mixed-100mb-5000-k100-v1` | PASS | 7.373 | PASS | PASS | 8.244 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/branch_development` |
| `v016-branch-mixed-500mb-30000-k10-v1` | PASS | 6.771 | PASS | PASS | 13.204 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/branch_development` |
| `v016-branch-mixed-500mb-30000-k100-v1` | PASS | 16.492 | EXCEPTION | PASS | 23.100 | EXCEPTION | 60 perf / 30 verify | PASS | `final3-seed1/branch_development` |
### `dedup_branch_history`

The six F5/F6 history profiles - boundary-cycle, large-hotset and namespace-inode at K10 and K100 - each of which replays a five-stage schedule and proves the retained history independently. Performance 2.045-2.821 s, verification 3.670-9.448 s, all PASS. The namespace-inode rows are the family that v0.1.6 rebuilt after the earlier revision's boundary-cycle exchange was found wrong; the corrected exchange (the higher file's byte moves down, (131071,131073)<->(131072,131072)) is what both rows now verify.

| Selection | Performance | Perf wall (s) | Perf gate | Verification | Verify wall (s) | Verify gate | Declared allowance (s) | Cleanup | Evidence |
|---|---|---:|---|---|---:|---|---:|---|---|
| `v016-history-boundary-cycle-k10-v1` | PASS | 2.045 | PASS | PASS | 3.957 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/dedup_branch_history` |
| `v016-history-boundary-cycle-k100-v1` | PASS | 2.503 | PASS | PASS | 3.670 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/dedup_branch_history` |
| `v016-history-large-hotset-k10-v1` | PASS | 2.051 | PASS | PASS | 3.980 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/dedup_branch_history` |
| `v016-history-large-hotset-k100-v1` | PASS | 2.257 | PASS | PASS | 3.784 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/dedup_branch_history` |
| `v016-history-namespace-inode-k10-v1` | PASS | 2.115 | PASS | PASS | 3.683 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/dedup_branch_history` |
| `v016-history-namespace-inode-k100-v1` | PASS | 2.821 | PASS | PASS | 9.448 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/dedup_branch_history` |
### `mixed_load_bearing`

Four regular mixed-development rows (3.042-18.312 s performance, 3.029-22.121 s verification, all PASS) plus two verify-only exhaustive rows that carry no performance timer by declaration. The exhaustive rows are the campaign's deepest oracle replay: 15.014 s and 59.326 s against frozen 120 s and 300 s watchdogs, reported as the measured walls they are.

| Selection | Performance | Perf wall (s) | Perf gate | Verification | Verify wall (s) | Verify gate | Declared allowance (s) | Cleanup | Evidence |
|---|---|---:|---|---|---:|---|---:|---|---|
| `v016-mixed-development-100mb-5000-k10-v1` | PASS | 3.042 | PASS | PASS | 3.029 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/mixed_load_bearing` |
| `v016-mixed-development-100mb-5000-k100-v1` | PASS | 7.423 | PASS | PASS | 7.971 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/mixed_load_bearing` |
| `v016-mixed-development-500mb-30000-k10-v1` | PASS | 5.657 | PASS | PASS | 7.546 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/mixed_load_bearing` |
| `v016-mixed-development-500mb-30000-k100-v1` | PASS | 18.312 | EXCEPTION | PASS | 22.121 | EXCEPTION | 60 perf / 25 verify | PASS | `final3-seed1/mixed_load_bearing` |
| `v016-mixed-exhaustive-100mb-5000-k100-v1` | N/A | — | — | PASS | 15.014 | PASS | 120 verify | PASS | `final3-ext/mixed_load_bearing` |
| `v016-mixed-exhaustive-500mb-30000-k100-v1` | N/A | — | — | PASS | 59.326 | PASS | 300 verify | PASS | `final3-ext/mixed_load_bearing` |
### `multi_workspace_development`

Four concurrent-workspace rows (3.304-15.760 s performance, 3.824-20.360 s verification) plus the F4 four-workspace control (7.573 s / 9.882 s). `v016-workspace-mixed-500mb-30000-k100-v1` is above the 15 s family target in both modes under its declared allowance; the four-workspace control is the only row that runs four named workspaces with 100 commits each.

| Selection | Performance | Perf wall (s) | Perf gate | Verification | Verify wall (s) | Verify gate | Declared allowance (s) | Cleanup | Evidence |
|---|---|---:|---|---|---:|---|---:|---|---|
| `v016-workspace-mixed-100mb-5000-k10-v1` | PASS | 3.304 | PASS | PASS | 3.824 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/multi_workspace_development` |
| `v016-workspace-mixed-100mb-5000-k100-v1` | PASS | 7.524 | PASS | PASS | 8.231 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/multi_workspace_development` |
| `v016-workspace-mixed-500mb-30000-k10-v1` | PASS | 5.960 | PASS | PASS | 10.513 | PASS | 60 perf / 25 verify | PASS | `final3-seed1/multi_workspace_development` |
| `v016-workspace-mixed-500mb-30000-k100-v1` | PASS | 15.760 | EXCEPTION | PASS | 20.360 | EXCEPTION | 60 perf / 25 verify | PASS | `final3-seed1/multi_workspace_development` |
| `v016-workspace-four-100mb-5000-k100-v1` | PASS | 7.573 | PASS | PASS | 9.882 | PASS | 60 perf / 60 verify | PASS | `final3-ext/multi_workspace_development` |
### `historical_access`

The six F6 `historical_access` cases are verify-only by declaration: they carry no performance timer and every one is proved by an independent reader plus a verifier pass in 1.593-1.964 s. Together they cover the boundary before/after pair (131071/131072 B), the inode before/after pair (8192/4096 B), the fork-point trunk and the divergent head B. These are the v0.1.6 rows that replaced the inherited revision's seven-case declaration, and their producers are sealed per case.

| Selection | Performance | Perf wall (s) | Perf gate | Verification | Verify wall (s) | Verify gate | Declared allowance (s) | Cleanup | Evidence |
|---|---|---:|---|---|---:|---|---:|---|---|
| `v016-access-boundary-after-v1` | N/A | — | — | PASS | 1.943 | PASS | 25 verify | PASS | `final3-seed1/historical_access` |
| `v016-access-boundary-before-v1` | N/A | — | — | PASS | 1.655 | PASS | 25 verify | PASS | `final3-seed1/historical_access` |
| `v016-access-divergent-head-v1` | N/A | — | — | PASS | 1.593 | PASS | 25 verify | PASS | `final3-seed1/historical_access` |
| `v016-access-fork-point-v1` | N/A | — | — | PASS | 1.964 | PASS | 25 verify | PASS | `final3-seed1/historical_access` |
| `v016-access-inode-after-v1` | N/A | — | — | PASS | 1.748 | PASS | 25 verify | PASS | `final3-seed1/historical_access` |
| `v016-access-inode-before-v1` | N/A | — | — | PASS | 1.781 | PASS | 25 verify | PASS | `final3-seed1/historical_access` |
## Results above the 15 s family target

| Selection | Mode | Measured (s) | Gate | Declared allowance (s) |
|---|---|---:|---|---:|
| `v016-branch-mixed-500mb-30000-k100-v1` | performance | 16.492 | EXCEPTION | 60 perf / 30 verify |
| `v016-branch-mixed-500mb-30000-k100-v1` | verification | 23.100 | EXCEPTION | 60 perf / 30 verify |
| `v016-mixed-development-500mb-30000-k100-v1` | performance | 18.312 | EXCEPTION | 60 perf / 25 verify |
| `v016-mixed-development-500mb-30000-k100-v1` | verification | 22.121 | EXCEPTION | 60 perf / 25 verify |
| `v016-workspace-mixed-500mb-30000-k100-v1` | performance | 15.760 | EXCEPTION | 60 perf / 25 verify |
| `v016-workspace-mixed-500mb-30000-k100-v1` | verification | 20.360 | EXCEPTION | 60 perf / 25 verify |
| `v016-mixed-exhaustive-100mb-5000-k100-v1` | verification | 15.014 | PASS | 120 verify |
| `v016-mixed-exhaustive-500mb-30000-k100-v1` | verification | 59.326 | PASS | 300 verify |

The two verify-only rows carry gate `PASS` because the allowance their own receipt
declared is 120 s and 300 s; measured against the 15 s family target they are
target misses, and they are printed here as such rather than reported as fast
rows. Every other gate in this closeout is `PASS`, and the three `EXCEPTION`
gates are the rows above the 15 s target inside a declared allowance. No row in
this release record is a `FAIL`, a `TIMEOUT` or a `NOT_RUN`.


## Identity of every row in this closeout

| Field | Value |
|---|---|
| Source commit | `823f556ca301e261b71d41b09ef619897b48e5f9` |
| `LAYERFS_SOURCE_SEAL` | `86f14b2d68ece2ae368f8aec29070520505a61c7aa69b445414b64561640e954` |
| `LAYERFS_SOURCE_DIRTY` | `true` (foreign uncommitted `docs/roadmap/0.1.7` study material; recorded, not hidden) |
| `LAYERFS_PRODUCT_SEAL` | `970964e9af43a8bf57f0d7bec70736a94171f7beb62fc3378ea5cc4797500ebd` |
| `LAYERFS_COMPILATION_SEAL` | `679b17f0e17636ae419144edb7377be03dd1709ac362d72ea0dff61d3b89306b` |
| `LAYERFS_DEPENDENCY_SEAL` | `374f4dfa08f98ced734e540f62dd4a8b0edb5a88c090c051ae704fbf7f67faf1` |
| Host binary SHA256 | `fcaa14d8decf96a6247d95a037e83eea7322aeb66a51d418acbee2e21fdd6aa7` |
| Harness identity | `8a6d76dd2f2e639fb356ac74551a4aae2634df1eaf2b3603ebd458cafd90dff5` |
| Image | `layerfs-bench-infra:86f14b2d68ece2ae` = `sha256:2bd697b90894749592cf62e6470a0d0a9cf54a0416cbf3cd903dbbcc3d9954df` |
| Build identity probe | `schema_version: 10`, `storage_policy: ordinary`, SQLite 3.51.0, status PASS |
| Host load during collection | `load_1m` 6.73–8.83 on a 14-CPU host, recorded per driver record |
| Evidence root | `benchmark-results/v016/final3-seed1/` and `benchmark-results/v016/final3-ext/` (untracked local receipts, cited by path per row) |
| Matrices | `docs/roadmap/0.1/0.1.6/evidence/issue154/final-complete-matrix.json`, `…-extended-matrix.json` |

Every row omits an exhaustive Phase 1 replay by declaration
(`omissions: ["no exhaustive Phase 1 replay"]`), and no row carries a reused proof
identity (`reused_proof_identities: []` in all 36 verifications).

This closeout covers the v0.1.6 selections only. The sandbox-local comparison
campaign — the B1/B2/B3 controls and the 196-cell #152 matrix — is published
separately in the
[#152 final report](../../docs/roadmap/0.1/0.1.6/evidence/issue152-final-report.md).
