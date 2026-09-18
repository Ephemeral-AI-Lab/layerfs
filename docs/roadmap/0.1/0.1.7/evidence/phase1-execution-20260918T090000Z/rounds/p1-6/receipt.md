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
