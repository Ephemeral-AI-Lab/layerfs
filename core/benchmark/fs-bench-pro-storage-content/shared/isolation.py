#!/usr/bin/env python3
"""Per-worktree isolation for builds and measurements.

Owner direction, 2026-09-21: builds and measurements no longer take a
machine-global lock.  Build/build and build/measurement pairs may therefore run
in parallel **across worktrees**, and what replaces the old mutual exclusion is
isolation rather than exclusion:

* every mutable root the harness owns resolves **inside its own worktree** —
  the measurement lock, the prepared masters, the run/receipt roots and the Cargo
  target directory.  None of them is shared with a sibling worktree, so two
  worktrees cannot collide on one;
* the Cargo **target directory** is checked before a build, because a shared
  `CARGO_TARGET_DIR` (inherited from a shell, or a `build.target-dir` in a
  user-level Cargo config) is the one realistic way two worktrees end up writing
  the same artifacts.  The check fails closed and names the offender;
* a small declared set of **read-only** inputs may live outside the worktree —
  today only the retained-history corpus.  They are read, hashed and never
  written; `self_check` proves the declaration still matches
  `history_corpus.DEFAULT_ROOT` so the two cannot drift apart;
* the one daemon-global name the harnesses use is the legacy sealed image tag,
  and it is content-addressed (`layerfs-bench-infra:<sha256 of sources>`), so two
  worktrees at the same content resolve the same image — a reuse, not a conflict —
  and two at different content resolve different tags. Nothing here namespaces
  it, and nothing needs to.

What this module deliberately does **not** claim: resource isolation.  Two
worktrees are still one host — one CPU, one disk, one page cache.  A build that
overlaps a timed phase perturbs that phase, and no lock is taken here to stop it.
`observe()` records the builders that were live at the instant a timed child
started, so a receipt says whether its own number was taken on a quiet host
rather than assuming it was.  A row whose observation names a competing builder
is a row whose number carries declared interference.

    python3 core/benchmark/fs-bench-pro-storage-content/shared/isolation.py
"""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

HARNESS_ROOT = Path(__file__).resolve().parent.parent

#: Inputs a run may read although they live outside the worktree.  Each is
#: read-only, hash-pinned by its own reader, and never written by the harness.
#: `self_check` asserts the corpus entry still equals `history_corpus.DEFAULT_ROOT`.
SHARED_READ_ONLY: tuple[str, ...] = (
    "/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data",
)

#: Executable names that mean "a build is running somewhere on this host".
BUILD_HEADS = (
    "cargo", "rustc", "clang", "cc", "gcc", "g++", "ld", "as", "swiftc",
    "make", "ninja", "cmake",
)

#: Executable names whose work is container-side rather than compiler-side.
CONTAINER_HEADS = ("docker", "buildx", "podman", "colima", "qemu-system-aarch64")

#: Command substrings that mean another measurement process is live.  A second
#: worktree's `perf` run is not a build, but it perturbs a timed phase exactly as
#: one does, so the observation names it too.
MEASUREMENT_HINTS = (
    "runner.py", "fs-bench-storage-content", "fs-benchmark-pro", "storage_smoke.py",
    "verify-selected.py",
)


class IsolationError(Exception):
    """A root that must belong to one worktree does not."""


@dataclass(frozen=True)
class Namespace:
    """The mutable roots one worktree owns, and nothing outside it."""

    worktree_root: Path
    harness_root: Path

    @property
    def label(self) -> str:
        """A short stable tag for this worktree: safe in a docker tag or a filename."""
        digest = hashlib.sha256(str(self.worktree_root).encode("utf-8")).hexdigest()
        return digest[:12]

    @property
    def lock_path(self) -> Path:
        """The measurement lock.  One per worktree, which is the whole change."""
        return self.harness_root / ".measurement.lock"

    @property
    def results_root(self) -> Path:
        return self.worktree_root / "benchmark-results" / "fs-bench-pro-storage-content"

    @property
    def artifact_root(self) -> Path:
        return self.results_root / "prepared"

    @property
    def build_target(self) -> Path:
        """The harness workspace's own target directory (its default)."""
        return self.harness_root / "target"

    def assert_owned(self, path: str | Path, what: str) -> Path:
        """Returns `path` resolved, refusing anything outside this worktree."""
        resolved = Path(path).expanduser().resolve()
        root = self.worktree_root.resolve()
        if resolved != root and root not in resolved.parents:
            raise IsolationError(
                f"{what} {resolved} is outside the worktree {root}; worktrees never "
                "share mutable harness state"
            )
        return resolved

    def as_fields(self) -> dict[str, object]:
        """The identity fields a receipt carries about its own isolation."""
        return {
            "scope": "per-worktree",
            "worktree_root": str(self.worktree_root),
            "harness_root": str(self.harness_root),
            "label": self.label,
            "lock_path": str(self.lock_path),
            "artifact_root": str(self.artifact_root),
            "build_target": str(self.build_target),
            "measurement_excludes": "other runs in this worktree only; other worktrees run freely",
            "shared_read_only": list(SHARED_READ_ONLY),
            "claim": (
                "state isolation, not resource isolation: overlapping work on this host "
                "still perturbs a timed phase, and that interference is recorded per row"
            ),
        }


def namespace(harness_root: str | Path = HARNESS_ROOT) -> Namespace:
    """The namespace of the worktree that owns `harness_root`."""
    harness = Path(harness_root).resolve()
    return Namespace(worktree_root=harness.parents[2], harness_root=harness)


