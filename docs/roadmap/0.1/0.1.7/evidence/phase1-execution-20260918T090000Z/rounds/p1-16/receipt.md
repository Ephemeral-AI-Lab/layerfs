# P1-16 receipt — the pending-ceiling dial, documented and bounded

> **Status:** Item receipt. Written once, from [`after/`](after/) (commit
> `84ca5c851`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../p1-12/after/`](../p1-12/after/), collected on `ca0cf985a`, this item's
> parent. P1-16 is **documentation + one boundary test**: production LOC delta 0,
> no product source, and **no default change** — re-defaulting
> `DEFAULT_MAXIMUM_PENDING` is an owner ruling on #178, not the implementer's.

## 1. What was documented

| Document | What it now states |
| --- | --- |
| `core/docs/architecture/04-filesystem.md` §5.5 | the dial is `FilesystemResources.maximum_pending_records`; the spill-free bound is `floor(ordering_bytes / (2 × ROW_BYTES))` = **349,525 rows** under the default 64 MiB ceiling; the ×2 rationale (a pending row plus the run it becomes); the two caveats — the account owns encoded-row equivalents, not heap (~42 MiB of real `BTreeMap` heap is invisible at the top), and `maximum_touched_serials` does not bind before the pending bound; **the default is not changed by the description** |
| `core/docs/architecture/06-limits.md` §7.4 | the same arithmetic beside the spill line it qualifies |

## 2. The boundary test (the deliverable that is not prose)

`filesystem_ordering::a_high_pending_ceiling_runs_spill_free_to_the_byte_bound`
**calibrates the shape first** — the same operation under a generous ceiling, whose
`peak_pending` becomes the row count — and then walks the bound:

| Arm | resources | verdict |
| --- | --- | --- |
| calibration | 64 MiB, P = 4,096 | 0 spilled, `peak_pending = rows` (102 for 100 files) |
| at the bound | `ordering_bytes = 2 × 96 × rows`, P = `rows` | **0 spilled, 0 runs, `peak_pending == rows`**, root identical to calibration |
| one row less, 4× ceiling | same map, wider ceiling | **spills**, root identical — planned work, not a different result |
| one row less, at the bound | same map, tight ceiling | **refused** with `ObjectLimitExceeded`, `limit == ordering_bytes` |

Two of the plan's assumptions were corrected by writing it: a shape's row count is
**not** its file count (100 files → 102 rows here), and a spill reserves its run
**and** a merge output, so a "spills" arm needs headroom the bound itself cannot
give. Both are recorded rather than hidden behind a magic constant.

## 3. Before → after on the frozen set

**Nothing moved.** A counter diff of all 29 D-rows and the three M-rows between
[`../p1-12/after/`](../p1-12/after/) and [`after/`](after/) is **empty** — the
commit is documentation and a test, and the receipt says so instead of dressing a
zero movement as a result. The item's evidence is the boundary test and the
documents, which is what its plan asks for.

## 4. The eight checks (exit codes)

| # | Command | Exit |
| --- | --- | ---: |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 (116 files) |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 (6 OK) |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 (17 OK) |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 (**450 passed, 0 failed**) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 |
| 7 | `python3 tools/production_loc.py --files` | 0 |
| 8 | `git diff --check` | 0 |

Parity: the test diff is `filesystem_ordering.rs`; the seven sealed-oracle targets
are untouched and green.

## 5. Production LOC

`P1-16 | 0` was the plan's estimate; the actual is **0**.
`core` **18,971 → 18,971 (delta 0)**; `crates/` reference 65,417; combined 84,388.
Method: `tools/production_loc.py` over the first parent (`ca0cf985a`, via
`git archive`) and the staged tree.

## 6. Acceptance

- [x] The dial, its arithmetic and its caveats are documented in the two named documents
- [x] One boundary test pins the arithmetic in miniature, including the refusal shape
- [x] **No default changed** — the owner ruling is asked for, not taken
- [x] The frozen set is measured and nothing moved (stated as such)
- [x] Eight checks green; parity untouched; LOC disclosed
