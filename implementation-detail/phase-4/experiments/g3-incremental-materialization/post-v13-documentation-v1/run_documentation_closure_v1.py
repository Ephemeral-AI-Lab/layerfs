#!/usr/bin/env python3
"""Run the three one-shot documentation closure checks and write the closure."""

import hashlib
import json
import os
import stat
import subprocess
import time
from pathlib import Path


HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
BASE = HERE.parent
SEALED_PARENT = REPO / "target/phase4-g3-incremental-materialization-20260822-v13"
SEALED = SEALED_PARENT / "results-v13"
CLOSURE = BASE / "G3-POST-V13-DOCUMENTATION-CLOSURE-v1.json"
VERIFICATION = BASE / "G3-POST-V13-DOCUMENTATION-VERIFICATION-v1.txt"
CUSTODY = HERE / "documentation_custody_status_v1.py"
INDEPENDENT = HERE / "verify_closure_v1.py"
DOCS = [
    "implementation-detail/phase-4/experiments/g3-incremental-materialization/G3-REPORT.md",
    "implementation-detail/phase-4/baseline/g3-incremental-materialization-baseline-v1.md",
    "implementation-detail/phase-4/baseline/index.md",
    "implementation-detail/phase-4/2026-08-21-phase-4-full-grind.md",
    "implementation-detail/phase-4/baseline/current-benchmark-scoreboard.md",
    "implementation-detail/phase-4/README.md",
    "research/phase-4/decision-map.md",
    "implementation-detail/phase-4/experiments/g3-incremental-materialization/execution-handoff.md",
]


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1048576), b""):
            digest.update(block)
    return digest.hexdigest()


def canonical_hash(value):
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def mode(path):
    return f"{stat.S_IMODE(path.stat().st_mode):04o}"


def fsync_file(path):
    with path.open("rb") as handle:
        os.fsync(handle.fileno())


