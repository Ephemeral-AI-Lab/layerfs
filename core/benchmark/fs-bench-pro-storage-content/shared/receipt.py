#!/usr/bin/env python3
"""`layerfs-core-receipt-v1`: identity, append-only writing and the budget.

Three rules live here, and each one exists because its absence produced a defect
somewhere in this tree already.

**Append-only.** A receipt, report or failed attempt is never overwritten. A rerun
that overwrites the evidence destroys the only witness there is, so
[`write_append_only`] creates with `O_EXCL` and fails on an existing path.

**Identity before number.** Every receipt names the tree it ran on: the source
commit and its dirty flag, the harness binary's hash, both lockfiles' hashes, the
generated registry table's hash, and the exported worker count. A rebuilt artifact
invalidates its matched arm, so an identity that is not recorded cannot be checked
later.

**Budget.** The complete command is `<= 15 s`; a declared exception is `<= 25 s`;
verification is `<= 60 s`. A selection that cannot fit is recorded `NOT_RUN` with
its measured wall time — never made to fit by moving work outside the timer,
enlarging a timeout or shrinking the workload.
"""

from __future__ import annotations

import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

SCHEMA = "layerfs-core-receipt-v1"
COMPLETE_COMMAND_LIMIT_NS = 15_000_000_000
DECLARED_EXCEPTION_LIMIT_NS = 25_000_000_000
VERIFICATION_LIMIT_NS = 60_000_000_000


class ReceiptError(Exception):
    """The receipt contract was violated."""


def sha256_file(path: str | Path) -> str:
    """Streaming SHA-256 of a file."""
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def sha256_bytes(payload: bytes) -> str:
    """SHA-256 of a byte string."""
    return hashlib.sha256(payload).hexdigest()


