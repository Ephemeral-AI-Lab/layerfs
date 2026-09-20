# #209 update — root cause found, one treatment landed, capability measured

**Status: Research; diagnostic. Not a release claim. #209 stays open.**

## The root cause is a SQL clause, and the commit message blamed the wrong bucket

The handoff ordered the RCA before any fix, because `commit_ns` (+2.696 s) and
`resolve_ns` (+6.108 s) disagreed. They do not disagree any more. `resolve_ns` is a
**symptom**.

The engine attribution puts **10.771 s of the 10.411 s `resolve_ns` bucket** into the
locator query `sqlite/lookup.rs::candidates` — 380,380 queries at **28.32 µs** each, in
one stride10 run. Two one-difference arms separate the mechanisms:

| arm | the one difference | operation | `commit_ns` | `resolve_ns` | per locator |
| --- | --- | ---: | ---: | ---: | ---: |
| `instr0` | — (shipped `eb319aaa9`) | **33.116 s** | 3.015 s | 10.411 s | **28.32 µs** |
| `c1nocommit` | every step commit disabled | **25.523 s** | 0.065 s | 7.987 s | **21.78 µs** |
| `c2locator` | the locator `ORDER BY`/`LIMIT` removed | **25.630 s** | 3.039 s | **6.691 s** | **10.00 µs** |

So the 42× cadence is real but **second** (7.59 s), and the dominant per-query cost is
the `LIMIT` (isolated: previous-model query 4.559 µs, new-model query with publication
scoping 3.488 µs — *faster* — and the same query with `ORDER BY … LIMIT ?` 15.543 µs;
`ORDER BY` alone 3.485 µs, `LIMIT` alone 13.840 µs). It is paid **inside** the step, so
no commit cadence avoids it.

## The treatment, and what it did not touch

Remove the `ORDER BY … LIMIT ?` and its bound parameter. **Nothing else.** Operation
**33.116 → 26.467 s (−6.65 s, −20.1 %)** on matched clean arms; **commits unchanged at
48,446**, so multi-writer cadence is untouched. All 31 workload counters identical and
the saved Store **byte-identical** (`7ea2fe6ccf13bc5a…`). Publication scoping, collision
checking, the ownership watermark and failure cleanup are unchanged.

## The design tension has no interior

The handoff asked whether the step can be enlarged. **It cannot: it fails.** A
measurement-only, default-1 lever committed once per N steps — N=1 passes in 5/5 rounds
with zero `OwnershipUnavailable`; **N=8, 64, 100000 all fail in round 0** with
`CleanupFailed { original: OwnershipUnavailable, cleanup: OwnershipUnavailable }`,
because `busy_timeout` is zero by declared profile and the refused save's cleanup needs
the lock it cannot get. A widened step does not cost the second writer time; it costs
the second writer the save.

## Multi-writer capability: measured, retained

Shipped step, two writers, same 24 MiB payload, same binary: **1,290 of 1,306 calls
sub-millisecond**; the ~50 that are not are the group seals. The step a second writer
can be locked out of is **p50 0.4 µs / p99 ≈ 11 ms / worst observed 37.6 ms**, and the
writer that owns no lock pays it (solo mean 120–134 µs vs pair mean 400–413 µs for the
same calls). **Both writers finished in every round; zero `OwnershipUnavailable`.**

## Still open

10.11 s remains against the held 16.360 s: `commit_ns` +2.72 s, `filesystem` +3.35 s
(out of this RCA's scope), and **≈4.8 s of accept growth the seven buckets do not
charge**, whose largest named candidate is `placement.rs::write_pack` — a
whole-pack-body UPDATE charged to no bucket (255 packs, 45,298,203 bytes, ~8.1 GB of
BLOB rewritten over 45,794 appends). That is arithmetic over retained counters, **not a
time measurement**, and it is the next experiment this lane would run.

Receipts: `docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-rca-20260920T191016Z/`.
Ledger entry **L54**. Product commit `63fa15c49`.
