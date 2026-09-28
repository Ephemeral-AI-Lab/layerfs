# #273 → #264 continuation: incomplete semantic union, NO-MERGE

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Source: first parent `1a0a48d66e95d37d866f6242459e2521b5d4b6c1` (docs-only
> successor of `8dcc47206`, product tree `cb23d730b`). This document describes
> changes in its own commit; the commit message records the exact production LOC.
> No PR merged, no historical receipt amended, and no numeric arm taken.

## Live state and source custody (2026-09-28 19:25 UTC)

`gh pr view N --json number,state,isDraft,headRefOid,baseRefName,headRefName`
and `gh issue view 276 --json state,comments` found no movement from the
[completion checkpoint](PREMERGE-COMPLETION-20260928.md#1-rechecked-live-state-2026-09-28t1605z):

| PR | Head | Base branch | Live state | Disposition |
| --- | --- | --- | --- | --- |
| #262 | `6bcfa464` | `codex/issue245-review-fixes` | OPEN draft | Bounded source review only; NO MERGE. |
| #272 | `48b51e8` | `codex/issue-271-root-edge-io` | OPEN draft | No numeric admission; NO MERGE. |
| #274 | `29fc5747d` | `codex/issue271-root-custody` | OPEN draft | C1 repair is on owned branch, not PR head; NO MERGE. |
| #263 | `ef3a3104` | `codex/issue245-review-fixes` | OPEN | Partial integrated semantics only; NO MERGE. |
| #269 | `6eb7553` | `codex/issue258-phase4` | OPEN draft | Repair is on owned branch, not PR head; union still red; NO MERGE. |
| #275 | `c39d6085` | `main` | OPEN | Unrelated statfs; not reviewed. |

#276 remains OPEN; last owner comment is 2026-09-28 15:40:26 UTC (1A/2A,
separate repairs, functional-only deferral). No merge authorization inferred.
The inherited branches `fbda0f0f1`, `05eb5c148`, `6f18a5f42`, and #274/#263
are ancestors of this first parent; no merge/rebase conflict in this continuation.
Original #263/#269 conflicts and LOC remain as recorded in the checkpoint.
Production LOC for this commit's first parent versus staged tree, counted with
`python3 tools/production_loc.py --json --root <git-archive-snapshot>` on
`git archive HEAD` and `git archive $(git write-tree)` using the same counter:
Core **70,457 → 70,408 (-49)**, reference **65,417 → 65,417 (0)**,
combined **135,874 → 135,825 (-49)**. The removal is the now-unused
charged path-origin map, not a claimed algorithmic speed improvement.

## Changes, limited proof, and blocking red cells

The active selected view now requests `ChildAttributes {parent,name}` and
`InodeList {serial,...}` for inherited names and `InodeReadlink {serial}` for
canonical symlinks (including a moved link and a pinned lease). The old
charged `ActiveOrigins` path-copy map was removed because these canonical
queries resolve by immutable inode serial. The production Server already
answered these identity queries. The `readable` test's in-process service did
not: both fixture handlers now answer exactly the queries issued by this
route (ChildAttributes, InodeList, InodeAttributes, InodeReadlink). No assertion
was removed or relaxed. Active publication remains the only mutation route;
no RootOwner mutation functions were restored.

**Blocking:** the active resident Node is still a 4,096-byte fixed path and
`child_path` and `preflight_rename_paths` retain that limit, including a
never-resident inherited subtree walk. Identity-based canonical resolution
alone cannot admit the two #264 beyond-4,096 move cases; removing the check
without replacing resident-node representation would overflow its fixed array.
Likewise, a detached retained parent can still accept a mutation, the active
route carries no RootOwner escrow, and the descendant-cost and quota semantics
of the #269 suite differ. A new solution needs charged identity/ancestry
representation and explicit refusal/cleanup invariants; **do not weaken the
suite** to mark this partial port complete.

Linux aarch64, Docker Desktop kernel 6.12 linuxkit, `rust:1.85.1-bookworm`,
owned ext4 volume, `LAYERFS_TEST_BACKING_ROOT=/data`, `TMPDIR=/data`,
`LAYERFS_STAGE_TEST_ROOT=/data`, `LAYERFS_CONSTRUCTION_WORKERS=1`, privileged
container with `/dev/fuse`, `SYS_ADMIN` and `MKNOD`, locked offline Cargo in
worktree-local `core/target/issue273-linux-owned`:

