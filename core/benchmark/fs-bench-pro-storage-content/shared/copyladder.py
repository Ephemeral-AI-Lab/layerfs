#!/usr/bin/env python3
"""The copy ladder R0-R3 and the ENOSPC preflight.

| Rung | Method token | What it is |
| --- | --- | --- |
| R0 | `read-only-master-mmap-no-copy` | the master itself, read-only, no copy |
| R1 | `apfs-clonefile-cow-v1` | `clonefile(2)`: copy-on-write, blocks shared |
| R2 | `closed-quiescent-byte-copy` | an independent byte copy |
| R3 | `regenerated-in-process` | rebuilt by the child from its recipe |

**R1 is forbidden wherever `st_blocks` gates.** A COW clone's allocated blocks are
shared with its master, so `store_allocated_bytes = st_blocks * 512` double-counts
and the O6 gate becomes meaningless. The repository says so twice — *"Copies/APFS
clones are not allocation controls"* and *"APFS clones/copies preserve content, not
allocation equivalence"* — so this module refuses the combination rather than
warning about it. Logical fields (`page_count`, `freelist_count`,
`pack_bodies_bytes`, `store_apparent_bytes`) remain valid on a clone.

`--setup clone` keeps its v0.1.6 byte-copy meaning; `--setup reflink` and
`--setup auto` are the new spellings, and `auto` never silently upgrades to R1 for
a family that gates allocated bytes.
"""

from __future__ import annotations

import ctypes
import os
import shutil
import sys
from dataclasses import dataclass
from pathlib import Path

R0 = "read-only-master-mmap-no-copy"
R1 = "apfs-clonefile-cow-v1"
R2 = "closed-quiescent-byte-copy"
R3 = "regenerated-in-process"

RUNGS = (R0, R1, R2, R3)

# Setup spellings accepted on the command line, and the rung each means.
SETUP_TOKENS = {
    "clone": R2,
    "copy": R2,
    "reflink": R1,
    "master": R0,
    "fresh": R3,
    "regenerate": R3,
}

# Families and cells where allocated bytes gate, so R1 is refused.
ALLOCATION_GATED_FAMILIES = ("c2.footprint",)

ENOSPC_FACTOR = 1.05

_libc = ctypes.CDLL(None, use_errno=True)
if hasattr(_libc, "clonefile"):
    _libc.clonefile.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_int]
    _libc.clonefile.restype = ctypes.c_int


class LadderError(Exception):
    """The requested rung is not allowed here, or could not be taken."""


def rung_for_setup(token: str, family: str, gates_allocated_bytes: bool) -> str:
    """Maps a `--setup` token to a rung, refusing R1 where it is forbidden."""
    if token not in SETUP_TOKENS:
        raise LadderError(f"unknown setup {token!r}; expected one of {sorted(SETUP_TOKENS)}")
    rung = SETUP_TOKENS[token]
    if rung == R1 and (gates_allocated_bytes or family in ALLOCATION_GATED_FAMILIES):
        raise LadderError(
            f"{family}: reflink (R1) is forbidden for a row that gates allocated bytes: "
            "a COW clone's st_blocks double-counts blocks shared with the master"
        )
    return rung


def free_bytes(path: str | Path) -> int:
    """Free bytes on the filesystem holding `path`."""
    stats = os.statvfs(path)
    return stats.f_bavail * stats.f_frsize


def preflight_enospc(path: str | Path, master_logical_bytes: int, factor: float = ENOSPC_FACTOR) -> int:
    """Requires `free_bytes >= factor * master_logical_bytes`.

    Raises rather than proceeding: a copy that runs out of space mid-way leaves a
    truncated artifact, and a truncated artifact measured as if it were whole is
    worse than a row that never ran.
    """
    needed = int(master_logical_bytes * factor)
    available = free_bytes(path)
    if available < needed:
        raise LadderError(
            f"ENOSPC preflight: {available} bytes free, {needed} required "
            f"({factor} x {master_logical_bytes}); refusing to copy"
        )
    return available


@dataclass(frozen=True)
class Acquisition:
    """One rung taken, with everything the receipt must declare."""

    rung: str
    master: str
    destination: str
    master_bytes: int
    destination_bytes: int
    free_bytes_before: int
    copy_wall_ns: int
    attribution: str

    def as_fields(self) -> dict[str, object]:
        return {
            "clone_method": self.rung,
            "master_path_released_to_sample": False,
            "master_logical_bytes": self.master_bytes,
            "acquired_logical_bytes": self.destination_bytes,
            "free_bytes_before": self.free_bytes_before,
            "acquisition_wall_ns": self.copy_wall_ns,
            "allocation_attribution": self.attribution,
        }


