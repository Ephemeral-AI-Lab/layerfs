"""Independent C1, supplied-object C2 and integrated rows, one sample each.

Records the exact command, its exit status, its complete wall time and its stdout
for each mode into this append-only directory. Warm in-process fixtures: no
cold-cache or storage claim.
"""
import json
import pathlib
import subprocess
import time

HERE = pathlib.Path(__file__).resolve().parent
ROOT = next(parent for parent in HERE.parents if (parent / "core/Cargo.toml").exists())
ROWS = (
    ("c1", ["cargo", "+1.85.1", "run", "--locked", "--manifest-path", "core/Cargo.toml",
            "-p", "layerfs-content", "--example", "filesystem_timing_c1", "--",
            "--case", "directory-update"], HERE / "c1-output"),
    ("c2", ["cargo", "+1.85.1", "run", "--locked", "--manifest-path", "core/Cargo.toml",
            "-p", "layerfs-storage", "--example", "measure_filesystem", "--",
            "--mode", "c2", "--case", "inode-update"], HERE / "c2-output"),
    ("integrated", ["cargo", "+1.85.1", "run", "--locked", "--manifest-path",
                    "core/Cargo.toml", "-p", "layerfs-storage", "--example",
                    "measure_filesystem", "--", "--mode", "pipeline", "--case",
                    "subtree-remove"], HERE / "pipeline-output"),
)
receipts = []
for name, command, output in ROWS:
    assert not output.exists(), f"refusing to reuse {output}"
    output.mkdir(parents=True)
    full = command + ["--output", str(output)]
    started = time.monotonic()
    result = subprocess.run(full, cwd=ROOT, capture_output=True, text=True, timeout=120)
    wall = time.monotonic() - started
    (HERE / f"{name}.stdout").write_text(result.stdout)
    (HERE / f"{name}.stderr").write_text(result.stderr)
    assert result.returncode == 0, result.stderr
    receipts.append(
        {
            "row": name,
            "argv": full,
            "exit": result.returncode,
            "wall_s": round(wall, 3),
            "output_directory": str(output),
        }
    )
    print(f"row {name} wall_s {wall:.3f}")
    print(result.stdout.strip())
(HERE / "receipt.json").write_text(
    json.dumps(
        {
            "rows": receipts,
            "cache_state": "warm in-process fixtures; no cold-cache claim",
            "samples": "one per row",
            "worker_policy": "single producer; the examples add no workers",
        },
        indent=2,
    )
    + "\n"
)
