# #219 round 8 — a wave bounded by its transaction: `operation_work_ns` −164.8 ms

Pre-registration: `pre-registration.md` (written before the first run, including the pin
consequence). Control: **D4b**, the last row on this product tree. Rows:
`ns19-H1-wavebound-20260921T075305Z` (**FAIL — the declared pin, and nothing else**) and
`ns19-H2-pinned-20260921T075340Z` (**PASS, 13/13 gates, 14/14 pinned counters**), the covering
run after the one declared re-pin. Landed as `5c2858b40`.

## 1. The treatment, and what it moved

One difference: `PendingBatch`'s canonical byte budget is the transaction's own declared capacity
(`WAVE_CANONICAL_BYTES_LIMIT = TRANSACTION_CANONICAL_BYTES_LIMIT`, 4 MiB − 1) instead of an
independent 512 KiB figure; that figure keeps its other job — one *group*'s canonical byte bound —
as `GROUP_CANONICAL_BYTES_LIMIT`.

| instrument | D4b | H2 | movement |
| --- | ---: | ---: | ---: |
| **`operation_work_ns` (the row's formula)** | 1821.0 ms | **1656.2 ms** | **−164.8 ms, −9.05 %** |
| **CPU user+system** | 1835.1 ms | **1682.5 ms** | **−152.6 ms, −8.3 %** |
| `diag_commit_total_ns` | 402.7 ms | 336.6 ms | −66.1 |
| `diag_wave_ns` | 112.7 ms | 66.6 ms | −46.0 |
| `diag_write_pack_total_ns` | 231.0 ms | 206.9 ms | −24.1 |
| `diag_insert_objects_ns` | 151.8 ms | 133.8 ms | −18.0 |
| `diag_begin_ns` | 11.5 ms | 3.9 ms | −7.6 |
| `diag_validate_ns` | 52.1 ms | 49.5 ms | −2.7 |
| `profile_total_ns` | 1063.2 ms | 953.2 ms | −110.0 |
| `span_content_ns` | 1457.4 ms | 1321.1 ms | −136.2 |
| **waves (`presence_queries`)** | 398 | **73** | **−81.7 %** |
| **`pipeline.commits`** | 800 | **284** | −64.5 % (the declared re-pin) |

**The count is the mechanism and the row's own figure is inside the drift band.** 302,406,480
canonical bytes / (4 MiB − 1) = 73 waves exactly, and the row reports **73** presence queries: the
byte bound, not the object bound, is what bounds a wave. The row's wall figure moved 164.8 ms, below
the ~250 ms this machine drifts, so the claim rests on the wave and commit counts, on the per-wave
price (§2) and on **CPU, which fell 152.6 ms** — CPU counts work rather than waiting and the control
regions did not drift.

## 2. The prediction that was missed, stated plainly

The pre-registration predicted `diag_wave_ns` **15–30 ms** and registered `>= 60 ms` as refuting.
It came in at **66.6 ms**, so **that refutation clause fired** and the prediction is reported as
missed. The reason is arithmetic, not mystery: a wave now carries 8.1x the content, so the wave's own
locator query and presence seed grew with it. Per wave the cost went **189 us → 909 us (4.8x)** while
the wave count fell **8.2x**; the two together leave only 41 % off `diag_wave_ns` instead of the
predicted 80 %. The registration priced the *count* of waves and did not price the *width* of one.
`diag_validate_ns` is the same story in the same direction (2.7 ms, not the predicted 8–15 ms of
saving): the same 25,245 rows are compared, in 73 calls instead of ~596.

Predictions that were hit: `diag_commit_total_ns` 336.6 ms against a predicted 320–375;
`diag_begin_ns` 3.9 ms against ≤ 3 (missed by 0.9 ms); `operation_work_ns` 1656.2 ms against a
predicted 1550–1700.

## 3. What did not move, and the one figure that moved the wrong way

Every work counter is identical to the control: `inserted` 25245, `statements` 16595,
`packs_created` 1268, `pack_appends` 15534, `pack_bytes_written` 302,406,480, `content_bytes`
301,171,810, `content_objects` 24863, `reused` 0, `batches` 3, `bindings` 10100,
`largest_batch_bindings` 4096, `metadata_objects` 382, `objects_emitted` 67 — and
`digest:filesystem_root` is `1d6fba29aba105aefbb668c34b026f8b89d20773fc36d548b991034dc3857847`
unchanged (`g1.o1-pinned-identity` PASS). The store is byte-identical: the same treatment writes the
same bytes in fewer transactions.

**The inclusive figure moved the wrong way and is not hidden:** `phases.operation_ns`
1864.9 → **1973.2 ms** (+108.3), because `pipeline.teardown_ns` — the save's connection close,
outside the row's formula — went **39.4 → 312.9 ms** in this window. That is the same
machine-charged close the earlier rounds measured at 6.7 ms to 469.9 ms on identical source. It is
published rather than subtracted silently, and it is why the row's formula and CPU, not the
inclusive span, carry this round.

## 4. The pin procedure, and the red receipt

`pipeline.commits` is pinned and this treatment moves it by design; the consequence was declared in
the pre-registration **before the number existed**, exactly as round 3 declared its own. H1 is the
first run: **FAIL**, one gate — `g1.o3-pinned-counters`, `pipeline.commits 800 -> 284` — and the
other twelve PASS. The count was read from that receipt, `tests/golden/expected.tsv` was re-pinned
once, the harness was rebuilt, and the covering run H2 was taken once. **Both receipts are on disk
and H1 is reported as red.** No other pin moved.

## 4b. The two seals, stated exactly

| row | `identity.source_commit` | `source_dirty` | dirty files | harness binary sha256 |
| --- | --- | --- | --- | --- |
| H1 (FAIL, the pin) | `5c2858b40f0961803f78b2b0569b2ef2162554df` | **False** | none | `c5d53824fb670861…` |
| H2 (PASS, the arm) | `5c2858b40f0961803f78b2b0569b2ef2162554df` | **True** | `M core/benchmark/fs-bench-pro-storage-content/tests/golden/expected.tsv` | `5884b480c5903be8…` |

**The product source is the same commit in both rows and the product tree was clean at both runs.**
H2's dirty file is the **golden table itself** — the re-pin is by definition a harness edit made
between the two runs, and the pin commit `a93e0c557` carries it. This is the same shape as round 3's
own `T3b`/`T3c` pair (L64): the re-pin cannot be committed before the number it contains exists. It is
recorded here rather than left for a reader to find in `run.json`, and **the arm's measurements do not
read the dirty file**: `expected.tsv` is compiled into the harness binary, so the *binary* (whose hash
is pinned in the receipt and differs between the two rows) is what graded the row, and the file on
disk only had to agree with it at build time.

## 5. One test fixture encoded the old bound

`persistence_failure::a_terminal_operation_refuses_further_work` filled the batch with 40 objects of
64 KiB so that a wave would run; at a 4 MiB bound that no longer fills anything, so the case stopped
forcing a wave and asserted nothing. It now derives the count from
`WAVE_CANONICAL_BYTES_LIMIT`. This was found by the covering command, diagnosed from its output, and
fixed once.

Checks as run: `cargo test -p layerfs-storage` **33 binaries, 0 failed**; the seven write-path
invariants plus `memory_bounds` green; `clippy --all-targets` clean; `fmt --check` clean;
`core/tools/check_product_boundary.py` PASS; the whole core workspace `--no-fail-fast`
**108 `test result: ok`, 0 failed**. Not run: the reference `crates/` workspace, any other harness
case or lane.

Production LOC: 31376 -> 31377 (delta +1), `python3 tools/production_loc.py --root <tree>`, first
parent `4e0c0ce04` against the committed tree. The golden pin and the test fixture are test/tooling
changes and report the unchanged production total, delta 0.

## 6. What this does and does not say about the reference's 73 transactions

The reference implementation (`crates/`) reports 73 transactions for the same 25,158 objects and
302,182,831 bytes; this row now reports **73 waves**. That is a **coincidence of bound arithmetic,
not a paired comparison** — different workspaces, and the v0.1.6 receipt carries no seal this side
can be matched against, so the pairing stays `NOT_MEASURED`. What the round does establish is that
the reference's step count was not mysterious: an 8x-too-small batch budget was producing 596 steps
where the transaction capacity supports 73.
