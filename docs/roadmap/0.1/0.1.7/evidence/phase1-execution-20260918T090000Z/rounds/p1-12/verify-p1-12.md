# verify-p1-12 — a page keeps a running total of its row widths

> **author-verified** (single-agent Phase 1: no independent reviewer exists; the
> evidence is the reproducibility of the commands below on the named trees).
> Round: [`receipt.md`](receipt.md). Tree: `9b4eff169`, arm [`after/`](after/);
> before arm [`../p1-5/after/`](../p1-5/after/) (tree `a3dbeef88`).

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 9b4eff169 \| tar -x -C /tmp/verify-p12` | 0 | clean P1-12 tree |
| R2 | `cargo +1.85.1 test --release --offline --locked --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_sorted page_widths` | 0 | `1 passed` — the recorded partition reproduces |
| R3 | the same with the `append_entry` increment removed | 101 | the partition assertion fails (pages never split) |
| R4 | `filesystem_timing_c1 --case directory-update` from the archive | 0 | `directories: … scratch peak 67716`, `inodes: … 67652` — identical to `after/logs/D2` |
| R5 | the client's `order 4000 2000 64` | 0 | `dir_scratch 376158`, `ino_scratch 709396`, `rows_read 27777` — identical to `after/logs/D26` |

## 2. Falsification answers

**2a — does the new test fail on the pre-item tree?** The test pins the partition
that the parent tree also produces, so it does **not** fail there: P1-12 changes
*how* the decision is computed, not *what* it decides, and that is the point — the
canonical partition must be identical. The falsification that matters is the
opposite direction: with the running total not maintained (the increment removed)
the test fails, which is the failure mode the item could introduce. Stated
plainly rather than presented as a parent-tree failure.

**2b — did the counter move in the predicted direction, and did anything else
move?** There is no counter for the work; the movement that *did* appear is the
declared scratch charge, +8 B per page slot (D2/D3/D4/D5 and the D25/D26 scratch
peaks), which the plan's risk note denied and its memory target predicted. Every
other counter, every root and every emitted byte is identical. A movement
anywhere else would have been the finding.

**2c — parity green and unchanged?** The test diff is `filesystem_sorted.rs`; the
seven sealed-oracle targets are untouched and green. The sorted suite's own
partition test (`leaf_and_branch_boundaries_are_partitioned_canonically`) and the
sealed reference roots are the strongest available pins for this item and both are
green.

**2d — single-variable?** Three files: `sorted/page.rs` (the field and its
maintenance), `sorted/merge.rs` (the fill test and the drain) and
`filesystem_sorted.rs` (the test), plus the same-commit doc paragraph. No
boundary, format or named bound moves.

**2e — error paths.** `append_entry`'s `checked_add` fails closed with
`LengthOverflow` rather than wrapping, and its capacity path is unchanged; the
`ObjectLimitExceeded` refusal when a page is already at `F::page_items` is
untouched and pinned by the sorted suite. The split's `nearest_half` input is
still built from the rows themselves, so a wrong running total cannot silently
change a split *decision* without changing the partition the test pins.

**2f — elapsed as a gate?** No elapsed figure is used anywhere in this round.

## 3. UNVERIFIED

* **The O(k²) → O(k) time claim is not measured.** No counter reports CPU work in
  a page fill, and the elapsed column is diagnostic-only; the claim is an
  arithmetic argument over the loop (`push` no longer iterates `page.entries`),
  and the receipt says so instead of quoting a timing.
* **The drain subtraction is not observable on these fixtures** (§2 of the
  receipt); it is kept for the invariant, not for a measured effect.
* **`Page` grows by one `usize`**, which raises every page slot's declared charge
  by 8 bytes; the effect on a *whole-operation* scratch ceiling is not measured
  beyond the frozen rows above.
