# Issue 179 namespace/attribute implementation: second-round handoff

Written after the round that fixed the namespace accounting behind the failures
`52-issue179-handoff.md` lists. The governing plan is still
`51-implementation-completion-spec.md`; this file does not replace it and does
not close the issue. It records what now passes, the exact remaining cause of
each failure, and the reasoning that was expensive to rediscover.

## 1. State in one paragraph

Commit `578ef7012` (branch `codex/pair1-remote-mount`, parent `96371122f`) raises
the registered selections that pass from four of sixteen to eight of sixteen.
Four of the ten namespace cases pass — `setattr`, `mknod`, `link`, `rmdir` — and
four of the six mkdir cases pass — `semantics`, `refusals`, `reserve_denied`,
`reserve_unknown`. Every other registered selection still fails, and section 4
names the statement that is wrong for each one. Nothing here is a performance,
durability or crash-recovery claim; no numeric limit was raised and no registered
selection was dropped.

## 2. Verified before and after

Both columns were produced the same way: the parent's source was exported with
`git archive`, built into its own container-local target directory, staged out,
and run through the same routes with its own output directory. The parent column
is not quoted from any earlier receipt.

| Selection | Parent `96371122f` | Now `578ef7012` |
| --- | --- | --- |
| `namespace/setattr` | FAIL | **PASS** |
| `namespace/mknod` | PASS | PASS |
| `namespace/link` | FAIL | **PASS** |
| `namespace/rmdir` | FAIL | **PASS** |
| `namespace/unlink`, `unlink_fresh`, `rename`, `rename_base`, `generation`, `notification_failure` | FAIL | FAIL |
| `mkdir/semantics` | PASS | PASS |
| `mkdir/refusals` | FAIL | **PASS** |
| `mkdir/reserve_denied`, `reserve_unknown` | PASS | PASS |
| `mkdir/capacity`, `successor` | FAIL | FAIL (pre-existing, not a regression) |

