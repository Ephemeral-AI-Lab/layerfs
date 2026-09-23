# #237: known-length native Init candidate stopped before timing

> **Status:** Archived, not adopted, no public 10k sample. The owner selected
> the earlier reorganized `ImportBatch` research source and its one-shot
> **1,110.332-ms / 270.189-MB/s** observation as the stopping point.

The [prospective protocol](evidence/known-length-stopped/prereg.md) proposed
passing the length already checked by native source scanning into a new C1
constructor. The isolated source commit `644b226ed` added
`construct_stream_known_length` and used it only for native Init, leaving the
generic constructor unchanged. It used exact-size scratch for files below
128 KiB and sent larger files directly to CDC instead of buffering and
replaying the 128-KiB threshold prefix. The fixture has 9,499 files below
128 KiB (including 100 empty files) and 501 larger files. The existing
probe requests about 1.31 GB of **cumulative capacity across files**, not
peak memory; an earlier standalone synthetic probe measured that entire
reserve/read sequence at only 6.560 ms, so no large public gain was proven.

The isolated candidate's focused C1 `file_complete` checks and Service
compile check passed, and control/candidate release binaries were built. The
[sealed prospective pair record](evidence/known-length-stopped/pair-prospective.json)
contains their identities and fresh output paths, but **neither public arm was
run**. Before timing, review found two public-API edge cases in the candidate:
its small path allocated with `vec![0; length]` instead of the existing
fallible reserve path, and its large path used saturating `expected_len + 1`,
which cannot detect growth at `u64::MAX`. A correction was started but
interrupted by the owner's rollback request; it was not rebuilt or timed.
The [committed candidate diff](evidence/known-length-stopped/candidate-source-and-tests.diff.gz)
and [unfinished correction diff](evidence/known-length-stopped/interrupted-correction.diff.gz)
are retained as records. The uncommitted correction was then restored in the
isolated worktree, which is clean. Its candidate commit remains isolated for
provenance; no known-length source entered the root worktree.

The root research product under `core/crates/` is byte-for-byte the same Git
tree as the accepted refactor checkpoint `970854f2c` (tree
`0d2aa55282b7bf968ccbd64fb71cc332caff5eb1`). Packs stay in SQLite
BLOBs, SQLite pages remain 4,096 B, the whole-file cutoff stays 128 KiB,
and native Init retains the four existing file constructors and one C2 owner.
The accepted public receipt remains `INCOMPLETE` because of telemetry loss;
its source payload was 0/27,503 resident pages before timing and a separate
full reopened 10k/300-MB readback passed. Metadata cache residency remains
unqualified. The owner accepts this **research stopping point**, not a
release-admission PASS.
