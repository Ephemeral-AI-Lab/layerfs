# G3-v10 post-campaign static-closure revision

Disposition: **CAMPAIGN PASS / STATIC CLOSURE REVISE / TERMINAL SEAL FORBIDDEN**

The v10 build, all nine once-only measured rows, all 18 durable cleanup events,
primary analysis, independent recomputation, and campaign record completed
`PASS`. The subsequent workspace static-closure command exposed a Cargo target
discovery defect. v10 therefore remains valid campaign evidence but cannot be
finalized, terminally sealed, or reused as a v11 measurement.

The retained v10 result root remains append-only evidence:

```text
target/phase4-g3-incremental-materialization-20260822-v10/results-v10
```

## Exact static-closure failure

`STATIC-CLOSURE-v10.json` has SHA-256
`9adcb1d8c4df922bdb73a8aaacb57538b634d379c8ccee7303a4fdb465f29fbd`
and status `REVISE`. It records exactly two commands:

| Seq | Label | Result | Wall ns | Stdout SHA-256 / bytes | Stderr SHA-256 / bytes |
|---:|---|---:|---:|---|---|
| 1 | `focused-g3-tests` | exit 0 | 1021632875 | `731d9a48fa9166d483834061782636e56ae22abcea1783642a2cfb4401a14075` / 1038 | `7ca0c3df8d6e3206263628f98f4ab5b4ea3bc273d320c2fbc6eb264417e3094b` / 202 |
| 2 | `workspace-tests` | exit 101 | 2232333250 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` / 0 | `105d89e8eec383391c65ec4580746464f4096041435ca44e7f7740c38668df72` / 87233 |

The focused command ran the exact G3 test module and passed **9 tests, 0
failed**. Its retained artifacts are:

- `static-v10/01-focused-g3-tests.stdout`;
- `static-v10/01-focused-g3-tests.stderr`.

The next command, `cargo test --workspace --offline --all-targets`, failed while
compiling `crates/layerfs-engine/src/bin/phase4_g3_materialization.rs` as an
independent binary target. Cargo's default binary auto-discovery treated every
top-level Rust file in `src/bin/` as a binary. That file is deliberately a
module of `phase4_create_edit_benchmark.rs`, so standalone compilation has no
parent for `use super::*` or `pub(super)` and consequently emitted the leading
`super` and missing parent-scope type errors retained in
`static-v10/02-workspace-tests.stderr`.

The repair is manifest-only: set `autobins = false` for `layerfs-engine` and
declare the intended main binary explicitly. It neither changes the G3 module,
main benchmark source, schedule, mechanism, counters, nor measured v10 facts.

No clippy, rustfmt, diff-check, custody-review, finalizer, manifest, terminal, or
terminal-verification command ran after the workspace failure. The static record
has `candidate_retained=false`; v10 cannot seal.

## Preserved v10 campaign identity

| Evidence | SHA-256 / identity |
|---|---|
| Campaign status | `PASS` / `G3_V10_CAMPAIGN_PASS_STATIC_CLOSURE_REQUIRED` |
| Campaign record | `05edc1458a028d3a4657f0e72bf789015ecb5f1572eebbc423579fe4de7d1d41` |
| Campaign source set | `70ef2606389813ebd980bf2e5fe9f4585333717fd7dabf21fb69cb4e4c140c9f` |
| Campaign methodology set | `2c3fb42e64d11b60d84a3a60619403974851c245707b48e87e6c870ce35f7613` |
| Frozen executable | `82136ed86f19e645cb5611b9b520fe0454b947188a824e6b7022491421b34cd3` |
| Raw JSONL | `09c151dfd6e0d5da33e3ca12259eb8e3228de3399e4615d62178c5cdafb0e089` |
| Primary analysis | `5daf462e5b2b990b22e1f3c6fa0885263bd70e948b10dac226b18d5e59e9dcb1` |
| Independent recomputation | `775433fc488c3a64e4321fd3b41127b7cfc57bd612383d955b75928f34d2422a` |
| Normalized ledger | `669b75b4b811e7a449d647fed22caa90a8029e426f773340cc4b799f29b31de3` |
| Cleanup summary | `05936a1c452507b6ed341a9161e87c997edd900e651b2e7119be912348d9ef92` |
| Row-cleanup JSONL | `db1210f717c6f10b0af34cb51c1693a910ce870a2c45abc33fa4334eea2a8555` |

The campaign wall remains `7,138,212,917 ns`, the operation sum remains
`24,192,708 ns`, and every retained row remains exact with zero terminal Q and
zero temp/seed residue. This report does not upgrade those campaign facts into
a terminal PASS.

## v11 consequence

v11 must use a fresh result root and lock, freeze the new manifest hash and
derived source-set digest, rebuild exactly once, and rerun all nine rows. Before
any build, its runner and self-check must read Cargo metadata and require exactly
one `layerfs-engine` binary target: `phase4_create_edit_benchmark` at
`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs`. The G3 module
must not be a target. Any other binary count, name, kind, or path is a preflight
failure.
