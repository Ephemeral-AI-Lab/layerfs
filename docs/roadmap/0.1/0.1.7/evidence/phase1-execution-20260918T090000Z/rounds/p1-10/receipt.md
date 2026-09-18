# P1-10 receipt — the ordering state carried out of `touched_serials`

> **Status:** Item receipt. Written once, from [`after/`](after/) (commit
> `8327f87bb`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../p1-16/after/`](../p1-16/after/), collected on `9c0cb6f9e`, this item's parent.

## 1. The item

`zero_count_serials` asked the reducer for each touched serial's state one at a
time (`reducer.state(serial)`), and every serial the runs held paid a run read for
it — a second full pass over the consolidated run immediately after
`touched_serials` had read every row to build the serial list. The state now
travels with the serial out of that visit: `touched_serials` returns
`Vec<(u64, PendingState)>`, and the caller derives the counts from it. `state()`
keeps its meaning (shared `state_of` mapping) for its other caller (`release.rs`).

**Public-API and ceiling change, named:** the collection is `(u64, PendingState)`
per touched inode, so `FilesystemResources::maximum_touched_serials` charges 16 B
per serial instead of 8 — the divisor moves from `ordering_bytes / 8` to
`/ 16`. A caller with a tight ordering ceiling is therefore refused at half the
serials it was before. That is owner-visible and stated, not slipped in.

## 2. Before → after on the frozen set

| Row | counter | before | after | predicted |
| --- | --- | ---: | ---: | --- |
| **D26** `order.forced64` | `runs.rows_read` | **27,777** | **25,809** | −`serials_scanned` (2,001) ✔ |
| D26 | `serials_scanned` / `rows_written` / `runs_created` / `merges` / peaks | 4,001 / 25,760 / 124 / 61 | identical | unchanged ✔ |
| D25 `order.default` | every counter | 0 spills / 0 rows | **identical** | no run to re-find ✔ |
| D1–D29 except D26 | every counter | — | **identical** (0-row diff) | unchanged ✔ |

The removed reads are 1,968 against the 2,001 serials scanned: the re-pass is
gone, and the small difference is the serials the pending map still held (a
pending hit never charged a run read). Roots and emitted bytes are identical.

## 3. The item's test

`filesystem_ordering::carried_state_removes_the_zero_count_re_pass`: a spilling
reducer (pending 8, 200 serials, a real `RecordingBacking`), then

* the carried states derive every count with **zero** additional run reads
  (`runs.rows_read` unchanged across the whole derivation), and
* the contrast that keeps it from being a tautology: one `state()` lookup for a
  serial the runs hold still **charges** a read — which is what the old derivation
  paid once per serial.

The test is API-shaped, so it cannot compile against the parent tree; the
parent-tree evidence for the movement is the D26 measurement above (the plan's own
"before" row) plus the plan's derivation of the re-pass.

## 4. The eight checks (exit codes)

| # | Command | Exit |
| --- | --- | ---: |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 (116 files) |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 (6 OK) |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 (17 OK) |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 (**451 passed, 0 failed**) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 |
| 7 | `python3 tools/production_loc.py --files` | 0 |
| 8 | `git diff --check` | 0 |

Parity: the test diff is `filesystem_ordering.rs`; the seven sealed-oracle targets
are untouched and green. The suites that pin this subsystem's semantics —
threshold parity (spilling and non-spilling arms reach the same root), the hardlink
and release cases, and the ceiling-enforcement case — are green with the carried
states.

## 5. Determinism

`X1` (`order.forced64` repeat) is bit-identical to its gate sample on every
counter, `rows_read 25,809` included; `X2` likewise. `elapsed_ns` moved
135.2 ms → 173.1 ms between the arms (and 135–179 ms across repeats) and is
diagnostic-only.

## 6. Production LOC

`P1-10 | +50..+90` was the plan's estimate; the actual is **+3**
(`references/reduce.rs` 469 → 472; `update.rs` and `input.rs` changed
line-neutrally).
`core` **18,971 → 18,974 (delta +3)**; `crates/` reference 65,417; combined
84,388 → 84,391. Method: `tools/production_loc.py` over the first parent
(`9c0cb6f9e`, via `git archive`) and the staged tree.

## 7. Acceptance

- [x] The counter moved by the predicted amount (D26 `rows_read` 27,777 → 25,809 ≈ −`serials_scanned`)
- [x] No other counter moved (0-row diff outside D26; D25 identical); roots identical
- [x] The new test pins the carried-state derivation at zero reads and the contrast
- [x] The public-API and ceiling-divisor change is named in the commit and here
- [x] Eight checks green; parity untouched; determinism labelled; LOC disclosed
