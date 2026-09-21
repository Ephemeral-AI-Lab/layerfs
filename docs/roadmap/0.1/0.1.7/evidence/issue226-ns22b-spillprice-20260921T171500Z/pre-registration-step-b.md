# Pre-registration — #226, the content stream's copy, and what it is worth

Written **before the first locked run** of this step: no `runner.py perf` had been invoked from this
worktree after the change it registers. It is step 1b of the round that removed the prefix snapshots, and
it is the only option of the design page's four that improves speed without adding a read.

## 1. What changes

`ops/pipeline.rs`'s content stream offered each object to the save through
`content.cloned_object(id).cloned()` — a deep copy of every canonical object, **inside the timer**
(`ops/pipeline.rs:1118-1124` before this change). `Store::accept` takes the object **by value**
(`cas/store.rs:504`), so the copy bought nothing: the operation can be handed the object the harness
already holds. `TreeStore::drain` moves them out once (`providers.rs`), the loop counts what it offered,
and `g6.content-complete` requires the offered count to equal the constructed count. This is the design
page's **option B**, and it is the one option that removes mechanism rather than adding it.

It is taken **now**, ahead of option A, because A would remove this same copy anyway and because B's
price does not depend on which of A or C2 is eventually chosen.

## 2. What was measured before registering a prediction

`tests/spill_read_price.rs`, `the_timed_clone_of_the_content_stream`, an unlocked diagnostic on the
changed tree: the driver's own two calls over the whole content store cost **79,174,500 ns** for
**109,373 objects / 502,912,427 canonical bytes** — **2.23 %** of the row's declared figure at
6.35 GB/s of memory bandwidth.

**The clone is transient, and that is the correction this step registers rather than hides.** The loop
copies one object, hands it over, and drops it, so the copy never accumulates: a diagnostic on the
changed tree reports `heap.peak_incremental_bytes` 36,307,687 against the pre-change row's 27,557,706,
and the measured-region RSS increment 31,997,952 against 36,929,536 — both **inside the session's own
teardown swing** (round 22 report §8.2) and neither attributable to this change. **Option B buys time and
does not buy memory.** The registered prediction below says so in advance so that a flat memory result is
not read as a failure.

## 3. Registered predictions

| # | quantity | prediction | basis |
| --- | --- | --- | --- |
| 1 | all fifteen pins, both `pipeline.*` rows | **reproduce exactly**, both filesystem roots | the change is harness-only; the digest is the check that the same objects reach the same places |
| 2 | `pipeline.content_objects_offered` | **109,414** at 100,000 entries, **24,863** at 10,000 | equals `pipeline.content_objects`; gated by `g6.content-complete` |
| 3 | `pipeline.operation_work_ns`, 100,000 row | **1.5–4 % below** this round's `ns22-D2` (3,640,415,749) → **3,494,799,119–3,585,809,513** | the measured copy, 2.23 %, minus the session's ±1.3 % run-to-run spread |
| 4 | measured-region **increment** | **within ±20 %** of `ns22-D2`'s 36,929,536 — i.e. this step claims **no** memory improvement | §2: the copy is transient |
| 5 | lifetime peak RSS | **within ±2 %** of `ns22-D2`'s 885,325,824 | the fixture dominates and is unchanged |
| 6 | complete command | ≤ 15 s | `AGENTS.md` §3.7 |
| 7 | session anchor `namespace-10000` | inside ±20 % of the 68,514,625 reference | round 21 §2's rule |

**Refuted if** a pin moves, if `content_objects_offered` differs from `content_objects`, if the declared
figure does **not** fall by at least 1.5 % (the copy was not what the diagnostic measured), or if the
lifetime peak moves by more than 2 % (the change reached the fixture, which it must not).

## 4. The ruling this step relies on

**Option B needs no owner ruling — the design page §3 says so, and the handoff's §3 repeats it.** It
moves no boundary, reads no fixture page inside the timer, adds no cache state and changes nothing about
what the row measures: the same objects reach the same save operation in the same order, and the timer's
boundary is untouched. **What it changes is how much the harness copies while the clock runs.**
