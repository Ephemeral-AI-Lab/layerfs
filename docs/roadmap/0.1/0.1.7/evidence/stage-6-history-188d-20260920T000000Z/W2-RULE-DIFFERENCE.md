# The product consults the similarity index more broadly than the harness arm does

**Flagged by W2, verified in source by the parent.** This changes what the campaign's headline means.

## The two rules

| | when is the similarity index consulted? |
| --- | --- |
| **the harness arm** (@ops/history.rs:1759-1768@) | only when **nothing was DECLARED** — @match previous { Some(p) => vec![p], None => cross_path_predecessors(...) }@ |
| **the product** (@encoding/delta/select.rs:392-402@ then @:254-262@) | when nothing was **ACQUIRED** — @acquisition()@ loops the advisory list, returns @Some@ only if @probe@ succeeds, and falls through to @None@ when every declared candidate is **ineligible**; only then does the caller reach @input.candidates.find(...)@ |

**"Declared but ineligible" is the gap.** The harness arm never consults the index in that case; the product
does.

## Why it matters

The product's rule is **strictly broader**: it consults the index in every case the harness arm does, plus
the declared-but-ineligible case. In that extra case the object would otherwise be offered nothing, so the
index can only **add** candidates — it cannot displace a declared base that was going to be used, because
there wasn't one.

**Therefore the harness arm's 49,672,192 B is a LOWER BOUND on what the product's own index can do**, and
the campaign's headline figure understates the product.

**Label: hypothesis until measured.** "Broader consultation" is not automatically "fewer bytes" — the
four-slot arm is the standing proof that adding candidates can cost bytes. But that arm *replaced* good
declared bases; this rule adds only where a declared base was **refused**. W2 is measuring it.

## A consequence for how the campaign's numbers should be read

Every figure this campaign has quoted for the similarity arm came from the **harness's narrower rule**. If
the product's broader rule lands lower, then:

- the gate is closer than 356,352 B, by an amount not yet measured;
- and the harness arm should be re-run with the broader rule so the two are comparable.

## Also recorded from W2

**The saturation mechanism, measured:** @delta.no_candidate@ is incremented and **only then**
@candidates.insert@ is called; @delta.prefix_selected@ objects are **never** inserted. So the index holds
only the objects that **failed to get a base** — its size is bounded by the @no_candidate@ population, not
by the object count. That is why it saturates at 7,614 entries and why the ring never binds above 8,192.

**The memory arithmetic, measured:** 8,192 x 68 B + 65,536 x 2 B = **688,128 B = 672 KiB**, asserted in an
external test via @Store::content_index_bytes()@. The 32-bit fold takes the entry from 96 B
(32 id + 64 signature; @Option@ -> 100) to **68 B**. 32,768 slots would be **2,359,296 B more**.
