# Pre-registration — #209 RCA, before the treatment was written

> Status: Research; diagnostic. Written after the RCA's separation arms were
> sampled and **before** the treatment existed in the tree.

## What the RCA named

| arm | operation | commits | `commit_ns` | `resolve_ns` | per-locator query |
| --- | ---: | ---: | ---: | ---: | ---: |
| `instr0` (shipped model, instrumented) | 33.116 s | 48,446 | 3.015 s | 10.411 s | **28.32 µs** |
| `c1nocommit` (same binary, step commits disabled) | 25.523 s | 34 | 0.065 s | 7.987 s | **21.78 µs** |

`lookup::candidates` is the one locator query. Eliminating every step commit — the
whole 42× cadence the commit message blames — recovers **7.593 s** and leaves the
per-locator cost at 21.78 µs. Isolated SQL timing of the two query texts against the
run's own Store, 30,000 calls each: previous model **4.559 µs**, new model **3.488 µs**;
adding `ORDER BY o.object_id,o.save_id LIMIT ?` to the same new-model query:
**15.543 µs**. `ORDER BY` alone: 3.485 µs. `LIMIT` alone: 13.840 µs.

**So the dominant cause is the `LIMIT` clause, not the publication scoping and not the
commit cadence.** It is a per-call cost on every one of 380,380 locator queries, and it
is paid inside the step, so no step size changes it.

## The one treatment, pre-registered

Remove `ORDER BY o.object_id,o.save_id LIMIT ?{}` from the locator query in
`sqlite/lookup.rs::candidates` and stop binding the limit parameter. Nothing else.

## What the treatment must not change

- **Publication scoping.** The `JOIN saves` and the
  `(o.save_id = r.save_id OR s.publication <= r.publication)` predicate are untouched;
  the `pack_id <= ceiling` filter is untouched.
- **Collision checking.** `decode_location`, the eligibility flag and the
  `SAVE_SLOTS` ownership bound are untouched.
- **Ownership watermark, cleanup, failure paths.** Untouched.
- **The commit cadence and the step boundary.** Untouched; multi-writer capability is
  not traded for the speed.
- **Stored bytes.** Every workload counter and the saved Store must be identical to
  the untreatment arm.

## The falsifier

If the saved Store or any workload counter (`statements`, `pack_appends`,
`packs_created`, `commits`, state roots) differs from the untreatment arm, the change
altered the operation rather than its cost and is withdrawn. If the per-locator cost
does not fall below 15 µs, the diagnosis is wrong and the treatment is withdrawn.
