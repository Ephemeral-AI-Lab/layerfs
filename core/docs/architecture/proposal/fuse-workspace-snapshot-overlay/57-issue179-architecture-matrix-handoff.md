# Issue 179 architecture and operation-matrix update: task-3 handoff

> **Status: Current assignment for the next implementation agent.**
> Written after `f802cc124` closed task 2 of `55-issue179-realdaemon-handoff.md`
> section 5, and after the owner directed that the branch be pushed and the
> issue updated. The governing plan is still `51-implementation-completion-spec.md`.
> This note does not close the issue; it hands over exactly one task: the
> architecture/operation-matrix update and the exact reproduction commands.

## 1. The worktree and its state

- Worktree: `/Users/yifanxu/.codex/worktrees/795c/layerfs`, branch
  `codex/pair1-remote-mount`, pushed to `origin` through the documentation
  commit that carries this note. Product commits on the branch, in order:
  - `a9657b85f` (task 1): the post-Commit rename fix and the completed
    `commit_small_project` scenario.
  - `f802cc124` (task 2): the mounted namespace syscall evidence plus the
    live link-count presentation fix.
- Working tree clean apart from this handoff note. Read `AGENTS.md` and
  `core/AGENTS.md` first — the architecture-document rules that govern this
  task are in `core/AGENTS.md` ("Architecture documents follow the code"):
  docs describe the workspace's product source, a change to a stated rule
  updates the affected document in the same commit, every document records the
  source commit it was written against, and a pin is advanced explicitly.
