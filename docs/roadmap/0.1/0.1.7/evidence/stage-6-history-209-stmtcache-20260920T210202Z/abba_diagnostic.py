#!/usr/bin/env python3
"""A drift-cancelling A B B A diagnostic of the one treatment, same binary.

**Diagnostic, never a gate row.** The round's gate pair is one sample per arm
(`control2`, `treat2`). This script exists because the machine's between-sample
level moved by more than the treatment is expected to: the *same archived binary*
that read 26.467 s in the previous round's session reads 19.908 s in this one, and
two binaries with the same product behaviour read 17.105 s and 19.908 s fifteen
minutes apart. A single A/B pair cannot resolve an effect that size.

Rows 1 and 2 are the gate pair, one sample per arm; A B B A cancels a linear drift over the window: the treatment effect is
mean(B) - mean(A) and the two A rows measure the drift the window actually had.
Every row is retained, every deferral is retained, and no row is dropped or
re-selected. `collect.py` and the measured binary are unchanged: this is a driver,
not a harness change.

Usage: abba_diagnostic.py <binary> <label-prefix>
"""
import subprocess
import sys
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parent
STEPS = [("a", ()), ("b", ("--env", "LAYERFS_STORAGE_STATEMENT_CACHE=0")),
         ("b", ("--env", "LAYERFS_STORAGE_STATEMENT_CACHE=0")), ("a", ())]


def main() -> int:
    binary = sys.argv[1]
    prefix = sys.argv[2]
    repo = CAMPAIGN.parents[5]
    for index, (arm, extra) in enumerate(STEPS, start=1):
        for attempt in range(4):
            label = f"{prefix}-{index}{arm}" + (f"r{attempt}" if attempt else "")
            command = ["python3", str(CAMPAIGN / "with_locks.py"), f"{label}-stride10",
                       "python3", str(CAMPAIGN / "collect.py"), label, "history-stride10",
                       "--binary", binary, "--cwd", str(repo), "--seal-repo", str(repo),
                       "--pre-execute", *extra]
            print(f"--- step {index} arm {arm} label {label}", flush=True)
            result = subprocess.run(command)
            if result.returncode == 0:
                break
            print(f"    deferred or failed ({result.returncode}); retrying into a new output",
                  flush=True)
        else:
            print(f"    step {index} could not be sampled")
            return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