Every run used one attempt, its own staged output directory under
`core/target/pair1-evidence/NEW-UNUSED/`, and the pinned image
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`. The
fixture is `core/target/pair1-evidence/fixtures/large-edit-master-01/result.json`
and the host binaries are the `binary-archive/d582e04078…/host` set.

## 3. What changed

All of it is in `core/crates/layerfs-workspace`:

- **A link reuses an identity that already exists.** `filesystem/create.rs` ran
  the "a newly allocated serial must be absent from this overlay" guards for
  `Creation::Link`, counted a dirty key the reused serial already owned, and
  returned a freshly constructed file's defaults (size 0, mode 0777) instead of
  the target's kind, size and metadata. It also pushed the target's dirty key
  twice; `RootOwner::update` rejects a duplicate key as `Capacity`.
- **Re-anchoring ran without an anchor.** `create.rs`, `filesystem/remove.rs` and
  `filesystem/rename.rs` cleared the delta's entry and tombstone pages even when
  `MetadataHost::anchor(owner)` was `None`, silently dropping every name an
  earlier operation of the same generation had added to that directory. The reset
  now happens only with the reference that holds those pages.
- **One name owned both a binding and its removal.** A removed name stayed in the
  entry page while a removal record was added, so lowering emitted two records
  for one name and `prepared_directories` refused the delta. The new checked
  `overlay/directories.rs::has_entry`/`drop_entry` pair keeps a name in exactly
  one page, `backing/metadata_build.rs` accepts the entry-page key it rebuilds,
  and a directory with no origin to shadow needs no removal record at all.
- **The dirty frontier disagreed with the root's dirty keys.**
  `runtime/state.rs` now keeps the three quantities equal: an unlink counts only
  the parent, a link counts its target only when the generation has not marked it
  yet, and an identity this generation created that loses its last name leaves
  the frontier. A capture resets the counters, so that set survives the capture
  that lowering runs behind, and `commit/lower.rs` plus `commit/reconcile.rs`
  step over such an identity instead of returning a `Dirty::Unbound` that the
  captured counters never described.
- **`rename` double-counted its destination** and rewrote rows that already
  existed in the captured name accounting; a removed name now also releases the
  local lookup reference it owned.

`tests/namespace.rs::namespace_rename_base` was corrected: the attached fixture
already binds both `data.bin` and `alias` to one inode, so the second local name
that case adds must be one the base is free of. It now also asserts that
re-linking the fixture's own name is refused with `Exists`.

## 4. Exact remaining failures, with the statement that is wrong

Ordered by expected cost. Item 1 is diagnosed to the line; the rest name the
boundary that still needs a trace.

1. **`rename_base` — `list_view` ignores tombstones for names that come from the
   origin.** `filesystem/namespace_view.rs::list_view` filters tombstoned names
   while it walks the delta's own entry pages, but the block that merges the
   Service's `Inspect::List { path }` result inserts those names with no
   tombstone check. After `unlink(top, "data.bin")` the origin still lists
   `data.bin`, so `readdir` returns it and then re-resolves it through
   `resolve_child`, which correctly answers `NotFound`
   (`filesystem/directory.rs`, the `resolve_child` call after `list_view`). The
   fix belongs in that merge loop, where the window and `directory.tombstones`
   are already in scope. Expect it to fix silent wrong listings for every
   tombstoned base-bound name, not only this case.
2. **`unlink` — an open orphan must stay readable across Commit.** The test reads
   the unlinked inode through its still-open handle after Commit;
   `filesystem/read.rs` reaches `overlay_inode(serial, root)` first and only
   falls through to a Service lookup of the node's stale path when that returns
   `None`. `commit/reconcile.rs` currently drops the unbound identity's inode
   record from the successor root, so the record is gone and the fallback asks
   the Service for a path that no longer exists
   (`Service(Failure { code: PathNotFound })`). The successor root must carry
   that identity's record forward — emit it unchanged in the inode phase while
   still emitting no dirty key for it — and `Current::count` must keep agreeing
   with the walk while it does.
3. **`unlink_fresh` — `close_clean` refuses `Busy` inside metadata teardown.**
   Every condition `runtime/lifecycle.rs::close_clean_until` tests itself is
   clear (no submission, no dirty inode, not mounted, no active call, no handle,
   no lookup) and the two later refusals (external pins, remaining payloads) are
   not reached either, so the refusal comes from `MetadataHost::close`
   (`backing/metadata_reclaim.rs`), most likely its "a root still holds this
   arena" check. Two Commits have run by then; find which root is still alive.
4. **`generation` — a mutation during a retained G is refused `Busy`.** The case
   renames, sets attributes and unlinks while a stage is in flight and expects
   the later changes to survive G's completion. The refusal happens after the
   rename has resolved both parents and before it edits a delta, so it is one of
   the state checks that run there: `check_child_stamp` is the strongest
   candidate, because a capture advances `state.revision` while G is retained,
   and `check_mutation_coherence` is the other. Instrument those two rather than
   the frozen capture.
5. **`notification_failure` — a mounted rename returns `Err(Io)`.** The case
   expects the rename to publish and to report the notification result
   separately, and the mounted status to remain Coherence. Two candidates remain
   and neither is instrumented yet: the mounted-state check the operation runs
   before publication, and `complete_projection_mutation` afterwards.
6. **`rename` — the registered case contradicts the operation it registers.**
   `tests/namespace.rs` asserts `Err(WorkspaceError::NotFound)` for a NOREPLACE
   rename whose destination does not exist; the product returns `Ok(())`, which
   is the ordinary meaning of the flag. The later assertions in the same case
   depend on that rename *not* moving the file, so this needs an owner decision:
   either the expectation is corrected together with the assertions after it, or
   the semantics are changed deliberately. Do not make the rename fail to make
   the line green.
7. **`mkdir_successor` — its second Commit is refused with `InvalidInput`.** The
   declaration ledger must still declare a directory the first Commit did not
   reach, in the same generation.
8. **`mkdir_capacity` — a refused mkdir still consumes a reservation**
   (`tests/mkdir.rs:493`). The refusal is expected to leave the reservation count
   untouched.

## 5. What is expensive to rediscover

**The dirty frontier is a counter that must equal the keys, not a set derived
from them.** `Captured::count` is `State::dirty_inodes` at capture time, while
`next_dirty` walks the dirty keys the published root actually holds. Every
mutation must add exactly the keys it writes and nothing else, or the failure
appears far away as `StageFailure { phase: LocalBookkeeping, cause: Io }` from
`count != captured.count` in `commit/save.rs`. When a `LocalBookkeeping`/`Io`
appears, instrument that comparison first: it names the delta (`count=2
captured=3`) and is the fastest route to the offending operation.

**Two different byte quantities are in play.** `Directory::bytes` is the record's
**entry** page only — `Directory::parse` bounds it by `count * 11 ..= count * 265`
— while `Captured::name_bytes` must equal the sum over **entry and removal** rows
that `prepared_directories` emits. Dropping a binding shrinks the first whether
or not a removal record takes its place; the second only changes when a row
appears or disappears.

**A name may appear in exactly one of the two pages.** `prepared_directories`
sorts the emitted rows and refuses `pair[0].0 >= pair[1].0`, so an entry and a
removal record for one name are an `Io`, and `RootOwner::update` refuses
duplicate or unsorted keys with `Capacity` rather than a diagnosable error.

**`Origin::Empty` with tombstones is an invalid record.** `Directory::parse`
refuses it, so a fresh directory must express a removal by dropping the binding
and nothing else; only a delta with an origin to shadow needs a removal record.

**`capture()` clears the `fresh` ledger.** Any "this generation created it and no
name binds it now" predicate read during lowering must live outside that ledger.
This round gave `runtime/state.rs` an explicit `unbound` set that survives the
capture and is cleared with `declared` once the canonical successor is installed.

**The re-anchor reset is conditional on having a reference.** Moving a delta's
inherited pages into `Origin::Captured` is only valid when
`MetadataHost::anchor(owner)` is `Some`. Resetting the pages without it discards
them, because no other record holds those names.

**Build the test binary incrementally, in the container.** Section 5 of
`52-issue179-handoff.md` recommends `cp -a /work/core /tmp/s`, which copies the
18 GB of target directories and forces a full rebuild every round. Only the
source needs to move: keep one long-lived container with a container-local copy
of `core/{Cargo.toml,Cargo.lock,crates}` and a container-local target directory,
re-extract the source (a few MB) before each build, and `touch` the crate you
changed. That turns a round into roughly two seconds of build plus one and a half
seconds per selection.

```sh
docker run -d --name lfs-build --cpus=2 \
  -v "$PWD":/work -v "$HOME/.cargo/registry":/usr/local/cargo/registry \
  sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4 sleep infinity
