#!/usr/bin/env python3
"""The runner's view of the retained-history corpus.

Three jobs, and no fourth:

* **identity** — the corpus identity every receipt of this lane records: the
  manifest's SHA-256, the pinned tip, the checkpoint count, and the per-state
  oracle digests. A missing corpus, a manifest whose SHA differs, or a tip that
  differs is a **refusal with the reason**, never a partial document and never a
  default path.
* **selection** — the three selections, as `full157` indices.
* **pins** — the frozen totals each row must reproduce, and `{}` where none is
  recorded rather than an invented zero.

**It does not read blob bytes.** The driver does that, in Rust, where the blobs are
hashed against both their Git object id and their recorded digest. This module
reads the manifest and the oracles, which is what a receipt needs and what a
pre-run pin check needs.

**`path_states` is the oracle entry count — files *and* directories** (erratum E1).
It is not `checkpoints[].files` and not the `manifest.tsv` line count; both are
about 18 % smaller, and implementing the pin as either makes three correct numbers
look wrong. `manifest_files` is carried beside it so the difference is visible in
the evidence rather than only in the prose.

    python3 core/benchmark/fs-bench-pro-storage-content/shared/history_corpus.py
"""

from __future__ import annotations

import hashlib
import json
import sys
from dataclasses import dataclass
from pathlib import Path

HARNESS_ROOT = Path(__file__).resolve().parent.parent
REPO_ROOT = HARNESS_ROOT.parents[2]

#: SHA-256 of `checkpoint-manifest.json`. The corpus's root identity.
MANIFEST_SHA = "03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271"

#: The pinned source tip the manifest must name.
SOURCE_TIP = "b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed"

#: How many checkpoints the manifest carries.
CHECKPOINTS = 157

#: The corpus root. The runner's `--corpus` default; a reader is always handed a
#: path, and an absent one is refused rather than substituted.
DEFAULT_ROOT = Path("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data")

#: The three selections, in `full157` index order.
SELECTIONS: dict[str, tuple[int, ...]] = {
    "history-stride10": tuple(sorted(set(range(1, 158, 10)) | {157})),
    "history-stride3": tuple(range(1, 158, 3)),
    "history-stride1": tuple(range(1, 158)),
}

#: The frozen pins. `canonical_bytes` / `canonical_objects` are `None` for
#: `history-stride10`, which has no recorded canonical total: its canonical numbers
#: become **first-run pins** and thereafter must be reproduced. `None` is not zero
#: and must never be rendered as one.
PINS: dict[str, dict[str, int | None]] = {
    "history-stride10": {
        "states": 17,
        "path_states": 101_477,
        "logical_bytes": 561_010_345,
        "canonical_bytes": None,
        "canonical_objects": None,
    },
    "history-stride3": {
        "states": 53,
        "path_states": 306_861,
        "logical_bytes": 1_676_767_835,
        "canonical_bytes": 589_423_458,
        "canonical_objects": 73_476,
    },
    "history-stride1": {
        "states": 157,
        "path_states": 904_143,
        "logical_bytes": 4_936_693_030,
        "canonical_bytes": 871_588_115,
        "canonical_objects": 104_705,
    },
}

#: v0.1.6's recorded Store allocated bytes, per row. **A gate** (owner ruling 7):
#: the core figure must land below these, and above is a finding rather than a new
#: baseline. Quoted from `docs/roadmap/0.1/0.1.6/evidence/issue153-retained-history-report.md`
#: (ledger `L31`).
V016_ALLOCATED = {
    "history-stride10": 49_344_512,
    "history-stride3": 64_024_576,
    "history-stride1": 83_947_520,
}

#: The declared **verification** ceilings in nanoseconds, by owner direction of
#: 2026-09-19. Declared before collection and never inflated afterwards.
VERIFICATION_CEILING_NS = {
    "history-stride10": 10_000_000_000,
    "history-stride3": 20_000_000_000,
    "history-stride1": 30_000_000_000,
}

#: Matched Git comparators, cited rather than re-run.
GIT_ALLOCATED = {"stride3": 49_332_224, "stride1": 56_373_248}


class CorpusError(Exception):
    """The corpus is absent or does not authenticate. Never a partial document."""


@dataclass(frozen=True)
class Expectation:
    """What a corpus must be for a caller to accept it.

    The default is the pinned identity of §3. It is an object rather than three
    constants so that a test can hand a **synthetic** corpus its own expectation and
    prove each refusal on its own: against the real pin a wrong tip is unreachable,
    because the manifest's SHA-256 refuses first, and a test that could only ever see
    `ManifestIdentity` would not have proved the tip check exists. `runner.py` never
    constructs one — it uses [`PINNED`].
    """

    sha: str = MANIFEST_SHA
    tip: str = SOURCE_TIP
    checkpoints: int = CHECKPOINTS


