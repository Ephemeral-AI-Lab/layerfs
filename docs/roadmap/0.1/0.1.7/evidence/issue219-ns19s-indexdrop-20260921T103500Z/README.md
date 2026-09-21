# #219 round 18 — dropping the locator's second B-tree: `operation_work_ns` −92.15 ms, and a refuted clause that corrects round 17

Status: **PASS, 13/13 gates, 14/14 pinned counters**, one sample, `--verify full`, sealed tree
(`source_dirty: false`, `e3a46d74b`). Arm
`benchmark-results/issue219/ns19-S1-indexdrop-20260921T103500Z`, control
`benchmark-results/issue219/ns19-Q2-repin-20260921T115200Z`. Pre-registration, written before the
first edit and before the first run: [`pre-registration.md`](pre-registration.md). Raw evidence:
`raw/arm-receipt.json` (the arm's whole receipt) and `raw/term-deltas.txt` (every term that moved).

**Owner ruling honoured: the database page size is 4 KiB and was not set, read or touched.** No
pragma changed; `sqlite/connection.rs` is untouched.

---

## The one difference

`CREATE INDEX objects_save ON objects(save_id, object_id);` is removed from
`core/crates/layerfs-storage/sql/schema.sql`, together with the two declarations that require it:
`REQUIRED_INDEXES` 3 → 2 (`sqlite/schema.rs:95`) and `SCHEMA_VERSION` 9 → 10
(`policy.rs:51`, because `sql/schema.sql`'s own rule is *"Older schemas are rejected, never
migrated"* and a schema-9 Store carries an index this build no longer requires). No column, no
constraint, no pack framing, no pragma, no statement text, no cadence, no worker count. Product commit
`e3a46d74b`.

## The result

| term | control | arm | delta |
| --- | ---: | ---: | ---: |
| **`pipeline.operation_work_ns`** | 1,218,880,166 | **1,126,731,417** | **−92,148,749** |
| `pipeline.accept_span_ns` | 1,393,706,416 | 1,208,264,083 | −185,442,333 |
| `pipeline.teardown_ns` (= `diag_finish_drop_ns`) | 174,826,250 | 81,532,666 | −93,293,584 |
| `pipeline.diag_insert_objects_ns` | 143,438,848 | 99,619,592 | −43,819,256 |
| `pipeline.diag_commit_total_ns` | 458,259,288 | 431,236,291 | −27,022,997 |
| `pipeline.diag_write_pack_total_ns` | 125,584,783 | 121,477,103 | −4,107,680 |
| `pipeline.diag_wave_ns` | 65,376,701 | 62,289,960 | −3,086,741 |
| `pipeline.diag_validate_ns` | 48,450,628 | 50,070,583 | +1,619,955 |
| `pipeline.diag_begin_ns` | 1,792,498 | 1,640,545 | −151,953 |
| `phases.cpu_user_ns + cpu_system_ns` | 1,228,332,000 | 1,132,985,000 | −95,347,000 |
| `phases.operation_ns` (wall) | 1,396,256,541 | 1,210,556,500 | −185,700,041 |
| complete command | 2.221 s | 2.017 s | −0.204 s |

**`operation_work_ns` 1218.88 → 1126.73 ms, −92.15 ms, −7.56 %.** The row's formula is
`accept_span_ns − diag_finish_drop_ns` (`src/ops/pipeline.rs:1259-1262`), and **both halves roughly
halved**: the accept span fell 185.44 ms and the teardown — the OS's price for dirtied pages, excluded
from the formula — fell 93.29 ms. So the change also bought 93.29 ms *outside* the row's own figure.

Nothing else moved. `pipeline.commits` 95, `pipeline.inserted` 25,245, `pipeline.statements` 7,666,
`pipeline.pack_bytes_written` 301,865,004, `pipeline.packs_created` 1,270, every content count, and
`digest:filesystem_root` `1d6fba29…` — all identical. `g1.o3-pinned-counters` and
`g1.o1-pinned-identity` **PASS**, so **no re-pin was needed**; unlike L73 and L74 this treatment
changed no pinned counter at all.

## Against the pre-registration, clause by clause

| registered | predicted | measured | outcome |
| --- | --- | --- | --- |
| `diag_insert_objects_ns` | 93.4–116.9 ms | **99.62 ms** | **held** |
| `operation_work_ns` | 1168.9–1192.3 ms | **1126.73 ms** | **refuted — 1.84× better than the band allowed** |
| `diag_commit_total_ns` | moves ≤ 2 ms | **−27.02 ms** | **REFUTATION 3 FIRED** |
| pins, digest, counts | unchanged | unchanged | held |
| PASS 13/13, 14/14, `--verify full`, one sample | required | satisfied | held |
| `abandon` still correct | required | `persistence_failure.rs:313`, `content_index.rs:119`, both passing | held |

### Refutation clause 3 fired, and it corrects round 17

Clause 3 read: *"**`diag_commit_total_ns` moves by more than 15 ms.** Then the index's pages were
being paid in the commit term rather than the insert term, round 17's attribution is wrong."* That is
what happened, and the diagnosis in the clause is the right one.

**Round 17's replica timed the insert loop only.** Under this Store's profile — `journal_mode =
MEMORY` with `synchronous = OFF` (`sqlite/connection.rs:33-47`) — an insert **dirties** pages and the
**COMMIT flushes** them; squad C recorded exactly this (*"the step … does not pay for its pages — it
only dirties them — and the `COMMIT` that follows pays for them. `commit_ns` is therefore a
**page-flush** region"*, `issue219-squadC-cadence-20260921T044258Z/README.md` §2). The replica put one
transaction around the inserts and timed the loop, so it counted the dirtied pages and charged the
index only the work the loop itself does, leaving the flush uncounted. **The product pays both.**

The corrected attribution of the index on this row, measured:

| where the index's cost lands | ms |
| --- | ---: |
| `diag_insert_objects_ns` (the loop that dirties its pages) | **−43.82** |
| `diag_commit_total_ns` (the COMMIT that flushes them) | **−27.02** |
| those two terms alone | **−70.84** |
| the whole row | **−92.15** |

Inside the formula, the named charges that moved sum to **−76.57 ms** (insert −43.82, commit −27.02,
pack writes −4.11, wave −3.09, begin −0.15, validate **+1.62**); the seven profile buckets total
**−76.98 ms**. Against the row's −92.15 that leaves **−15.17 ms unattributed inside the formula**, and
it is reported as unattributed rather than assigned to a term that was not measured.

**On keeping the change.** Clause 3's remedy as written is that the change is *"not kept on this
prediction"* — and it is not. The prediction is **withdrawn and corrected above**; the change is kept
on the arm's own receipt instead, which is a stronger basis than the estimate it replaces: a sealed
clean tree, 13/13 gates, every pinned counter and the root digest unchanged, and a movement 1.84× the
upper bound the withdrawn prediction allowed. The alternative — reverting a −92.15 ms, pin-neutral,
contract-argued change because the estimate was too conservative — would be keeping the error rather
than the measurement.

## The page mechanism is confirmed exactly, at the product level

Round 17's replica, on a table of this row's own shape, predicted **265 index pages**. The product's
own Store lost **269 pages and 1,101,824 bytes**, and the whole of it is the index:

| `resources.space` | control | arm | delta |
| --- | ---: | ---: | ---: |
| `after.page_count` | 82,256 | 81,987 | **−269** |
| `after.apparent_bytes` (`st_size`) | 336,920,576 | 335,818,752 | **−1,101,824** |
| `nonpack_bytes` | 3,997,696 | 2,895,872 | **−1,101,824** |
| `pack_bodies_bytes` | 332,922,880 | 332,922,880 | **0** |
| `canonical_bytes_total` | 302,231,057 | 302,231,057 | **0** |
| `freelist_count` | 0 | 0 | 0 |

`269 × 4096 = 1,101,824`, and the fall is **entirely** in `nonpack_bytes`: no pack byte and no
canonical byte moved, so this is the index's own B-tree leaving the file, plus the four pages of
schema and pointer that named it. The replica's 265 pages and the product's 269 are the same
mechanism counted twice, on two instruments.

## Where the row now stands

| | ms |
| --- | ---: |
| `operation_work_ns` | **1126.73** |
| target | 1000.00 |
| **gap to 1 s** | **−126.73** (was −218.88) |
| serial floor (L74) | 842.90 |

**Boundary-matched** (round 17's boundary: `operation_work_ns` + `construct_ns` + `construct_noise_ns`)
the row is **1680.22 ms of work** and **1686.48 ms of CPU**, against the reference's fastest same-shape
row at **1789.51 ms** — **−6.11 % on work, −5.76 % on CPU.** The row is now *ahead* of the fastest
v0.1.6 row it can be compared with, on the only boundary the two can share. (`construct_ns` also moved
−9.17 ms between the two runs on identical source and identical work; it is outside the formula and
this round makes no claim about it beyond noting it.)

The remaining 126.73 ms, and the floor, are this product's own. The levers named in round 17 stand:
the C1 build span's uncharted ~70–100 ms is still the largest block with no attribution, the native
lane's 2.03 records per group is still a policy constant, and the insert statement shape — now the
*second* largest remaining charge at 99.62 ms — still carries L73's varying statement text.

## Checks as run

- `cargo +1.85.1 test --locked --manifest-path core/Cargo.toml --no-fail-fast` — **630 passed / 0 failed**, 3 ignored.
- `-p layerfs-storage` alone — **224 passed / 0 failed**, including `persistence_failure.rs:313`
  (`abandon`) and `content_index.rs:119` (`an_abandoned_save_leaves_no_entry_behind`).
- `clippy --locked --manifest-path core/Cargo.toml --all-targets` clean;
  `fmt --manifest-path core/Cargo.toml --all --check` clean;
  `core/tools/check_product_boundary.py` **PASS** (194 production files).
- Harness release build from the repository root, then
  `runner.py perf --case pipeline-namespace-10000 --out benchmark-results/issue219/ns19-S1-indexdrop-20260921T103500Z --verify full --no-build`
  — **1 case, PASS**, 2.2 s, one sample, fresh `--out`, complete command 2.017 s inside the 15 s limit.
- **Not run:** the reference `crates/` workspace, any other harness case or lane, any further sample of
  this arm, and the harness's own wider suite beyond this case. `abandon`'s query is now a full scan of
  `objects`; its cost is **NOT_MEASURED** and this round claims nothing about it — only that it is still
  correct, which its tests cover.

## Production LOC

**31684 → 31683 (delta −1)**: the removed `CREATE INDEX` line. The three comment blocks added or
corrected in `policy.rs` and `sqlite/schema.rs` contribute nothing to the count. Method
`python3 tools/production_loc.py --root <tree>`, first parent against the committed tree.
