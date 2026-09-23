# #237: Pack BLOB write comparison and feasibility

> **Status:** source and retained-evidence analysis on product `970854f2c`.
> No product change, build, or new timed 10k sample was made for this study.
> The 4,096-B DB page, 128-KiB whole-file cutoff, single C2 owner, and pack
> payload inside SQLite BLOBs remain fixed.

## Side-by-side mechanism

| Step | v0.1.6 release `44cf74848` | Core `970854f2c` |
| --- | --- | --- |
| New pack | Assemble a variable-length BLOB in memory; bind several packs per `INSERT`, capped at 1 MiB per SQL statement ([admission](../../../../crates/layerfs-layerstack-store/src/objects/admission.rs)). | Allocate one `zeroblob(capacity)` row per pack, then open its BLOB and write the new body, directory entries, and 24-B control area ([writer](../../../crates/layerfs-storage/src/sqlite/write.rs)). |
| Later groups in an open pack | Reassemble and `UPDATE` the whole pack BLOB ([session](../../../../crates/layerfs-layerstack-store/src/objects.rs)). | Query Save ownership, then write only the increment's body, new directory entries, and control area through incremental BLOB I/O. |
| Capacity | Row length equals assembled BLOB length at each write. | Nonsingleton rows reserve the lane's 256-KiB pack limit when created; `sqlite3_blob` cannot resize them in place ([layout](../../../crates/layerfs-storage/src/pack/layout.rs)). |
| Same-Save visibility | Objects and pack bytes publish within the admission transaction. | `place_groups` writes packs before their object locator rows; same-Save predecessor reads can demand queued groups before a wave ends ([placement](../../../crates/layerfs-storage/src/cas/placement.rs)). |

The [matched-source SQL trace](v016-core-sqlite-head2head.md) counted **590**
multirow v0.1.6 pack INSERT statements for 1,693 final packs and **408**
whole-BLOB UPDATEs. The older Core diagnostic identity counted **1,262**
one-row creates and **1,237** changed-span appends across three Saves. These
are operation counts, not per-call timing on the same product architecture.
The v0.1.6 whole-BLOB UPDATE rewrites more bytes; a smaller SQL statement
count alone does not prove a faster pack path.

## Current-format API-call ceiling

A separate [ImportBatch count diagnostic](evidence/slab-handoff/countdiag/service.stderr)
on the batch algorithm gives the closest exact selected-write population.
Parsing its three `save_outcome` lines gives:

| Save | Pack creations | Pack appends | `write_in_place` calls | BLOB `write_at` calls implied by source |
| --- | ---: | ---: | ---: | ---: |
| File | 1,256 | 1,126 | 2,382 | 7,146 |
| Prerequisite | 2 | 0 | 2 | 6 |
| Tree | 3 | 200 | 203 | 609 |
| **Total** | **1,261** | **1,326** | **2,587** | **7,761** |

Every selected write contains at least one group, so the body and directory
slices are nonempty. `write_in_place` writes them and the control area once,
then closes its BLOB handle. An append also makes one ownership `SELECT`.
These 7,761 BLOB writes and 1,326 ownership reads are **derived call counts**,
not measured wall time. The reorganized integrated ImportBatch run at
`bc944fe63` did not report exact pack calls; do not silently assign the
count-only diagnostic to its 1.110-s speed row.

The earlier Core head-to-head instrumented file Save measured **78.920 ms**
inside `write_pack`, including its SQL, BLOB I/O, and cache invalidation.
Even eliminating that entire nested region at that older identity would
cover at most 78.920 ms directly, versus the integrated ImportBatch row's
**359.706-ms raw gap** to the v0.1.6 750.626-ms observation. This is a
scope/priority bound, not a forecast: changes in pager or COMMIT behavior
could have indirect effects, and the two timing identities differ. Removing
one of three BLOB writes only on the 1,261 creates is a smaller treatment still.
On append, the new directory entries start after earlier entries, so the
control area and new entries are not contiguous. Coalescing them would rewrite
the intervening directory or require a read/copy; it is not a free one-call
change.

## Space and attempted alternative

The matched-source closed Stores held 301,646,854 B of variable-length
v0.1.6 pack BLOBs versus Core's 330,825,728 B of reserved capacity. Core
declared 305,969,540 B used and 24,856,188 B spare. The 29,178,874-B
capacity difference is 4,322,686 B more used under a different format plus
24,856,188 B spare. These are footprint facts, not measured write latency.

An exact-length assembled INSERT while keeping Core's current append grammar
would have to resize the row on a later append; SQLite's incremental BLOB API
cannot do that. Deferring the INSERT until pack closure would also defer
object-locator visibility and force an in-memory path for same-Save reads and
dependency barriers. Whole-BLOB UPDATE restores visibility but reintroduces
the reference's repeated rewrite. A segmented logical pack held in multiple
SQLite BLOB rows is a format and read-path redesign; it adds indexed rows and
has no source-backed 10k speed case yet. None is a narrow pack-write treatment.

The earlier [exact-pack-fit experiment](exact-pack-fit-experiment.md) changed
placement timing, not BLOB capacity: appends fell **1,054 → 21** and the pack
write region by **9.085 ms**, while apparent Store size rose **528,384 B**,
spare rose **516,064 B**, and sampled RSS rose **16.9 MB**. Its control had
concurrent workload and incomplete telemetry, so its raw public times are
not a causal speed pair. Repeating that treatment would not resolve the
remaining gap.

**Decision:** no pack-only candidate or 10k sample from this worktree. The
observed pack-write region is too small to justify another full public pair
for a minor API-call reduction, and current-format exact-length assembly
conflicts with incremental append and immediate visibility. Reconsider a pack
format change only after a diagnostic shows pack I/O or its pager effects
consume a material share of the *integrated* ImportBatch run. The next #237
experiment should target the larger measured C2/transaction or import phases.
