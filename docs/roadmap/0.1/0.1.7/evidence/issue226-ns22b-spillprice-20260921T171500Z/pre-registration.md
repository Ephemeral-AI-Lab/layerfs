# Pre-registration — #226, the cold-read price of a bounded fixture

Written **before the first run** of `tests/spill_read_price.rs`. It registers no row, changes no product
line, and is not evidence; it exists because the design page's §5 says *"A's read is unpriced in wall
time"* and the owner's direction is *"proceed if it can bring better memory usage and speed"*. **It
cannot bring both**, and this diagnostic decides whether it can bring the first without losing the
second.

## 1. What is being decided

Options A and C2 of [`issue226-bounded-fixture-design.md`](../../issue226-bounded-fixture-design.md) each
replace a fixture read **from RAM** with a read of the same ~503 MB **from the device**. Today's row
reads 502,914,928 canonical bytes out of a `HashMap` inside the timer. Under either option the harness
adds a cold device read of comparable size.

* If that read costs **≤ 1 %** of the row's declared figure, it is inside the row's own measured
  within-session spread (3.38 %, round 21 §11.6) and the option is **speed-neutral**: memory is bought
  for free, and the direction proceeds.
* If it costs **> 5 %**, the option is **slower than today's row** and is bought with memory rather than
  with speed. **The recommendation then changes**: decline A, and prefer B (which bounds nothing but adds
  no read) over a bounded fixture that costs the row a tenth of its figure.
* Between 1 % and 5 %: proceed, but the round that builds it must publish the read as its own term and
  must not claim the row got faster.

## 2. Registered predictions

| # | quantity | prediction | basis |
| --- | --- | --- | --- |
| 1 | cold read of the spilled fixture | **≤ 1 %** of 3,549,393,833 ns → **≤ 35,493,938 ns** | a 503 MB sequential read on this host's storage, per the harness's own #151/L18 note of 2.1 GiB/s from storage: 502,914,928 B / 2.1 GiB/s ≈ 223 ms is the *pessimistic* end, and the row reads it object by object with a hash per object |
| 2 | the same bytes read back warm | **below** the cold pass | it is the same code with the pages resident |
| 3 | resident pages after `msync(MS_INVALIDATE)` over every spilled file | **0** | the contract's enforcement primitive, `shared/residency.py:154` / `support/instruments.rs:674` |
| 4 | objects read back | **109,414**, bytes **> 500,000,000** | the fixture is deterministic in the declaration and the seed |

**Refuted if** the cold pass is not slower than the warm pass (the de-warm did not take), if any spilled
page is still resident after invalidation (the contract is not enforceable as written), or if the object
count differs from the pinned `pipeline.content_objects`.

## 3. What this does not measure, stated so the number is not over-read

* **Not the whole of option A.** It measures the *read* — 109,414 object files read and re-identified.
  A's real row would read them **inside** the save operation, interleaved with `accept` and the Store's
  own writes, and that interleaving is not measured here. The number below is a **lower bound** on A's
  added cost and an **estimate** of C2's.
* **Not a row.** No lock, no receipt, no `--verify full`, no pinned counter. It writes nothing under
  `benchmark-results`.
* **Not a claim about storage throughput in general.** It is this host, this filesystem, this fixture.
