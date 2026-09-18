# Phase 2 closing pass — the completion audit of a finished phase

> **Status:** Closing record for Phase 2 of
> [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178), written after the
> last box. Tree: `3cfbaf512` (docs) / `0a593084c` (product), i.e. the final tree.
> Contract: [`CONTRACT.md`](CONTRACT.md). Rounds: [`rounds/`](rounds/).
> **Single agent, author-verified**: no second reviewer, no subagent, no external
> coding agent touched this phase; every number below names its tree and its
> command.

## 1. Every item, prediction against measurement

`prediction` is the plan's, `measured` is the frozen set (or the round's nominated
row where stated). "Identity" means the plan predicted no counter movement.

| item | prediction | measured | state |
| --- | --- | --- | --- |
| **P2-0** split `cas/owner.rs` | every counter bit-identical; relocaton | 35/35 steps identical; `move_check` 0 code lines lost; LOC **+50** | landed (correction: one guard path) |
| **V5** statement counter | statements ≈ rows on `c2.ceiling` | **8,191 = 8,191**; 33/35 steps identical, 2 differ only by the added field | landed (correction: scope) |
| **V6** group-decode counter | decodes > distinct groups | D21–D24: **2 decodes / 1 group**; 25/35 identical | landed |
| **V7** save-cache observability | spill counter live, then read on the save's connection | profile read on the save's connection (4 pragmas); **spill half blocked** (no safe FFI path); control: 15,588 spills at 8 pages | blocked (profile half landed) |
| **P2-8** running total | `packs_created`/`pack_appends` and emitted bytes identical | **37/37 steps identical**, 12/12 stores byte-identical; boundary case moved | landed |
| **P2-6** hash once per wave | one hash per requested object; tamper detection unchanged | structural (no counter exists); guard fails on the parent; 37/37 identical | landed |
| **P2-7** drop `copy_run` | `rows_written`/`runs_created` move; bytes identical | **`runs_created` 124 → 123, `rows_written` 25,760 → 25,632**; 12/12 stores identical | landed |
| **P2-4** decoded-group cache | decodes → distinct groups | D21–D24 **2 → 1**; 37/37 otherwise identical | landed (correction: cache scope) |
| **P2-5** batched presence + one reader | one presence query per wave | nominated rows **64/512/1,024/4,096 → 1/1/2/8**; frozen set D22 **0 → 1** (a cost) | landed |
| **P2-2** multi-row INSERT | statements `r` → `⌈r/k⌉`; `commits` unchanged | **8,191 → 72**, `commits` 31 unchanged; `⌈8191/128⌉ = 64` not reached (group granularity) | landed |
| **P2-1** `cache_size`/`cache_spill` | A/B; may be declined | profile changed, **every counter identical**; P0-2's 0 spills | **measured-and-declined** |
| **P2-3** `locking_mode` | commits/statements/wall on the same save | every counter identical; **3 pinned visibility cases fail** | **measured-and-declined** |

Two predictions were refuted and the receipts say so: `P2-2`'s exact `⌈r/k⌉`
(group-at-a-time insertion costs 8 statements) and `P2-5`'s "no frozen movement"
(the wave seed adds one query on D22). `P2-1`/`P2-3` were authorised by ruling and
closed declined on measurement, which ruling 3 and the plan explicitly allow.

**Instruments added beyond the plan's V5–V7:** `V8`, the presence-query counter, in
its own commit (`12527f477`). `P2-5`'s stated gate ("one presence query per wave")
had no counter — the plan's own row named `edges`/`pages`, which count dependency
edges and locator pages. Under ruling 2's instrument class, the counter landed
before the item it gates, with its own receipt.

## 2. Final-tree collection

```sh
python3 collect.py closing-pass final all
python3 compare_arms.py rounds/p2-2/after rounds/closing-pass/final
# steps compared: 40, differing: 0
python3 pack_bytes_census.py rounds/p2-2/after rounds/closing-pass/final
# 12 stores compared across 2 arms, identical: True
```

The frozen set re-collected on the final tree is **bit-identical** to the last
item's arm, including the nominated `Y1`–`Y4` rows: the phase's last measurement
reproduces. Arm: [`rounds/closing-pass/final/`](rounds/closing-pass/final/)
(`commands.tsv`, `logs/`, `artifacts.txt`, `checks/`).

## 3. The audits

**Parity set — green and unchanged.** 35/35 (`fixture_seal` 2,
`filesystem_reference` 2, `edit_reference` 3 — the 34-test set plus ruling 8's
accepted third pin, `object_identity` 11, `filesystem_codec` 9,
`filesystem_updates` 6, `filesystem_profile` 2). No parity expectation was
re-pinned in any of the phase's eleven product commits.

**Commit audit — eleven product commits, one variable each.** Every commit's
`git show --stat` touches the item's own files only; every one carries its
architecture-document update and its production-LOC line:

