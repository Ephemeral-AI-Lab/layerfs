#!/usr/bin/env python3
"""Phase 0 collection driver.

Runs each frozen command exactly once, appends its exit code and wall time to
commands.tsv, and writes its complete output to a fresh log. Nothing here edits
the product, the reference, the tests or the examples: it only launches them.
"""

import json
import os
import pathlib
import subprocess
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[5].resolve()
TOOLCHAIN = "+1.85.1"
CLIENT = "/tmp/layerfs-phase0-target/release/phase0client"
CORE_TARGET = REPO / "core/target/release/examples"

RECORD = HERE / "commands.tsv"


def run(step, label, command, cwd=REPO, env=None):
    started = time.monotonic()
    merged = dict(os.environ)
    merged["LAYERFS_CONSTRUCTION_WORKERS"] = "1"
    if env:
        merged.update(env)
    result = subprocess.run(
        command, cwd=cwd, capture_output=True, text=True, env=merged
    )
    wall = time.monotonic() - started
    log = HERE / "logs" / f"{step}-{label}.log"
    log.parent.mkdir(exist_ok=True)
    log.write_text(
        f"$ {' '.join(command)}\n"
        f"cwd {cwd}\n"
        f"exit_code {result.returncode}\n"
        f"wall_s {wall:.3f}\n"
        f"--- stdout ---\n{result.stdout}"
        f"--- stderr ---\n{result.stderr}"
    )
    with RECORD.open("a") as handle:
        handle.write(f"{step}\t{label}\t{result.returncode}\t{wall:.3f}\t{' '.join(command)}\n")
    print(f"{step} {label} exit {result.returncode} wall {wall:.3f}s -> {log.name}")
    return result, wall


def fresh(*parts):
    """A path that does NOT exist yet.

    `filesystem_timing_c1` and `measure_pooled` create their own output
    directory; `measure_filesystem` and `measure_edits` refuse an output that
    already exists. Passing a non-existent path therefore satisfies all of them,
    and the parent is created so the vehicles can write into it.
    """
    path = HERE.joinpath(*parts)
    if path.exists():
        raise SystemExit(f"refusing to reuse an existing output path: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    return path


def main():
    which = sys.argv[1] if len(sys.argv) > 1 else "all"
    RECORD.write_text("step\tlabel\texit_code\twall_s\tcommand\n") if not RECORD.exists() else None

    if which in ("all", "build"):
        run("B1", "core-examples", ["cargo", TOOLCHAIN, "build", "--release", "--offline",
                                    "--locked", "--manifest-path", "core/Cargo.toml", "--examples"])
        run("B2", "reference-example", ["cargo", TOOLCHAIN, "build", "--release", "--offline",
                                        "--locked", "--manifest-path", "Cargo.toml",
                                        "-p", "layerfs-content", "--example",
                                        "stage5_component_reference"])
        run("B3", "client", ["cargo", TOOLCHAIN, "build", "--release", "--offline", "--locked"],
            cwd=HERE / "client", env={"CARGO_TARGET_DIR": "/tmp/layerfs-phase0-target"})

    if which in ("all", "p0-1"):
        run("C0", "component-primitives", ["python3", "tools/stage5_component_comparison.py",
                                           "--output", str(HERE / "p0-1-component" / "run-1")])

    if which in ("all", "p0-2"):
        # S0: ceiling confirmation at two sizes.
        run("S0a", "c2-rows-1", [CLIENT, "c2", "1", "default"])
        run("S0b", "c2-rows-8191", [CLIENT, "c2", "8191", "default"])
        run("S1", "c2-ceiling-default", [CLIENT, "c2", "8191", "default"])
        run("S2", "c2-ceiling-diagnostic-default-cache",
            [CLIENT, "c2", "8191", "diagnostic-default-cache"])
        run("S2b", "c2-ceiling-diagnostic-large-cache",
            [CLIENT, "c2", "8191", "diagnostic-large-cache"])
        run("S2c", "c2-diagnostic-default-cache-4x",
            [CLIENT, "c2", "32764", "diagnostic-default-cache"])
        run("S2d", "c2-diagnostic-large-cache-4x",
            [CLIENT, "c2", "32764", "diagnostic-large-cache"])
        run("S2e", "spill-counter-control",
            ["/tmp/layerfs-spillctl-target/release/spillcontrol"])
        run("S3", "c2-small-default", [CLIENT, "c2", "1023", "default"])

    if which in ("all", "p0-3"):
        fs = CORE_TARGET / "measure_filesystem"
        for index, mode in enumerate(["c1", "c2", "pipeline"]):
            out = fresh("p0-3-counters", f"fs-{mode}-directory-update")
            out.mkdir(parents=True)
            run(f"D{index + 7}", f"fs-{mode}-directory-update",
                [str(fs), "--mode", mode, "--case", "directory-update",
                 "--output", str(out)])
        edits = CORE_TARGET / "measure_edits"
        step = 10
        for mode in ["c1", "c2", "pipeline"]:
            for case in ["small", "chunked", "small-to-large", "large-to-small", "batch"]:
                out = fresh("p0-3-counters", f"edits-{mode}-{case}")
                run(f"D{step}", f"edits-{mode}-{case}",
                    [str(edits), "--mode", mode, "--case", case,
                     "--threshold-bytes", "131072", "--output", str(out)])
                step += 1
        if which == "all":
            run("D25", "order-default-4096", [CLIENT, "order", "4000", "2000", "4096"])
            run("D26", "order-forced-64", [CLIENT, "order", "4000", "2000", "64"])

    if which in ("all", "extra"):
        et = CORE_TARGET / "edit_timing_c1"
        run("D27", "edit-timing-c1-nodes-read", [str(et)])

    if which in ("all", "diag"):
        run("X1", "order-forced-64-repeat", [CLIENT, "order", "4000", "2000", "64"])
        run("X1b", "order-default-4096-repeat", [CLIENT, "order", "4000", "2000", "4096"])
        c1 = CORE_TARGET / "filesystem_timing_c1"
        out = fresh("p0-3-counters", "c1-subtree-remove-repeat")
        out.mkdir(parents=True)
        run("X2", "c1-subtree-remove-repeat", [str(c1), "--case", "subtree-remove", "--output", str(out)])
        run("X3", "component-wide-repeat", ["python3", "tools/stage5_component_comparison.py",
                                            "--output", str(HERE / "p0-1-component" / "diagnostic-run-2")])


if __name__ == "__main__":
    main()
