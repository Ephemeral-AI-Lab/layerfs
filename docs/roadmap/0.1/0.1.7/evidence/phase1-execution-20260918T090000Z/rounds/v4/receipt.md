# V4 receipt — the edit's own node-load count in the edit vehicle

> **Status:** Vehicle receipt (prerequisite V4, added during Phase 1 execution and
> recorded on #178 as a plan refinement). No product or test source is touched: an
> **example** gains one printed line. Written once, from [`after/`](after/)
> (commit `f5e53f312`) under [`../../CONTRACT.md`](../../CONTRACT.md). V4 has no
> #178 checkbox; its deliverable is the observable P1-9 moves.

## 1. The gap it closes

`edit_timing_c1` printed `nodes_read`, which is the **provider-demand** count
(`demanded.len()`), not the operation's own node-load count. A node this operation
built is charged to `EditCounters::nodes_read` and never demanded, so work served
from the operation's own drafts — a split, or the rightmost walk P1-9 gates — is
invisible to the printed figure. Measured: P1-9's gate changes `EditCounters`
for the `delete` fixture and moves the provider count **not at all**.

## 2. The change and the rows

One additive `println!`, `edit_nodes_read: <n>`, from the `ConstructedFile`'s
counters. Every previously printed field is unchanged.

| Row | `nodes_read` (provider demands) | `edit_nodes_read` (this vehicle's new field) |
| --- | ---: | ---: |
| D27 `edit_timing_c1` (40,000-byte overwrite) | 9 | **10** |
| M2 `--case delete` | 4 | **8** |
| M3 `--case shrink` | 11 | **0** |

M3's zero is the whole-file route's documented `EditCounters::default()`
(`edit_transitions.rs` pins it on that route), not a missing measurement. The
three rows are the same shapes V2 baselined; the full frozen set was collected in
[`after/`](after/) and no other counter moved.

## 3. Checks

The eight checks all exit 0 on this tree (boundary guard 0 with 116 files;
core/tools 6 OK; production_loc 17 OK; fmt 0; core workspace 445 passed /
0 failed; clippy `-D warnings` 0; `production_loc.py --files` 0; `git diff
--check` 0). Production LOC delta **0** (`examples/` is outside the production
scope). One stray blank line at the end of `edit_localized.rs` was removed by
amending this commit before it landed, so the commit touches exactly one file.
