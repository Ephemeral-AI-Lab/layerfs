# Test speed-ups: make a fix cost a fix

> **Status:** Current general guide.

How to keep checking a change proportional to the change. It adds no new rule
and waives none: the 120 s ceiling, one attempt per operation and
[checks and completion](../../engine/AGENTS.md#checks-and-completion) still
apply, and the covering checks still run once at final identity.

## 1. Two kinds of run

- **Loop run:** after each fix, run the tests that the fix can affect (§2), on
  the platforms and features it can affect (§3). Record the selection and why.
- **Final run:** once, at final identity, the full covering checks on every
  platform, feature set and suite the change needs. Never after each fix.

If a loop selection misses something, the final run catches it at the cost of
one more loop. That is cheaper than a full run after every fix.

## 2. Select tests from the diff

1. List changed files: `git diff --name-only <merge-base>..HEAD`.
2. Map files to workspace packages, then add the packages that depend on them:
   `cargo +1.85.1 tree --locked -i <package> -e normal,dev --prefix none`,
   keeping workspace packages only.
3. Inside each dependent package, grep the changed functions and types for
   callers to choose its targets. For example, a change to `home::detect`
   reaches `layerfs-sandbox` only through its image targets, so only those run.
4. Run the chosen test targets. While fixing, run one test by name
   (`--test <target> -- <name>`); the recheck runs the whole target.

By change kind:

| Change | Loop run |
| --- | --- |
| Product source | §2 selection, §3 variants |
| Test only | the changed tests |
| Tooling (`tools/`, generators) | the owning `tools/test_*.py` and the generators' `--check` |
| Spec or `calls.json` | spec rebuild with no diff, generators `--check`, contract and calls_table targets |
| Docs only | links, anchors, status banner, claims; say why Rust checks do not apply |

## 3. Run a variant only when the diff reaches it

| Variant | Run it in a loop only when the diff touches |
| --- | --- |
| Linux | `#![cfg(target_os = "linux")]` code, daemon or FUSE, the Sandbox runtime, Store locking or process paths, or the Linux bundle |
| `--features machine` | code under `cfg(feature = "machine")`, or a target that is empty without it (`tools/ci_tests.py featured`) |
| PostgreSQL targets | PostgreSQL Store code or the PostgreSQL calls |
| Live tests (`LAYERFS_DAEMON`) | daemon, Sandbox, mount, remove or intent paths |
| Privileged FUSE mount targets | daemon or control mount code |
| Node, MCP and Python suites | the bindings, `calls.json` or their generators (the `--check`s are the cheap gate) |
| Clippy | each changed package, with the features it has |

## 4. Run the selection cheaply

- **One command per package and feature set, not per target.** Pass several
  `--test <target>` flags to one `cargo test -p <package>` (or one
  `tools/worktree.py cargo` call, which refuses workspace aggregates). Split a
  group so each command stays under 120 s; the `SLOW` list in
  [tools/ci_tests.py](../../tools/ci_tests.py) names the long targets.
- **Build once with `--no-run`** for the whole selection on each platform, so
  dependencies compile once, then run.
- **Skip targets that compile to nothing on the platform.** `ci_tests.py`
  detects the Linux-only marker; leave those targets out on macOS instead of
  launching them.
- **cargo-nextest is optional.** CI pins it (`.github/workflows/ci.yml`) and
  [.config/nextest.toml](../../.config/nextest.toml) stops a test at 120 s;
  `ci_tests.py filter` emits filtersets. It runs test binaries in parallel. Use
  the CI pin if it is installed; installing it is the owner's call.

## 5. Keep caches warm

- **One private target per worktree for the whole round** (the
  [worktree launcher](parallel-worktrees.md) registers it). Release it at
  hand-off, not between loops: a cold Linux target costs a full rebuild.
- **On Docker Desktop, removing a volume frees guest space, not host space.**
  Its VM disk does not shrink, so releasing a warm target volume mid-round does
  not help host free space. Release one mid-round only for Docker guest headroom.
- **After an unclean host restart,** look for zero-length or stale files under
  `target/*/incremental` and delete only the affected session directories
  rather than the whole incremental cache.
- **Reuse one debug pin per tip** for live tests; build a new one only when the
  product source changes.

## 6. Order the loop

- **Review before merge, in parallel with the loop run,** on the package
  branch. Merge when both pass. A review after the merge turns each finding
  into a whole extra loop.
- **One review round, one fix package.** Collect every finding of a round and
  fix them together.
- **Rebuild a runtime only when its inputs changed.** A test, tool or docs
  change needs none.
- **Keep the smoke as a numbered script** that grows by one step per fix;
  rerun it rather than rewriting it.

## 7. Observed costs

Local observation from the 2.0 call-surface rollout (2026-11-19, macOS host
plus the Linux build container); raw logs are not kept in the repository.

| Run | Observed |
| --- | --- |
| Full check batch, one command per target | macOS: 765 test commands in 38 min; 647 ended in ≤2 s and added up to about 15 min, mostly start-up; 172 ran zero tests; 170 were `--features machine` repeats. Linux: 484 commands, 47 with zero tests, 82 repeats, 15 min of package builds |
| Targeted recheck after a small fix | about 22 min with a cold Linux target |
| Release runtime build | about 13 s on macOS plus about 70 s for the Linux bundle, warm |

Most of the time went to running the full batch after small fixes, to
per-target start-up, and to cold rebuilds. Compiling the runtime was a small part.
