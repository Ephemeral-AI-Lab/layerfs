# P1-8 receipt — one ordered cursor for an assembly's retained runs

> **Status:** Landed. Commit `e9b4d1510` (tree of this round's `after/` arm);
> before arm [`../p1-6/after/`](../p1-6/after/) — the same commit `360431d10` the
> continuation handoff starts from
> (`phase1-continuation-handoff-20260918.md` §1). Client, vehicles and example
> identities for this arm: [`after/artifacts.txt`](after/artifacts.txt).
> Elapsed figures below are **diagnostic only**; no box is gated on one.

## 1. The item, as planned

`assemble_inner` (`file/edit/apply.rs`) served every `Segment::Retain` through
`view.read_range` → `mapping::read_range` → `traverse`, and `traverse` descends
from the mapping root every time it is called. A whole-file result reached
through R retained runs therefore paid R descents, the shared root included.

Target: retain segments `O(R·h)` → `O(h + shared)` provider demands. The plan's
sketch put an ordered cursor in `assemble_inner`, built once per assembly, with a
frontier retained *across* segments.

## 2. What landed, and the one deviation from the sketch

`RangeCursor` (`file/mapping/read.rs`) serves a sequence of ascending ranges of one
chunked file through the same bounded traversal `read_range` uses. Instead of a
cross-segment frontier, the cursor keeps the **mapping pages** it has already
acquired (`PageCache`, keyed `(id, is_root)`), so a page two ranges share is one
provider demand and one `nodes_read` charge for the whole assembly. The cache is
capped at `READ_NAVIGATION_CACHE_PAGES` = 2 × `READ_NAVIGATION_WAVE` = 64 pages and
emptied wholesale when it would exceed that; a whole-file base never builds a
cursor (`assemble_inner` keeps the slice path for `file_state() == None`).

**Why not the sketch's retained frontier.** The sketch's own trap note names it:
`traverse`'s early `finished` break drops entries after the active range's end, and
the fix — keep not-yet-visited siblings across the segment gap — needs the
frontier to carry *partially consumed leaves* between segments. That was built
first and measured here: the page accounting for a carried leaf has to move from
the traversal to the caller, and every arrangement tried left either a re-read page
or a mis-accounted byte count on the boundary (the same page demanded twice across
a segment edge, or a slice served twice). The page cache reaches the same Big-O
target with the traversal untouched: R ranges still each run the bounded,
already-proven traversal, and only the *provider demand* is shared. That is the
smaller and the safer of the two mechanisms, and it is the one the receipts below
measure.

**The plan's second test target is refuted.** `straddling_payload_demanded_once_across_segments`
cannot hold for this design, and should not: each retained run is served **exactly
and independently** (that is what makes `payload_bytes_read == requested` a
per-range check), so a chunk payload that straddles two runs is demanded once per
run — as it was before. Sharing payloads across runs would mean holding a payload
across a segment boundary and re-slicing it, which is a different mechanism with a
different bound, not this item. Only the page half of the plan's claim
(`O(R·h)` → `O(h + shared)`) is delivered. The test bearing that name is kept as
the **parity guard** on the same shape (it asserts every payload is demanded at
most once *and* the emitted bytes are the reference bytes); it passes on both
trees, and §5 says so rather than presenting it as evidence of movement.

## 3. The measurement

### 3.1 The discriminating row: `M4` (new, additive)

`edit_timing_c1 --case split` was added with this commit: three 40,000-byte kept
runs of a 3,000,000-byte chunked base (a two-level mapping tree), result 120,000
bytes — one whole-file object, reached through three `Segment::Retain`s. No frozen
row reaches that route with a chunked base (`edits.c1.small` assembles over a
whole-file base), which is the gap the continuation handoff already recorded.

| Row | Counter | Before (`360431d10`) | After (`e9b4d1510`) | Predicted |
| --- | --- | ---: | ---: | --- |
| M4 | `nodes_read` | **16** | **13** | lower by ≈ (R−1)·h ✔ |
| M4 | `edit_nodes_read` | 0 | 0 | unchanged ✔ |
| M4 | `edited_root` | `4a4caa46…` | `4a4caa46…` | identical ✔ |
| M4 | `objects_written` / `_bytes` | 1 / 120023 | 1 / 120023 | identical ✔ |
| M4 | `mapping_pages` | 3 | 3 | unchanged ✔ |

The three saved demands are the root page and the two mapping leaves the runs do
not share: today each retained run re-demands the root (3 demands) and the leaves
its own range reaches. The before arm is the same example source compiled against
the parent tree; both samples and both binaries' sha256 are stored under
[`parent-m4/`](parent-m4/) (parent binary `bfc7332d…`, after binary `124462ab…`,
which is the hash `after/artifacts.txt` records).

### 3.2 The frozen set does not move — the whole counter-only diff

`python3 /tmp/counters.py rounds/p1-8/after` vs `…/p1-6/after` (every counter line,
after stripping `elapsed_ns` and the timing-tree lines) differs **only** by the new
`M4` row. Every D-row and every other M-row is byte-identical, including:

| Row | Counter | Before | After |
| --- | --- | ---: | ---: |
| D27 | `nodes_read` / `edit_nodes_read` / `edited_root` | 9 / 10 / `b6dca354…` | 9 / 10 / `b6dca354…` |
| M2 (`--case delete`) | `nodes_read` / `edit_nodes_read` | 4 / 7 | 4 / 7 |
| M3 (`--case shrink`) | `nodes_read` | 11 | 11 |
| D25 (`order.default`) | all counters | 0 spills | 0 spills |
| D26 (`order.forced64`) | `rows_read` / `rows_written` / `merges` / `runs_created` | 25809 / 25760 / 61 / 124 | 25809 / 25760 / 61 / 124 |
| D26 | `dir_pages_read` / `ino_pages_read` / `ino_scratch` | 17 / 81 / 709396 | 17 / 81 / 709396 |
| D2 | `validation: objects 21 waves 2 inode_demands 21 inode_pages 2` | — | identical |

**M3 is the handoff's stated anchor and it does not move: `nodes_read` 11 → 11.**
That is a refutation of the handoff's §2.1 receipt line, not a miss. The `shrink`
shape is a **single** retained run (`delete [131000, 3300000)` leaves the result
below the cutoff, so `Plan` yields exactly one `Retain`), and with R = 1 there is
nothing to share: `(R−1)·h = 0`, exactly the formula the handoff itself gives. The
before measurement in §5 of the P1-8 probe confirms the demand census — 1 file
state + 2 mapping pages + 7 payloads, no page demanded twice, so no cursor can
remove a demand there. M3 is reported as the **negative control** it is.

## 4. Parity

* The 34-test sealed-oracle set is green and unchanged; `git diff 360431d10..e9b4d1510 -- '*tests*'` touches exactly one file, `edit_transitions.rs`, and only adds two tests and their helpers — no pinned value is edited.
* Every emitted root on the frozen set is unchanged, including `edits.c1.small`'s `65,559` canonical bytes and `edits.pipeline.small`'s byte-for-byte readback (D10–D24 identical in the counter diff).
* `edit_noop`'s verdict and its zero-emission pin are untouched (the change is downstream of the no-op check: `assemble_inner` runs only in the `WholeFile` arm).

## 5. The falsification that mattered

`retained_segments_share_one_descent` (`edit_transitions.rs`) applies the same
three-run shape in-test through `apply_edits` with the `Census` provider and
asserts the two-level mapping root is demanded **once** for the whole assembly.
On the parent tree it fails: `left: 3, right: 1` — three descents, one per retained
run. On this commit it passes. That is the reproducible form of the same finding
M4 measures, and it is the version that holds on a tree nobody has to trust.

## 6. Honest gaps

1. **Payload demands are unchanged** (§2). A chunk straddling two retained runs is
   still demanded once per run. Recorded as a refutation of the plan's target.
2. **`README.md`/`CONTRACT.md` unchanged:** the driver gained one vehicle row (`M4`,
   `collect.py`) and the addition is recorded as correction 4 in `ROUND-README.md`.
   The frozen vehicles' names, cases, parameters and printed fields are unchanged;
   `--case split` is a new case whose `case:` line is additive.
3. **The cache is a declared ceiling, not a measured peak.** 64 pages ×
   `MAX_NODE_OBJECT_BYTES` (8,192) is the worst case the constant allows; no frozen
   row reaches the eviction path (the fixture's tree is 3 pages), so eviction is
   review-verified only — stated rather than claimed.
4. **`M4` is a vehicle row, not a frozen-set row** (like V2's M1–M3): it is
   collected by `collect.py v2` and included in `all`.
