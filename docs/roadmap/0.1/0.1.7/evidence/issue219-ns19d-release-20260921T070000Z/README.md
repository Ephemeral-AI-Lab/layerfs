# #219 round 4 — the release is the connection close, and what the row's timer includes

Pre-registration: `pre-registration.md` (written before the first run). Control: **T3c**
(`755bfa21f`): `operation_ns` 2065.0 ms, `diag_validate_ns` 96.9 ms, `diag_finish_drop_ns` 261.3 ms.
Receipts: `receipts/`. Probe: `close-probe.py`.

## 1. D3/D3b — what the release frees: the connection, and nothing else

Round 2 left the row's largest unexplained term as `finish_drop_ns`, "the drop of the save's owner",
6.7–261 ms across rows of identical code. D3 released that owner's structures in charged groups
(caches, index clones, pack tails, codec workspaces) and found **1.31 ms** in all of them together,
leaving the residual unexplained. D3b took the owner apart **field by field** and dropped each field
in its own charged step:

| field | ns |
| --- | ---: |
| **`connection`** | **46,878,000** (46.878 ms) |
| compression workspace | 1,000 |
| decompression workspace | 1,000 |
| pooled reader (pack + value caches) | 2,000 |
| delta pack cache | 0 |
| content-signature index clone | 1,000 |
| pooled value index clone | 65,000 |
| retained pack tails (all lanes) | 17,000 |
| everything else in the owner | 2,000 |
| **total (`finish_drop_ns`)** | **46,966,000** |

**The release is the connection close. Every cache, workspace and index clone the save holds costs
0.087 ms between them.** The earlier rows' 261.3 ms and 469.9 ms were the same close, slower.

## 2. The teardown price, measured with no product code

`close-probe.py` writes N transactions of P pages into a scratch store under the product's own pragma
profile and times `sqlite3_close`:

