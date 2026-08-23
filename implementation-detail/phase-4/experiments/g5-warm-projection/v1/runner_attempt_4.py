#!/usr/bin/env python3
import argparse
import importlib.util
import pathlib

HERE = pathlib.Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("g5_projection_v1_attempt_3", HERE / "runner_attempt_3.py")
ATTEMPT_3 = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ATTEMPT_3)
RUNNER = ATTEMPT_3.RUNNER
ATTEMPT_2 = ATTEMPT_3.ATTEMPT_2
SOURCE_INPUT = HERE / "method/INPUT-MANIFEST-ATTEMPT-3-v1.json"

OLD_INPUT = RUNNER.INPUT_MANIFEST
RUNNER.INPUT_MANIFEST = HERE / "method/INPUT-MANIFEST-ATTEMPT-4-v1.json"
RUNNER.FREEZE = HERE / "method/SOURCE-FREEZE-ATTEMPT-4-v1.json"
RUNNER.DRY_RUN = HERE / "DRY-RUN-ATTEMPT-4-v1.json"
RUNNER.RESULT_NAMES = {
    "screen": "phase4-g5-warm-projection-v1-screen-attempt-4",
    "gate": "phase4-g5-warm-projection-v1-gate-attempt-4",
}
RUNNER.AUTHORITATIVE = tuple(
    RUNNER.INPUT_MANIFEST if path == OLD_INPUT else path for path in RUNNER.AUTHORITATIVE
) + (HERE / "runner_attempt_4.py", SOURCE_INPUT)


def adopt_inputs(executable, input_root):
    executable = pathlib.Path(executable).resolve()
    input_root = pathlib.Path(input_root).resolve()
    if RUNNER.INPUT_MANIFEST.exists() or input_root.exists():
        raise RuntimeError("attempt-4 adopted inputs already exist")
    source = RUNNER.load_json(SOURCE_INPUT)
    input_root.mkdir(parents=True)
    records = {}
    for mode in ("self-check", "screen-count", "screen", "gate"):
        old_root = pathlib.Path(source["inputs"][mode]["root"])
        new_root = input_root / mode
        ATTEMPT_2._run(["/bin/cp", "-cR", str(old_root), str(new_root)], check=True)
        relocation = ATTEMPT_2._relocate(new_root, old_root)
        product = dict(source["inputs"][mode]["product"])
        product["fixture"] = str(new_root / "G5-PROJECTION-FIXTURE-v1.tsv")
        records[mode] = {
            "root": str(new_root),
            "product": product,
            "inventory": RUNNER.inventory(new_root),
            "adoption": relocation,
        }
    manifest = {
        "schema": "phase4-g5-2-input-manifest-v1",
        "status": "PASS",
        "executable_sha256": RUNNER.sha256(executable),
        "preparation_timing": "outside-campaign",
        "input_reuse": True,
        "source_input_manifest_sha256": RUNNER.sha256(SOURCE_INPUT),
        "source_prepare_algorithm_unchanged": True,
        "inputs": records,
    }
    RUNNER.write_json(RUNNER.INPUT_MANIFEST, manifest)
    return manifest


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("adopt-inputs", "freeze", "forecast", "screen", "gate"))
    parser.add_argument("--executable")
    parser.add_argument("--input-root")
    args = parser.parse_args()
    if args.action == "adopt-inputs":
        return adopt_inputs(args.executable, args.input_root)
    if args.action == "freeze":
        return RUNNER.freeze(args.executable)
    if args.action == "forecast":
        return RUNNER.forecast()
    return RUNNER.campaign(args.action)


if __name__ == "__main__":
    print(RUNNER.compact(main()))
