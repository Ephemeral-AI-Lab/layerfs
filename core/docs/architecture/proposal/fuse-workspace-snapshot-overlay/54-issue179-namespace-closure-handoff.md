# Issue 179 namespace implementation: third-round handoff

Written after the round that closed the eight registered selections
`53-issue179-namespace-completion-handoff.md` left open. The governing plan is
still `51-implementation-completion-spec.md`; this file does not replace it and
does not close the issue. It records what now passes, the cause and fix of each
selection, the four owner rulings this round used, and the four tasks plus two
owner acts that remain before #179 can be closed.

## 1. State in one paragraph

Commits `5e4a89d0a` and `a7356d665` (branch `codex/pair1-remote-mount`, parent
`a7ee87d6e`) raise the registered selections that pass from eight of sixteen to
**sixteen of sixteen**: all ten `namespace_route.py` cases and all six
`mkdir_route.py` cases. The stage route's ten registered selections were re-run
on this source as a regression check and all ten pass, as do the whole-core
locked tests (702 passed, 0 failed), `cargo clippy -- -D warnings` and
`cargo fmt --check`. Nothing here is a performance, durability or crash-recovery
claim; no numeric bound was raised, no registered selection was dropped, and no
assertion was removed. The two round-53 commits and every historical receipt
remain unchanged on disk.

## 2. The eight selections, their cause and their fix

1. **`rename_base` — `list_view` merged the origin without its tombstones.**
   Fixed as diagnosed: the merge drops names the delta tombstoned and reads the
   next origin page instead of returning a listing a mounted caller would read as
   the end of the directory (`filesystem/namespace_view.rs`). The case then
   exposed a second gap: a hard link to an identity the attached base still owns
   left no local record, so the new name could not resolve, lowering had no record
   to declare, and an open handle lost its content at the first Commit. `link`
   now materialises that identity's record from the exact base roots it was
   resolved from, `resolve_child` answers such a name from the identity's own
   locator, and `State::linked` keeps the fresh-name ledger to identities this
   generation created.
2. **`unlink` — an open orphan lost its record at the first Commit.** The
   successor root now carries that identity's record unchanged in the inode phase
   while emitting no dirty key and no declaration for it
   (`commit/reconcile.rs`). Two further causes followed: `edges()` counted a
   maintained directory's entry page but not its removal page, so one arena page
   leaked per removed inherited name and `close_clean` refused `Busy`; and a
   creation owned a handle whether or not it opened one, so a mknod'd node was
   never collected.
3. **`unlink_fresh` — `close_clean` refused `Busy`.** The cause was not a live
   root: `close_clean_until` released every owner except its own live overlay, so
   a Commit whose successor root carries records left the arena unreclaimable.
   Closing now releases that overlay before the arena is reclaimed.
4. **`generation` — a later rename was refused while G was retained.** Traced, and
   round 53's hypothesis was wrong: the refusal was the consumer's single remote
   admission, held by the save parked in its delivery, while the rename's
   destination and the mknod's existence check each needed a read-only
   consultation. Two product fixes and two corrections followed; section 3
   records the admission ruling.
5. **`notification_failure` — a mounted rename returned `Coherence`.** A
   cross-directory rename notifies both parents, but only the first entry found
   the pending status, so the second reported failure on a notification that had
   already succeeded. `complete_projection_mutation` now accepts a later entry of
   the same mutation while the unchanged revision still names it.
6. **`rename` — the case contradicted the operation.** Section 4 records the
   owner ruling and the corrected expectation.
7. **`mkdir_successor` — the second Commit was refused `InvalidInput`.** The
   declaration ledger was cleared wholesale by the first Commit, so a directory
   the successor generation created was never declared. The submission now
   records the declarations it sends, and a completed Commit forgets exactly
   those (`commit/directories.rs`, `overlay/snapshot.rs`, `runtime/state.rs`).
8. **`mkdir_capacity` — a refused mkdir consumed a reservation.** The whole local
   envelope is now admitted before the operation consumes a serial reservation,
   so a refusal it can see coming leaves the count untouched
   (`filesystem/create.rs`).

## 3. Owner rulings this round used

1. **`rename`**: correct the registered expectation, do not change the semantics.
   The case now asserts NOREPLACE's ordinary meaning in both directions — an
   occupied destination is refused with the source left where it was, a free one
   is accepted — and keeps every later assertion, with the file moved back so the
   commit-side checks still name `right/target`.
