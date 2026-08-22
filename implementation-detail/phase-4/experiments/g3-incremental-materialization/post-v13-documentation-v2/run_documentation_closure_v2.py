#!/usr/bin/env python3
"""Reuse v1 PASS format/diff streams, run fresh custody, and write v2 closure."""

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
V1_DIR = BASE / "post-v13-documentation-v1"
V1_CLOSURE = BASE / "G3-POST-V13-DOCUMENTATION-CLOSURE-v1.json"
V1_VERIFICATION = BASE / "G3-POST-V13-DOCUMENTATION-VERIFICATION-v1.txt"
CLOSURE = BASE / "G3-POST-V13-DOCUMENTATION-CLOSURE-v2.json"
VERIFICATION = BASE / "G3-POST-V13-DOCUMENTATION-VERIFICATION-v2.txt"
CUSTODY = HERE / "documentation_custody_status_v2.py"
INDEPENDENT = HERE / "verify_closure_v2.py"
SEALED_PARENT = REPO / "target/phase4-g3-incremental-materialization-20260822-v13"
SEALED = SEALED_PARENT / "results-v13"
V1_CLOSURE_SHA256 = "5a20c669cba588de9dc08343fdc3432441cb6c8765c81e70a9d9179399165dd8"
V1_VERIFICATION_SHA256 = "6e8e97934a099d96ded2cffc5bf42eaa619a00feba89b6fc4e1b096e74f3a77a"
V1_DEFECT = "documentation-verifier-newline-normalization-defect"
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


def fsync_directory(path):
    descriptor = os.open(path, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def file_row(path, relative_to=REPO):
    return {
        "path": str(path.relative_to(relative_to)),
        "sha256": sha256(path),
        "size_bytes": path.stat().st_size,
        "mode": mode(path),
    }


def file_rows(paths, relative_to=REPO):
    return [file_row(path, relative_to) for path in paths]


def status_record():
    value = subprocess.check_output(["git", "status", "--short"], cwd=REPO, text=True)
    return {
        "lines": value.splitlines(),
        "line_count": len(value.splitlines()),
        "sha256": hashlib.sha256(value.encode()).hexdigest(),
    }


def sealed_fingerprint():
    files = sorted(path for path in SEALED.rglob("*") if path.is_file())
    return canonical_hash(file_rows(files, SEALED_PARENT))


def verify_reused_v1():
    if sha256(V1_CLOSURE) != V1_CLOSURE_SHA256:
        raise RuntimeError("v1 closure changed")
    if sha256(V1_VERIFICATION) != V1_VERIFICATION_SHA256:
        raise RuntimeError("v1 verification changed")
    closure = json.loads(V1_CLOSURE.read_text())
    verification = json.loads(V1_VERIFICATION.read_text())
    if closure["status"] != verification["status"] or closure["status"] != "REVISE":
        raise RuntimeError("v1 disposition mismatch")
    if verification["failure_class"] != V1_DEFECT or verification["no_rerun"] is not True:
        raise RuntimeError("v1 defect mismatch")
    expected = [
        ["cargo", "fmt", "--all", "--", "--check"],
        ["git", "diff", "--check", "--", *DOCS],
    ]
    reused = []
    streams = []
    for command, argv in zip(closure["commands"][:2], expected):
        if command["argv"] != argv or command["exit_code"] != 0:
            raise RuntimeError("v1 reusable command mismatch")
        copied = dict(command)
        copied["evidence_origin"] = str(V1_CLOSURE.relative_to(REPO))
        copied["executed_in_v2"] = False
        reused.append(copied)
        for kind in ["stdout", "stderr"]:
            path = REPO / command[f"{kind}_path"]
            if sha256(path) != command[f"{kind}_sha256"] or path.stat().st_size != command[f"{kind}_size_bytes"]:
                raise RuntimeError("v1 reusable stream mismatch")
            streams.append(file_row(path))
    return closure, reused, streams


def run_custody():
    stdout_path = HERE / "03-documentation-custody-status.stdout"
    stderr_path = HERE / "03-documentation-custody-status.stderr"
    if stdout_path.exists() or stderr_path.exists():
        raise RuntimeError("refusing to rerun v2 custody")
    argv = ["python3", str(CUSTODY)]
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
        "sequence": 3,
        "label": "documentation-custody-status",
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
        "executed_in_v2": True,
    }


