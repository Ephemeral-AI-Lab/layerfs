# verify-p1-7 — the two edit passes share one page memo

> **author-verified** (single-agent Phase 1: no independent reviewer exists; the
> evidence is the reproducibility of the commands below on the named trees).
> Round: [`receipt.md`](receipt.md). Tree: `974b525be`, arm [`after/`](after/);
> before arm [`../p1-8/after/`](../p1-8/after/) (tree `e9b4d1510`).

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 974b525be \| tar -x -C /tmp/verify-p17` | 0 | clean P1-7 tree |
| R2 | `cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples` | 0 | `edit_timing_c1` sha256 recorded in `after/artifacts.txt` |
| R3 | `…/edit_timing_c1` (no argument) | 0 | `nodes_read: 7`, `edit_nodes_read: 8`, `edited_root: b6dca354…` |
| R4 | the same three commands in `/tmp/p1-8-parent` (tree `e9b4d1510`) | 0 | `nodes_read: 9`, `edit_nodes_read: 10`, same root |
| R5 | `cargo +1.85.1 test … -p layerfs-content --test edit_localized a_page_two_passes` on R1 | 0 | `1 passed` |
| R6 | the same test file copied into R4 and run | 101 | one mapping page demanded **2** times ([`parent-test/log.txt`](parent-test/log.txt)) |
| R7 | `cargo +1.85.1 test --workspace --locked --no-fail-fast` on R1 | 0 | 455 passed / 0 failed |
| R8 | counter-only diff of `after/` against `../p1-8/after/` | 0 | only D27's `nodes_read` and `edit_nodes_read` differ |

## 2. Falsification answers

**2a — does the new test fail on the parent tree?** Yes: R6, with the census
naming the object and its two demands. The test uses only public API (`apply_edits`
and a provider), so copying the test file into the parent archive is the whole
falsification run.

**2b — did the counter move in the predicted direction and magnitude, and did
anything else move?** Predicted D27 `nodes_read` 9 → ≈7; measured **9 → 7**, with
`edit_nodes_read` 10 → 8 and every other frozen row and vehicle unchanged (R8). The
mechanism is visible in the demand trace: the root and one leaf change from two
demands each to one.

**2c — parity green and unchanged?** Green: the 34-test sealed-oracle set, every
`edits.*` row, `edit_noop`'s verdict and its zero-emission pin, and D27's root.
Unchanged in the sense that matters: the three test files the commit touches are
`edit_localized.rs` and `edit_reference.rs`, and each change is a **tightening**
(§5 of the receipt) — a new exact value plus the old value as an upper bound. The
sealed-oracle targets are untouched.

**2d — single-variable?** One mechanism (the shared memo and the comparison
cursor), its call sites, its new test, the two pin tightenings it forces, and the
architecture note. No unrelated change rides along.

**2e — error paths.** Probe each one:
* *The memo hit path validates.* A hit decodes under the caller's root context and
  re-checks level, logical length and extent count against the summary, exactly as
  the read path does — a page the memo holds cannot be accepted with a stale
  summary.
* *A cache overflow* empties the memo and re-reads: nothing is assumed present.
  The 4,096-edit ceiling case is the test that fills it.
* *A whole-file base* leaves the memo untouched and keeps the slice path.
* *An Equal verdict / a `Differs` verdict* are decided before any memo use.
* The full edit suite (`transitions`, `single`, `batch`, `model`, `noop`,
  `localized`, `bounds`, `reference`) and the read suite are green (R7).

**2f — elapsed as a gate?** No. The gates are `nodes_read` / `edit_nodes_read` (D27)
and the in-test demand census.

## 3. UNVERIFIED

* **Error ordering can move** (receipt §7.1). A demand the memo answers would
  otherwise have re-read a page, so an error that re-read would raise now surfaces
  at that demand. No test pins the failure point on a corrupt base, before or
  after; this is stated, not measured.
* **The 64-page ceiling is not reached by any frozen row.** D27's mapping tree is
  3 pages; the eviction path is exercised only by the 4,096-edit ceiling test.
* **The comparison's own window count does not move on any frozen row.** D27's
  replacement is 40,000 bytes = one window, so the "one descent for all windows"
  half of the change is review-verified plus the ceiling test, not measured by a
  frozen row.
* **No independent reviewer exists.** Every claim above is author-verified; the
  reproducibility of R1–R8 on the named trees is the evidence.