#: The pinned identity every real caller uses.
PINNED = Expectation()


def manifest_path(root: Path) -> Path:
    """The manifest inside a corpus root."""
    return Path(root) / "checkpoint-manifest.json"


def identity(
    root: Path = DEFAULT_ROOT, expectation: Expectation = PINNED
) -> dict[str, object]:
    """Authenticates the corpus and returns what a receipt records about it.

    Raises rather than returning a partial document: a missing corpus, a manifest
    whose SHA-256 differs, a tip that differs, or a checkpoint count that differs is
    a refusal with the reason.

    `expectation` is [`PINNED`] unless a test injects its own; see [`Expectation`].
    """
    root = Path(root)
    path = manifest_path(root)
    if not path.is_file():
        raise CorpusError(f"CorpusMissing: {path} is not a file")
    raw = path.read_bytes()
    found = hashlib.sha256(raw).hexdigest()
    if found != expectation.sha:
        raise CorpusError(
            f"ManifestIdentity: expected sha256 {expectation.sha}, found {found}"
        )
    try:
        document = json.loads(raw)
    except json.JSONDecodeError as error:
        raise CorpusError(f"Document: {path}: {error}") from error
    tip = document.get("tip")
    if tip != expectation.tip:
        raise CorpusError(f"SourceTip: expected {expectation.tip}, found {tip!r}")
    checkpoints = document.get("checkpoints")
    if not isinstance(checkpoints, list) or len(checkpoints) != expectation.checkpoints:
        raise CorpusError(
            f"CheckpointCount: expected {expectation.checkpoints}, "
            f"found {len(checkpoints) if isinstance(checkpoints, list) else checkpoints!r}"
        )
    return {
        "root": str(root),
        "manifest_sha256": found,
        "source_tip": tip,
        "checkpoints": len(checkpoints),
        "total_logical_bytes": document.get("total_logical_bytes"),
        "unique_blob_bytes": document.get("unique_blob_bytes"),
        "reachable_commits": document.get("reachable_commits"),
    }


def checkpoints(
    root: Path = DEFAULT_ROOT, expectation: Expectation = PINNED
) -> list[dict[str, object]]:
    """The manifest's checkpoints, after authentication."""
    identity(root, expectation)
    document = json.loads(manifest_path(root).read_bytes())
    return list(document["checkpoints"])


def selection(lane: str) -> tuple[int, ...]:
    """The `full157` indices of one row, or a refusal."""
    try:
        return SELECTIONS[lane]
    except KeyError:
        raise CorpusError(
            f"SelectionShape: {lane!r} is not one of {sorted(SELECTIONS)}"
        ) from None


def check_selection(lane: str, indices: tuple[int, ...] | list[int]) -> None:
    """Refuses a selection that is not strictly ascending from 1 to 157 at the
    declared length."""
    declared = selection(lane)
    if list(indices) != list(declared):
        raise CorpusError(
            f"SelectionShape: {lane} declares {len(declared)} indices "
            f"{list(declared)[:3]}..{list(declared)[-1]}, "
            f"got {len(indices)} {list(indices)[:3]}..{list(indices)[-1] if indices else None}"
        )


def pins(lane: str) -> dict[str, int | None]:
    """The frozen values for one row.

    Returns `{}` for an unknown lane and the recorded mapping — including its
    `None` entries — for a known one. A row with no recorded canonical total gets
    `None`, never an invented zero.
    """
    return dict(PINS.get(lane, {}))


def oracle_paths(root: Path = DEFAULT_ROOT, lane: str = "history-stride10") -> list[Path]:
    """The oracle path of every state in a selection."""
    root = Path(root)
    states = checkpoints(root)
    return [root / "oracles" / f"{states[index - 1]['sha']}.json" for index in selection(lane)]


def derive(
    root: Path = DEFAULT_ROOT,
    lane: str = "history-stride10",
    expectation: Expectation = PINNED,
) -> dict[str, object]:
    """Derives a row's totals **from the corpus**, for the pins to be checked against.

    `path_states` is the oracle entry count — files **and** directories (erratum
    E1). `manifest_files` is the manifest's own `files` total, carried beside it so
    the ~18 % difference is visible.
    """
    root = Path(root)
    states = checkpoints(root, expectation)
    indices = selection(lane)
    path_states = 0
    logical_bytes = 0
    manifest_files = 0
    for index in indices:
        checkpoint = states[index - 1]
        oracle = root / "oracles" / f"{checkpoint['sha']}.json"
        if not oracle.is_file():
            raise CorpusError(f"OracleMissing: {oracle}")
        path_states += len(json.loads(oracle.read_bytes()))
        logical_bytes += int(checkpoint["logical_bytes"])
        manifest_files += int(checkpoint["files"])
    return {
        "row": lane,
        "states": len(indices),
        "path_states": path_states,
        "logical_bytes": logical_bytes,
        "manifest_files": manifest_files,
    }