COPYFILE_DISABLE=1 tar -cf - -C core Cargo.toml Cargo.lock crates \
  | docker exec -i lfs-build sh -c 'rm -rf /tmp/s && mkdir -p /tmp/s && tar -xf - -C /tmp/s'
docker exec lfs-build sh -c 'find /tmp/s/crates/layerfs-workspace -name "*.rs" -exec touch {} +;
  cd /tmp/s && export CARGO_TARGET_DIR=/tmp/tb CARGO_BUILD_JOBS=2 \
  RUSTFLAGS="--cfg aes_armv8 --cfg polyval_armv8 --cfg chacha20_force_neon -C target-feature=+aes,+sha2";
  cargo test --manifest-path Cargo.toml -p layerfs-workspace --test namespace --no-run --locked --offline'
docker cp lfs-build:/tmp/tb/debug/deps/namespace-dc60a93a3a48653b \
  core/target-linux/debug/deps/namespace-dc60a93a3a48653b
```

Stage the binary out before running a route: the route mounts its parent
directory read-only into the test container. The hash in the file name is the
target's, and it does not change when the source does.

**Compare against the parent commit, not a remembered baseline.** Export the
parent with `git archive HEAD core/{Cargo.toml,Cargo.lock,crates}`, build it into
its own target directory, and run both columns through the routes. It costs one
build and removes every "was this failing before?" question; it is how
`mkdir/capacity` and `mkdir/successor` were shown to be pre-existing failures
rather than regressions.

## 6. Accepted bounds and deferred work

Unchanged and not to be raised to obtain a PASS: 128 admission rules, 32 KiB
metadata, 256 Nodes, 128 handles, 4096 payload records, 8 MiB/256-edit replay,
1024 pieces, 8 MiB default memory budget, 4096-entry topology work bound, one
construction worker. Deferred as before: DSH, large workloads, R6, xattr/ACL/
atime/ctime, special files, rename exchange/whiteout, readdirplus, fallocate,
copy-file-range, crash recovery, durability.

Not started: mounted FUSE syscall coverage for the six operations, the
real-daemon small-project scenario in `core/crates/layerfs-daemon/tests/`,
whole-core final checks on frozen source, the architecture/operation matrix, and
the final report's split of accepted bounds versus open failures.

Tooling state: `cargo fmt --check` is clean. `cargo clippy -D warnings` reports
three findings that are present identically at `96371122f` and are untouched by
this round — `rename_from`'s argument count and two on the pre-existing
`name_page` in `overlay/directories.rs`. The pinned Linux image has no clippy
component, so a Linux clippy run needs the owner's decision on installing one;
`cargo test -p layerfs-workspace --lib` has no unit tests.

## 7. Production LOC comparison

| Scope | Before | After | Delta |
| --- | --- | --- | --- |
| Replacement core | 49,494 | 49,642 | +148 |
| Reference `crates/` | 65,417 | 65,417 | 0 |
| Combined | 114,911 | 115,059 | +148 |

Method: the unchanged `tools/production_loc.py`, no flags, on exact snapshots
(`git archive REV crates core/crates` into a root, then `--root ROOT --json`) of
the parent `96371122f` and the committed product tree `578ef7012`. The growth is
the accounting fixes and the new `has_entry`/`drop_entry` pair in
`layerfs-workspace`, not relocation or a scope change. This handoff's own
documentation commit changes no production line.

## 8. Copyable prompt for the next implementation agent

```text
Finish #179's bounded Linux namespace implementation and its integration.

