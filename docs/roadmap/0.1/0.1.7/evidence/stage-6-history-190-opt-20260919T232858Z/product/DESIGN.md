# Directory-parent acquisition and reuse

Status: implemented after root confirmed baseline binary and both selections frozen; source at HEAD `9f35c49ad62956f131dc2676787f99d69659686e`. No measurements in this document.

The operation already validates directory ordering before mutation. Its directory loop calls `lookup_base` once for each existing live directory, and its later value overlay calls it again when the caller omitted that directory's typed inode value. Every singleton invokes `lookup_many` afresh. That helper can group records across shared ancestors, but these calls cannot share traversal.

Use existing `base_read_batch` for bounded chunks of input directories, capped by existing `MAXIMUM_READ_DEMANDS`. Exclude unreachable and declared-new parents and the absent-base build path. Input validation already enforces ascending unique parent serials, so neither sorting nor deduplication is required here. One existing `lookup_many` answers the remaining parents for each chunk.

Carry each returned immutable-base `InodeValue` through that directory's merge and final value overlay. Prefer the caller's typed metadata when present; otherwise preserve the carried base value. Pass the overlaid value into the existing reducer immediately, then discard the batch. The existing contents map is removed; the existing input.update_for binary search excludes rebuilt directories from later supplied-value processing. No global memo or expanded validation memo, no new dependencies, public API, format, worker, or policy ceiling.

The reducer's `note_value` replaces only its typed value; retained/removal binding counts are independent and are derived after all observations. Early value insertion preserves semantics but can change pending-row/spill order. Forced-small-pending tests must therefore cover correctness, quotas and cleanup. Batching also changes the physical acquisition/error order when multiple demanded objects fail; each failure still aborts once, without publishing a filesystem root or retrying. No validation checks are removed.

External verification should compare exact canonical roots, final directory metadata, provider page/wave counts, omitted parent values versus explicitly supplied values, batch size 1 versus larger batches, initial/new/empty/unreachable cases, corruption, and forced spill. History comparisons must separate any moved final-value bookkeeping (now within `directories`) from complete build/update totals.


## Revision after quota differential

The early-value-insertion candidate was rejected before history measurement:
it changed successful quota cells into resource refusals. Its evidence and patch
remain archived by the coordinator. The replacement retains the original
`contents` map and exact reducer insertion order: all directory effects first,
then ascending directory values, then other supplied values.

Both parent acquisition and missing final-metadata acquisition are now batched.
Only the final nonempty parent batch is retained for reuse in the second pass.
At most two lookup windows coexist, each capped by the existing base-read batch
and object-wave ceiling. Earlier omitted metadata may be read again, but in
shared-ancestor batches. No whole-input InodeValue memo is created. This narrower
reuse trades some repeated work for unchanged reference-ordering resource
behavior; the quota matrix must establish that behavior on the selected cases.

## Frozen v2 structural result

Coordinator-reported focused suite: 4 tests PASS. The exact 56-cell quota output
matches baseline byte-for-byte, including successful canonical roots and refused
error values. These outcomes establish the selected fixture/budget matrix, not a
proof for all inputs.

External provider demands in the five-directory one-leaf fixture:

| base_read_batch | baseline omitted | v2 omitted | baseline explicit | v2 explicit |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 23 | 22 | 18 | 18 |
| 3 | 17 | 10 | 12 | 9 |
| 64 | 15 | 6 | 10 | 6 |

The full-window case removes nine of fifteen omitted-metadata acquisitions while
retaining the same canonical root. Batching and reuse are one cohesive change:
there is no separately measured production arm attributing the total latency
saving to one or the other. At batch 1 only the final record is reused, so omitted
metadata still incurs earlier rereads. This is deliberately stated rather than
claiming universal operation-wide memoization. Baseline/candidate history results,
full workspace checks and exact production LOC are owned by the coordinator.

Product implementation and architecture were frozen after this result. The
implementation subagent ran no builds, tests or history commands; it ran only
source reads, owned-file edits and rustfmt on `update.rs`.
