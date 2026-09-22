# Issue 179 namespace implementation: third-round handoff

Written after the round that closed the eight registered selections
`53-issue179-namespace-completion-handoff.md` left open. The governing plan is
still `51-implementation-completion-spec.md`; this file does not replace it and
does not close the issue. It records what now passes, the cause and fix of each
selection, the four owner rulings this round used, and what is still not done.

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

## 6. Still not done

- The section 6 real-daemon small-project integration (two explicit Commits,
  public saved-state inspection, remount, clean close) is still not written, and
  the mounted FUSE syscall coverage for the six operations is still not a route.
- The architecture/operation matrix update in 51 section 7 is still open. This
  round changes an admission rule, the re-anchor rule and several ownership
  rules, so the affected documents need that update in a later commit.
- A *replaced* identity that is a base identity, not one this generation created,
  still loses its local record when a later Commit produces an empty successor
  root; its open handle then needs the service to resolve a name that is gone.
  The registered cases read such a handle before their Commit, so this is not
  covered, and it is the same class as the unbound case fixed here.
- The pinned Linux image has no clippy component, so a Linux clippy run still
  needs the owner's decision on installing one; the host clippy above is clean.
- `Service(Failure { code: Io })` on a long selection is a real transient
  transport outcome, not a product refusal, and deserves its own bounded check.

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
Finish #179's bounded Linux integration.

Use /Users/yifanxu/.codex/worktrees/795c/layerfs on codex/pair1-remote-mount.
HEAD is a7356d665, working tree clean, both product commits already made;
nothing is pushed. Preserve every existing worktree, receipt and historical
failure.

Read AGENTS.md and core/AGENTS.md, then, in this order:
  core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/51-implementation-completion-spec.md
  core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/54-issue179-namespace-closure-handoff.md

51 is still the governing owner-directed plan. 54 is the current state: all
sixteen registered namespace and mkdir selections pass, the stage route still
passes, and the whole-core checks are clean. Do not redo that round; start from
54 section 6.

Remaining work, in order: the section 6 real-daemon small-project integration
with two explicit Commits, public saved-state inspection, remount and clean
close; the mounted FUSE syscall coverage for the six namespace operations; the
architecture/operation matrix update for the admission, re-anchor and ownership
rules this round changed; and the replaced-base-identity record gap in 54
section 6. Keep every numeric limit, one construction worker, the host's single
primary remote admission and the existing failure ownership. Do not raise a
bound to make a line green, do not rewrite a registered case to make it pass,
and do not add a test that only restates the implementation.

Verify with namespace_route.py (10 cases), mkdir_route.py (6 cases) and the stage
route (10 cases) as the regression check, one attempt each and one output
directory each, building the Linux test binaries in the long-lived container
described in 53 section 5 and comparing against a build of the parent commit in
its own target directory. Run the whole-core checks once on the final source.
Commit each coherent step with the exact per-commit production LOC accounting
(tools/production_loc.py, parent and staged snapshots,
"Production LOC: <before> -> <after> (delta <signed>)"). Do not push, merge or
close the issue without authorization.
```
