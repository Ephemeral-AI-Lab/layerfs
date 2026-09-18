# verify-p1-1 — branch children in one wave

> **author-verified** (single-agent Phase 1: no independent reviewer exists; the
> evidence is the reproducibility of the commands below on the named trees).
> Round: [`receipt.md`](receipt.md). Tree: `32eda6f29`, arm [`after/`](after/);
> before arm [`../p1-2/after/`](../p1-2/after/) (tree `9ec299f13`).

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 32eda6f29 \| tar -x -C /tmp/verify-p11/tree` | 0 | clean P1-1 tree |
| R2 | `cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples` + the client in `…/client` | 0 | both arms' artifacts |
| R3 | `phase0client order 4000 2000 4096` from the archive's own client | 0 | `read_waves 5`, `ino_scratch 709388`, `dir_pages_read 17`, `ino_pages_read 81`, `objects_read 98` — identical to `after/logs/D25` |
| R4 | `phase0client order 4000 2000 64` | 0 | same, with `spilled 3968` / `rows_read 59007` unchanged |
| R5 | `filesystem_timing_c1 --case directory-update` / `subtree-remove` | 0 ×2 | `objects read 6 waves 3`, roots `820dcf46…` / `3855544b…` — identical to `after/logs/D2`, `D5` |
| R6 | `cargo +1.85.1 test … -p layerfs-content --test filesystem_bounds` on the archive | 0 | 15 passed |

## 2. Falsification answers

**2a — do the new tests fail on the parent tree?** Yes, both, reproduced by
exporting `9ec299f13`, copying **only** `filesystem_bounds.rs` into it and running
the suite:

```text
a_wide_branch_page_reads_its_children_in_one_wave  FAILED  left: [] right: [80]
    (the wave list is [1 … 1, 14, 1 … 1, 32, 32, 16]: the branch is split three ways)
narrowing_keeps_a_small_scratch_working             FAILED  "the batch narrowed: 32 against 32"
```

**2b — did the counter move in the predicted direction and magnitude, and did
anything else move?** Predicted: `read_waves` down, `peak_scratch_bytes` up
(bounded), everything else identical. Measured on D25/D26: `read_waves` 7 → 5,
`ino_scratch` 349,820 → 709,388, and **53 of 55 fields identical** on the same
line; every other frozen row is unchanged (a diff of all 29 D-rows + 3 M-rows
between the arms reports exactly those two rows and those two fields). The rise is
inside the lease by construction: 256 × 8,280 + decode ≈ 2.08 MiB against 4 MiB.

**2c — parity green and unchanged?** The test diff in this commit is
`core/crates/layerfs-content/tests/filesystem_bounds.rs` only — not one of the
seven sealed-oracle targets, which are untouched and were re-run green on the C1
tree ([`../c1-rebaseline/verify-c1.md`](../c1-rebaseline/verify-c1.md) §1).

**2d — single-variable?** Three files: `sorted/page.rs` (the constant and its
documentation), `filesystem_bounds.rs` (the authorized bound, one generalized
assertion, three tests) and `04-filesystem.md` (the same-commit doc update). An
earlier staging of this commit accidentally included the V4 experiment's vehicle
prints; the commit was amended before landing so the vehicle files are untouched
(`git show --stat 32eda6f29` lists exactly those three files).

**2e — error paths.** The narrowing path is the item's own error-adjacent
behaviour and has its own test (`narrowing_keeps_a_small_scratch_working`); the
refusal path — a lease too small for even one child — is already pinned by
`filesystem_sorted::a_tiny_supported_budget_works_or_refuses_before_allocating`,
green in the workspace run. `an_unsorted_duplicate_or_repeating_change_is_refused_once`
and the ordering/partition suites are green. No malformed-input path changes:
this commit reads the same pages, authenticated and context-checked exactly as
before.

**2f — elapsed as a gate?** No. The gates are `read_waves`, `ino_scratch`, the
wave shapes and the roots; `X1`/`X2` reproduce every counter while the clock moves
175.0 ms → 157.5 ms.

## 3. UNVERIFIED

* **The declined `list_after` half is argued, not measured.** §5.1 of the receipt
  gives the over-read arithmetic from the code (a bounded listing stops on the
  first entry over its bounds; the branch page carries no per-child summaries).
  I did not implement it to measure the over-read, because implementing it would
  change the listing's read set — the thing the decline is about. An owner who
  wants it measured can ask for it as its own item.
* **The deferred `patch.rs` half** has no frozen anchor and no measurement; it is
  recorded, not verified.
* **No cross-host or elapsed claim.** One host, one process, one sample per row.
* **`peak_scratch_bytes` is the operation's own declared reservation**, not RSS;
  the rise it reports is the batch's reservation, and it stays inside the lease by
  construction rather than by measurement of resident memory.
