# Phase 0 run plan (frozen with the contract, before collection)

Every command below is run **once**, in this order, with the measurement lock
held. Each command's exit code and wall time are appended to `commands.tsv` by
the driver (`collect.py`) as it runs, and its complete stdout/stderr is written
to a fresh log file that is never overwritten.

## Build (not timed, outside the lock)

| # | Command | Purpose |
| --- | --- | --- |
| B1 | `cargo +1.85.1 build --release --offline --manifest-path core/Cargo.toml --locked --examples` | candidate arm artifacts |
| B2 | `cargo +1.85.1 build --release --offline --manifest-path Cargo.toml --locked -p layerfs-content --example stage5_component_reference` | reference arm artifact |
| B3 | `cargo +1.85.1 build --release --offline --locked` in `client/` | P0-3 probe client |

`--offline` is used because the dependency set is already in the local registry
cache; `--locked` still pins the resolution against the committed lockfiles.

## P0-1 `component.primitives` (matched, collected)

| # | Case | Command (identical shape, both arms, `--samples 1`) |
| --- | --- | --- |
| C1..C3 | `small` / `wide` / `large-few-changes` | the precedent driver `tools/stage5_component_comparison.py --output <fresh>` |

The precedent driver starts one process per arm per case, `release` profile, one
sample, and enforces the six-key identity gate. It is used unmodified, exactly as
the earlier collections were.

## P0-1 `pipeline.filesystem` and `pipeline.c2` (matched? re-verified)

No command: the determination is a source re-verification at this tree, recorded
in `receipts/p0-1-pipeline-notrun.md` with `path:line` citations. A `NOT_RUN`
row with a source-backed reason and no measurement.

## P0-2 spilling determination

| # | Command | Labelling |
| --- | --- | --- |
| S1 | `phase0client c2 <rows> default` | **gate sample** (shared with P0-3 `c2.ceiling`) |
| S2 | `phase0client c2 <rows> diagnostic-large-cache` | **diagnostic only** (raises `cache_size` before the Store is created) |
| S3 | `phase0client c2 <rows/8> default` | **gate sample** (P0-3 `c2.small`) |

`<rows>` is the row count that reaches the declared transaction ceiling of
8,191 rows under the default policy, determined in S0 below.

| # | Command | Purpose |
| --- | --- | --- |
| S0 | `phase0client c2 1 default` then `c2 8191 default` | confirm one full-size transaction is 8,191 rows and that no second transaction opens |

## P0-3 counter baseline

| # | Workload | Command |
| --- | --- | --- |
| D1..D6 | `c1.empty` .. `c1.attributes` | `filesystem_timing_c1 --case <case> --output <fresh>` (core) |
| D7..D9 | `fs.c1/c2/pipeline.directory-update` | `measure_filesystem --mode <m> --case directory-update --entries 200 --output <fresh>` |
| D10..D24 | `edits.<mode>.<case>` | `measure_edits --mode <m> --case <c> --threshold-bytes 131072 --output <fresh>` |
| D25..D26 | `order.default` / `order.forced64` | `phase0client order 4000 2000 <4096|64>` |
| D27..D28 | `c2.ceiling` / `c2.small` | the S1/S3 gate samples, cited |

Case lists: `measure_edits` cases are `small`, `chunked`, `small-to-large`,
`large-to-small`, `batch`; modes are `c1`, `c2`, `pipeline`.
`measure_filesystem --case directory-update` uses `--entries 200`.

## Determinism diagnostics (labelled)

| # | Command | Labelling |
| --- | --- | --- |
| X1 | repeat D25 | diagnostic: counter stability |
| X2 | repeat D5 | diagnostic: counter stability |
| X3 | repeat the `component.primitives` `wide` case | diagnostic: cross-run spread, not a gate |

Diagnostics are reported beside their gate sample and never replace it.
