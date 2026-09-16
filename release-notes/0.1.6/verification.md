# v0.1.6 verification and source applicability

> **Status:** LayerFS 0.1.6 release record.

## Measured product and evidence base

| Field | Value |
|---|---|
| Measured source revision recorded on every receipt | `823f556ca301e261b71d41b09ef619897b48e5f9` |
| `LAYERFS_SOURCE_SEAL` as recorded | `86f14b2d68ece2ae368f8aec29070520505a61c7aa69b445414b64561640e954` |
| `LAYERFS_SOURCE_DIRTY` as recorded | `true` (foreign uncommitted `docs/roadmap/0.1.7` study material; recorded, never hidden) |
| `LAYERFS_PRODUCT_SEAL` | `970964e9af43a8bf57f0d7bec70736a94171f7beb62fc3378ea5cc4797500ebd` |
| `LAYERFS_COMPILATION_SEAL` | `679b17f0e17636ae419144edb7377be03dd1709ac362d72ea0dff61d3b89306b` |
| `LAYERFS_DEPENDENCY_SEAL` | `374f4dfa08f98ced734e540f62dd4a8b0edb5a88c090c051ae704fbf7f67faf1` |
| Host binary SHA256 | `fcaa14d8decf96a6247d95a037e83eea7322aeb66a51d418acbee2e21fdd6aa7` |
| Harness identity | `8a6d76dd2f2e639fb356ac74551a4aae2634df1eaf2b3603ebd458cafd90dff5` |
| Image | `layerfs-bench-infra:86f14b2d68ece2ae` = `sha256:2bd697b90894749592cf62e6470a0d0a9cf54a0416cbf3cd903dbbcc3d9954df` |
| Build identity probe | `schema_version: 10`, `storage_policy: ordinary`, SQLite 3.51.0, status PASS (in every receipt's `integrated_format_probe`) |
| Matrices | `docs/roadmap/0.1/0.1.6/evidence/issue154/final-complete-matrix.json` (33 rows) and `…-extended-matrix.json` (3 rows) |
| Receipts | `benchmark-results/v016/final3-seed1/**` and `benchmark-results/v016/final3-ext/**` (untracked local evidence, cited by path) |

The build identity probe is the product's own format probe, reporting the observed
schema version and storage policy for the exact measured binary. It reports
**schema 10 / ordinary**, which is what this release contract and manual claim.

## What this release measured

| Verdict | Count |
|---|---:|
| Rows in the release set (33 regular + 3 declared extensions) | 36 |
| Performance receipts | 28 |
| Performance gate `PASS` / gate `EXCEPTION` | 25 / 3 |
| Rows verify-only by declaration (no performance timer) | 8 |
| Independent verifications, all `PASS` | 36 |
| Verification gate `PASS` / gate `EXCEPTION` | 33 / 3 |
| `FAIL`, `TIMEOUT`, `NOT_RUN` anywhere in the set | 0 |
| Cleanup `PASS` (64 driver records: 28 performance + 36 verification) | 64 |
| Reused proof identities | 0 |
| Mode-level results above the 15 s family target | 8 |

Seed 1, one sample per case and per mode, repetition 1, `--setup clone` for every
post-initialization case, and the same image, harness, product and sealed producer
for both modes. Every row omits an exhaustive Phase 1 replay by declaration. The
full per-case table is the [closeout](benchmark-closeout.md).

## Declared allowances and what admits a row

* **Family target 15 s** — reported for every performance and verification row,
  unchanged. A row above it is a target miss; three performance rows and five
  verification results are, and are printed with their measured wall.
* **Regular performance invocation allowance: 60 s** (owner ruling, ledger L10).
  It is an execution allowance, not a redefinition of the target.
* **Regular verification ceiling: 25 s** (22 s worker stop + 3 s cleanup reserve),
  frozen in `cases.json` and unchanged by the performance ruling.
* **One declared verification exception: 30 s** for
  `v016-branch-mixed-500mb-30000-k100-v1` (owner ruling on #154, ledger L12),
  keyed by the exact registered ID and asserted against accidental widening by a
  harness test. Measured: 23.10 s in the collection, 24.17 s standalone.
* **Three frozen extended watchdogs: 60 s / 120 s / 300 s** for the four-workspace
  control and the two exhaustive verify-only replays. The two 120 s / 300 s
  verification watchdogs are wider than the rules' regular verification band; the
  measured walls (15.014 s and 59.326 s) are inside them and are reported as
  measured.

A row is admitted only by the allowance its own receipt declared and enforced; the
derived tables print that number per row. No oracle, coverage, limit, workload or
worker count was changed to make a row fit, and no timeout was enlarged at run
time.

## Cache stance, reuse and host load

* All 28 performance receipts reused the closed prepared master
  (`preparation.cache_hit: true`, `setup_identity: clone`,
  `clone_method: closed-quiescent-byte-copy`, `prepared_master_unchanged: true`).
  Preparation cost 0.424–2.563 s and ran outside every timer; cleanup ran
  0.338–0.864 s and passed in all 64 records.
* No proof was reused (`reused_proof_identities: []` in all 36 verifications).
* The samples were collected on a busy 14-CPU host: `load_1m` 6.73–8.83, recorded
  in every driver record. All samples in this set share that regime; none is
  pooled with a quiet-host sample, and none is presented as a quiet-host number.
* The sandbox-local comparison campaign in [#152](../../docs/roadmap/0.1/0.1.6/evidence/issue152-final-report.md)
  records the cache-stance finding that shapes v0.1.6: a transfer credited by the
  workload's own recent writes read at 19 GB/s, and the same bytes cost 2.1 GiB/s
  once the spool's resident window was bounded. That is why the measured Commit
  transfer is the storage read and not the cache number.

## Exact-source applicability

The receipts' `LAYERFS_SOURCE_SEAL` is a **build-time** identity: it is stored in
the host binary's `.identity.json` and validated against the image labels, and the
campaign reused that binary rather than rebuilding. Reproducing it here pins the
delta exactly.

| Check | Result |
|---|---|
| Product seal recomputed over the tagged tree's `crates/**` | `970964e9…` — **identical to every receipt's product seal** |
| `crates/**` files changed since the measured revision | **none** (byte-identical; the version bump lives in the root `Cargo.toml`/`Cargo.lock` only, and crate manifests inherit `version.workspace`) |
| Harness identity recomputed from the tagged tree's `runner.py`, `runtime.py`, `cold.py`, `verify-selected.py` | `8a6d76dd…` — **identical to the identity the receipts recorded** |
| Source seal recomputed over the tagged tree | `fdbd6273…` — differs from the recorded value |
| Source seal recomputed with the measured revision's `Cargo.toml`/`Cargo.lock` **and** the three harness route files at their `823f556ca` content | `86f14b2d68ece2ae…` — **reproduces the recorded value byte-for-byte** |

So the recorded source seal differs from the tag tree by exactly two things:

1. the **release version bump** `0.1.5 → 0.1.6` in the root `Cargo.toml` and the
   12 project-owned packages in `Cargo.lock` (imported packages, versions,
   features and checksums unchanged), which is inside the source seal by
   construction and changes no product code path; and
2. the **harness route declarations** committed after the binary was built: the
   owner-declared 30 s verification exception and its "no widening" harness test
   (`3e6f2dac5`) plus the verify-only wiring for the six `historical_access`
   cases. These are collector-side Python; they change what a *collection* may
   declare, never what the product does. The receipts show them at work (the
   branch row's own policy is 30 s) and the harness identity that collected the
   rows reproduces exactly from this tree.

`docs/**` and `release-notes/**` are outside the source seal and outside the
product seal, so the release documentation, the manual, the changelog, the release
record and this file cannot change either identity — the same boundary the v0.1.5
release recorded. No measured result is invalidated by the tag-tree delta, and the
release therefore reuses the campaign rather than re-measuring it.

## Release-preparation checks run in this closure

`tools/preflight.sh` is the repository's pre-push gate (the repository runs no CI).

| Step | Result |
|---|---|
| `cargo +1.96.0 fmt --all --check` | **PASS** — no diff |
| tools unit tests | **PASS** — 7 tests, 0 failures |
| workspace fast suite (`RUSTUP_TOOLCHAIN=1.85.1`, 4 bounded jobs) | **PASS** — 72 test binaries, 512 tests passed, 0 failed, 258 s, exit 0. The suite ran 138 s above the 120 s warm-suite soft ceiling and preflight reported that warning; it is a soft budget, not a gate, and it is recorded rather than omitted. |
| `cargo +1.96.0 clippy --workspace --locked -- -D warnings` | **PASS** — no warning |
| benchmark harness tests (`benchmark/fs-bench-pro/shared`) | **PASS** — 97 tests, 0 failures, 7.0 s |
| `tools/preflight.sh` overall | `preflight: all steps passed`, exit 0 |
| `python3 release-notes/0.1.6/check_artifact_helper.py` | **PASS** — `artifact helper fixture check: PASS` (tag rejection, six-asset build, overwrite refusal, checksum corruption) |
| `python3 release-notes/0.1.6/derive_tables.py` | **PASS** — 36 rows written, every receipt path present |
| Seal reproduction (product / harness / source) | as tabulated above |

No benchmark, proof or benchmark workload was executed for this release; the
audit commands above read receipts, files and seal inputs only.

## What is not verified by this release

* No new performance, storage or endurance measurement was made for publication.
  Every number is the seed-1 campaign's own, with its receipt path in the derived
  tables.
* Endurance (a sustained 600 s proof) is **not** qualified.
* Sandbox *process* memory is not measurable on this frozen harness: only
  container-scoped quota/current/lifetime-peak numbers exist, and a lifetime
  cgroup peak is not a phase number.
* Kernel-dirty shared `mmap` is still not captured by a Commit — an open
  obligation carried from the specification, isolated to exact byte level, with
  no registered selection affected.
* The 11 + 11 inherited `historical_access` rows in [#152](../../docs/roadmap/0.1/0.1.6/evidence/issue152-final-report.md)
  remain `NOT_RUN` because their sealed v2 Store is not recoverable from this
  tree; the six `historical_access` cases v0.1.6 registers are a separate,
  fully measured and verified implementation.
* The recorded `LAYERFS_SOURCE_DIRTY=true` is a real property of the campaign
  tree, not a claim about this release's tree; the dirty files were foreign
  `docs/roadmap/0.1.7` study material outside both seals.
