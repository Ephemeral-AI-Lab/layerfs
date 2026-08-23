#!/usr/bin/env python3
import importlib.util
import json
import os
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("g5_projection_v1_frozen_runner", HERE / "runner.py")
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)

RUNNER.RESULT_NAMES = {
    "screen": "phase4-g5-warm-projection-v1-screen-attempt-2",
    "gate": "phase4-g5-warm-projection-v1-gate-attempt-2",
}

_run = RUNNER.subprocess.run
_inventory = RUNNER.inventory
_pending = {}


def _relocate(destination, source):
    fixture = destination / "G5-PROJECTION-FIXTURE-v1.tsv"
    before = fixture.read_text(encoding="utf-8")
    directory_lines = [line for line in before.splitlines() if line.startswith("directory\t")]
    if len(directory_lines) != 1:
        raise RuntimeError("cloned fixture directory binding is not exact")
    old_directory = pathlib.Path(directory_lines[0].split("\t", 1)[1])
    if old_directory.parent != source:
        raise RuntimeError("cloned fixture source root binding is not exact")
    new_directory = destination / old_directory.name
    old = f"directory\t{old_directory}\n"
    new = f"directory\t{new_directory}\n"
    after = before.replace(old, new)
    with fixture.open("w", encoding="utf-8") as handle:
        handle.write(after)
        handle.flush()
        os.fsync(handle.fileno())
    RUNNER.fsync_dir(fixture.parent)
    return {
        "classification": "AttemptLocalAbsolutePathRelocationAfterCloneInventoryEquality",
        "source_root": str(source),
        "attempt_root": str(destination),
        "old_directory": str(old_directory),
        "new_directory": str(new_directory),
        "product_or_canonical_bytes_changed": False,
        "fixture_bytes_before": len(before.encode()),
        "fixture_bytes_after": len(after.encode()),
    }


def inventory(path):
    path = pathlib.Path(path)
    result = _inventory(path)
    if path in _pending and "receipt" not in _pending[path]:
        _pending[path]["receipt"] = _relocate(path, _pending[path]["source"])
    return result


def run(command, *args, **kwargs):
    result = _run(command, *args, **kwargs)
    values = [str(value) for value in command]
    if values[:2] == ["/bin/cp", "-cR"] and result.returncode == 0:
        _pending[pathlib.Path(values[3])] = {"source": pathlib.Path(values[2])}
    elif "--g5-projection-run" in values and result.returncode == 0:
        root = pathlib.Path(values[values.index("--g5-projection-run") + 1])
        receipt = _pending.get(root, {}).get("receipt")
        if receipt is None:
            raise RuntimeError("missing attempt-local fixture relocation receipt")
        product = json.loads(result.stdout)
        product["fixture_relocation"] = receipt
        result = subprocess.CompletedProcess(
            result.args,
            result.returncode,
            RUNNER.compact(product) + "\n",
            result.stderr,
        )
    return result


RUNNER.inventory = inventory
RUNNER.subprocess.run = run


if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in ("screen", "gate"):
        raise SystemExit("usage: runner_attempt_2.py screen|gate")
    print(RUNNER.compact(RUNNER.campaign(sys.argv[1])))
