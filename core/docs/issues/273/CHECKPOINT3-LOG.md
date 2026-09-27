# #273 checkpoint 3 running evidence log

This log begins at first parent `3897d7cdd` (checkpoint-2 product pin
`b68c968e3`). All paths below are in the isolated
`codex/issue273-active-head` worktree. This is functional work, not a timed
candidate or release admission. Source and receipt identities for the final
committed checkpoint are appended after that commit; these development
diagnostics retain their original status.

## Implementation and observations before the source commit

- `overlay/snapshot.rs` captures one active index root, namespace/inode/dirty
  revision, generation, Branch context and sealed pack tail under the state
  gate. `commit/active.rs` lowers that pinned final view through existing
  SaveFile, portable metadata, symlink and prepared C5 operations. Reconcile
  installs saved roots while preserving successor G2 extents and names.
- The locked release `aarch64-unknown-linux-musl` Stage binary exercised an
  owned Docker ext4 volume with a fresh live native Service and independent
  byte-copy fixture per case. `active_generation` passed G1 staged bytes,
  simultaneous G2 live bytes, CommitStaged and the G2 composite Commit.
  `active_close` passed fresh file/name Commit and public clean close.
  `active_namespace` passed fresh directory/file/link/symlink Commit followed
  by rename/unlink Commit. `active_mounted` passed real FUSE write/append,
  unmount, Commit and independent canonical byte read. The last development
  outputs are under `core/target/issue273/final-stage-active-*-01/`; the
  mounted output is `core/target/issue273/stage-active-mounted-01/`. These
  runs predate the source commit and do not pin its Git identity.
- The existing `stage_semantics` concurrency case passed after its external
  fixture was corrected to open a writable handle and expect existing rather
  than fresh identity rows. It blocked native Save while G2 published and
  checked frozen candidate bytes, live successor bytes and one Stage call.
  `stage-semantics-01` failed at its stale read-only handle and
  `stage-semantics-02` failed at its stale fresh-row assertion; both receipts
  remain under `core/target/issue273/`. `stage-semantics-03` passed all three
  registered checks in 0.96 s complete command wall.
- The release Linux `readable` selection passed 18/18 nonignored tests (two
  mounted cases ignored in that selection). The release `active_backing`
  selection passed 23/24; the one failure was the fixture expecting revision
  2 after `capture` even though active capture now advances the Workspace and
  index revisions together. The corrected focused case passed 1/1. The full
  24-case backing suite was not repeated at this source identity.
- Warning-denying locked release Core workspace Clippy, Core examples build,
  Core fmt, product boundary (339 production Rust/SQL files), nine boundary
  guard tests, and targeted Core Workspace tests on macOS passed. The full
  locked Core test command failed in the same two unchanged `layerfs-content`
  `filesystem_ordering` cases recorded at checkpoint 2:
  `a_fresh_build_charges_its_count_array_to_the_ordering_ceiling` and
  `a_high_pending_ceiling_runs_spill_free_to_the_byte_bound` (actual object
  count 19 against limit 18). Exact local log:
  `core/target/issue273/checks/core-test-checkpoint3-01.log`. No
  `layerfs-content` or `layerfs-storage` source changed.

## Retained nonpassing diagnostics

- `stage-lowering-01`: `INCOMPLETE` before the test because Alpine lacked
  `findmnt`; its owned container and volume were removed explicitly.
- `stage-lowering-02`: `FAIL` in the old external helper, whose default
  read-only handle attempted a write. The helper now opens ReadWrite.
- `stage-lowering-03` and labelled `stage-lowering-diagnostic-04` through
  `-06`: `FAIL` in the old test's read at offset 32 MiB on a 326,098-byte
  staged file. The Service reported C1 `InvalidRange { start: 33554432,
  end: 33554944, length: 326098 }`; the test's source assumption does not
  match `stage_route.py`'s 326,300-byte fixture. The old case was not promoted
  or relabelled. Temporary first-party Service diagnostics were removed from
  source after the cause was recorded.
- `stage-active-close-01`: `FAIL` in the external fixture's fresh-file
  observation guard; the case now selects its existing fresh-file fixture
  option. `stage-active-namespace-01`: `FAIL` because the test chose `alias`,
  a name already present in the canonical fixture; the corrected test uses
  `new-alias`. All failed outputs remain at fresh paths under
  `core/target/issue273/`; their owned Docker containers/volumes were
  cleaned after inspection.

## Open work after checkpoint 3

Mixed live/dead sealed-page compaction, public many-file and retained-generation
allocated-block accounting, exact pin-aware refunds after Commit, the
preregistered 3 × 3 public matrix, clean/one-edit controls, and the unchanged
#248 gate are `NOT_RUN`. No cache-qualified speed number or 2× claim exists.
`Q_fetch`, per-phase wall, mounted full backing allocation, and complete-command
cleanup charge have not yet been recorded. The historical #248 `FAIL` and #271
cache-`INELIGIBLE` receipts retain their status.