2. **`generation`**: admit the work rather than record the conflict. One bounded
   metadata call — a read-only inspection or one serial reservation — may now
   overlap a remote call that is *actually in flight*. The primary admission still
   admits exactly one save, construction, history or attach call, so one
   construction worker stays one producer; the second slot carries no input and
   never a save, construction or history Commit; and it is refused while no call
   is in flight, so a retained admission (a held read reply, a pending submission)
   keeps the single-call refusal that `readable.rs` pins. The host's minimum
   memory budget grows by one call allowance (128 KiB) for that slot; the 8 MiB
   default budget and every other registered bound are unchanged.
3. **`notification_failure`**: add the missing explicit Commit before the close,
   because a dirty Workspace keeps the native close refusal and the case's
   registered `native-clean-close` row can only pass after the published rename is
   committed.
4. **Clippy**: fix the three pre-existing findings rather than record them.
   `rename_from` takes one `RenameRequest`, and `name_page` takes a slice and
   elides its lifetime.

Two further corrections of the same kind were needed by the selections that had
never reached their final assertions: `generation` names the two consecutive
generations a fresh attach actually uses (1 and 2, not 0 and 1), and the three
cases that close themselves now forget exactly the Local lookup references they
took, including their directories. Each case's own assertions are unchanged.

## 4. Further product defects this round exposed

Each was found by a selection reaching an assertion it had never reached before,
and each is fixed in `a7356d665`:

- A delta is re-anchored on the frozen capture itself, not on the parent of the
  root it was read from, so the first mutation after a capture anchors correctly.
  A record the successor generation created keeps its own rows, which are exactly
  what that generation counted; a stale re-anchor made the Commit's own counters
  disagree with the pages it emitted.
- Converting an inode to a captured record resets its edit count and replacement
  bytes: the whole version is one base read from there on, and a metadata-only
  mutation kept a stale list that failed lowering.
- A moved regular identity's cached path follows the new name, so a later service
  read does not ask for the name the rename removed.
- A replaced identity releases the lookup reference and the generation ledger
  entry its name owned, so a fresh replaced inode is not declared with no binding
  left; its record still reaches the successor root for its open handle.
- The rename's row accounting is signed, because dropping a locally bound name
  without a removal record takes a row away.

## 5. Verification, exactly

One attempt per selection, one output directory each, under
`core/target/pair1-evidence/round54/` (`final3-ns-*`, `final3-mkdir-*`,
`regress-stage-*`, `regress2-stage-*`), with the pinned image
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`, the
fixture `core/target/pair1-evidence/fixtures/large-edit-master-01/result.json`,
the host binaries in `binary-archive/d582e04078…/host`, and the test binaries
built in the long-lived `lfs-build-179` container (about two seconds a round).
The parent column is its own build of `a7ee87d6e` in a separate container target
directory, staged as `namespace-base-dc60a93a3a48653b` /
`mkdir-base-5c889e2cd6a39bdf`, and not a remembered baseline.

One attempt, `mkdir/capacity`, hit the pre-existing transient transport failure
`Service(Failure { code: Io })` that `native-create/create-capacity-01` already
records at `ab473145`. Its diagnostic re-run passed with byte-identical
observations (`accepted=108 … pages_after=30`). The failing receipt stays on disk
and the transient cause is not explained by this round.

Whole-core checks on this source, from the repository root with Rust 1.85.1,
locked and offline, `CARGO_BUILD_JOBS=2`, `LAYERFS_CONSTRUCTION_WORKERS=1`,
`CARGO_TARGET_DIR=$PWD/core/target`:

```sh
cargo +1.85.1 test   --manifest-path core/Cargo.toml --workspace --locked --offline   # 702 passed, 0 failed
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings
cargo +1.85.1 fmt    --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

## 6. What is left to close #179

The work below is what section 8's prompt asks for, in the order it asks for it.
Each item names the artifact to change and the evidence that closes it.

1. **The 51 section 6 real-daemon small-project scenario.** Add one registered
   case to `core/crates/layerfs-daemon/tests/control_commit.py`, reusing its
   helpers, and script all six of 51 section 6's steps through a real mount,
   service and two explicit authenticated Commits, with a remount and a checked
   close. Closes 51 section 7's real-daemon integration box.
2. **Mounted FUSE syscall evidence for `mknod`, `link`, `unlink`, `rmdir`,
   `rename` and `setattr`.** Add one mounted route that specialises
   `namespace_route.py`'s driver plus its Rust cases, following
   `mounted_mkdir_route.py` and `mounted_create.rs`. The callbacks already exist
   in `layerfs-fuse/src/adapter.rs`; this supplies the missing actual-kernel
   evidence for the six operations. Closes 51 section 3's "applicable actual
   FUSE path" requirement.
