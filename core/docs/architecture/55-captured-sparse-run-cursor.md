# Captured sparse run cursor

> Status: source implementation after local checkpoint `dcdf52758`;
> five Overlay and three Daemon public cases pass with affected host/Linux checks
> in the [component checkpoint](../issues/307/SPARSE-SERIAL-PROGRESS-20261007.md). This supplies retained local input
> discovery, not captured edit normalization, Save/history/install or S10.

R7 update, 2026-10-09 (schema 23): a metadata row may be a dense row of
several cells (at most `RUN_BYTES`, inside one aligned slot). A probe starts
at the slot of its position, its seek passes over rows that end before the
cursor's cell, the next seek starts at the row's end, and a window ends with
the widest row that holds its first byte, so its length and mask are bounded
by `RUN_BYTES` and `RUN_BYTES/8` instead of one cell.

`CapturedRunCursor` retains the exact engine-minted CapturedReader, serial,
checked caller-supplied logical size, current position and fixed per-layer
metadata lookahead. It acquires/releases no reader or operation. When a local
inode row exists its size must match and its kind must be a live regular file.
When none exists the caller still owes authenticated immutable kind/length facts;
an absent local cell or inode does not establish immutable zero or root authority.
SQLite's existing signed offset format remains; no smaller total file/run cap
is introduced.

The same port supplies `captured_inode(reader, serial)`: one exact local point
within the retained generation/floor. It does not use the current orphan-domain
override from ordinary live inode lookup. None still delegates to authenticated
immutable facts; it establishes neither base absence nor zero. The reader's
`created_above(born)` compares only that exact row's creation generation with its
retained floor. Known install cannot change this classification.

Each `captured_run_step` checks original reader/root/generation/installed floor,
then reads the existing at-most-four retained inode layers. Layer identities
must remain identical across continuation. A relevant layer contributes at most
one physical metadata row, selected by the exact namespace/serial/generation and
forward cell offset, with `ORDER BY cell_offset LIMIT 1`. The query projects
offset, epoch and data/mask lengths, not their BLOB bodies. Shape checks and the
existing shrink staircase decide whether that row is still effective. Stale
rows pay their actual binary-search/SQL work and yield bounded continuation.

The cursor advances its exclusive seek before considering a stale row. Ready
future cells and completed prefixes survive intervening windows/gaps; advancing
the logical position only raises a seek floor. It never rewinds a lower-layer
stale prefix for each newer window. `Continue` is normal progress of the original
stable input, not a retry after a failed operation. Original errors end the
caller operation and retain custody; constructing another pass does not repair
that failed operation.

`seek_forward` exposes the same monotone advancement to an owning run source.
It rejects backwards/out-of-file positions and preserves lookahead while
skipping unrequested retained intervals. A new planned pass may create a new
cursor; an original failed operation must not reset and submit again.

Missing physical cells remain `Inherited` until actual captured EOF/cutoff
establishes `Zero`. The existing composition precedence governs: an upper cutoff
ends lower fallthrough, and a lower layer's EOF/cutoff establishes local zero.
Relevant later cells limit the gap. A Gap owns only offset/length/next cursor,
so a large proven local hole needs no byte window. It supplies no on-demand
immutable zero proof; the separate Content comparison owns that evidence.

If local cell data can overlap the current point, `Window` invokes the existing
`compose_layers` within one 4,096-byte cell. Its bytes, inherited mask, shrink
epochs and lower-cutoff semantics remain the old engine path. Returned windows
carry the CapturedReader's original base root after known install or logical
close. A mask hole is still inherited; actual canonical reads happen in Workspace
outside the SQL owner. End means the checked logical size was consumed, not
reader/Save/Workspace/process teardown.

The new Workspace `OverlayCapturedRuns` port is implemented by Direct Overlay
and Daemon OwnerClient. One typed captured-run command uses the existing fair
Read class and exact route check. Its input cursor and reply are boxed so this
capability does not inflate every unrelated Command/Response. Admission charges
that actual fixed cursor allocation, bounded reply and one cell/mask window.
Success clones one independently returned bounded window while its original
Completion is held; this is not zero-copy. An unattempted command/cause or
attempted error retains its original credited object. The owning normalization
adapter still owes first-cause terminal behavior and explicit reader/operation
release after real construction/Save/transport fences.

`CapturedRunWork` counts actual metadata rows and stale rows for one successful
unit. Existing DatabaseWork observes its SQL attempts/rows/VM/bindings/delivery;
PayloadWork observes the old composed-window copy/zeroing. These are distinct
domains. No returned payload BLOB does not mean no SQLite pager/physical I/O,
root metadata, parser/allocation, queue/service debt or process residency.
The exact metadata EQP and VM are exposed by `explain_captured_run`; those
diagnostics are prepared separately from scoped runtime counters.

External public cases cover a cutoff gap above 4 GiB, actual root metadata work,
mask holes, shrink/regrow, retained reads after install/close, explicit release,
missing local rows, size refusal, stale-prefix continuation, interleaved stale/
fresh cells and a start beyond the trimmed physical prefix. Seek totals are
fixture row counts rather than an eligible whole-system bound. Real Daemon jobs
exercise bounded returned windows, original Completion/unattempted custody and
healthy independent scope progress.

Normalizing captured final-state edits and per-directory typed input remains
required. A local byte-window loop is not that normalization or a sparse claim.
Stable base facts, coalesced changed intervals, indexed fixed edit records,
replayable edit/run adapters and their original failure custody are subsequent
work. Dense stale physical rows still pay every necessary discovery turn; this
cursor claims no constant-cost hole, universal logarithmic algorithm or qualified
performance/aggregate resident bound.

`CapturedReader::owner_id` and `installed_floor` expose exact retained binding
facts for the owning normalized-input context. The SQL source/retained-reader
rows enforce a nonnegative floor. Reading either identifier acquires no lease,
authorizes no independent job and creates no serializable restart capability.
The opaque reader token and exact provider checks still govern every read and
explicit release; the numeric values alone cannot reconstruct that custody.

The subsequent [captured-file adapter](56-captured-file-normalization.md) uses
`metadata_advanced_from` to validate a `Continue` against its submitted opaque
cursor. Reader, serial, logical position/size and stable layer identities must
match; seeks cannot retreat, completed probes and retained lookahead cannot
change, and at least one metadata seek must advance. A claimed row count alone
does not establish progress. The adapter also checks actual returned window and
mask capacities before retaining either allocation. Malformed continuation or
allocation ends the operation with its original custody; no iteration cap or
failed-operation restart substitutes for these checks.
