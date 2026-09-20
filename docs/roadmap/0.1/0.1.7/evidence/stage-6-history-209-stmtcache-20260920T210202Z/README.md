# #209 — the per-step statement cache: measured, and **withdrawn**

> Status: Research; **diagnostic evidence, not release admission**. Round of
> 2026-09-21. **No product line is kept from this round**: the tree is reverted to
> `f3e84c073`. Admission `INELIGIBLE`, every budget class `NOT_RUN`. Nothing here
> closes #190, #205, #208 or #209.

## The result, in one paragraph

The treatment did exactly what its pre-registration said inside the bucket it was aimed
at, and **nothing at all to the wall clock**, so it is dropped. Both arms ran from one
binary (sha256 `85605a522738e6318efed46144113a28493fb320d3584af28a15f6ff65ee530d`), the
control behind a measurement-only lever, A B B A balanced, and the saved Store is
byte-identical in all four rows. `sql_ns` fell by **0.275 s** — the 4,723 ns parse of
the pack `UPDATE` times its 46,049 calls, which is what D3 predicted — and the operation
moved by **−0.024 s (−0.1 %)**, because **`commit_ns` rose by 0.087 s** and the
uncharged per-step work by 0.055 s. **The mechanism by which removing a parse from the
statement the step issues adds time to the commit that follows it is not established**,
and this round does not claim the win it cannot see. Ledger L59, handoff
[`issue-commit-time-rca-handoff.md`](../../issue-commit-time-rca-handoff.md).

## 1. The treatment, pre-registered

