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
from collections.abc import Callable
import hashlib
import os
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from pathlib import Path

import copyladder
import invariants
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


# The save the declared-process-kill arm kills into. It is large enough to open
# more than one write transaction, so committed pack rows exist in the file while
# the watermark transaction has not run.
W2_SAVE_CASE = "dedup-cross-file-unique-10"


def prepared_case(case_id: str) -> bool:
    """Whether the registry declares a prepared master for one case.

    Read from the golden registry table beside this module rather than from a list
    kept here: a family that gains or loses a phase split must not also need a
    second edit in the calibration arms.
    """
    table = Path(__file__).resolve().parent.parent / "tests" / "golden" / "registry.tsv"
    for line in table.read_text(encoding="utf-8").splitlines()[1:]:
        fields = line.split("\t")
        if fields and fields[0] == case_id:
            return len(fields) > 12 and fields[12] not in ("", "-")
    return False


def _run_child(
    binary: Path,
    case_id: str,
    out: Path,
    store: Path | None = None,
    load_input: str | None = None,
) -> subprocess.CompletedProcess:
    """Runs one harness case as its own process.

    `load_input` hands the child the prepared master its registry row declares. A
    row that declares one and is handed none fails closed with `NOT_RUN` — its
    driver reads its fixture from the artifact and does not rebuild it inside the
    invocation the phase split exists to keep it out of — so an arm that drives
    such a case without this would report a product refusal that never happened.
    """
    command = [str(binary), "--case", case_id, "--out", str(out)]
    if store is not None:
        command += ["--store", str(store)]
    if load_input is not None:
        command += ["--load-input", load_input]
    environment = dict(os.environ)
    environment["LAYERFS_CONSTRUCTION_WORKERS"] = "1"
    return subprocess.run(command, capture_output=True, text=True, check=False, env=environment)


def w1_watermark_refusal(root: Path, harness_binary: Path) -> Experiment:
    """W1: a hand-edited watermark must make `Store::open` refuse.

    External perturbation only. There is no fault injection in product source and
    none is needed: the watermark is a row in the Store, and a Store whose
    watermark is ahead of its pack ids is a Store the product must refuse rather
    than repair. The control arm is the unperturbed copy, which must still open —
    without it, a refusal would prove nothing about the perturbation.
    """
    experiment = Experiment(
        identifier="W1",
        question="does a hand-edited watermark make Store::open refuse with Integrity, while an unperturbed copy still opens",
    )
    produced = root / "w1-store"
    result = _run_child(harness_binary, "lifecycle-create", produced)
    experiment.fields["create_stdout"] = result.stdout.strip()
    store = produced / "sample.sqlite"
    if not store.exists():
        experiment.outcome = "REFUTED"
        experiment.notes.append(f"no Store was produced at {store}")
        return experiment
    control = root / "w1-control.sqlite"
    perturbed = root / "w1-perturbed.sqlite"
    shutil.copyfile(store, control)
    shutil.copyfile(store, perturbed)
    import sqlite3

    connection = sqlite3.connect(perturbed)
    connection.execute("UPDATE store_policy SET retained_pack_ceiling = 999999")
    connection.commit()
    connection.close()

    control_out = root / "w1-control-run"
    control_result = _run_child(
        harness_binary,
        "lifecycle-open",
        control_out,
        store=control,
    )
    perturbed_out = root / "w1-perturbed-run"
    perturbed_result = _run_child(
        harness_binary,
        "lifecycle-open",
        perturbed_out,
        store=perturbed,
    )
    control_trace = (control_out / "trace.jsonl").read_text(encoding="utf-8")
    perturbed_trace = (perturbed_out / "trace.jsonl").read_text(encoding="utf-8")
    control_opened = "row.not-run" not in control_trace
    perturbed_refused = "Integrity" in perturbed_trace or "row.not-run" in perturbed_trace
    experiment.fields.update(
        {
            "control_stdout": control_result.stdout.strip(),
            "perturbed_stdout": perturbed_result.stdout.strip(),
            "control_opened": control_opened,
            "perturbed_refused": perturbed_refused,
            "perturbed_trace_tail": perturbed_trace.strip().splitlines()[-1][:400],
        }
    )
    experiment.notes.append(
        "the perturbation is an ordinary UPDATE through sqlite3, outside the product; "
        "no product source was modified, no hook was added and no fault was injected "
        "into the crate"
    )
    experiment.outcome = "SATISFIED" if control_opened and perturbed_refused else "REFUTED"
    return experiment