def acquire(
    rung: str,
    master: str | Path,
    destination: str | Path,
    gates_allocated_bytes: bool = False,
    family: str = "",
) -> Acquisition:
    """Takes one rung from `master` to `destination`.

    The master is never released to a sample process and never written: R0 returns
    the master path itself for a read-only open, and R1/R2 materialise an
    independent path.
    """
    import time

    master = Path(master)
    destination = Path(destination)
    if rung not in RUNGS:
        raise LadderError(f"unknown rung {rung!r}")
    if rung == R1 and (gates_allocated_bytes or family in ALLOCATION_GATED_FAMILIES):
        raise LadderError(f"{family}: R1 is forbidden where allocated bytes gate")
    if not master.exists():
        raise LadderError(f"master {master} does not exist")
    master_bytes = master.stat().st_size
    available = preflight_enospc(destination.parent, master_bytes)
    destination.parent.mkdir(parents=True, exist_ok=True)
    started = time.monotonic_ns()
    if rung == R0:
        attribution = "shared-with-master"
    elif rung == R1:
        _clonefile(master, destination)
        attribution = "shared-with-master"
    elif rung == R2:
        shutil.copyfile(master, destination)
        attribution = "exclusive"
    else:
        raise LadderError(
            "R3 is regenerated by the child from its recipe; it is not a copy step"
        )
    elapsed = time.monotonic_ns() - started
    return Acquisition(
        rung=rung,
        master=str(master),
        destination=str(destination),
        master_bytes=master_bytes,
        destination_bytes=destination.stat().st_size,
        free_bytes_before=available,
        copy_wall_ns=elapsed,
        attribution=attribution,
    )


def _clonefile(master: Path, destination: Path) -> None:
    """`clonefile(2)`, refusing rather than falling back.

    A platform without `clonefile` reports the limitation; it never silently
    substitutes a byte copy, because a receipt that says `apfs-clonefile-cow-v1`
    and took a different rung is a false declaration.
    """
    if not hasattr(_libc, "clonefile"):
        raise LadderError("clonefile(2) is unavailable on this host; R1 cannot be taken")
    status = _libc.clonefile(
        str(master).encode(), str(destination).encode(), 0
    )
    if status != 0:
        error = ctypes.get_errno()
        raise LadderError(f"clonefile({master} -> {destination}) failed: errno {error}")


def self_check() -> list[str]:
    """Proves the ladder's two hard rules and the preflight."""
    import tempfile

    failures: list[str] = []
    try:
        rung_for_setup("clone", "c1.edit.length-preserving", False)
        if rung_for_setup("clone", "x", False) != R2:
            failures.append("--setup clone did not mean the byte copy")
    except LadderError as error:
        failures.append(f"--setup clone was refused: {error}")
    try:
        rung_for_setup("reflink", "c2.footprint", True)
        failures.append("reflink was allowed for a row that gates allocated bytes")
    except LadderError:
        pass
    try:
        rung_for_setup("nonsense", "c1.edit.length-preserving", False)
        failures.append("an unknown setup token was accepted")
    except LadderError:
        pass
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        master = root / "master.bin"
        master.write_bytes(b"layerfs" * 4096)
        try:
            preflight_enospc(root, 1 << 62)
            failures.append("an impossible ENOSPC preflight passed")
        except LadderError:
            pass
        acquisition = acquire(R2, master, root / "copy.bin", False, "c1.edit")
        if acquisition.destination_bytes != acquisition.master_bytes:
            failures.append("the byte copy changed the artifact's length")
        target = root / "clone.bin"
        if hasattr(_libc, "clonefile"):
            cloned = acquire(R1, master, target, False, "c1.edit")
            if cloned.rung != R1:
                failures.append("the clone acquisition did not declare R1")
            if target.read_bytes() != master.read_bytes():
                failures.append("the clone does not carry the master's content")
        try:
            acquire(R1, master, root / "refused.bin", True, "c2.footprint")
            failures.append("acquire took R1 for an allocation-gated row")
        except LadderError:
            pass
    return failures


def main() -> int:
    failures = self_check()
    for failure in failures:
        print(f"copyladder: FAIL: {failure}", file=sys.stderr)
    if failures:
        return 1
    print("copyladder: PASS (R1 refused where st_blocks gates; ENOSPC preflight enforced)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
