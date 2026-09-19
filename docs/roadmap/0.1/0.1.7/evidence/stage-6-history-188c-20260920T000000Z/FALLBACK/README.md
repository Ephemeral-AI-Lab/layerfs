# Is the "fallback" an error-driven alternate algorithm, or a cache?

**Answer: it is a cache, and the failure path is the opposite of a fallback.** It is **not** the kind of
fallback this repository forbids.

## What the rule actually is

@```text
  if the caller declared a predecessor for this path:
      offer that one                     <- the caller knows the correspondence
  else:
      consult the content-similarity index   <- a CACHE lookup
  then: run the selector exactly once on whatever list resulted
@```

It is a **precedence rule between two candidate sources**, evaluated *before any work is attempted*.
Nothing has failed when it is evaluated. There is no retry, no second attempt, and no alternate
algorithm selected by an error.

## The evidence, from the product's own source

| claim | where |
| --- | --- |
| A stored FULL is a **policy outcome, never error recovery** | @cas/selection.rs:6@, @:26@ — *"a stored FULL alternative is a policy outcome, never error"* |
| The second source **is a cache** — a content-keyed similarity index | @encoding/delta/candidates.rs@ |
| **A failed acquisition is a hard error, not a fallback** | @select.rs:294@ — @let base = acquire(input, base_id)?;@ — the @?@ propagates and **the save fails**. There is no catch and no "store FULL instead" |
| Choosing FULL when no candidate is found is **counted as policy** | @delta.no_candidate@, @delta.ineligible_candidates@ |
| The rule that forbids the other kind | @core/AGENTS.md@: *"One attempted operation. No automatic retry, busy handler, refresh/reprepare, **error-driven alternate algorithm**…"* |

**The code does the opposite of an error-driven fallback.** When the selector has picked a base and
acquiring it fails, the operation fails. The only thing that is ever "fallen back to" is a *different
candidate source*, and only when the first source is **empty** — not when it is broken.

## The concern that IS real, and is not about legality

The harness maintains that similarity index itself. **The product's own index is owned by one save**
(@cas/lifecycle.rs:90@, dropped at @cas/store.rs:392@), so **a cross-save match is impossible in the
product as it stands.**

There is a real distinction between the two declarations this lane makes:

- **The same-path declaration (R0)** is a *caller* knowing its own previous version. That is legitimate:
  the product's own edit path does exactly it (@file/edit/apply.rs:121@), and the driver is the caller.
- **The cross-path fallback** asks the *Store* to search its whole history for something similar. The
  harness can do that; the product cannot.

So the honest position is: **the mechanism is legal, but the capability is not yet the product's.**

## Is it an inferior solution? Yes as a *base* — which is why it must not be the primary rule

Measured, fallback off -> on, same binary, one variable:

@```
  apparent                 56,049,664 -> 49,672,192   -6,377,472
  no_candidate                 10,080 ->      6,674   -3,406 objects offered NOTHING
  prefix_selected              35,089 ->     38,447   +3,358 objects newly given a base
  ineligible_candidates           658 ->      2,671
@```

**Without it, 10,080 objects are offered nothing at all.** It reaches **3,406** of them, for
**6,377,472 B** — about 1,875 B each.

**Per object it is genuinely inferior:**

| base | ratio on the whole-file lane |
| --- | --: |
| same-path previous version | ~16.9x |
| **cross-path (this fallback)** | **~10x** |
| **no base at all (FULL)** | **~2.9x** |

**So the choice is not "inferior base vs superior base" — it is "inferior base vs NO base".** 10x beats
2.9x by 3.5x. It is not competing with the same-path rule; the rule fires **only** when
@previous.is_none()@, so it cannot displace a good base.

**And here is the measured proof that using it as a primary rule is much worse.** The four-slot ordered
arm applied cross-path candidates to **every** object, including ones that already had a good same-path
base:

@```
  gained   -8,625,119 B   (3,987 objects it newly delta'd)
  lost    +18,214,699 B   (22,368 objects whose base was REPLACED; 17,141 same-path -> cross-path)
  ------------------------------------------------------------------
  net     +10,242,310 B   WORSE
@```

**That is what "let the inferior base win" costs: 10.4 MB.** The current rule exists precisely to prevent
it.

**The name is the problem.** "Fallback" suggests a degraded mode entered after a failure. It is neither:
it is a **second candidate source**, consulted only when the first is *empty*, and it is evaluated before
anything is attempted.

## State, measured

@```
  fallback OFF  (current default)   56,049,664 B   = 1.13654x v0.1.6   +6,733,824   native 3,957,829
  fallback ON                       49,672,192 B   = 1.00723x v0.1.6     +356,352   native 3,957,829
  v0.1.6                            49,315,840 B
@```

@LAYERFS_HISTORY_FALLBACK_PREDECESSORS=1@ turns it on. **Turning it off costs 6,377,472 B** and puts the
gate 6,733,824 B away instead of 356,352 B.

**The way to have it legitimately is to build the capability in the product** — a persisted, cross-save
content index, which is a new table and a @SCHEMA_VERSION@ bump and needs an owner ruling. That is the
same ~6.4 MB, obtained without modelling anything the product cannot do.