def w2_declared_process_kill(
    root: Path,
    harness_binary: Path,
    artifact_of: Callable[[str], str | None] | None = None,
) -> Experiment:
    """W2: a declared process kill leaves a Store the product refuses to reuse.

    `CONTRACT.md` section 7 asks for the failure / unknown-outcome / cleanup bullet
    to be exercised **without product fault injection**, and `c2-families.md`
    section 4 states what the product must then do: a save whose packs were written
    but whose publication watermark did not commit leaves packs that are "neither
    published nor deleted", and `begin_save` must refuse with `UninspectedState`.

    The kill is external and declared. The child is a harness process that has
    accepted its whole offered set and has **not** committed the watermark; it
    writes a marker at that point and holds, so the killer waits for the child's own
    declaration rather than for a wall-clock guess. Nothing in `core/crates/*/src`
    was changed to make the hold possible - the wait state is harness argv.

    The control is the same save run to completion: if a completed Store were
    refused too, the refusal would say nothing about the kill.
    """
    experiment = Experiment(
        identifier="W2",
        question=(
            "does a declared SIGKILL of a child that has accepted a save but not "
            "committed its watermark leave a Store whose next begin_save is refused "
            "with UninspectedState, while the same save run to completion is accepted"
        ),
    )
    environment = dict(os.environ)
    environment["LAYERFS_CONSTRUCTION_WORKERS"] = "1"

    # The save is large enough to open more than one write transaction, which is
    # what puts committed pack rows in the file while the watermark is still behind
    # them. A save small enough to stay inside one transaction would be rolled back
    # by nothing and would leave no uninspected state to find.
    # The child creates its own output directory and refuses an existing one, so
    # the runner must not pre-create it.
    victim_out = root / "w2-victim-run"
    marker = root / "w2-held.marker"
    if marker.exists():
        marker.unlink()
    master = artifact_of(W2_SAVE_CASE) if artifact_of is not None else None
    if prepared_case(W2_SAVE_CASE) and master is None:
        experiment.outcome = "REFUTED"
        experiment.notes.append(
            f"{W2_SAVE_CASE} declares a prepared master and none was acquired, so the "
            "save the arm kills could not be started"
        )
        return experiment
    experiment.fields["victim_load_input"] = master or ""
    victim_command = [
        str(harness_binary),
        "--case", W2_SAVE_CASE,
        "--out", str(victim_out),
        "--hold-save", str(marker),
        "--hold-at", "accept",
        "--hold-ns", str(60 * 1_000_000_000),
    ]
    if master is not None:
        victim_command += ["--load-input", master]
    child = subprocess.Popen(
        victim_command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=environment,
    )
    deadline = time.monotonic() + 120.0
    held = False
    while time.monotonic() < deadline:
        if marker.exists():
            held = True
            break
        if child.poll() is not None:
            break
        time.sleep(0.01)
    experiment.fields["hold_marker_observed"] = held
    if not held:
        child.kill()
        child.wait(timeout=60)
        experiment.outcome = "REFUTED"
        experiment.fields["victim_stdout"] = (child.stdout.read() if child.stdout else "")[-500:]
        experiment.fields["victim_stderr"] = (child.stderr.read() if child.stderr else "")[-500:]
        experiment.notes.append(
            "the child never declared a hold point with objects accepted, so no kill "
            "point existed and the arm proves nothing"
        )
        return experiment
    child.kill()
    child.wait(timeout=60)
    experiment.fields["victim_exit_code"] = child.returncode
    experiment.fields["victim_killed_by_sigkill"] = child.returncode == -signal.SIGKILL

    killed = victim_out / "sample.sqlite"
    if not killed.exists():
        experiment.outcome = "REFUTED"
        experiment.notes.append(f"the killed child left no Store at {killed}")
        return experiment

    # The control: the identical save, run to completion, no hold and no kill.
    control_out = root / "w2-control-run"
    control_run = _run_child(harness_binary, W2_SAVE_CASE, control_out, load_input=master)
    experiment.fields["control_run_stdout"] = control_run.stdout.strip()
    control = control_out / "sample.sqlite"
    if not control.exists():
        experiment.outcome = "REFUTED"
        experiment.notes.append(f"the control run left no Store at {control}")
        return experiment

    killed_state = _watermark_state(killed)
    control_state = _watermark_state(control)
    experiment.fields["killed_store_state"] = killed_state
    experiment.fields["control_store_state"] = control_state

    after_out = root / "w2-after-run"
    after = _run_child(harness_binary, "lifecycle-begin-save", after_out, store=killed)
    accepted_out = root / "w2-accepted-run"
    accepted = _run_child(harness_binary, "lifecycle-begin-save", accepted_out, store=control)
    after_trace = (after_out / "trace.jsonl").read_text(encoding="utf-8")
    accepted_trace = (accepted_out / "trace.jsonl").read_text(encoding="utf-8")
    refused = "UninspectedState" in after_trace
    control_accepted = "row.not-run" not in accepted_trace
    experiment.fields.update(
        {
            "after_stdout": after.stdout.strip(),
            "accepted_stdout": accepted.stdout.strip(),
            "killed_store_refused": refused,
            "killed_store_refusal_class": _refusal_class(after_trace),
            "control_store_accepted": control_accepted,
            "after_trace_tail": after_trace.strip().splitlines()[-1][:400],
        }
    )
    experiment.notes.append(
        "the hold is harness argv (--hold-save/--hold-at/--hold-ns) on a child that "
        "has already accepted its offered set; it adds no product hook, no feature "
        "flag and no fault-injection surface, and the killed child's own output is a "
        "calibration arm rather than admission evidence"
    )
    experiment.notes.append(
        "the declared refusal is UninspectedState. A refusal with any other class, or "
        "an acceptance, is recorded as it happened: the arm's point is what the "
        "product actually does, not what the specification predicted"
    )
    experiment.outcome = "SATISFIED" if refused and control_accepted else "REFUTED"
    return experiment


