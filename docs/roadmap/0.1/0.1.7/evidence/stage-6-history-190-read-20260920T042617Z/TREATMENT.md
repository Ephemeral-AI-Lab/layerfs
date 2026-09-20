# #190 read-path treatment: one catalogue statement per pooled leaf

> Status: Research; diagnostic evidence, not release admission. Frozen **before**
> the candidate sample was taken.

## What was measured first

One instrumented stride10 diagnostic (`baseline2`, instrument v2) decomposed the
filesystem provider's own elapsed time. The instrument is an aggregate counter
set with no per-object map, identical in both arms, and is a **separate recorded
patch** (`source/instrument-read-path-*.patch`) that is not part of the retained
product change.

| Provider interval | ns | Share |
|---|---:|---:|
| Pack BLOB acquisition, both lanes (142,686 fetches, 12,080,963,142 B copied) | 3,928,606,618 | 43.6% |
| Metadata value-group catalogue SQL (278,927 statements) | 2,180,465,764 | 24.2% |
| Unattributed remainder | 1,339,076,683 | 14.8% |
| Object/locator SQL | 666,844,674 | 7.4% |
| Pooled value materialisation, authentication, decode, conversion | 644,700,604 | 7.1% |
| Ordinary-lane group decompression | 177,929,427 | 2.0% |
| Control-area validation (header + directory) | 47,134,250 | 0.5% |
| Ordinary record decode call | 21,186,780 | 0.2% |
| Canonical leaf re-encode | 17,180,051 | 0.2% |
| Physical pooled body rebuild (delta chain) | 14,238,663 | 0.2% |
| Record framing | 13,122,420 | 0.1% |
| Physical pooled body decode into rows | 1,717,437 | 0.0% |
| **Provider elapsed** | **9,019,067,220** | |

Spans nest; the table is disjoint as stated and its residual against the
provider elapsed is the unattributed row. The ordinary lane's internal split
(control / decode / frame / payload reconstruction) is **not** separated: its
whole `decode_canonical` call is one row. `retained_pack_ceiling` per wave, the
identity hash per leaf and the `group_value` cache lookup per row are **not**
separately measured.

## The mechanism

`PoolReader::leaf_canonical_with_groups` resolved the covering value group with
one `sqlite::pool::group_for` statement per **change of covering group**, memoising
only the previous group. The comment above that loop states the reason it was
believed cheap: *"The rows of one leaf are in ordinal order, so the group covering
a row is almost always the group that covered the row before it."*

The measurement contradicts the premise. A pooled leaf's rows are ordered by
**serial** (`decode_pooled_body` rejects a body whose serials are not strictly
increasing); the ordinal is a separate field. So the covering group changes
roughly as often as the row does:

| Quantity | Value |
|---|---:|
| Pooled leaf resolutions | 11,268 |
| Leaf rows resolved (covering-group hits + catalogue calls) | 677,234 |
| Catalogue statements (`group_for` calls) | 278,927 |
| Statements per leaf | 24.8 |
| Distinct value groups loaded per leaf | 5.8 |
| Catalogue nanoseconds | 2,180,465,764 |
| Nanoseconds per statement | 7,817 |

So each distinct covering group is asked for about four times per leaf, and the
whole 2.180-second interval is repeated catalogue lookup for information the leaf
already has in hand: `decode_pooled_body` returns **every** row's ordinal before
the first lookup happens.

## Predeclared treatment (v2, frozen before the candidate2 samples)

**Resolve a pooled leaf's rows in ordinal order, so the existing covering-group
memo collapses to one catalogue statement per distinct covering group.**

The rows arrive from `decode_pooled_body` in **serial** order, and the memo above
the resolution loop assumes the opposite: its comment says *"the rows of one leaf
are in ordinal order"*. They are not, so the memo only helps when two consecutive
serial-ordered rows happen to share a group, and the leaf asks the catalogue for
24.8 groups (stride10) / 31.8 groups (stride3) when it touches only 5.80 / 8.77.

The treatment sorts the row positions by ordinal before resolving and writes each
resolved value back at its own row's index. Nothing else changes: same statement,
same memo, same checks, same value cache, same `values` vector order handed to
`rebuild_leaf`.

