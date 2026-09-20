# Correctness of the retained treatment

> Status: Research; what was checked, and what was not.

**The change.** `PoolReader::leaf_canonical_with_groups` visits a pooled leaf's
rows in ascending ordinal order instead of the body's serial order, and writes each
resolved value back at its own row's index. Nothing else changes.

**Why it is equivalent.**

- `decode_pooled_body` returns every row with its ordinal before any lookup; the
  visit order is an implementation detail of the resolution loop, not part of the
  leaf's grammar.
- Every row resolves its own value through `group_value(.., ordinal)`, which
  validates the ordinal against the covering row's span and returns that row's
  value. The covering row for an ordinal does not depend on which row was resolved
  before it.
- The decoded-value cache is keyed by `first_ordinal` and the per-chain
  `decoded_work` charge is a sum; both are order-independent.
- `values[position]` places each value at its own row's index, so the `values`
  slice handed to `rebuild_leaf(&prefix, &rows, &values)` is the same slice the
  serial-order loop produced.
- The sort is over at most `MAXIMUM_LEAF_ROWS` (100) positions with distinct keys.

**What is preserved.** Header and directory validation, lane checks, group extent
and decoded-length checks, digest authentication of the group body, the publication
ceiling check before the cache, per-chain canonical/encoded/decoded work budgets,
record framing and limits, canonical identity authentication and every declared
operation boundary. `group_for`, its cached statement and its ordinal-range
validation are untouched.

**Declared difference.** When a leaf holds more than one offending ordinal, the
refusal may be raised at a different row than before, because rows are now visited
in ordinal order. The error kind for a given ordinal is unchanged.

**Evidence.** Both cases' saved Stores are **byte-identical** between the arms, all
17 / 53 state roots match, and the canonical and value-group inventories match.
Both arms read back 1,083 / 3,377 sampled paths with zero mismatch, zero missing
and zero unexpected.

**Not established.** The read-back is a declared sample, not exhaustive read-back
of every path. No new corruption, ceiling, cancellation or adversarial test was
added for this change: it adds no new failure mode, but it also adds no new
coverage, and the existing `visibility`, `metadata_pool` and
`catalogue_statement_reuse` tests are the ones that exercise the surrounding
checks. The 7,817-nanosecond per-statement cost is measured, not decomposed into
engine time and wrapper time. No exclusive SQL or codec CPU is claimed.
