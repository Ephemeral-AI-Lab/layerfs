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

## Committed checkpoint-3 source and retained proof

Product and test source were committed at
`a4c54e62b805c56545e6144c383307f6dcd57f85`, Git tree
`fa7e145dd53ad702a8f6287b8af31857963e83e2`, first parent `3897d7cdd`.
The commit's exact first-parent/staged `tools/production_loc.py` comparison is
combined **127,926 → 128,964 (+1,038)**, Core **62,509 → 63,547
(+1,038)** and reference **65,417 → 65,417 (0)**. The new Core lines are
capture/lowering code, with no reference retirement. This docs/receipt
successor adds zero production LOC.

The clean-source `stage_route.py` outputs under
`core/target/issue273/committed-*-01/` were copied byte-for-byte into
[`evidence/checkpoint3`](evidence/checkpoint3/). Each used a fresh native
Service/history producer, an independent byte copy of the closed 64 MiB
fixture Store, an owned ext4 Docker volume, warning-denying locked release
product binaries and `LAYERFS_CONSTRUCTION_WORKERS=1`. The arm is functional:
`cache_claim=null`, no timer gate or speed comparison. The five complete
command walls include setup, Service, Docker, test and cleanup:

| Case | Status | Complete wall | Public observation |
| --- | --- | ---: | --- |
| `active_generation` | PASS, 2/2 checks | 0.994 s | G1 staged bytes, G2 live bytes, both Commits |
| `active_close` | PASS, 2/2 | 0.948 s | fresh file/name Commit and clean close |
| `active_namespace` | PASS, 2/2 | 1.020 s | directory/file/link/symlink, then successor rename/unlink |
| `active_mounted` | PASS, 2/2 | 0.982 s | actual FUSE write/append, unmount, Commit, canonical bytes |
| `semantics` | PASS, 3/3 | 1.033 s | native Save held while G2 edits, frozen/live separation |

The Stage test binary SHA-256 is
`589c42dc1cc3546a2aec77cfb7b4470fbbb6a8153d3adc6e60157929b38278ba`;
the Service binary is
`95b6d54c0c16bab0da66fbfa8653c1f99d2374db75435a6dd3b31f6095b5b530`;
the route's product-input hash is
`148beb413a8c2e449ecf012f10b845c5a4f66ee6ecaced36f999b5d05cb01a18`.
The runtime image is
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`.
The repository AEAD profile `.cargo/config.toml` SHA-256 is
`3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9`;
the locked `core/Cargo.lock` SHA-256 is
`09b880a18e1c221ba830908467f0985b0419ae0bf3c80c90ede7f7c5178272d6`.
Each route receipt records its own exact binary, driver, fixture and output
identities. The cross build used locked Cargo release, rustc 1.85.1 and
`aarch64-unknown-linux-musl`; no debug performance binary was used.

At the committed source, Linux `active_backing` passed **24/24 in 26.00 s**,
`readable` passed **18/18** nonignored cases, and its privileged mounted
selection passed **2/2**. The latter two test walls were 0.06 s and 0.08 s.
They are functional, not speed samples. The full Core test command's two
unchanged `layerfs-content` failures above remain a **FAIL**, even though
targeted Workspace tests and the other named checks passed. The retained
full test log and all copied receipts are append-only evidence. No public
backing-space bound, mixed-page refund proof, measured-phase cache claim, or
checkpoint-5 selection is promoted by this checkpoint.
