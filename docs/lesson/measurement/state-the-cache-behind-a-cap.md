# State the cache behind a cap

> **Status:** Lesson; distilled from merged work, informative and not a product contract.
>
> **Category:** measurement · **Applies when:** a target, cap or ceiling is set or judged · **From:** [#816](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816)

## Rule

Record the cache state and counting method with every target, and judge a result
only against a target counted the same way.

## What happened

#816 set caps of 40 PostgreSQL round trips for B5e and C1 from counts taken while
the Workspace's base pages were already cached: v2 measured 52 and 37. The final
runs used a checked-cold harness, where the Workspace readers fetch base pages
from PostgreSQL. Cold, v2 itself measured 90 and 82, and the candidate 53 and 58,
which read as misses.

Split by role, the candidate's writer trips were 28 and 27, as in the warm count.
The excess was 25 and 31 reader trips, which the change never touched. The caps
were waived only after the two counts were made comparable.

## How to spot it

- A target written as a bare number, without the cache state, harness or roles it
  counts.
- A control arm that lands on the other side of the target from its baseline, as
  C1 did: 37 warm, 82 cold.
- A miss that sits entirely in a role or phase the change did not touch.

## What to do

- Next to each cap, record the cache state and how it was enforced, the harness and
  source, which roles count (writer, readers) and the phases in scope.
- Before judging, measure the control arm under the same state. A cap that the
  control arm misses by far under that state was set under a different one.
- Report counts by role and phase, so a miss points at the part that moved.

## Limits

This lesson is about comparing counts. Which cache state may support a timing
claim is still governed by the
[measurement workflow §1](../../general/agent-measurement-policy.md#1-a-warm-cache-must-never-credit-a-measured-phase):
a warm cache must never credit a measured phase. Nothing here relaxes that.

## Evidence

The warm counts and caps are in the
[#816 issue](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816); the cold
counts by role and the waiver are in its
[closing comment](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7202676223).
