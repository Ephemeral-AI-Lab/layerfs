# The balance point — and there is probably a better one than either endpoint

**Measured.** Both points are on the SAME tree (W2's index + W3's row grammar); only the codec differs.

@```
  A  codec payload 3 / group 1     49,324,032  = 1.00017x   vs gate      -8,192
  B  codec payload 9 / group 19    45,432,832  = 0.92126x   vs gate  +3,883,008
  ---------------------------------------------------------------------------
  the step buys 3,891,200 B for 7.589 s of codec CPU = 512,742 B per CPU-second
@```

## The correction that matters

**A alone is 8,192 B ABOVE the gate.** "Stick with 49 MB" as-is does **not** meet it — by 0.017 %.

But it is trivially closable **without** the codec step:

@```
  A + VACUUM at close     48,828,416  = 0.99012x   vs gate +487,424   <- BELOW, keeps the 7.589 s
  B                       45,432,832  = 0.92126x   vs gate +3,883,008
@```

**So the real choice is what you spend, not how low you go:**

| option | below the gate by | spends |
| --- | --: | --- |
| **A + VACUUM** | 487,424 B | a **close-time VACUUM — I/O UNMEASURED** |
| **B** | 3,883,008 B | **7.589 s of codec CPU — MEASURED** |

One option spends a measured CPU cost; the other spends an unmeasured close-time cost. **Neither is
obviously right, and that is exactly why it is a decision rather than an optimisation.**

## And the curve says the best point is probably neither

W1's measured marginal curve:

@```
  L3 -> L5     1,408,000 B per CPU-second    <- the best point on the whole curve
  L5 -> L9       227,500
  L9 -> L12       29,400
  L15 -> L19       3,900
  L19 -> L22         444
```

**L3 -> L5 is worth 2.7x more bytes per CPU-second than the L3 -> L9 step actually taken.** If that
marginal rate holds, a payload level of **5** would buy roughly **2.1 MB for ~1.5 s** — taking A to about
**47.2 MB, below the gate, with no VACUUM and a fifth of the CPU.**

**That is a hypothesis, not a measurement** — the lane has never been run at L5. But it is the cheapest
useful experiment on the board: **three lane runs (L3, L5, L9), one arm each**, and the question is settled
with data.

## Recommendation

**Run the three-point sweep before choosing.** The user's instinct — that ~49 MB is the better balance —
is well-founded, and the measurement may well show a point between the endpoints that dominates both.
