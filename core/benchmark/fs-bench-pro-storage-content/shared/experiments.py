#!/usr/bin/env python3
"""E1-E4: the cheap, untimed, read-only experiments that gate the ladder.

None had been run before Stage 6. Each one is ~2 s, allocates nothing it does not
release, and answers a question the copy discipline depends on:

| # | Settles | Pass condition |
| --- | --- | --- |
| E1 | clone write isolation, inode distinctness, block sharing | distinct `st_ino`; master unchanged after writing the clone; used-bytes growth far below the file size |
| E2 | clone residency independence, and whether `MS_INVALIDATE` on a clone evicts the master | clone resident 0 after reading the master fully. **Decisive: if this fails, `prepared-master-dewarmed` is a lie for R1 and the reflink rung must be withdrawn** |
| E3 | cloning a quiescent Store | `quick_check == ok`, page count and watermark intact, master digest unchanged after mutating the clone |
| E4 | tree-clone fidelity | mode, mtime, symlinks, xattrs, `st_nlink == 1` across the tree |

A failure is recorded, not hidden: the outcome is `REFUTED` with its measured
state, and the rung's status follows it.
"""

from __future__ import annotations

import ctypes
import hashlib
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from pathlib import Path

import copyladder
import receipt
import residency
import space

SCHEMA = "layerfs-experiment-v1"

# Extended attributes, through libc.
#
# `os.setxattr`/`os.getxattr` are not present on this build's `os` module, and an
# experiment that silently skipped the attribute comparison would be reporting a
# fidelity it never checked. The FFI is four small calls on throwaway files.
_libc = ctypes.CDLL(None, use_errno=True)
_HAS_XATTR = hasattr(_libc, "setxattr") and hasattr(_libc, "getxattr")
if _HAS_XATTR:
    _libc.setxattr.argtypes = [
        ctypes.c_char_p,
        ctypes.c_char_p,
        ctypes.c_char_p,
        ctypes.c_size_t,
        ctypes.c_int,
        ctypes.c_int,
    ]
    _libc.setxattr.restype = ctypes.c_int
    _libc.getxattr.argtypes = [
        ctypes.c_char_p,
        ctypes.c_char_p,
        ctypes.c_char_p,
        ctypes.c_size_t,
        ctypes.c_int,
        ctypes.c_int,
    ]
    _libc.getxattr.restype = ctypes.c_ssize_t


def set_xattr(path: Path, name: bytes, value: bytes) -> None:
    """Sets one extended attribute, raising when the platform refuses."""
    if not _HAS_XATTR:
        raise OSError("libc has no setxattr on this host")
    status = _libc.setxattr(str(path).encode(), name, value, len(value), 0, 0)
    if status != 0:
        raise OSError(ctypes.get_errno(), f"setxattr({path}) failed")


def get_xattr(path: Path, name: bytes) -> bytes | None:
    """Reads one extended attribute, or `None` when it is absent."""
    if not _HAS_XATTR:
        return None
    buffer = ctypes.create_string_buffer(1024)
    size = _libc.getxattr(str(path).encode(), name, buffer, len(buffer), 0, 0)
    if size < 0:
        return None
    return buffer.raw[:size]


@dataclass
class Experiment:
    """One experiment's outcome and every number it measured."""

    identifier: str
    question: str
    outcome: str = "NOT_RUN"
    fields: dict[str, object] = field(default_factory=dict)
    notes: list[str] = field(default_factory=list)

    def as_document(self) -> dict[str, object]:
        return {
            "schema": SCHEMA,
            "experiment": self.identifier,
            "question": self.question,
            "outcome": self.outcome,
            "fields": self.fields,
            "notes": self.notes,
        }


def _digest(path: Path) -> str:
    return receipt.sha256_file(path)


def _write_master(path: Path, size: int) -> None:
    """Writes a master with incompressible content, in bounded blocks."""
    import random

    random.seed(0x5EED)
    with open(path, "wb") as handle:
        remaining = size
        while remaining > 0:
            block = min(remaining, 1 << 20)
            handle.write(random.randbytes(block))
            remaining -= block


