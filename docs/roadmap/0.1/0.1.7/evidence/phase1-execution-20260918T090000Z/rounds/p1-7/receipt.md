# P1-7 receipt — one shared page memo between the two edit passes

> **Status:** Landed. Commit `974b525be` (tree of this round's `after/` arm);
> before arm [`../p1-8/after/`](../p1-8/after/) — `e9b4d1510`, the commit P1-7
> builds on. Artifact identities: [`after/artifacts.txt`](after/artifacts.txt).
> Elapsed figures are **diagnostic only**; no box is gated on one.

## 1. The item, as planned (in its corrected form)

The plan's §P1-7 sketch was **rejected on 2026-09-17**: fusing comparison into the
split descent would publish finalized nodes before a later segment's Equal verdict
is known, breaking the zero-emission contract pinned at `edit_noop.rs:82`. The
correction keeps comparison as its own pass — verdict location unchanged, no
dispatch reorder — and makes it one ordered cursor whose page demands are
**memo-served to the construction pass**.

Target: provider demands `O((⌈L/64K⌉+R)·h)` → `O(h + distinct union-path pages)`;
D27 `nodes_read` **9 → ≈7**.

## 2. What landed

One `PageCache` per `apply_edits`, handed to both passes:

* **The comparison** navigates a chunked base through a `RangeCursor` over that
  memo. A replacement of many 64 KiB windows shares one descent instead of
  descending from the root per window. A whole-file base has no mapping: it keeps
  the slice path and leaves the memo untouched.
* **The construction pass** (`EditObjects::load_node`) consults the memo *before*
  the `nodes_read` charge and before the reader demand — a hit is neither a read
  nor a charge, which is what keeps the counter's doc honest. A hit still decodes
  under the caller's root context: only canonical bytes are shared, never decoded
  nodes, because the two contexts validate different partition rules.
* **The bound** is `READ_NAVIGATION_CACHE_PAGES` (64 pages), enforced by
  `PageCache::make_room_for` **before** a batch is inserted. Cleared wholesale.

**A bug the ceiling case caught, recorded rather than hidden.** The first version
enforced the bound inside `PageCache::insert`. In the 4,096-edit ceiling case
(`edit_bounds::the_edit_stream_ceiling_is_enforced_before_any_work`) a batch insert
cleared a page it had just cached, and the result's readback raised
`MissingObject`. The 4,096-edit run is the only test in the suite that fills the
cache, so nothing smaller exposed it. The fix moves eviction to the batch
boundary; the test is green and no assertion was changed for it.

## 3. The measurement

### 3.1 D27 — the header's own row

| Row | Counter | Before (`e9b4d1510`) | After (`974b525be`) | Predicted |
| --- | --- | ---: | ---: | --- |
| D27 | `nodes_read` | **9** | **7** | ≈7 ✔ |
| D27 | `edit_nodes_read` | **10** | **8** | lower ✔ |
| D27 | `edited_root` | `b6dca354…` | `b6dca354…` | identical ✔ |
| D27 | `objects_written` / `_bytes` | 7 / 47357 | 7 / 47357 | identical ✔ |

The trace: the mapping root and one leaf are demanded once by the comparison and
again by the split descent. The memo turns the second pair into hits — 9 → 7, and
the operation's own node-load count 10 → 8.

### 3.2 The other vehicles — no movement, and why

| Row | Counter | Before | After | Why unchanged |
| --- | --- | ---: | ---: | --- |
| M2 `--case delete` | `nodes_read` / `edit_nodes_read` | 4 / 7 | 4 / 7 | A length-changing edit is a difference **by construction**: `compare_replacements` returns `Differs` before it reads a base byte, so there is nothing to memo. |
| M3 `--case shrink` | `nodes_read` | 11 | 11 | Same: the shrunk result is a length change. |
| M4 `--case split` | `nodes_read` | 13 | 13 | Same. |

### 3.3 The whole counter-only diff

`python3 /tmp/counters.py rounds/p1-7/after` vs `…/p1-8/after` (every counter line,
after stripping `elapsed_ns` and the timing tree) differs **only** by D27's two
counter lines. D25/D26, D1–D24, D28/D29, M1–M4 and X1/X2 are byte-identical,
including D26's `rows_read 25809` / `rows_written 25760` / `merges 61` and D2's
`validation: objects 21 waves 2 inode_demands 21 inode_pages 2`.

## 4. What must not have changed — and did not

* **`edit.compare` survives as its own scope.** The pass is still there, before the
  representation match; `edit_timing.rs`'s scope pins are green unmodified.
* **`edit_noop`'s verdict and its zero-emission pin** (`:82`) are green: the memo
  answers demands, it does not decide the verdict.
* **Every emitted root** is unchanged, D27's included, and the sealed-oracle set is
  green and unchanged.

## 5. The pinned tests that moved

Three test pins measure exactly the count this item lowers, so each was
**tightened, not relaxed**: the new exact value is asserted *and* the bound the case
protects is kept as an upper bound, so any future rise is still a failure.

| Test | Pin | Before | After | Bound kept |
| --- | --- | ---: | ---: | ---: |
| `edit_reference::an_interior_join_reads_its_boundary_child_once` | `nodes_read` | 22 | 20 | ≤ 22 |
| ↳ negative controls | `unequal-height-join` / `height-growth` / `root-collapse` / `join-80-100` | 4 / 4 / 4 / 9 | 3 / 3 / 4 / 9 | ≤ 4 / 4 / 4 / 9 |
| `edit_localized::a_pure_deletion_never_walks_the_rightmost_path` | overwrite `nodes_read` / provider demands | 7 / 6 | 6 / 5 | ≤ 7 / ≤ 6 |

This is the pattern the handoff names for such a case ("generalize a
cross-subsystem assertion … that is the pattern to follow"): the invariant each
test exists for is preserved and stated, the count it happens to observe is
updated with its provenance, and the old value stays as a bound. No assertion was
deleted or weakened.

## 6. Parity / falsification

`a_page_two_passes_reach_is_demanded_once_for_the_operation`
(`edit_localized.rs`) applies an extent-aligned overwrite to a 262,144-byte chunked
base through a recording provider and asserts the demand census is at most one per
object identity. On the P1-8 tree it fails — one mapping page demanded **2** times —
and on this commit every object is demanded once. The failing run is stored at
[`parent-test/log.txt`](parent-test/log.txt).

## 7. Honest gaps

1. **Error ordering can move, and no test pins it.** A page the memo serves would
   otherwise have been re-read: an error that the re-read would have raised (a
   corrupt stored page, a provider refusal) is now raised at the demand the memo
   answers instead. The verdict, the emission behaviour and every recorded byte are
   unchanged; the *point of failure* on a corrupt base is not pinned by any test in
   the suite today. Stated here rather than claimed safe.
2. **The memo is per operation and cross-pass only.** It lives one `apply_edits`
   and is never shared between operations; a cursor's own frontier already prevents
   intra-pass re-demands.
3. **The 64-page bound is a declared ceiling, not a measured peak.** D27's tree is
   3 pages; the ceiling test is what actually fills the cache (>64 pages over 4,096
   edits), and it is where the eviction path is exercised.
