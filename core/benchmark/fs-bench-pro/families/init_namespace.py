"""Frozen SDK Init cases, source fixture and one public SDK operation."""
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import time
import uuid

PROFILE = "core-sdk-init-fixture-v1"
ROUTE = "host-direct-sdk-v1"
MTIME_NS = 1_700_000_000_000_000_000
CHUNK = 1024 * 1024


@dataclass(frozen=True)
class Case:
    id: str
    files: int
    directories: int
    logical_bytes: int
    anchor_bytes: int
    classes: tuple[int, int, int, int, int]


CASES = {
    case.id: case for case in (
        Case("namespace-100-compact-v3", 100, 1, 5_000_000, 1_000_000, (1, 1, 78, 15, 5)),
        Case("namespace-1000-compact-v3", 1_000, 10, 20_000_000, 5_000_000, (1, 10, 789, 150, 50)),
        Case("namespace-10000", 10_000, 100, 300_000_000, 100_000_000, (1, 100, 7_899, 1_500, 500)),
        Case("namespace-100000", 100_000, 1_000, 500_000_000, 100_000_000, (2, 1_000, 78_998, 15_000, 5_000)),
    )
}
SELECTED = tuple(CASES)[:2]
NOT_RUN_REASON = "outside the #236 SDK two-case selection; #231 four-tier gate open"