| Quantity | stride10 baseline | stride3 baseline |
|---|---:|---:|
| Pooled leaf resolutions | 11,268 | 41,958 |
| Leaf rows resolved | 677,234 | 2,792,755 |
| Distinct covering groups per leaf | 5.80 | 8.77 |
| Catalogue statements per leaf | 24.8 | 31.8 |
| Catalogue statements | 278,927 | 1,334,277 |
| Nanoseconds per statement | 7,817 | 7,827 |
| Catalogue interval | 2,180,465,764 | 10,443,893,588 |
| **Predicted statements after the treatment** | **65,337** | **368,074** |
| **Predicted catalogue interval** | **510,740,529** | **2,880,915,198** |

The prediction is arithmetic on the measured per-statement cost, not a measurement.

### Why the first shape was rejected

The first candidate (`candidate`) replaced the memo with one statement per leaf
over the leaf's whole ordinal span. It was measured: stride10 improved by
2,298,597,419 ns, but **stride3 regressed by 1,884,341,075 ns** because a span
query's cost grows with the *width* of the leaf's ordinal span, not with the number
of groups the leaf uses. Measured per span statement: 101,011 ns on stride10 and
259,906 ns on stride3, against 7,817 ns for a single-group statement. That
candidate is retained as a rejected treatment with its samples.

The ordinal-ordered memo has no such term: its statement count is the number of
distinct covering groups, which is a property of the leaf, not of its span.

### Bounds and equivalence

- **No format change, no new SQL, no new cache, no lifetime change.** Read-only;
  the same `group_for` statement is issued, only in a different order.
- **Bounded.** At most one statement per distinct covering group of one leaf, and
  one sort of at most `MAXIMUM_LEAF_ROWS` positions.
- **Same validation and same errors.** `group_for` still validates the ordinal
  range on every lookup and still reports `metadata ordinal missing` / `metadata
  ordinal range` for the same inputs. One declared difference: when a leaf holds
  more than one offending ordinal, the refusal may be raised at a different row
  than before, because the rows are visited in ordinal order.
- **Same results.** Every value is resolved for its own row and written to that
  row's index; the `values` slice handed to `rebuild_leaf` is unchanged, and the
  decoded-value cache and `decoded_work` charge are order-independent.
- **Measurement.** One stride10 and one stride3 sample of `candidate2`, matched
  against the unchanged `baseline2` samples with identical instrumentation.

## Why this and not the pre-registered range read

The pre-registered candidate for this priority was a selected-group BLOB read
instead of whole-pack copying. The attribution does not reject it, but it does not
select it either: 43.6% of the provider is pack acquisition, and the pack cache
already serves 67% of 436,068 demands without touching SQLite. A range route would
add a BLOB open and range read per extraction while giving up that reuse, so its
removable share is not separable from the overhead it adds without its own screen.
The catalogue interval, by contrast, is 2.180 seconds of *pure repeated statement
execution* that one statement per leaf removes, with no new cache, no lifetime
change, no retained bytes and no format effect. That is the smaller equivalent
treatment for this round.

Priority B's per-leaf pack-cache scope was also not selected: it is a real 2.5-3.9
second target, but it changes the lifetime of a cache that spans Store writes and
therefore needs its own invalidation contract and corruption tests. It stays a
proposal.

## Bounds and equivalence

- **No format change.** Read-only; the catalogue grammar, the pack grammar and the
  stored bytes are untouched. A saved Store and the canonical inventory must be
  identical between the arms.
- **Bounded.** One statement per leaf, whose result is bounded by the groups whose
  start lies in that leaf's own ordinal span; the span is bounded by the leaf's
  ordinal extent. Nothing is retained past the leaf.
- **Same validation.** Header and directory validation, ordinal-range checks,
  digest authentication of the group body, chain budgets, ceiling checks and
  canonical identity are unchanged and still run on every path.
- **Error behaviour.** The same two integrity errors are produced for the same
  inputs. One difference is declared: a catalogue row inside the leaf's span that
  no leaf row needs is now *read* but not *checked*, exactly as before — only the
  covering row of an actually-resolved ordinal is checked.
- **Measurement.** One stride10 sample per arm, baseline2 (instrument only) versus
  candidate (instrument + this treatment), identical instrumentation, fresh
  outputs, both under the shared global flocks. Confirm once on stride3 and run
  the identity-matched separate verification only if stride10 is worthwhile.