def git(repo_root: str | Path, *arguments: str) -> str:
    """Runs one git command in `repo_root` and returns its stripped stdout."""
    result = subprocess.run(
        ["git", *arguments],
        cwd=str(repo_root),
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        return ""
    return result.stdout.strip()


@dataclass
class Identity:
    """Everything a receipt must name before it states a number."""

    source_commit: str
    source_dirty: bool
    source_dirty_files: list[str]
    harness_root: str
    harness_binary: str | None
    harness_binary_sha256: str | None
    product_lock_sha256: str
    harness_lock_sha256: str
    registry_tsv_sha256: str
    construction_workers: str
    host: str
    platform: str
    python: str
    started_utc: str

    def as_fields(self) -> dict[str, object]:
        return {
            "source_commit": self.source_commit,
            "source_dirty": self.source_dirty,
            "source_dirty_files": self.source_dirty_files,
            "harness_root": self.harness_root,
            "harness_binary": self.harness_binary,
            "harness_binary_sha256": self.harness_binary_sha256,
            "product_lock_sha256": self.product_lock_sha256,
            "harness_lock_sha256": self.harness_lock_sha256,
            "registry_tsv_sha256": self.registry_tsv_sha256,
            "construction_workers": self.construction_workers,
            "host": self.host,
            "platform": self.platform,
            "python": self.python,
            "started_utc": self.started_utc,
        }


def identify(
    repo_root: str | Path,
    harness_root: str | Path,
    binary: str | Path | None,
) -> Identity:
    """Collects the identity of the tree and the binary a run will use."""
    repo_root = Path(repo_root)
    harness_root = Path(harness_root)
    dirty_files = git(repo_root, "status", "--porcelain").splitlines()
    binary_path = Path(binary) if binary else None
    return Identity(
        source_commit=git(repo_root, "rev-parse", "HEAD"),
        source_dirty=bool(dirty_files),
        source_dirty_files=dirty_files[:50],
        harness_root=str(harness_root),
        harness_binary=str(binary_path) if binary_path else None,
        harness_binary_sha256=(
            sha256_file(binary_path) if binary_path and binary_path.exists() else None
        ),
        product_lock_sha256=sha256_file(repo_root / "core" / "Cargo.lock"),
        harness_lock_sha256=sha256_file(harness_root / "Cargo.lock"),
        registry_tsv_sha256=sha256_file(harness_root / "tests" / "golden" / "registry.tsv"),
        construction_workers=os.environ.get("LAYERFS_CONSTRUCTION_WORKERS", ""),
        host=platform.node(),
        platform=platform.platform(),
        python=platform.python_version(),
        started_utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    )


def write_append_only(path: str | Path, payload: object) -> Path:
    """Writes JSON to a fresh path, refusing to overwrite anything.

    The write goes to a temporary file in the same directory and is then linked
    into place with `os.link`, so a reader never sees a half-written receipt and a
    second writer cannot claim the same name.
    """
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        raise ReceiptError(f"{path} already exists; receipts are append-only")
    temporary = path.with_name(f".{path.name}.partial")
    with open(temporary, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, sort_keys=True)
        handle.write("\n")
    try:
        os.link(temporary, path)
    except FileExistsError as error:
        raise ReceiptError(f"{path} already exists; receipts are append-only") from error
    finally:
        temporary.unlink(missing_ok=True)
    return path


def write_text_append_only(path: str | Path, text: str) -> Path:
    """The same discipline for a rendered report."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        raise ReceiptError(f"{path} already exists; reports are append-only")
    temporary = path.with_name(f".{path.name}.partial")
    temporary.write_text(text, encoding="utf-8")
    try:
        os.link(temporary, path)
    except FileExistsError as error:
        raise ReceiptError(f"{path} already exists; reports are append-only") from error
    finally:
        temporary.unlink(missing_ok=True)
    return path


@dataclass
class Budget:
    """The complete-command budget, as a measured outcome."""

    wall_ns: int
    declared_exception: bool
    limit_ns: int
    status: str
    reason: str

    def as_fields(self) -> dict[str, object]:
        return {
            "wall_ns": self.wall_ns,
            "limit_ns": self.limit_ns,
            "declared_exception": self.declared_exception,
            "status": self.status,
            "reason": self.reason,
        }


def budget(wall_ns: int, declared_exception: bool = False) -> Budget:
    """Classifies one complete-command wall time.

    A miss is `NOT_RUN` with the measured wall time and the reason — never a
    relaxed limit and never a shrunk workload.
    """
    limit = DECLARED_EXCEPTION_LIMIT_NS if declared_exception else COMPLETE_COMMAND_LIMIT_NS
    if wall_ns <= limit:
        return Budget(
            wall_ns, declared_exception, limit, "PASS",
            f"{wall_ns / 1e9:.3f} s within the {'declared exception' if declared_exception else 'complete-command'} limit",
        )
    return Budget(
        wall_ns, declared_exception, limit, "NOT_RUN",
        f"{wall_ns / 1e9:.3f} s exceeds the {limit / 1e9:.0f} s "
        f"{'declared exception' if declared_exception else 'complete-command'} limit; "
        "the row is recorded NOT_RUN with this measured wall time and is never shrunk to fit",
    )


def verification_budget(wall_ns: int) -> Budget:
    """Classifies one verification wall time against its own 60 s limit."""
    if wall_ns <= VERIFICATION_LIMIT_NS:
        return Budget(wall_ns, False, VERIFICATION_LIMIT_NS, "PASS", f"{wall_ns / 1e9:.3f} s")
    return Budget(
        wall_ns, False, VERIFICATION_LIMIT_NS, "INCOMPLETE",
        f"{wall_ns / 1e9:.3f} s exceeds the 60 s verification limit",
    )


def measurement_lock(lock_path: str | Path):
    """A context manager holding the measurement lock.

    Resource-sensitive work never overlaps, and another owner's run is never
    interrupted. The lock is an `O_EXCL` file holding the holder's pid and start
    time; a stale lock is reported, not silently stolen.
    """
    import contextlib

    @contextlib.contextmanager
    def held(path: str | Path):
        path = Path(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        try:
            descriptor = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        except FileExistsError as error:
            holder = path.read_text(encoding="utf-8", errors="replace").strip()
            raise ReceiptError(
                f"the measurement lock {path} is held ({holder}); "
                "resource-sensitive work never overlaps"
            ) from error
        try:
            os.write(
                descriptor,
                f"pid={os.getpid()} since={time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}\n".encode(),
            )
            os.close(descriptor)
            yield path
        finally:
            path.unlink(missing_ok=True)

    return held(lock_path)


def self_check() -> list[str]:
    """Proves append-only and the three budget limits behave."""
    import tempfile

    failures: list[str] = []
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "receipt.json"
        write_append_only(path, {"a": 1})
        if not path.exists():
            failures.append("write_append_only did not create the receipt")
        try:
            write_append_only(path, {"a": 2})
            failures.append("write_append_only overwrote an existing receipt")
        except ReceiptError:
            pass
        if json.loads(path.read_text())["a"] != 1:
            failures.append("the first receipt was mutated by the second write")
        lock = Path(directory) / "measurement.lock"
        with measurement_lock(lock):
            try:
                with measurement_lock(lock):
                    failures.append("the measurement lock was taken twice")
            except ReceiptError:
                pass
        if lock.exists():
            failures.append("the measurement lock was not released")
    if budget(14_000_000_000).status != "PASS":
        failures.append("a 14 s command was not within the complete-command limit")
    if budget(20_000_000_000).status != "NOT_RUN":
        failures.append("a 20 s undeclared command was not NOT_RUN")
    if budget(20_000_000_000, declared_exception=True).status != "PASS":
        failures.append("a 20 s declared exception was not within its limit")
    if budget(26_000_000_000, declared_exception=True).status != "NOT_RUN":
        failures.append("a 26 s declared exception was not NOT_RUN")
    if verification_budget(59_000_000_000).status != "PASS":
        failures.append("a 59 s verification was not within its limit")
    if verification_budget(61_000_000_000).status != "INCOMPLETE":
        failures.append("a 61 s verification was not INCOMPLETE")
    return failures


def main() -> int:
    failures = self_check()
    for failure in failures:
        print(f"receipt: FAIL: {failure}", file=sys.stderr)
    if failures:
        return 1
    print("receipt: PASS (append-only honoured, 15/25/60 s budgets classified)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
