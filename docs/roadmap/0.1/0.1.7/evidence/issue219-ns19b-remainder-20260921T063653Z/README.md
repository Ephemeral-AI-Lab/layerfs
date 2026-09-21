# #219 round 2 — two refuted treatments and the row's own noise floor

> **Continued in [`../issue219-ns19c-cadence-20260921T065326Z/`](../issue219-ns19c-cadence-20260921T065326Z/):**
> the T3 pre-registration filed here was run, and it landed — `operation_ns` 2714.3 -> 2065.0 ms
> (−23.92 %). This file is round 2's own report and is not rewritten.

Pre-registration: `pre-registration.md` (written before the first run of this round). Control:
**T1c** (`a5c54df16`, `operation_ns` 2776.3 ms). Raw receipts: `receipts/`.

**Summary: both treatments of this round are withdrawn.** The fixed-width query text is
**refuted with numbers** (`collision_query_ns` 81.4 -> 511.1 ms); the batched collision query is
semantically verified but its predicted ~27 ms is **below this row's noise floor**, so it is
reverted rather than claimed. The round's real product is a **measurement finding**: the row carries
a **7-255 ms resource-release transient** that no previous row separated from accept-path work.

## 1. T4b — fixed-width query text: REFUTED, reverted

The registration predicted that building the locator query's SQL text at a constant page width
would turn 587 x 2 statement re-parses into cache hits and save <= 55 ms. It did the opposite:

| instrument | T1c | D2T4 (T4a+T4b) | movement |
| --- | ---: | ---: | ---: |
| `diag_collision_query_ns` | 81.4 ms | **511.1 ms** | **+429.7 ms** |
| `operation_ns` | 2776.3 ms | 3098.5 ms | +322.2 ms |
| `cpu_user_ns` | 1641.0 ms | 2020.7 ms | +379.7 ms |

The mechanism is the one the registration got wrong: **the statement cache was never the cost, the
plan is.** `x IN (?,?,...?)` with 128 parameters is a different query to SQLite's planner than the
same predicate with one parameter, and the 128-term list is planned as an ephemeral-index build
over `objects` instead of a primary-key seek per element. A fixed text bought a cache hit and paid
for it with 16,802 worse plans. Every pinned counter and the root digest were unchanged, so the
change was *semantically* invisible and *plan-wise* disastrous: a reminder that a query-shape
change has to be measured on the plan, not on the statement count.

Reverted in full (`src/sqlite/lookup.rs` is back at `a5c54df16`).

## 2. T4a — one paged collision query per seal: UNMEASURABLE, reverted

`validate_candidates` issued one `SELECT` per row (25,245 over 16,802 seals); it became one paged
call per seal. The D2T4 row shows it is semantically sound - `commits` 17378, `inserted` 25245,
`presence_queries` 398, `statements` 16595, `full_records` 25241, `prefix_records` 4 and the
filesystem root all identical, row PASS - but its effect was never isolated, because T4b ran in the
same arm and the two terms cannot be separated after the fact.

Its predicted movement is **~27 ms** (25,245 -> 16,802 statements at the measured 3.2 us per
statement). The row's own spread on identical product code is **2714.3 - 2776.3 ms** (D2b against
T1c, same source, different instrument), i.e. 62 ms, and its release transient alone spans 7-255 ms.
**A 27 ms prediction cannot be tested at one sample per arm in this harness**, so the change is
reverted rather than landed on an argument. It is retained here as a described, unclaimed
improvement; the next round that can measure 30 ms should take it.

## 3. D2 — the finish span, decomposed: a 7-255 ms release transient

The registration asked where the 257.5 ms of `span_finish_ns` lives, since `finish_inner` is 1.2 ms
and the final drain was bounded above at 0.8 ms by subtraction. D2 charges the three parts by name
and the harness charges the telemetry node as a fourth. Measured:

| row | `span_finish_ns` | `timing.child()` | finish call | drain | `finish_inner` | **owner drop** | residue |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| A1 | 258.4 ms | not measured | not measured | not measured | 1.4 ms | *(by subtraction)* ~256 ms | - |
| T1 | 243.3 ms | not measured | not measured | not measured | 1.1 ms | *(by subtraction)* ~241 ms | - |
| T1b | 242.6 ms | not measured | not measured | not measured | 1.1 ms | *(by subtraction)* ~240 ms | - |
| T1c | 257.5 ms | not measured | not measured | not measured | 1.2 ms | *(by subtraction)* ~255 ms | - |
| D2T4 | 13.633 ms | 0.001 ms | 13.632 ms | 5.570 ms | 1.361 ms | **6.699 ms** | 0.003 ms |
| D2b | 39.749 ms | 0.000 ms | 39.748 ms | 4.214 ms | 1.109 ms | **34.423 ms** | 0.001 ms |

The two instrumented rows close the span to 1-3 us, so the decomposition is exact: **the unnamed
term is the drop of the save's owner** - the connection, the codec workspaces, the retained pack
tails and the save's private candidate and pool index clones. Across six rows of *identical product
code* that one drop costs **6.7 ms, 34.4 ms and (by subtraction) ~240-256 ms**, i.e. up to **9 % of
`operation_ns`**, and it is not accept-path work: nothing is stored by releasing memory.

**The cause is NOT_ESTABLISHED and is recorded as such.** The four high rows ran 05:48-06:18 UTC and
the two low ones 06:39-06:42, so the term is window-correlated; the obvious interference hypothesis
was checked and **not supported** - no other worktree in `/Users/yifanxu/Ephemeral-AI-Lab` wrote a
result file in that window and no `fs-bench` run was active (`ps` showed one `--list` invocation in
`layerfs-190-scope`). The round does not claim a mechanism it did not measure.

**What it means for every future round on this row, stated plainly:** a predicted movement below
~250 ms on `operation_ns` cannot be tested at one sample per arm here until this term is bounded.
Round 1's -747.8 ms is unaffected - both of its arms carry the transient (A1 258.4, T1c 257.5) - but
**the row's level includes it**, and any comparison against a row from a different window must
state which side of the split each row sits on.

**The boundary is deliberately NOT moved.** Ending the accept span at the acknowledgement instead of
after the drop would remove the term from the phase, and that is exactly the move the measurement
contract forbids: it is work the process does inside the timed region, and the v0.1.6 comparison
arm's timer includes its own teardown. The term is published, not excluded.

## 4. What did NOT change

No pinned counter and no root digest moved in any row of this round, and `SCHEMA_VERSION`,
the pack framing and every product decision are exactly `a5c54df16`'s. The only landed change is
the instrument (D2): three `Instant` pairs in `SaveOperation::finish` plus one per seal, charged to
`DiagProfile`, which `SaveOutcome`'s equality ignores and `total_ns()` does not include.

Checks as run on the landed tree: `cargo test -p layerfs-storage` **33 binaries, 0 failed**;
`cargo fmt -p layerfs-storage -- --check` clean; the measured row PASS with 13/13 gates and 14/14
pinned counters. **Not run:** the whole-workspace test (round 1 ran it on the same product code;
this round changes three charge sites and one harness span), and any other harness case or lane.

## 5. Files

| file | what it is |
| --- | --- |
| `pre-registration.md` | the registration, including the withdrawn 400-600 ms prediction for T3 |
| `receipts/ns19-T1c-final-…` | the control |
| `receipts/ns19-D2T4-query-…` | T4a + T4b, the refuted arm |
| `receipts/ns19-D2b-instrument-…` | the instrument alone, the row this round lands |
