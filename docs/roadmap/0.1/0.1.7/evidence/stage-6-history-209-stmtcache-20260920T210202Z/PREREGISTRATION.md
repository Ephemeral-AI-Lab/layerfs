# Pre-registration — #209 per-step statement preparation, before the treatment

> Status: Research; diagnostic. Written after this round's diagnostics were sampled
> and **before** the treatment was written into the tree.

## What the diagnostics measured first

**D4 — the prepared-statement cache is not this workload's problem, and two
hypotheses die here.** The product's connection carries rusqlite's LRU whose default
capacity is **16**, and two of the texts it caches vary with the call: the locator's
carries one placeholder per page width (`LOOKUP_PAGE_IDS = 128`), the object insert
one per chunk size. Isolated, on the run's own Store with every SQL text built before
the timer ([`statement-cache-probe.rs.txt`](statement-cache-probe.rs.txt),
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

So one width over capacity turns a 108 ns hit into a 10.7 µs miss. **The workload does
not do that.** A measurement-only probe, enabled for one declared diagnostic row
([`cache-probe.rs.txt`](cache-probe.rs.txt), `runs/width-probe-history-stride10`),
counts what a real stride10 run issues:

| call site | calls | identifiers | distinct widths | distribution |
| --- | ---: | ---: | ---: | --- |
| locator (`lookup::candidates`) | 380,444 | 443,574 | **126** | **1 × 376,398 (99.0 %)**, 2 × 2,141, 3 × 627, 4 × 214, 128 × 67, then ~120 widths with counts of tens |
| object insert (`write::insert_objects`) | 44,334 | 52,032 | 68 | **1 × 44,143 (99.6 %)**, 128 × 22, then tens |

The distinct-width sets exceed the LRU, but the **hot width is 1 and it stays
resident**, so the misses are confined to ~4,000 locator calls and ~200 insert calls —
tens of milliseconds, not seconds. **Both candidate explanations for the ≈6.5 µs per
locator call that separates the in-run cost (10.00 µs) from the isolated one
(3.488 µs) are therefore refuted**: it is not text construction (a one-placeholder
`format!` is tens of nanoseconds, not 6.75 µs — that figure belongs to the 128-wide
text, which occurs 67 times a run) and it is not cache thrash. **Raising the cache
capacity is not part of this treatment**; it would buy tens of milliseconds and cost
memory.

**D3 — what one per-step statement costs** (previous round, retained): prepared fresh
against cached, on the same connection — `UPDATE object_packs SET data …` **4,723 ns
against 102 ns** (46,049 calls a run), `SELECT next_pack_id …` **1,106 ns against
83 ns** (48,446), `UPDATE store_policy …` 1,966 ns against 84 ns (now 255 calls),
`UPDATE saves SET pack_ceiling …` 1,926 ns against 86 ns (now 255 calls).

## The one treatment, pre-registered

**A statement the step issues every time is prepared once.**

Every SQL text the per-step path issues goes through the connection's
prepared-statement cache instead of being parsed again on each call:

| text | issued | today |
| --- | ---: | --- |
| `BEGIN IMMEDIATE` | 48,446 | parsed per call (`execute_batch`) |
| `COMMIT` | 48,446 | parsed per call (`execute_batch`) |
| `UPDATE object_packs SET data = ?2 …` | 46,049 | parsed per call |
| `INSERT INTO object_packs …` | 255 | parsed per call |
| `SELECT next_pack_id FROM store_policy WHERE id = 1` | 48,446 | parsed per call |
| `ROLLBACK` | failure paths only | parsed per call |

Nothing else changes: the same statements, in the same order, with the same
parameters and the same counters. The cache capacity stays at its default: D4 shows
the hot working set is one locator text, one insert text, the pool texts and now these
six — comfortably inside 16, with the tail already missing either way.

## What the treatment must not change

- **The Store, every counter, the cadence, the arbitration invariants.** Untouched.
- **The locator's cost.** `resolve_ns` must not rise: the treatment adds six entries to
  the LRU the locator also uses, and D4 says the hot entries survive that — this is a
  falsifier, not an assumption.

## What it must move, stated in advance

| term | shipped | predicted | why |
| --- | ---: | ---: | --- |
| stride10 operation | 16.6 s | **≤ 16.35 s** | 6.8 µs of parse per step over 48,446 steps ≈ 0.33 s |
| `sql_ns` | 1.179 s | ≤ 0.96 s | the pack `UPDATE`'s parse (4,723 ns × 46,049 ≈ 0.22 s) is charged here |
| `commit_ns` | 1.806 s | ≤ 1.79 s | `COMMIT`'s own parse, ≈0.02 s |
| `resolve_ns` | 4.382 s | **not above 4.45 s** | the locator must keep its cache entries |

## The falsifier

- If the saved Store does not hash
  `7ea2fe6ccf13bc5aee7ba59bddef2d60d61d50d6b90329ee1488b1f096228358`, or any workload
  counter differs: **withdrawn**.
- If `resolve_ns` rises beyond 4.45 s, the treatment evicted the locator's hot
  entries and is **withdrawn** whatever the total says.
- If the operation does not fall below 16.35 s, or `sql_ns` does not fall, the parse
  was not where the measurement said: **withdrawn**.
- If the second writer is refused or its step latency leaves the retained band, or
  `multi_writer.rs` is not green: **failed**, however fast the single-writer number is.

## How the pair is measured

Both arms from **one binary**, selected by a measurement-only environment lever that
is removed before the round closes (`LAYERFS_STORAGE_STATEMENT_CACHE=0` restores
today's per-call parsing): the round's gate pair plus an **A B B A** diagnostic, as in
the previous round, because the machine's between-sample level is larger than the
effect being measured. One sample per arm per order, fresh `--output`, both global
flocks, quiet preflight, caps unchanged, the second writer measured on the shipped
step.