**A statement the step issues every time is prepared once.** Six texts went through the
connection's prepared-statement cache instead of being parsed per call: `BEGIN
IMMEDIATE`, `COMMIT`, `ROLLBACK`, `INSERT INTO object_packs`, `UPDATE object_packs SET
data = ?2 …`, `SELECT next_pack_id FROM store_policy WHERE id = 1`. Full prediction and
falsifiers in [`PREREGISTRATION.md`](PREREGISTRATION.md); the lever is
`LAYERFS_STORAGE_STATEMENT_CACHE=0` (unset = the treatment), declared in every receipt's
`extra_environment`.

## 2. The four rows, and the arms named the right way round

The driver's arm `b` carries the lever, and the lever **restores** the old behaviour, so
`b` is the **control** and `a` is the **treatment**. (An intermediate analysis in this
round read them the other way; the rows below are the corrected reading, checked against
each receipt's `extra_environment`.)

| row | arm | operation | `commit_ns` | `sql_ns` | `resolve_ns` | `filesystem` |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `stmt-1a` | treatment (prepared once) | 22.256 s | 2.485 s | 1.279 s | 5.985 s | 6.166 s |
| `stmt-2b` | control (`=0`, parsed per call) | 22.536 s | 2.377 s | 1.590 s | 5.994 s | 6.140 s |
| `stmt-3b` | control | 22.537 s | 2.429 s | 1.581 s | 5.944 s | 6.153 s |
| `stmt-4a` | treatment | 22.770 s | 2.494 s | 1.343 s | 6.170 s | 6.245 s |
| | **treatment mean** | **22.513 s** | **2.490 s** | **1.311 s** | **6.078 s** | 6.206 s |
| | **control mean** | **22.537 s** | **2.403 s** | **1.586 s** | **5.969 s** | 6.147 s |
| | **Δ (treatment − control)** | **−0.024 s** | **+0.087 s** | **−0.275 s** | +0.109 s | −0.058 s |

The other buckets do not move: `full_ns` −0.019 s, `delta_ns` −0.008 s, `place_ns`
−0.008 s, `group_ns` −0.001 s, `content` −0.016 s.

## 3. Why it is withdrawn

The pre-registered falsifier, verbatim from [`PREREGISTRATION.md`](PREREGISTRATION.md):

> If the operation does not fall below 16.35 s, or `sql_ns` does not fall, the parse was
> not where the measurement said: **withdrawn**.

`sql_ns` fell — by 0.275 s, against a predicted 0.22 s at the previous window's scale,
which is the parse confirming itself. **The operation did not fall** (−0.024 s, inside
the window's own drift of −0.514 s from first row to last). Three readings, stated in
order of how much they are supported:

1. **The saving is real and is redistributed.** `sql_ns` −0.275 s, `commit_ns` +0.087 s,
   uncharged per-step work +0.055 s — the three nearly cancel. Both arms' charges are
   taken by the same instrument in the same run, and the arms do not overlap in either
   bucket (`sql_ns` control 1.590/1.581 against treatment 1.279/1.343; `commit_ns`
   control 2.377/2.429 against treatment 2.485/2.494).
2. **The commit-path rise may be a time effect, not an arm effect.** The A B B A order
   makes `commit_ns` a U in time (2.485, 2.377, 2.429, 2.494) while `sql_ns` is a hump
   (1.279, 1.590, 1.581, 1.343) — the two buckets move in opposite directions across the
   same four rows, which two samples per arm cannot separate from an arm effect. A second
   window would decide it; this round did not run one.
3. **What is not claimed**: that caching the per-step statements makes stride10 slower.
   Only that it does not make it faster, which is the bar the treatment had to clear.

## 4. The diagnostics this round leaves behind

**D4 — the prepared-statement cache is not this workload's problem, and two hypotheses
died here.** The connection's cache is rusqlite's LRU with a default capacity of **16**,
and two of the texts the product caches vary with the call. Isolated, every SQL text
built before the timer ([`statement-cache-probe.rs.txt`](statement-cache-probe.rs.txt),
[`scratch/statement-cache.txt`](scratch/statement-cache.txt)):

| arm | ns per statement |
| --- | ---: |
| locator, 1 width, capacity 16 | 207 |
| locator, 16 widths, capacity 16 | 108 |
| **locator, 17 widths, capacity 16** | **10,677** |
| **locator, 128 widths, capacity 16** | **18,477** |
| locator, 128 widths, capacity 192 | 200 |
| locator 1 width + the four per-step texts, capacity 16 | 103 |
| locator 128 widths + the four per-step texts, capacity 16 | 3,700 |
| locator 128 widths + the four per-step texts, capacity 192 | 100 |

One width over capacity turns a 108 ns hit into a 10.7 µs miss. **The workload does not
do that.** A measurement-only probe ([`cache-probe.rs.txt`](cache-probe.rs.txt), one
declared diagnostic row, `runs/width-probe-history-stride10`) counted what a real
stride10 run issues:

| call site | calls | identifiers | distinct widths | distribution |
| --- | ---: | ---: | ---: | --- |
| locator (`lookup::candidates`) | 380,444 | 443,574 | 126 | **1 × 376,398 (99.0 %)**, 2 × 2,141, 3 × 627, 4 × 214, 128 × 67, then ~120 widths with counts of tens |
| object insert (`write::insert_objects`) | 44,334 | 52,032 | 68 | **1 × 44,143 (99.6 %)**, 128 × 22, then tens |

The distinct-width sets exceed the LRU, but **the hot width is 1 and it stays resident**,
so the misses are ~4,000 locator calls and ~200 insert calls — tens of milliseconds.
**Both candidate explanations for the ≈6.5 µs per locator call that separates the in-run
cost (10.00 µs) from the isolated one (3.488 µs) are refuted**: not text construction
(the 6.75 µs `format!` belongs to the 128-wide text, which occurs 67 times a run, not
380,380) and not cache thrash. **Raising the cache capacity is not a lever**; it would
buy tens of milliseconds and cost memory.

**D5 — a cached statement is cheaper than a fresh one, in isolation.** The pack `UPDATE`
replayed with the product's own parameter shape and a realistic body cycle (96 KB ↔
262 KB, growth 1,450 B/append, 15,000 appends per arm, three rounds, interleaved; raw
output [`scratch/rebind.txt`](scratch/rebind.txt)):

| arm | ns per statement |
| --- | ---: |
| fresh statement, growing body | 35,611 |
| **cached statement, growing body** | **31,036** |
| fresh statement, fixed body | 17,768 |
| **cached statement, fixed body** | **14,036** |
| growth penalty (growing − fixed) | +17,843 fresh / +17,000 cached |
| **cache effect** | **−4,575 growing / −3,732 fixed** |

So the parse saving is real in isolation (−4.6 µs, the 4,723 ns D3 measured), and the
17.8 µs *growth* penalty — the whole overflow chain rewritten because the row grew — is
the format's price, confirmed independently. **The probe's source was deleted with the
product tree before it was copied here** (a process slip this round records rather than
papers over); its shape is: `UPDATE object_packs SET data = ?2 WHERE pack_id = ?1 AND
save_id = (SELECT save_id FROM temp.layerfs_read_scope)`, `PROBE_CALLS=15000`,
`PROBE_ROUNDS=3`, `PROBE_GROWTH=1450`, pack cycled at `PACK_CAP=262,144`, arms
`fresh`/`cached` × `growing`/`fixed`, timing the statement only, run against a copy of
the previous round's `sample.sqlite`. Its raw measurements are retained.

## 5. Equivalence, custody and checks

- **The saved Store is byte-identical in all four rows**
  (`7ea2fe6ccf13bc5aee7ba59bddef2d60d61d50d6b90329ee1488b1f096228358`) with `commits`
  48,446 in every row; the lever changed no stored byte and no counter.
- One binary for both arms, sha256
  `85605a522738e6318efed46144113a28493fb320d3584af28a15f6ff65ee530d`, archived at
  [`binary-archive/lever-pair`](binary-archive/lever-pair); the width-probe row used the
  same binary with the probe enabled and is a **diagnostic**, marked `admission
  INELIGIBLE` like every other row.
- **The diagnostic rows are not comparable to the measured rows for time** (the width
  probe writes to stderr per save), and only their counts are used above.
- Preflight: quiet per `collect.py`; no deferrals were written in this round's four
  measured rows.
- Checks on the reverted tree: `fmt --check` exit 0, `test --locked --workspace`
  **537 passed / 0 failed**, `clippy -D warnings` exit 0, `check_product_boundary.py`
  PASS over 175 files, self-tests 6/6 OK. Harness unchanged (117 tests, not re-run this
  round; the harness source seal `04bcfab5…` is identical in every row).
- **Not run:** verification mode; every row is a diagnostic, admission `INELIGIBLE`,
  every budget class `NOT_RUN`. No CI, no `tools/preflight.sh`.

## 6. What the tree looks like now

`git diff f3e84c073` over `core/crates` and `crates` is **empty**: the treatment, the
lever and both probes are removed. The campaign directory keeps the pre-registration,
the four measured rows, the two diagnostic outputs and the archived binary, because a
withdrawal is evidence too.
