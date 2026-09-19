# Verification — all three rows, at the DECLARED budget

**Measured.** `--verify-sample` left at its default (64 paths per state). One run per row. The machine was
**not quiet** (load 5.1-6.0, one `cargo` process throughout) — see §3.

## The results

| row | declared path-states | **compared** | per state | **mismatches** | missing | unexpected | wall | budget | verdict |
| --- | --: | --: | --: | --: | --: | --: | --: | --: | --- |
| **stride10** | 101,477 | **1,083** | 64 | **0** | 0 | 0 | **7.5 s** | 15 s | **FITS** |
| **stride3** | 306,861 | **3,377** | 64 | **0** | 0 | 0 | **20.3 s** | 30 s | **FITS** |
| **stride1** | 904,143 | **9,996** | 64 | **0** | 0 | 0 | **64.2 / 90.3 s** | 60 s | **OVER** |

**Every state of every row reads back and matches its oracle**, with `g1.o4-readback` **PASS** and
`g6.verify-sample-declared` **PASS** on all three:

@```
  g1.o4-readback             PASS  9996 sampled paths across 157 states read back:
                                   presence, kind, size and digest all match
  g6.verify-sample-declared  PASS  9996 sampled of 904143 declared path-states, at most 64 per state
```

## 1. The correction that made this affordable

**A full-oracle verification was run first and it was a mistake.** `--verify-sample 1000000` covered
**101,477 of 101,477** path-states for stride10 — a genuinely complete verification, **zero mismatches** —
but it cost **531.6 s against a 15 s budget**, and the same setting would have cost **~79 minutes** on
stride1. It was killed.

**The declared default is 64 paths per state, and it is the right budget:**

@```
  cost model from the full run:  101,477 units in 531.6 s = 5.24 ms/unit
  at 64 paths/state:  stride10   17 x 64 =  1,088  ->  ~5.7 s   (measured 7.5 s)
                      stride3    53 x 64 =  3,392  -> ~17.8 s   (measured 20.3 s)
                      stride1   157 x 64 = 10,048  -> ~52.6 s   (measured 64.2 s)
@```

**The full-coverage stride10 run is retained as a diagnostic** — it is the strongest single verification
this campaign produced (100 % of the oracle, 0 mismatches) and it is labelled as **over budget**, not as a
receipt.

## 2. What the 64-path sample actually covers

**Every state, not a prefix.** `g6.verify-sample-declared` PASS is worded *"the phase read back at least
one path of every state it was asked to verify"*, and the sampler spreads across the whole path order.
**1,083 / 3,377 / 9,996 paths is 64 per state across 17 / 53 / 157 states** — so the sample is per-state
and every state is exercised.

## 3. stride1's wall is OVER, and the measurement is contaminated

**64.2 s and 90.3 s on two runs of the SAME configuration — a 40 % spread.** That spread is the proof: the
machine was at **load 5.1-6.0 with one competing `cargo` process** throughout, so **neither figure is a
clean wall time.** Per `AGENTS.md` §3.7 a selection that cannot fit is *"recorded as `NOT_RUN` with the
measured wall time and the reason — never made to fit by moving work outside the timer, enlarging a
timeout, or shrinking the workload."*

**So stride1's verification is recorded as: correctness PASS (9,996 compared, 0 mismatches), wall
64.2-90.3 s against a 60 s budget, ON A CONTENDED MACHINE.** A quiet re-run is required to settle whether
it fits.

## 4. A harness behaviour worth knowing

**The verify phase refuses to overwrite `phases-verify.json`** (`File exists (os error 17)`). That is
correct append-only behaviour, but it means **a re-run silently produces no verification at all** unless
the previous receipt is removed — two of my runs "completed" in 88 s and 64 s having verified nothing.
The wall time looked plausible; only the gates revealed it.

## Not claimed

- **No timing claim for stride1.** Contaminated, and stated as such.
- **The 217-row lane has not been re-run** since the codec change.
- Production LOC delta for this round: **0** (verification only).
