# Issue 179 real-daemon integration: fourth-round handoff

Written after the round that fixed the replaced-identity record and the rename
destination binding, and that wrote the 51 section 6 real-daemon scenario as far
as it now runs. The governing plan is still `51-implementation-completion-spec.md`;
this file does not replace it and does not close the issue. It records what now
passes, the exact blocker that stops the integration scenario, the reference-source
study the next round owes before it edits anything, and the four tasks plus two
owner acts that remain before #179 can be closed.

## 1. State in one paragraph

Commit `107c81a18` (branch `codex/pair1-remote-mount`, parent `22c0181fd`) fixes
three real defects in the registered rename path and adds the registered
`commit_small_project` case to `core/crates/layerfs-daemon/tests/control_commit.py`.
All **ten** `namespace_route.py` selections pass on this source, and the whole-core
checks are clean: 702 tests passed / 0 failed, `cargo clippy -- -D warnings`,
`cargo fmt --check`, `python3 core/tools/check_product_boundary.py` (260 production
files) and the guard's six self-tests. The new scenario reaches and verifies
**Commit A** and performs the second edit, then **fails** in the second Commit's
rename with `EIO`; section 3 is the exact diagnosis. Nothing here is a performance,
durability or crash-recovery claim; no numeric bound was raised, no registered
selection was dropped, and no assertion was removed. Nothing is pushed. The round
before this one, `54-issue179-namespace-closure-handoff.md`, remains accurate for
everything it records.

## 2. What this round fixed, and how it was found

The 51 section 6 scenario found each of these by reaching an assertion no
registered case had reached. All three are in `107c81a18`.

1. **A rename left a removal record for the name it bound.** The registered
   `namespace_rename` case only asserted that the destination name was
   *listed*; it never asserted *which inode* the name resolved to. The mounted
   scenario did, and read the replaced inode's bytes back through the moved
   name. `overlay/directories.rs::keep_name` now clears the removal record a
   rename wrote for a name it binds again: one name owns a binding or a removal,
   never both.
2. **A rename treated an inherited binding as its own entry.** `rename.rs`
   consulted `has_entry` on the delta's own entry page, and the destination
   parent's inherited binding was not that page's. The publication now writes
   the destination binding whatever the origin held, so an inherited binding is
   shadowed rather than skipped. This is also what makes the replacement
   resolvable from the very next lookup.
3. **A replaced base identity had no record of its own.** `victim.txt` in the
   scenario is a base identity: its canonical version lives in the attached base
   and no delta record names it. Replacing it left the destination pointing at an
   identity with no record, so the directory listing itself failed and, after a
   Commit, even the held FD could not be resolved. `runtime/state.rs` now records
   one base identity that lost its last local name while a **live local owner**
   (a handle or a lookup reference) still addresses it, and
   `commit/reconcile.rs` carries that record into the successor root *only while
   that owner is live*.

The liveness filter in item 3 is load-bearing and was found the hard way: carrying
a record unconditionally regressed the registered `generation` selection to
`close_clean` → `Busy`, because the carried record kept the arena unreclaimable
after its last owner was gone. With the filter, `generation` passes again.

## 3. The blocker: a rename cannot resolve a destination parent after a Commit

This is the single product defect between the current state and 51 section 6's
step 5. It is not a test or harness problem.

