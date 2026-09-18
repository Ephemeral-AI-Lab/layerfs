# verify-p1-8 — an assembly's retained runs share one descent

> **author-verified** (single-agent Phase 1: no independent reviewer exists; the
> evidence is the reproducibility of the commands below on the named trees).
> Round: [`receipt.md`](receipt.md). Tree: `e9b4d1510`, arm [`after/`](after/);
> before arm [`../p1-6/after/`](../p1-6/after/) (tree `360431d10`).

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive e9b4d1510 \| tar -x -C /tmp/verify-p18` | 0 | clean P1-8 tree |
| R2 | `cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples` | 0 | `edit_timing_c1` sha256 `124462ab…` (matches `after/artifacts.txt`) |
| R3 | `/tmp/verify-p18/core/target/release/examples/edit_timing_c1 --case split` | 0 | `nodes_read: 13`, `edited_root: 4a4caa46…` |
| R4 | `git archive 360431d10 \| tar -x -C /tmp/p1-8-parent` + the same example source copied in + R2's build | 0 | `edit_timing_c1` sha256 `bfc7332d…`; `--case split` → `nodes_read: 16`, same root |
| R5 | `cargo +1.85.1 test --offline --locked --manifest-path core/Cargo.toml -p layerfs-content --test edit_transitions` on R1 | 0 | `9 passed` |
| R6 | the same test file copied into R4 and run | 101 | `retained_segments_share_one_descent` fails: `left: 3, right: 1` |
| R7 | `edit_timing_c1` (no argument / `--case delete` / `--case shrink`) on R1 | 0 ×3 | `nodes_read` 9 / 4 / 11 — identical to `after/logs/D27`, `M2`, `M3` |
| R8 | counter-only diff of `after/` against `../p1-6/after/` (strip `elapsed_ns` and the timing tree) | 0 | only the new `M4` row differs |

Rows R4/R6 are stored under [`parent-m4/`](parent-m4/) (`sample.log`,
`repeat.log`) and in this file; R8 is the §3.2 table of the receipt.

## 2. Falsification answers

**2a — does the new test fail on the parent tree?** Yes. `retained_segments_share_one_descent`
reports the mapping root demanded **3** times on `360431d10` and **1** time here
(R6). The helper it needs (`tree_pages`, `collect_nodes`) and the fixture are
self-contained in the test file, so copying that one file into the parent archive
is the whole falsification run.

**2b — did the counter move in the predicted direction and magnitude, and did
anything else move?** Predicted "lower by ≈ (R−1)·h"; measured `M4` `nodes_read`
16 → 13 with R = 3 retained runs and a two-level tree (`h` = 3 pages, of which the
root is shared), i.e. −(R−1)·(shared pages) = −3. **Nothing else moved:** the
counter-only diff over all 35 commands and 29 D-rows is empty apart from the new
`M4` row (R8), including D25/D26/D27, D2's `validation:` line, and every
`edits.*` row.

**2c — parity green and unchanged?** Green: the 34-test sealed-oracle set passes,
`edits.c1.small` keeps its root and 65,559 canonical bytes, `edits.pipeline.small`
reads back byte-for-byte, and `M4`'s own `edited_root` and 120,023 emitted bytes are
identical across the two trees. Unchanged: `git diff 360431d10..e9b4d1510 -- '*tests*'`
shows one file (`edit_transitions.rs`) with two added tests and their helpers, and
no edit to any existing assertion.

**2d — single-variable?** One product mechanism (`RangeCursor` + the page cache in
`file/mapping/read.rs`, and the one call site in `file/edit/apply.rs`), its two
tests, the additive `--case split` vehicle row, the driver row that collects it,
and the two architecture documents. The traversal itself is unchanged except that
one level's pages are now acquired through `Wave::pages` instead of calling the
provider directly — the same batch, the same order, the same charge.

**2e — error paths.** Probe each one:
* *A non-ascending range* is refused before any work: `read_segment` returns
  `InvalidRange` for `range.start < segment_start`, so a caller cannot rewind the
  cursor.
* *A range past the end* is refused by the same check (`range.end > logical_len`).
* *A short service* is refused: each segment checks its own
  `payload_bytes_read == requested` and returns `mapping coverage`, so a cursor can
  never report a partial range under `Ok`.
* *A provider that returns too few pages* is refused at `Wave::pages`
  (`BatchCardinality`), before any decode.
* *An empty range* returns the running counters and demands nothing.
* The whole edit suite (`transitions`, `single`, `batch`, `model`, `noop`,
  `localized`, `bounds`, `reference`) and the read suite (`file_read`, including
  the navigation-wave and wave-ceiling pins) are green.

**2f — elapsed as a gate?** No. The gates are `nodes_read` (M4) and the in-test
demand census. `elapsed_ns` is quoted nowhere as evidence.

## 3. UNVERIFIED

* **Payload demands do not move.** The plan's
  `straddling_payload_demanded_once_across_segments` target is **refuted**, not
  met: a chunk straddling two retained runs is demanded once per run, by design
  (each run is served exactly and independently). The test with that name survives
  as the parity guard on the same shape. Recorded in the receipt §2 and §6.
* **M3 does not move** (`nodes_read` 11 → 11): it is a single retained run, so
  `(R−1)·h = 0`. The handoff's §2.1 receipt line predicted a fall on M3; that
  prediction is refuted by the demand census (1 file state + 2 mapping pages + 7
  payloads, nothing demanded twice). M4 is the discriminating row instead.
* **The eviction path is review-verified only.** No frozen row's mapping tree
  reaches 64 pages (M4's is 3), so the cache never empties in a measured run; the
  bound is the constant and the wholesale clear in `Wave::pages`.
* **`edit_transitions` is a debug-profile test.** The demand census is the same
  counter the release vehicle reports, but the two are not compared byte-for-byte
  here.
* **No independent reviewer exists.** Every claim above is author-verified; the
  reproducibility of R1–R8 on the named trees is the evidence.