def effective_target_directory(manifest: str | Path, cwd: str | Path | None = None) -> Path:
    """Cargo's own answer for where a build will write, config and env included."""
    result = subprocess.run(
        [
            "cargo", "+1.85.1", "metadata", "--no-deps", "--format-version", "1",
            "--manifest-path", str(manifest),
        ],
        capture_output=True, text=True, check=False, cwd=str(cwd) if cwd else None,
    )
    if result.returncode != 0:
        raise IsolationError(f"cargo metadata failed: {result.stderr.strip()[-2000:]}")
    try:
        return Path(json.loads(result.stdout)["target_directory"])
    except (KeyError, json.JSONDecodeError) as error:  # pragma: no cover - cargo contract
        raise IsolationError(f"cargo metadata carried no target_directory: {error}") from error


def assert_target_owned(
    space: Namespace, manifest: str | Path, cwd: str | Path | None = None
) -> Path:
    """Fails closed when a build would write artifacts another worktree also owns."""
    target = effective_target_directory(manifest, cwd)
    if target.resolve() == space.build_target.resolve() or space.worktree_root.resolve() in target.resolve().parents:
        return target
    raise IsolationError(
        f"the effective Cargo target directory {target} is outside the worktree "
        f"{space.worktree_root}; two worktrees sharing one target directory is exactly "
        "the build/build conflict this guard exists to refuse. Unset CARGO_TARGET_DIR "
        "or point it inside the worktree"
    )


def _process_table() -> list[tuple[int, int, str]]:
    """`(pid, ppid, command)` for every process, or an empty table off POSIX."""
    try:
        result = subprocess.run(
            ["ps", "-Ao", "pid=,ppid=,command="], capture_output=True, text=True, check=False
        )
    except OSError:  # pragma: no cover - non-POSIX host
        return []
    table: list[tuple[int, int, str]] = []
    for line in result.stdout.splitlines():
        fields = line.split(None, 2)
        if len(fields) == 3 and fields[0].isdigit() and fields[1].isdigit():
            table.append((int(fields[0]), int(fields[1]), fields[2]))
    return table


def _ancestry() -> set[int]:
    """Our own process and its parents, so `ps` never reports us as a competitor."""
    parents = {pid: ppid for pid, ppid, _ in _process_table()}
    lineage = {os.getpid()}
    current = os.getpid()
    while (parent := parents.get(current)) not in (None, 0, 1):
        lineage.add(parent)
        current = parent
    return lineage


def concurrent_work(exclude: set[int] | None = None) -> list[dict[str, object]]:
    """Build- and measurement-shaped processes live on this host, any worktree.

    A snapshot, not a proof of absence: work that starts after this call is not in
    the list, which is why a receipt treats an empty list as "none observed"
    rather than "the host was quiet".
    """
    skip = _ancestry() | (exclude or set())
    found: list[dict[str, object]] = []
    for pid, _, command in _process_table():
        if pid in skip or not command.split():
            continue
        head = Path(command.split()[0]).name
        if head in BUILD_HEADS:
            kind = "build"
        elif head in CONTAINER_HEADS:
            kind = "container"
        elif any(hint in command for hint in MEASUREMENT_HINTS):
            kind = "measurement"
        else:
            continue
        found.append({"pid": pid, "kind": kind, "command": command[:200]})
    return found


def observe(space: Namespace, manifest: str | Path | None = None) -> dict[str, object]:
    """The isolation block a receipt carries for one timed invocation."""
    fields = space.as_fields()
    fields["concurrent_work"] = concurrent_work()
    fields["observation"] = (
        "a `ps` snapshot taken immediately before the timed child started; an empty "
        "list means no competing work was observed, never that the host was quiet"
    )
    if manifest is not None:
        try:
            fields["build_target_effective"] = str(effective_target_directory(manifest))
        except IsolationError as error:  # an unreadable manifest is recorded, not fatal
            fields["build_target_effective"] = f"unavailable: {error}"
    return fields


def self_check() -> list[str]:
    """Proves the namespace is per-worktree and that an escape fails closed."""
    failures: list[str] = []
    one = namespace(Path("/tmp/isolation-check/a/core/benchmark/fs-bench-pro-storage-content"))
    two = namespace(Path("/tmp/isolation-check/b/core/benchmark/fs-bench-pro-storage-content"))
    if one.lock_path == two.lock_path:
        failures.append("two worktrees resolved one measurement lock")
    if one.label == two.label:
        failures.append("two worktrees resolved one docker namespace")
    if one.artifact_root == two.artifact_root:
        failures.append("two worktrees resolved one artifact root")
    for name, path in (("lock", one.lock_path), ("artifact", one.artifact_root)):
        try:
            one.assert_owned(path, name)
        except IsolationError:
            failures.append(f"a namespace refused its own {name} path {path}")
    for foreign in (two.worktree_root, Path("/tmp"), one.worktree_root.parent):
        try:
            one.assert_owned(foreign, "a foreign root")
            failures.append(f"a foreign root {foreign} was accepted as owned")
        except IsolationError:
            pass
    try:
        import history_corpus

        if history_corpus.DEFAULT_ROOT.as_posix() not in SHARED_READ_ONLY:
            failures.append(
                f"the shared-read-only declaration omits the corpus {history_corpus.DEFAULT_ROOT}"
            )
    except ImportError:  # pragma: no cover - only when run from another directory
        pass
    for entry in concurrent_work():
        if entry.get("kind") not in {"build", "container", "measurement"}:
            failures.append(f"a concurrent-work entry carried no known kind: {entry}")
    if not isinstance(observe(one).get("concurrent_work"), list):
        failures.append("observe() did not carry a concurrent-work list")
    return failures


def main() -> int:
    failures = self_check()
    for failure in failures:
        print(f"isolation: FAIL: {failure}", file=sys.stderr)
    if failures:
        return 1
    space = namespace()
    print(f"isolation: PASS ({space.as_fields()['label']} at {space.worktree_root})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