| workload | written | close |
| --- | ---: | ---: |
| no transactions, empty file | 0 MB | 0.22 ms |
| 800 transactions x 125 pages | 410.1 MB | **316.03 ms** |
| 17,378 transactions x 1 page | 80.1 MB | **58.40 ms** |
| 800 x 125 pages, repeated | 410.1 MB | **301.06 ms** |
| 800 x 125 pages, `journal_mode=OFF` | 410.1 MB | 41.33 ms |
| 800 x 125 pages, `journal_mode=DELETE` | 410.1 MB | 41.31 ms |
| 800 x 125 pages, `journal_mode=WAL` | 410.1 MB | 42.39 ms |
| 800 x 125 pages, `journal_mode=MEMORY` (the product's profile) | 410.1 MB | 44.20 ms |

Two facts, and neither is a product property:

1. **The cost is proportional to the bytes written** — ~0.75 ms/MB in the first window, ~0.1 ms/MB in
   the second — and it is **the same in every journal mode**, so it is not the rollback journal, not
   the page cache's configuration and not the format.
2. **It moves by ~7x within minutes** on the same machine with the same probe: 316.03 ms and then
   44.20 ms for identical work. It is the operating system's price for the ~100,000 pages the process
   left dirty, charged when the last descriptor to the file closes.

**It is therefore not addressable in the product except by writing fewer pages** — which is what
rounds 1 and 3 already did (2.29 GB of append traffic to 302 MB).

## 3. What the row's timer includes, stated exactly

The case's own note reads `measured_region: Store::open + build_filesystem + content accept + save +
acknowledgement`. Measured on `ns19-D3b-fields-20260921T070613Z`:

| region | ms | share | why it is inside |
| --- | ---: | ---: | --- |
| **establishment**: `Store::open` + `begin_save` — two connection opens, pragma configuration, the read-scope temp table, the save-slot reservation and the two index clones | **3.936** | 0.19 % | **declared**: the region starts at `Store::open` |
| C1 tree build (`build_filesystem`/`update_filesystem`) | 354.723 | 17.1 % | declared: the pipeline case measures the C1→C2 handoff |
| content accept + save | 1667.290 | 80.3 % | declared |
| — of which the **teardown**: the save's connection close | **46.966** | 2.3 % | **not declared anywhere**: `SaveOperation` owns the `Connection`, so its `Drop` lands inside `finish` |
| `operation_ns` | 2077.262 | 100 % | |
| `accept_span_ns` (the profile's denominator) | 2073.326 | | excludes the 3.936 ms of establishment |

**Establishment is 0.19 % of the row and is declared. The teardown is neither declared nor
establishment, and in the row that carried 469.9 ms it was 22.7 % of the row.**

**The boundary is deliberately NOT moved.** Ending the region at the acknowledgement would remove
the teardown from the phase, and that is the move the measurement contract forbids; it would also
break comparability with the v0.1.6 arm, whose timer includes its own setup. The term is published
(`finish_drop_ns`, `release_connection_ns`) so a reader can subtract it, and the case-definition
question — should the region end at the acknowledgement? — is recorded for the owner rather than
decided here.

## 4. T5 — one collision check per wave: landed, measured on its own instrument

`validate_candidates` ran once per seal (16,802 calls, 25,245 per-row queries). Since round 3 a
wave's seals share one transaction and hold the Store's write lock, so the rows it compares against
cannot change between them; the wave now validates every row it wrote in one call at its end, and a
seal outside a wave keeps the per-seal call.

| instrument | T3c | round 4 | movement |
| --- | ---: | ---: | ---: |
| `diag_validate_ns` | 96.94 ms | **51.36 / 52.69 ms** (two rows) | **−45.6 ms** |
| `diag_collision_query_ns` | 58.65 ms | 48.14 / 49.41 ms | −10.5 ms |
| query sets | 16,802 | ~600 | 28x fewer |
| `pipeline.commits`, `inserted`, `statements`, `packs_*`, `presence_queries`, `full_records`, `prefix_records`, `content_bytes` | — | **identical** | — |
| root digest | `1d6fba29…` | `1d6fba29…` | unchanged |

The prediction was `validate_ns <= 20 ms` and it is **51 ms**, so the prediction is **missed and
reported as missed**: the check's cost is dominated by the *identifiers* it asks about, not by the
statement count, and hoisting removes the per-seal scope read (38.3 -> ~3 ms) and the per-statement
overhead, not the lookups themselves. The treatment is landed on the **−45.6 ms measured twice on a
count-driven instrument**, not on `operation_ns`, which this round cannot resolve below ~250 ms.
A side effect worth stating: a collision now rolls the whole wave back instead of leaving earlier
seals committed.

## 5. Checks as run on this tree

`cargo test -p layerfs-storage` **33 binaries, 0 failed**; the whole core workspace `--no-fail-fast`
**108 `test result: ok`, 0 failed, exit 0**; `clippy --all-targets` clean; `fmt --check` clean; the
measured row **PASS, 13/13 gates, 14/14 pinned counters**. **Not run:** the reference `crates/`
workspace's tests, any other harness case or lane, any durability run, and a pair-latency probe.

Production LOC: **31318 -> 31371 (delta +53)**; combined 96735 -> 96793. Method
`python3 tools/production_loc.py --root <tree>`, first parent `755bfa21f` against the committed tree.

## 6. The row's formula changed: the connection is out of it (owner direction, 2026-09-21)

**Direction.** Remove the database connection's establishment and teardown from the row's formula.
This section records what was implemented, what it costs and what it does not fix.

**What the instrument now publishes** (`src/ops/pipeline.rs`), all in the row's receipt:

| counter | meaning |
| --- | --- |
| `pipeline.establishment_ns` | the closure's start to `accept_span_ns`'s start: `Store::open` + `begin_save` — two connection opens, pragma configuration, the read-scope temp table, the save-slot reservation, the two index clones |
| `pipeline.teardown_ns` | the save's connection close, taken from `diag_finish_drop_ns` |
| `pipeline.operation_work_ns` | **the row's formula**: `accept_span_ns - teardown_ns` |
| `pipeline.accept_span_ns` | unchanged, the profile's denominator |
| `phases.operation_ns` | unchanged, the inclusive span the runner times |

`establishment_ns + operation_work_ns + teardown_ns` is exactly `operation_ns`, so **the inclusive
figure is always reconstructible from the receipt** and nothing is hidden.

**Measured, `ns19-D4b-formula-20260921T071758Z`** (PASS, 13/13 gates, 14/14 pinned counters):

| | ms |
| --- | ---: |
| inclusive `operation_ns` | 1864.9 |
| establishment | 4.511 |
| teardown | 39.410 |
| **`operation_work_ns`** | **1821.0** |

**What the change costs, stated plainly.** The teardown is *caused* by the phase's own writes — it is
the operating system charging for the pages the save dirtied — so removing it means the phase no
longer pays the full price of its own work. That is a real objection and it is recorded rather than
argued away. Two things make it acceptable here: the term is a property of the machine and the
window (6.7 ms to 469.9 ms on identical code), so keeping it in the phase makes the row's level
incomparable between windows; and it is **published**, not dropped, so a reader who wants the
inclusive figure has it.

**What it does not fix.** The v0.1.6 pairing is `NOT_MEASURED` under this boundary, and **the
comparison arm must use the same boundary on both sides** before any paired claim is made. The
clean-tree arm A0 also predates the teardown instrument: its `accept_span_ns` is known
(3487.313 ms) and its establishment is derivable (2.948 ms), but its teardown is not, so the
cumulative work figure below is an upper bound rather than a like-for-like pair.

**Cumulative, against this worktree's clean tree (A0):**

| | A0 | now | movement |
| --- | ---: | ---: | ---: |
| inclusive `operation_ns` | 3490.3 ms | 1864.9 ms | **−46.6 %** |
| `operation_work_ns` (the new formula) | <= 3487.3 ms (teardown unmeasured) | 1821.0 ms | **<= −47.8 %** |
| CPU (user+system) | 3388.9 ms | 1835.1 ms | **−45.8 %** |
| complete command | 4770.5 ms | 3130.7 ms | −34.4 % |

Against the campaign's own baseline row (3351.0 ms inclusive) it is **−44.3 %**.

**A pre-existing harness failure, reported and not repaired.** `cargo test` for the harness fails
three cases in `registry_negative` (`the_frozen_cardinality_array_is_what_the_registry_holds`,
`the_lane_sizes_and_admission_split_are_the_frozen_ones`, `the_registry_is_clean_under_its_own_self_check`):
the registry holds 221 rows and a per-family cardinality of `[…, 2, 5]` where the frozen array and
`CONTRACT.md` section 3 say 220 and `[…, 2, 4]`. The fifth row is `pipeline-namespace-10000`, added by
the earlier campaign. **It is not caused by this round and not repaired here**: `src/registry.rs`,
`tests/registry_negative.rs` and `tests/golden/registry.tsv` are byte-identical to `HEAD` (an empty
`git diff HEAD` for all three), and the base commit's own `run.json` records the identical mismatch
(`ns17-squadA-packcounters-…/run.json`, `registry_self_check`). Repairing it means editing a frozen
expectation transcribed from a contract document, which deserves its own review rather than a quiet
line in this commit.