def main():
    if Path.cwd().resolve() != REPO:
        raise RuntimeError("run from repository root")
    if CLOSURE.exists() or VERIFICATION.exists():
        raise RuntimeError("v2 closure namespace already used")
    if any(HERE.glob("0[1-4]-*.stdout")) or any(HERE.glob("0[1-4]-*.stderr")):
        raise RuntimeError("v2 stream namespace already used")
    scripts = [Path(__file__).resolve(), CUSTODY, INDEPENDENT]
    if not all(path.is_file() for path in scripts):
        raise RuntimeError("v2 scripts incomplete")

    v1, reused, reused_streams = verify_reused_v1()
    pre_status = status_record()
    pre_docs = file_rows([REPO / name for name in DOCS])
    pre_sealed = sealed_fingerprint()
    if pre_status != v1["pre_git_status"] or pre_status != v1["post_git_status"]:
        raise RuntimeError("status changed since reusable v1 checks")
    if pre_docs != v1["docs"]:
        raise RuntimeError("docs changed since reusable v1 checks")
    if pre_sealed != v1["sealed_root_fingerprint_sha256"]:
        raise RuntimeError("sealed payload changed since v1")

    command = run_custody()
    post_status = status_record()
    post_docs = file_rows([REPO / name for name in DOCS])
    post_sealed = sealed_fingerprint()
    custody = None
    if command["exit_code"] == 0:
        custody = json.loads((REPO / command["stdout_path"]).read_text())
    fresh_streams = file_rows([REPO / command["stdout_path"], REPO / command["stderr_path"]])
    script_rows = file_rows(scripts)
    passed = (
        command["exit_code"] == 0
        and custody is not None
        and custody.get("status") == "PASS"
        and pre_status == post_status
        and pre_docs == post_docs == custody.get("docs")
        and pre_sealed == post_sealed == custody.get("sealed_root", {}).get("fingerprint_sha256")
    )
    closure = {
        "schema": "phase4-g3-post-v13-documentation-closure-v2",
        "status": "PASS" if passed else "REVISE",
        "date": "2026-08-22",
        "repository": str(REPO),
        "branch": subprocess.check_output(["git", "branch", "--show-current"], cwd=REPO, text=True).strip(),
        "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip(),
        "artifact_outside_sealed_payload": True,
        "sealed_payload_unchanged": pre_sealed == post_sealed,
        "sealed_result_root": str(SEALED.relative_to(REPO)),
        "sealed_root_fingerprint_sha256": post_sealed,
        "docs": post_docs,
        "docs_set_sha256": canonical_hash(post_docs),
        "links_checked": custody.get("links_checked") if custody else None,
        "broken_links": custody.get("broken_links") if custody else None,
        "manifest_entries": custody.get("manifest_entries") if custody else None,
        "sealed_root": custody.get("sealed_root") if custody else None,
        "sealed_hashes": custody.get("sealed_hashes") if custody else None,
        "identities": custody.get("identities") if custody else None,
        "rows_checked": custody.get("rows_checked") if custody else None,
        "report_tables_checked": custody.get("report_tables_checked") if custody else None,
        "focused_tests": custody.get("focused_tests") if custody else None,
        "workspace_tests": custody.get("workspace_tests") if custody else None,
        "history": custody.get("history") if custody else None,
        "stage": custody.get("stage") if custody else None,
        "limitations": custody.get("limitations") if custody else None,
        "v1_revise": {
            "closure_sha256": V1_CLOSURE_SHA256,
            "verification_sha256": V1_VERIFICATION_SHA256,
            "status": "REVISE",
            "failure_class": V1_DEFECT,
            "commands_rerun": 0,
        },
        "only_v2_change": "normalize Markdown whitespace before no-persistent-replayable-destination-receipt predicate",
        "reused_evidence_justification": "v1 commands 1 and 2 passed with empty streams; v2 rehashed their exact argv/streams only after exact docs, git status, and sealed fingerprint matched v1 pre/post custody",
        "reused_commands": reused,
        "reused_command_streams": reused_streams,
        "reused_command_streams_set_sha256": canonical_hash(reused_streams),
        "fresh_commands": [command],
        "fresh_command_streams": fresh_streams,
        "fresh_command_streams_set_sha256": canonical_hash(fresh_streams),
        "commands_planned": 3,
        "commands_reused": 2,
        "commands_fresh_executed": 1,
        "commands_rerun": 0,
        "input_scripts": script_rows,
        "input_scripts_set_sha256": canonical_hash(script_rows),
        "inherited_v1_custody_verifier_sha256": sha256(V1_DIR / "documentation_custody_status_v1.py"),
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
        raise RuntimeError("v2 documentation closure recorded REVISE")
    print(json.dumps({"status": "PASS", "closure_sha256": sha256(CLOSURE)}, sort_keys=True))


if __name__ == "__main__":
    main()
