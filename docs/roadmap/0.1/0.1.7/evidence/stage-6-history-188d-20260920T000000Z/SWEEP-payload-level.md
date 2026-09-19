# The payload-level sweep — and the knee is at **7**, not 9

**Measured.** One variable (`PAYLOAD_LEVEL`); `GROUP_LEVEL = 19`, `ENCODE_WORKSPACE_BYTES = 16 MiB` and
`windowLog = 18` held constant. **Quiet before AND after every arm** (the script checked). One sample per
arm, no best-of. All four Stores re-verified independently with `space.py`, `quick_check = ok`.
`codec.rs` restored to `PAYLOAD_LEVEL = 9` afterwards.

## The curve

| level | apparent | vs v0.1.6 | vs the gate | **CPU s** | real s | RSS MB |
| --: | --: | --: | --: | --: | --: | --: |
| **3** | **49,053,696** | **0.99468x** | **+262,144** | **38.88** | 44.18 | 245.7 |
| 5 | 46,665,728 | 0.94626x | +2,650,112 | 42.05 | 49.08 | 250.8 |
| **7** | **45,830,144** | **0.92932x** | **+3,485,696** | **42.76** | 49.54 | 247.4 |
| 9 | 45,432,832 | 0.92126x | +3,883,008 | 47.48 | 54.35 | 241.0 |

@```
  MARGINAL, adjacent levels
    L3 -> L5     2,387,968 B for +3.17 CPU-s =   753,302 B/CPU-s
    L5 -> L7       835,584 B for +0.71 CPU-s = 1,176,879 B/CPU-s   <- THE KNEE
    L7 -> L9       397,312 B for +4.72 CPU-s =    84,176 B/CPU-s   <- poor
@```

## Three findings

### 1. Level 3 ALREADY CLEARS THE GATE — at the lowest CPU of any arm

@```
  L3 (payload 3, group 19)   49,053,696 B  = 0.99468x   ->  262,144 B BELOW the gate
                             CPU 38.88 s                ->  the CHEAPEST arm measured
@```

**And the cheap win was the GROUP level, not the payload level.** Payload 3 with group **1** was 49,324,032
(8,192 B *above* the gate); payload 3 with group **19** is 49,053,696 — so **group 19 alone bought 270,336 B
for about 1.0 s**, and it is what carries the gate.

### 2. The knee is L7, not L9

**L7 -> L9 costs 4.72 CPU-seconds for 397,312 B** — the worst value on the curve. **L7 is dominated-free:
it takes 3,485,696 B below the gate for +3.88 s over L3.**

### 3. W1's codec-only marginal curve did NOT transfer to the lane

W1 measured the codec instrument and reported **L5 -> L9 = 227,500 B/CPU-s**. On the **lane** the same
region splits into **L5 -> L7 = 1,176,879** and **L7 -> L9 = 84,176**. The instrument's knee and the lane's
knee are in different places, and **the lane is what ships.**

**So W1's shipped `PAYLOAD_LEVEL = 9` is the worst-value point on the measured lane curve.** It was chosen
on a curve that did not transfer — the same class of error as the ring size, and caught the same way.

## The recommendation, as a choice rather than a number

| if you want | level | apparent | vs the gate | CPU |
| --- | --: | --: | --: | --: |
| **the balance you described** | **3** | **49,053,696** | **262,144 B below** | **38.88 s — cheapest** |
| the knee | **7** | 45,830,144 | 3,485,696 B below | 42.76 s (+3.88 s) |
| — | 9 | 45,432,832 | 3,883,008 B below | 47.48 s (+8.60 s) |

**Your instinct — "stick with ~49 MB, it has a good balance of speed and storage" — is exactly right, and
the sweep says it is better than either of us framed it: level 3 lands at 49.05 MB, clears the gate by
262,144 B, and is the fastest configuration measured.**

**L9 buys 3.62 MB more than L3 for 8.60 CPU-seconds. L7 buys 3.22 MB of that for 3.88 s.** The choice
between L3 and L7 is the real one; **L9 is not.**

## Not claimed

One sample per arm. No stride3 confirmation. No verify PASS (declared sample, `INCOMPLETE`). `RSS` is
roughly flat across the arms (241-251 MB) so **memory does not differentiate them** — and the workspace was
held at 16 MiB, so a lower level could use a smaller one; that is a separate, unmeasured saving.
