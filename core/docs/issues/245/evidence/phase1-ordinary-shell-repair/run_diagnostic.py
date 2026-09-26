#!/usr/bin/env python3
"""Labelled #245 ordinary-shell diagnostic runner.

This is a *diagnostic* tool, not a benchmark selection. It reuses one sealed
#243 prepared state (image, release driver, release verifier, masters) and runs
one exact ordinary shell command through the public `WorkspaceApi::exec` route
on an independent writable byte copy of a closed master. It records the driver
receipt, the `LAYERFS_FUSE_CALLBACK` trace and the `LAYERFS_FUSE_ERROR`
diagnostic, then runs the sealed independent verifier over the published Store
and History.

It claims no latency: the host Store copy and the Linux FUSE backing cache are
uncontrolled, so every attempt is `INELIGIBLE` for numeric admission. It exists
to identify the Workspace error behind a mapped `EIO`, to count the kernel's
own callbacks, and to check the published tree.

Example (from the repository root):

    python3 core/docs/issues/245/evidence/phase1-ordinary-shell-repair/run_diagnostic.py \
        --name run-43-root-lockfile-replace --shape package \
        --old-manifest package.tsv --new-manifest diag-root-lockfile.tsv \
        --image sha256:ea06c66e5ec21def0728239a971dff28a280ab9ae4d81ac127adbde2aec1412f \
        --command /tmp/cmd.txt --output /tmp/i245/run-43-root-lockfile-replace
"""
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

# The retained #243 prepared state: image, binaries and closed masters.
DEFAULT_PREPARED = Path(
    "benchmark-results/fs-bench-pro/issue243-shell-package-v1-prepared-02/prepared.json"
)


def manifest_path(prepared_root, prepared, value):
    """A sealed fixture manifest by name, or any explicit path when one exists."""
    candidate = Path(value)
    if candidate.is_absolute() or candidate.exists():
        return candidate
    return prepared_root / prepared.parent / "fixture" / value


