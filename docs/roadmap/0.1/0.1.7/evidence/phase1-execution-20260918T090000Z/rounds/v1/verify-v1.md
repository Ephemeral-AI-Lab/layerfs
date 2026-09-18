# verify-v1 — validation-work exposure in the C1 timing vehicle

> **author-verified** (single-agent Phase 1: no independent reviewer exists; the
> evidence is the reproducibility of the commands below). Round:
> [`receipt.md`](receipt.md). Tree: `2b5e27e65` (the V1 commit), arm
> [`after/`](after/) with `artifacts.txt` recording the binaries that ran.

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 2b5e27e65 \| tar -x -C /tmp/verify-v1` | 0 | clean V1 tree |
| R2 | `cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --example filesystem_timing_c1` | 0 | example rebuilt from the archive |
| R3 | the six cases run from the archive's own example binary | 0 ×6 | content identical to [`after/logs/`](after/logs) — `diff` over stdout minus `elapsed_ns`/`phase`/`report_nodes` is **0 lines for all six rows**, including the new `validation:` line; e.g. D2 `validation: objects 21 waves 42 inode_demands 21 inode_pages 42 directory_pages 0 entries_examined 0` |

```sh
# run from the repository root, after R1/R2
for i in 1 2 3 4 5 6; do
  f=$(ls rounds/v1/after/logs/D$i-*.log)
  case=$(awk '/^--- stdout ---$/{f=1;next}/^--- stderr ---$/{f=0}f' "$f" | sed -n 's/^case //p' | head -1)
  diff <(awk '/^--- stdout ---$/{f=1;next}/^--- stderr ---$/{f=0}f' "$f" \
         | grep -vE '^(elapsed_ns |phase |report_nodes)') \
       <(/tmp/verify-v1/target/release/examples/filesystem_timing_c1 --case "$case" \
         --output /tmp/verify-v1/out-$case | grep -vE '^(elapsed_ns |phase |report_nodes)')
done   # exit 0, no output, six times
```

The six readings in the round's own arm are reproduced from the archive, so the
receipt's §4 table is not a claim about a dirty tree.

## 2. Falsification answers

**2a — does a "new test" fail on the pre-item tree?** V1 adds no test: it is a
vehicle, and its observable is a printed line. The equivalent check is the
**pre-item vehicle does not print the field at all**: `c1-rebaseline/after/logs/
D2-c1-directory-update.log` (C1's tree, before V1) contains no `validation:` line
— the field is `NOT_EXPOSED` there by construction, which is exactly the Phase 0
gap V1 closes. That is the failure mode V1 removes, and it is visible in the two
arms' logs side by side.

**2b — did the counter move in the predicted direction?** There is no predicted
movement: V1 changes no product code, so every pre-existing counter must be
**identical**, and it is (§5 of the receipt: six rows, empty diff after removing
the new line and elapsed figures). A movement would have been the finding.

**2c — parity green and unchanged?** V1 touches one example file and no test
file: `git diff 7447f87d9..2b5e27e65 --stat` is
`core/crates/layerfs-content/examples/filesystem_timing_c1.rs | 13 +`. The 34-test
sealed-oracle set is untouched and green (re-run on the C1 tree,
[`../c1-rebaseline/verify-c1.md`](../c1-rebaseline/verify-c1.md) §1).

**2d — single-variable?** Yes: one example file, one added print. No product
source, no test, no tool.

**2e — error paths.** The vehicle exits non-zero when its timing report is
clipped (`report.is_incomplete()` → exit 2); the six frozen cases exit 0 and the
round's `commands.tsv` records each. A malformed-input probe is not applicable to
a print-only change.

**2f — elapsed as a gate?** No. §8 of the receipt labels every elapsed figure
diagnostic.

## 3. UNVERIFIED

* **`entries_examined` is 0 on all six frozen cases**, so V1 does not create an
  anchor for that field. Recorded in the receipt §4; P1-4 must check it on the
  suite's directory-rename fixtures, and this file does not claim otherwise.
* **`measure_filesystem` and `measure_edits` still do not print
  `ValidationWork`.** The plan nominates `filesystem_timing_c1` only; if P1-4's
  movement needs a second vehicle, that is a new vehicle decision, not a silent
  extension of this one.
* **The round's `order`/`c2` rows are the corrected-driver values** and are
  therefore the first comparable post-C1 rows; C1's own `after/` arm for those
  rows is superseded by [`../c1-rebaseline/receipt.md`](../c1-rebaseline/receipt.md) §9.