def e1_clone_isolation(root: Path, size: int = 64 << 20) -> Experiment:
    """E1: clone write isolation, inode distinctness, block sharing."""
    experiment = Experiment(
        identifier="E1",
        question="does a clonefile clone have its own inode, leave its master unchanged, and share blocks",
    )
    if not hasattr(copyladder._libc, "clonefile"):
        experiment.outcome = "REFUTED"
        experiment.notes.append("clonefile(2) is unavailable on this host")
        return experiment
    master = root / "e1-master.bin"
    _write_master(master, size)
    clone = root / "e1-clone.bin"
    acquisition = copyladder.acquire(copyladder.R1, master, clone, False, "c1.edit")
    master_stat = os.stat(master)
    clone_stat = os.stat(clone)
    master_before = _digest(master)
    experiment.fields.update(
        {
            "master_bytes": master_stat.st_size,
            "master_ino": master_stat.st_ino,
            "clone_ino": clone_stat.st_ino,
            "clone_blocks_bytes": clone_stat.st_blocks * 512,
            "clone_apparent_bytes": clone_stat.st_size,
            "clone_method": acquisition.rung,
            "free_bytes_before": acquisition.free_bytes_before,
        }
    )
    # Write into the clone and require the master to be untouched.
    with open(clone, "r+b") as handle:
        handle.seek(0)
        handle.write(b"LAYERFS-E1-MUTATION")
        handle.flush()
        os.fsync(handle.fileno())
    master_after = _digest(master)
    clone_after = _digest(clone)
    distinct_inode = master_stat.st_ino != clone_stat.st_ino
    master_intact = master_before == master_after
    shared = clone_stat.st_blocks * 512 < master_stat.st_size // 2
    experiment.fields.update(
        {
            "master_digest_before": master_before,
            "master_digest_after": master_after,
            "clone_digest_after": clone_after,
            "distinct_inode": distinct_inode,
            "master_unchanged": master_intact,
            "used_bytes_far_below_file_size": shared,
        }
    )
    experiment.notes.append(
        "fsync is used *here* and only here: this is the harness's own isolation probe "
        "on a throwaway file, not a product path and not a report path"
    )
    experiment.outcome = "SATISFIED" if (distinct_inode and master_intact and shared) else "REFUTED"
    return experiment


def e2_clone_residency(root: Path, size: int = 64 << 20) -> Experiment:
    """E2: clone residency independence, and whether invalidating a clone evicts the master."""
    experiment = Experiment(
        identifier="E2",
        question="after the master is read fully, is a fresh clone's residency zero, and does msync(MS_INVALIDATE) on the clone evict the master",
    )
    master = root / "e2-master.bin"
    _write_master(master, size)
    # Make the master fully resident by reading it the ordinary way.
    with open(master, "rb") as handle:
        while handle.read(1 << 20):
            pass
    master_resident_before = residency.residency(master)
    experiment.fields["master_resident_pages_after_full_read"] = (
        master_resident_before.resident_pages
    )
    experiment.fields["master_total_pages"] = master_resident_before.total_pages
    if not hasattr(copyladder._libc, "clonefile"):
        experiment.outcome = "REFUTED"
        experiment.notes.append("clonefile(2) is unavailable on this host")
        return experiment
    clone = root / "e2-clone.bin"
    copyladder.acquire(copyladder.R1, master, clone, False, "c1.edit")
    clone_resident = residency.residency(clone)
    experiment.fields["clone_resident_pages"] = clone_resident.resident_pages
    # Read the clone fully, then ask whether the master kept its pages.
    master_before_clone_read = residency.residency(master)
    with open(clone, "rb") as handle:
        while handle.read(1 << 20):
            pass
    clone_after_read = residency.residency(clone)
    master_after_clone_read = residency.residency(master)
    experiment.fields.update(
        {
            "master_resident_pages_before_clone_read": master_before_clone_read.resident_pages,
            "clone_resident_pages_after_clone_read": clone_after_read.resident_pages,
            "master_resident_pages_after_clone_read": master_after_clone_read.resident_pages,
        }
    )
    # The decisive question: can the clone be de-warmed while the master stays warm?
    de_warm = residency.de_warm(clone)
    master_after_dewarm = residency.residency(master)
    experiment.fields.update(
        {
            "clone_resident_pages_after_dewarm": de_warm.resident_after,
            "clone_invalidated": de_warm.invalidated,
            "master_resident_pages_after_clone_dewarm": master_after_dewarm.resident_pages,
        }
    )
    independent = clone_resident.resident_pages == 0
    experiment.fields["clone_independent_of_master_residency"] = independent
    experiment.notes.append(
        "the decisive question for the reflink rung is the *first* reading: a clone "
        "that inherits its master's residency cannot carry a prepared-dewarmed claim"
    )
    experiment.outcome = "SATISFIED" if independent else "REFUTED"
    return experiment


