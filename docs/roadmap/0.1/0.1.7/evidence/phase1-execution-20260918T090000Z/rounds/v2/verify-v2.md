# verify-v2 — the edit-path memory and shape vehicles

> **author-verified** (single-agent Phase 1: no independent reviewer exists; the
> evidence is the reproducibility of the commands below on the named tree).
> Round: [`receipt.md`](receipt.md). Tree: `582dea9dd`, arm [`after/`](after/)
> with `artifacts.txt`.

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 582dea9dd \| tar -x -C /tmp/verify-v2/tree` | 0 | clean V2 tree |
| R2 | `cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples` | 0 | both examples built from the archive |
| R3 | `edit_memory_probe` from the archive | 0 | `baseline_live_bytes 132175`, `peak_live_bytes 394503`, `peak_delta_bytes 262328`, root `8ddfe36c…` — identical to `after/logs/M1` |
| R4 | `edit_timing_c1 --case delete` / `--case shrink` from the archive | 0 ×2 | `nodes_read 4` / `nodes_read 11`, roots `7d3eb265…` / `4a45d246…` — identical to `after/logs/M2`, `M3` |
| R5 | `edit_timing_c1` (no argument) from the archive | 0 | D27's fields, plus `case: default` |

## 2. Falsification answers

**2a — does the new observable fail to exist on the pre-item tree?** Yes, and
that is the gap: `v1/after/logs/D27-edit-timing-c1-nodes-read.log` (before V2) has
no `case:` line and no `--case` argument exists, so `edit_timing_c1 --case shrink`
on that tree exits **2** with `unsupported argument --case`; and no binary named
`edit_memory_probe` exists there at all, so P1-14 had no memory row. The
pre-item failure is "the observable is absent", recorded in Phase 0 as
`NOT_EXPOSED`-class gaps.

**2b — did any counter move?** None may: V2 is vehicle-only. Checked across all
29 frozen rows between the `v1` arm (before) and this arm (after) — 0
counter-bearing lines differ; the only differences are timing figures. A movement
would have been the finding.

**2c — parity green and unchanged?** V2 touches two example files and
`collect.py`; `git diff --stat fe86f3bdd..582dea9dd -- '*tests*'` is empty. The
34-test sealed-oracle set is untouched (re-run green on the C1 tree,
[`../c1-rebaseline/verify-c1.md`](../c1-rebaseline/verify-c1.md) §1).

**2d — single-variable?** Yes: two examples + the driver's `v2` set + the round
directory. No product source, no test.

**2e — error paths.** `edit_timing_c1 --case nope` exits **2** with
`edit_timing_c1: unsupported case nope`; `--case` with no value exits 2 with
`--case needs a value`; no argument keeps the frozen shape. The memory probe has
no arguments and fails closed (`expect`) if the fixture or the edit fails — the
probe's own `Timing::disabled` means it cannot be confused by a clipped timing
tree.

**2f — elapsed as a gate?** No. M1's gate is `peak_delta_bytes` (a byte count),
M2/M3's are `nodes_read`/roots. Every timing figure in the receipt is labelled
diagnostic, and `X3`/`X4` show the counters reproduce while the clock does not.

## 3. UNVERIFIED

* **`peak_delta_bytes` is a requested-byte ledger, not RSS and not a cgroup
  reading.** It cannot see allocator-internal rounding beyond what `Layout` was
  asked for, and it is one process, one sample. P1-14's receipt must state the
  same limit.
* **The probe's `peak` includes allocations the example itself makes inside the
  measured region** (the `demanded` vector, the `Collector`'s object). They are
  identical in both arms by construction, but they are not subtracted: the gate
  is the whole-region delta, as §3 of the receipt states.
* **No `edit_timing_c1` case exercises a **no-op** edit or a malformed stream.**
  V2 does not add one; the zero-emission contract stays pinned by
  `edit_noop.rs`, not by this vehicle.
