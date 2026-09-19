# Why there are blockers, and whether resolving them pays

**Diagnostic.** Every byte figure is measured unless marked @est@. **No timing claim.**

## 1. Why blockers exist — three different kinds, and none is "we don't know how"

| kind | what it is | which levers | resolvable by |
| --- | --- | --- | --- |
| **policy** | a format change requires an **owner amendment** — a new table, a @SCHEMA_VERSION@ bump, existing Stores rejected | the persisted similarity index; L5 | a ruling |
| **evidence** | the cost is **unmeasured**, because the machine was shared for the whole campaign | VACUUM's close-time I/O; the codec level's CPU | one quiet-machine measurement |
| **contract** | @budget.complete-command@ is **40.0 s against a 15 s ceiling** — already failing | any CPU-costing lever makes it worse | its own work; **no storage lever fixes it** |

**Every lever has a designed, measured mechanism.** Nothing here is blocked on feasibility — the blockers
are a ruling, a measurement, and a budget that was already broken before this work started.

## 2. The product index is not an *addition* — it is the *legitimisation*

@```text
  today's default (declared only)          56,049,664     6,733,824 ABOVE the gate
  the similarity capability, as measured   49,672,192       356,352 ABOVE the gate
  v0.1.6 (the gate)                        49,315,840
  the T1 target                            47,048,435
@```

The 6,377,472 B is **already measured** — but the harness supplies the index. **Building it in the
product does not add bytes; it makes the result a product result.** Until then, the measured arm is a
modelling of a capability the Store does not have.

## 3. What resolving them pays — measured, on top of the capability

@```text
  combination                        apparent      vs gate     vs target
  ---------------------------------------------------------------------------
  (capability alone)               49,672,192     -356,352    -2,623,757
  + VACUUM                         49,176,576     +139,264    -2,128,141   <- closes the gate
  + L5 row grammar                 48,435,200     +880,640    -1,386,765   <- closes the gate
  + codec level 3->19              45,401,749   +3,914,091    +1,646,686   <- CLOSES BOTH
  + L5 + VACUUM                    47,939,584   +1,376,256      -891,149   <- closes the gate
  + codec + L5                     44,164,757   +5,151,083    +2,883,678   <- CLOSES BOTH
  + codec + L5 + VACUUM            43,669,141   +5,646,699    +3,379,294   <- CLOSES BOTH
  + everything incl. #185          43,192,824   +6,123,016    +3,855,611   <- CLOSES BOTH
  (positive = BELOW the line = good)
@```

**Yes — the benefits are large, and two levers each close the gate on their own.**

**And the T1 target of 47,048,435 B IS reachable.** V4 reported it was not, and that was true *for the
path V4 costed* (L1 + L4 + L5) — **the codec level was not on it.** Adding it clears the target by
1,646,686 B.

## 4. Ranked by benefit per unit of risk

| rank | lever | closes | cost | risk |
| --: | --- | --- | --- | --- |
| 1 | **codec level 3 -> 19** | **gate + target** (3.9 MB / 1.6 MB margin) | 2 constants, **no format change** | CPU **unmeasured**; the budget already fails. The byte figure is @est@ — its based-half was modelled |
| 2 | **L5 row grammar** | gate (880,640 B margin) | **format change** + one btree seek per chain edge | read cost against a storage gain — the wrong direction on a time-constrained lane |
| 3 | **VACUUM at close** | gate (139,264 B margin) | close-time I/O | **thin**; and it is fragmentation, not content |
| 4 | **the persisted similarity index** | *legitimises* 6,377,472 B | new table + @SCHEMA_VERSION@ bump | **prerequisite** — without it nothing above is a product claim |
| 5 | **#185 cross-role** | nothing | an amendment | **0 B of the gap** — both trees refuse it |

## 5. The honest summary

@```text
  the blockers are not walls:  one ruling, one measurement, and one broken budget
  the benefits are large:      two levers each close the gate alone;
                               the codec level closes the gate AND the target
  the prerequisite:            the product index, so the 6.38 MB is ours and not the harness's
  the open risk:               CPU, on a lane already 2.7x over its command budget
@```

**The one thing that is genuinely unresolved is the budget.** 40.0 s against a 15 s ceiling, with the
corpus reading alone at 15-17 s. **No storage lever touches it**, and the codec level pushes the wrong
way. That is a separate problem with a separate owner.