3. **The last correctness gap.** A replaced identity that the generation did not
   create still loses its local record when a later Commit produces an empty
   successor root, so its open handle then needs the service to resolve a name
   that is gone. Materialise that identity's content and metadata roots at
   replacement time, carry records for identities with a live local owner into
   the successor root, and extend the registered `rename` case to read the
   replaced FD after its Commit. Closes 51 section 7's replaced/open-unlinked
   identity box for base identities.
4. **The architecture and operation-matrix update.** Refresh
   `01-workspace-fuse-contract.md` section 7 and the documents that state the
   rules this round changed: the bounded second metadata call and its derived
   floor, the re-anchor rule, the page-edge ownership rule, `close_clean`'s
   overlay release, the reconcile carry, the declaration ledger, rename
   replacement semantics and the `list_view` tombstone filter. Record the exact
   reproduction commands for every route this phase uses. Closes 51 section 7's
   documentation box and `core/AGENTS.md`'s same-commit documentation rule.

Two items need the owner rather than the agent: the **Linux clippy component**
(the pinned image reports `cargo-clippy is not installed for the toolchain
'1.85.1-aarch64-unknown-linux-gnu'`, so the clean host clippy run above is what
stands), and the **push, merge and issue closure** themselves. A third item is
worth one bounded check: `Service(Failure { code: Io })` on a long selection is a
real transient transport outcome — already recorded at
`native-create/create-capacity-01`, `ab473145` — rather than a product refusal,
and it should be characterised rather than silently re-run.

Beyond the bounded phase, and deferred by 51 section 2 rather than cancelled:
R5b (the declared npm workflow), R6 and the matched mounted comparison owned by
#207, and the full writable target's capacity, concurrency and performance
qualification.

## 7. Production LOC comparison

| Scope | Before | After | Delta |
| --- | --- | --- | --- |
| Replacement core | 49,494 | 49,916 | +422 |
| Reference `crates/` | 65,417 | 65,417 | 0 |
| Combined | 114,911 | 115,333 | +422 |

Method: the unchanged `tools/production_loc.py`, no flags, on exact snapshots
(`git archive REV crates core/crates` into a root, then `--root ROOT --json`) of
`96371122f` and the committed tree `a7356d665`. The round's own two commits are
`49,642 -> 49,823 (+181)` (`5e4a89d0a`, parent `a7ee87d6e`) and
`49,823 -> 49,916 (+93)` (`a7356d665`, parent `5e4a89d0a`). The growth is the
namespace, identity, ownership, re-anchor and admission fixes above, not
relocation or a scope change; test and documentation changes do not enter the
count. This handoff's own documentation commit changes no production line.

## 8. Copyable prompt for the next implementation agent

