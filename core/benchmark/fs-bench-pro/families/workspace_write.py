"""Frozen #286 public SDK/POSIX write matrix; execution follows family 2."""

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path


HERE = Path(__file__).resolve().parents[1]
ROOT = HERE.parents[2]
SIZE = 10 << 20
MASTER = Path(
    "/Users/yifanxu/.codex/worktrees/issue271-root-custody/layerfs/"
    "benchmark-results/fs-bench-pro/issue271/fourhop-patterns-prepared-v1/prepared.json"
)
MASTER_STORE_SHA256 = "20ea70dc9509e1819b11667bf97ab2a5ab3bd2b5d628d0a48f4c4fa6876691be"
MASTER_HISTORY_SHA256 = "55a02d7da0c35b084b721706354dcaf6e30aee3f824ad32543cf2e108dbdfd19"
MASTER_PROOF_SHA256 = "d2f535e0a405eca68746b7968350118b85ddbe69b95a66416a38238a7da5f6c0"
OLD_MANIFEST_SHA256 = "d664c30d37508679421ab4989c3256f5da430edda0c4dc4d148d863085249927"
PATTERNS = ("append", "dispersed", "repeated")
COUNTS = (100, 512, 4097)


@dataclass(frozen=True)
class Case:
    id: str
    pattern: str
    writes: int
    command_budget_ns: int
    verifier_budget_ns: int = 9_000_000_000

    @property
    def command(self):
        return f"/fixtures/bin/write-separated {self.pattern} data.bin {self.writes}"


CASES = {case.id: case for case in (
    Case(f"workspace-write-{pattern}-writes-{count}-v1", pattern, count,
         (25 if count == 4097 else 15) * 1_000_000_000)
    for pattern in PATTERNS for count in COUNTS
)}
SELECTED = tuple(CASES)


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def master():
    """Pin the independent prepared master without reading it into a timed phase."""
    prepared = json.loads(MASTER.read_text())
    row = prepared["master"]
    source = Path(row["path"])
    for name, expected in (("store", MASTER_STORE_SHA256),
                           ("history", MASTER_HISTORY_SHA256),
                           ("verify.stdout", MASTER_PROOF_SHA256)):
        path = source / (name if name == "verify.stdout" else f"{name}.sqlite")
        if sha256(path) != expected:
            raise ValueError(f"prepared {name} seal changed")
    if (row["store_sha256"] != MASTER_STORE_SHA256
            or row["history_sha256"] != MASTER_HISTORY_SHA256
            or row["proof_sha256"] != MASTER_PROOF_SHA256):
        raise ValueError("prepared master record changed")
    return {**row, "path": str(source), "prepared": str(MASTER)}


def expected(pattern, writes):
    if pattern not in PATTERNS or writes not in COUNTS:
        raise ValueError("unregistered write matrix cell")
    data = bytearray(b"A" * SIZE)
    for i in range(writes):
        value = ord("B") + i % 24
        if pattern == "append":
            data.append(value)
        elif pattern == "dispersed":
            data[(104729 + i * 2654435761) % SIZE] = value
        else:
            data[5 << 20] = value
    return bytes(data)


def manifest(data):
    return (f".\td\t493\t0\t-\n"
            f"data.bin\tf\t420\t{len(data)}\t{hashlib.sha256(data).hexdigest()}\n")


def manifests():
    old = manifest(b"A" * SIZE)
    if hashlib.sha256(old.encode()).hexdigest() != OLD_MANIFEST_SHA256:
        raise ValueError("old oracle manifest differs from frozen #271")
    return {"old": old, **{case.id: manifest(expected(case.pattern, case.writes))
                            for case in CASES.values()}}
