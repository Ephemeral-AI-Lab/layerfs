# Stage 5 terminal round, harness and contract receipts (2026-09-18)

> **Status:** Round-3 evidence for the Stage 5 terminal handoff
> ([`stage-5-terminal-handoff-20260917.md`](../../component-decoupling/stage-5-terminal-handoff-20260917.md)).
> Append-only. Nothing in this directory was edited after it was written, and no
> prior receipt, report or failed attempt was overwritten.

| | |
| --- | --- |
| Tree | `2fe2a4642989920c25c80a8435e367243abb4a94` (`head.txt`), branch `main` |
| Commits in this round | `5e2a20a0c` (harness case selection, report §6), `2fe2a4642` (product bounds, telemetry, C1 contracts) |
| Previous reviewed tree | `6b7170e05` / `f288d2af7` (round-2 review snapshot) |
| Production LOC | core 18,650 → **18,708** (+58): C1 11,875 → 11,902, C2 6,043 → 6,043, telemetry 732 → 763; reference 65,417 unchanged |
| Checks | all eight of the handoff §6 exit 0 (`check-*.log`) |

## What this directory proves

| Finding | Receipt | What it shows |
| --- | --- | --- |
| `R2-F1` | `case-selection/c1-attribute-case/{empty,attributes}.log` | `--case attributes` applies a real attribute patch inside the timed region: 5 objects emitted and 2 canonical pages read where `empty` emits 1 object and reads none, plus the patch counters and the 21 bytes it wrote read back through the patch and value paths |
| `R2-F2` | `case-selection/c2-*.log` | six `--case` values in `--mode c2` produce six distinct case roots, six distinct applied change sets and six distinct readbacks |
| `R2-F3` | `case-selection/edits-c2/small.log` | `measure_edits --mode c2` times Store creation, begin/accept/finish and nothing else; the two C1 objects are encoded in a labelled untimed preparation step and the read-back is its own labelled region |
| `R2-F6`, `AT-4`, `R2-F25` | `attribute-boundary.log` | the declared value bound equals `cdc::MAXIMUM_CHUNK_BYTES`; 32,768 bytes are emitted and read back whole, 32,769 are refused by the declared bound |
| `R2-F7`, `VF-4` | `walk-ceiling.log` | a directory is grown past the 4,096-entry walk ceiling by an accepted operation and its rename is then refused by that ceiling |
| `N-11`, `R2-F21` | `read-wave.log` | an oversized chunk served under a legal leaf is refused at the wave boundary with no bytes emitted, and the largest legal wave equals the declared `READ_WAVE_BYTES` |
| `R2-F12`, `R2-F13` | `telemetry.log` | disabled, complete and clipped are three distinct states; a node that never returned reports `Unknown`, not `Ok` |
| `R2-F17` | `encoder-context.log` | the same short page is accepted as a root and refused as a non-root |
| `R2-F18` | `policy-validation.log` | `construct_bytes` refuses an unsupported policy and emits nothing |
| `R2-F20` | `edit-single-read.log` | a counting provider shows the base root is demanded exactly once per edit and no identity twice |
| `R2-F22` | `into-parts.log` | the moved pieces carry the advisory predecessor and its provenance |
| `N-16`, `VF-4` | `filesystem-limits.log`, `storage-limits.log` | one boundary case per reachable limit on both sides, plus the limits no fixture can reach with their arithmetic |

## Reproduce

```sh
git checkout 2fe2a4642989920c25c80a8435e367243abb4a94
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
python3 -m unittest discover -s tools -p 'test_production_loc.py'
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
python3 tools/production_loc.py --files
git diff --check
```

The case-selection receipts are produced by the examples themselves, one fresh
`--output` directory per row:

```sh
for m in c1 c2 pipeline; do for c in empty directory-update inode-update hardlink-move subtree-remove attributes; do
  ./core/target/debug/examples/measure_filesystem --mode $m --case $c --output <fresh-dir>
done; done
./core/target/debug/examples/filesystem_timing_c1 --case attributes --output <fresh-dir>
./core/target/debug/examples/measure_edits --mode c2 --case small --threshold-bytes 131072 --output <fresh-dir>
```

## Numbers

| Artifact | Value |
| --- | --- |
| `cargo test` result blocks | 62 |
| Tests passed / failed | 422 / 0 |
| `check_product_boundary.py` | 116 production Rust/SQL files scanned, exit 0 |
| `core/tools` unit tests | 5 passed, exit 0 |
| `tools` LOC unit tests | 17 passed, exit 0 |
| `fmt --check`, `clippy -D warnings`, `git diff --check` | exit 0 each |
| Distinct `c2` case roots | 6 of 6 |
| Distinct `pipeline` case roots | 6 of 6 |

## What this round does **not** close

Named here so no row is silently dropped:

- **`R2-F8` / `N-6` (ordering superlinearity)** — the dead `LookupScan::settled`
  state and the duplicated `from` arm are fixed; the reader-per-tier hoist is
  **not** done. `RunStore::find` builds a `RunReader` per lookup, and the reader
  borrows the tier's run, so keeping one per tier needs an ownership change
  (the store would have to own each reader beside its run). No scaling receipt
  is claimed for this round.
- **`R2-F4` / `VF-7`, `R2-F5` / `VF-5`, `VF-6`, `TR-5`, `VF-3`, `N-13`** — the
  per-commit LOC correction beyond §6, comparison governance, the
  complete-operation comparison, the simultaneous-memory row, the
  non-discriminating cases, and the `forbid(unsafe_code)` decision are **not**
  addressed by this round's commits.
- **`VF-6` (complete-operation comparison)** remains `NOT_RUN` and still needs
  an owner disposition: run it, waive it in writing, or move it to the stage that
  owns it.

No row is marked PASS by this document. A row flips only when an independent
reviewer reproduces it from these receipts.
