# P2-7 receipt — consolidation adopts its newest run

> **Status:** Item receipt. Written once, from [`after/`](after/) (product commit
> `b2abb6455`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../p2-6/after/`](../p2-6/after/), collected on `57c4cf3bd`, this item's parent.
> **Terminal state: landed** — the aliasing proof held, so the copy is gone and the
> counters moved where the plan predicted.

## 1. The item

`consolidate()` moved every live run out of its tier and merged them into one. The
first step was a **copy** of the newest run, whose own comment gave the reason:

```rust
/// Copies one run into a fresh handle so a merge never aliases its own input.
fn copy_run(...)
```

That copy re-read and re-wrote the whole newest run on every consolidation to
guard against a merge writing where it reads. `merge_runs` appends exclusively to
a run it creates (`let mut handle = backing.create_run()?;` — the inputs stay
borrowed), so the guard was removable **once the property was proved**; two
research reports had flagged the aliasing comment UNKNOWN, and the plan made the
proof a gate rather than an argument. `copy_run` and its call are deleted.

## 2. The gate — the aliasing proof, and the counters that moved

| Row | counter | before | after | predicted |
| --- | --- | ---: | ---: | --- |
| **D26** `order.forced64` | `runs_created` | 124 | **123** | one fewer copy ✔ |
| **D26** | `rows_written` | 25,760 | **25,632** | −the copied run's 128 rows ✔ |
| **D26** | `rows_read` | 25,809 | **25,681** | −the same 128 rows ✔ |
| **D26** | `merges` / `spilled` / `emitted` / pages / peaks / `rows_touched` / `final_values` | — | **identical** | unchanged ✔ |
| **X1** (D26's determinism re-run) | every counter | — | **identical to D26** | ✔ |
| D1–D25, D27–D29, M1–M4, X2, X3, Y1 (35 steps) | every counter | — | **bit-identical** | unchanged ✔ |

```sh
python3 compare_arms.py rounds/p2-6/after rounds/p2-7/after
# steps compared: 37, differing: 2 -> D26 and X1, each by the three counters above
python3 pack_bytes_census.py rounds/p2-6/after rounds/p2-7/after
# 12 stores, identical (pack rows, pack bytes, object rows, pack-body sha256)
```

D25 (`order.default`, the same shape at 4,096 pending) is unchanged because it
never spills, so it never consolidates — the item only moves work that existed.

## 3. The aliasing proof

`core/crates/layerfs-content/tests/filesystem_ordering_consolidate.rs` (new test
binary):

1. twelve spills of 64 rows leave several runs live;
2. the pre-consolidation row stream is captured (`visit_newest_first`);
3. **every run that exists is sealed** — the recording backing refuses an append
   into a run created before the seal, so any write into an input fails the
   consolidation itself rather than being noticed as a changed byte;
4. `consolidate()` must succeed; the row stream after must equal the stream before;
5. a **control** in the same case: a handle taken before the seal must refuse an
   append after it, so a green run is not a dead detector.

**A second control shows the case fails when the property does not hold.** In a
scratch tree, one `append` was aimed at a pre-existing run inside `consolidate`:

```sh
# /tmp/p2-7-control: `for mut run in sources { run.handle.append(&[0_u8; 96])?; … }`
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content \
  --test filesystem_ordering_consolidate
# test result: FAILED. 0 passed; 1 failed
```

The case lives in its own binary rather than in `filesystem_ordering_scan.rs` as
the plan sketched: that binary carries an allocation-budget probe over a **global**
allocator, and the new case's parallel allocations disturbed it (15 against a bound
of `live_tiers * 2 + 4` = 8). The pinned case's bound was not loosened; the new case
moved, and the reason is in the test's own documentation.

## 4. The eight checks (exit codes)

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 | PASS, 120 files |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 | OK |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 | OK |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 | clean |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 | **465 passed, 0 failed** (464 + the new case) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 | clean (the first run caught an unused import left in `filesystem_ordering_scan.rs`; removed, re-run) |
| 7 | `python3 tools/production_loc.py --files` | 0 | 19,346 core |
| 8 | `git diff --check` | 0 | clean |

Logs: [`after/checks/`](after/checks/). Parity 35/35 green and unchanged.

## 5. Production LOC

**19,368 → 19,346 (delta −22).** `layerfs-content` 12,320 → 12,298. The plan's
estimate was −15..0; the delta is larger because `copy_run` was a whole function
plus its call and its doc block.

## 6. Architecture document (same commit)

`core/docs/architecture/04-filesystem.md` gains the paragraph under the ordering
account: consolidation adopts its newest input, why the copy was removable, how the
property is proved, and what it removes on the forced-64 probe.

## 7. Clean-tree reproduction

```sh
git archive b2abb6455 | tar -x -C /tmp/verify-p2-7
(cd …/client && cargo +1.85.1 build --release --offline --locked)
…/phase0client order 4000 2000 64
# order pairs 2000 pending 64 spilled 3968 elapsed_ns <t> rows_read 25681 rows_written 25632
# runs_created 123 merges 61 … emitted 14 … objects_read 98 read_waves 5
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content \
  --test filesystem_ordering_consolidate
# test result: ok. 1 passed
```

Falsification answers: [`verify-p2-7.md`](verify-p2-7.md).
