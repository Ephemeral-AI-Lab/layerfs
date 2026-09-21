#!/usr/bin/env python3
"""Per-worktree isolation for the legacy benchmark harness.

Owner direction, 2026-09-21: the machine-global
`$TMPDIR/layerfs-infra-measurement.lock` is retired here. It keyed on the
machine, so a build in one worktree and a measurement in another — or two
builds, or two measurements — excluded each other for no reason that has
anything to do with their artifacts: every mutable root this harness owns already
resolves inside its own worktree.

What replaces it:

* **the lock is per worktree**, at `<worktree>/benchmark-results/host-store/
  .measurement.lock`. Two runs in *this* worktree still never overlap, because
  they would collide on the host store, the prepared masters and the image
  archive. Another worktree never contends with this one.
* **builds take no lock at all.** They write to this worktree's own target
  directory (`--target-dir` under `HOST_ROOT`, or the repo's own `target/`), and
  `assert_target_owned` refuses a target directory that escapes the worktree,
  which is the one way two worktrees could really share build state.
* **no docker name needs namespacing**: the sealed image tag is
  `layerfs-bench-infra:<sha256 of the sources>`, so two worktrees at the same
  content resolve one image (a reuse) and two at different content resolve
  different tags. The archive it feeds is per worktree already.

Resource isolation is *not* claimed: two worktrees are still one host, so a build
that overlaps a timed phase perturbs it. That interference is declared on the
receipt by `runner.observation`-style fields rather than excluded by a lock.

    python3 benchmark/fs-bench-pro/shared/isolation.py
"""

from __future__ import annotations

import hashlib
import sys
from pathlib import Path

HARNESS_ROOT = Path(__file__).resolve().parent
BENCH_ROOT = HARNESS_ROOT.parent
REPO = BENCH_ROOT.parent.parent

#: Every mutable root of the legacy harness, and the lock that serializes them
#: *within this worktree only*.
HOST_ROOT = REPO / "benchmark-results" / "host-store"
LOCK_PATH = HOST_ROOT / ".measurement.lock"

#: The retired machine-global path, kept only so a receipt can name what it
#: replaced and a reader can see the two are different files.
RETIRED_MACHINE_LOCK = "layerfs-infra-measurement.lock"


class IsolationError(Exception):
    """A path that must belong to this worktree does not."""


def label() -> str:
    """A short stable tag for this worktree: safe in a docker tag or filename."""
    return hashlib.sha256(str(REPO).encode("utf-8")).hexdigest()[:12]


def worktree_lock_path() -> Path:
    """The measurement lock this worktree owns and no other worktree contends for."""
    return LOCK_PATH


def assert_owned(path: str | Path, what: str) -> Path:
    """Returns `path` resolved, refusing anything outside this worktree."""
    resolved = Path(path).expanduser().resolve()
    root = REPO.resolve()
    if resolved != root and root not in resolved.parents:
        raise IsolationError(
            f"{what} {resolved} is outside the worktree {root}; worktrees never "
            "share mutable harness state"
        )
    return resolved


def assert_target_owned(path: str | Path, what: str = "the Cargo target directory") -> Path:
    """Refuses a target directory that another worktree could also be writing."""
    return assert_owned(path, what)


def as_fields() -> dict[str, object]:
    """The isolation fields a legacy receipt carries."""
    return {
        "scope": "per-worktree",
        "worktree_root": str(REPO),
        "lock_path": str(LOCK_PATH),
        "label": label(),
        "retired_machine_lock": RETIRED_MACHINE_LOCK,
        "measurement_excludes": "other runs in this worktree only; other worktrees run freely",
        "claim": (
            "state isolation, not resource isolation: overlapping work on this host "
            "still perturbs a timed phase"
        ),
    }


def self_check() -> list[str]:
    """Proves the lock is this worktree's and that an escape fails closed."""
    failures: list[str] = []
    if LOCK_PATH.name != ".measurement.lock" or RETIRED_MACHINE_LOCK in str(LOCK_PATH):
        failures.append(f"the worktree lock {LOCK_PATH} still names the machine-global file")
    try:
        assert_owned(LOCK_PATH, "the lock")
    except IsolationError:
        failures.append(f"the lock {LOCK_PATH} is not inside the worktree")
    for foreign in (Path("/tmp"), Path("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data")):
        try:
            assert_owned(foreign, "a foreign root")
            failures.append(f"a foreign root {foreign} was accepted as owned")
        except IsolationError:
            pass
    return failures


def main() -> int:
    failures = self_check()
    for failure in failures:
        print(f"isolation: FAIL: {failure}", file=sys.stderr)
    if failures:
        return 1
    print(f"isolation: PASS ({label()} at {REPO}, lock {LOCK_PATH})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
