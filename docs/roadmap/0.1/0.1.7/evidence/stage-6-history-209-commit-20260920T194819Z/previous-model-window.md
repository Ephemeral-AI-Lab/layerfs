# Addendum — the per-append multiple, and the regression, re-measured in one window

> Status: Research; **diagnostic evidence, not release admission**. Addendum to
> [`README.md`](README.md) and correction to a figure the lane has been quoting.
> One sample per arm per order, four rows, all retained.

## What prompted it

The round's report and its answer quoted the per-append commit price of the shipped
model against the previous model's **9.5 µs**, taken from
[`gap-attribution.md`](../stage-6-history-209-rca-20260920T191016Z/gap-attribution.md)
§3, and called the multiple **~4×**. That comparison divides a figure measured in
*this* window by a figure measured in the **2026-09-20T08:10Z** window, which is the
class of comparison this lane withdraws. The previous-model binary was never
archived, so the multiple could not be re-measured without rebuilding it. It has
been.

## The previous model, rebuilt and validated

`git worktree add … f039bcaf2` — the commit the retained row's receipt names — then
`cargo +1.85.1 build --release --locked`. The rebuilt binary is
`3d818895f70b99f48eba9970e08caf36d2f91b81f59a8bb9175e1d795b82a2a9`, archived as
[`binary-archive/previous-model-rebuild`](binary-archive/previous-model-rebuild); it
is **not** the original `418ee508…`, which is not retained anywhere.

It is validated as the same operation by the strongest available evidence: **both of
its rows save the Store to the retained previous-model constant
`4af37932aa3391b12269de8130b9c66dc504f64ca78fc3e585f7afddabed8487`**, byte-identical
to the row measured twelve hours earlier, with the same counters (`commits` 1,149,
`pack_appends` 45,791, `inserted` 52,032, `chain.objects` 107,628). The harness source
seal is `04bcfab573ab40544981ad4358883cad97af7a8eae0a46e2c095285b40d301e0` in every
row on both sides — **the harness is identical; only the product crates differ**,
which is what makes the pair harness-matched.

## The four rows, balanced in order

Run back to back, both global flocks, quiet preflight, fresh `--output` each, in the
order `prev → shipped → shipped → prev` so that the corpus-residency asymmetry the
first pair revealed (whichever arm runs first leaves 0 resident pages, the second
5,141) is **balanced across the two models** rather than charged to one:

| row | order | operation | `commit_ns` | per append | commits | corpus resident |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `prev-model` | 1 prev | 11.033 s | 0.242 s | 5.29 µs | 1,149 | 0 |
| `shipped-b` | 2 shipped | 16.481 s | 1.794 s | 39.18 µs | 48,446 | 5,141 |
| `shipped-c` | 3 shipped | 16.714 s | 1.817 s | 39.68 µs | 48,446 | 0 |
| `prev-model-b` | 4 prev | 10.921 s | 0.228 s | 4.98 µs | 1,149 | 5,141 |

| pair | operation | `commit_ns` | per append |
| --- | ---: | ---: | ---: |
| prev → shipped | 11.033 → 16.481 s (**1.49×**) | 0.242 → 1.794 s (**7.40×**) | 5.29 → 39.18 µs (**7.40×**) |
| shipped → prev | 10.921 → 16.714 s (**1.53×**) | 0.228 → 1.817 s (**7.96×**) | 4.98 → 39.68 µs (**7.96×**) |
| **balanced means** | 10.977 → 16.598 s (**1.51×**) | 0.235 → 1.806 s (**7.67×**) | **5.14 → 39.43 µs (7.67×)** |

## What this corrects

1. **The per-append commit multiple is ~7.7×, not ~4×.** The 4× divided this
   window's 36.6–38.7 µs by the 08:10 window's 9.5 µs. The ratio is stable across
   windows — the 08:10 same-session pair (`cp-explicit1` 0.437 s / 45,791 appends
   against `mw-history-stride10` 3.133 s / 45,794) gives **7.17×** — while the
   absolute prices are not: the previous model reads 9.54 µs there and 5.14 µs here,
   the multi-writer model 68.4 µs there and 39.4 µs here. **Both sides scale with the
   window, so the ratio survives and the absolute figures do not.**
2. **`gap-attribution.md` §3's "6.8×" is a units mismatch.** It divided 65.1 µs — the
   shipped model's `commit_ns` per **commit** — by 9.5 µs per **append**. Per append
   the shipped model reads 3.154/45,794 = 68.9 µs, so the same-session multiple is
   **7.2×**, not 6.8×. The conclusion the section draws is unchanged; the number is.
3. **The regression itself is window-dependent, and today it is 1.51×, not 2.02×.**
   The #205 pair (`cp-explicit1` 16.360 s against `mw-history-stride10` 33.123 s,
   both 2026-09-20T08:10Z) is a same-session pair and stands as measured. In this
   window the same two models read 10.977 s and 16.598 s. Both got faster, by
   different factors, so **the operation-level multiple moved while the commit
   multiple did not** — which is itself evidence about where the regression lives.

## The gap, decomposed in this window

Balanced means, the seven buckets billed by the same instrument plus the spans the
buckets do not charge:

| term | previous | shipped | Δ | share of the gap |
| --- | ---: | ---: | ---: | ---: |
| operation | 10.977 s | 16.598 s | **+5.621 s** | — |
| `commit_ns` | 0.235 s | 1.806 s | +1.570 s | 27.9 % |
| `resolve_ns` | 2.854 s | 4.382 s | +1.528 s | 27.2 % |
| `filesystem` | 3.502 s | 4.483 s | +0.980 s | 17.4 % |
| uncharged inside the accept span | 0.341 s | 1.120 s | +0.780 s | 13.9 % |
| `sql_ns` | 0.678 s | 1.179 s | +0.501 s | 8.9 % |
| `full_ns` + `delta_ns` + `place_ns` + `group_ns` | 1.754 s | 1.809 s | +0.055 s | 1.0 % |
| `content` (outside accept) | 1.098 s | 1.106 s | +0.008 s | 0.1 % |
| other spans outside `accept_loop` | — | — | +0.199 s | 3.5 % |

`commit_ns` and `resolve_ns` are within 3 % of each other as the two largest terms —
which is exactly the shape the round's own treatment was aimed at, and it says the
next two rounds are `resolve_ns` and `filesystem`, not `commit_ns` again.

## Custody

Four rows, one sample per arm per order, fresh `--output` each, both global flocks
held, quiet preflight per `collect.py`; one deferral was written and retained during
this addendum. Corpus residency is **balanced** (each model one cold row, one warm
row) and declared; the timed operation's own phases do not move with it (`content`
+0.008 s between the models, `filesystem` *worse* for the warmer arm). Every row's
Store hash is the constant of its own model, in both orders. Two binaries are
necessarily compared — the two models cannot be one binary — and the harness seal is
identical on both sides; the rebuild is validated by the byte-identical Store, and
the original binary's absence is stated rather than glossed.

Reproduce with `analyze.py` (extended for these rows) or:

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-commit-20260920T194819Z/with_locks.py \
  prev-model-stride10 \
  python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-commit-20260920T194819Z/collect.py \
    prev-model history-stride10 \
    --binary /Users/yifanxu/Ephemeral-AI-Lab/layerfs-205-prev/core/benchmark/fs-bench-pro-storage-content/target/release/fs-bench-storage-content \
    --cwd /Users/yifanxu/Ephemeral-AI-Lab/layerfs-205-prev \
    --seal-repo /Users/yifanxu/Ephemeral-AI-Lab/layerfs-205-prev --pre-execute
```
