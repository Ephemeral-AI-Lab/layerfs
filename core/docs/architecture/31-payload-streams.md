# Payload layers and canonical zero runs

> **Status:** Current general guide. Implemented S5 work after `f5558fc22`;
> release, native FUSE and integrated Commit qualification remain separate.

R7 update, 2026-10-09 (schema 26): a payload row may hold several whole
cells, up to 32 KiB inside one aligned slot. Whole cells of a write become
or overwrite such rows, a shrink cuts the row that holds its boundary, and
the statement counts and page figures below describe one cell per row. The
current rules and numbers are in
[the daemon overlay note](19-daemon-overlay.md).

The selected [payload derivation](../issues/307/S5-PAYLOAD-CONTRACT.md) describes
schema v8. Each live inode generation has a lower-view cutoff, shrink epoch and
indexed shrink staircase. Cells store bytes only through their last valid byte;
NULL validity means every stored byte is valid. A partial cell carries a trimmed
bit mask. [Cell normalization](../../crates/layerfs-overlay/src/payload/cells.rs),
[layers/shrink](../../crates/layerfs-overlay/src/payload/layers.rs) and
[window operations](../../crates/layerfs-overlay/src/payload/stream.rs) own these rules.

Workspace exposes `Write { serial, position: At|End, data: WriteData }`, size
changes in `SetAttributes`, and `SourceView::read`. Append selects the current
EOF inside the owner job. Data, validity, inode size/mtime and the publication
ticket change in one transaction. Data is a shared owned byte slice, with debug
output limited to its length. One write/read window is at most 128 KiB; larger
files and flows use successive windows. No write copies base payload upward.

A full-cell overwrite is one upsert; each partial edge is one cell seek/merge
and upsert. At most 33 cells intersect a request. Shrink eagerly trims one edge,
lowers the lower-view cutoff, and records one staircase step after a binary
search. It neither deletes nor walks discarded cells. Regrow never resurrects
discarded local or inherited bytes. A hole has no cell rows. Captures preserve
exact sealed bytes/cutoffs; known install replaces the lower view with its exact
canonical equivalent without rewriting later layers.

The read job composes one bounded local window and an inherited-byte bitmap.
Workspace demands one covering canonical range outside SQL, even when thousands
of gaps need base bytes. `cell`/`captured_cell` are raw stored observations;
`source_read`/`captured_read` provide the effective view, including staleness.

Per write, work is O(request bytes + touched cells times indexed key work), plus
O(log shrink height) for stale partial edges. Read costs actual window bytes per
live layer and indexed cell/staircase work. S5 alone does not bound layer depth
across failed captures or reclaim stale cells/abandoned steps; S6 owns both.
S6 now provides [bounded live composition and independent custody](33-independent-custody.md).
Positive zero-reference tombstones prevent fall-through until install; descriptor
writes require exact open authority and use the independent orphan domain.
Symlinks retain one format-bounded target window; [S6 composed readlink](34-name-and-lookup-custody.md)
follows its effective layers and independent lookup/request custody.

`construct_runs`, `FileRuns`, `FileRun` and `RunConstruction` belong to content.
The [zero-run derivation](../issues/307/S5-HOLE-CONTRACT.md) preserves the frozen
32 KiB zero CDC period and existing canonical codecs. A bounded threshold probe
chooses whole-file representation; chunked holes scan a boundary chunk and tail,
then reuse complete chunks and mapping subtrees. The existing unfinished-page
lookahead/final partition produces the same root as ordinary streaming.
Worst-case run work is O(chunk bytes + unfinished capacity times tree height),
with logarithmic height and bounded resident boundary pages. Actual data still
costs its bytes. Source/consumer failures stop the one attempt, and synchronous
child-before-parent acceptance retains downstream backpressure. The consumer
owns accepted canonical objects; storage completion still requires Save finish.

Retained evidence is [S5 checks](../issues/307/checks/s5-payload/). On macOS,
65,536 alternating one-byte writes do not increase a full-window overwrite:
45 statements/2,752 VM steps/36 changed rows, matching an unfragmented file.
Shrink discarding 2 versus 2,048 cells has identical 16 statements. The tiny-file
fixture reports 184 allocated bytes per 100-byte file; dense cells use 1.133
pages per cell. These are shared allocation deltas, not exclusive per-row pages.
A one-byte append replaces at most one 4096-byte cell. Payload bind/copy work is
request bytes plus at most two edge cells, masks and framing. SQLite additionally
copies/buffers pages and rollback journal; exact journal/dirty-byte and aggregate
pager/OS-cache peaks are unavailable through the current safe driver diagnostics
and remain S7 observations, not invented zeros or a bounded-RSS claim.

Complete real-owner profiles include source acquisition/release, every semantic
round, publication, reply-attempt release and the post-observation route seek.
They retain original failed/confounded cases. Reply credits cover the admitted
result; the Workspace port clones a read result before releasing those credits,
and base reads plus caller sinks have separate custody. Native reply/kernel and
aggregate output credits remain S8. P3's resident deferred-edit refusal remains
explicitly assigned to backed S10 editing; S5 does not claim its removal or
integrated sparse Commit acceptance.
