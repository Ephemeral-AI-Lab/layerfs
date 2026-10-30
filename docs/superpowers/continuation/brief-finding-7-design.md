# Retro finding 7 — late line windows byte cursor (DESIGN ONLY)

Role: contract designer. DESIGN ONLY — do NOT implement. You
do NOT commit, push, open PRs, or touch the git clone, the
baseline, or the connect checkout. The commander reviews your
design, and only after the commander approves it will a
separate implementation brief be written.

Workdir: the Area P product tree (re-verify the p1-p15 tip at
launch). Report: retro finding 7 (P8, medium).

## Problem

Every `startLine` read opens a fresh scan at file offset
zero. Earlier lines are counted and discarded, and the read
cache intentionally lives only for one call. For a file of B
bytes read sequentially in 48 KiB windows, total content
processing is O(B²/48 KiB), not O(B). P8's tests cover a large
single line but not a large many-line file at increasing
offsets.

Locations (re-verify at launch; line numbers from the
2026-10-29 retro report):
- `engine/crates/layerfs-store-service/src/read/window.rs:128-191,309-335`
  (`scan` calls `view.read_range(reader, 0..size, ...)` at
  :180-185; `lines` merely skips to `from` at :323-335)
- `engine/crates/layerfs-store-service/src/read/reads.rs:35-47`
- `engine/crates/layerfs-store-service/src/read/cached.rs:1-5,21-27`
- Tests: `area_p8_reads.rs`

## Design task — a LARGER additive SDK/CLI contract change

Design, do not code, the following (this is NOT a Store
schema or transport-codec change):

1. Track byte offsets at line boundaries and add an optional
   byte cursor (`nextOffset` beside `nextLine`) so the next
   call starts `read_range` at the known boundary.
2. Bind the cursor to the immutable content AND the line
   number. Specify exactly what a cursor from another commit,
   another file, or a changed file does (refusal code and
   message class, or a defined safe behavior — decide from
   precedent, e.g. how `diff_commits`/`changed_paths` cursors
   refuse, and state the precedent).
3. Specify how sequential cursor use preserves text-mode
   Binary classification already performed by earlier pages
   (classification must not silently change mid-file).
4. Retain numeric line-only calls for compatibility. Specify
   the precedence when both a line start and a cursor are
   given (cursor wins, or an explicit refusal — decide and
   state the precedent, as finding 5 did for cursor vs
   pagePos).
5. Specify the Rust, JS, and CLI surface changes (argument
   shapes, response fields, CLI flags) and the scaling tests
   that will prove O(B) sequential reads (two file sizes,
   byte/read counters, exactly-once in-order lines across
   pages).
6. Address the rejected alternative explicitly: a bounded
   immutable-content line-index cache would avoid the API
   addition but still has a cold full scan and adds cache
   identity/eviction complexity. Say why the byte cursor
   remains the simpler durable shape, or overturn that
   conclusion with evidence if the design work shows
   otherwise.
7. List every file the implementation will touch and every
   test it will add, with the old-code failure each new test
   must exhibit.

## Output

A design document (not code) covering: the cursor format and
binding, refusal semantics, classification preservation,
compatibility + precedence, the full surface change, the test
plan with discrimination proofs, and the implementation file
list. End with an explicit list of open questions the
commander must decide before implementation. Do not claim a
design is complete while a question that changes the wire
shape is still open.
