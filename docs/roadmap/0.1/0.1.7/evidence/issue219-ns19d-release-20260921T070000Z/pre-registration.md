# Pre-registration — round 4: D3 (probe the release) and T5 (hoist the collision check)

Written 2026-09-21T07:00Z, **before** the run. Control: **T3c**
(`ns19-T3c-pinned-20260921T065326Z`, commit `755bfa21f`): `operation_ns` 2065.0 ms,
`accept_span_ns` 2061.7 ms, `diag_finish_drop_ns` 261.3 ms, `diag_validate_ns` 96.9 ms,
`diag_collision_query_ns` 58.6 ms, 13/13 gates, 14/14 pinned counters.

## D3 — name what the release frees (DIAGNOSTIC)

`diag_finish_drop_ns` is **261.3 ms (12.7 % of the row)** on T3c and **6.7 ms** on a row of the
same code. The drop is the save's owner: the connection, the codec workspaces, the pooled reader's
caches, the delta reader's pack cache, the retained pack tails, and the save's private clones of the
content-signature and pooled value indexes.

D3 releases those structures in charged groups before the residual drop, replacing each with an
empty one of the same type, so the allocator is asked to free exactly what dropping the owner would
have freed. It changes no decision, no byte and no row.

- Predicted: the named groups (caches, index clones, tails) total **< 100 ms**, and the residual —
  the connection and the codec workspaces, which D3 also names separately — holds **>= 150 ms** of
  the 261.3 ms. The reason for the prediction is arithmetic, not intuition: those structures are at
  most ~20 MB together and freeing 20 MB cannot cost 261 ms.
- Refuted if: the named groups total >= 150 ms (then it *is* the frees), or the groups plus the
  residual do not close to `finish_drop_ns`.
- Diagnostic. No pinned counter and no root digest may move.

## T5 — one collision check per wave (THE TREATMENT)

`validate_candidates` runs once per seal (16,802 calls, 25,245 per-row queries) to check the rows a
seal just wrote against rows *other* saves hold under the same identity. Since round 3 a wave's seals
all share one transaction and hold the Store's write lock for its duration, so the rows it compares
against cannot change between seals: **the wave validates every row it wrote in one call at its end**
and the per-seal call remains for a seal outside a wave (`finish_inner`'s lane loop,
`Store::read_batch`'s on-demand seal).

- Predicted: `diag_validate_ns` **96.9 -> <= 20 ms**, `diag_collision_query_ns` **58.6 -> <= 15 ms**
  (16,802 query sets -> ~600), `operation_ns` unchanged within the row's noise.
- Refuted if: any pinned counter or the root digest moves; or `diag_validate_ns` does not fall below
  40 ms; or the row is not PASS.
- A side effect worth stating: a collision now rolls back the whole wave rather than leaving earlier
  seals committed, which is strictly stronger than the behaviour it replaces.

## Method

One sample per case per arm, fresh `--out`, single thread, no re-run. The control is T3c, measured on
the same machine 25 minutes earlier. T5's effect is judged on `diag_validate_ns` and the query count,
which are count-driven and therefore not subject to the ~250 ms release transient that round 2
established; `operation_ns` is reported but is not the instrument of this treatment.