- `cargo test --offline --manifest-path core/Cargo.toml --locked -p layerfs-sdk --test inherited_workspace -- --nocapture`: **4/11 PASS; 7 FAIL**. The passed cells are `fresh_upper_directory_move_is_the_control`, `base_file_move_from_an_unmodified_root_keeps_its_identity`, `moved_canonical_symlink_reads_target_by_serial`, and `moved_directory_keeps_frozen_g1_and_later_g2_write`. Failed: `a_successful_directory_rename_seals_once_and_refunds_exactly` (expected 851,968 escrow, active route 0); `base_directory_move_keeps_inherited_children_handles_and_commits` (expected 3 vs actual 4 in held directory listing); `growing_rename_refuses_private_budget_before_publication` (**expected Capacity/Backing refusal but rename succeeded**, so quota divergence, not a PASS); both `growing_*beyond_4096_bytes` cases (Capacity); `pinned_directory_retains_forgotten_ancestors_and_detached_parent_refuses_mutation` (detached mknod succeeded); `growing_prefix_move_has_descendant_independent_private_counts` (resident nodes 10 vs 138). These are seven independent retained failures, not folded into the original three hypotheses.
- macOS Darwin 25.4 arm64, `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-workspace --test readable`: **9/9 PASS** (previous union 3/9). The full `-p layerfs-workspace` package run on macOS also passed **17/17**
(including the readable suite). This is in-process fixture closure, not a
native daemon or Linux route-driver proof.
- Warning-denying workspace Clippy, fmt check, boundary scanner (359 files), boundary self-tests (9) and `git diff --check`: PASS on this staged product change.
- Broad macOS `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --no-fail-fast`: **INCOMPLETE**, timed out at 160 s (the log had reached the `commit_staged` external test after earlier suites; no complete-workspace PASS is claimed). Targeted package checks are separate and cannot promote this aggregate attempt.

## Required matrix, falsifiers, capability, recommendation

| Required final-tree cell | Result |
| --- | --- |
| #269 inherited namespace union / detached ancestry / quota/custody | **FAIL (4/11)** as above. |
| #264 readable test (macOS fixture) | **PASS (9/9)**; Linux route-driver variant **NOT_RUN**. |
| Full Linux route-driver matrix: inherited/resident/never-resident moves, deep lookup/list/readlink, move-back, replacement, cycle, detached-parent, old heads | **NOT_RUN** on this partial union. |
| Active tiny/Base/Zero/Payload/overlap/slot, G1/G2 same PID/fd/inode; handles/aliases/forget; known-C1-local-C5 failure; uncertain cleanup | **NOT_RUN** on this source. Earlier source-pinned results remain earlier results. |
| Original #248 4,097-write functional gate and anchored C1 edit-load-zero | **NOT_RUN** on this source, no numeric promotion. |
| Rebuilt daemon image + completed-tree SDK lease proof | **NOT_RUN**: union is incomplete; earlier SDK-lane image cannot prove this source. |
| SDK lease deadline, response Budget, known C1 + local C5 and uncertain-release falsifiers | **NOT_RUN**: no deterministic fault/budget gates were built here. |
| Common VM/backend/device cache identity and phase-local cgroup peaks | **PARTIAL/falsified** at prior host source: `memory.reclaim` and current/stat deltas work, `memory.peak` reset does not. Observer unsealed. |
| Matched numeric campaign and clean/one-edit registered controls | **NOT_RUN / INELIGIBLE**; nine historical numeric INELIGIBLE rows unchanged. |
| #256 many-file/package, #270 path-local C1 move Commit | **NOT_PROVED**. |

NO MERGE for #262, #272, #274, #263 or #269 on this union. No PR head
has gained this branch's repair. The cache gate is not prospectively sealed,
there is no numeric admission or release claim. The owner must review a
completed and re-proved union before deciding any PR merge; do not close #276.
Owned Docker container and ext4 volume were removed after the test; the
worktree-local Cargo target stays for incremental non-measurement builds.
