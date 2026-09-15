#!/usr/bin/env python3
"""One bounded chmod-persistence probe on a sealed v0.1.6 M1 prepared Store.

Purpose: separate the declared M1 workload from a product behaviour question.
The probe creates one live Workspace, applies one `chmod` to one directory and
one `chmod` to one regular file, publishes one full-status Commit, reads the
mode back through the Store, and then reads it back again from the same
workspace. It produces no benchmark receipt and is not part of the qualified
matrix; its output is diagnostic evidence only.
"""

import argparse
import json
import os
from pathlib import Path
import sys

BENCH = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(BENCH))

from shared import runner, runtime  # noqa: E402


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--family", default="mixed_load_bearing")
    parser.add_argument("--case", default="v016-mixed-development-100mb-5000-k10-v1")
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--image", default=os.environ.get("LAYERFS_BENCH_IMAGE"))
    parser.add_argument("--host-binary", default=str(runner.REPO / "target/release/fs-benchmark-pro"))
    args = parser.parse_args()
    binary = Path(args.host_binary)
    identity = json.loads(Path(str(binary) + ".identity.json").read_text())
    if runtime.file_sha256(binary) != identity["binary_sha256"]:
        raise SystemExit("host binary seal mismatch")
    if not args.image:
        raise SystemExit("--image is required")
    source = identity["LAYERFS_SOURCE_SEAL"]
    case = args.case
    fixture = runner.records(
        runner._command(
            [str(binary), "infra-fixture-info", args.family, case, str(args.seed)],
            runner._deadline(runtime.time.monotonic() + 120),
            env={"LAYERFS_V013_IMAGE": args.image},
        ).stdout
    )[-1]
    listed = runner.records(
        runner._command(
            [str(binary), "infra-list", args.family, case],
            runner._deadline(runtime.time.monotonic() + 120),
            env={"LAYERFS_V013_IMAGE": args.image},
        ).stdout
    )
    recipe = [row for row in listed if row.get("scenario_id") == case][0]
    key = runner.digest(
        {"family": args.family, "case": case, "seed": args.seed, "source": source, "recipe": recipe}
    )
    root = runner.HOST_ROOT / "prepared" / key
    if not root.is_dir():
        raise SystemExit(f"prepared master absent: {root}")
    print(f"prepared={root}")
    print(json.dumps(fixture, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