def fsync_directory(path):
    descriptor = os.open(path, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def status_record():
    value = subprocess.check_output(
        ["git", "status", "--short"], cwd=REPO, text=True
    )
    return {
        "lines": value.splitlines(),
        "line_count": len(value.splitlines()),
        "sha256": hashlib.sha256(value.encode()).hexdigest(),
    }


def file_rows(paths, relative_to=REPO):
    return [
        {
            "path": str(path.relative_to(relative_to)),
            "sha256": sha256(path),
            "size_bytes": path.stat().st_size,
            "mode": mode(path),
        }
        for path in paths
    ]


def sealed_fingerprint():
    files = sorted(path for path in SEALED.rglob("*") if path.is_file())
    rows = file_rows(files, SEALED_PARENT)
    return canonical_hash(rows)


def run_command(sequence, label, argv):
    stem = f"{sequence:02d}-{label}"
    stdout_path = HERE / f"{stem}.stdout"
    stderr_path = HERE / f"{stem}.stderr"
    if stdout_path.exists() or stderr_path.exists():
        raise RuntimeError(f"refusing to rerun existing command stream: {stem}")
    start = time.monotonic_ns()
    with stdout_path.open("xb") as stdout, stderr_path.open("xb") as stderr:
        process = subprocess.Popen(argv, cwd=REPO, stdout=stdout, stderr=stderr)
        exit_code = process.wait()
        stdout.flush()
        stderr.flush()
        os.fsync(stdout.fileno())
        os.fsync(stderr.fileno())
    wall_ns = time.monotonic_ns() - start
    fsync_directory(HERE)
    return {
        "sequence": sequence,
        "label": label,
        "argv": argv,
        "exit_code": exit_code,
        "wall_ns": wall_ns,
        "stdout_path": str(stdout_path.relative_to(REPO)),
        "stdout_sha256": sha256(stdout_path),
        "stdout_size_bytes": stdout_path.stat().st_size,
        "stdout_mode": mode(stdout_path),
        "stderr_path": str(stderr_path.relative_to(REPO)),
        "stderr_sha256": sha256(stderr_path),
        "stderr_size_bytes": stderr_path.stat().st_size,
        "stderr_mode": mode(stderr_path),
    }


def main():
    if Path.cwd().resolve() != REPO:
        raise RuntimeError("run from repository root")
    if CLOSURE.exists() or VERIFICATION.exists():
        raise RuntimeError("post-v13 documentation closure namespace already used")
    required_scripts = [Path(__file__).resolve(), CUSTODY, INDEPENDENT]
    if not all(path.is_file() for path in required_scripts):
        raise RuntimeError("closure scripts incomplete")
    if any(HERE.glob("0[1-4]-*.stdout")) or any(HERE.glob("0[1-4]-*.stderr")):
        raise RuntimeError("command stream namespace already used")

    pre_status = status_record()
    pre_docs = file_rows([REPO / name for name in DOCS])
    pre_sealed_fingerprint = sealed_fingerprint()
    commands = []
    plan = [
        (1, "rustfmt-check", ["cargo", "fmt", "--all", "--", "--check"]),
        (2, "documentation-diff-check", ["git", "diff", "--check", "--", *DOCS]),
        (3, "documentation-custody-status", ["python3", str(CUSTODY)]),
    ]
    for sequence, label, argv in plan:
        result = run_command(sequence, label, argv)
        commands.append(result)
        if result["exit_code"] != 0:
            break

    post_status = status_record()
    post_docs = file_rows([REPO / name for name in DOCS])
    post_sealed_fingerprint = sealed_fingerprint()
    custody_output = None
    custody_stdout = HERE / "03-documentation-custody-status.stdout"
    if len(commands) == 3 and commands[-1]["exit_code"] == 0:
        custody_output = json.loads(custody_stdout.read_text())

    stream_paths = []
    for command in commands:
        stream_paths.extend([REPO / command["stdout_path"], REPO / command["stderr_path"]])
    stream_rows = file_rows(stream_paths)
    scripts = file_rows(required_scripts)
    passed = (
        len(commands) == 3
        and all(command["exit_code"] == 0 for command in commands)
        and custody_output is not None
        and custody_output.get("status") == "PASS"
        and pre_docs == post_docs == custody_output.get("docs")
        and pre_sealed_fingerprint
        == post_sealed_fingerprint
        == custody_output.get("sealed_root", {}).get("fingerprint_sha256")
        and pre_status == post_status
    )

    closure = {
        "schema": "phase4-g3-post-v13-documentation-closure-v1",
        "status": "PASS" if passed else "REVISE",
        "date": "2026-08-22",
        "repository": str(REPO),
        "branch": subprocess.check_output(["git", "branch", "--show-current"], cwd=REPO, text=True).strip(),
        "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip(),
        "artifact_outside_sealed_payload": True,
        "sealed_payload_unchanged": pre_sealed_fingerprint == post_sealed_fingerprint,
        "sealed_result_root": str(SEALED.relative_to(REPO)),
        "sealed_root_fingerprint_sha256": post_sealed_fingerprint,
        "docs": post_docs,
        "docs_set_sha256": canonical_hash(post_docs),
        "links_checked": custody_output.get("links_checked") if custody_output else None,
        "broken_links": custody_output.get("broken_links") if custody_output else None,
        "manifest_entries": custody_output.get("manifest_entries") if custody_output else None,
        "sealed_root": custody_output.get("sealed_root") if custody_output else None,
        "sealed_hashes": custody_output.get("sealed_hashes") if custody_output else None,
        "identities": custody_output.get("identities") if custody_output else None,
        "rows_checked": custody_output.get("rows_checked") if custody_output else None,
        "report_tables_checked": custody_output.get("report_tables_checked") if custody_output else None,
        "focused_tests": custody_output.get("focused_tests") if custody_output else None,
        "workspace_tests": custody_output.get("workspace_tests") if custody_output else None,
        "history": custody_output.get("history") if custody_output else None,
        "stage": custody_output.get("stage") if custody_output else None,
        "limitations": custody_output.get("limitations") if custody_output else None,
        "commands": commands,
        "commands_planned": 3,
        "commands_executed": len(commands),
        "commands_rerun": 0,
        "command_streams": stream_rows,
        "command_streams_set_sha256": canonical_hash(stream_rows),
        "input_scripts": scripts,
        "input_scripts_set_sha256": canonical_hash(scripts),
        "pre_git_status": pre_status,
        "post_git_status": post_status,
        "pre_post_git_status_equal": pre_status == post_status,
        "pre_post_docs_equal": pre_docs == post_docs,
        "verification_command": {
            "argv": ["python3", str(INDEPENDENT)],
            "stdout_path": str((HERE / "04-independent-verification.stdout").relative_to(REPO)),
            "stderr_path": str((HERE / "04-independent-verification.stderr").relative_to(REPO)),
        },
        "verification_path": str(VERIFICATION.relative_to(REPO)),
        "build_rerun": False,
        "tests_rerun": False,
        "campaign_rerun": False,
        "static_closure_rerun": False,
        "finalizer_rerun": False,
    }
    data = (json.dumps(closure, indent=2, sort_keys=True) + "\n").encode()
    descriptor = os.open(CLOSURE, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
    try:
        os.write(descriptor, data)
        os.fsync(descriptor)
    finally:
        os.close(descriptor)
    fsync_directory(BASE)
    if not passed:
        raise RuntimeError("documentation closure recorded REVISE")
    print(json.dumps({"status": "PASS", "closure_sha256": sha256(CLOSURE)}, sort_keys=True))


if __name__ == "__main__":
    main()