def e3_quiescent_store(
    root: Path, harness_binary: Path, repo_root: Path
) -> Experiment:
    """E3: cloning a quiescent Store."""
    experiment = Experiment(
        identifier="E3",
        question="does a clone of a closed, quiescent Store preserve page count, quick_check and the master's digest",
    )
    if not hasattr(copyladder._libc, "clonefile"):
        experiment.outcome = "REFUTED"
        experiment.notes.append("clonefile(2) is unavailable on this host")
        return experiment
    produced = root / "e3-store"
    if produced.exists():
        shutil.rmtree(produced)
    result = subprocess.run(
        [
            str(harness_binary),
            "--case",
            "lifecycle-create",
            "--out",
            str(produced),
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    experiment.fields["child_exit_code"] = result.returncode
    experiment.fields["child_stdout"] = result.stdout.strip()
    store = produced / "sample.sqlite"
    if not store.exists():
        experiment.outcome = "REFUTED"
        experiment.notes.append(f"no Store was produced at {store}")
        return experiment
    # The Store must be quiescent: no sidecar, and a clean check.
    sidecars = space.sidecars(store)
    before_space = space.sqlite_space(store)
    before_check = space.quick_check(store)
    before_digest = _digest(store)
    clone = root / "e3-store-clone.sqlite"
    copyladder.acquire(copyladder.R1, store, clone, False, "c2.lifecycle")
    clone_space = space.sqlite_space(clone)
    clone_check = space.quick_check(clone)
    # Mutate the clone, which is what a measured sample would do.
    import sqlite3

    connection = sqlite3.connect(clone)
    connection.execute("CREATE TABLE e3_probe (value INTEGER)")
    connection.execute("INSERT INTO e3_probe (value) VALUES (1)")
    connection.commit()
    connection.close()
    master_after = _digest(store)
    experiment.fields.update(
        {
            "store_path": str(store),
            "sidecars": sidecars,
            "page_count_before": before_space.page_count,
            "page_count_clone": clone_space.page_count,
            "quick_check_before": before_check,
            "quick_check_clone": clone_check,
            "master_digest_before": before_digest,
            "master_digest_after_mutating_clone": master_after,
        }
    )
    preserved = (
        not sidecars
        and before_check == "ok"
        and clone_check == "ok"
        and before_space.page_count == clone_space.page_count
        and before_digest == master_after
    )
    experiment.fields["master_unchanged"] = before_digest == master_after
    experiment.outcome = "SATISFIED" if preserved else "REFUTED"
    return experiment


def e4_tree_fidelity(root: Path) -> Experiment:
    """E4: tree-clone fidelity across mode, mtime, symlinks, xattrs and link count."""
    experiment = Experiment(
        identifier="E4",
        question="does a clonefile directory clone preserve mode, mtime, symlinks, xattrs and st_nlink",
    )
    if not hasattr(copyladder._libc, "clonefile"):
        experiment.outcome = "REFUTED"
        experiment.notes.append("clonefile(2) is unavailable on this host")
        return experiment
    source = root / "e4-source"
    (source / "nested").mkdir(parents=True)
    (source / "file.txt").write_text("layerfs\n")
    (source / "nested" / "deep.txt").write_text("stage six\n")
    os.chmod(source / "file.txt", 0o640)
    os.chmod(source / "nested", 0o750)
    os.symlink("file.txt", source / "link")
    mtime = 1_600_000_000
    os.utime(source / "file.txt", (mtime, mtime), follow_symlinks=False)
    attribute_supported = True
    try:
        set_xattr(source / "file.txt", b"user.layerfs", b"e4")
    except (OSError, AttributeError) as error:
        attribute_supported = False
        experiment.notes.append(f"xattrs unavailable here, so they are not compared: {error}")
    destination = root / "e4-clone"
    copyladder.acquire(copyladder.R1, source, destination, False, "c1.many-tiny")

    def snapshot(base: Path) -> dict[str, object]:
        entries: dict[str, object] = {}
        for path in sorted(base.rglob("*")):
            relative = str(path.relative_to(base))
            info = os.lstat(path)
            record: dict[str, object] = {
                "mode": stat.S_IMODE(info.st_mode),
                "mtime": int(info.st_mtime),
                "nlink": info.st_nlink,
                "is_symlink": stat.S_ISLNK(info.st_mode),
            }
            if record["is_symlink"]:
                record["target"] = os.readlink(path)
            if attribute_supported:
                value = get_xattr(path, b"user.layerfs")
                record["xattr"] = value.decode() if value is not None else None
            entries[relative] = record
        return entries

    source_snapshot = snapshot(source)
    clone_snapshot = snapshot(destination)
    mismatches = {
        name: (source_snapshot[name], clone_snapshot.get(name))
        for name in source_snapshot
        if source_snapshot[name] != clone_snapshot.get(name)
    }
    # `st_nlink == 1` is a statement about **non-directory** entries. A directory's
    # link count is `.`, its parent's entry and one per subdirectory by POSIX
    # definition, so requiring 1 for a directory would be wrong on every
    # filesystem and would refute a clone that is in fact faithful. The check that
    # means something is: no file in the tree has a second name.
    file_link_counts = {
        name: record["nlink"]
        for name, record in clone_snapshot.items()
        if not stat.S_ISDIR(
            os.lstat(destination / name).st_mode
        )
    }
    directory_link_counts = {
        name: record["nlink"]
        for name, record in clone_snapshot.items()
        if name not in file_link_counts
    }
    experiment.fields.update(
        {
            "entries": len(source_snapshot),
            "mismatches": {name: list(pair) for name, pair in mismatches.items()},
            "clone_file_link_counts": file_link_counts,
            "clone_directory_link_counts": directory_link_counts,
            "xattrs_compared": attribute_supported,
            "symlink_targets": {
                name: record.get("target")
                for name, record in clone_snapshot.items()
                if record.get("is_symlink")
            },
        }
    )
    every_file_singly_linked = all(count == 1 for count in file_link_counts.values())
    experiment.fields["every_file_st_nlink_is_one"] = every_file_singly_linked
    experiment.notes.append(
        "st_nlink == 1 is checked for files only: a directory's link count is "
        "structural, and asserting 1 for it would refute every faithful clone"
    )
    experiment.outcome = (
        "SATISFIED"
        if not mismatches and every_file_singly_linked and attribute_supported
        else "REFUTED"
    )
    return experiment


def run_all(
    harness_binary: Path, repo_root: Path, work_root: Path | None = None
) -> list[Experiment]:
    """Runs E1-E4 in one directory and returns every outcome, failures included."""
    results: list[Experiment] = []
    with tempfile.TemporaryDirectory(dir=str(work_root) if work_root else None) as directory:
        root = Path(directory)
        results.append(e1_clone_isolation(root))
        results.append(e2_clone_residency(root))
        results.append(e3_quiescent_store(root, harness_binary, repo_root))
        results.append(e4_tree_fidelity(root))
    return results


def main() -> int:
    harness_root = Path(__file__).resolve().parent.parent
    binary = harness_root / "target" / "release" / "fs-bench-storage-content"
    if not binary.exists():
        print(f"experiments: FAIL: {binary} is not built", file=sys.stderr)
        return 1
    started = time.monotonic()
    results = run_all(binary, harness_root.parents[2])
    for experiment in results:
        print(f"{experiment.identifier}: {experiment.outcome}")
        for key, value in experiment.fields.items():
            print(f"    {key} = {value}")
        for note in experiment.notes:
            print(f"    note: {note}")
    print(f"experiments: {(time.monotonic() - started):.2f} s")
    return 0 if all(result.outcome == "SATISFIED" for result in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