def ceiling(lane: str) -> int | None:
    """The declared verification ceiling in nanoseconds, or `None` if undeclared."""
    return VERIFICATION_CEILING_NS.get(lane)


def self_check(root: Path = DEFAULT_ROOT) -> list[str]:
    """Checks the pins against the corpus, and the module against itself.

    A **missing corpus is reported, not failed**: the corpus is a campaign input
    rather than a harness dependency, and a machine that has never held it must
    still be able to run the 217-row self-check. The caller prints the reason. A
    corpus that is *present* and does not authenticate **is** a failure.
    """
    failures: list[str] = []

    # The selections are the declared shapes, without the corpus.
    for lane, expected in (("history-stride10", 17), ("history-stride3", 53), ("history-stride1", 157)):
        indices = selection(lane)
        if len(indices) != expected:
            failures.append(f"{lane}: {len(indices)} indices, expected {expected}")
        if indices[0] != 1 or indices[-1] != CHECKPOINTS:
            failures.append(f"{lane}: does not start at 1 and end at {CHECKPOINTS}")
        if any(left >= right for left, right in zip(indices, indices[1:])):
            failures.append(f"{lane}: not strictly ascending")
        check_selection(lane, indices)
    if SELECTIONS["history-stride10"] != tuple(
        sorted(set(range(1, 158, 10)) | {157})
    ):
        failures.append("stride10 is not range(1,158,10) | {157}")
    if SELECTIONS["history-stride3"] != tuple(range(1, 158, 3)):
        failures.append("stride3 is not range(1,158,3)")

    # A wrong selection is refused rather than accepted.
    try:
        check_selection("history-stride3", list(range(1, 158)))
        failures.append("a stride1 selection was accepted for stride3")
    except CorpusError:
        pass
    try:
        selection("history-stride7")
        failures.append("an unknown lane returned a selection")
    except CorpusError:
        pass

    # A row with no recorded canonical total reports None, not zero.
    if pins("history-stride10").get("canonical_bytes") is not None:
        failures.append("stride10 has a recorded canonical total it should not have")
    if pins("history-stride3").get("canonical_objects") != 73_476:
        failures.append("stride3's canonical object pin moved")
    if pins("no-such-lane") != {}:
        failures.append("an unknown lane returned pins")
    for lane, expected in VERIFICATION_CEILING_NS.items():
        if ceiling(lane) != expected or expected <= 0:
            failures.append(f"{lane}: verification ceiling {ceiling(lane)}")

    # The corpus, when it is here.
    if not manifest_path(root).is_file():
        return failures
    try:
        document = identity(root)
    except CorpusError as error:
        failures.append(f"the corpus does not authenticate: {error}")
        return failures
    if document["checkpoints"] != CHECKPOINTS:
        failures.append(f"the corpus has {document['checkpoints']} checkpoints")
    for lane, pin in PINS.items():
        try:
            derived = derive(root, lane)
        except CorpusError as error:
            failures.append(f"{lane}: {error}")
            continue
        for field in ("states", "path_states", "logical_bytes"):
            if derived[field] != pin[field]:
                failures.append(
                    f"{lane}: {field} is {derived[field]}, pin says {pin[field]}"
                )
        if derived["manifest_files"] >= derived["path_states"]:
            failures.append(
                f"{lane}: manifest files {derived['manifest_files']} is not below "
                f"path-states {derived['path_states']}, so erratum E1 is not visible"
            )
    return failures


def main() -> int:
    failures = self_check()
    if not manifest_path(DEFAULT_ROOT).is_file():
        print(f"history-corpus SKIP: {DEFAULT_ROOT} is absent")
        return 0 if not failures else 1
    for lane in SELECTIONS:
        derived = derive(DEFAULT_ROOT, lane)
        print(
            f"  {lane:<18} states={derived['states']:3d} "
            f"path_states={derived['path_states']:7d} "
            f"logical_bytes={derived['logical_bytes']:11d} "
            f"manifest_files={derived['manifest_files']:7d}"
        )
    for failure in failures:
        print(f"history-corpus: FAIL: {failure}", file=sys.stderr)
    print("history-corpus: " + ("FAIL" if failures else "PASS"))
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