- Read, in this order: `51-implementation-completion-spec.md` (governing plan,
  §7's checklist is the definition of done for the phase),
  `55-issue179-realdaemon-handoff.md` §5 (the task list),
  `56-issue179-mounted-syscall-handoff.md` (task 2's assignment and its §2
  invariants), and this note.

## 2. What tasks 1 and 2 established (do not redo, do not regress)

Task 1 (`a9657b85f`, receipts in `core/target/pair1-evidence/round56/`):
`commit_small_project` passes end to end — two explicit Commits, public
saved-state reads, root A readable after B, remount, acknowledged read,
checked Unmount + CloseClean — and all ten native namespace selections pass
with the new read-the-replaced-FD-after-Commit rename assertion.

Task 2 (`f802cc124`, receipts in `core/target/pair1-evidence/round57/`):
the mounted FUSE syscall route for the six namespace operations passes
(`final-ns-mounted-kernel-01`, `final-ns-mounted-durability-01`), all ten
native selections re-pass on the fixed source (`final3-ns-<case>-01`), and
`commit_small_project` re-passes with the rebuilt daemon
(`final3-t1-small-project-01`). Task 2 also fixed one real product defect the
route exposed, diagnosed with labelled diagnostic builds before editing
(`diag-ns-mounted-kernel-01`, `diag-ns-mounted-durability-01..03`).

Product invariants your documentation must state correctly:

1. A rename publishes the **moved identity's own record** when the live root
   holds none; a rename destination binding shadows an inherited binding;
   `keep_name` clears the removal record a rename wrote for a name it binds
   again (one name owns a binding or a removal, never both).
2. A **moved directory whose namespace record is absent is refused**
   (`Unsupported`): a committed directory's children resolve through the
   canonical tree by path, and the bounded profile has no identity-keyed
   service query to re-anchor them after a move. Moving a fresh directory
   works. This is a declared limitation.
3. A replaced or unlinked committed identity with a live local owner gets a
   **canonical carry record** into the successor root, only while that owner
   is live (open handle or lookup reference).
4. A metadata-only setattr falls back to the **record's current** mode/mtime,
   not the original version's (`filesystem/write.rs`).
5. The **live namespace link count is presented as `references`** at every
   attribute seam: `State::presented` maps a regular inode's `references` to
   `node.names` (with the generation's fresh-name count as the fallback when
   the node was collected), `cache_lookup` baselines a newly resolved node
   from that tracked count, and the link, OPEN, SETATTR and resize replies
   all present it (`runtime/state.rs`, `filesystem/namespace.rs`,
   `filesystem/write.rs`, `filesystem/create.rs`, `filesystem/open.rs`).
   The FUSE layer maps it to `st_nlink` for regular files, hardcodes 2 for
   directories and 1 for symlinks (`layerfs-fuse/src/replies.rs`).
6. **Within one baseline** the resident node and a resolution must agree on
   the namespace link count; a Commit that republishes the identity with a
   changed canonical count **refreshes** the resident node instead of
   refusing (`cache_lookup`'s references comparison is baseline-gated like
   the adjacent original/roots staleness check), and `serial_original`'s
   stale-baseline query checks serial and kind only (`filesystem/original.rs`).
   Declared bounded corner: a re-resolved base identity whose delta link-count
   adjustment was lost with its resident node presents the canonical count
   until the next Commit republishes it; no registered selection reaches it.
7. `RootOwner::seal` queues and completes cleanup frames for pages it drops to
   zero refs; the arena `update` admits at most 4 cells per call for D/I/N
   keys, 1 for E/T keys.

## 3. The task (55 section 5 item 3, verbatim scope)

> The architecture and operation-matrix update. Refresh
> `01-workspace-fuse-contract.md` section 7 and the documents that state the
> rules the last two rounds changed: the bounded second metadata call and its
> derived floor, the re-anchor rule, the page-edge ownership rule,
> `close_clean`'s overlay release, the reconcile carry, the declaration
> ledger, rename replacement semantics, the `list_view` tombstone filter, and
> — new this round — `keep_name`'s binding-versus-removal rule. Record the
> exact reproduction commands for every route this phase uses.

Since 55 was written, task 2 added two more rules to that list: the live
link-count presentation with its baseline-gated guards (section 2, items 5
and 6) and the mounted namespace route's reproduction commands.

**This is a documentation-only task.** No product source should change. If
writing the documentation exposes a real product defect, stop and bring the
owner the trace and a recommendation instead of fixing it in this task.

## 4. Checklist

1. **`01-workspace-fuse-contract.md` — the operation matrix.** Section 7.2's
   rows for `create`, `mknod`, `mkdir`, `symlink`, `unlink`, `rmdir`,
   `rename`, `link` and `setattr` mode/mtime still describe *missing shared
   capabilities* in their gap column. Every one of them is now implemented,
   mounted and evidenced; replace the gap column with the implemented
   behavior and the receipt that proves it (section 5 below names them).
   Section 7.1's `lookup`/`getattr` rows now carry the live link-count
   presentation and the proven handle-based getattr after rename/unlink;
   `readdir` carries the tombstone filter. Section 7.3 keeps rename
   exchange/whiteout unsupported (`RENAME_NOREPLACE` is the only selected
   flag) and records the committed-directory move refusal as a declared
   limitation. Section 8.1's attribute-projection table has no link-count
   row: add one (regular files present the live namespace link count;
   directories 2; symlinks 1; the mapping lives in
   `core/crates/layerfs-fuse/src/replies.rs`).
2. **`02-overlay-snapshot.md`** — section 2.3 (precedence and tombstones):
   `keep_name`'s binding-versus-removal rule, the rename destination binding
   shadowing an inherited binding, and the `list_view` tombstone filter.
   The re-anchor rule (an edited parent's record resolves from the live root
   and the record's own revision, never from a cached path plus a service
   fallback) belongs with the delta-record rules here or in 01's lookup row —
   state it where it reads most naturally and cross-reference the other.
3. **`03-commit-integration.md`** — section 7 (known completion and live
   successor reconciliation): the reconcile carry with its live-owner filter,
   the declaration ledger, rename replacement semantics (the replaced
   identity's record is preserved for its open handles and moves to the
   successor root), and the successor-root re-baseline of an identity's
   canonical link count with the baseline-gated refresh on the resident node.
4. **`13-portable-metadata.md`** — the bounded second metadata call and its
   derived floor; the metadata-only setattr fallback to the record's current
   unnamed fields.
5. **`15-local-range-edit.md`** — the page-edge ownership rule.
6. **`28-control-close-clean.md`** (with `21-routine-reclamation.md` as
   needed) — `close_clean`'s overlay release.
7. **Advance the pins.** Every document you touch records the source commit
   it was written against (`01` and `02` carry dated status headers, `03` a
   written-against pin). Advance the pin to the commit your documentation
   round lands on and say so; never silently re-date a document.
8. **`README.md` packet pointers.** "Current namespace implementation state"
   still points at `54`. Advance it through the 55 → 56 → 57 chain to the
   current state (the real-daemon scenario complete, the mounted syscall
   evidence complete, the operation matrix refreshed), keeping the
   historical chain paragraphs intact below it.
9. **Reproduction commands.** Give the exact commands (section 5 below) a
   permanent home: the operation-matrix document or the README's evidence
   section — wherever a reader verifying a matrix row would look first.
   Include the routes that evidence this phase: the ten native namespace
   selections, the two mounted namespace selections, and
   `commit_small_project`; reference `40-mounted-mkdir.md`,
   `44-mounted-create.md` and `48-mounted-symlink.md` for the earlier
   mounted routes instead of restating their commands.
10. **Check and commit** per section 6.

## 5. Verification identities and mechanics (exact)

| Identity | Value |
| --- | --- |
| Host binaries | `core/target/pair1-evidence/binary-archive/57cc5227bb963dc4e0fde628dc594ba088e1411d95d485eca8266dc78c28084e/host` |
| Runtime image | `sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4` |
| Fixture | `core/target/pair1-evidence/fixtures/large-edit-master-01/result.json` |
| Native namespace test binary | `core/target-linux/debug/deps/namespace-dc60a93a3a48653b` (sha256 `f9b94ed8…`) |
| Mounted namespace test binary | `core/target-linux/debug/deps/mounted_namespace-b4fa939a7bd6f016` (sha256 `f5ed8262…`) |
| Linux daemon (fixed source) | `core/target/pair1-evidence/round57/layerfs-daemon-fixed` (sha256 `ec064a3a…`) |
| Build container | long-lived `lfs-build-179`; tests at `/tmp/s` (target `/tmp/tb`), daemon at `/tmp/daemon-src` (target `/tmp/daemon-tb`) |

Reproduction, from the repository root — each into its own fresh output
directory; receipts are append-only and runs are serialized (per-worktree
measurement lock):

```sh
BIN=core/target/pair1-evidence/binary-archive/57cc5227bb963dc4e0fde628dc594ba088e1411d95d485eca8266dc78c28084e/host
IMG=sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4

# The ten native namespace selections (setattr mknod link unlink unlink_fresh
# rmdir rename rename_base generation notification_failure):
python3 core/crates/layerfs-workspace/tests/namespace_route.py \
  --fixture core/target/pair1-evidence/fixtures/large-edit-master-01/result.json \
  --binaries "$BIN" --test-binary core/target-linux/debug/deps/namespace-dc60a93a3a48653b \
  --output core/target/pair1-evidence/<round>/<fresh-dir> --case <case> --image "$IMG"

# The two mounted namespace selections (kernel, durability):
python3 core/crates/layerfs-workspace/tests/mounted_namespace_route.py \
  --fixture core/target/pair1-evidence/fixtures/large-edit-master-01/result.json \
  --binaries "$BIN" --test-binary core/target-linux/debug/deps/mounted_namespace-b4fa939a7bd6f016 \
  --output core/target/pair1-evidence/<round>/<fresh-dir> --case <case> --image "$IMG"

# The real-daemon small-project scenario:
LAYERFS_CONSTRUCTION_WORKERS=1 python3 core/crates/layerfs-daemon/tests/control_commit.py \
  --fixture core/target/pair1-evidence/fixtures/large-edit-master-01/result.json \
  --binaries "$BIN" --linux-daemon core/target/pair1-evidence/round57/layerfs-daemon-fixed \
  --output core/target/pair1-evidence/<round>/<fresh-dir> --case commit_small_project \
  --image "$IMG"
```

Rebuilding the Linux test binaries and the daemon in `lfs-build-179` uses the
exact commands in `56-issue179-mounted-syscall-handoff.md` section 5: tar the
`core` workspace into `/tmp/s` (or `/tmp/daemon-src` for the daemon), build
with `CARGO_TARGET_DIR=/tmp/tb` (or `/tmp/daemon-tb` plus
`--target aarch64-unknown-linux-gnu`), repeating all four ARMv8 `RUSTFLAGS`
because `/tmp` is outside the repository root's `.cargo/config.toml`, then
`docker cp` the binary out. A documentation-only round needs no rebuild: the
receipts above already exist
(`round57/final3-ns-<case>-01`, `round57/final-ns-mounted-{kernel,durability}-01`,
`round57/final3-t1-small-project-01`) and the commands are recorded for
reproduction, not for re-running.

## 6. Verification and the commit

Documentation-only change; run the standard pre-commit checks anyway and
report exactly which ran:

```sh
cargo +1.85.1 fmt    --manifest-path core/Cargo.toml --all -- --check
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

Production LOC for the commit message: `tools/production_loc.py --root ROOT
--json` on `git archive` snapshots of the parent commit and the staged tree
(`git add -A && git write-tree` restricted to your documentation files),
scope `crates core/crates`. Documentation does not contribute, so the
expected result is delta 0 with the unchanged totals: core 50,312 (workspace
14,562), reference 65,417, combined 115,729. Do not run or claim the
whole-core `cargo test` suite — it runs once on the final frozen source in
the final round.

Commit message: what was refreshed, document by document, with the receipt
path that evidences each rule you state, plus the `Production LOC:` line.

## 7. What is NOT yours this round

- Any product-source change. If the documentation work exposes a real defect,
  bring the owner the trace and a recommendation.
- The final whole-core checks and the #179 closure report (the final round).
- The merge to the main line and closing the issue (owner acts).
- Any numeric bound, admission rule, worker count or cache policy change.

## 8. Copyable prompt for the next agent

```text
Work task 3 of issue #179: the architecture and operation-matrix update plus
the exact reproduction commands.

Use /Users/yifanxu/.codex/worktrees/795c/layerfs on codex/pair1-remote-mount
(pushed to origin). The product commits are a9657b85f (task 1: post-Commit
rename fix and the completed commit_small_project scenario) and f802cc124
(task 2: the mounted namespace syscall evidence and the live link-count
presentation fix). The working tree is clean.

Read AGENTS.md and core/AGENTS.md first, then:
  core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/51-implementation-completion-spec.md
  core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/55-issue179-realdaemon-handoff.md (section 5, item 3)
  core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/56-issue179-mounted-syscall-handoff.md (section 2 invariants)
  core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/57-issue179-architecture-matrix-handoff.md
57 is your assignment: section 2 lists the invariants your documentation must
state correctly (including task 2's live link-count presentation and the
baseline-gated guards), section 4 is the document-by-document checklist,
section 5 the exact reproduction commands, section 6 the commit discipline.

This is a documentation-only task: refresh 01-workspace-fuse-contract.md
section 7's operation matrix (every namespace operation row still claims a
missing shared capability; all are implemented, mounted and evidenced now),
add the link-count row to section 8.1, update the rule documents that state
what the last three rounds changed (02-overlay-snapshot: keep_name's
binding-versus-removal, tombstone precedence and the list_view filter, the
re-anchor rule; 03-commit-integration: the reconcile carry with its
live-owner filter, the declaration ledger, rename replacement semantics, the
link-count re-baseline; 13-portable-metadata: the bounded second metadata
call and its floor; 15-local-range-edit: the page-edge ownership rule;
28-control-close-clean: close_clean's overlay release), advance every touched
document's source pin explicitly, advance the README's current-state pointer
off 54, and record the exact reproduction commands for the ten native
namespace selections, the two mounted namespace selections and
commit_small_project (57 section 5 has them verbatim).

State only what the receipts prove: cite the evidence path for each rule
(round56/ for the scenario, round57/final3-ns-* for the native selections,
round57/final-ns-mounted-* for the mounted evidence). The committed-directory
move refusal, the rename exchange/whiteout refusal and the re-resolved
base-identity link-count corner are declared limitations - document them as
such, never as unsupported-by-oversight.

Do not change product source. If the documentation work exposes a real
product defect, stop and bring the owner the trace and a recommendation.

Before committing: cargo fmt --check, clippy -D warnings on the core
workspace, the boundary guard and its self-tests. Commit with the exact
production LOC accounting (documentation-only: delta 0; core 50312 /
workspace 14562 / reference 65417 / combined 115729). Do not run or claim the
whole-core test suite, and do not push, merge or close the issue.
```
