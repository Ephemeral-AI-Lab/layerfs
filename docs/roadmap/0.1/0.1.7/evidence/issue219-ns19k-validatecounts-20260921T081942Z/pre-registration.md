# Pre-registration — #219 round 11: price `validate` with counts

Written **before** the first run of this arm and before any edit. **No product line changes.**

Control: **J1** (`benchmark-results/issue219/ns19-J1-buildsplit-20260921T081749Z`, `b09783b98`):
`span_build_ns` 308.59 ms, `validate` **221.70 ms** (71.8 % of the span, 13.9 % of the row),
`references` 9.32, `inodes` 6.02, `directories` 3.11, residual 68.38, `operation_work_ns` 1590.39 ms,
13/13 gates, 14/14 pins.

## The one difference

**The build's own work counters are published**, where the row publishes one of them
(`objects.objects_emitted`). Round 10 located 221.70 ms in a single phase with a single call site
(`layerfs-content/src/filesystem/validate.rs`, 824 lines, `update.rs:162`) and nothing in the tree
says *which* loop spends it. The counters that do are already maintained by the product and already
published by two other rows (`ops/fs.rs`, `ops/history.rs`) — this row accumulates them over its
three batches and publishes them:

`pipeline.validation_objects_read`, `_read_waves`, `_inode_demands`, `_inode_pages_read`,
`_directory_pages_read`, `_entries_examined`, and `pipeline.build_base_records_read`.

Nothing else moves: no product line, no bound, no format, no timer boundary, no pin.

## What each count would decide

| count | if it is large | if it is small |
| --- | --- | --- |
| `entries_examined` | the effective-tree cycle walk (`check_effective_cycles`, one fresh walk per directory binding, a fresh `seen` set and a fresh `visited` budget each) dominates, and its cost is entries examined | validation is not walking, and the phase is spent in the per-binding loop |
| `objects_read` / `inode_demands` | the phase is **authenticated reads** of base inode records, and its price is the product's own read path, not validation logic | the records are memoised and the phase is pure CPU |
| `directory_pages_read` | `effective_entries` re-lists base directories per walk | listings are amortised |
| `base_records_read` | the final counts re-read the base independently of validation | they do not |

The prior, stated so it can be wrong: 10,101 bindings across 3 batches, `MAXIMUM_CYCLE_CHECK_ENTRIES`
= 4,096 **per walk**, one walk per directory-kind binding, ~40 directory bindings per batch, ~100
entries per listed directory — so `entries_examined` of order **12,000–40,000** and `objects_read`
small (the batch's children and parents are prefetched into one grouped demand), which would put the
phase in pure CPU over the walks.

## What would refute it

1. `validation_objects_read` or `inode_demands` of order 10^4 or more: the phase is reads, the prior
   above is refuted, and the next round is about the read path rather than about a loop.
2. Any of the 14 pinned counters moves, `pipeline.commits` != 284, the root digest changes, or the
   row is not PASS 13/13.
3. `span_build_ns` outside 308.6 ± 100 ms, or `operation_work_ns` outside 1590.4 ± 250 ms: charging
   counters the product already maintains must be work-neutral.
4. The harness's own suite gains a failure beyond the three pre-existing `registry_negative` cases.

## What is not claimed

No fix, no movement, no prediction about which loop wins beyond the prior above. The output is a
priced cause, and the round after it is the treatment.
