"""Append-only R7 command custody around the existing checkout lock.

This supervises one command; it is neither a test suite nor an approval gate.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--limit", type=int, required=True)
    parser.add_argument("--kind", choices=("build", "test", "inventory", "setup", "check"), required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command or args.limit <= 0 or (args.kind == "test" and args.limit > 120):
        parser.error("command and positive wall limit required; tests <=120 seconds")
    root = Path(__file__).resolve().parents[3]
    args.output.mkdir(parents=True, exist_ok=False)
    git = lambda *a: subprocess.check_output(["git", *a], cwd=root, text=True).strip()
    record = {"schema": "layerfs-r7-command-v1", "kind": args.kind,
              "source_commit": git("rev-parse", "HEAD"),
              "source_tree": git("rev-parse", "HEAD^{tree}"),
              "worktree_status": git("status", "--short"), "command": command,
              "limit_seconds": args.limit, "admission_eligible": False,
              "construction_workers": 1, "global_profile": "Disposable/WAL/OFF",
              "durable": "NOT_RUN — disabled by owner until explicit reauthorization"}
    (args.output / "invocation.json").write_text(json.dumps(record, indent=2) + "\n")
    env = dict(os.environ, LAYERFS_CONSTRUCTION_WORKERS="1")
    start = time.monotonic_ns()
    with (args.output / "stdout.txt").open("xb") as out, (args.output / "stderr.txt").open("xb") as err:
        result = subprocess.run(["perl", str(root / "core/target/r4-locked.pl"),
                                 str(args.limit), *command], cwd=root, env=env, stdout=out, stderr=err)
    record.update(exit_code=result.returncode, complete_command_ns=time.monotonic_ns() - start,
                  status="LOCK_BUSY_NOT_ATTEMPTED" if result.returncode == 75 else
                  "FAILED_WALL_LIMIT" if result.returncode == 124 else
                  "PASS" if result.returncode == 0 else "FAILED")
    (args.output / "result.json").write_text(json.dumps(record, indent=2) + "\n")
    manifest = {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                for p in sorted(args.output.iterdir()) if p.is_file()}
    (args.output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(record, indent=2))
    print((args.output / "stdout.txt").read_text(errors="replace"))
    print((args.output / "stderr.txt").read_text(errors="replace"))
    raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()
