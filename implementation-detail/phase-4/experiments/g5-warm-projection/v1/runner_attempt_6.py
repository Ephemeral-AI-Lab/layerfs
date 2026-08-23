#!/usr/bin/env python3
import argparse
import importlib.util
import pathlib

HERE = pathlib.Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("g5_projection_v1_attempt_5", HERE / "runner_attempt_5.py")
ATTEMPT_5 = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ATTEMPT_5)
RUNNER = ATTEMPT_5.RUNNER

OLD_INPUT = RUNNER.INPUT_MANIFEST
ATTEMPT_5.ATTEMPT_4.SOURCE_INPUT = HERE / "method/INPUT-MANIFEST-ATTEMPT-5-v1.json"
RUNNER.INPUT_MANIFEST = HERE / "method/INPUT-MANIFEST-ATTEMPT-6-v1.json"
RUNNER.FREEZE = HERE / "method/SOURCE-FREEZE-ATTEMPT-6-v1.json"
RUNNER.DRY_RUN = HERE / "DRY-RUN-ATTEMPT-6-v1.json"
RUNNER.RESULT_NAMES = {
    "screen": "phase4-g5-warm-projection-v1-screen-attempt-6",
    "gate": "phase4-g5-warm-projection-v1-gate-attempt-6",
}
RUNNER.AUTHORITATIVE = tuple(
    RUNNER.INPUT_MANIFEST if path == OLD_INPUT else path for path in RUNNER.AUTHORITATIVE
) + (HERE / "runner_attempt_6.py", ATTEMPT_5.ATTEMPT_4.SOURCE_INPUT)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("adopt-inputs", "freeze", "forecast", "screen", "gate"))
    parser.add_argument("--executable")
    parser.add_argument("--input-root")
    args = parser.parse_args()
    if args.action == "adopt-inputs":
        return ATTEMPT_5.ATTEMPT_4.adopt_inputs(args.executable, args.input_root)
    if args.action == "freeze":
        return RUNNER.freeze(args.executable)
    if args.action == "forecast":
        return RUNNER.forecast()
    return RUNNER.campaign(args.action)


if __name__ == "__main__":
    print(RUNNER.compact(main()))