Use /Users/yifanxu/.codex/worktrees/795c/layerfs on codex/pair1-remote-mount.
HEAD is 578ef7012, working tree clean, product commit already made; nothing is
pushed. Preserve every existing worktree, receipt and historical failure.

Read AGENTS.md and core/AGENTS.md, then, in this order:
  core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/51-implementation-completion-spec.md
  core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/53-issue179-namespace-completion-handoff.md

51 is still the governing owner-directed plan. 53 is the current state: eight of
sixteen registered selections pass, and each remaining failure has a named cause.
Do not redo the round 53 describes; start from its section 4.

Work the remaining failures in the order 53 lists them. Item 1 (`rename_base`)
is diagnosed to the line: list_view in
core/crates/layerfs-workspace/src/filesystem/namespace_view.rs merges the
origin's Inspect::List result without applying the delta's tombstones. Item 2
(`unlink`) needs commit/reconcile.rs to carry an unbound identity's inode record
into the successor root so an open orphan handle keeps reading locally. Items 3-8
each name the boundary that still needs a trace; trace it before changing
anything, and for item 6 (`rename`) get an owner decision rather than making the
line green.

Verify with the routes, one attempt each, one output directory each:
  core/crates/layerfs-workspace/tests/namespace_route.py  (10 cases)
  core/crates/layerfs-workspace/tests/mkdir_route.py      (6 cases)
Build the Linux test binary in the long-lived container described in 53 section
5 -- a few seconds per round -- and stage it into core/target-linux/debug/deps
before running a route. When a selection fails, re-run it with
--diagnostic-trace and read the staged test.stderr. Before claiming any fix,
build the parent commit into its own target directory and compare, so a
pre-existing failure is never reported as a regression or the reverse.

Keep every numeric limit, the one construction worker, and the existing failure
ownership. Do not rewrite a registered case to make it pass, do not raise a
bound, and do not add a test that only restates the implementation.

Keep heavy work serial, CARGO_BUILD_JOBS=2, container --cpus=2. Run
cargo fmt --check and the whole-core checks once on the final source: cargo
+1.85.1 test/clippy/fmt --manifest-path core/Cargo.toml --locked, plus
python3 core/tools/check_product_boundary.py. Start the fmt/clippy set from the
tree as it is; clippy already fails on three pre-existing findings named in 53
section 6, so decide with the owner whether to fix those or record them.

Commit each coherent step with the exact per-commit production LOC accounting
that AGENTS.md requires (tools/production_loc.py, parent and staged snapshots,
"Production LOC: <before> -> <after> (delta <signed>)"). Do not push, merge or
close the issue without authorization.
```
