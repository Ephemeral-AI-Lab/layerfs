# Build and measurement isolation across worktrees

> **Status:** Owner direction, 2026-09-21. Normative for how builds and
> measurements are gated. It supersedes the machine-global measurement lock for
> the worktree-parallel case and does not change any cache, sample, budget or
> append-only rule in [`benchmark_rules.md`](../../../general/benchmark_rules.md)
> or [`AGENTS.md`](../../../../AGENTS.md).

## The decision

Builds and measurements no longer exclude each other **across worktrees**, and
two builds never exclude each other at all. The machine-global
`$TMPDIR/layerfs-infra-measurement.lock` — held since v0.1.4 by every build, every
campaign script and every run — is retired. It keyed on the host, so a build in
one worktree failed with `another benchmark owns the measurement lock` while a
measurement ran in another, although the two share no mutable artifact.

What replaces it is **isolation, not exclusion**:

| Resource | Owner | Enforcement |
| --- | --- | --- |
| Measurement lock | one per worktree, `<harness>/.measurement.lock` | two runs in the *same* worktree still never overlap |
| Legacy measurement lock | one per worktree, `<worktree>/benchmark-results/host-store/.measurement.lock` | same |
| Prepared masters, run and receipt roots | the worktree that produced them | `isolation.assert_owned` refuses a path outside it |
| Cargo target directory | the worktree's own | `isolation.assert_target_owned` fails closed on a shared or foreign target |
| Docker image name | content-addressed (`layerfs-bench-infra:<sha256 of sources>`) | equal content resolves one image (a reuse); different content resolves different tags |
| Retained-history corpus, storage fixture data | declared read-only | read and hash-pinned, never written |

## What is not claimed

**State isolation is not resource isolation.** Two worktrees are one host: one
CPU set, one disk, one page cache. A build that overlaps a timed phase perturbs
that phase, and no lock here prevents it — that is the price of the parallelism
this direction asks for. The receipt therefore records what was live:

* every `perf` case receipt and every `verify` and run document carries a
  `resource_isolation` block: the namespace, its lock path, the effective build
  target, the declared read-only inputs, and `concurrent_work` — a `ps` snapshot
  taken immediately before the timed child started, classifying other work as
  `build`, `container` or `measurement`.
* an empty `concurrent_work` means **no competing work was observed**, never that
  the host was quiet. The observation is a snapshot, and the receipt says so.

A row whose observation names competing work is a row with **declared
interference**. It is not silently a clean number: an admission-grade campaign
still needs a quiet host, verified by that observation, or it must report the row
as diagnostic and say so. Nothing in this direction licenses a warm cache, a
pooled cache state or a re-run to get a quieter sample.

## Why the old lock was not doing what it looked like

The lock's path was derived from `TMPDIR`, which contains no repository or
worktree component, so every worktree and clone resolved one inode. It covered
builds (`--build-host`, `--build-image`, `--prune-builds`, `--prune-images`), the
legacy run entry point, and — through campaign scripts — preparation, packaging
and custody checks. Worktree isolation never entered the arbitration, and the
core harness's own lock was already per worktree, so the two halves of the
repository disagreed about what the lock meant.

## Implementation

* `core/benchmark/fs-bench-pro-storage-content/shared/isolation.py` — the
  canonical namespace: lock, artifact and result roots, the read-only
  declaration, the target guard, the concurrent-work observation, `self_check`.
* `benchmark/fs-bench-pro/shared/isolation.py` — the same namespace for the
  legacy tree, plus `HOST_ROOT` as its single source of truth.
* `runner.py` (core) routes `LOCK_PATH`, `RESULTS_ROOT` and `artifact_root()`
  through the namespace, checks the effective Cargo target before every build,
  and publishes `resource_isolation` on each receipt.
* The legacy tree's eight machine-global acquisitions now use
  `isolation.worktree_lock_path()`; the build branch takes **no** lock, while
  pruning still does, because deleting a master a running lane is about to load
  would corrupt that lane. The legacy host build also asserts that its target
  directory is inside the worktree.
* `shared/test_isolation.py` holds the semantics, including a **sealed scan**
  that fails if any harness source derives a lock from `TMPDIR` again.

## Verification performed

| Check | Command | Result |
| --- | --- | --- |
| Isolation unit + live-flock tests | `python3 core/benchmark/fs-bench-pro-storage-content/shared/test_isolation.py` | PASS, 19 tests |
| Core harness self-check (includes `isolation`) | `python3 core/benchmark/fs-bench-pro-storage-content/runner.py self-check` | PASS |
| Legacy namespace self-check | `python3 benchmark/fs-bench-pro/shared/isolation.py` | PASS |
| Target guard, no override | `assert_target_owned(namespace(), <harness Cargo.toml>)` | accepts the worktree's own `target/` |
| Target guard, shared target | `CARGO_TARGET_DIR=/tmp/shared-target …` | REFUSED, names the offender |
| Target guard, another worktree's target | `CARGO_TARGET_DIR=<other worktree>/core/target …` | REFUSED |
| Live two-worktree lock, real paths | holder in worktree A, probes from A and B | A's own lock **BLOCKED**; B's lock **ACQUIRED** |
| Legacy harness test sweep | 12 files run directly from `benchmark/fs-bench-pro/shared` | all OK (`test_build_reuse.py` 13 tests, two of them new); one pre-existing unrelated failure in `test_issue104_selection.py` (`validate_campaign` campaign-declaration mismatch, untouched by this change) |

Not run: an end-to-end legacy `--build-host` or measurement invocation under the
new lock. No measurement was made for this change, and no release, performance or
admission claim is made by it.

## Consequences for a future campaign

1. Build in as many worktrees as you like; keep each one's `CARGO_TARGET_DIR`
   inside its own worktree. The guard refuses anything else.
2. A measurement takes its own worktree's lock. It does not wait for, and is not
   waited on by, another worktree.
3. Check `resource_isolation.concurrent_work` before promoting a row. Competing
   work means declared interference, not a failed run and not a clean one.
4. Never restore a machine-global lock: the sealed scan in
   `shared/test_isolation.py` fails the harness self-check if one reappears.