| commit | item | files | tests touched | LOC delta |
| --- | --- | ---: | --- | ---: |
| `ed5ab5d95` | P2-0 | 12 | `visibility.rs` (1 line: the guard's path) | +50 |
| `464807178` | V5 | 6 | new `statement_batching.rs` | +4 |
| `3e7b3db80` | V6 | 8 | new `group_decodes.rs` | +24 |
| `8f0fda297` | V7 | 4 | `connection_profile.rs` +2 cases | +19 |
| `6a4abb256` | P2-8 | 5 | `pack_locator.rs` boundary case | +7 |
| `57c4cf3bd` | P2-6 | 6 | `cas_reuse.rs` +2 cases | 0 |
| `b2abb6455` | P2-7 | 4 | new `filesystem_ordering_consolidate.rs` + support seal | −22 |
| `3ce5f409e` | P2-4 | 12 | `group_decodes.rs`, `visibility.rs` | +64 |
| `12527f477` | V8 | 7 | `cas_reuse.rs` +1 case | +4 |
| `7db87bb8b` | P2-5 | 7 | `metadata_pool.rs` +1 case | +41 |
| `0a593084c` | P2-2 | 5 | `statement_batching.rs` +1 case, 2 moved | +48 |

**Gate audit — work counters.** Every box is gated on a work counter
(`statements`, `group_decodes`, `presence_queries`, `runs_created`/`rows_written`,
`packs_created`/`pack_appends`, identity of the frozen set) or on a named
structural/mutation test; no box is gated on `elapsed`. Every receipt reports
`elapsed_ns` as diagnostic-only, and P2-7's receipt records an elapsed figure that
moved the *wrong* way rather than hiding it.

**Scope audit.** Nothing from the parked register (producer pool, group target,
branch-row summaries, pack-BLOB chunking, membership single-hash, persisted pool
cursor) was touched: `git log 502f2aae1..HEAD -- core/crates` is the eleven commits
above. `P1-13`/`P1-15` are untouched. No product code reads an environment variable
(`grep -rn "std::env" core/crates/*/src` → 0), no worker count was raised, and no
cache/buffer policy was relaxed to make a case pass: the two profile items were
declined rather than landed.

## 4. Checks on the final tree (exit codes)

| # | Command | Exit |
| --- | --- | ---: |
| 1 | `python3 core/tools/check_product_boundary.py` (120 files) | 0 |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 (**469 passed, 0 failed**) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 |
| 7 | `python3 tools/production_loc.py --files` | 0 |
| 8 | `git diff --check` | 0 |

Logs: [`rounds/closing-pass/final/checks/`](rounds/closing-pass/final/checks/)
(`exits.tsv` plus one log per check). No CI and no aggregate preflight exists in
this repository (both retired by owner decision); the checks above are the whole
verification, and no claim of "CI green" is made anywhere in this phase.

## 5. Production LOC — the phase total

| scope | before (`502f2aae1`) | after (`0a593084c`) | delta |
| --- | ---: | ---: | ---: |
| core, all crates | 19,264 (116 files) | **19,503** (120 files) | **+239** |
| `layerfs-content` | 12,320 | 12,298 | −22 (P2-7) |
| `layerfs-storage` | 6,181 | 6,442 | +261 |
| `layerfs-telemetry` | 763 | 763 | 0 |
| reference `crates/` | 65,417 | 65,417 | 0 |
| combined | 84,681 | **84,920** | +239 |

Method: `python3 tools/production_loc.py --detail` over each committed tree; tests,
examples, evidence and docs never enter the number. The plan's estimate was
+200..+460 with the low end likelier; the phase closed at **+239**, inside the band,
with two items declined (which the estimate anticipated would lower it) and one
instrument added (V8).

## 6. What the phase did not do, stated plainly

* **V7's spill half is blocked** (no safe `sqlite3_db_status` path under the crate's
  audited-`unsafe` boundary) and its box stays unticked with that state named.
* **P2-1 and P2-3 are declined**, not landed: the product keeps SQLite's default
  page-cache profile and NORMAL locking. Both have their candidate patches on disk
  in their round directories.
* **`P2-5`'s seed adds one presence query on D22** — a cost, recorded.
* **`P2-2` stops at 72 statements rather than 64** — the sealed-group granularity,
  recorded.
* **`P2-0` changed one line of a test** (a source-locator guard's path) against its
  stated "no test file changed" gate — recorded as a correction.
* **`P2-6`'s magnitude is structural**: no counter prices a hash, so its evidence is
  the removed call site plus a guard that fails on the parent tree.
* **`P2-4`'s cache bound is not stressed** by any frozen row, and **`P2-5`'s
  end-to-end staleness scenario** is proved at the reader level rather than in a
  save.
* **`05-storage.md`'s field lists** needed a separate correction commit
  (`6587740b3`) after V5/V6 — recorded rather than folded into an item.

## 7. Terminal states

| state | items |
| --- | --- |
| landed | P2-0 (with correction), V5 (with correction), V6, P2-8, P2-6, P2-7, P2-4 (with correction), P2-5, P2-2 (with refuted detail), V8 (added instrument) |
| landed in part, blocked | V7 |
| measured-and-declined | P2-1, P2-3 |
| not delivered / blocked | none |