def digest(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--prepared-root", type=Path, required=True,
                        help="worktree that holds the retained prepared state")
    parser.add_argument("--prepared", type=Path, default=DEFAULT_PREPARED)
    parser.add_argument("--worktree", type=Path, required=True,
                        help="candidate worktree the SDK driver runs from")
    parser.add_argument("--name", required=True, help="attempt id, also the receipt scenario_id")
    parser.add_argument("--shape", required=True, choices=["package", "large", "repeated"])
    parser.add_argument("--old-manifest", required=True,
                        help="manifest file name inside the sealed fixture tree, or a path")
    parser.add_argument("--new-manifest", required=True,
                        help="manifest file name inside the sealed fixture tree, or a path")
    parser.add_argument("--image", required=True)
    parser.add_argument("--command", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--expected-failure", action="store_true")
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()

    prepared = json.loads((args.prepared_root / args.prepared).read_text())
    master = prepared["masters"][args.shape]
    args.output.mkdir(parents=True, exist_ok=False)
    for name in ("store", "history"):
        source = args.prepared_root / master["path"] / f"{name}.sqlite"
        if digest(source) != master[f"{name}_sha256"]:
            raise SystemExit(f"master {name} seal mismatch")
        target = args.output / f"{name}.sqlite"
        shutil.copyfile(source, target)
        target.chmod(0o644)
        if digest(target) != master[f"{name}_sha256"]:
            raise SystemExit(f"clone {name} seal mismatch")
    command = args.command.read_text()
    fields = {key: master[key] for key in
              ("project_id", "genesis_layer", "genesis_root", "genesis_root_serial",
               "branch_id", "old_commit")}
    fields.update(scenario_id=args.name, command_hex=command.encode().hex(),
                  expected_failure="1" if args.expected_failure else "0",
                  telemetry_run=int.from_bytes(os.urandom(16), "big") or 1)
    if args.expected_failure:
        fields["expected_head_commit"] = master["old_commit"]
    (args.output / "case.before").write_text("".join(f"{k}={v}\n" for k, v in sorted(fields.items())))
    env = {**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": prepared["cursor_key"],
           "LAYERFS_CONSTRUCTION_WORKERS": "1"}
    driver = args.prepared_root / prepared["binaries"]["benchmark_shell"]["path"]
    if digest(driver) != prepared["binaries"]["benchmark_shell"]["sha256"]:
        raise SystemExit("driver seal mismatch")
    start = time.monotonic_ns()
    try:
        process = subprocess.run(
            [str(driver), "run", str(args.output / "case.before"),
             str(args.output / "store.sqlite"), str(args.output / "history.sqlite"), args.image],
            cwd=args.worktree, capture_output=True, timeout=args.timeout, env=env)
        stdout, stderr, code, timed_out = process.stdout, process.stderr, process.returncode, False
    except subprocess.TimeoutExpired as error:
        stdout, stderr, code, timed_out = error.stdout or b"", error.stderr or b"", None, True
    wall = time.monotonic_ns() - start
    (args.output / "driver.stdout").write_bytes(stdout)
    (args.output / "driver.stderr").write_bytes(stderr)
    receipt = None
    lines = [line[8:] for line in stdout.splitlines() if line.startswith(b"RECEIPT\t")]
    if lines:
        receipt = json.loads(lines[0])
        if receipt.get("head_commit"):
            fields["expected_head_commit"] = receipt["head_commit"]
    (args.output / "case.verify").write_text(
        "".join(f"{k}={v}\n" for k, v in sorted(fields.items())))
    verifier = args.prepared_root / prepared["binaries"]["verify_shell"]["path"]
    if digest(verifier) != prepared["binaries"]["verify_shell"]["sha256"]:
        raise SystemExit("verifier seal mismatch")
    verify = {"status": "NOT_RUN"}
    try:
        check = subprocess.run(
            [str(verifier), str(args.output / "case.verify"), str(args.output / "store.sqlite"),
             str(args.output / "history.sqlite"),
             str(manifest_path(args.prepared_root, args.prepared, args.old_manifest)),
             str(manifest_path(args.prepared_root, args.prepared, args.new_manifest))],
            cwd=args.worktree, capture_output=True, timeout=9, env=env)
        verify = {"status": "PASS" if check.returncode == 0 else "FAIL",
                  "detail": check.stdout.decode()[-2000:],
                  "error": check.stderr.decode()[-2000:]}
    except subprocess.TimeoutExpired:
        verify = {"status": "FAIL", "detail": "verifier timeout"}
    (args.output / "verifier.stdout").write_text(verify.get("detail", ""))
    (args.output / "verifier.stderr").write_text(verify.get("error", ""))
    counts, written, errors = {}, 0, []
    for line in stderr.decode(errors="replace").splitlines():
        if line.startswith("LFS_FUSE_CALLBACK"):
            found = dict(part.split("=", 1) for part in line.split()[2:] if "=" in part)
            counts[found.get("op", "?")] = counts.get(found.get("op", "?"), 0) + 1
            if found.get("op") == "write":
                written += int(found.get("len", 0))
        elif line.startswith("LFS_FUSE_ERROR"):
            errors.append(line.split("error=", 1)[1])
    if written:
        counts["write_bytes"] = written
    print(json.dumps({
        "attempt": args.name, "wall_ns": wall, "driver_exit": code, "driver_timeout": timed_out,
        "driver": {key: (receipt or {}).get(key) for key in
                   ("status", "detail", "commit_called", "exec_ns", "commit_ns", "cleanup_ns",
                    "projection_counts", "unmount_ok", "sandbox_delete_ok")},
        "fuse_callbacks": counts, "workspace_errors": errors, "verifier": verify["status"],
        "verifier_detail": verify.get("detail", "").strip()[-300:],
        "cache_status": "INELIGIBLE"}, indent=1))
    return 0 if verify["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
