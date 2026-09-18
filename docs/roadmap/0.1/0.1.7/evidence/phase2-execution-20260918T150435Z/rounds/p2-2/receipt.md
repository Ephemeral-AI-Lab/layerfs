# P2-2 receipt — one INSERT per bound chunk of a group's rows

> **Status:** Item receipt. Written once, from [`after/`](after/) (product commit
> `0a593084c`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../p2-5/after/`](../p2-5/after/), collected on `7db87bb8b`, this item's parent.
> **Terminal state: landed** — the statement count fell from one per row to one per
> bound chunk, exactly as V5's anchor predicted, with the group granularity stated.

## 1. The item

`seal_group` inserted each object row with its own statement. The engine takes a
bounded set of rows per statement, so the save paid one statement per row:

* `write::insert_objects` issues one multi-row `INSERT` per chunk and **reports the
  statements it issued**, which the owner charges to `statements` (V5's counter);
* `insert_chunk_rows` derives `k` from the connection's own limits -
  `min(SQLITE_LIMIT_VARIABLE_NUMBER / 7, SQLITE_LIMIT_SQL_LENGTH / 16, 128)` - read
  back through rusqlite's `limit`, never hardcoded. The 128 cap bounds the
  prepared-statement cache, one entry per distinct chunk size, not the engine;
* SQLite applies a multi-row `INSERT` atomically: a chunk's rows all land or none
  do, and a failure is the same engine/integrity error the single-row form
  produced. The attribution the per-row loop had *was* the statement; that is what
  is preserved, and no error path re-runs a chunk;
* `insert_object` and its loop are deleted.

## 2. The gate — statements `r` → `⌈r/k⌉`, commits unchanged

| Row | counter | before | after | predicted |
| --- | --- | ---: | ---: | --- |
| **D28** `c2.ceiling` (8,191 rows) | `statements` | 8,191 | **72** | one per chunk ✔ |
| **D28** | `inserted` / `commits` / `packs_created` / `pack_appends` / `reused` | — | **identical** | unchanged ✔ |
| **D29** `c2.small` (1,023 rows) | `statements` | 1,023 | **9** | ✔ |
| **X3** (D28's determinism re-run) | `statements` | 8,191 | **72** | identical to D28 ✔ |
| D1–D27, M1–M4, X1, X2, Y1–Y4 (37 steps) | every counter | — | **bit-identical** | unchanged ✔ |

```sh
python3 compare_arms.py rounds/p2-5/after rounds/p2-2/after
# steps compared: 40, differing: 3 -> D28, D29, X3, each only in `statements`
python3 pack_bytes_census.py rounds/p2-5/after rounds/p2-2/after   # 12 stores, identical
```

**Why 72 and not 64.** Rows are inserted one **sealed group** at a time, so the
total is `Σ_g ⌈rows_g / k⌉`, not `⌈rows / k⌉`: the ideal single-chunk figure at
`k = 128` is `⌈8191/128⌉ = 64`, and the group boundaries add 8 more statements.
Collapsing them would mean deferring a sealed group's rows past the group that
sealed them - a transaction-accounting change, not this item. The measured
derivation is confirmed by construction: `k = min(32766/7, 10⁹/16, 128) = 128` on
this build, and `Σ_g ⌈rows_g/128⌉ = 72` over the 21 groups the 8,191 leaves form.

`sqlite_master` is unchanged: the schema text is pinned in `sqlite/schema.rs` and
re-verified at every open, and the arms' 12 stores are byte-identical.

## 3. The item's tests

`tests/statement_batching.rs` (V5's file, which the plan assigns to both items):

* `a_save_inserts_its_rows_in_bounded_statements` — every row is in the engine,
  `statements < inserted`, and `statements ≥ ⌈inserted/128⌉`; **fails on the parent
  tree** (`statements == inserted` there);
* `the_statement_count_tracks_the_rows_and_not_a_constant` — updated to the
  post-batching reading; **fails on the parent tree** too;
* `the_insert_chunk_is_derived_from_the_engine_limits` — new: the chunk is read
  from the connection (≥ 1, ≤ 128, within the bind-variable limit).

```text
# on the 7db87bb8b archive, with this round's test file
test a_save_inserts_its_rows_in_bounded_statements ... FAILED
test the_statement_count_tracks_the_rows_and_not_a_constant ... FAILED
test result: FAILED. 0 passed; 2 failed
```

## 4. The eight checks (exit codes)

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 | PASS, 120 files |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 | OK |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 | OK |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 | clean |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 | **469 passed, 0 failed** (468 + the derivation case) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 | clean |
| 7 | `python3 tools/production_loc.py --files` | 0 | 19,503 core |
| 8 | `git diff --check` | 0 | clean |

Logs: [`after/checks/`](after/checks/). Parity 35/35 green and unchanged.

## 5. Production LOC

**19,455 → 19,503 (delta +48).** `layerfs-storage` 6,394 → 6,442. The plan's
estimate was +40..80.

## 6. Architecture documents (same commit)

`core/docs/architecture/10-counters.md` (`statements` now records what the landed
batching made it read, and why 72 rather than 64) and `13-physical-writing.md`
(§18.4's neighbour: the group's rows are written with one statement per bound
chunk, derived from the engine's limits, atomic per statement, schema untouched).

## 7. Clean-tree reproduction

```sh
git archive 0a593084c | tar -x -C /tmp/verify-p2-2      # + this round's client
(cd …/client && cargo +1.85.1 build --release --offline --locked)
…/phase0client c2 8191 default    # statements 72, inserted 8191, commits 31
…/phase0client c2 1023 default    # statements 9,  inserted 1023, commits 4
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test statement_batching
```

Falsification answers: [`verify-p2-2.md`](verify-p2-2.md).
