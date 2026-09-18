# P1-6 receipt — **measured-and-declined** (the discarded validation load)

> **Status:** Disposition receipt. No commit, no box: this records a measurement
> that refutes the item's premise and a check the plan's own risk note flagged.
> Taken on the tree of `7b48c686a` (the P1-4 round commit; the product tree is
> `bfb01f262`), with the frozen set's arm
> [`../p1-4/after/`](../p1-4/after/) as the before state.

## 1. The item, as planned

`file/edit/tree.rs` loads `last` (taller-left case) and `first` (taller-right
case) and discards the result; `concat_inner` immediately loads the same node
again. The plan's target: "removes one redundant `load_node` (decode + demand +
summary re-check) per height-mismatched join side; `nodes_read` −1 per fired
branch, **0–2 per edit**".

## 2. The measurement

Both discarded loads were deleted in the working tree (the two-line change the
plan specifies) and the three edit vehicles were re-run on the same tree, one
sample each:

| Row | vehicle | before | after the deletion | predicted |
| --- | --- | ---: | ---: | --- |
| D27 | `edit_timing_c1` (no argument) | `nodes_read` **9** | **9** | 0–2 ✔ (0) |
| M2 | `edit_timing_c1 --case delete` | `nodes_read` **4** | **4** | 0–2 ✔ (0) |
| M3 | `edit_timing_c1 --case shrink` | `nodes_read` **11** | **11** | 0–2 ✔ (0) |
| D27/M2/M3 | `edited_root` | `b6dca354…` / `7d3eb265…` / `4a45d246…` | identical | identical |

**The branch never fires on any available row.** The movement is 0 everywhere —
inside the plan's predicted range, but with no discriminating evidence: no frozen
row, no V2 row and no in-tree counter distinguishes the change from its absence.
The change was then **reverted**; the tree is unmodified.

## 3. Why declining, not landing

Deleting the load does not merely remove a duplicate read — it removes the only
**non-root context check** on that node:

* `EditObjects::load_node(summary, root)` (`tree.rs:153-170`) calls
  `decode_node_with_context(canonical, root)` and then compares the decoded node
  against its summary.
* `ExtentNode::validate(root)` (`file/mapping/types.rs:171-176`) adds exactly one
  thing when `root == false`: `count < MIN_ENTRIES` →
  `NonCanonicalPagePartition`. Everything else — entry arithmetic, ordering,
  adjacency, level bounds — is checked under both contexts.
* The discarded call is `load_node(last, false)` / `load_node(first, false)`; the
  re-load inside `concat_inner` uses `root = true`, which does **not** enforce the
  non-root partition.

So the deletion trades a validation for a counter movement that no row exhibits:
a base whose dismantled child page is underfilled would be refused today and
absorbed silently afterwards (its children are re-hosted by `root_from_children`,
so the output stays canonical). The plan's own risk note says exactly this —
"the discarded load validated with non-root decode context — corrupt-tree error
identity could shift (no test pins it today; **the new test must**)" — and no such
test exists: the suite has no fixture for a hand-built non-canonical mapping, and
the item's guaranteed shape (`edit_reference`'s unequal-height-join, height-growth,
root-collapse) is an *oracle* case that pins emitted bytes, not a read count or a
refusal.

## 4. What would make it landable

1. **The safe variant**: validate the node once under the *stricter* (non-root)
   context and pass the decoded node into `concat_inner` instead of its summary, so
   no check is lost and the second read disappears. That is a refactor of a
   recursive function in `tree.rs` (895/999 physical lines — the tightest file in
   the tree), and its own receipt would still show `nodes_read` unchanged on every
   available row; its evidence would be the oracle cases staying byte-identical.
2. **Or an owner waiver**: accept the lost non-root partition check on this path,
   in writing, with the finding above as the record.

Both are owner decisions, so this receipt asks for one rather than choosing.

## 5. What is *not* claimed

* No counter moved: this is a decline **because** the movement is zero and the
  only certain effect is a lost check — not because the item is hard.
* The measurement is one sample per row on one tree; the vehicles are
  deterministic (X1/X2 bit-identical on every work counter), so a re-run gives the
  same figures.
* The `nodes_read` figures here are the same ones `../v2/receipt.md` §3 and
  `../p1-4/after/` record for the same rows.

## 6. Correction (appended 2026-09-18): the item **landed**, as the safe variant

Nothing above is edited. The decline was wrong in its evidence, not in its
reasoning about the check, and this section records both the error and the landing.

### 6.1 What the decline got wrong

§2 measured three shapes — D27, M2, M3 — and concluded "the branch never fires".
Those three all join **equal heights**, so they never dismantle a taller side and
never load a boundary child. The plan named the guaranteed shape itself:
"`edit_reference`'s `unequal-height-join`, `height-growth`, `root-collapse` oracle
cases". Measured on the oracle fixtures (and with V4's `EditCounters.nodes_read`,
which is the counter that can see this work — the printed `nodes_read` is the
provider-demand count and cannot):

| oracle case | with the discarded load | with it removed |
| --- | ---: | ---: |
| `interior-multi-level` (400 extents, 40,000-byte mid-file replacement) | **24** | **22** |
| `unequal-height-join` / `height-growth` / `root-collapse` | 4 / 4 / 4 | 4 / 4 / 4 |
| `join-80-100` | 9 | 9 |

So the branch fires — twice, on the one shape that reaches an interior join — and
the item is measurable after all. Two mistakes compounded: the wrong shapes *and*,
at the time of the first measurement, the wrong counter (V4 did not exist yet).

### 6.2 What the decline got right, and how it was resolved

The discarded load is the only **non-root** context check on that node
(`validate(false)`'s `count < MIN_ENTRIES`; the join re-decodes with `root = true`,
which does not enforce it). Deleting it would have traded a validation for the
saving. The landing therefore keeps the check and removes only the duplicate read:
`JoinSide` carries a side's summary **plus the node when the caller already has
it**, so the join reuses the node the boundary check just decoded instead of
reading and decoding it again.

### 6.3 The landed arm

Commit `360431d10`, one product file (`file/edit/tree.rs` 691 → 736) and one test
(`edit_reference::an_interior_join_reads_its_boundary_child_once`, which pins the
interior case at 22 loads and its root `57e0a51c…`, with the four equal-height
cases as negative controls; it fails on the parent tree at 24). The frozen set is
unchanged — a counter diff of all 29 D-rows and the three M-rows against the
`p1-10` arm is **empty**, and D27/M2/M3 keep `nodes_read` 9/4/11 and
`edit_nodes_read` 10/7/0, exactly as §2 recorded. The sealed-oracle parity set is
green: `the_candidate_reproduces_the_reference_root_and_partition` covers every
case with this change in the join.

Production LOC: **+45** against the plan's −4..−8, which assumed the plain
deletion; the difference is the `JoinSide` type and the call-site rewrites that
keep the check.

The dispositions §4 asked for are therefore moot: (a) was chosen and implemented.
