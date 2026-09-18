# V5 receipt — the statement counter for object-row inserts

> **Status:** Item receipt. Written once, from [`after/`](after/) (product commit
> `464807178`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../p2-0/after/`](../p2-0/after/), collected on `ed5ab5d95`, this item's parent.
> **Terminal state: landed with a correction** — the instrument landed as
> authorised; the plan's reading of *which* statements it counts is refuted by
> measurement and the counter is scoped to the class `P2-2` changes (§4).

## 1. The item

`P2-2`'s claim — "8,191 single-row statements become 64" — was unobservable:
`OutcomeCounters` (`cas/owner.rs`) and `SaveOutcome` (`cas/store.rs`) carry
`inserted`, `reused`, `packs_created`, `pack_appends`, `transactions` and
`commits`, and every one of those can stay identical while the statement count
falls. The instrument:

* `OutcomeCounters.statements` — one `u64`, documented as the `INSERT` statements
  issued for object rows (**statements, not rows**: a multi-row `INSERT` of `k`
  rows is one statement; `inserted` counts rows);
* the charge comes from the writer: `write::insert_object` returns the number of
  statements it executed (`Ok(1)` today) and `seal_group` adds that report, so a
  batched insert reports its chunk count the same way and the counter cannot drift
  from the engine's work;
* `SaveOutcome.statements` surfaces it, with the `From<OutcomeCounters>` mapping;
* the `c2` probe rows print it beside `inserted`/`commits`.

Scope is the `objects` insert alone: pack writes, value-group inserts and
transaction statements are already `packs_created`/`pack_appends`, `pool.groups`,
`transactions` and `commits`.

## 2. The gate — statements ≈ rows on `c2.ceiling`

| Row | counter | before (`p2-0/after`) | after (`v5/after`) | predicted |
| --- | --- | ---: | ---: | --- |
| **D28** `c2.ceiling` (8,191 rows) | `inserted` (the only row-count the before arm could print) | 8,191 | `statements` **8,191** | statements ≈ rows ✔ |
| **D29** `c2.small` (1,023 rows) | `inserted` | 1,023 | `statements` **1,023** | ≈ rows ✔ |
| D28/D29 | `packs_created` / `pack_appends` / `commits` / `pool_groups` | 33 / 8,169 / 31 / 8,191 | identical | no other counter moves ✔ |
| D1–D27, M1–M4, X1/X2 (33 steps) | every counter | — | **bit-identical** | no other counter moves ✔ |

```sh
python3 compare_arms.py rounds/p2-0/after rounds/v5/after
# steps compared: 35, differing: 2 -> D28, D29, each differing only by the added field
```

Exactly two of the 35 measurement steps differ, and each differs only by the added
`statements` field: the instrument changed no work. The counter is **proven live
by a control**, not by inspection:

* D28 and its labelled determinism re-run `X3` (`X3-c2-ceiling-repeat`) agree on
  every counter, `statements 8191` included;
* the two sizes charge proportionally (8,191 and 1,023 statements for 8,191 and
  1,023 rows), so the value is not a constant;
* the new external tests pin the charge against the engine's own table:
  `SELECT COUNT(*) FROM objects` equals the charged statements, an all-reuse save
  charges **0**, and a larger file charges exactly the rows it added.

## 3. The item's tests

`core/crates/layerfs-storage/tests/statement_batching.rs` (new; the plan's §5
file, which `P2-2` extends):

* `a_save_charges_one_statement_per_inserted_row` — `statements == inserted`, and
  equal to the engine's own row count read on an external connection;
* `the_statement_count_tracks_the_rows_and_not_a_constant` — the two controls.

Both fail on the parent tree by **not compiling**, which is the honest form of
"the new test fails on the parent tree" for an instrument that adds a field:

```text
error[E0609]: no field `statements` on type `SaveOutcome`
  --> crates/layerfs-storage/tests/statement_batching.rs:38:17   (… 38, 42, 57, 63)
```

## 4. The correction — which statements the counter counts

The plan's §2.2 sketch is "a SQL-statement counter … charge at the two insert
entry points (`sqlite/write.rs:76` `insert_pack`, `:100` `insert_object`)", with
"statements ≈ rows" as V5's before-anchor and "statements `r` → `⌈r/k⌉`" as
`P2-2`'s gate. Measured on `c2.ceiling` **before** choosing the charge, that
reading cannot hold: the same save issues

| statement class | count at 8,191 rows |
| --- | ---: |
| `INSERT INTO objects` (one per row) | 8,191 |
| `INSERT INTO object_packs` / `UPDATE object_packs` | 33 + 8,169 = 8,202 |
| `INSERT INTO object_value_groups` | 8,191 |
| `BEGIN IMMEDIATE` / `COMMIT` (31 + 32 transactions) | ~63 |

so a whole-save statement counter would read **≈ 24,647** against 8,191 rows —
neither "≈ rows" nor reducible to `⌈r/k⌉` by `P2-2` (which batches only the row
inserts). The counter is therefore scoped to, and named for, the statement class
`P2-2` actually changes, with the other classes left to the counters that already
report them (`packs_created`, `pack_appends`, `pool.groups`, `transactions`,
`commits`). With that scope the gate is exact rather than approximate: 8,191 rows
read `statements 8191`, and `P2-2` must make them read 64.

## 5. The eight checks (exit codes)

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 | PASS, 120 files |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 | OK |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 | OK |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 | clean |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 | **458 passed, 0 failed** (456 + the two new) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 | clean |
| 7 | `python3 tools/production_loc.py --files` | 0 | 19,318 core |
| 8 | `git diff --check` | 0 | clean |

Logs: [`after/checks/`](after/checks/). The sealed-oracle parity set is green and
unchanged: 35/35, no test file re-pinned (`git diff ed5ab5d95..464807178 -- '*tests*'`
adds `statement_batching.rs` and touches nothing else).

## 6. Production LOC

**19,314 → 19,318 (delta +4).** `python3 tools/production_loc.py --detail`:
`layerfs-storage` 6,231 → 6,235. The delta is one field, one charge, one writer
report and one mapping line; the plan's +15..25 estimate counted the doc comments,
which production LOC excludes.

## 7. Architecture document (same commit)

`core/docs/architecture/10-counters.md`: the `SaveOutcome` field row gains
`statements`, and §15.3 gains the paragraph that says what it counts, what it
deliberately does not, and why `inserted` cannot substitute for it. The pin note
records the addition as #178 **V5**.

## 8. Clean-tree reproduction

The product tree of `464807178` plus this round's client (whose source is
committed with this round; the binary the arm ran is hashed in
`after/artifacts.txt`):

```sh
git archive 464807178 | tar -x -C /tmp/verify-v5          # + this round's client/src/main.rs
cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples   # exit 0
(cd …/client && cargo +1.85.1 build --release --offline --locked)                             # exit 0
…/phase0client c2 8191 default
# c2 rows 8191 canonical_bytes 1023875 cache_arm default elapsed_ns <t> inserted 8191 reused 0
# packs_created 33 pack_appends 8169 commits 31 statements 8191 full_records 8191 …
…/phase0client c2 1023 default
# … inserted 1023 … commits 4 statements 1023 … pool_groups 1023 …
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test statement_batching
# test result: ok. 2 passed; 0 failed
```

Falsification answers: [`verify-v5.md`](verify-v5.md).