def plan(case: Case):
    """Exact count, class and byte allocation declared in SPEC.md."""
    if case.files != case.directories * 100 or sum(case.classes) != case.files:
        raise ValueError("case count mismatch")
    roles = [role for role, count in zip(("anchor", "empty", "tiny", "small", "medium"), case.classes) for _ in range(count)]
    paths = [f"d{index // 100:04}/f{index:06}" for index in range(case.files)]
    sizes = [case.anchor_bytes if role == "anchor" else 0 if role == "empty" else 1 for role in roles]
    weights = {"tiny": 1, "small": 64, "medium": 1024}
    total_weight = sum(weights.get(role, 0) for role in roles)
    budget = case.logical_bytes - sum(sizes)
    if budget < 0 or not total_weight:
        raise ValueError("case byte budget")
    floors = [(budget * weights.get(role, 0)) // total_weight for role in roles]
    sizes = [size + floor for size, floor in zip(sizes, floors)]
    extra = case.logical_bytes - sum(sizes)
    for index in sorted((i for i, role in enumerate(roles) if role in weights), key=lambda i: paths[i])[:extra]:
        sizes[index] += 1
    if sum(sizes) != case.logical_bytes:
        raise ValueError("case byte apportionment")
    return list(zip(paths, roles, sizes))


def _row(path, kind, mode, size=0, sha="-"):
    return f"{path}\t{kind}\t{mode}\t{MTIME_NS}\t{size}\t{sha}\n"


def _check_reuse(case: Case, home: Path):
    receipt = json.loads((home / "fixture.json").read_text())
    raw = (home / "manifest.tsv").read_bytes()
    if receipt["case"] != case.id or receipt["profile"] != PROFILE or receipt["route"] != ROUTE or hashlib.sha256(raw).hexdigest() != receipt["manifest_sha256"]:
        raise ValueError("prepared fixture seal mismatch")
    expected = {parts[0]: parts for parts in (line.split("\t") for line in raw.decode().splitlines())}
    if len(expected) != case.files + case.directories + 1:
        raise ValueError("prepared fixture path count")
    source = home / "payload"
    actual = {"."}
    for path in source.rglob("*"):
        relative = path.relative_to(source).as_posix()
        item = expected.get(relative)
        metadata = path.lstat()
        actual_kind = "f" if stat.S_ISREG(metadata.st_mode) else "d" if stat.S_ISDIR(metadata.st_mode) else "unsupported"
        if item is None or item[1] != actual_kind:
            raise ValueError("prepared fixture inventory mismatch")
        if metadata.st_size != int(item[4]) and item[1] == "f":
            raise ValueError("prepared fixture size mismatch")
        if metadata.st_mode & 0o7777 != int(item[2]) or metadata.st_mtime_ns != int(item[3]):
            raise ValueError("prepared fixture metadata mismatch")
        actual.add(relative)
    if actual != set(expected):
        raise ValueError("prepared fixture missing path")
    root = source.lstat()
    if root.st_mode & 0o7777 != 0o750 or root.st_mtime_ns != MTIME_NS:
        raise ValueError("prepared fixture root metadata mismatch")
    return receipt


def prepare(case: Case, prepared_root: Path):
    """Create once and seal while writing; reuse checks never read payload bytes."""
    start = time.monotonic_ns()
    key = hashlib.sha256(f"{PROFILE}|{ROUTE}|{case.id}|1".encode()).hexdigest()[:16]
    home = prepared_root / f"{case.id}-{key}"
    if home.exists():
        receipt = _check_reuse(case, home)
        return {**receipt, "source": str(home / "payload"), "manifest": str(home / "manifest.tsv"),
                "preparation_wall_ns": time.monotonic_ns() - start, "reused": True}
    prepared_root.mkdir(parents=True, exist_ok=True)
    partial = prepared_root / f".{case.id}.partial-{uuid.uuid4().hex}"
    partial.mkdir()
    source = partial / "payload"
    source.mkdir()
    rows = [_row(".", "d", 0o750)]
    for index in range(case.directories):
        directory = source / f"d{index:04}"
        directory.mkdir()
        rows.append(_row(directory.name, "d", 0o750))
    for relative, _, size in plan(case):
        file = source / relative
        digest = hashlib.sha256()
        with file.open("wb") as output:
            left = size
            index = 0
            while left:
                count = min(left, CHUNK)
                block = hashlib.shake_256(f"{ROUTE}|{case.id}|1|{relative}|{index}".encode()).digest(count)
                output.write(block)
                digest.update(block)
                left -= count
                index += 1
        os.chmod(file, 0o640)
        os.utime(file, ns=(MTIME_NS, MTIME_NS))
        rows.append(_row(relative, "f", 0o640, size, digest.hexdigest()))
    for directory in source.iterdir():
        os.chmod(directory, 0o750)
        os.utime(directory, ns=(MTIME_NS, MTIME_NS))
    os.chmod(source, 0o750)
    os.utime(source, ns=(MTIME_NS, MTIME_NS))
    raw = "".join(rows).encode()
    (partial / "manifest.tsv").write_bytes(raw)
    receipt = {"case": case.id, "profile": PROFILE, "route": ROUTE, "seed": 1, "files": case.files,
               "directories": case.directories + 1, "logical_bytes": case.logical_bytes,
               "manifest_sha256": hashlib.sha256(raw).hexdigest()}
    (partial / "fixture.json").write_text(json.dumps(receipt, sort_keys=True, indent=2) + "\n")
    partial.rename(home)
    return {**receipt, "source": str(home / "payload"), "manifest": str(home / "manifest.tsv"),
            "preparation_wall_ns": time.monotonic_ns() - start, "reused": False}


def public_import(binary, case: Case, source: Path, store: Path, history: Path,
                  private: str, cursor: str):
    """One public SDK Init; the driver times only Client::init_project."""
    command = [binary, str(source), str(store), str(history), case.id]
    started = time.monotonic_ns()
    try:
        child = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               timeout=15, env={**os.environ, "LAYERFS_PRIVATE_KEY": private,
                                                "LAYERFS_HISTORY_CURSOR_KEY": cursor})
        stdout, stderr, exit_code, timed_out = child.stdout, child.stderr, child.returncode, False
    except subprocess.TimeoutExpired as error:
        stdout, stderr, exit_code, timed_out = error.stdout or b"", error.stderr or b"", None, True
    command_wall_ns = time.monotonic_ns() - started
    try:
        result = json.loads(stdout) if stdout else None
    except (ValueError, UnicodeDecodeError):
        result = None
    if not isinstance(result, dict):
        result = {"status": "INCOMPLETE", "operation_ns": None,
                  "error": "missing or malformed SDK result"}
    if result.get("status") == "COMPLETE" and (
        exit_code != 0 or timed_out or not result.get("project_id", "").startswith("31") or
        not result.get("genesis_layer", "").startswith("32")
    ):
        result["status"] = "INCOMPLETE"
        result["error"] = "SDK result or process terminal mismatch"
    return result, command_wall_ns, stdout, stderr, exit_code, timed_out, command
