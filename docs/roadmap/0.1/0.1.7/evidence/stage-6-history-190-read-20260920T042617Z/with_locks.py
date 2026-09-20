#!/usr/bin/env python3
"""Hold both shared global flocks (deduplicated) for one resource command.

This is the #190 global lock protocol: `flock(LOCK_EX|LOCK_NB)` on the resolved
`$TMPDIR/layerfs-infra-measurement.lock` then `/tmp/layerfs-infra-measurement.lock`.
It deliberately does NOT touch the harness private `.measurement.lock`, which uses
a different `O_CREAT|O_EXCL` marker protocol owned by `shared/receipt.py`.
A held lock means defer; this wrapper never waits and never interrupts anyone.
"""
import contextlib
import fcntl
import json
import os
import subprocess
import sys
import time
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parent


def paths():
    return list(dict.fromkeys([
        (Path(os.environ.get("TMPDIR", "/tmp")) / "layerfs-infra-measurement.lock").resolve(),
        Path("/tmp/layerfs-infra-measurement.lock").resolve(),
    ]))


def main() -> int:
    argv = sys.argv[1:]
    label = argv[0]
    record = {"schema": "h190-read-lock-v1", "label": label, "command": argv[1:],
              "utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
              "locks": [str(p) for p in paths()]}
    start = time.monotonic_ns()
    status = None
    with contextlib.ExitStack() as stack:
        for path in paths():
            handle = stack.enter_context(path.open("a"))
            try:
                fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except OSError:
                record.update(outcome="DEFERRED", reason=f"held: {path}",
                              wall_ns=time.monotonic_ns() - start)
                (CAMPAIGN / "checks" / f"lock-deferred-{time.time_ns()}.json").write_text(
                    json.dumps(record, indent=2) + "\n")
                print(f"DEFERRED: {path} is held")
                return 3
        record["acquired_utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        result = subprocess.run(argv[1:])
        status = result.returncode
    record.update(outcome="COMPLETED", exit_code=status,
                  wall_ns=time.monotonic_ns() - start)
    (CAMPAIGN / "checks" / f"{label}.json").write_text(json.dumps(record, indent=2) + "\n")
    return status if status is not None else 1


if __name__ == "__main__":
    raise SystemExit(main())