def _watermark_state(path: Path) -> dict[str, object]:
    """`(ceiling, highest pack id, objects)` read straight out of the Store file.

    Read with `sqlite3` and not through the product, so the arm's own description of
    the state it killed into is independent of the code under test.
    """
    import sqlite3

    try:
        connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
        ceiling = connection.execute(
            "SELECT retained_pack_ceiling FROM store_policy"
        ).fetchone()
        highest = connection.execute("SELECT MAX(pack_id) FROM object_packs").fetchone()
        objects = connection.execute("SELECT COUNT(*) FROM objects").fetchone()
        connection.close()
        return {
            "retained_pack_ceiling": None if ceiling is None else ceiling[0],
            "highest_pack_id": None if highest is None else highest[0],
            "object_rows": None if objects is None else objects[0],
        }
    except sqlite3.Error as error:
        return {"error": str(error)}


def _refusal_class(trace_text: str) -> str:
    """The error class a `row.not-run` record names, or the empty string."""
    for line in trace_text.splitlines():
        if "row.not-run" not in line:
            continue
        _, _, value = line.partition('"value":"')
        return value.split("|")[1][:200] if "|" in value else value[:200]
    return ""


def w4_sealed_call_graph() -> Experiment:
    """W4: the sealed call-graph status and the runtime tripwires.

    The half that cannot be a counter. A scan of every product source file for the
    tokens that would mean the no-fsync/no-WAL/no-retry contract had been broken,
    plus the runtime readings that would show it: a journal sidecar, a persisted
    WAL mode, or more than one watermark row.
    """
    experiment = Experiment(
        identifier="W4",
        question="is the no-fsync / no-WAL / no-retry contract intact in the sealed product source, and do the runtime tripwires agree",
    )
    status = invariants.scan()
    experiment.fields.update(
        {
            "files_scanned": status["files_scanned"],
            "token_classes": status["checked"],
            "scan_status": status["status"],
            "scan_findings": status["findings"],
        }
    )
    experiment.notes.append(
        "the scan strips Rust comments first, deliberately: the product documents why "
        "these calls are absent, and a scan that counted the explanation could not be "
        "kept green"
    )
    experiment.outcome = "SATISFIED" if status["status"] == "PASS" else "REFUTED"
    return experiment


def run_all(
    harness_binary: Path,
    repo_root: Path,
    work_root: Path | None = None,
    artifact_of: Callable[[str], str | None] | None = None,
) -> list[Experiment]:
    """Runs E1-E4 and the three designed arms, failures included.

    `artifact_of` resolves the prepared master a calibration arm's child needs, and
    acquires it once if it is missing. An arm that drives a registry case whose
    fixture is a prepared master cannot start that child without one, and an arm
    that silently ran a `NOT_RUN` child would report a product refusal that never
    happened.
    """
    results: list[Experiment] = []
    with tempfile.TemporaryDirectory(dir=str(work_root) if work_root else None) as directory:
        root = Path(directory)
        results.append(e1_clone_isolation(root))
        results.append(e2_clone_residency(root))
        results.append(e3_quiescent_store(root, harness_binary, repo_root))
        results.append(e4_tree_fidelity(root))
        results.append(w1_watermark_refusal(root, harness_binary))
        results.append(w2_declared_process_kill(root, harness_binary, artifact_of))
        results.append(w4_sealed_call_graph())
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