```text
Close #179: finish the bounded Linux Workspace/FUSE implementation, prove it
through the real daemon and mount, and hand the owner a completed closure
checklist.

Use /Users/yifanxu/.codex/worktrees/795c/layerfs on codex/pair1-remote-mount.
HEAD is e26b40bf9, working tree clean, the product commits already made; nothing
is pushed.

Read AGENTS.md and core/AGENTS.md, then, in this order:
  core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/51-implementation-completion-spec.md
  core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/54-issue179-namespace-closure-handoff.md

51 is the governing owner-directed plan, and its section 7 is the definition of
done for this phase. 54 records the current state: all sixteen registered
namespace and mkdir selections pass, the stage route still passes and the
whole-core checks are clean. Start from 54 section 6 and work these four tasks in
order, committing each one with its exact per-commit production LOC accounting.

1. Write the 51 section 6 real-daemon small-project scenario. Extend
   core/crates/layerfs-daemon/tests/control_commit.py with one new registered
   case, reusing its existing helpers (begin_commit, commit, current,
   verify_saved, remount, mounted_edit, held_live_files, close_client) and the
   docker_route.py / mounted_read.py harness, and script all six of 51 section
   6's steps: attach and mount a writable Workspace with one negative
   authorization check; create two directories, several tiny files, a regular
   mknod file, a symlink and a hard-link alias, then write, append, truncate,
   chmod and set mtime with atime omitted (UTIME_OMIT); rename between
   directories while the replaced destination FD stays open and check that FD
   still addresses its own inode; unlink a last name with its FD held and read
   and write that FD; prove a nonempty rmdir and a NOREPLACE collision fail
   without partial change, then remove an empty directory; verify names,
   contents, inode identities, link counts and portable metadata with
   lstat/readlink/read/listing. Request explicit authenticated Commit A, verify
   the branch head and the saved namespace, content and metadata through public
   Service reads, then make a small second edit with a rename, an unlink and a
   metadata change, request Commit B, and verify B's exact tree while A's root
   stays readable and unchanged. Close every FD, remount the same live
   Workspace, read the acknowledged state, and finish with checked
   Unmount/CloseClean and normal cleanup. Acceptance: the case passes with two
   explicit Commits, no hidden intermediate Commit, and a receipt naming both
   generations and heads.

2. Add the mounted FUSE syscall evidence for the six operations. Follow the
   mounted-route pattern (mounted_mkdir_route.py is 22 lines specialising the
   shared driver): add one mounted route that specialises namespace_route.py's
   driver, with its Rust cases modelled on mounted_create.rs, covering kernel
   mknod, link, unlink with a held FD, rmdir for the empty and nonempty cases,
   rename for cross-directory, NOREPLACE collision and directory-cycle refusal,
   and setattr for chmod plus UTIME_OMIT mtime. Assert errno, inode identity,
   link counts, listing visibility and post-Commit durability through the real
   mount. The adapter callbacks already exist in layerfs-fuse/src/adapter.rs, so
   this task produces evidence for them and fixes whatever it exposes.

3. Close the last correctness gap in 54 section 6: a replaced identity the
   generation did not create must keep its record for its open handle across a
   Commit. Materialise that identity's exact content and metadata roots at
   replacement time using the link-to-base pattern in filesystem/create.rs, and
   have commit/reconcile.rs carry records for identities that still have a live
   local owner (a handle or a lookup reference) into the successor root even
   when the dirty frontier is empty. Extend the registered rename case to read
   the replaced FD after its Commit, which is the assertion 51 section 6 step 2
   already asks the integration scenario to make.

4. Update the architecture documents and the operation matrix for the rules the
   last round changed, in the same commits as any further change: the bounded
   second metadata call and its derived floor (runtime/host.rs,
   runtime/state.rs), the re-anchor rule (filesystem/create.rs, remove.rs,
   rename.rs), the page-edge ownership rule (backing/metadata_index.rs),
   close_clean's overlay release (runtime/lifecycle.rs), the reconcile carry and
   the declaration ledger (commit/reconcile.rs, commit/directories.rs), rename
   replacement semantics and the list_view tombstone filter
   (filesystem/namespace_view.rs). Refresh the operation matrix in
   01-workspace-fuse-contract.md section 7 and record the exact reproduction
   commands for every route this phase uses.

Then run the whole-core checks once on the final source, from the repository root
with Rust 1.85.1, locked and offline, CARGO_BUILD_JOBS=2,
LAYERFS_CONSTRUCTION_WORKERS=1, CARGO_TARGET_DIR=$PWD/core/target:

  cargo +1.85.1 test   --manifest-path core/Cargo.toml --workspace --locked --offline
  cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings
  cargo +1.85.1 fmt    --manifest-path core/Cargo.toml --all -- --check
  python3 core/tools/check_product_boundary.py
  python3 -m unittest discover -s core/tools -p 'test_*.py'

Work the way this phase works. Run one attempt per registered selection into its
own output directory under core/target/pair1-evidence/, build the Linux test
binaries in the long-lived container described in 53 section 5, and compare
against a build of the parent commit in its own container target directory so a
pre-existing failure is never reported as a regression or the reverse. Keep
every registered numeric limit, one construction worker, the host's single
primary remote admission and the existing failure ownership. Make a failing
selection pass by fixing the product it registers; when a registered case
contradicts the operation it registers, bring the owner the trace and a
recommendation, as this round did for rename and for the remote admission. Add
tests that verify new behaviour end to end rather than restating the
implementation, and report FAIL, INCOMPLETE and unrun work as plainly as PASS.

Finish with the closure report the owner needs to close #179: which of 51
section 7's boxes are now closed and which are not, the exact commands and
receipt paths, the per-commit LOC comparison, the accepted bounds and deferred
work stated separately from the open failures, and the two items that need the
owner's decision or action — the push/merge and the issue closure itself, and
the Linux clippy component. Request those two authorizations with the completed
checklist attached; they are the last step before #179 can be closed.
```