**Symptom.** `commit_small_project` passes step 1 through step 4 (attach, mount,
the negative authorization check, the first phase's syscalls, Commit A and the
first phase's public saved-state verification). Step 5 then renames
`left/a.txt` → `right/moved.txt` on the remounted live Workspace, and the rename
returns `EIO`; the following `chmod`/`stat` on `right/moved.txt` inherit it:

```
OSError: [Errno 5] Input/output error: '/layerfs/workspace/read/right/moved.txt'
```

**Trace.** A diagnostic build of the same source printed, in order:

```
DIAG rename start src=4/"a.txt" dst=5/"moved.txt"
DIAG rename resolved src=File dst=Err(Service(Failure { code: PathNotFound, unknown: false, ... }))
DIAG inspect path="right" base=<32-byte root> error=Service(Failure { code: PathNotFound, ... })
```

**Reading.** The rename resolves its destination name. The name does not exist,
so `resolve_child` falls back to resolving the **destination parent** `right`.
That fallback reaches a service `Inspect::Attributes` query for the logical path
`"right"` against a root that answers `PathNotFound`. `resolve_child` then returns
`WorkspaceError::Io` (not `NotFound`), so rename never takes the "destination is
free" branch, and every later lookup under `right` inherits the same error.

**Mechanism, to the extent it is confirmed.** The node for `right` is still
resident, but its `baseline` is stale after Commit A, so
`filesystem/original.rs::serial_original` does not return the node's own cached
`original`/`content`/`metadata`; it falls through to `inspect_view` with the
node's **cached path** (`right`), a locator recorded before the Commit.

**What is NOT yet established, and must be established before the fix.**

- Whether Commit A's successor root contains `right`'s namespace record at all
  (`arena.find(successor_root, namespace_key(right_serial))`).
- The exact 32-byte base root the failing `inspect_view` used, compared against
  `branch.effective_root` from a public `BranchSnapshot` for the same commit.
- Whether the service's canonical tree for that root contains `right`.
- Whether the *content* or the *directory* seam is at fault: `right` is a
  directory, and `serial_original` refuses directories early
  (`WorkspaceError::IsDirectory`), so the failing query is reached from the
  directory path in `resolve_child`, not from `serial_original`'s file branch.

**Where to look, and the likely shape of the fix.** Section 4 is the study the
next round owes. The short version: `resolve_child` must resolve an edited
parent's record from the **live root** and the record's own revision, the same
way the re-anchor rule in `create.rs` / `remove.rs` / `rename.rs` treats a
directory delta, instead of trusting a cached path plus a service fallback. Do
not guess: the previous two rounds' rename diagnoses were both wrong on the first
attempt, and the same is true here.

**One known non-blocker.** The daemon admits exactly one control session
(`layerfs-daemon/src/control.rs`: an accepted connection is closed while another
session is live or closing). The scenario needs a control session for
Status/Unmount/Remount/CloseClean and a public Service session for saved-state
reads, and cannot hold both. The case handles this by owning one at a time and
retrying the hand-off. This costs time and explains the case's structure; it is
not what fails.

## 4. Read v0.1.6 before editing anything

The next round **must** study the reference release's FUSE and Commit design
before it changes `rename.rs` or anything under `commit/`. The replacement is a
deliberate re-architecture of exactly the machinery this blocker sits in, and the
release answers the question the blocker asks — how a name is resolved and how a
renamed or replaced identity keeps its identity — in a different way that is
worth understanding before re-deriving it.

**Sources, in this order.**

1. `core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/05-v016-source-comparison.md`
   — the existing 987-line audit of the release's FUSE crate and the daemon, SDK,
   Workspace, snapshot and host-construction seams. Read sections 1, 3, 4 and 6
   first; section 9.1 is the must-keep / must-not-port list. This document is
   informative, not a contract, and it predates the current blocker — treat it as
   the map, not as the answer.
2. `03-commit-integration.md` and `02-overlay-snapshot.md` — the design owners for
   the Commit seam.
3. The release source itself, at the immutable tag `v0.1.6`
   (`44cf748486863ab7c21ca47e731bd88e2b9a7b4a`), read through `git show` so the
   working tree is untouched:
   - `crates/layerfs-fuse/src/live_owner.rs` — `rename_async` (line ~2016) and
     the surrounding name/namespace operations.
   - `crates/layerfs-workspace/src/cow_tree.rs` — `acquire_name` (line ~251) and
     `rename` (line ~655), where a name becomes a resident identity.
   - `crates/layerfs-workspace-core/src/namespace.rs` — `NameInput`,
     `NameLookup`, `ResolvedName` (line ~174), the name-resolution contract.
   - `crates/layerfs-workspace/src/remote_commit.rs` and `reconcile.rs` — the
     release's Commit and reconciliation seams.
   - `crates/layerfs-fuse/src/filesystem.rs` — the kernel callback surface
     (`rename` at line ~569, plus `mknod`, `unlink`, `rmdir`, `link`, `setattr`).

**Questions the study must answer in writing, before any edit.**

1. How does the release resolve a name, and what makes a renamed or replaced
   identity keep its identity across a rename and across a Commit? The short
   answer is visible in `cow_tree.rs::acquire_name`: a lookup yields a
   `ResolvedName` carrying an optional `NodeId`, and `LiveWorkspace` keeps both
   `nodes: HashMap<NodeId, Node>` and `canonical_nodes: HashMap<inode, NodeId>`.
   Write down what the replacement replaced that with, and why.
2. In the release, what is the locator for a node whose last name is gone, and
   who owns its bytes while a handle is open? Compare with the replacement's
   `Node.path` plus `State::carried` and reconcile's `Frontier::Unbound`.
3. In the release, how is a destination parent found for a rename, and what
   happens when the parent's record is not in the delta? Name the exact function
   and the exact lookup key.
4. What does the release do on Commit for a live node whose names all went away —
   worse, better or the same as the replacement's carry-plus-liveness filter?
5. Which release mechanism is **not** portable into this bounded profile
   (`51` section 2), and what is the bounded substitute? The release's resident
   node table is the obvious candidate; say so explicitly and say what the bound
   is.

**Deliverable of the study.** A short written section appended to this file (or a
new numbered note) that answers the five questions with file/function/line
citations from the `v0.1.6` blobs, and states which of them changes the fix for
section 3's blocker. Do not port the release's resident tables; `51` section 2
keeps the 128-entry admission rules, 256 resident nodes, 128 handles and the
8 MiB accounted default.

## 5. What is left, in order

Section 3's blocker comes first. Tasks 1, 2 and 4 are exactly the three that
`54` section 6 named and this round did not finish.

1. **Fix section 3's blocker, then finish the 51 section 6 scenario.** The case
   is already registered (`commit_small_project`) and already runs steps 1-4; it
   needs step 5's rename to resolve, then step 5's Commit B, then step 6's
   close-every-FD, remount, acknowledged-state read and checked
   Unmount/CloseClean. Acceptance is unchanged: two explicit Commits, no hidden
   intermediate Commit, and a receipt naming both generations and heads.
   Extend the registered `rename` selection with the same assertion the mounted
   scenario makes — read the replaced FD after its Commit.
2. **The mounted FUSE syscall evidence for the six operations.** Follow the
   mounted-route pattern (`mounted_mkdir_route.py` is 22 lines specialising the
   shared driver): one mounted route specialising `namespace_route.py`'s driver,
   with Rust cases modelled on `mounted_create.rs`, covering kernel `mknod`,
   `link`, `unlink` with a held FD, `rmdir` for the empty and nonempty cases,
   `rename` for cross-directory, NOREPLACE collision and directory-cycle refusal,
   and `setattr` for chmod plus `UTIME_OMIT` mtime. Assert errno, inode identity,
   link counts, listing visibility and post-Commit durability through the real
   mount. The adapter callbacks already exist in `layerfs-fuse/src/adapter.rs`.
3. **The architecture and operation-matrix update.** Refresh
   `01-workspace-fuse-contract.md` section 7 and the documents that state the
   rules the last two rounds changed: the bounded second metadata call and its
   derived floor, the re-anchor rule, the page-edge ownership rule,
   `close_clean`'s overlay release, the reconcile carry, the declaration ledger,
   rename replacement semantics, the `list_view` tombstone filter, and — new this
   round — `keep_name`'s binding-versus-removal rule. Record the exact
   reproduction commands for every route this phase uses.

## 6. Verification, exactly

Binaries and receipts this round.

| Identity | Value |
| --- | --- |
| Host binaries | `core/target/pair1-evidence/binary-archive/57cc5227bb963dc4e0fde628dc594ba088e1411d95d485eca8266dc78c28084e/host` (`layerfs-service` `57cc5227…`, `examples/public_key` `104a69a5…`) |
| Linux daemon | `core/target/pair1-evidence/binary-archive/67c19245c2008bbd3f1f24c482536ebcfe6386bf7cd9ed8c2be88b8c889f0efe/layerfs-daemon` |
| Linux test binary | `core/target-linux/debug/deps/namespace-dc60a93a3a48653b` |
| Runtime image | `sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4` |
| Build container | long-lived `lfs-build-179`, container-local source `/tmp/daemon-src` (daemon) and `/tmp/s` (tests), target `/tmp/daemon-tb` / `/tmp/tb` |
| Fixture | `core/target/pair1-evidence/fixtures/large-edit-master-01/result.json` |
| Namespace receipts | `core/target/pair1-evidence/round55/round55c-ns-<case>-01/result.json` (ten PASS) |
| Scenario receipts | `core/target/pair1-evidence/round55/t1-small-project-*.` (one attempt each; `…-z` is the traced failure) |
| Check logs | `core/target/pair1-evidence/round55/checks/` |

Reproduction, from the repository root:

```sh
BIN=core/target/pair1-evidence/binary-archive/57cc5227bb963dc4e0fde628dc594ba088e1411d95d485eca8266dc78c28084e/host
python3 core/crates/layerfs-workspace/tests/namespace_route.py \
  --fixture core/target/pair1-evidence/fixtures/large-edit-master-01/result.json \
  --binaries "$BIN" --test-binary core/target-linux/debug/deps/namespace-dc60a93a3a48653b \
  --output core/target/pair1-evidence/round55/<fresh-dir> --case <case> \
  --image sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4

LAYERFS_CONSTRUCTION_WORKERS=1 python3 core/crates/layerfs-daemon/tests/control_commit.py \
  --fixture core/target/pair1-evidence/fixtures/large-edit-master-01/result.json \
  --binaries "$BIN" \
  --linux-daemon core/target/pair1-evidence/binary-archive/67c19245c2008bbd3f1f24c482536ebcfe6386bf7cd9ed8c2be88b8c889f0efe/layerfs-daemon \
  --image sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4 \
  --output core/target/pair1-evidence/round55/<fresh-dir> --case commit_small_project
```

Rebuild the Linux test binary and the Linux daemon in `lfs-build-179` (about two
and three seconds) with the exact commands in section 5 of
`53-issue179-namespace-completion-handoff.md`; the container-local target
directories are reused, and `RUSTFLAGS` must repeat all four ARMv8 flags because
`/tmp` is outside the repository root's `.cargo/config.toml`.

Whole-core checks on the committed source, from the repository root with Rust
1.85.1, locked and offline, `CARGO_BUILD_JOBS=2`,
`LAYERFS_CONSTRUCTION_WORKERS=1`, `CARGO_TARGET_DIR=$PWD/core/target`:

```sh
cargo +1.85.1 test   --manifest-path core/Cargo.toml --workspace --locked --offline   # 702 passed, 0 failed
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings
cargo +1.85.1 fmt    --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py                                          # 260 files, PASS
python3 -m unittest discover -s core/tools -p 'test_*.py'                             # 6 tests, OK
```

Two process notes this round cost real time and are worth not repeating. The
host archives under `core/target/pair1-evidence/binary-archive/` are subject to
macOS code-signature evaluation; a stale copy can be `SIGKILL`ed on exec even
though `cmp` shows it byte-identical to a working copy. Rebuild the archive
directory when a host binary mysteriously dies. And a route run against a test
binary built before the last source change reports `SIGKILL`/`-9` from the
container, which is a stale binary, not a product failure.

## 7. Production LOC comparison

| Scope | Before | After | Delta |
| --- | --- | --- | --- |
| Replacement core | 49,916 | 49,972 | +56 |
| Reference `crates/` | 65,417 | 65,417 | 0 |
| Combined | 115,333 | 115,389 | +56 |

Method: the unchanged `tools/production_loc.py`, no flags, on exact snapshots
(`git archive REV crates core/crates` into a root, then `--root ROOT --json`) of
`22c0181fd` and the committed tree `107c81a18`. The growth is the reconcile
carry, the rename destination binding and `keep_name`, not relocation or a scope
change; the test and driver changes in the same commit do not enter the count.
`layerfs-workspace` moves 14,166 -> 14,222. This handoff's own documentation
commit changes no production line.

## 8. Two items that need the owner

Unchanged from `54` section 6, and both still open.

1. **The push, merge and issue closure.** `107c81a18` and its predecessors are
   local to `codex/pair1-remote-mount`. #179 closed no box this round, because
   section 5's tasks 1-3 are unfinished; do not close it on this commit.
2. **The Linux clippy component.** The pinned image reports
   `cargo-clippy is not installed for the toolchain
   '1.85.1-aarch64-unknown-linux-gnu'`, so the clean host clippy run above is
   what stands. Either the component is added to the pinned image or the gap is
   accepted in writing.

## 9. Copyable prompt for the next implementation agent

```text
Close #179: fix the post-Commit rename resolution blocker, then finish the
bounded Linux Workspace/FUSE implementation and prove it through the real
daemon and mount.

Use /Users/yifanxu/.codex/worktrees/795c/layerfs on codex/pair1-remote-mount.
HEAD is the tip of that branch - the documentation commit that carries this
prompt, whose parent is the last product commit `107c81a18` - the working tree
is clean, and nothing is pushed.

Read AGENTS.md and core/AGENTS.md, then, in this order:
  core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/51-implementation-completion-spec.md
  core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/55-issue179-realdaemon-handoff.md

51 is the governing owner-directed plan and its section 7 is the definition of
done for this phase. 55 section 1 records what now passes; 55 section 3 is the
one product blocker; 55 section 5 is the remaining work.

BEFORE YOU EDIT ANY PRODUCT SOURCE, do what 55 section 4 requires: study how
the v0.1.6 release handles FUSE and Commit. Read
05-v016-source-comparison.md sections 1, 3, 4, 6 and 9.1, then read the release
source through `git show v0.1.6:...` (tag v0.1.6 = 44cf748486863ab7c21ca47e731bd88e2b9a7b4a):
  crates/layerfs-workspace/src/cow_tree.rs      (acquire_name ~251, rename ~655)
  crates/layerfs-workspace-core/src/namespace.rs (NameInput/NameLookup/ResolvedName ~174)
  crates/layerfs-fuse/src/live_owner.rs          (rename_async ~2016)
  crates/layerfs-fuse/src/filesystem.rs          (rename ~569, mknod/unlink/rmdir/link/setattr)
  crates/layerfs-workspace/src/remote_commit.rs and reconcile.rs
Answer, in writing in that file, the five questions in 55 section 4 - in
particular (a) what the release's name resolution and its NodeId/canonical_nodes
tables give a renamed or replaced identity that this bounded replacement's
cached-path Nodes do not, and (b) which release mechanism is not portable into
this profile and what the bounded substitute is. Do not port the release's
resident tables; 51 section 2 keeps every current numeric bound.

Then fix 55 section 3's blocker: after a Commit, a rename cannot resolve a
destination parent whose name the attached base still holds, because
resolve_child falls back through a stale cached path to a service query that
answers PathNotFound. Establish first the four facts 55 section 3 lists as not
yet established (the successor root's namespace record for the parent, the exact
base root used, whether the canonical tree holds the parent, and which of the
content/directory seams is at fault), then fix the owning layer so an edited
parent's record is resolved from the live root and the record's own revision
rather than from a cached path. Do not guess: the last two rounds' first rename
diagnoses were both wrong.

Then work 55 section 5's three tasks in order, committing each with its exact
per-commit production LOC accounting:
  1. extend the registered namespace `rename` selection to read the replaced FD
     after its Commit, and finish the registered `commit_small_project` case
     through both explicit Commits, the remount and the checked close;
  2. add the mounted FUSE syscall route for mknod, link, unlink, rmdir, rename
     and setattr, following mounted_mkdir_route.py and mounted_create.rs;
  3. update 01-workspace-fuse-contract.md section 7 and the architecture
     documents the last two rounds changed, including keep_name's
     binding-versus-removal rule, with exact reproduction commands.

Work the way this phase works. Run one attempt per registered selection into its
own output directory under core/target/pair1-evidence/, build the Linux test and
daemon binaries in the long-lived lfs-build-179 container (53 section 5), and
compare against a build of the parent commit in its own container target
directory so a pre-existing failure is never reported as a regression or the
reverse. Keep every registered numeric limit, one construction worker, the
host's single primary remote admission and the existing failure ownership. The
daemon admits exactly one control session: never hold a control session and a
witness session at once (55 section 3, "one known non-blocker"). Make a failing
selection pass by fixing the product it registers; when a registered case
contradicts the operation it registers, bring the owner the trace and a
recommendation. Add tests that verify new behaviour end to end rather than
restating the implementation, and report FAIL, INCOMPLETE and unrun work as
plainly as PASS.

Then run the whole-core checks once on the final source, from the repository
root with Rust 1.85.1, locked and offline, CARGO_BUILD_JOBS=2,
LAYERFS_CONSTRUCTION_WORKERS=1, CARGO_TARGET_DIR=$PWD/core/target:

  cargo +1.85.1 test   --manifest-path core/Cargo.toml --workspace --locked --offline
  cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings
  cargo +1.85.1 fmt    --manifest-path core/Cargo.toml --all -- --check
  python3 core/tools/check_product_boundary.py
  python3 -m unittest discover -s core/tools -p 'test_*.py'

Finish with the closure report the owner needs to close #179: which of 51
section 7's boxes are closed and which are not, the exact commands and receipt
paths, the per-commit LOC comparison, the accepted bounds and deferred work
stated separately from the open failures, the study's answers, and the two items
that need the owner's decision or action - the push/merge and the issue closure
itself, and the Linux clippy component.
```
